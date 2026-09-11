//! engine.rs - the audio thread and the master chain.
//!
//! ---------------------------------------------------------------------------
//! WHAT CHANGED FROM THE WEB BUILD, AND WHY IT IS SIMPLER
//! ---------------------------------------------------------------------------
//!
//! The browser gives you a graph of nodes and a clock, and you *schedule*
//! against that clock: "start this oscillator at t=12.80, stop it at t=13.55".
//! Because JavaScript timers drift, the web version needed a look-ahead
//! scheduler - a sloppy timer that wakes often and queues notes into the near
//! future. Two clocks, carefully kept apart. That is the single subtlest thing
//! in the whole web codebase.
//!
//! Here the sound card asks us, every few milliseconds, to fill a buffer with
//! the next N samples. There is exactly one clock - a running count of samples
//! written - and it *is* the audio. Nothing can drift from it, because it is not
//! measuring time, it is time. The look-ahead scheduler does not get ported; it
//! stops existing.
//!
//! ---------------------------------------------------------------------------
//! THE ONE RULE OF THE AUDIO THREAD
//! ---------------------------------------------------------------------------
//!
//! `fill` runs on a real-time thread owned by the operating system. If it takes
//! too long you do not get a slow instrument, you get a click - a hole in the
//! sound. So it must never allocate, never lock a mutex, never block, and never
//! touch the UI. Everything it needs is either already inside it or arrives
//! through a queue it can drain without waiting. That is why notes arrive as
//! messages instead of the UI reaching in and pushing a voice.
//!
//! What a single note sounds like is voice.rs; the room it sits in is reverb.rs.
//! This file is the plumbing and the mix.

use std::sync::mpsc::{Receiver, Sender, channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::reverb::Reverb;
use crate::voice::{PATCHES, Patch, Voice};

/// Level of one note before the master stage. Seven chords of three notes is 21
/// voices, and this leaves room for all of them.
const VOICE_GAIN: f32 = 0.20;

/// Hard ceiling on simultaneous voices. Preallocated and never grown, because
/// growing a Vec on the audio thread would allocate.
const MAX_VOICES: usize = 64;

/// What the UI thread can ask the audio thread to do.
pub enum Msg {
    /// Sound these pitches together, owned by `id` until released. The engine
    /// spreads them across the stereo field in the order given.
    NoteOn { id: u64, notes: Vec<i32> },
    NoteOff { id: u64 },
    AllOff,
    SetPatch(usize),
}

/// Everything the audio thread owns. Nothing else may touch it.
struct AudioState {
    voices: Vec<Voice>,
    reverb: Reverb,
    patch: Patch,
    rx: Receiver<Msg>,
    sample_rate: f32,
}

impl AudioState {
    fn new(rx: Receiver<Msg>, sample_rate: f32, patch: Patch) -> Self {
        AudioState {
            voices: Vec::with_capacity(MAX_VOICES),
            reverb: Reverb::new(sample_rate),
            patch,
            rx,
            sample_rate,
        }
    }

    /// Drain the queue. `try_recv` never blocks, which is the whole point.
    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::NoteOn { id, notes } => {
                    let last = notes.len().saturating_sub(1).max(1) as f32;
                    for (i, midi) in notes.iter().enumerate() {
                        if self.voices.len() >= MAX_VOICES {
                            break;
                        }
                        // Spread the chord left to right in the order the notes
                        // were given - lowest note left, highest right. A triad
                        // stacked dead centre is the mono sound the web build
                        // had; spreading it is most of why this one feels wide.
                        let pan = (i as f32 / last) * 2.0 - 1.0;
                        let freq = crate::theory::midi_to_freq(*midi);
                        self.voices
                            .push(Voice::new(id, freq, pan, &self.patch, self.sample_rate));
                    }
                }
                Msg::NoteOff { id } => {
                    for v in self.voices.iter_mut().filter(|v| v.id == id) {
                        v.release();
                    }
                }
                Msg::AllOff => {
                    // Release rather than cut. Key, octave and patch changes all
                    // come through here, and letting the old chord fade the way
                    // it normally would is far nicer than a hard stop - the
                    // reverb tail going with it is part of that.
                    for v in self.voices.iter_mut() {
                        v.release();
                    }
                }
                Msg::SetPatch(i) => {
                    self.patch = PATCHES[i.min(PATCHES.len() - 1)];
                    // Voices already sounding keep the patch they were born
                    // with; converting one mid-note would click.
                }
            }
        }
    }

    /// Fill one buffer. Called by the OS; see the rule at the top of this file.
    fn fill(&mut self, out: &mut [f32], channels: usize) {
        self.handle_messages();

        // `retain` reuses the existing allocation, so reaping voices is free.
        self.voices.retain(|v| !v.finished());

        let wet_amount = self.patch.reverb;

        for frame in out.chunks_mut(channels) {
            let (mut l, mut r) = (0.0, 0.0);
            for v in self.voices.iter_mut() {
                let (vl, vr) = v.next();
                l += vl;
                r += vr;
            }
            l *= VOICE_GAIN;
            r *= VOICE_GAIN;

            if wet_amount > 0.0 {
                let (wl, wr) = self.reverb.process(l, r);
                l += wl * wet_amount;
                r += wr * wet_amount;
            }

            // Soft clip. tanh squashes peaks smoothly instead of letting them
            // hit the rails and crackle - the job a limiter does, in one line.
            let l = (l * 1.2).tanh() * 0.75;
            let r = (r * 1.2).tanh() * 0.75;

            match channels {
                1 => frame[0] = (l + r) * 0.5,
                _ => {
                    frame[0] = l;
                    frame[1] = r;
                    // Anything past the first two channels gets silence rather
                    // than a surprise.
                    for slot in frame.iter_mut().skip(2) {
                        *slot = 0.0;
                    }
                }
            }
        }
    }
}

/// A running audio stream. Dropping this stops the sound, so `main` has to hold
/// on to it for as long as the window is open.
pub struct Engine {
    _stream: cpal::Stream,
    tx: Sender<Msg>,
    pub sample_rate: f32,
    pub buffer_frames: Option<u32>,
    pub device_name: String,
}

impl Engine {
    pub fn start(patch: Patch) -> Result<Engine, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no output device - is anything plugged in?")?;
        let device_name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "unknown device".into());

        let supported = device
            .default_output_config()
            .map_err(|e| format!("no default output config: {e}"))?;
        // cpal 0.18 makes SampleRate a plain u32 rather than a newtype.
        let sample_rate = supported.sample_rate() as f32;
        let channels = supported.channels() as usize;
        let sample_format = supported.sample_format();
        let mut config: cpal::StreamConfig = supported.into();

        // Ask for a small buffer. This is the knob the browser never exposed:
        // `latencyHint: 'interactive'` was a polite suggestion, this is a
        // number. 256 frames is under 6ms at 44.1kHz.
        config.buffer_size = cpal::BufferSize::Fixed(256);

        let (tx, rx) = channel::<Msg>();
        let mut state = AudioState::new(rx, sample_rate, patch);

        let err_fn = |e| eprintln!("audio stream error: {e}");

        // Only f32 output is handled. Every desktop device this is likely to
        // meet offers it; anything else is a clear error rather than silence.
        if sample_format != cpal::SampleFormat::F32 {
            return Err(format!("unsupported sample format {sample_format:?}"));
        }

        let stream = device
            .build_output_stream(
                config.clone(),
                move |out: &mut [f32], _| state.fill(out, channels),
                err_fn,
                None,
            )
            .map_err(|e| format!("could not open the audio stream: {e}"))?;

        stream.play().map_err(|e| format!("could not start audio: {e}"))?;

        let buffer_frames = match config.buffer_size {
            cpal::BufferSize::Fixed(n) => Some(n),
            cpal::BufferSize::Default => None,
        };

        Ok(Engine {
            _stream: stream,
            tx,
            sample_rate,
            buffer_frames,
            device_name,
        })
    }

    /// Fire and forget. A failed send means the audio thread has died, and
    /// there is nothing useful the UI can do about that mid-keypress.
    pub fn send(&self, msg: Msg) {
        let _ = self.tx.send(msg);
    }

    /// Latency the buffer accounts for, in milliseconds. Not the whole story -
    /// the operating system adds its own - but it is the part this program
    /// chose, and the part the browser would not let us choose at all.
    pub fn buffer_latency_ms(&self) -> Option<f32> {
        self.buffer_frames
            .map(|n| n as f32 / self.sample_rate * 1000.0)
    }
}

/// Render offline, with no sound card involved.
///
/// `events` are (seconds, message) pairs. Returns interleaved stereo f32.
///
/// This exists so the sound can be *listened to* rather than asserted about -
/// `heptad --render` writes a WAV of the same engine the speakers get, which is
/// a far better way to judge a synth patch than any test could be.
pub fn render(
    patch: Patch,
    sample_rate: f32,
    seconds: f32,
    mut events: Vec<(f32, Msg)>,
) -> Vec<f32> {
    let (tx, rx) = channel::<Msg>();
    let mut state = AudioState::new(rx, sample_rate, patch);

    // Earliest first, so the walk below can just peel off the front.
    events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    events.reverse(); // so pop() gives the earliest

    let total = (seconds * sample_rate) as usize;
    let mut out = vec![0.0f32; total * 2];

    // Same block size the live stream asks for, so the offline render goes
    // through exactly the same code path with the same timing granularity.
    const BLOCK: usize = 256;
    let mut i = 0;
    while i < total {
        let t = i as f32 / sample_rate;
        while events.last().map(|e| e.0 <= t).unwrap_or(false) {
            let (_, msg) = events.pop().unwrap();
            let _ = tx.send(msg);
        }
        let n = BLOCK.min(total - i);
        state.fill(&mut out[i * 2..(i + n) * 2], 2);
        i += n;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// An engine under test with no sound card attached. `fill` is pure
    /// arithmetic over a buffer, so the mix is testable even though the stream
    /// is not.
    fn harness(patch_index: usize) -> (Sender<Msg>, AudioState) {
        let (tx, rx) = channel::<Msg>();
        (tx, AudioState::new(rx, SR, PATCHES[patch_index]))
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn silence_until_something_is_pressed() {
        let (_tx, mut st) = harness(1);
        let mut buf = vec![0.0f32; 512];
        st.fill(&mut buf, 2);
        assert_eq!(peak(&buf), 0.0, "idle engine should be silent");
    }

    #[test]
    fn a_note_on_actually_makes_sound() {
        let (tx, mut st) = harness(1);
        tx.send(Msg::NoteOn { id: 1, notes: vec![60, 64, 67] }).unwrap();
        let mut buf = vec![0.0f32; 8192];
        st.fill(&mut buf, 2);
        assert!(peak(&buf) > 0.05, "expected audible output, got {}", peak(&buf));
        assert_eq!(st.voices.len(), 3, "a triad is three voices");
    }

    #[test]
    fn a_released_note_decays_to_silence_and_is_reaped() {
        let (tx, mut st) = harness(0); // raw: no reverb tail to wait out
        tx.send(Msg::NoteOn { id: 7, notes: vec![60] }).unwrap();
        let mut buf = vec![0.0f32; 4096];
        st.fill(&mut buf, 2);
        assert!(peak(&buf) > 0.05);

        tx.send(Msg::NoteOff { id: 7 }).unwrap();
        for _ in 0..24 {
            st.fill(&mut buf, 2);
        }
        assert!(peak(&buf) < 0.001, "note kept ringing: {}", peak(&buf));
        st.fill(&mut buf, 2);
        assert_eq!(st.voices.len(), 0, "finished voice was not reaped");
    }

    #[test]
    fn releasing_one_id_leaves_the_others_ringing() {
        // The bug the web build hit: two inputs holding different chords, and
        // letting go of one must not silence the other.
        let (tx, mut st) = harness(1);
        tx.send(Msg::NoteOn { id: 1, notes: vec![60, 64, 67] }).unwrap();
        tx.send(Msg::NoteOn { id: 2, notes: vec![67, 71, 74] }).unwrap();
        tx.send(Msg::NoteOff { id: 1 }).unwrap();
        let mut buf = vec![0.0f32; 4096];
        st.fill(&mut buf, 2);
        // All six are still alive: the released three are fading, not gone, and
        // crucially they were not reaped before making a sound.
        assert_eq!(st.voices.iter().filter(|v| !v.finished()).count(), 6);
        // The id-2 voices must still be alive well after id 1 has gone.
        for _ in 0..20 {
            st.fill(&mut buf, 2);
        }
        assert!(st.voices.len() >= 3, "id 2 was silenced along with id 1");
    }

    #[test]
    fn full_polyphony_never_clips() {
        // Seven chords of three notes, all down at once - the worst case the
        // instrument can produce. Anything past +/-1.0 is a digital crackle.
        let (tx, mut st) = harness(1);
        for id in 0..7u64 {
            let root = 48 + id as i32 * 2;
            tx.send(Msg::NoteOn { id, notes: vec![root, root + 4, root + 7] }).unwrap();
        }
        let mut buf = vec![0.0f32; 8192];
        for _ in 0..8 {
            st.fill(&mut buf, 2);
            assert!(peak(&buf) <= 1.0, "clipped at {}", peak(&buf));
        }
        assert_eq!(st.voices.len(), 21);
    }

    #[test]
    fn voice_count_is_capped() {
        let (tx, mut st) = harness(1);
        for id in 0..40u64 {
            tx.send(Msg::NoteOn { id, notes: vec![60, 64, 67] }).unwrap();
        }
        let mut buf = vec![0.0f32; 256];
        st.fill(&mut buf, 2);
        assert!(st.voices.len() <= MAX_VOICES, "voice cap breached: {}", st.voices.len());
    }

    #[test]
    fn a_chord_is_actually_stereo() {
        // The three notes are panned across the field, so the two channels must
        // differ. If they match, the spread silently stopped working.
        let (tx, mut st) = harness(1);
        tx.send(Msg::NoteOn { id: 1, notes: vec![60, 64, 67] }).unwrap();
        let mut buf = vec![0.0f32; 8192];
        st.fill(&mut buf, 2);
        let differs = buf.chunks(2).any(|f| (f[0] - f[1]).abs() > 1e-4);
        assert!(differs, "chord came out mono");
    }

    #[test]
    fn the_raw_patch_is_mono_by_design() {
        // Control for the test above - raw has stereo 0.0, so it should be
        // centred and identical in both channels.
        let (tx, mut st) = harness(0);
        tx.send(Msg::NoteOn { id: 1, notes: vec![60, 64, 67] }).unwrap();
        let mut buf = vec![0.0f32; 4096];
        st.fill(&mut buf, 2);
        assert!(buf.chunks(2).all(|f| (f[0] - f[1]).abs() < 1e-5), "raw should be centred");
    }

    #[test]
    fn reverb_keeps_sounding_after_the_notes_stop() {
        let (tx, mut st) = harness(1); // warm has a reverb send
        tx.send(Msg::NoteOn { id: 1, notes: vec![60, 64, 67] }).unwrap();
        let mut buf = vec![0.0f32; 8192];
        for _ in 0..4 {
            st.fill(&mut buf, 2);
        }
        tx.send(Msg::NoteOff { id: 1 }).unwrap();

        // Run past the amplitude release so every voice is gone, then listen.
        // 40 buffers of 4096 samples is ~1.7s at 48k; the release is 0.45s.
        for _ in 0..40 {
            st.fill(&mut buf, 2);
        }
        assert_eq!(st.voices.len(), 0, "voices should be finished by now");
        st.fill(&mut buf, 2);
        assert!(peak(&buf) > 0.0, "no reverb tail once the voices ended");
    }

    #[test]
    fn everything_eventually_goes_quiet_after_all_off() {
        // AllOff releases rather than cuts, so silence arrives after the release
        // AND the reverb tail behind it - a few seconds, not instantly. What
        // matters is that it does arrive and nothing sustains forever.
        let (tx, mut st) = harness(1);
        tx.send(Msg::NoteOn { id: 1, notes: vec![60, 64, 67] }).unwrap();
        let mut buf = vec![0.0f32; 4096];
        st.fill(&mut buf, 2);
        tx.send(Msg::AllOff).unwrap();

        // ~6 seconds at 48k.
        for _ in 0..140 {
            st.fill(&mut buf, 2);
        }
        assert_eq!(st.voices.len(), 0, "a voice outlived AllOff");
        assert!(peak(&buf) < 0.001, "something survived AllOff: {}", peak(&buf));
    }

    #[test]
    fn every_patch_plays_without_blowing_up() {
        for i in 0..PATCHES.len() {
            let (tx, mut st) = harness(i);
            tx.send(Msg::NoteOn { id: 1, notes: vec![48, 52, 55] }).unwrap();
            tx.send(Msg::NoteOn { id: 2, notes: vec![72, 76, 79] }).unwrap();
            let mut buf = vec![0.0f32; 8192];
            for _ in 0..8 {
                st.fill(&mut buf, 2);
                assert!(
                    buf.iter().all(|s| s.is_finite()),
                    "patch '{}' produced a non-finite sample",
                    PATCHES[i].name
                );
                assert!(peak(&buf) <= 1.0, "patch '{}' clipped", PATCHES[i].name);
            }
        }
    }

    #[test]
    fn switching_patch_does_not_disturb_notes_already_sounding() {
        let (tx, mut st) = harness(0);
        tx.send(Msg::NoteOn { id: 1, notes: vec![60] }).unwrap();
        let mut buf = vec![0.0f32; 2048];
        st.fill(&mut buf, 2);
        tx.send(Msg::SetPatch(2)).unwrap();
        st.fill(&mut buf, 2);
        assert_eq!(st.voices.len(), 1, "the sounding note was dropped");
        assert!(buf.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn mono_devices_get_both_channels_summed() {
        let (tx, mut st) = harness(1);
        tx.send(Msg::NoteOn { id: 1, notes: vec![60, 64, 67] }).unwrap();
        let mut buf = vec![0.0f32; 4096];
        st.fill(&mut buf, 1);
        assert!(peak(&buf) > 0.02, "mono output was silent");
        assert!(buf.iter().all(|s| s.is_finite()));
    }
}

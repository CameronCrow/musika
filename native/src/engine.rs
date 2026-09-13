//! engine.rs - the audio thread: voices, the looper, the arpeggiator, the mix.
//!
//! ---------------------------------------------------------------------------
//! WHAT CHANGED FROM THE WEB BUILD, AND WHY IT IS SIMPLER
//! ---------------------------------------------------------------------------
//!
//! The browser gives you a graph of nodes and a clock, and you *schedule*
//! against that clock: "start this oscillator at t=12.80, stop it at t=13.55".
//! Because JavaScript timers drift, the web version needed a look-ahead
//! scheduler - a sloppy timer that wakes often and queues notes into the near
//! future - and two clocks carefully kept apart.
//!
//! Here the sound card asks, every few milliseconds, for the next N samples.
//! There is exactly one clock - a count of samples written - and it *is* the
//! audio. The looper and the arpeggiator both run on it, advanced one sample at
//! a time, so a looped chord or an arpeggio step lands on its exact sample every
//! single time. The look-ahead scheduler was not ported; it stopped existing.
//!
//! ---------------------------------------------------------------------------
//! THE ONE RULE OF THE AUDIO THREAD
//! ---------------------------------------------------------------------------
//!
//! `fill` runs on a real-time thread owned by the operating system. If it takes
//! too long you do not get a slow instrument, you get a click - a hole in the
//! sound. So it never allocates, never locks, never blocks. Every list it keeps
//! is allocated up front and never grown; every message is drained without
//! waiting; chords arrive as fixed-size values rather than heap vectors.
//!
//! ---------------------------------------------------------------------------
//! TALKING TO THE UI
//! ---------------------------------------------------------------------------
//!
//! Commands go in through a channel. What comes back - the loop's state, its
//! position, which pads it is playing - is a handful of atomics the UI reads
//! each frame. Neither side ever waits for the other.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering::Relaxed};
use std::sync::mpsc::{Receiver, Sender, channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::arp::Arp;
use crate::looper::{LoopState, Looper};
use crate::reverb::Reverb;
use crate::theory::{ArpPattern, Chord};
use crate::voice::{PATCHES, Patch, Voice};

/// Level of one note before the master stage.
const VOICE_GAIN: f32 = 0.20;

/// Hard ceiling on simultaneous voices.
const MAX_VOICES: usize = 64;

/// Notes the engine has started and must stop at a known sample.
const MAX_PENDING_OFFS: usize = 256;

/// Pads currently lit by the loop.
const MAX_FLASHES: usize = 128;

/// Voice ids at or above this belong to notes the engine started itself - the
/// looper's and the arpeggiator's - rather than to a finger. Letting go of every
/// live input leaves these alone, which is what lets you change key while a loop
/// plays without chopping its current chord off. The UI keeps its ids below it.
pub const AUTO_ID_BASE: u64 = 1 << 48;

/// What the UI can ask the audio thread to do.
pub enum Msg {
    /// A finger went down on pad `degree`, sounding `chord`, until `NoteOff`.
    NoteOn { id: u64, chord: Chord, degree: u8 },
    NoteOff { id: u64 },
    /// Let go of every live input - key, octave and patch changes use this.
    ReleaseLive,
    SetPatch(usize),
    /// The looper's record button: arm, close the loop, overdub.
    Pedal,
    PlayStop,
    ClearLoop,
    SetArp(bool),
    SetArpPattern(ArpPattern),
    SetTempo(f32),
}

/// What the audio thread reports back, as atomics so reading them never blocks.
#[derive(Default)]
pub struct Status {
    loop_state: AtomicU8,
    /// Position round the loop, scaled to 0..=65535.
    loop_pos: AtomicU32,
    /// Bit `n` is set while the loop is playing pad `n`.
    looping_pads: AtomicU8,
}

impl Status {
    pub fn loop_state(&self) -> LoopState {
        LoopState::from_u8(self.loop_state.load(Relaxed))
    }

    pub fn loop_position(&self) -> f32 {
        self.loop_pos.load(Relaxed) as f32 / 65535.0
    }

    pub fn pad_looping(&self, degree: usize) -> bool {
        degree < 8 && self.looping_pads.load(Relaxed) & (1 << degree) != 0
    }
}

/// Spread a chord across the stereo field, lowest note left, highest right.
fn chord_pan(i: usize, n: usize) -> f32 {
    if n > 1 {
        i as f32 / (n - 1) as f32 * 2.0 - 1.0
    } else {
        0.0
    }
}

/// Everything the audio thread owns. Nothing else may touch it.
struct AudioState {
    voices: Vec<Voice>,
    reverb: Reverb,
    patch: Patch,
    looper: Looper,
    arp: Arp,
    rx: Receiver<Msg>,
    status: Arc<Status>,
    sample_rate: f32,
    /// Samples written since the stream started. The only clock there is.
    clock: u64,
    next_auto_id: u64,
    /// (voice id, sample to release it on).
    pending_offs: Vec<(u64, u64)>,
    /// (pad, sample it stops being lit on).
    flashes: Vec<(u8, u64)>,
    /// Notes the looper and arpeggiator asked for this sample: (id, pitch, pan).
    requests: Vec<(u64, i32, f32)>,
    /// Every note started, with the sample it started on - so the tests can
    /// check timing to the exact sample. Compiled out of the real program.
    #[cfg(test)]
    started: Vec<(u64, i32)>,
}

impl AudioState {
    fn new(rx: Receiver<Msg>, sample_rate: f32, patch: Patch, status: Arc<Status>) -> Self {
        // The shortest note worth playing: long enough for any patch's attack
        // to open, so a looped stab or a fast arpeggio is a note, not a click.
        let min_note = (sample_rate * 0.008) as u64;
        AudioState {
            voices: Vec::with_capacity(MAX_VOICES),
            reverb: Reverb::new(sample_rate),
            patch,
            looper: Looper::new(min_note),
            arp: Arp::new(sample_rate, min_note),
            rx,
            status,
            sample_rate,
            clock: 0,
            next_auto_id: AUTO_ID_BASE,
            pending_offs: Vec::with_capacity(MAX_PENDING_OFFS),
            flashes: Vec::with_capacity(MAX_FLASHES),
            requests: Vec::with_capacity(MAX_VOICES),
            #[cfg(test)]
            started: Vec::new(),
        }
    }

    fn spawn(&mut self, id: u64, midi: i32, pan: f32) {
        if self.voices.len() >= MAX_VOICES {
            return;
        }
        let freq = crate::theory::midi_to_freq(midi);
        self.voices
            .push(Voice::new(id, freq, pan, &self.patch, self.sample_rate));
        #[cfg(test)]
        self.started.push((self.clock, midi));
    }

    fn release_live_voices(&mut self) {
        for v in self.voices.iter_mut().filter(|v| v.id < AUTO_ID_BASE) {
            v.release();
        }
    }

    /// Drain the queue. `try_recv` never blocks, which is the whole point.
    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            let now = self.clock;
            match msg {
                Msg::NoteOn { id, chord, degree } => {
                    if self.arp.on {
                        self.arp.hold_on(now, id, chord);
                    } else {
                        for (i, &midi) in chord.as_slice().iter().enumerate() {
                            self.spawn(id, midi, chord_pan(i, chord.len()));
                        }
                    }
                    // No-op unless the looper is armed or recording.
                    self.looper.note_on(now, id, chord, degree);
                }
                Msg::NoteOff { id } => {
                    for v in self.voices.iter_mut().filter(|v| v.id == id) {
                        v.release();
                    }
                    self.arp.hold_off(now, id);
                    self.looper.note_off(now, id);
                }
                Msg::ReleaseLive => {
                    self.release_live_voices();
                    self.arp.release_live(now);
                    self.looper.release_all(now);
                }
                Msg::SetPatch(i) => {
                    // Voices already sounding keep the patch they were born
                    // with; converting one mid-note would click.
                    self.patch = PATCHES[i.min(PATCHES.len() - 1)];
                }
                Msg::Pedal => self.looper.pedal(now),
                Msg::PlayStop => self.looper.play_stop(now),
                Msg::ClearLoop => self.looper.clear(),
                Msg::SetArp(on) => {
                    if on != self.arp.on {
                        // A chord that started as a block would end as an
                        // arpeggio, or the other way round. Let held notes go
                        // instead of trying to convert one mid-note.
                        self.release_live_voices();
                        self.arp.clear();
                        self.arp.on = on;
                    }
                }
                Msg::SetArpPattern(p) => self.arp.pattern = p,
                Msg::SetTempo(bpm) => self.arp.set_tempo(bpm),
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
            let now = self.clock;

            // Borrow the request list out for this sample. `take` swaps in an
            // empty Vec, which does not allocate, and it goes back below.
            let mut requests = std::mem::take(&mut self.requests);

            // The looper fires any event recorded for this exact sample.
            self.looper.tick(now, |event| {
                let until = now + event.dur;
                if self.flashes.len() < MAX_FLASHES {
                    self.flashes.push((event.degree, until));
                }
                if self.arp.on {
                    // A recorded chord is a hold over a known span - exactly
                    // what the arpeggiator eats - so loops arpeggiate too.
                    self.arp.schedule(now, event.chord, until);
                } else {
                    let id = self.next_auto_id;
                    self.next_auto_id += 1;
                    let n = event.chord.len();
                    for (i, &midi) in event.chord.as_slice().iter().enumerate() {
                        if requests.len() < MAX_VOICES {
                            requests.push((id, midi, chord_pan(i, n)));
                        }
                    }
                    if self.pending_offs.len() < MAX_PENDING_OFFS {
                        self.pending_offs.push((id, until));
                    }
                }
            });

            // The arpeggiator plays any step that falls on this sample.
            self.arp.tick(now, |midi, pan, gate| {
                let id = self.next_auto_id;
                self.next_auto_id += 1;
                if requests.len() < MAX_VOICES {
                    requests.push((id, midi, pan));
                }
                if self.pending_offs.len() < MAX_PENDING_OFFS {
                    self.pending_offs.push((id, now + gate));
                }
            });

            for &(id, midi, pan) in requests.iter() {
                self.spawn(id, midi, pan);
            }
            requests.clear();
            self.requests = requests;

            // Release anything whose time is up. swap_remove never allocates.
            let mut i = 0;
            while i < self.pending_offs.len() {
                let (id, at) = self.pending_offs[i];
                if at <= now {
                    for v in self.voices.iter_mut().filter(|v| v.id == id) {
                        v.release();
                    }
                    self.pending_offs.swap_remove(i);
                } else {
                    i += 1;
                }
            }

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
                    for slot in frame.iter_mut().skip(2) {
                        *slot = 0.0;
                    }
                }
            }

            self.clock += 1;
        }

        // Report back, once per buffer - a few milliseconds is far finer than
        // any screen redraw.
        let now = self.clock;
        self.flashes.retain(|&(_, until)| until > now);
        let mask = self
            .flashes
            .iter()
            .fold(0u8, |m, &(degree, _)| m | (1u8 << degree.min(7)));
        self.status.loop_state.store(self.looper.state as u8, Relaxed);
        self.status
            .loop_pos
            .store((self.looper.position(now) * 65535.0) as u32, Relaxed);
        self.status.looping_pads.store(mask, Relaxed);
    }
}

/// A running audio stream. Dropping this stops the sound, so `main` has to hold
/// on to it for as long as the window is open.
pub struct Engine {
    _stream: cpal::Stream,
    tx: Sender<Msg>,
    pub status: Arc<Status>,
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
        let status = Arc::new(Status::default());
        let mut state = AudioState::new(rx, sample_rate, patch, status.clone());

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
            status,
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

    /// Latency the buffer accounts for, in milliseconds - the part of the delay
    /// this program chose, and the part the browser would not let it choose.
    pub fn buffer_latency_ms(&self) -> Option<f32> {
        self.buffer_frames
            .map(|n| n as f32 / self.sample_rate * 1000.0)
    }
}

/// Render offline, with no sound card involved.
///
/// `events` are (seconds, message) pairs. Returns interleaved stereo f32.
/// `musika --render` uses this to write a WAV of the same engine the speakers
/// get - a far better way to judge a patch than any test could be.
pub fn render(
    patch: Patch,
    sample_rate: f32,
    seconds: f32,
    mut events: Vec<(f32, Msg)>,
) -> Vec<f32> {
    let (tx, rx) = channel::<Msg>();
    let mut state = AudioState::new(rx, sample_rate, patch, Arc::new(Status::default()));

    events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    events.reverse(); // so pop() gives the earliest

    let total = (seconds * sample_rate) as usize;
    let mut out = vec![0.0f32; total * 2];

    // The same block size the live stream asks for, so the offline render goes
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
    /// arithmetic over a buffer, so everything but the stream is testable.
    fn harness(patch_index: usize) -> (Sender<Msg>, AudioState) {
        let (tx, rx) = channel::<Msg>();
        let status = Arc::new(Status::default());
        (tx, AudioState::new(rx, SR, PATCHES[patch_index], status))
    }

    fn triad(root: i32) -> Chord {
        Chord::new(&[root, root + 4, root + 7])
    }

    /// Advance `n` frames of stereo; returns the loudest sample produced.
    fn frames(st: &mut AudioState, n: usize) -> f32 {
        let mut buf = vec![0.0f32; n * 2];
        st.fill(&mut buf, 2);
        buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn silence_until_something_is_pressed() {
        let (_tx, mut st) = harness(1);
        assert_eq!(frames(&mut st, 512), 0.0, "idle engine should be silent");
    }

    #[test]
    fn a_note_on_actually_makes_sound() {
        let (tx, mut st) = harness(1);
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        assert!(frames(&mut st, 4096) > 0.05, "expected audible output");
        assert_eq!(st.voices.len(), 3, "a triad is three voices");
    }

    #[test]
    fn a_released_note_decays_to_silence_and_is_reaped() {
        let (tx, mut st) = harness(0); // raw: no reverb tail to wait out
        tx.send(Msg::NoteOn { id: 7, chord: Chord::new(&[60]), degree: 0 }).unwrap();
        assert!(frames(&mut st, 2048) > 0.05);
        tx.send(Msg::NoteOff { id: 7 }).unwrap();
        let mut last = 1.0;
        for _ in 0..24 {
            last = frames(&mut st, 2048);
        }
        assert!(last < 0.001, "note kept ringing: {last}");
        frames(&mut st, 2048);
        assert_eq!(st.voices.len(), 0, "finished voice was not reaped");
    }

    #[test]
    fn releasing_one_id_leaves_the_others_ringing() {
        // Two inputs holding different chords: letting go of one must not
        // silence the other.
        let (tx, mut st) = harness(1);
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        tx.send(Msg::NoteOn { id: 2, chord: triad(67), degree: 4 }).unwrap();
        tx.send(Msg::NoteOff { id: 1 }).unwrap();
        frames(&mut st, 2048);
        assert_eq!(st.voices.iter().filter(|v| !v.finished()).count(), 6);
        for _ in 0..20 {
            frames(&mut st, 2048);
        }
        assert!(st.voices.len() >= 3, "id 2 was silenced along with id 1");
    }

    #[test]
    fn full_polyphony_never_clips() {
        let (tx, mut st) = harness(1);
        for id in 0..7u64 {
            let root = 48 + id as i32 * 2;
            tx.send(Msg::NoteOn { id, chord: triad(root), degree: id as u8 }).unwrap();
        }
        for _ in 0..8 {
            assert!(frames(&mut st, 4096) <= 1.0, "clipped");
        }
        assert_eq!(st.voices.len(), 21);
    }

    #[test]
    fn voice_count_is_capped() {
        let (tx, mut st) = harness(1);
        for id in 0..40u64 {
            tx.send(Msg::NoteOn { id, chord: triad(60), degree: 0 }).unwrap();
        }
        frames(&mut st, 256);
        assert!(st.voices.len() <= MAX_VOICES, "voice cap breached: {}", st.voices.len());
    }

    #[test]
    fn a_chord_is_actually_stereo() {
        let (tx, mut st) = harness(1);
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        let mut buf = vec![0.0f32; 8192];
        st.fill(&mut buf, 2);
        assert!(buf.chunks(2).any(|f| (f[0] - f[1]).abs() > 1e-4), "chord came out mono");
    }

    #[test]
    fn the_raw_patch_is_mono_by_design() {
        let (tx, mut st) = harness(0);
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        let mut buf = vec![0.0f32; 4096];
        st.fill(&mut buf, 2);
        assert!(buf.chunks(2).all(|f| (f[0] - f[1]).abs() < 1e-5), "raw should be centred");
    }

    #[test]
    fn reverb_keeps_sounding_after_the_notes_stop() {
        let (tx, mut st) = harness(1); // warm has a reverb send
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        frames(&mut st, 16_000);
        tx.send(Msg::NoteOff { id: 1 }).unwrap();
        frames(&mut st, 80_000); // well past the 0.45s release
        // Finished voices are reaped at the start of the next buffer, so ask
        // whether they have finished, not whether they are already gone.
        assert!(st.voices.iter().all(|v| v.finished()), "voices should be finished by now");
        assert!(frames(&mut st, 2048) > 0.0, "no reverb tail once the voices ended");
        assert_eq!(st.voices.len(), 0, "finished voices were not reaped");
    }

    #[test]
    fn everything_eventually_goes_quiet_after_releasing_live_input() {
        let (tx, mut st) = harness(1);
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        frames(&mut st, 2048);
        tx.send(Msg::ReleaseLive).unwrap();
        frames(&mut st, 48_000 * 6);
        assert!(frames(&mut st, 4096) < 0.001, "something survived");
        // Reaped at the start of that last buffer.
        assert_eq!(st.voices.len(), 0, "a voice outlived the release");
    }

    #[test]
    fn every_patch_plays_without_blowing_up() {
        for i in 0..PATCHES.len() {
            let (tx, mut st) = harness(i);
            tx.send(Msg::NoteOn { id: 1, chord: triad(48), degree: 0 }).unwrap();
            tx.send(Msg::NoteOn { id: 2, chord: triad(72), degree: 1 }).unwrap();
            for _ in 0..8 {
                let mut buf = vec![0.0f32; 8192];
                st.fill(&mut buf, 2);
                assert!(buf.iter().all(|s| s.is_finite()), "'{}' produced NaN", PATCHES[i].name);
                assert!(buf.iter().all(|s| s.abs() <= 1.0), "'{}' clipped", PATCHES[i].name);
            }
        }
    }

    #[test]
    fn switching_patch_does_not_disturb_notes_already_sounding() {
        let (tx, mut st) = harness(0);
        tx.send(Msg::NoteOn { id: 1, chord: Chord::new(&[60]), degree: 0 }).unwrap();
        frames(&mut st, 1024);
        tx.send(Msg::SetPatch(2)).unwrap();
        frames(&mut st, 1024);
        assert_eq!(st.voices.len(), 1, "the sounding note was dropped");
    }

    #[test]
    fn mono_devices_get_both_channels_summed() {
        let (tx, mut st) = harness(1);
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        let mut buf = vec![0.0f32; 4096];
        st.fill(&mut buf, 1);
        assert!(buf.iter().fold(0.0f32, |m, s| m.max(s.abs())) > 0.02, "mono output was silent");
    }

    // --- the looper, through the whole engine ---------------------------------

    /// Record chord 0 for 10,000 samples, closing the loop at 48,000.
    fn record_one_chord(tx: &Sender<Msg>, st: &mut AudioState) {
        tx.send(Msg::Pedal).unwrap();
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        frames(st, 10_000);
        tx.send(Msg::NoteOff { id: 1 }).unwrap();
        frames(st, 38_000);
        tx.send(Msg::Pedal).unwrap();
    }

    #[test]
    fn a_recorded_loop_comes_back_round_on_the_exact_sample() {
        let (tx, mut st) = harness(0);
        record_one_chord(&tx, &mut st);
        st.started.clear();
        frames(&mut st, 48_000 * 2 + 1);

        let starts: Vec<u64> = st.started.iter().map(|s| s.0).collect();
        assert_eq!(
            starts,
            vec![48_000, 48_000, 48_000, 96_000, 96_000, 96_000, 144_000, 144_000, 144_000]
        );
        assert_eq!(st.status.loop_state(), LoopState::Playing);
    }

    #[test]
    fn the_pad_a_loop_is_playing_lights_up() {
        let (tx, mut st) = harness(0);
        record_one_chord(&tx, &mut st);
        frames(&mut st, 256);
        assert!(st.status.pad_looping(0), "pad 0 should be lit by the loop");
        assert!(!st.status.pad_looping(4));
        frames(&mut st, 20_000); // past the 10,000-sample chord
        assert!(!st.status.pad_looping(0), "pad stayed lit after the chord ended");
    }

    #[test]
    fn letting_go_of_live_input_does_not_cut_the_loop_off() {
        let (tx, mut st) = harness(0);
        // A long chord this time: held for 40,000 of the loop's 48,000.
        tx.send(Msg::Pedal).unwrap();
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        frames(&mut st, 40_000);
        tx.send(Msg::NoteOff { id: 1 }).unwrap();
        frames(&mut st, 8_000);
        tx.send(Msg::Pedal).unwrap();
        frames(&mut st, 1_000); // the loop's chord is sounding again

        tx.send(Msg::ReleaseLive).unwrap(); // e.g. a key change
        frames(&mut st, 10_000);
        assert!(frames(&mut st, 2_000) > 0.05, "the looped chord was cut off");
    }

    // --- the arpeggiator, through the whole engine ----------------------------

    #[test]
    fn the_arpeggiator_plays_a_held_chord_one_note_per_step() {
        let (tx, mut st) = harness(0);
        tx.send(Msg::SetArp(true)).unwrap(); // 120bpm eighths: 12,000 samples a step
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        frames(&mut st, 12_000 * 3);
        assert_eq!(st.started, vec![(0, 60), (12_000, 64), (24_000, 67)]);
    }

    #[test]
    fn tempo_and_pattern_reach_the_arpeggiator() {
        let (tx, mut st) = harness(0);
        tx.send(Msg::SetArp(true)).unwrap();
        tx.send(Msg::SetTempo(60.0)).unwrap(); // 24,000 samples a step
        tx.send(Msg::SetArpPattern(ArpPattern::Down)).unwrap();
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        frames(&mut st, 24_000 * 2);
        assert_eq!(st.started, vec![(0, 67), (24_000, 64)]);
    }

    #[test]
    fn a_loop_recorded_as_chords_arpeggiates_once_the_arp_is_on() {
        let (tx, mut st) = harness(0);
        // Recorded as a block chord, held 24,000 samples, loop 48,000 long.
        tx.send(Msg::Pedal).unwrap();
        tx.send(Msg::NoteOn { id: 1, chord: triad(60), degree: 0 }).unwrap();
        frames(&mut st, 24_000);
        tx.send(Msg::NoteOff { id: 1 }).unwrap();
        frames(&mut st, 24_000);
        tx.send(Msg::Pedal).unwrap();
        tx.send(Msg::SetArp(true)).unwrap();
        st.started.clear();

        frames(&mut st, 24_000);
        // Steps at the loop start and one step later; the hold ends at 72,000.
        assert_eq!(st.started, vec![(48_000, 60), (60_000, 64)]);
    }
}

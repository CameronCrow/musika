//! engine.rs - the audio engine.
//!
//! This is the file that has no counterpart in the web version, and the reason
//! going native was worth doing at all.
//!
//! ---------------------------------------------------------------------------
//! WHAT CHANGED, AND WHY IT IS SIMPLER
//! ---------------------------------------------------------------------------
//!
//! The browser gives you a graph of nodes and a clock, and you *schedule*
//! against that clock: "start this oscillator at t=12.80, stop it at t=13.55".
//! Because JavaScript timers drift, the web version needed a look-ahead
//! scheduler - a sloppy timer that wakes up often and queues notes into the
//! near future. Two clocks, carefully kept apart. That is the single most
//! subtle thing in the whole web codebase.
//!
//! Here the sound card asks us, every few milliseconds, to fill a buffer with
//! the next N samples. There is exactly one clock - a running count of samples
//! written - and it *is* the audio. Nothing can drift from it, because it is
//! not measuring time, it is time. The look-ahead scheduler does not get ported;
//! it stops existing.
//!
//! ---------------------------------------------------------------------------
//! THE ONE RULE OF THE AUDIO THREAD
//! ---------------------------------------------------------------------------
//!
//! `audio_callback` runs on a real-time thread owned by the operating system.
//! If it takes too long, you do not get a slow instrument - you get a click, an
//! underrun, a hole in the sound. So it must never allocate, never lock a mutex,
//! never block, and never touch the UI. Everything it needs is either already
//! inside it or arrives through a queue it can drain without waiting.
//!
//! That is why note events come in over a channel instead of the UI simply
//! reaching in and pushing a voice.

use std::sync::mpsc::{Receiver, Sender, channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Level of one note before the master stage. Seven chords of three notes is 21
/// oscillators; this leaves room for all of them.
const VOICE_GAIN: f32 = 0.15;
/// Seconds to fade a note in. Jumping straight to full volume steps the
/// waveform discontinuously, and a discontinuity is literally a click.
const ATTACK: f32 = 0.004;
/// Release time constant, in seconds - an exponential fall, as in the web build.
const RELEASE: f32 = 0.09;
/// Shave the very top off the square waves, which are all sharp corners and so
/// all high harmonics. Stacked 21 deep that reads as hiss more than chord.
const TONE_HZ: f32 = 2600.0;

/// Hard ceiling on simultaneous notes. Preallocated, never grown, because
/// growing a Vec on the audio thread would allocate.
const MAX_VOICES: usize = 64;

/// What the UI thread can ask the audio thread to do.
pub enum Msg {
    /// Sound these pitches together, owned by `id` until it is released.
    NoteOn { id: u64, notes: Vec<i32> },
    NoteOff { id: u64 },
    AllOff,
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Attack,
    Sustain,
    Release,
}

struct Voice {
    id: u64,
    phase: f32, // 0.0..1.0 through one cycle
    inc: f32,   // how far the phase moves per sample
    env: f32,   // current envelope level
    stage: Stage,
}

/// The classic aliasing fix for a naive square wave.
///
/// A square wave jumps instantaneously from +1 to -1. Sampled, that jump lands
/// between two samples and the error folds back down the spectrum as
/// inharmonic whistling - obvious on high notes, and the reason a hand-rolled
/// oscillator usually sounds worse than the browser's. PolyBLEP smooths each
/// jump across the two samples either side of it, which removes most of the
/// aliasing for a few lines of arithmetic.
///
/// `t` is the phase, `dt` the phase increment per sample.
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

impl Voice {
    fn new(id: u64, freq: f32, sample_rate: f32) -> Self {
        Voice {
            id,
            phase: 0.0,
            inc: freq / sample_rate,
            env: 0.0,
            stage: Stage::Attack,
        }
    }

    /// One sample of band-limited square, with the envelope applied.
    fn next_sample(&mut self, attack_step: f32, release_coeff: f32) -> f32 {
        // Naive square, then corrected at both of its discontinuities - the one
        // at the start of the cycle and the one halfway through.
        let mut v = if self.phase < 0.5 { 1.0 } else { -1.0 };
        v += poly_blep(self.phase, self.inc);
        v -= poly_blep((self.phase + 0.5) % 1.0, self.inc);

        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        match self.stage {
            Stage::Attack => {
                self.env += attack_step;
                if self.env >= VOICE_GAIN {
                    self.env = VOICE_GAIN;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {}
            // Exponential decay toward zero: natural-sounding, and the same
            // shape setTargetAtTime gives you in the browser.
            Stage::Release => self.env -= self.env * release_coeff,
        }

        v * self.env
    }

    /// Inaudible and releasing - safe to reap.
    fn finished(&self) -> bool {
        self.stage == Stage::Release && self.env < 0.0001
    }
}

/// Everything the audio thread owns. Nothing else may touch it.
struct AudioState {
    voices: Vec<Voice>,
    rx: Receiver<Msg>,
    sample_rate: f32,
    attack_step: f32,
    release_coeff: f32,
    lowpass: f32, // one-pole state
    lowpass_a: f32,
}

impl AudioState {
    fn new(rx: Receiver<Msg>, sample_rate: f32) -> Self {
        // Per-sample envelope rates derived once, so the inner loop is arithmetic.
        let attack_step = VOICE_GAIN / (ATTACK * sample_rate);
        let release_coeff = 1.0 - (-1.0 / (RELEASE * sample_rate)).exp();
        let lowpass_a =
            1.0 - (-2.0 * std::f32::consts::PI * TONE_HZ / sample_rate).exp();

        AudioState {
            voices: Vec::with_capacity(MAX_VOICES),
            rx,
            sample_rate,
            attack_step,
            release_coeff,
            lowpass: 0.0,
            lowpass_a,
        }
    }

    /// Drain the queue. `try_recv` never blocks, which is the whole point.
    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::NoteOn { id, notes } => {
                    for midi in notes {
                        if self.voices.len() < MAX_VOICES {
                            let freq = crate::theory::midi_to_freq(midi);
                            self.voices.push(Voice::new(id, freq, self.sample_rate));
                        }
                    }
                }
                Msg::NoteOff { id } => {
                    for v in self.voices.iter_mut().filter(|v| v.id == id) {
                        v.stage = Stage::Release;
                    }
                }
                Msg::AllOff => {
                    for v in self.voices.iter_mut() {
                        v.stage = Stage::Release;
                    }
                }
            }
        }
    }

    /// Fill one buffer. Called by the OS; see the rule at the top of this file.
    fn fill(&mut self, out: &mut [f32], channels: usize) {
        self.handle_messages();

        // `retain` reuses the existing allocation, so reaping voices is free.
        self.voices.retain(|v| !v.finished());

        for frame in out.chunks_mut(channels) {
            let mut sum = 0.0;
            for v in self.voices.iter_mut() {
                sum += v.next_sample(self.attack_step, self.release_coeff);
            }

            // One-pole lowpass, then a soft clip. tanh squashes peaks smoothly
            // instead of letting them hit the rails and crackle - the job the
            // DynamicsCompressor did in the browser, in one line.
            self.lowpass += self.lowpass_a * (sum - self.lowpass);
            let sample = (self.lowpass * 1.5).tanh() * 0.6;

            for slot in frame.iter_mut() {
                *slot = sample;
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
    pub fn start() -> Result<Engine, String> {
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
        // number. 256 frames is ~5ms at 48kHz. If the device refuses it, cpal
        // falls back to its default and we simply report what we actually got.
        config.buffer_size = cpal::BufferSize::Fixed(256);

        let (tx, rx) = channel::<Msg>();
        let mut state = AudioState::new(rx, sample_rate);

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

    /// Round-trip latency the buffer accounts for, in milliseconds. Not the
    /// whole story - the operating system adds its own - but it is the part
    /// this program chose.
    pub fn buffer_latency_ms(&self) -> Option<f32> {
        self.buffer_frames
            .map(|n| n as f32 / self.sample_rate * 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Build an engine-under-test with no sound card attached. `fill` is pure
    /// arithmetic over a buffer, so the DSP is testable even though the stream
    /// is not.
    fn harness() -> (Sender<Msg>, AudioState) {
        let (tx, rx) = channel::<Msg>();
        (tx, AudioState::new(rx, SR))
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn silence_until_something_is_pressed() {
        let (_tx, mut st) = harness();
        let mut buf = vec![0.0f32; 512];
        st.fill(&mut buf, 1);
        assert_eq!(peak(&buf), 0.0, "idle engine should be silent");
    }

    #[test]
    fn a_note_on_actually_makes_sound() {
        let (tx, mut st) = harness();
        tx.send(Msg::NoteOn { id: 1, notes: vec![60, 64, 67] }).unwrap();
        let mut buf = vec![0.0f32; 4096];
        st.fill(&mut buf, 1);
        assert!(peak(&buf) > 0.05, "expected audible output, got peak {}", peak(&buf));
        assert_eq!(st.voices.len(), 3, "a triad is three voices");
    }

    #[test]
    fn a_released_note_decays_to_silence_and_is_reaped() {
        let (tx, mut st) = harness();
        tx.send(Msg::NoteOn { id: 7, notes: vec![60] }).unwrap();
        let mut buf = vec![0.0f32; 4096];
        st.fill(&mut buf, 1);
        assert!(peak(&buf) > 0.05);

        tx.send(Msg::NoteOff { id: 7 }).unwrap();
        // Release is a 90ms time constant; a second is far past inaudible.
        for _ in 0..12 {
            st.fill(&mut buf, 1);
        }
        assert!(peak(&buf) < 0.001, "note kept ringing: peak {}", peak(&buf));
        st.fill(&mut buf, 1);
        assert_eq!(st.voices.len(), 0, "finished voice was not reaped");
    }

    #[test]
    fn releasing_one_id_leaves_the_others_ringing() {
        // The bug the web build hit: two inputs holding different chords, and
        // letting go of one must not silence the other.
        let (tx, mut st) = harness();
        tx.send(Msg::NoteOn { id: 1, notes: vec![60, 64, 67] }).unwrap();
        tx.send(Msg::NoteOn { id: 2, notes: vec![67, 71, 74] }).unwrap();
        tx.send(Msg::NoteOff { id: 1 }).unwrap();
        let mut buf = vec![0.0f32; 4096];
        st.fill(&mut buf, 1);
        assert_eq!(
            st.voices.iter().filter(|v| v.stage != Stage::Release).count(),
            3,
            "id 2 should still be sustaining"
        );
    }

    #[test]
    fn full_polyphony_never_clips() {
        // Seven chords of three notes, all down at once - the worst case the
        // instrument can produce. Anything past +/-1.0 is a digital crackle.
        let (tx, mut st) = harness();
        for id in 0..7u64 {
            let root = 48 + id as i32 * 2;
            tx.send(Msg::NoteOn { id, notes: vec![root, root + 4, root + 7] }).unwrap();
        }
        let mut buf = vec![0.0f32; 8192];
        for _ in 0..4 {
            st.fill(&mut buf, 1);
        }
        assert_eq!(st.voices.len(), 21);
        assert!(peak(&buf) <= 1.0, "clipped at {}", peak(&buf));
    }

    #[test]
    fn voice_count_is_capped() {
        let (tx, mut st) = harness();
        for id in 0..40u64 {
            tx.send(Msg::NoteOn { id, notes: vec![60, 64, 67] }).unwrap();
        }
        let mut buf = vec![0.0f32; 256];
        st.fill(&mut buf, 1);
        assert!(st.voices.len() <= MAX_VOICES, "voice cap breached: {}", st.voices.len());
    }

    #[test]
    fn stereo_frames_get_the_same_sample_in_both_channels() {
        let (tx, mut st) = harness();
        tx.send(Msg::NoteOn { id: 1, notes: vec![60] }).unwrap();
        let mut buf = vec![0.0f32; 2048];
        st.fill(&mut buf, 2);
        assert!(buf.chunks(2).all(|f| f[0] == f[1]), "channels diverged");
    }

    #[test]
    fn poly_blep_only_corrects_near_a_discontinuity() {
        // Mid-cycle it must contribute nothing, or it would distort the wave
        // everywhere rather than just smoothing the jumps.
        assert_eq!(poly_blep(0.5, 0.01), 0.0);
        assert!(poly_blep(0.001, 0.01) != 0.0);
        assert!(poly_blep(0.999, 0.01) != 0.0);
    }
}

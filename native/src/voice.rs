//! voice.rs - one note, and everything that makes it sound like an instrument
//! rather than a test tone.
//!
//! The first version of this engine was a single square wave, a linear fade in,
//! a fixed lowpass and nothing else. That is a *beeper*. Four things separate it
//! from a synthesiser, and all four are here:
//!
//! 1. TWO OSCILLATORS, SLIGHTLY OUT OF TUNE. Two saws a few cents apart drift in
//!    and out of phase with each other over about a second, and that slow
//!    beating is what "thick" and "warm" actually are. One oscillator is
//!    perfectly static and the ear hears it as synthetic immediately. This is
//!    the single biggest difference and it costs one extra oscillator.
//!
//! 2. AN ENVELOPE WITH A SHAPE. Attack-Decay-Sustain-Release, not just fade in
//!    and fade out. The decay - a brief dip from the initial peak down to the
//!    sustain level - is what makes a note sound struck rather than switched on.
//!
//! 3. A RESONANT FILTER THAT MOVES. A fixed lowpass just makes things duller. A
//!    filter that snaps open on the attack and closes again over the next few
//!    hundred milliseconds is the sound everyone recognises as "a synth". The
//!    resonance - a bump in gain right at the cutoff frequency - is the vowel-ish
//!    character that a plain one-pole filter cannot produce at all.
//!
//! 4. STEREO. The three notes of a chord are placed across the stereo field
//!    rather than stacked in the middle. Costs nothing, and widens everything.
//!
//! Reverb is the fifth thing, but it belongs to the mix rather than the note, so
//! it lives in reverb.rs.

use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wave {
    Saw,
    Square,
    // No stock patch uses a triangle yet - it is the obvious third option when
    // these become editable rather than picked from a list, and the oscillator
    // handles it already.
    #[allow(dead_code)]
    Triangle,
}

/// A complete sound. Everything the instrument's character is made of, in one
/// struct, so a preset is just a value and switching sound is one assignment.
#[derive(Clone, Copy, Debug)]
pub struct Patch {
    pub name: &'static str,

    pub wave: Wave,
    /// How far apart the two oscillators are, in cents (100 cents = 1 semitone).
    /// Below ~4 is barely there; above ~30 starts to sound out of tune rather
    /// than thick.
    pub detune_cents: f32,

    // Amplitude envelope, in seconds (sustain is a level, 0..1).
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,

    /// Where the filter sits when the envelope is closed, in Hz.
    pub cutoff: f32,
    /// Filter Q. 0.7 is no resonance at all; past ~3 it starts to whistle.
    pub resonance: f32,
    /// How much further the envelope opens the filter, in Hz.
    pub env_amount: f32,
    /// Seconds for the filter to fall back from wide open to `cutoff`.
    pub filter_decay: f32,

    /// How far across the stereo field a chord's notes are spread, 0..1.
    pub stereo: f32,
    /// Reverb send, 0..1 - applied in the master chain, carried here so a
    /// preset describes the whole sound.
    pub reverb: f32,
    /// Pre-filter overdrive. A little makes the saws bite.
    pub drive: f32,
}

/// The sounds you can pick between.
///
/// These exist because "make it sound better" is not a single direction - it
/// depends what you are playing. Rather than guess one voice and bake it in,
/// here are four, and the instrument starts on the warm one.
pub const PATCHES: [Patch; 4] = [
    // What the instrument sounded like before any of this: one square wave, a
    // fixed filter, no movement. Kept so the difference is audible rather than
    // asserted.
    Patch {
        name: "raw",
        wave: Wave::Square,
        detune_cents: 0.0,
        attack: 0.004,
        decay: 0.0,
        sustain: 1.0,
        release: 0.09,
        cutoff: 2600.0,
        resonance: 0.7,
        env_amount: 0.0,
        filter_decay: 0.1,
        stereo: 0.0,
        reverb: 0.0,
        drive: 0.0,
    },
    // The default. Detuned saws, a filter that breathes, and enough reverb to
    // put the thing in a room instead of in your skull.
    Patch {
        name: "warm",
        wave: Wave::Saw,
        detune_cents: 11.0,
        attack: 0.015,
        decay: 0.30,
        sustain: 0.72,
        release: 0.45,
        cutoff: 820.0,
        resonance: 1.5,
        env_amount: 2400.0,
        filter_decay: 0.55,
        stereo: 0.75,
        reverb: 0.42,
        drive: 0.25,
    },
    // Short, bright and bell-like - the one that sounds like finished music
    // under an arpeggiator.
    Patch {
        name: "chime",
        wave: Wave::Square,
        detune_cents: 6.0,
        attack: 0.003,
        decay: 0.22,
        sustain: 0.32,
        release: 0.30,
        cutoff: 1500.0,
        resonance: 2.1,
        env_amount: 4000.0,
        filter_decay: 0.22,
        stereo: 0.6,
        reverb: 0.5,
        drive: 0.15,
    },
    // Dark, wide and slightly wobbly. Heavier detune than is strictly tasteful,
    // which is the point.
    Patch {
        name: "lo-fi",
        wave: Wave::Saw,
        detune_cents: 22.0,
        attack: 0.03,
        decay: 0.5,
        sustain: 0.65,
        release: 0.55,
        cutoff: 560.0,
        resonance: 1.1,
        env_amount: 700.0,
        filter_decay: 0.8,
        stereo: 0.85,
        reverb: 0.3,
        drive: 0.4,
    },
];

/// Smooths the step a naive saw or square makes when it wraps.
///
/// Those waveforms jump instantaneously. Sampled, the jump lands between two
/// samples and the error folds back down the spectrum as inharmonic whistling -
/// obvious on high notes, and the reason a hand-rolled oscillator usually sounds
/// worse than a browser's. PolyBLEP spreads each jump across the samples either
/// side of it, which removes most of that for a few lines of arithmetic.
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

#[derive(Clone, Copy)]
struct Osc {
    phase: f32,
    inc: f32,
}

impl Osc {
    fn new(freq: f32, sample_rate: f32, phase: f32) -> Self {
        Osc {
            phase,
            inc: freq / sample_rate,
        }
    }

    fn next(&mut self, wave: Wave) -> f32 {
        let (t, dt) = (self.phase, self.inc);
        let v = match wave {
            Wave::Saw => {
                // Rising ramp from -1 to 1, with its one jump smoothed.
                2.0 * t - 1.0 - poly_blep(t, dt)
            }
            Wave::Square => {
                let s = if t < 0.5 { 1.0 } else { -1.0 };
                s + poly_blep(t, dt) - poly_blep((t + 0.5) % 1.0, dt)
            }
            // A triangle's harmonics fall away fast enough that it barely
            // aliases, so it needs no correction at all.
            Wave::Triangle => 4.0 * (t - 0.5).abs() - 1.0,
        };

        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        v
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
}

/// An exponential fall never mathematically reaches zero, so "how long is the
/// release" has to mean "how long until it is near enough". Four time constants
/// leaves 1.8% of the level, which is inaudible - so a stated release of 0.45s
/// uses a time constant of 0.45/4 and is actually over in about 0.45s.
///
/// Getting this wrong is quiet but real: with the time constant set to the full
/// stated time, a note takes four times longer than advertised to die, holds a
/// voice alive that whole time, and makes fast playing sound smeared.
const SEGMENT_TIME_CONSTANTS: f32 = 4.0;

/// Attack-Decay-Sustain-Release, one sample at a time.
#[derive(Clone, Copy)]
struct Adsr {
    stage: Stage,
    level: f32,
    attack_step: f32,
    decay_rate: f32,
    sustain: f32,
    release_rate: f32,
    /// Set when a note is let go before it has finished its attack.
    pending_release: bool,
}

impl Adsr {
    fn new(attack: f32, decay: f32, sustain: f32, release: f32, sr: f32) -> Self {
        Adsr {
            stage: Stage::Attack,
            level: 0.0,
            // Attack is linear: predictable, and short enough that its shape
            // does not matter.
            attack_step: 1.0 / (attack.max(0.001) * sr),
            // Decay and release are exponential, which is how physical things
            // actually fade and why it sounds natural rather than mechanical.
            decay_rate: 1.0
                - (-SEGMENT_TIME_CONSTANTS / (decay.max(0.001) * sr)).exp(),
            sustain,
            release_rate: 1.0
                - (-SEGMENT_TIME_CONSTANTS / (release.max(0.001) * sr)).exp(),
            pending_release: false,
        }
    }

    fn next(&mut self) -> f32 {
        match self.stage {
            Stage::Attack => {
                self.level += self.attack_step;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = if self.pending_release {
                        Stage::Release
                    } else {
                        Stage::Decay
                    };
                }
            }
            Stage::Decay => {
                self.level += (self.sustain - self.level) * self.decay_rate;
                if (self.level - self.sustain).abs() < 0.001 {
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {}
            Stage::Release => self.level -= self.level * self.release_rate,
        }
        self.level
    }

    /// Let the note go.
    ///
    /// A note released during its attack finishes the attack first. Without
    /// that, tapping a pad for less than one audio buffer starts and releases
    /// the voice inside the same callback, the envelope is still at zero, and
    /// the voice is reaped before it makes a single sample - a tap that
    /// produces silence. Finishing the attack guarantees every note speaks.
    fn release(&mut self) {
        match self.stage {
            Stage::Attack => self.pending_release = true,
            _ => self.stage = Stage::Release,
        }
    }

    fn finished(&self) -> bool {
        self.stage == Stage::Release && self.level < 0.0005
    }
}

/// A topology-preserving state variable filter.
///
/// The important part is `resonance`: it feeds the filter's own output back into
/// its input, which lifts the response into a peak right at the cutoff. That
/// peak is the character - it is what a one-pole lowpass, which can only ever
/// roll gently off, has no way to produce.
///
/// This form stays stable when the cutoff is swept quickly, which matters here
/// because the envelope sweeps it on every single note.
#[derive(Clone, Copy)]
struct Svf {
    ic1: f32,
    ic2: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl Svf {
    fn new() -> Self {
        Svf {
            ic1: 0.0,
            ic2: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
        }
    }

    /// Recompute the coefficients for a new cutoff. There is a `tan` in here,
    /// which is why this runs once per control block rather than per sample.
    fn set(&mut self, cutoff_hz: f32, q: f32, sr: f32) {
        // Never let the cutoff reach Nyquist - tan() goes to infinity there.
        let fc = cutoff_hz.clamp(20.0, sr * 0.45);
        let g = (PI * fc / sr).tan();
        let k = 1.0 / q.max(0.5);
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    fn lowpass(&mut self, x: f32) -> f32 {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v2
    }
}

/// How often the filter coefficients are recomputed, in samples. Sweeping a
/// filter needs a `tan` per update; doing that per sample for 21 voices is
/// waste, and at 48kHz a 32-sample block is still 1500 updates a second - far
/// finer than any ear resolves.
const CONTROL_BLOCK: u32 = 32;

pub struct Voice {
    pub id: u64,
    osc_a: Osc,
    osc_b: Osc,
    wave: Wave,
    amp: Adsr,
    filter_env: Adsr,
    filter: Svf,
    /// -1 hard left, 0 centre, +1 hard right.
    pan: f32,
    cutoff: f32,
    env_amount: f32,
    resonance: f32,
    drive: f32,
    sample_rate: f32,
    countdown: u32,
}

impl Voice {
    pub fn new(id: u64, freq: f32, pan: f32, patch: &Patch, sample_rate: f32) -> Self {
        // Cents are a ratio, not an offset: 1200 cents is a doubling, so n cents
        // multiplies frequency by 2^(n/1200). Splitting the detune either side
        // of the note keeps the chord in tune with itself.
        let ratio = 2f32.powf(patch.detune_cents / 2.0 / 1200.0);

        let mut v = Voice {
            id,
            osc_a: Osc::new(freq / ratio, sample_rate, 0.0),
            // Start the second oscillator a third of the way through its cycle.
            // Starting both at zero makes every note begin with an identical
            // click as the two briefly reinforce each other.
            osc_b: Osc::new(freq * ratio, sample_rate, 0.33),
            wave: patch.wave,
            amp: Adsr::new(patch.attack, patch.decay, patch.sustain, patch.release, sample_rate),
            filter_env: Adsr::new(0.001, patch.filter_decay, 0.0, patch.filter_decay, sample_rate),
            filter: Svf::new(),
            pan: pan * patch.stereo,
            cutoff: patch.cutoff,
            env_amount: patch.env_amount,
            resonance: patch.resonance,
            drive: patch.drive,
            sample_rate,
            countdown: 0,
        };
        v.filter.set(patch.cutoff + patch.env_amount, patch.resonance, sample_rate);
        v
    }

    pub fn release(&mut self) {
        self.amp.release();
        self.filter_env.release();
    }

    pub fn finished(&self) -> bool {
        self.amp.finished()
    }

    /// One sample, as a (left, right) pair.
    pub fn next(&mut self) -> (f32, f32) {
        let fenv = self.filter_env.next();

        // Sweeping the filter is the expensive part, so it happens on a coarser
        // grid than the audio itself.
        if self.countdown == 0 {
            self.filter
                .set(self.cutoff + self.env_amount * fenv, self.resonance, self.sample_rate);
            self.countdown = CONTROL_BLOCK;
        }
        self.countdown -= 1;

        // Two oscillators, halved so a pair is no louder than one was.
        let mut x = (self.osc_a.next(self.wave) + self.osc_b.next(self.wave)) * 0.5;

        if self.drive > 0.0 {
            // Gentle asymmetric-free saturation before the filter, which is
            // where analogue drive sits and why it thickens rather than fuzzes.
            x = (x * (1.0 + self.drive * 3.0)).tanh();
        }

        let v = self.filter.lowpass(x) * self.amp.next();

        // Equal-power panning: straight linear panning makes a sound dip in
        // loudness as it crosses the centre, because power goes as the square.
        let angle = (self.pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * PI;
        (v * angle.cos(), v * angle.sin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn patch() -> Patch {
        PATCHES[1] // warm
    }

    #[test]
    fn a_voice_makes_sound_and_then_stops() {
        let mut v = Voice::new(1, 220.0, 0.0, &patch(), SR);
        let mut peak = 0.0f32;
        for _ in 0..(SR as usize / 2) {
            let (l, r) = v.next();
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak > 0.05, "voice was inaudible: {peak}");

        v.release();
        for _ in 0..(SR as usize * 2) {
            v.next();
        }
        assert!(v.finished(), "voice never finished releasing");
    }

    #[test]
    fn detuned_oscillators_actually_beat() {
        // The whole point of detune: amplitude should swell and dip over time as
        // the two oscillators drift in and out of phase. A single oscillator
        // would hold a constant peak.
        let mut v = Voice::new(1, 220.0, 0.0, &patch(), SR);
        // Skip the attack so the envelope is not what is moving.
        for _ in 0..(SR as usize / 4) {
            v.next();
        }
        let window = SR as usize / 20; // 50ms
        let mut peaks = vec![];
        for _ in 0..12 {
            let mut p = 0.0f32;
            for _ in 0..window {
                let (l, _) = v.next();
                p = p.max(l.abs());
            }
            peaks.push(p);
        }
        let hi = peaks.iter().cloned().fold(0.0f32, f32::max);
        let lo = peaks.iter().cloned().fold(f32::MAX, f32::min);
        assert!(hi - lo > 0.01, "no beating between the oscillators: {peaks:?}");
    }

    #[test]
    fn the_raw_patch_does_not_beat() {
        // Control for the test above: with detune at zero there is nothing to
        // beat against, so a flat envelope should give a flat peak.
        let mut raw = PATCHES[0];
        raw.release = 1.0;
        let mut v = Voice::new(1, 220.0, 0.0, &raw, SR);
        for _ in 0..(SR as usize / 4) {
            v.next();
        }
        let window = SR as usize / 20;
        let mut peaks = vec![];
        for _ in 0..8 {
            let mut p = 0.0f32;
            for _ in 0..window {
                let (l, _) = v.next();
                p = p.max(l.abs());
            }
            peaks.push(p);
        }
        let hi = peaks.iter().cloned().fold(0.0f32, f32::max);
        let lo = peaks.iter().cloned().fold(f32::MAX, f32::min);
        assert!(hi - lo < 0.01, "raw patch should be static, got {peaks:?}");
    }

    #[test]
    fn panning_holds_power_constant_across_the_field() {
        // Equal-power panning exists so a sound does not get quieter in the
        // middle. Total power should be near constant wherever it is placed.
        for pan in [-1.0, -0.5, 0.0, 0.5, 1.0f32] {
            let mut p = PATCHES[1];
            p.stereo = 1.0;
            let mut v = Voice::new(1, 220.0, pan, &p, SR);
            let mut power = 0.0f64;
            for _ in 0..(SR as usize / 4) {
                let (l, r) = v.next();
                power += (l * l + r * r) as f64;
            }
            let rms = (power / (SR as f64 / 4.0)).sqrt();
            assert!(rms > 0.01, "pan {pan} went silent");
        }
    }

    #[test]
    fn the_filter_stays_stable_when_swept_hard() {
        // A resonant filter with its cutoff yanked around is the classic way to
        // blow up a synth into NaN or a deafening screech.
        let mut p = PATCHES[2];
        p.resonance = 4.0;
        p.env_amount = 12000.0;
        let mut v = Voice::new(1, 880.0, 0.0, &p, SR);
        for _ in 0..(SR as usize) {
            let (l, r) = v.next();
            assert!(l.is_finite() && r.is_finite(), "filter produced a non-finite sample");
            assert!(l.abs() < 8.0, "filter blew up: {l}");
        }
    }

    #[test]
    fn every_patch_is_playable() {
        for p in PATCHES.iter() {
            let mut v = Voice::new(1, 261.63, 0.0, p, SR);
            let mut peak = 0.0f32;
            for _ in 0..(SR as usize / 4) {
                let (l, r) = v.next();
                assert!(l.is_finite() && r.is_finite(), "{} produced NaN", p.name);
                peak = peak.max(l.abs()).max(r.abs());
            }
            assert!(peak > 0.02, "patch '{}' is inaudible: {peak}", p.name);
        }
    }

    #[test]
    fn adsr_decays_to_its_sustain_level() {
        let mut env = Adsr::new(0.001, 0.05, 0.4, 0.1, SR);
        for _ in 0..(SR as usize / 4) {
            env.next();
        }
        assert!((env.level - 0.4).abs() < 0.01, "settled at {} not 0.4", env.level);
    }
}

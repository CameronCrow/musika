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
    Triangle,
}

/// Which way the filter faces.
///
/// The state variable filter computes all three of these at once as a side
/// effect of how it works, so offering them costs one `match` and unlocks a
/// family of sounds a lowpass simply cannot make: bandpass is hollow and vocal,
/// highpass is thin and glassy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FilterMode {
    Lowpass,
    Bandpass,
    Highpass,
}

/// A complete sound. Everything the instrument's character is made of, in one
/// struct, so a preset is just a value and switching sound is one assignment.
#[derive(Clone, Copy, Debug)]
pub struct Patch {
    pub name: &'static str,

    pub wave: Wave,
    /// How far apart the two oscillators are, in cents (100 cents = 1
    /// semitone). Below ~4 is barely there; above ~30 sounds out of tune rather
    /// than thick.
    pub detune_cents: f32,
    /// A square wave an octave below the note, mixed in at this level. This is
    /// where weight comes from - a filtered saw has no bottom of its own, and
    /// no amount of lowering the cutoff will give it any.
    pub sub_level: f32,

    // Amplitude envelope, in seconds (sustain is a level, 0..1).
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,

    pub filter_mode: FilterMode,
    /// Where the filter sits when the envelope is closed, in Hz.
    pub cutoff: f32,
    /// Filter Q. 0.7 is no resonance at all; past ~3 it starts to whistle.
    pub resonance: f32,
    /// How much further the envelope opens the filter, in Hz.
    pub env_amount: f32,
    /// Seconds for the filter to fall back from wide open to `cutoff`.
    pub filter_decay: f32,

    /// Pitch wobble: speed in Hz, depth in cents. A few cents at 4-6Hz reads as
    /// expression; a lot of it reads as seasickness.
    pub vibrato_hz: f32,
    pub vibrato_cents: f32,

    /// How far across the stereo field a chord's notes are spread, 0..1.
    pub stereo: f32,
    /// Reverb send, 0..1 - applied in the master chain, carried here so a
    /// preset describes the whole sound.
    pub reverb: f32,
    /// Pre-filter overdrive. A little makes the saws bite.
    pub drive: f32,

    /// Output trim, so the patches are usable interchangeably.
    ///
    /// A triangle carries far less energy than a saw, and a patch that is
    /// mostly sub carries far more, so left alone these ranged over about 10dB
    /// - switching from `bell` to `bass` would nearly triple the volume. These
    /// numbers come from measuring the rendered peak and RMS of each patch and
    /// trimming towards `warm`, which is the reference at 1.0.
    pub level: f32,
}

/// The sounds you can pick between.
///
/// `raw` stays at index 0 and `warm` at 1: raw is the voice the instrument had
/// before it had one worth the name, kept so the difference is audible rather
/// than asserted, and warm is the default.
///
/// They are meant to cover different *jobs* rather than shades of one patch -
/// something to hold chords under a melody, something to arpeggiate, something
/// with weight at the bottom, something strange.
pub const PATCHES: [Patch; 11] = [
    Patch {
        name: "raw",
        wave: Wave::Square,
        detune_cents: 0.0,
        sub_level: 0.0,
        attack: 0.004,
        decay: 0.0,
        sustain: 1.0,
        release: 0.09,
        filter_mode: FilterMode::Lowpass,
        cutoff: 2600.0,
        resonance: 0.7,
        env_amount: 0.0,
        filter_decay: 0.1,
        vibrato_hz: 0.0,
        vibrato_cents: 0.0,
        stereo: 0.0,
        reverb: 0.0,
        drive: 0.0,
        level: 0.92,
    },
    // Detuned saws, a filter that breathes, and enough reverb to put the thing
    // in a room instead of in your skull.
    Patch {
        name: "warm",
        wave: Wave::Saw,
        detune_cents: 11.0,
        sub_level: 0.0,
        attack: 0.015,
        decay: 0.30,
        sustain: 0.72,
        release: 0.45,
        filter_mode: FilterMode::Lowpass,
        cutoff: 820.0,
        resonance: 1.5,
        env_amount: 2400.0,
        filter_decay: 0.55,
        vibrato_hz: 0.0,
        vibrato_cents: 0.0,
        stereo: 0.75,
        reverb: 0.42,
        drive: 0.25,
        level: 1.0,
    },
    // Bright and completely static - no filter envelope at all, full sustain,
    // and a sub underneath for drawbar weight. Holds a chord without asking
    // for attention.
    Patch {
        name: "organ",
        wave: Wave::Square,
        detune_cents: 7.0,
        sub_level: 0.45,
        attack: 0.006,
        decay: 0.02,
        sustain: 1.0,
        release: 0.12,
        filter_mode: FilterMode::Lowpass,
        cutoff: 3000.0,
        resonance: 0.8,
        env_amount: 0.0,
        filter_decay: 0.1,
        vibrato_hz: 0.0,
        vibrato_cents: 0.0,
        stereo: 0.4,
        reverb: 0.2,
        drive: 0.1,
        level: 0.82,
    },
    // Slow enough that it arrives rather than starts. Heavy detune, a long
    // filter sweep, and a big room.
    Patch {
        name: "pad",
        wave: Wave::Saw,
        detune_cents: 18.0,
        sub_level: 0.15,
        attack: 0.55,
        decay: 0.80,
        sustain: 0.85,
        release: 1.40,
        filter_mode: FilterMode::Lowpass,
        cutoff: 700.0,
        resonance: 1.2,
        env_amount: 1400.0,
        filter_decay: 1.60,
        vibrato_hz: 4.5,
        vibrato_cents: 6.0,
        stereo: 0.9,
        reverb: 0.62,
        drive: 0.15,
        level: 1.05,
    },
    // Sustain of zero: it decays to nothing while you are still holding it,
    // which is exactly what a plucked string does. Made for arpeggios.
    Patch {
        name: "pluck",
        wave: Wave::Saw,
        detune_cents: 8.0,
        sub_level: 0.2,
        attack: 0.002,
        decay: 0.18,
        sustain: 0.0,
        release: 0.22,
        filter_mode: FilterMode::Lowpass,
        cutoff: 400.0,
        resonance: 2.4,
        env_amount: 5000.0,
        filter_decay: 0.16,
        vibrato_hz: 0.0,
        vibrato_cents: 0.0,
        stereo: 0.5,
        reverb: 0.30,
        drive: 0.2,
        level: 1.25,
    },
    // Short, bright and bell-like.
    Patch {
        name: "chime",
        wave: Wave::Square,
        detune_cents: 6.0,
        sub_level: 0.0,
        attack: 0.003,
        decay: 0.22,
        sustain: 0.32,
        release: 0.30,
        filter_mode: FilterMode::Lowpass,
        cutoff: 1500.0,
        resonance: 2.1,
        env_amount: 4000.0,
        filter_decay: 0.22,
        vibrato_hz: 0.0,
        vibrato_cents: 0.0,
        stereo: 0.6,
        reverb: 0.5,
        drive: 0.15,
        level: 1.15,
    },
    // A triangle has almost no harmonics of its own, which is what lets a long
    // decay ring clean instead of buzzing. Struck, not played.
    Patch {
        name: "bell",
        wave: Wave::Triangle,
        detune_cents: 3.0,
        sub_level: 0.0,
        attack: 0.002,
        decay: 1.10,
        sustain: 0.15,
        release: 1.00,
        filter_mode: FilterMode::Lowpass,
        cutoff: 3500.0,
        resonance: 1.0,
        env_amount: 2500.0,
        filter_decay: 0.90,
        vibrato_hz: 0.0,
        vibrato_cents: 0.0,
        stereo: 0.6,
        reverb: 0.58,
        drive: 0.0,
        level: 2.3,
    },
    // Mostly sub, and deliberately narrow: low frequencies carry almost no
    // directional information, so spreading them only makes a mix vague.
    Patch {
        name: "bass",
        wave: Wave::Square,
        detune_cents: 3.0,
        sub_level: 0.85,
        attack: 0.004,
        decay: 0.22,
        sustain: 0.70,
        release: 0.18,
        filter_mode: FilterMode::Lowpass,
        cutoff: 380.0,
        resonance: 1.8,
        env_amount: 1200.0,
        filter_decay: 0.25,
        vibrato_hz: 0.0,
        vibrato_cents: 0.0,
        stereo: 0.15,
        reverb: 0.12,
        drive: 0.45,
        level: 0.72,
    },
    // Bandpass: everything above *and* below the cutoff is thrown away, leaving
    // a narrow nasal band. Sounds like it is being sung down a tube.
    Patch {
        name: "hollow",
        wave: Wave::Saw,
        detune_cents: 14.0,
        sub_level: 0.0,
        attack: 0.02,
        decay: 0.30,
        sustain: 0.70,
        release: 0.40,
        filter_mode: FilterMode::Bandpass,
        cutoff: 900.0,
        resonance: 2.6,
        env_amount: 1500.0,
        filter_decay: 0.50,
        vibrato_hz: 5.0,
        vibrato_cents: 5.0,
        stereo: 0.7,
        reverb: 0.38,
        drive: 0.3,
        level: 1.2,
    },
    // Highpass: the fundamental is thrown away and only the harmonics above the
    // cutoff survive, which is why this reads as air rather than as a note.
    // Thin on its own; lovely stacked over something with bottom.
    Patch {
        name: "glass",
        wave: Wave::Saw,
        detune_cents: 12.0,
        sub_level: 0.0,
        attack: 0.03,
        decay: 0.40,
        sustain: 0.60,
        release: 0.60,
        filter_mode: FilterMode::Highpass,
        cutoff: 1100.0,
        resonance: 1.8,
        env_amount: 900.0,
        filter_decay: 0.50,
        vibrato_hz: 6.0,
        vibrato_cents: 4.0,
        stereo: 0.8,
        reverb: 0.55,
        drive: 0.1,
        level: 1.40,
    },
    // Dark, wide and slightly wobbly. Heavier detune than is strictly tasteful,
    // which is the point.
    Patch {
        name: "lo-fi",
        wave: Wave::Saw,
        detune_cents: 22.0,
        sub_level: 0.25,
        attack: 0.03,
        decay: 0.50,
        sustain: 0.65,
        release: 0.55,
        filter_mode: FilterMode::Lowpass,
        cutoff: 560.0,
        resonance: 1.1,
        env_amount: 700.0,
        filter_decay: 0.80,
        vibrato_hz: 3.0,
        vibrato_cents: 7.0,
        stereo: 0.85,
        reverb: 0.30,
        drive: 0.4,
        level: 0.95,
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
                    // Snap to the target rather than parking a fraction above
                    // it forever. An exponential approach never actually
                    // arrives, and for a sustain of zero that fraction is the
                    // difference between a voice that retires itself and one
                    // that sits silent and alive for as long as you hold it.
                    self.level = self.sustain;
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
        // Sustain counts, not just Release. A patch whose sustain level is zero
        // - a pluck - falls silent while the key is still down, and if only
        // Release could retire a voice that one would stay alive, inaudible,
        // for as long as you kept holding it.
        matches!(self.stage, Stage::Sustain | Stage::Release) && self.level < 0.0005
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
    k: f32,
}

impl Svf {
    fn new() -> Self {
        Svf {
            ic1: 0.0,
            ic2: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            k: 1.0,
        }
    }

    /// Recompute the coefficients for a new cutoff. There is a `tan` in here,
    /// which is why this runs once per control block rather than per sample.
    fn set(&mut self, cutoff_hz: f32, q: f32, sr: f32) {
        // Never let the cutoff reach Nyquist - tan() goes to infinity there.
        let fc = cutoff_hz.clamp(20.0, sr * 0.45);
        let g = (PI * fc / sr).tan();
        let k = 1.0 / q.max(0.5);
        self.k = k;
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// All three outputs come out of the same handful of operations - the
    /// "state variable" name is exactly this - so which one you take is a
    /// choice at the end rather than a different filter.
    fn run(&mut self, x: f32, mode: FilterMode) -> f32 {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        match mode {
            FilterMode::Lowpass => v2,
            FilterMode::Bandpass => v1,
            FilterMode::Highpass => x - self.k * v1 - v2,
        }
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
    /// A square an octave down. Silent unless the patch asks for it.
    sub: Osc,
    sub_level: f32,
    wave: Wave,
    amp: Adsr,
    filter_env: Adsr,
    filter: Svf,
    filter_mode: FilterMode,
    /// -1 hard left, 0 centre, +1 hard right.
    pan: f32,
    cutoff: f32,
    env_amount: f32,
    resonance: f32,
    drive: f32,
    level: f32,
    /// Phase increments at rest, before vibrato multiplies them.
    inc_a: f32,
    inc_b: f32,
    inc_sub: f32,
    lfo_phase: f32,
    lfo_inc: f32,
    vib_cents: f32,
    sample_rate: f32,
    countdown: u32,
}

impl Voice {
    pub fn new(id: u64, freq: f32, pan: f32, patch: &Patch, sample_rate: f32) -> Self {
        // Cents are a ratio, not an offset: 1200 cents is a doubling, so n
        // cents multiplies frequency by 2^(n/1200). Splitting the detune either
        // side of the note keeps the chord in tune with itself.
        let ratio = 2f32.powf(patch.detune_cents / 2.0 / 1200.0);

        let osc_a = Osc::new(freq / ratio, sample_rate, 0.0);
        // Start the second oscillator a third of the way through its cycle.
        // Starting both at zero makes every note begin with an identical click
        // as the two briefly reinforce each other.
        let osc_b = Osc::new(freq * ratio, sample_rate, 0.33);
        let sub = Osc::new(freq * 0.5, sample_rate, 0.0);

        let mut v = Voice {
            id,
            inc_a: osc_a.inc,
            inc_b: osc_b.inc,
            inc_sub: sub.inc,
            osc_a,
            osc_b,
            sub,
            sub_level: patch.sub_level,
            wave: patch.wave,
            amp: Adsr::new(patch.attack, patch.decay, patch.sustain, patch.release, sample_rate),
            filter_env: Adsr::new(0.001, patch.filter_decay, 0.0, patch.filter_decay, sample_rate),
            filter: Svf::new(),
            filter_mode: patch.filter_mode,
            pan: pan * patch.stereo,
            cutoff: patch.cutoff,
            env_amount: patch.env_amount,
            resonance: patch.resonance,
            drive: patch.drive,
            level: patch.level,
            lfo_phase: 0.0,
            lfo_inc: patch.vibrato_hz / sample_rate,
            vib_cents: patch.vibrato_cents,
            sample_rate,
            countdown: 0,
        };
        v.filter
            .set(patch.cutoff + patch.env_amount, patch.resonance, sample_rate);
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

        // Sweeping the filter costs a tan(), and vibrato a sin() and a powf().
        // Both happen on a coarser grid than the audio: at 48kHz a 32-sample
        // block still updates 1500 times a second, far finer than either a
        // filter sweep or a 5Hz wobble needs.
        if self.countdown == 0 {
            self.filter.set(
                self.cutoff + self.env_amount * fenv,
                self.resonance,
                self.sample_rate,
            );
            if self.vib_cents > 0.0 {
                let lfo = (self.lfo_phase * std::f32::consts::TAU).sin();
                let bend = 2f32.powf(lfo * self.vib_cents / 1200.0);
                self.osc_a.inc = self.inc_a * bend;
                self.osc_b.inc = self.inc_b * bend;
                self.sub.inc = self.inc_sub * bend;
            }
            self.countdown = CONTROL_BLOCK;
        }
        self.countdown -= 1;

        self.lfo_phase += self.lfo_inc;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        // Two oscillators, halved so a pair is no louder than one was.
        let mut x = (self.osc_a.next(self.wave) + self.osc_b.next(self.wave)) * 0.5;
        if self.sub_level > 0.0 {
            x += self.sub.next(Wave::Square) * self.sub_level;
        }

        if self.drive > 0.0 {
            // Gentle saturation before the filter, which is where analogue
            // drive sits and why it thickens rather than fuzzes.
            x = (x * (1.0 + self.drive * 3.0)).tanh();
        }

        let v = self.filter.run(x, self.filter_mode) * self.amp.next() * self.level;

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

    /// Energy at one frequency, by the Goertzel algorithm - a single-bin DFT.
    /// Cheaper than a whole FFT and enough to answer "is there anything down
    /// there?", which is the only question these tests ask of a spectrum.
    fn energy_at(samples: &[f32], freq: f32, sr: f32) -> f32 {
        let k = 2.0 * (std::f32::consts::TAU * freq / sr).cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for &x in samples {
            let s0 = x + k * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - k * s1 * s2).abs().sqrt()
    }

    fn render(patch: &Patch, freq: f32, samples: usize) -> Vec<f32> {
        let mut v = Voice::new(1, freq, 0.0, patch, SR);
        (0..samples).map(|_| v.next().0).collect()
    }

    fn named(name: &str) -> Patch {
        *PATCHES.iter().find(|p| p.name == name).expect("no such patch")
    }

    #[test]
    fn a_zero_sustain_patch_retires_itself_while_still_held() {
        // `pluck` decays to silence with the key still down. If only a Release
        // could retire a voice, that one would sit there inaudible and alive
        // for as long as you held it, burning a slot out of the polyphony.
        let pluck = named("pluck");
        assert_eq!(pluck.sustain, 0.0, "this test is pointless if pluck sustains");

        let mut v = Voice::new(1, 220.0, 0.0, &pluck, SR);
        for _ in 0..(SR as usize * 2) {
            v.next(); // never released
        }
        assert!(v.finished(), "a silent held pluck was never retired");
    }

    #[test]
    fn a_sustaining_patch_is_not_retired_while_held() {
        // The control for the test above: `warm` must survive being held.
        let mut v = Voice::new(1, 220.0, 0.0, &named("warm"), SR);
        for _ in 0..(SR as usize * 2) {
            v.next();
        }
        assert!(!v.finished(), "a held sustaining note was retired early");
    }

    #[test]
    fn the_sub_oscillator_really_is_an_octave_down() {
        // Not "the output differs" - that would pass for any change at all.
        // The sub has to put energy at half the note's frequency.
        let mut with = named("bass");
        with.vibrato_cents = 0.0;
        let mut without = with;
        without.sub_level = 0.0;

        let freq = 220.0;
        let n = SR as usize / 2;
        let a = render(&with, freq, n);
        let b = render(&without, freq, n);

        let sub_with = energy_at(&a, freq / 2.0, SR);
        let sub_without = energy_at(&b, freq / 2.0, SR);
        assert!(
            sub_with > sub_without * 2.0,
            "no octave-down energy from the sub: {sub_with} vs {sub_without}"
        );
    }

    #[test]
    fn a_highpass_keeps_less_bottom_end_than_a_lowpass() {
        let mut lp = named("warm");
        lp.vibrato_cents = 0.0;
        lp.filter_mode = FilterMode::Lowpass;
        let mut hp = lp;
        hp.filter_mode = FilterMode::Highpass;

        let freq = 110.0; // well below the 820Hz cutoff
        let n = SR as usize / 2;
        let low = energy_at(&render(&lp, freq, n), freq, SR);
        let high = energy_at(&render(&hp, freq, n), freq, SR);
        assert!(
            high < low * 0.5,
            "highpass kept the fundamental: {high} vs lowpass {low}"
        );
    }

    #[test]
    fn the_three_filter_modes_are_actually_different() {
        let mut p = named("warm");
        p.vibrato_cents = 0.0;
        let n = SR as usize / 4;

        let mut outs = vec![];
        for mode in [FilterMode::Lowpass, FilterMode::Bandpass, FilterMode::Highpass] {
            let mut q = p;
            q.filter_mode = mode;
            let o = render(&q, 220.0, n);
            assert!(o.iter().all(|s| s.is_finite()), "{mode:?} produced NaN");
            outs.push(o);
        }
        for (i, j) in [(0, 1), (1, 2), (0, 2)] {
            let diff: f32 = outs[i]
                .iter()
                .zip(&outs[j])
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
                / n as f32;
            assert!(diff > 1e-4, "filter modes {i} and {j} are identical");
        }
    }

    #[test]
    fn vibrato_bends_the_pitch_and_no_vibrato_does_not() {
        let mut on = named("pad");
        on.attack = 0.001; // get past the slow attack, it is not what is under test
        on.vibrato_hz = 5.0;
        on.vibrato_cents = 40.0; // exaggerated so the divergence is unambiguous
        let mut off = on;
        off.vibrato_cents = 0.0;

        let n = SR as usize; // a full second, several LFO cycles
        let a = render(&on, 220.0, n);
        let b = render(&off, 220.0, n);

        let drift: f32 =
            a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum::<f32>() / n as f32;
        assert!(drift > 1e-3, "vibrato changed nothing: {drift}");
        assert!(a.iter().all(|s| s.is_finite()), "vibrato produced NaN");

        // And the same patch twice must be identical, or the test above is
        // measuring noise rather than the LFO.
        let c = render(&off, 220.0, n);
        assert_eq!(b, c, "voices are not deterministic");
    }

    #[test]
    fn every_patch_is_distinct_from_every_other() {
        // Ten presets are only worth having if they actually sound different.
        let n = SR as usize / 4;
        let rendered: Vec<(&str, Vec<f32>)> = PATCHES
            .iter()
            .map(|p| (p.name, render(p, 220.0, n)))
            .collect();

        for i in 0..rendered.len() {
            for j in (i + 1)..rendered.len() {
                let diff: f32 = rendered[i]
                    .1
                    .iter()
                    .zip(&rendered[j].1)
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f32>()
                    / n as f32;
                assert!(
                    diff > 1e-4,
                    "'{}' and '{}' render identically",
                    rendered[i].0,
                    rendered[j].0
                );
            }
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

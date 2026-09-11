//! reverb.rs - the room the instrument is played in.
//!
//! Of everything that separates "a synth patch" from "a record", reverb is the
//! largest single item. A dry chord sounds like it is happening inside your
//! head; the same chord with a tail sounds like it is happening somewhere. It
//! costs about a hundred lines and changes more than any other hundred here.
//!
//! ---------------------------------------------------------------------------
//! HOW A REVERB IS BUILT OUT OF DELAYS
//! ---------------------------------------------------------------------------
//!
//! A real room returns thousands of copies of a sound, each a little later and
//! quieter than the last, having bounced off different surfaces. You cannot
//! simulate that literally in real time, so the standard trick - Schroeder's,
//! from the 1960s, and the basis of the well-known Freeverb - fakes it in two
//! stages:
//!
//!   COMB FILTERS make the tail. A comb is a delay line that feeds its own
//!   output back into its input, so one input produces a long series of fading
//!   echoes. Several in parallel, with deliberately *non-multiple* delay
//!   lengths, overlap into something dense. If the lengths shared factors the
//!   echoes would line up and you would hear a pitch rather than a room.
//!
//!   ALLPASS FILTERS smear it. An allpass passes every frequency at equal
//!   volume but scrambles their timing, which blurs the individual echoes into
//!   a wash. That is what turns a stack of distinct repeats into a tail.
//!
//! Damping is the other half of the realism: real rooms absorb treble faster
//! than bass, so each comb lowpasses its own feedback a little on every pass.
//! Without it the tail stays bright and sounds like a metal tank.

/// Comb delay lengths in samples, at 44.1kHz, scaled at runtime for other
/// rates. These are the Freeverb values and they are chosen to share no common
/// factors - that is the whole reason they sound like a room.
const COMB_LENGTHS: [usize; 4] = [1116, 1188, 1277, 1356];
const ALLPASS_LENGTHS: [usize; 2] = [556, 441];

/// The right channel reads from delays this many samples longer than the left,
/// so the two are decorrelated and the reverb is genuinely wide rather than the
/// same mono tail sent to both ears.
const STEREO_SPREAD: usize = 23;

struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Comb {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            store: 0.0,
        }
    }

    fn process(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.pos];
        // One-pole lowpass inside the feedback path: each time round the loop
        // the tail loses a little more treble, exactly as a room absorbs it.
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = input + self.store * feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Allpass {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buf[self.pos];
        let out = -input + buffered;
        // 0.5 is the conventional allpass coefficient; it is what makes the
        // gain flat across frequency while the phase is scrambled.
        self.buf[self.pos] = input + buffered * 0.5;
        self.pos = (self.pos + 1) % self.buf.len();
        out
    }
}

pub struct Reverb {
    combs_l: Vec<Comb>,
    combs_r: Vec<Comb>,
    allpass_l: Vec<Allpass>,
    allpass_r: Vec<Allpass>,
    feedback: f32,
    damp: f32,
}

impl Reverb {
    /// All the buffers are allocated here, on the thread that builds the
    /// engine - never on the audio thread, which must not allocate at all.
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / 44_100.0;
        let sized = |n: usize, extra: usize| ((n + extra) as f32 * scale) as usize;

        Reverb {
            combs_l: COMB_LENGTHS.iter().map(|&n| Comb::new(sized(n, 0))).collect(),
            combs_r: COMB_LENGTHS
                .iter()
                .map(|&n| Comb::new(sized(n, STEREO_SPREAD)))
                .collect(),
            allpass_l: ALLPASS_LENGTHS.iter().map(|&n| Allpass::new(sized(n, 0))).collect(),
            allpass_r: ALLPASS_LENGTHS
                .iter()
                .map(|&n| Allpass::new(sized(n, STEREO_SPREAD)))
                .collect(),
            // A long-ish but not endless tail, and a fairly absorbent room.
            feedback: 0.84,
            damp: 0.35,
        }
    }

    /// Wet signal only. The caller decides how much to blend back in, which is
    /// what makes the reverb amount a property of the patch.
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        // Combs run in parallel and are summed; the input is scaled down first
        // because four of them add up.
        let input = (left + right) * 0.015;

        let mut wl = 0.0;
        for c in self.combs_l.iter_mut() {
            wl += c.process(input, self.feedback, self.damp);
        }
        let mut wr = 0.0;
        for c in self.combs_r.iter_mut() {
            wr += c.process(input, self.feedback, self.damp);
        }

        // Allpasses run in series, each smearing the output of the last.
        for a in self.allpass_l.iter_mut() {
            wl = a.process(wl);
        }
        for a in self.allpass_r.iter_mut() {
            wr = a.process(wr);
        }

        (wl, wr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn silence_in_silence_out() {
        let mut r = Reverb::new(SR);
        for _ in 0..10_000 {
            let (l, rr) = r.process(0.0, 0.0);
            assert_eq!((l, rr), (0.0, 0.0));
        }
    }

    #[test]
    fn an_impulse_produces_a_tail_that_outlives_it() {
        let mut r = Reverb::new(SR);
        r.process(1.0, 1.0);

        // Nothing more goes in; anything that comes out is the room.
        let mut energy_early = 0.0f32;
        for _ in 0..(SR as usize / 10) {
            let (l, rr) = r.process(0.0, 0.0);
            energy_early += l.abs() + rr.abs();
        }
        assert!(energy_early > 0.0001, "no tail at all: {energy_early}");
    }

    #[test]
    fn the_tail_decays_rather_than_sustaining_or_exploding() {
        let mut r = Reverb::new(SR);
        r.process(1.0, 1.0);

        let chunk = SR as usize / 4;
        let mut energy = vec![];
        for _ in 0..4 {
            let mut e = 0.0f32;
            for _ in 0..chunk {
                let (l, rr) = r.process(0.0, 0.0);
                assert!(l.is_finite() && rr.is_finite(), "reverb went non-finite");
                e += l.abs() + rr.abs();
            }
            energy.push(e);
        }
        assert!(
            energy[3] < energy[0],
            "tail did not decay: {energy:?}"
        );
    }

    #[test]
    fn the_two_channels_are_not_identical() {
        // If they were, the reverb would be mono and add no width - the stereo
        // spread on the delay lengths is what prevents that.
        let mut r = Reverb::new(SR);
        r.process(1.0, 1.0);
        let mut differs = false;
        for _ in 0..(SR as usize / 4) {
            let (l, rr) = r.process(0.0, 0.0);
            if (l - rr).abs() > 1e-6 {
                differs = true;
            }
        }
        assert!(differs, "left and right tails are identical");
    }

    #[test]
    fn sustained_input_does_not_run_away() {
        // Feedback near 0.85 is close enough to 1.0 that a mistake here builds
        // without limit. Hold a loud signal in and check it stays bounded.
        let mut r = Reverb::new(SR);
        let mut peak = 0.0f32;
        for _ in 0..(SR as usize * 2) {
            let (l, rr) = r.process(1.0, 1.0);
            peak = peak.max(l.abs()).max(rr.abs());
        }
        assert!(peak.is_finite() && peak < 4.0, "reverb ran away to {peak}");
    }
}

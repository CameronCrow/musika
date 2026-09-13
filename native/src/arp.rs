//! arp.rs - the arpeggiator. Instead of a chord sounding as a block, its notes
//! take turns, one per step, for as long as the chord is held.
//!
//! This is what makes the instrument sound like finished music rather than
//! someone leaning on an organ, and the reason is worth knowing: three notes at
//! once is a texture, while the same three notes one after another is a
//! *pattern*, and a pattern has rhythm. The harmony does not change at all.
//!
//! ---------------------------------------------------------------------------
//! ONE IDEA: A HOLD
//! ---------------------------------------------------------------------------
//!
//! Chords reach the arpeggiator from two unrelated places - your finger, which
//! starts a chord now and ends it at some unknown moment, and the looper, which
//! knows in advance that a chord runs for exactly so many samples. Rather than
//! write the arpeggiator twice, both become the same thing: a HOLD, meaning "these
//! pitches are down over this span of samples". A live hold's end is simply
//! "never" until you let go.
//!
//! That shared idea is why a loop recorded as block chords starts arpeggiating
//! the moment you switch the arpeggiator on.
//!
//! ---------------------------------------------------------------------------
//! THE GRID
//! ---------------------------------------------------------------------------
//!
//! Steps are counted in samples, on the same clock the audio is made of. A step
//! at 120bpm and 48kHz is exactly 12,000 samples, every time. The web version
//! had to keep its own drifting timer and look ahead of it; this one can't be
//! late, because there is nothing to be late against.
//!
//! Like looper.rs, this knows nothing about sound: it decides which pitch should
//! start on which sample and hands that to the engine.

use crate::theory::{ArpPattern, Chord, arp_index};

/// Two steps per beat: eighth notes. Sixteenths are frantic at any usable tempo
/// and quarter notes barely register as an arpeggio.
const STEPS_PER_BEAT: f64 = 2.0;

pub const MIN_BPM: f32 = 40.0;
pub const MAX_BPM: f32 = 240.0;

/// Preallocated and never grown, for the same reason as everything else the
/// audio thread touches.
const MAX_HOLDS: usize = 64;

#[derive(Clone, Copy)]
struct Hold {
    /// `Some` for a finger, which the UI will release by id; `None` for a looped
    /// chord, which knows its own end.
    id: Option<u64>,
    chord: Chord,
    from: u64,
    until: u64,
    /// Counts from the moment this chord went down, not from a global beat, so
    /// every chord starts its pattern on its own first note.
    step: usize,
    /// The sound its notes play with - a looped layer keeps its own.
    patch: u8,
}

pub struct Arp {
    pub on: bool,
    pub pattern: ArpPattern,
    bpm: f32,
    step_len: u64,
    gate: u64,
    next_step: u64,
    holds: Vec<Hold>,
    sample_rate: f32,
    min_gate: u64,
}

impl Arp {
    /// `min_gate` is the shortest note it will play, in samples - never shorter
    /// than a voice's attack, or fast tempos would produce clicks, not notes.
    pub fn new(sample_rate: f32, min_gate: u64) -> Self {
        let mut arp = Arp {
            on: false,
            pattern: ArpPattern::Up,
            bpm: 120.0,
            step_len: 1,
            gate: 1,
            next_step: 0,
            holds: Vec::with_capacity(MAX_HOLDS),
            sample_rate,
            min_gate: min_gate.max(1),
        };
        arp.set_tempo(120.0);
        arp
    }

    pub fn set_tempo(&mut self, bpm: f32) {
        // A NaN or zero tempo would make the step infinitely long or zero
        // samples wide - a frozen arpeggiator or a machine gun. Clamp instead.
        let bpm = if bpm.is_finite() { bpm.clamp(MIN_BPM, MAX_BPM) } else { 120.0 };
        self.bpm = bpm;
        let seconds_per_step = 60.0 / bpm as f64 / STEPS_PER_BEAT;
        self.step_len = ((self.sample_rate as f64 * seconds_per_step).round() as u64).max(1);
        // Slightly shorter than a step, so consecutive notes separate audibly
        // instead of running together into one continuous tone.
        self.gate = ((self.step_len as f64 * 0.8) as u64)
            .max(self.min_gate)
            .min(self.step_len);
    }

    // Read by the tests only; the UI keeps its own copy of the tempo it set.
    #[cfg(test)]
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// One eighth note in samples - also the looper's grid.
    pub fn step_len(&self) -> u64 {
        self.step_len
    }

    #[cfg(test)]
    pub fn gate(&self) -> u64 {
        self.gate
    }

    fn sounding(&self, now: u64) -> bool {
        self.holds.iter().any(|h| h.from <= now && now < h.until)
    }

    fn add(&mut self, now: u64, id: Option<u64>, chord: Chord, until: u64, patch: u8) {
        if chord.is_empty() || self.holds.len() >= MAX_HOLDS {
            return;
        }
        // Nothing currently sounding: start the grid on this press, so the
        // first note lands now rather than up to a whole step late. When
        // something is already going, join its grid so the two stay in time.
        if !self.sounding(now) {
            self.next_step = now;
        }
        self.holds.push(Hold { id, chord, from: now, until, step: 0, patch });
    }

    /// A finger went down. It holds until `hold_off`.
    pub fn hold_on(&mut self, now: u64, id: u64, chord: Chord, patch: u8) {
        self.add(now, Some(id), chord, u64::MAX, patch);
    }

    /// A chord whose whole span is already known - the looper's.
    pub fn schedule(&mut self, now: u64, chord: Chord, until: u64, patch: u8) {
        self.add(now, None, chord, until, patch);
    }

    pub fn hold_off(&mut self, now: u64, id: u64) {
        for h in self.holds.iter_mut().filter(|h| h.id == Some(id)) {
            h.until = h.until.min(now);
        }
    }

    /// Let go of every finger, leaving the looper's chords alone.
    pub fn release_live(&mut self, now: u64) {
        for h in self.holds.iter_mut().filter(|h| h.id.is_some()) {
            h.until = h.until.min(now);
        }
    }

    /// Call once per sample. `emit(pitch, pan, gate, patch)` receives every note
    /// that starts on this sample and how many samples it should last.
    pub fn tick(&mut self, now: u64, mut emit: impl FnMut(i32, f32, u64, u8)) {
        if self.holds.is_empty() || now < self.next_step {
            return;
        }
        let (pattern, gate) = (self.pattern, self.gate);
        for h in self.holds.iter_mut() {
            if h.from <= now && now < h.until {
                let n = h.chord.len();
                let i = arp_index(pattern, n, h.step);
                // Spread by position in the chord, low left to high right -
                // the same layout a block chord gets, so switching the
                // arpeggiator on does not move the sound in the stereo field.
                let pan = if n > 1 { i as f32 / (n - 1) as f32 * 2.0 - 1.0 } else { 0.0 };
                emit(h.chord.notes[i], pan, gate, h.patch);
                h.step += 1;
            }
        }
        self.next_step += self.step_len;
        if self.next_step <= now {
            self.next_step = now + self.step_len;
        }
        // Retire finished holds on the step, not every sample: nothing between
        // steps can play, so there is no hurry.
        self.holds.retain(|h| h.until > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn c_major() -> Chord {
        Chord::new(&[60, 64, 67])
    }

    /// Run from `from` to `to` and collect (sample, pitch).
    fn run(a: &mut Arp, from: u64, to: u64) -> Vec<(u64, i32)> {
        let mut out = vec![];
        for now in from..to {
            a.tick(now, |p, _, _, _| out.push((now, p)));
        }
        out
    }

    #[test]
    fn a_step_at_120bpm_is_exactly_twelve_thousand_samples() {
        let a = Arp::new(SR, 400);
        assert_eq!(a.step_len(), 12_000); // 48000 * 60 / 120 / 2
        assert!(a.gate() < a.step_len(), "notes must separate");
    }

    #[test]
    fn the_first_note_plays_on_the_press_not_a_step_later() {
        let mut a = Arp::new(SR, 400);
        run(&mut a, 0, 5_000); // time passes with nothing held
        a.hold_on(5_000, 1, c_major(), 0);
        assert_eq!(run(&mut a, 5_000, 5_001), vec![(5_000, 60)]);
    }

    #[test]
    fn up_plays_the_chord_in_order_one_step_apart() {
        let mut a = Arp::new(SR, 400);
        a.hold_on(0, 1, c_major(), 0);
        assert_eq!(
            run(&mut a, 0, 12_000 * 4),
            vec![(0, 60), (12_000, 64), (24_000, 67), (36_000, 60)]
        );
    }

    #[test]
    fn down_and_up_down_follow_their_patterns() {
        let mut a = Arp::new(SR, 400);
        a.pattern = ArpPattern::Down;
        a.hold_on(0, 1, c_major(), 0);
        let got: Vec<i32> = run(&mut a, 0, 12_000 * 3).iter().map(|x| x.1).collect();
        assert_eq!(got, vec![67, 64, 60]);

        let mut a = Arp::new(SR, 400);
        a.pattern = ArpPattern::UpDown;
        a.hold_on(0, 1, c_major(), 0);
        let got: Vec<i32> = run(&mut a, 0, 12_000 * 6).iter().map(|x| x.1).collect();
        assert_eq!(got, vec![60, 64, 67, 64, 60, 64]);
    }

    #[test]
    fn a_second_chord_joins_the_first_one_s_grid() {
        let mut a = Arp::new(SR, 400);
        a.hold_on(0, 1, c_major(), 0);
        run(&mut a, 0, 5_000);
        a.hold_on(5_000, 2, Chord::new(&[67, 71, 74]), 0);
        // Nothing at 5000 - it waits for the shared step at 12000.
        let got = run(&mut a, 5_000, 12_001);
        assert_eq!(got, vec![(12_000, 64), (12_000, 67)]);
    }

    #[test]
    fn letting_go_stops_the_notes() {
        let mut a = Arp::new(SR, 400);
        a.hold_on(0, 1, c_major(), 0);
        run(&mut a, 0, 13_000);
        a.hold_off(13_000, 1);
        assert!(run(&mut a, 13_000, 100_000).is_empty());
    }

    #[test]
    fn a_looped_chord_plays_only_inside_its_span() {
        let mut a = Arp::new(SR, 400);
        a.schedule(1_000, c_major(), 30_000, 0);
        let got = run(&mut a, 0, 100_000);
        assert_eq!(got, vec![(1_000, 60), (13_000, 64), (25_000, 67)]);
    }

    #[test]
    fn releasing_fingers_leaves_the_loop_running() {
        let mut a = Arp::new(SR, 400);
        a.hold_on(0, 1, c_major(), 0);
        a.schedule(0, Chord::new(&[48]), 100_000, 0);
        run(&mut a, 0, 1);
        a.release_live(1);
        let got = run(&mut a, 1, 50_000);
        assert!(got.iter().all(|&(_, p)| p == 48), "a finger kept playing: {got:?}");
        assert!(!got.is_empty(), "the looped chord stopped too");
    }

    #[test]
    fn tempo_changes_the_spacing_and_bad_tempos_are_clamped() {
        let mut a = Arp::new(SR, 400);
        a.set_tempo(60.0);
        assert_eq!(a.step_len(), 24_000);
        a.set_tempo(0.0);
        assert_eq!(a.bpm(), MIN_BPM);
        a.set_tempo(f32::NAN);
        assert_eq!(a.bpm(), 120.0);
        a.set_tempo(10_000.0);
        assert_eq!(a.bpm(), MAX_BPM);
    }

    #[test]
    fn holds_never_grow_past_the_preallocated_storage() {
        let mut a = Arp::new(SR, 400);
        let cap = a.holds.capacity();
        for id in 0..(MAX_HOLDS as u64 * 3) {
            a.hold_on(0, id, c_major(), 0);
        }
        assert_eq!(a.holds.len(), MAX_HOLDS);
        assert_eq!(a.holds.capacity(), cap, "holds reallocated");
    }
}

//! theory.rs - the music theory layer, ported from `src/theory.js`.
//!
//! PURE FUNCTIONS ONLY. No audio, no UI, no state. Everything here is
//! arithmetic on integers, which is why it is the one part of the instrument
//! that can be tested properly - and why it was the only file that survived the
//! move to Rust essentially unchanged.
//!
//! The whole thing rests on one idea: a pitch is a number.
//!
//!   MIDI note numbers count semitones (the smallest step on a piano - one key
//!   to the very next key, black or white). Middle C is 60. C# is 61. D is 62.
//!   Twelve semitones later, 72, is C again - one octave up, and it sounds like
//!   "the same note, higher". So pitch arithmetic is just integer arithmetic,
//!   and "the same note in another octave" is just +/- 12.
//!
//! Nothing here hardcodes "C major has these chords". The chords are derived
//! from the interval pattern, so the same code produces correct chords for any
//! key and any mode.

/// A scale is a pattern of semitone offsets from a root. Out of the 12
/// semitones in an octave it picks 7 and calls them "in key" - and those 7 are
/// the only notes the instrument will ever play. That is the whole trick behind
/// "you cannot play a wrong note".
///
/// Major:  0, 2, 4, 5, 7, 9, 11
///          \/ \/ \/ \/ \/ \/ \/   gaps of 2,2,1,2,2,2,1 semitones
///
/// The two 1-semitone gaps are what make it sound major rather than just "seven
/// notes". Change the gap pattern and you have a different mode; that is all a
/// mode is.
pub const MAJOR_SCALE: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Natural minor - the same twelve semitones, three of them chosen differently.
/// Its gaps run 2,1,2,2,1,2,2: the same cycle of steps as major, started from a
/// different place. Stacking triads over it yields
///
///   minor, diminished, major, minor, minor, major, major
///      i        ii°      III    iv     v     VI    VII
///
/// with no new code below - which is the payoff for deriving chords instead of
/// tabulating them.
pub const MINOR_SCALE: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];

/// Sharps throughout. F# and Gb are the same key on a piano; an engraver would
/// care, an instrument you play does not.
pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Major,
    Minor,
    Diminished,
    Augmented,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Major,
    Minor,
}

impl Mode {
    pub fn pattern(self) -> [i32; 7] {
        match self {
            Mode::Major => MAJOR_SCALE,
            Mode::Minor => MINOR_SCALE,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Mode::Major => "major",
            Mode::Minor => "minor",
        }
    }
}

/// The note at a given position in the scale, as a MIDI number.
///
/// `index` is a scale position, NOT a semitone count: 0 is the first note of the
/// scale, 1 the second. Crucially it may run off either end, and when it does we
/// wrap to the start of the pattern and shift by an octave (+/-12 per wrap).
/// Index 7 is therefore the same letter as index 0, an octave higher.
///
/// This wrapping is the part that is easy to get wrong, and getting it wrong is
/// exactly what breaks the seventh chord.
///
/// The JS version needed `((i % n) + n) % n` because `%` there keeps the sign of
/// the left operand. Rust's `rem_euclid` and `div_euclid` are that correction
/// built in: they always floor toward negative infinity, which is precisely what
/// "how many octaves down did I go" means.
pub fn scale_note(root_midi: i32, pattern: &[i32; 7], index: i32) -> i32 {
    let len = pattern.len() as i32;
    let octave = index.div_euclid(len);
    let step = index.rem_euclid(len) as usize;
    root_midi + pattern[step] + 12 * octave
}

/// Build a triad on a scale degree by stacking thirds: take a note, skip one,
/// take the next, skip one, take the next - scale positions n, n+2, n+4.
/// "Skip one" is the entire rule; the chords of a key are just every-other-note
/// of that key's scale.
///
/// `degree` is 0-based because it indexes an array. Musicians count from 1 (the
/// "I chord", the "V chord"), so degree 0 is the I chord.
pub fn triad(root_midi: i32, pattern: &[i32; 7], degree: i32) -> [i32; 3] {
    [
        scale_note(root_midi, pattern, degree),
        scale_note(root_midi, pattern, degree + 2),
        scale_note(root_midi, pattern, degree + 4),
    ]
}

/// Name the flavour of a triad from the two gaps between its three notes.
///
/// Those gaps ARE the chord's identity; the actual pitches do not matter, which
/// is why this works out the same in every key.
///
///   4 then 3  -> major       bright, resolved      (C E G)
///   3 then 4  -> minor       darker, sadder        (D F A)
///   3 then 3  -> diminished  tense, unresolved     (B D F)
///   4 then 4  -> augmented   uneasy; never in a major scale
pub fn quality(notes: &[i32; 3]) -> Quality {
    match (notes[1] - notes[0], notes[2] - notes[1]) {
        (4, 3) => Quality::Major,
        (3, 4) => Quality::Minor,
        (3, 3) => Quality::Diminished,
        (4, 4) => Quality::Augmented,
        _ => Quality::Unknown,
    }
}

/// Every triad in the key, in order - the seven chords the instrument plays.
///
/// Over a major scale the qualities come out major, minor, minor, major, major,
/// minor, diminished. Nobody chose that; it falls out of the uneven gaps in the
/// pattern. It is also the single best sanity check on the wrap logic above - if
/// chord seven is not diminished, `scale_note` is broken.
pub fn chords_in_key(root_midi: i32, pattern: &[i32; 7]) -> [[i32; 3]; 7] {
    let mut out = [[0i32; 3]; 7];
    for (degree, chord) in out.iter_mut().enumerate() {
        *chord = triad(root_midi, pattern, degree as i32);
    }
    out
}

/// The order a chord's notes are played in when arpeggiated, as indices.
///
///   up       0 1 2       C E G   C E G   ...
///   down     2 1 0       G E C   G E C   ...
///   up-down  0 1 2 1     C E G E C E G E ...
///
/// Up-down drops the repeat of the top and bottom notes. The naive `0 1 2 2 1 0`
/// sounds the top note twice in a row and the turnaround stumbles - a limp
/// rather than a pulse. Chords shorter than three notes have no interior notes
/// to bounce off, so up-down is simply up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArpPattern {
    Up,
    Down,
    UpDown,
}

// The readable definition of the patterns, and the reference `arp_index` is
// tested against. The audio thread calls `arp_index` instead: same answer,
// without building a Vec - and building a Vec allocates.
#[allow(dead_code)]
pub fn arp_sequence(pattern: ArpPattern, length: usize) -> Vec<usize> {
    let up: Vec<usize> = (0..length).collect();
    match pattern {
        ArpPattern::Up => up,
        ArpPattern::Down => up.into_iter().rev().collect(),
        ArpPattern::UpDown if length < 3 => up,
        ArpPattern::UpDown => {
            let mut seq = up.clone();
            seq.extend(up[1..length - 1].iter().rev());
            seq
        }
    }
}

/// Which note of an `n`-note chord to play on step `step` of a pattern.
///
/// Exactly `arp_sequence(pattern, n)[step % its length]`, computed rather than
/// looked up. Up-down is a triangle wave over the indices: it rises for n-1
/// steps and falls for n-1 steps, so its period is 2n-2 - which for a triad is
/// the 0 1 2 1 that never repeats its endpoints.
pub fn arp_index(pattern: ArpPattern, n: usize, step: usize) -> usize {
    if n == 0 {
        return 0;
    }
    match pattern {
        ArpPattern::Up => step % n,
        ArpPattern::Down => n - 1 - step % n,
        ArpPattern::UpDown if n < 3 => step % n,
        ArpPattern::UpDown => {
            let period = 2 * n - 2;
            let p = step % period;
            if p < n { p } else { period - p }
        }
    }
}

/// A handful of pitches that sound together - a triad today, with room for a
/// 7th.
///
/// A fixed-size array rather than a `Vec`, because chords are sent to the audio
/// thread. A `Vec` would be freed on that thread when the message is dropped,
/// and freeing memory takes the allocator an unbounded amount of time - exactly
/// what the audio thread must never wait on. This is `Copy` and lives on the
/// stack, so sending one costs nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Chord {
    pub notes: [i32; 4],
    len: u8,
}

impl Chord {
    /// Keeps the first four pitches given.
    pub fn new(notes: &[i32]) -> Self {
        let n = notes.len().min(4);
        let mut array = [0; 4];
        array[..n].copy_from_slice(&notes[..n]);
        Chord { notes: array, len: n as u8 }
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[i32] {
        &self.notes[..self.len()]
    }
}

/// MIDI note number -> frequency in hertz, which is what an oscillator wants.
///
/// Anchor: MIDI 69 is A4, tuned to 440Hz by convention. Twelve semitones up
/// doubles the frequency (that is physically what an octave is), so one semitone
/// multiplies it by the twelfth root of two. Hence `2^(n/12)`.
pub fn midi_to_freq(midi: i32) -> f32 {
    440.0 * 2f32.powf((midi - 69) as f32 / 12.0)
}

pub fn note_name(midi: i32) -> &'static str {
    NOTE_NAMES[midi.rem_euclid(12) as usize]
}

/// How sheet music writes the chord: "C", "Dm", "Bdim".
pub fn chord_name(notes: &[i32; 3]) -> String {
    let root = note_name(notes[0]);
    match quality(notes) {
        Quality::Major => root.to_string(),
        Quality::Minor => format!("{root}m"),
        Quality::Diminished => format!("{root}dim"),
        Quality::Augmented => format!("{root}aug"),
        Quality::Unknown => root.to_string(),
    }
}

/// Roman numeral for a degree, carrying the quality in its casing: upper for
/// major, lower for minor, lower with a ring for diminished. This is the
/// notation the chords are actually written in - "I-V-vi-IV" means press pads
/// 1, 5, 6, 4 - so the pads are labelled with it.
pub fn roman_numeral(notes: &[i32; 3], degree: usize) -> String {
    const NUMERALS: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];
    let n = NUMERALS[degree];
    match quality(notes) {
        Quality::Major | Quality::Augmented => n.to_string(),
        Quality::Minor => n.to_lowercase(),
        Quality::Diminished => format!("{}°", n.to_lowercase()),
        Quality::Unknown => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C4: i32 = 60; // middle C

    #[test]
    fn major_scale_in_c_is_the_white_keys() {
        let notes: Vec<i32> = (0..7).map(|i| scale_note(C4, &MAJOR_SCALE, i)).collect();
        //                       C   D   E   F   G   A   B
        assert_eq!(notes, vec![60, 62, 64, 65, 67, 69, 71]);
    }

    #[test]
    fn positions_past_the_end_wrap_and_gain_an_octave() {
        assert_eq!(scale_note(C4, &MAJOR_SCALE, 7), 72); // C5
        assert_eq!(scale_note(C4, &MAJOR_SCALE, 8), 74); // D5
        assert_eq!(scale_note(C4, &MAJOR_SCALE, 13), 83); // B5
        assert_eq!(scale_note(C4, &MAJOR_SCALE, 14), 84); // C6
    }

    #[test]
    fn positions_below_zero_wrap_downwards() {
        // Inversions will reach below the root, and getting the sign handling
        // wrong there is silent and nasty.
        assert_eq!(scale_note(C4, &MAJOR_SCALE, -1), 59); // B3
        assert_eq!(scale_note(C4, &MAJOR_SCALE, -7), 48); // C3
        assert_eq!(scale_note(C4, &MAJOR_SCALE, -8), 47); // B2
    }

    #[test]
    fn triads_stack_every_other_scale_note() {
        assert_eq!(triad(C4, &MAJOR_SCALE, 0), [60, 64, 67]); // C  E  G
        assert_eq!(triad(C4, &MAJOR_SCALE, 1), [62, 65, 69]); // D  F  A
        assert_eq!(triad(C4, &MAJOR_SCALE, 4), [67, 71, 74]); // G  B  D5
    }

    #[test]
    fn last_triad_borrows_its_upper_notes_from_the_octave_above() {
        // The one that exposes broken wrap logic: B needs the D and F above it,
        // not the ones sitting below it in the same octave.
        assert_eq!(triad(C4, &MAJOR_SCALE, 6), [71, 74, 77]); // B  D5  F5
    }

    #[test]
    fn major_key_yields_the_headline_quality_sequence() {
        let q: Vec<Quality> = chords_in_key(C4, &MAJOR_SCALE).iter().map(quality).collect();
        use Quality::*;
        assert_eq!(q, vec![Major, Minor, Minor, Major, Major, Minor, Diminished]);
    }

    #[test]
    fn that_sequence_holds_in_every_one_of_the_12_keys() {
        use Quality::*;
        let expected = vec![Major, Minor, Minor, Major, Major, Minor, Diminished];
        for root in 48..60 {
            let q: Vec<Quality> = chords_in_key(root, &MAJOR_SCALE).iter().map(quality).collect();
            assert_eq!(q, expected, "broken for root MIDI {root}");
        }
    }

    #[test]
    fn minor_key_yields_minor_dim_major_minor_minor_major_major() {
        use Quality::*;
        let q: Vec<Quality> = chords_in_key(C4, &MINOR_SCALE).iter().map(quality).collect();
        assert_eq!(q, vec![Minor, Diminished, Major, Minor, Minor, Major, Major]);
    }

    #[test]
    fn minor_sequence_also_holds_in_every_key() {
        use Quality::*;
        let expected = vec![Minor, Diminished, Major, Minor, Minor, Major, Major];
        for root in 48..60 {
            let q: Vec<Quality> = chords_in_key(root, &MINOR_SCALE).iter().map(quality).collect();
            assert_eq!(q, expected, "broken for root MIDI {root}");
        }
    }

    #[test]
    fn quality_reads_the_gaps_not_the_pitches() {
        assert_eq!(quality(&[60, 64, 67]), Quality::Major);
        assert_eq!(quality(&[60, 63, 67]), Quality::Minor);
        assert_eq!(quality(&[60, 63, 66]), Quality::Diminished);
        assert_eq!(quality(&[60, 64, 68]), Quality::Augmented);
    }

    #[test]
    fn midi_to_frequency_is_anchored_at_a4_440() {
        assert_eq!(midi_to_freq(69), 440.0);
        assert_eq!(midi_to_freq(81), 880.0); // an octave up, double the hertz
        assert_eq!(midi_to_freq(57), 220.0); // an octave down, half
        assert!((midi_to_freq(60) - 261.6256).abs() < 0.001); // middle C
    }

    #[test]
    fn chords_of_c_major_are_named_as_sheet_music_names_them() {
        let names: Vec<String> = chords_in_key(C4, &MAJOR_SCALE).iter().map(chord_name).collect();
        assert_eq!(names, ["C", "Dm", "Em", "F", "G", "Am", "Bdim"]);
    }

    #[test]
    fn a_minor_is_the_white_keys_starting_from_a() {
        let names: Vec<String> = chords_in_key(57, &MINOR_SCALE).iter().map(chord_name).collect();
        assert_eq!(names, ["Am", "Bdim", "C", "Dm", "Em", "F", "G"]);
    }

    #[test]
    fn roman_numerals_carry_the_quality_in_their_casing() {
        let n: Vec<String> = chords_in_key(C4, &MAJOR_SCALE)
            .iter()
            .enumerate()
            .map(|(d, c)| roman_numeral(c, d))
            .collect();
        assert_eq!(n, ["I", "ii", "iii", "IV", "V", "vi", "vii°"]);
    }

    #[test]
    fn arp_patterns_walk_a_triad_in_the_right_order() {
        assert_eq!(arp_sequence(ArpPattern::Up, 3), vec![0, 1, 2]);
        assert_eq!(arp_sequence(ArpPattern::Down, 3), vec![2, 1, 0]);
        // 0 1 2 1, never 0 1 2 2 1 0.
        assert_eq!(arp_sequence(ArpPattern::UpDown, 3), vec![0, 1, 2, 1]);
    }

    #[test]
    fn up_down_turns_around_correctly_on_a_four_note_chord() {
        assert_eq!(arp_sequence(ArpPattern::UpDown, 4), vec![0, 1, 2, 3, 2, 1]);
    }

    #[test]
    fn arp_patterns_degrade_safely_on_short_chords() {
        assert_eq!(arp_sequence(ArpPattern::UpDown, 2), vec![0, 1]);
        assert_eq!(arp_sequence(ArpPattern::UpDown, 1), vec![0]);
        assert_eq!(arp_sequence(ArpPattern::Down, 1), vec![0]);
        assert!(arp_sequence(ArpPattern::Up, 0).is_empty());
        assert!(arp_sequence(ArpPattern::UpDown, 0).is_empty());
    }

    #[test]
    fn arp_index_agrees_with_the_readable_definition() {
        for pattern in [ArpPattern::Up, ArpPattern::Down, ArpPattern::UpDown] {
            for n in 1..=6usize {
                let seq = arp_sequence(pattern, n);
                for step in 0..seq.len() * 3 {
                    assert_eq!(
                        arp_index(pattern, n, step),
                        seq[step % seq.len()],
                        "{pattern:?} n={n} step={step}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_chord_keeps_its_pitches_and_caps_at_four() {
        let c = Chord::new(&[60, 64, 67]);
        assert_eq!(c.as_slice(), &[60, 64, 67]);
        assert_eq!(c.len(), 3);
        assert_eq!(Chord::new(&[1, 2, 3, 4, 5]).as_slice(), &[1, 2, 3, 4]);
        assert!(Chord::new(&[]).is_empty());
    }

    #[test]
    fn every_arp_step_indexes_a_note_that_exists() {
        for pattern in [ArpPattern::Up, ArpPattern::Down, ArpPattern::UpDown] {
            for n in 1..=5usize {
                let seq = arp_sequence(pattern, n);
                assert!(seq.iter().all(|&i| i < n), "{pattern:?}/{n} out of range: {seq:?}");
                let unique: std::collections::HashSet<_> = seq.iter().collect();
                assert_eq!(unique.len(), n, "{pattern:?}/{n} never plays every note: {seq:?}");
            }
        }
    }
}

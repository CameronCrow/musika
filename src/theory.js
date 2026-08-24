/*
 * theory.js - the music theory layer of Heptad.
 *
 * PURE FUNCTIONS ONLY. No audio, no DOM, no state. Everything in here is
 * arithmetic on numbers, which is why it's the one part of this project that
 * can be unit-tested (see tests/theory.test.js).
 *
 * The whole file rests on one idea: a pitch is a number.
 *
 *   MIDI note numbers count semitones (the smallest step on a piano - one key
 *   to the very next key, black or white). Middle C is 60. C# is 61. D is 62.
 *   Twelve semitones later, 72, is C again - one octave up, and it sounds like
 *   "the same note, higher". So pitch arithmetic is just integer arithmetic,
 *   and "the same note in another octave" is just +/- 12.
 *
 * Because it's all arithmetic, nothing here hardcodes "C major has these
 * chords". We derive the chords from the interval pattern, which means the
 * same code will produce correct chords for any key and any mode (milestone 3)
 * without a single new line of theory.
 */

/*
 * A SCALE is a pattern of semitone offsets from a root note. Out of the 12
 * available semitones in an octave, a scale picks 7 and calls them "in key".
 * These 7 are the only notes the instrument will ever play - that's the whole
 * trick behind "you can't play a wrong note".
 *
 * Major scale: 0, 2, 4, 5, 7, 9, 11
 *                \/ \/ \/ \/ \/ \/ \/   gaps of 2,2,1,2,2,2,1 semitones
 *
 * Those two 1-semitone gaps (between offsets 4->5, and 11->12 as it wraps into
 * the next octave) are what makes a major scale sound major rather than just
 * "seven notes". Change the gap pattern and you get a different mode; that's
 * all a mode is.
 */
const MAJOR_SCALE = [0, 2, 4, 5, 7, 9, 11];

/*
 * Note names, indexed by semitone offset within an octave. We use sharps
 * throughout - F# and Gb are the same key on a piano, and picking one keeps
 * this simple. A engraver writing sheet music would care; an instrument you
 * play does not.
 */
const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];

/**
 * The note at a given position in the scale, as a MIDI number.
 *
 * `index` is a scale position, NOT a semitone count: 0 is the first note of
 * the scale, 1 the second, and so on. Crucially the index is allowed to run
 * off either end of the pattern, and when it does we wrap around to the start
 * of the pattern and shift by an octave (+12 semitones per wrap). Index 7 is
 * therefore the same letter as index 0, one octave higher.
 *
 * This wrapping is the part that's easy to get wrong, and getting it wrong is
 * exactly what breaks the seventh chord (see chordsInKey below).
 *
 * @param {number} rootMidi - MIDI number of the key's root, e.g. 60 for C4
 * @param {number[]} pattern - scale pattern, e.g. MAJOR_SCALE
 * @param {number} index - scale position; may be negative or >= pattern.length
 * @returns {number} MIDI note number
 */
function scaleNote(rootMidi, pattern, index) {
  // How many times did we wrap past the end (or before the start) of the
  // pattern? Math.floor rounds toward negative infinity, so index -1 gives
  // octave -1, which is what we want for notes below the root.
  const octave = Math.floor(index / pattern.length);

  // Position within the pattern. The % operator in JS keeps the sign of the
  // left operand (-1 % 7 === -1), so the extra + length and second % force the
  // result into the 0..6 range we can actually index with.
  const step = ((index % pattern.length) + pattern.length) % pattern.length;

  return rootMidi + pattern[step] + 12 * octave;
}

/**
 * Build a triad (a three-note chord) on a scale degree.
 *
 * A triad is built by stacking thirds: take a scale note, skip one, take the
 * next, skip one, take the next. In scale-position terms that's index n, n+2,
 * n+4. "Skip one" is the entire rule - the chords of a key are just
 * every-other-note of that key's scale.
 *
 * `degree` is 0-based here because it's a JS array index. Musicians count
 * degrees from 1 (the "I chord", the "V chord"), so degree 0 is the I chord.
 *
 * Because scaleNote wraps and adds an octave, the higher degrees automatically
 * borrow their upper notes from the octave above instead of folding back down
 * into a wrong-sounding cluster.
 *
 * @returns {number[]} three MIDI numbers, lowest first
 */
function triad(rootMidi, pattern, degree) {
  return [degree, degree + 2, degree + 4]
    .map((index) => scaleNote(rootMidi, pattern, index));
}

/**
 * Name the "quality" of a triad - the flavour of the chord.
 *
 * We look only at the two gaps between the three notes, measured in semitones.
 * Those two gaps ARE the identity of the chord; the actual pitches don't
 * matter, which is why quality works out the same in every key:
 *
 *   4 then 3  -> major       bright, resolved      (C E G)
 *   3 then 4  -> minor       darker, sadder        (D F A)
 *   3 then 3  -> diminished  tense, unresolved     (B D F)
 *   4 then 4  -> augmented   uneasy; never occurs in a major scale
 *
 * A gap of 3 is called a minor third, a gap of 4 a major third. Stack one of
 * each and you get major or minor depending on the order; stack two of the
 * same size and you get one of the unstable ones.
 */
function quality(notes) {
  const lower = notes[1] - notes[0];
  const upper = notes[2] - notes[1];
  if (lower === 4 && upper === 3) return 'major';
  if (lower === 3 && upper === 4) return 'minor';
  if (lower === 3 && upper === 3) return 'diminished';
  if (lower === 4 && upper === 4) return 'augmented';
  return 'unknown';
}

/**
 * Every triad in the key, in order - the seven chords the instrument plays.
 *
 * Run this on a major scale and the qualities come out, in order:
 *
 *   major, minor, minor, major, major, minor, diminished
 *
 * Nobody chose that. It falls out of the uneven gaps in the scale pattern:
 * because the steps aren't all the same size, stacking every-other-note lands
 * on a 4-then-3 gap sometimes and a 3-then-4 gap other times. It's the reason
 * a happy major key contains sad-sounding chords at all, and it's the single
 * best sanity check on the wrap logic - if chord seven isn't diminished, then
 * scaleNote is broken.
 */
function chordsInKey(rootMidi, pattern) {
  return pattern.map((_, degree) => triad(rootMidi, pattern, degree));
}

/**
 * MIDI note number -> frequency in hertz, which is what an oscillator wants.
 *
 * Anchor: MIDI 69 is A4, tuned to 440 Hz by convention. Going up 12 semitones
 * doubles the frequency (that is physically what an octave is), so one single
 * semitone multiplies frequency by the twelfth root of two. Hence 2 ** (n/12).
 */
function midiToFreq(midi) {
  return 440 * 2 ** ((midi - 69) / 12);
}

/** Letter name of a MIDI note, ignoring which octave it lands in. */
function noteName(midi) {
  return NOTE_NAMES[((midi % 12) + 12) % 12];
}

/**
 * How a chord is written: root letter plus a suffix for its quality.
 * C major is "C", D minor is "Dm", B diminished is "Bdim".
 */
function chordName(notes) {
  const suffix = { major: '', minor: 'm', diminished: 'dim', augmented: 'aug' };
  return noteName(notes[0]) + (suffix[quality(notes)] ?? '?');
}

/**
 * Roman numeral for a chord, the way theory books label them: uppercase for
 * major, lowercase for minor, with a small circle for diminished. In C major:
 *
 *   I  ii  iii  IV  V  vi  vii*
 *
 * Same information as chordName, but key-independent - the V chord is the
 * fifth chord of the key whether you're in C or in F#, so these are the labels
 * you'd actually learn patterns from.
 */
function romanNumeral(notes, degree) {
  const numerals = ['I', 'II', 'III', 'IV', 'V', 'VI', 'VII'];
  const q = quality(notes);
  const isBig = q === 'major' || q === 'augmented';
  const numeral = isBig ? numerals[degree] : numerals[degree].toLowerCase();
  const mark = q === 'diminished' ? '°' : q === 'augmented' ? '+' : '';
  return numeral + mark;
}

/*
 * Two worlds, one file. In the browser this is loaded as a plain <script>, so
 * everything above is already a global and nothing needs exporting - which is
 * also why index.html works when opened straight off disk with no server and
 * no build step. Under Node (the test runner) top-level names are instead
 * module-scoped, so we hand them over explicitly. `module` doesn't exist in a
 * browser, hence the guard.
 */
if (typeof module !== 'undefined') {
  module.exports = {
    MAJOR_SCALE, NOTE_NAMES, scaleNote, triad, quality,
    chordsInKey, midiToFreq, noteName, chordName, romanNumeral,
  };
}

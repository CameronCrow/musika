/*
 * Unit tests for the theory layer. Run them with:
 *
 *   node --test tests/
 *
 * No test framework, no dependencies - `node --test` and `node:assert` ship
 * with Node itself.
 */

const { test } = require('node:test');
const assert = require('node:assert/strict');

const {
  MAJOR_SCALE, scaleNote, triad, quality,
  chordsInKey, midiToFreq, noteName, chordName, romanNumeral,
} = require('../src/theory.js');

const C4 = 60; // middle C

test('the major scale in C is the white keys, C to B', () => {
  const notes = MAJOR_SCALE.map((_, i) => scaleNote(C4, MAJOR_SCALE, i));
  //            C   D   E   F   G   A   B
  assert.deepEqual(notes, [60, 62, 64, 65, 67, 69, 71]);
});

test('scale positions past the end wrap and gain an octave', () => {
  // Position 7 is the same letter as position 0, twelve semitones higher.
  assert.equal(scaleNote(C4, MAJOR_SCALE, 7), 72); // C5
  assert.equal(scaleNote(C4, MAJOR_SCALE, 8), 74); // D5
  assert.equal(scaleNote(C4, MAJOR_SCALE, 13), 83); // B5
  assert.equal(scaleNote(C4, MAJOR_SCALE, 14), 84); // C6, two octaves up
});

test('scale positions below zero wrap downwards', () => {
  // Negative positions are not used by the instrument today, but inversions
  // and octave shift (milestone 5) will reach below the root, and getting the
  // sign handling wrong there is silent and nasty.
  assert.equal(scaleNote(C4, MAJOR_SCALE, -1), 59); // B3, just under middle C
  assert.equal(scaleNote(C4, MAJOR_SCALE, -7), 48); // C3, one octave down
  assert.equal(scaleNote(C4, MAJOR_SCALE, -8), 47); // B2
});

test('triads stack every other scale note', () => {
  assert.deepEqual(triad(C4, MAJOR_SCALE, 0), [60, 64, 67]); // C  E  G
  assert.deepEqual(triad(C4, MAJOR_SCALE, 1), [62, 65, 69]); // D  F  A
  assert.deepEqual(triad(C4, MAJOR_SCALE, 4), [67, 71, 74]); // G  B  D5
});

test('the last triad borrows its upper notes from the octave above', () => {
  // The one that exposes broken wrap logic: B needs D and F from above, not
  // the D and F sitting below it in the same octave.
  assert.deepEqual(triad(C4, MAJOR_SCALE, 6), [71, 74, 77]); // B  D5  F5
});

test('a major key yields major minor minor major major minor diminished', () => {
  // The headline check. This exact sequence is what makes the seven buttons
  // sound like a key rather than seven unrelated chords.
  const qualities = chordsInKey(C4, MAJOR_SCALE).map(quality);
  assert.deepEqual(qualities, [
    'major', 'minor', 'minor', 'major', 'major', 'minor', 'diminished',
  ]);
});

test('that quality sequence holds in every one of the 12 keys', () => {
  // Nothing in the theory layer knows what key it is in, so this must hold
  // everywhere. If it ever fails for one root only, something is hardcoded.
  const expected = [
    'major', 'minor', 'minor', 'major', 'major', 'minor', 'diminished',
  ];
  for (let root = 48; root < 60; root++) {
    const qualities = chordsInKey(root, MAJOR_SCALE).map(quality);
    assert.deepEqual(qualities, expected, `broken for root MIDI ${root}`);
  }
});

test('quality reads the two gaps, not the pitches', () => {
  assert.equal(quality([60, 64, 67]), 'major'); // 4 then 3
  assert.equal(quality([60, 63, 67]), 'minor'); // 3 then 4
  assert.equal(quality([60, 63, 66]), 'diminished'); // 3 then 3
  assert.equal(quality([60, 64, 68]), 'augmented'); // 4 then 4
});

test('MIDI to frequency is anchored at A4 = 440 Hz', () => {
  assert.equal(midiToFreq(69), 440); // A4, the tuning reference
  assert.equal(midiToFreq(81), 880); // A5 - one octave up, double the hertz
  assert.equal(midiToFreq(57), 220); // A3 - one octave down, half the hertz
  assert.ok(Math.abs(midiToFreq(60) - 261.6256) < 0.001); // middle C
});

test('notes are named by semitone within the octave', () => {
  assert.equal(noteName(60), 'C');
  assert.equal(noteName(72), 'C'); // same letter an octave up
  assert.equal(noteName(66), 'F#');
});

test('the seven chords of C major are named the way sheet music names them', () => {
  const names = chordsInKey(C4, MAJOR_SCALE).map(chordName);
  assert.deepEqual(names, ['C', 'Dm', 'Em', 'F', 'G', 'Am', 'Bdim']);
});

test('roman numerals carry the quality in their casing', () => {
  const numerals = chordsInKey(C4, MAJOR_SCALE).map(romanNumeral);
  assert.deepEqual(numerals, ['I', 'ii', 'iii', 'IV', 'V', 'vi', 'vii°']);
});

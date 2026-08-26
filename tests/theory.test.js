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
  MAJOR_SCALE, MINOR_SCALE, scaleNote, triad, quality,
  chordsInKey, midiToFreq, noteName, chordName, romanNumeral, arpSequence,
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

test('a minor key yields minor diminished major minor minor major major', () => {
  // The point of deriving chords instead of tabulating them: swapping one array
  // gives a whole different mode, with no new code in theory.js at all.
  const qualities = chordsInKey(C4, MINOR_SCALE).map(quality);
  assert.deepEqual(qualities, [
    'minor', 'diminished', 'major', 'minor', 'minor', 'major', 'major',
  ]);
});

test('that minor sequence also holds in every one of the 12 keys', () => {
  const expected = [
    'minor', 'diminished', 'major', 'minor', 'minor', 'major', 'major',
  ];
  for (let root = 48; root < 60; root++) {
    assert.deepEqual(
      chordsInKey(root, MINOR_SCALE).map(quality), expected,
      `broken for root MIDI ${root}`
    );
  }
});

test('A minor is the white keys too, starting from A', () => {
  // A minor uses exactly the same notes as C major, started three semitones
  // lower - which is why its chords are the same seven chords in a new order.
  const A3 = 57;
  const names = chordsInKey(A3, MINOR_SCALE).map(chordName);
  assert.deepEqual(names, ['Am', 'Bdim', 'C', 'Dm', 'Em', 'F', 'G']);
});

test('arpeggiator patterns walk a triad in the right order', () => {
  assert.deepEqual(arpSequence('up', 3), [0, 1, 2]);
  assert.deepEqual(arpSequence('down', 3), [2, 1, 0]);

  // Up-down does NOT repeat the endpoints: 0 1 2 1, never 0 1 2 2 1 0.
  // Repeating them makes the turnaround stumble and kills the pulse.
  assert.deepEqual(arpSequence('updown', 3), [0, 1, 2, 1]);
});

test('up-down still turns around correctly on a four-note chord', () => {
  // Matters from milestone 7 onward, when a 7th makes chords four notes long.
  assert.deepEqual(arpSequence('updown', 4), [0, 1, 2, 3, 2, 1]);
});

test('arpeggiator patterns degrade safely on short chords', () => {
  // Nothing to bounce off with fewer than three notes, so up-down is just up.
  assert.deepEqual(arpSequence('updown', 2), [0, 1]);
  assert.deepEqual(arpSequence('updown', 1), [0]);
  assert.deepEqual(arpSequence('down', 1), [0]);
  assert.deepEqual(arpSequence('up', 0), []);
  assert.deepEqual(arpSequence('updown', 0), []);
});

test('an unknown pattern falls back to up rather than producing nothing', () => {
  // A stale value in localStorage must never leave the arpeggiator silent.
  assert.deepEqual(arpSequence('sideways', 3), [0, 1, 2]);
  assert.deepEqual(arpSequence(undefined, 3), [0, 1, 2]);
});

test('every arpeggiator step indexes a note that actually exists', () => {
  // The sequence is used as chord[i] - an out-of-range index would be silence
  // or a crash, and a missing index would be a note you can never hear.
  for (const pattern of ['up', 'down', 'updown']) {
    for (let n = 1; n <= 5; n++) {
      const seq = arpSequence(pattern, n);
      assert.ok(seq.every((i) => Number.isInteger(i) && i >= 0 && i < n),
        `${pattern}/${n} produced an out-of-range index: ${seq}`);
      assert.equal(new Set(seq).size, n,
        `${pattern}/${n} never plays every note: ${seq}`);
    }
  }
});

test('roman numerals carry the quality in their casing', () => {
  const numerals = chordsInKey(C4, MAJOR_SCALE).map(romanNumeral);
  assert.deepEqual(numerals, ['I', 'ii', 'iii', 'IV', 'V', 'vi', 'vii°']);
});

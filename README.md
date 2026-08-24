# Heptad

A seven-button chord organ that runs in a browser. Every button plays a whole
chord, all seven chords belong to the same key, so any button sounds fine after
any other one. There is no wrong note to hit.

**Live: <https://cameroncrow.github.io/musika/>**

Inspired by the idea behind pocket chord synths like the HiChord; no code or
assets shared with it.

```
 I     ii    iii    IV     V     vi    vii°
 C     Dm    Em     F      G     Am    Bdim
```

Hold a pad and the chord rings. Let go and it stops. Hold several at once, with
both hands — it's polyphonic and multi-touch.

Status: **milestone 1 of 5**. C major only, one square-wave voice, no
arpeggiator yet.

## Running it

No build step, no bundler, no dependencies. Two ways:

**Open the file.** Double-click `index.html`. Everything is plain `<script>`
tags, so this works straight off disk with no server.

**Or serve it**, which is what you want if you're going to play it on your phone
over your home wi-fi:

```bash
python -m http.server 8777
```

Then open `http://<your-computer's-LAN-IP>:8777` on the phone.

Playing it at a laptop: number keys **1–7** play the seven chords, held for as
long as the key is down.

## Tests

The theory layer is pure arithmetic with no audio or DOM in it, which makes it
the one part of this project that can be tested automatically. It is, using
Node's built-in test runner — no framework, nothing to install:

```bash
node --test
```

Twelve tests covering scale generation, triad stacking, octave wrapping in both
directions, MIDI→frequency, chord naming, and the quality sequence below.

## How the theory works

All of it is in [`src/theory.js`](src/theory.js), heavily commented. The short
version:

**A pitch is a number.** MIDI note numbers count semitones — the smallest step
on a piano, one key to the very next key. Middle C is 60, C# is 61, D is 62.
Twelve semitones up (72) is C again, an octave higher. So "transpose", "octave
up" and "interval" are all just integer arithmetic.

**A scale is a pattern of offsets.** A major scale is `[0, 2, 4, 5, 7, 9, 11]`
semitones above its root: seven of the twelve available notes, declared to be
"in key". Those seven are the only notes the instrument will ever play, which
is precisely why you can't hit a wrong one.

**A chord is every other note of the scale.** Take a scale position, skip one,
take the next, skip one, take the next — positions `n`, `n+2`, `n+4`. That's a
triad. When a position runs off the end of the pattern it wraps back to the
start and gains 12 semitones, so the upper notes come from the octave above
rather than folding back down into the wrong register.

**Chord quality falls out of the gaps.** Nothing in the code says "the second
chord of a major key is minor". Look at the two gaps between a triad's three
notes: 4-then-3 semitones is major, 3-then-4 is minor, 3-then-3 is diminished.
Because the major scale's steps are uneven (2,2,1,2,2,2,1), stacking
every-other-note lands on different gap pairs at different degrees, and the
seven chords come out:

```
major, minor, minor, major, major, minor, diminished
   I     ii    iii     IV     V     vi     vii°
```

Nobody chose that sequence — it's a consequence of the scale pattern. It's also
the sharpest test of the wrap logic: **if the seventh chord isn't diminished,
the octave wrapping is broken.** That check is in the test suite, run across all
twelve keys.

Because none of this is a lookup table, changing key means changing one number
and changing mode means changing one array. That's milestone 3, and it needs no
new theory code.

**Frequency.** `440 * 2 ** ((midi - 69) / 12)`. MIDI 69 is A4, tuned to 440 Hz
by convention; twelve semitones doubles the frequency, so one semitone
multiplies it by the twelfth root of two.

## Device quirks worth knowing

- **iOS silent switch.** If the physical mute switch on the side of an iPhone is
  flipped on, Safari may play Web Audio silently with no warning whatsoever. If
  the pads light up and nothing comes out, check the switch first — the app
  isn't broken.
- **Audio needs a real tap to start.** Browsers won't let a page make sound
  until you've touched it. Heptad creates its audio engine on your first press,
  so the very first pad you hit may sound a few milliseconds late. Every one
  after is immediate. If audio ever fails to wake, the status line at the top
  says so instead of leaving you guessing.
- **Turn the phone sideways.** Portrait stacks the pads two-wide because seven
  columns on a phone are too narrow to hit. Landscape gives you the row of
  seven, which is the layout you can play with two hands.
- **Zoom and scroll are disabled** on purpose. A page that scrolls when you drag
  across it can't be played.

## Layout

```
index.html          the whole UI: markup and CSS
src/theory.js       music theory — pure functions, no audio, no DOM
src/app.js          the instrument — audio engine and touch handling
tests/theory.test.js
planning/           milestones and progress
```

Three source files, no dependencies, deliberately. A framework here would add a
toolchain, a build step and a node_modules directory to a page whose entire job
is to draw seven rectangles and open an AudioContext; none of that would make
the audio code — the only genuinely tricky part — any simpler to read or debug.

## Deploying

GitHub Pages serves the repository root as-is — there is nothing to build. Push
to `main` and the live site updates a minute or so later.

## Milestones

- [x] **1** — Seven pads, C major, hold-to-sustain, one synth voice
- [ ] **2** — Arpeggiator: on/off, tempo, up / down / up-down
- [ ] **3** — Key and mode selector: all 12 keys, major and minor
- [ ] **4** — Sound shaping: waveform, filter cutoff, attack/release
- [ ] **5** — Modifiers: 7ths, octave shift, inversions

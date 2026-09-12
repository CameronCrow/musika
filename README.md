# Musika

A seven-button chord organ. Every button plays a whole chord, all seven chords
belong to the same key, so any button sounds fine after any other one. There is
no wrong note to hit.

```
 I     ii    iii    IV     V     vi    vii°
 C     Dm    Em     F      G     Am    Bdim
```

Hold a pad and the chord rings. Let go and it stops. Hold several at once.

Inspired by the idea behind pocket chord synths like the HiChord; no code or
assets shared with it.

Musika is a native Windows app, written in Rust. It began as a web page — that
version is **retired**, see [The web build](#the-web-build-retired) at the bottom.

## Install and run

```powershell
powershell -ExecutionPolicy Bypass -File tools\install-native.ps1
```

That builds it, copies `musika.exe` and its icon to `%LOCALAPPDATA%\Musika`, and
puts a shortcut in the Start Menu. The stable path matters: `native/target/` is
gitignored and `cargo clean` wipes it, so a taskbar pin aimed there would break
the first time you cleaned. Re-run the script after changing the code and the
pinned copy updates in place.

**To pin it:** Start → type "Musika" → right-click → *Pin to taskbar*. That last
step is manual and always will be — Windows 10 deliberately removed programmatic
taskbar pinning, so no installer can do it for you.

To just run it from the source tree:

```bash
cd native && cargo run --release
```

Use `--release`. A debug build gives the audio thread far less headroom and can
crackle under full polyphony.

## Playing it

| | |
|---|---|
| **Chords** | `A S D F G H J`, or the number row `1`–`7`. Held for as long as you hold the key. |
| **Mouse** | Click and drag across the pads. |
| **Octave** | `-` and `=`, or the buttons. Range −3 to +1; it starts one octave below middle C. |
| **Key / mode** | All 12 roots, major or minor. |
| **Patch** | Eleven sounds — see [The patches](#the-patches). |

The roman numerals on the pads are the notation musicians actually use, so
"I–V–vi–IV" means press pads 1, 5, 6, 4 — a progression that carries a great many
pop songs. Play each for a slow count of four.

## The sound

A single square wave through a fixed lowpass is a *beeper*. These are the things
that separate that from a synthesiser:

| | why it matters |
|---|---|
| **Two detuned oscillators** | Two saws a few cents apart drift in and out of phase over about a second. That slow beating *is* what "thick" and "warm" are. One oscillator is perfectly static and reads as synthetic instantly. Biggest single win. |
| **A real ADSR** | Not just fade in and fade out. The decay — a dip from the initial peak down to the sustain level — is what makes a note sound *struck* rather than switched on. |
| **A resonant filter that moves** | A fixed lowpass only makes things duller. One that snaps open on the attack and closes over the next few hundred ms is the sound everyone recognises as "a synth". Resonance is a gain bump at the cutoff — a one-pole filter cannot produce it at all. |
| **Stereo** | The three notes of a chord are panned across the field instead of stacked in the middle, using equal-power panning so nothing dips in loudness crossing the centre. |
| **Reverb** | A dry chord happens inside your head; the same chord with a tail happens *somewhere*. Four comb filters make the tail, two allpasses smear it into a wash, and damping rolls the treble off each pass the way a real room absorbs it. |
| **Three filter modes** | The state variable filter computes lowpass, bandpass and highpass simultaneously as a side effect of how it works, so offering all three costs one `match`. Bandpass is hollow and vocal; highpass throws away the fundamental and leaves only air. |
| **A sub-oscillator** | A square an octave below the note. This is where weight comes from — a filtered saw has no bottom of its own, and no amount of lowering the cutoff will give it any. |
| **Vibrato** | A slow pitch wobble. A few cents at 4–6Hz reads as expression; a lot of it reads as seasickness. |

The comb delay lengths deliberately share no common factors. If they did, the
echoes would line up and you would hear a pitch instead of a room.

### The patches

| | |
|---|---|
| `raw` | The voice before any of the above — one square wave, nothing moving. Kept so the difference is audible rather than asserted. |
| `warm` | The default. Detuned saws, a breathing filter, a room. |
| `organ` | Bright and completely static, with a sub for drawbar weight. Holds a chord without asking for attention. |
| `pad` | Slow enough that it arrives rather than starts. |
| `pluck` | Sustain of zero: it decays to nothing while you are still holding it, which is what a plucked string does. Made for arpeggios. |
| `chime` | Short, bright, bell-like. |
| `bell` | A triangle has almost no harmonics of its own, which is what lets a long decay ring clean instead of buzzing. |
| `bass` | Mostly sub, deliberately narrow — low frequencies carry almost no directional information, so spreading them only makes a mix vague. |
| `hollow` | Bandpass. Sounds like it is being sung down a tube. |
| `glass` | Highpass. Air rather than a note; thin alone, lovely over something with bottom. |
| `lo-fi` | Dark, wide and slightly wobbly. |

Each carries an output trim. Left alone they ranged over about 10dB — switching
from `bell` to `bass` nearly tripled the volume — so each is measured and
trimmed towards `warm`. They now sit within 1.8dB of each other.

### Hearing them without launching anything

```bash
cd native && cargo run --release -- --render demo.wav
```

Plays I–V–vi–IV through every patch in turn and writes a WAV (about 68
seconds). A patch is judged by ear; no test can do it for you.

## Latency, measured

Measured on the same machine and the same headphones as the retired web build.

| | Web (Chrome) | Native (cpal) |
|---|---|---|
| JavaScript / UI thread | 0.1 ms | — |
| Audio buffer | 10 ms | **5.8 ms** (256 frames) |
| OS output path | 40 ms | driver-dependent, not self-inflicted |
| Attack ramp | 6 ms | 4 ms |
| **What the app chose** | **~56 ms** | **~10 ms** |

The browser's `latencyHint: 'interactive'` is a polite suggestion; cpal's
`BufferSize::Fixed(256)` is a number the device either accepts or refuses. That
is the entire reason this is a native app.

Read the middle row honestly: the OS still adds its own path in both cases. The
native figure is the part *this program* controls, not a full round-trip
measurement.

To see what your device actually gave you:

```bash
cd native && cargo run -- --probe
```

A debug build, deliberately — release builds are compiled as a GUI program with
no console to print to, which is what stops a terminal window flashing up when
you launch it. `--render` still writes its WAV from a release build; it just
prints nothing.

## Tests

```bash
cd native && cargo test
```

50 tests, no framework beyond the one built into Cargo.

**18 cover the theory** — scale generation, triad stacking, octave wrapping in
both directions, MIDI→frequency, chord naming, arpeggiator patterns, and the
quality sequences below, in both major and minor across all twelve keys.

**32 cover the DSP**, which is easy to get silently wrong: that detuned
oscillators actually beat (with `raw` as a control that they do *not*), that
panning holds power constant across the field, that a hard filter sweep at high
resonance stays finite, that the reverb tail decays rather than running away,
that its two channels differ at all, that every patch is audible and never clips
at full 21-voice polyphony, and that no two patches render identically.

Several go further than "the output changed", which would pass for any change at
all. A single-bin DFT checks that the sub-oscillator really does put energy an
octave below the note, and that a highpass really does throw the fundamental
away — claims that are otherwise easy to believe and wrong.

Two were written after the bug they describe:

- A voice released before it produced a single sample was reaped instantly, so
  a tap shorter than one audio buffer was **silent**.
- A zero-sustain patch parked *between* two thresholds forever: decay stopped
  within 0.001 of the sustain level, but a voice is only retired below 0.0005.
  `pluck` would have gone silent and stayed alive, burning a slot for as long as
  you held it. The envelope now snaps to its target instead of approaching it.

## How the theory works

All of it is in [`native/src/theory.rs`](native/src/theory.rs), heavily
commented. The short version:

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
and changing mode means changing one array. Minor's seven chords come out
`i ii° III iv v VI VII` without a line of new theory code.

**Frequency.** `440 * 2 ** ((midi - 69) / 12)`. MIDI 69 is A4, tuned to 440 Hz
by convention; twelve semitones doubles the frequency, so one semitone
multiplies it by the twelfth root of two.

## Layout

```
native/
  build.rs            embeds the Windows icon into the exe
  src/theory.rs       the music - pure integer arithmetic, no audio, no UI
  src/voice.rs        one note: oscillators, envelopes, filter, panning
  src/reverb.rs       the room, built out of combs and allpasses
  src/engine.rs       the audio thread and the mix; one clock, no scheduler
  src/main.rs         the instrument - window, pads, keyboard
tools/
  install-native.ps1  build + install + shortcut, for pinning
  make-icons.py       generates every icon from the pad hue formula
icons/                generated, committed
planning/             milestones and progress
```

Five source files, three dependencies (`eframe`, `cpal`, and `winresource` at
build time only).

### The one rule of the audio thread

`fill` in [`engine.rs`](native/src/engine.rs) runs on a real-time thread owned by
the OS. If it takes too long you don't get a slow instrument, you get a click — a
hole in the sound. So it never allocates, never locks, never blocks. Notes reach
it through a queue it can drain without waiting, which is why the UI sends
messages rather than reaching in and pushing a voice.

This is also why there is no look-ahead scheduler. In a browser you *schedule*
against a clock and JavaScript timers drift, so the web build needed two clocks
carefully kept apart. Here the sound card asks for the next N samples, and the
count of samples written **is** the clock. Nothing can drift from it.

## The web build (retired)

The original lives on in `index.html`, `src/*.js` and `sw.js`, still deployed at
<https://cameroncrow.github.io/musika/>. It is no longer maintained.

It still has two things the native build doesn't: a **looper** (record, overdub,
loop) and an **arpeggiator**. Both are the obvious next things to port —
the looper in particular gets simpler on a sample clock, since its look-ahead
scheduler stops being necessary at all.

It also still has the old square-wave voice, so it is not a fair comparison for
how Musika sounds now.

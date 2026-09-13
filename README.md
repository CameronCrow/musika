# Musika

<img src="icons/musika.png" width="112" align="right" alt="Musika icon">

A seven-button chord organ. Every button plays a whole chord, all seven chords
belong to the same key, so any button sounds fine after any other one. There is
no wrong note to hit.

```
 I     ii    iii    IV     V     vi    vii°
 C     Dm    Em     F      G     Am    Bdim
```

Hold a pad and the chord rings; let go and it stops. Record what you play into a
loop and stack more on top. Switch the arpeggiator on and held chords turn into
patterns.

Inspired by the idea behind pocket chord synths like the HiChord; no code or
assets shared with it.

Musika is a native Windows app, written in Rust. It began as a web page — that
version is **retired**, see [The web build](#the-web-build-retired) at the bottom.

## Install and run

```powershell
powershell -ExecutionPolicy Bypass -File tools\install-native.ps1
```

That builds it, copies `musika.exe` and its icon to `%LOCALAPPDATA%\Musika`, and
puts shortcuts in the Start Menu and on your Desktop. The stable path matters:
`native/target/` is gitignored and `cargo clean` wipes it, so a shortcut aimed
there would break the first time you cleaned. Re-run the script after changing
the code and every shortcut picks up the new build.

The Desktop is located by asking Windows rather than assuming
`%USERPROFILE%\Desktop` — with OneDrive backup on, it lives inside the OneDrive
folder, and a hardcoded path would silently write the shortcut somewhere you
never look.

**Pinning is manual.** Windows does not let a program pin itself: the taskbar
verb is gone from the shell API entirely, and on Windows 11 "Pin to Start" is
listed but refused with *Access denied* when a script invokes it. To pin: Start →
type "Musika" → right-click → *Pin to taskbar* or *Pin to Start*.

To just run it from the source tree:

```bash
cd native && cargo run --release
```

Use `--release`. A debug build gives the audio thread far less headroom and can
crackle under full polyphony.

## Playing it

| | |
|---|---|
| **Chords** | `A S D F G H J`, or the number row `1`–`7`, for as long as you hold the key. Or click and drag across the pads, or use a touchscreen — every finger is its own chord. |
| **Record** | `space` — see [Layering with the looper](#layering-with-the-looper). |
| **Play / stop** | `esc` |
| **Undo layer** | `backspace` — takes back the newest layer and leaves the rest playing. |
| **Clear all** | Unbound on purpose. It wipes every layer at once, so it shouldn't be one stray keystroke away. |
| **Loop length / tempo** | 1, 2, 4 or 8 bars, 40–240 bpm. Set before you record; fixed while a loop exists. |
| **Arpeggiator** | `q` toggles it; the pattern sits beside it. |
| **Octave** | `-` and `=`. Range −3 to +1; it starts one octave below middle C. |
| **Key / mode** | All 12 roots, major or minor. |
| **Patch** | Eleven sounds — see [The patches](#the-patches). |

Every key can be changed — see [Your keys](#your-keys) — and everything you set is
remembered between launches.

The roman numerals on the pads are the notation musicians actually use, so
"I–V–vi–IV" means press pads 1, 5, 6, 4 — a progression that carries a great many
pop songs. Play each for a slow count of four.

Touch support is written against egui's touch events but has not yet been tried
on touch hardware.

## Layering with the looper

You don't need to have used a looper before. The line along the top always says
what to press next.

1. **Pick a length and a tempo.** 4 bars at 120 bpm is the default — room for a
   four-chord progression, one chord a bar.
2. **Press `space`, then play.** Recording starts on your first chord, not the
   button, so there's no dead air while you get your hands ready. A quiet click
   counts the beats and the bar under the pads fills up in red. When the bars are
   up it **stops by itself** and starts looping. (Press `space` early to finish at
   the end of the bar you're in.)
3. **Change the sound, maybe switch the arp on, and press `space` again.** Play
   along for one time round the loop. That becomes **layer 2**, and it stops
   recording by itself when the loop comes back to where you started.
4. **Keep going,** up to eight layers.

Every layer appears as a button above the controls — `2  pluck · arp`:

- **Each layer keeps its own sound and arp setting.** A warm pad, then a pluck
  arpeggio over it, then a bass line: changing the sound or the arp only affects
  what you play next.
- **Click a layer to mute it**, click again to bring it back. **×** deletes it.
- **`backspace` undoes the newest layer** — including one you're halfway through
  recording — and leaves everything under it playing.

Two things make it hard to play out of time:

- **The loop is a whole number of bars.** A pedal whose loop is "however long you
  held the button" needs the button hit on the exact beat, and a loop closed a
  little late hiccups on every repeat, under every layer you ever add.
- **Notes snap to the nearest eighth note,** starts and ends both. A chord a touch
  early or late loops in time anyway, and an arpeggiated layer lines up with a
  block-chord one because they share the same grid.

Tempo and length are locked while a loop exists, because its notes sit on that
grid. Clear the loop to change them.

**What is recorded is not audio.** Each event is "these pitches started this many
samples into the loop and were held this long, with this sound", and playback
performs it again. That is a few bytes a chord rather than megabytes, it never
degrades however many layers you stack, and deleting a layer is deleting its
events.

**Pitches, not chord numbers.** What you played was "chord 4 *of C major*", and
the key is half of that — so a loop stays put when you change key to play over it,
and a layer played in A minor keeps its own key.

A new layer is heard from the next time round, not in the pass you played it in.
It joins the loop at the wrap, the one moment the playback position resets anyway,
so folding it in can never skip or double an event. A chord held over the end of
the loop is cut at the boundary rather than spilling into the next cycle.

### Why it is exact

The web build needed a look-ahead scheduler: a JavaScript timer waking every 25ms
to queue notes 120ms ahead of the audio clock, plus special handling so a
throttled background tab didn't dump a burst of missed notes on its return. None
of that exists here. The audio thread advances the looper one sample at a time,
and an event fires on the exact sample it was recorded on. The tests check it to
the sample.

## The arpeggiator

Hold a chord with the arp on and its notes take turns — one per eighth note at the
tempo you set — for as long as you hold it. **up** climbs, **down** descends,
**up-down** bounces.

Three notes at once is a texture; the same three notes in a row is a *pattern*,
and a pattern has rhythm. That is what makes the instrument sound like finished
music rather than someone leaning on an organ. The harmony doesn't change at all.

- The first note lands the instant you press, not up to a step late. A second
  chord pressed while the first is going joins the same grid, so the two stay in
  time.
- Up-down plays `0 1 2 1`, never `0 1 2 2 1 0`. Repeating the endpoints makes the
  turnaround stumble — you hear a limp instead of a pulse.
- At 120 bpm and 48kHz a step is exactly 12,000 samples, every time.
- Turning the arp on or off lets go of anything held, because a ringing block
  chord can't be turned into an arpeggio halfway through a note. Layers already
  recorded don't change: each plays the way it was recorded.

## Your keys

Click **rebind keys**, click a pad or a control, then press the key you want.
**Esc** finishes. Every control shows the key it is on.

Taking a key from another action leaves that one without it, rather than binding
one key to two things. Rebinding *replaces* an action's keys instead of adding to
them — "the key you press is the key for this" is behaviour you can predict
without being told.

If the window loses focus, held keys are let go. Windows doesn't deliver key-up
events to a window that isn't focused, so a chord held when you Alt-Tab away
would otherwise ring forever.

## Settings

Key, mode, octave, patch, tempo, loop length, the arpeggiator, and every key
binding are saved to
`%APPDATA%\Musika\settings.txt`: plain `name = value` lines, safe to edit in
Notepad, and deleting the file resets everything.

Loading is forgiving one value at a time. A garbled line or an out-of-range number
falls back to the default for that value only, because a corrupt settings file
must never be the reason the instrument won't open.

## The sound

A single square wave through a fixed lowpass is a *beeper*. These are the things
that separate that from a synthesiser:

| | why it matters |
|---|---|
| **Two detuned oscillators** | Two saws a few cents apart drift in and out of phase over about a second. That slow beating *is* what "thick" and "warm" are. One oscillator is perfectly static and reads as synthetic instantly. Biggest single win. |
| **A real ADSR** | Not just fade in and fade out. The decay — a dip from the initial peak down to the sustain level — is what makes a note sound *struck* rather than switched on. |
| **A resonant filter that moves** | A fixed lowpass only makes things duller. One that snaps open on the attack and closes over the next few hundred ms is the sound everyone recognises as "a synth". Resonance is a gain bump at the cutoff — a one-pole filter cannot produce it at all. |
| **Stereo** | The notes of a chord are panned across the field instead of stacked in the middle, using equal-power panning so nothing dips in loudness crossing the centre. |
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
| `pluck` | Sustain of zero: it decays to nothing while you are still holding it, which is what a plucked string does. Made for the arpeggiator. |
| `chime` | Short, bright, bell-like. |
| `bell` | A triangle has almost no harmonics of its own, which is what lets a long decay ring clean instead of buzzing. |
| `bass` | Mostly sub, deliberately narrow — low frequencies carry almost no directional information, so spreading them only makes a mix vague. |
| `hollow` | Bandpass. Sounds like it is being sung down a tube. |
| `glass` | Highpass. Air rather than a note; thin alone, lovely over something with bottom. |
| `lo-fi` | Dark, wide and slightly wobbly. |

Each carries an output trim. Left alone they ranged over about 10dB — switching
from `bell` to `bass` nearly tripled the volume — so each is measured and trimmed
towards `warm`. They now sit within 1.8dB of each other.

### Hearing them without launching anything

```bash
cd native && cargo run --release -- --render demo.wav
```

Plays I–V–vi–IV through every patch in turn and writes a WAV (about 68 seconds).
A patch is judged by ear; no test can do it for you.

## The icon

A rounded aluminium tile, a dark pocket routed into it, and the pads standing in
the pocket in their hues — V lit, as if it is being played. It is drawn by
[`tools/make-icons.py`](tools/make-icons.py), standard library only.

The first icon was three flat bars on a black square. Windows drew it faithfully
and it still read as a test card, because an icon needs a *silhouette* — a shape
with an edge that separates from whatever taskbar or wallpaper is behind it. The
rounded tile with transparent corners is that silhouette.

- **Small sizes are simplified, not shrunk.** Seven keys in a 16px tile are a pixel
  and a half each and grey into mush, so 16–32px draw three keys and 40–64px draw
  five.
- **Sizes below 256px are stored as classic bitmaps.** A PNG is allowed inside an
  `.ico`, but parts of the Windows shell draw small PNG entries badly or fall back
  to a generic icon, so only the 256px image is a PNG.
- **Edges are signed distance fields.** Each pixel measures its distance to a
  shape's edge, and coverage is that distance clamped over one pixel — smooth
  anti-aliasing at any size without supersampling.
- **One source.** `build.rs` embeds `icons/musika.ico` into the exe, and the window
  icon is `icons/musika-64.rgba` pulled in with `include_bytes!` — so the title
  bar, Alt-Tab and taskbar can't disagree with the Desktop and Start Menu.

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

107 tests, no framework beyond the one built into Cargo.

| | |
|---|---|
| **20 — theory** | Scale generation, triad stacking, octave wrapping in both directions, MIDI→frequency, chord naming, arpeggiator patterns, and the quality sequences below in major and minor across all twelve keys. |
| **19 — voice and reverb** | That detuned oscillators actually beat (with `raw` as a control that they do *not*), that panning holds power constant, that a hard filter sweep at high resonance stays finite, that the reverb tail decays rather than running away, that every patch is audible and distinct. |
| **23 — looper** | To the exact sample: recording stops itself after its bars, an early press finishes the bar, notes snap to eighths, a layer is one pass heard from the next time round, undo takes back only the newest layer (or the half-recorded one), deleting a middle layer keeps the ones above it, mute, the eight-layer limit, the click's accents — and recording never grows past its preallocated storage. |
| **10 — arpeggiator** | The first note on the press, a second chord joining the first one's grid, each pattern's order, a looped chord playing only inside its span, bad tempos clamped. |
| **23 — engine** | End to end through the audio thread: a recorded loop coming back round on the exact sample, its pad lighting up, a key change not cutting the loop off, each layer keeping its own sound and arp setting, the click only while recording, the tempo locked under a loop, full polyphony never clipping. |
| **5 — settings** | A round trip, and a file where every line is broken differently still loading the one good value. |
| **7 — controls** | Default keys surviving being saved by name, rebinding stealing a key, input ids never colliding with the engine's own. |

Several go further than "the output changed", which would pass for any change at
all. A single-bin DFT checks that the sub-oscillator really does put energy an
octave below the note, and that a highpass really does throw the fundamental away.

Some were written after the bug they describe:

- A voice released before it produced a single sample was reaped instantly, so a
  tap shorter than one audio buffer was **silent**.
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
  src/looper.rs       record / overdub / loop - a state machine over samples
  src/arp.rs          the arpeggiator, on the same sample clock
  src/engine.rs       the audio thread: all of the above, mixed
  src/settings.rs     what is remembered between launches
  src/main.rs         window, pads, controls, keys, touch
tools/
  install-native.ps1  build + install + Start Menu and Desktop shortcuts
  make-icons.py       draws every icon
icons/                generated, committed
planning/             milestones and progress
```

Eight source files, three dependencies: `eframe`, `cpal`, and `winresource` at
build time only.

The looper and the arpeggiator know nothing about sound. Each is a state machine
over sample numbers that decides *what should start on which sample* and hands
that to the engine — which is exactly what lets their tests pin timing to the
sample without an audio device.

### The one rule of the audio thread

`fill` in [`engine.rs`](native/src/engine.rs) runs on a real-time thread owned by
the OS. If it takes too long you don't get a slow instrument, you get a click — a
hole in the sound. So it never allocates, never locks, never blocks:

- every list it keeps is allocated up front and never grown past that capacity,
  which the looper and arpeggiator tests check;
- chords cross from the UI as a fixed-size `Chord`, not a `Vec`, because a `Vec`
  would be freed on the audio thread when the message is dropped;
- the overdub merge uses `sort_unstable`, which sorts in place — the stable sort
  allocates a scratch buffer;
- what comes back to the UI — loop state, position, which pads are lit — is a
  handful of atomics, so neither side ever waits for the other.

## The web build (retired)

The original lives on in `index.html`, `src/*.js` and `sw.js`, still deployed at
<https://cameroncrow.github.io/musika/>. It is no longer maintained.

Everything it did, Musika now does natively — the looper, the arpeggiator,
rebindable keys, remembered settings, multi-touch — on a sample-exact clock, and
with a voice the web build never had.

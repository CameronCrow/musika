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

Status: **milestone 5 of 7**. One square-wave voice; no sound shaping or
chord modifiers yet.

## The native build

There are now two Heptads. The web one is the whole instrument; the native one
under [`native/`](native/) is the same instrument on a real audio backend,
because the browser has a latency floor that no amount of JavaScript gets under.

```bash
cd native && cargo run --release
```

`cargo run --release` matters — a debug build gives the audio thread far less
headroom and can crackle. `cargo test` runs 26 tests. `cargo run -- --probe`
opens the audio device, prints what it actually got, and exits:

```
device      Headphones (Raycon Everyday Earbuds Classic Stereo)
sample rate 44100 Hz
buffer      256 frames (5.80 ms)
status      audio stream running
```

That 5.80ms is the number the browser would never give up. See
[Latency, measured](#latency-measured) below.

### Putting it on the taskbar

```powershell
powershell -ExecutionPolicy Bypass -File tools\install-native.ps1
```

Builds it, copies the exe and icon to `%LOCALAPPDATA%\Heptad`, and puts a
shortcut in the Start Menu. That stable path matters: `native/target/` is
gitignored and `cargo clean` wipes it, so a pin aimed there breaks the first
time you clean. Re-run the script after changing the native code and the pinned
copy updates in place.

Windows 10 deliberately removed programmatic taskbar pinning, so the last step
is manual and always will be: **Start → type "Heptad" → right-click → Pin to
taskbar.**

### The voice

Four patches: **raw**, **warm** (the default), **chime**, **lo-fi**. `raw` is
what the instrument sounded like before it had a voice worth the name - one
square wave, a fixed filter, nothing moving - and it is kept so the difference is
audible rather than asserted.

```bash
cargo run --release -- --render demo.wav
```

writes all four playing I–V–vi–IV back to back, so you can judge them by ear
without launching anything. See [The sound](#the-sound) below.

**What it has so far:** seven pads, all 12 keys, major/minor, octave, mouse and
keyboard, hold-to-sustain, four patches. **Not yet ported:** the looper, the
arpeggiator, multi-touch, and the aluminium styling. The web build remains the
complete one — and still has the old square-wave voice.

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

## Installing it as a desktop app

Heptad is a progressive web app, so it installs from the live site with no
download and no Rust:

- **Windows / macOS / Linux (Chrome or Edge)** — open
  <https://cameroncrow.github.io/musika/> and click the install icon in the
  address bar, or menu -> *Cast, save and share* -> *Install page as app*. You
  get a Start Menu / Launchpad entry and its own window with no browser chrome.
- **iPhone / iPad (Safari)** — Share -> *Add to Home Screen*. It gets the app
  icon and launches full-screen with no Safari UI.
- **Android (Chrome)** — menu -> *Add to Home screen*.

Once installed it **works with no network at all**. `sw.js` caches the eight
files the instrument needs on first visit, so it opens instantly and plays on a
plane. The one tradeoff: after a deploy you're one launch behind — the update
downloads in the background and you get it the next time you open the app.

To uninstall, it's the same as any app: right-click the Start Menu entry, or
long-press the home screen icon.

## Playing it with a keyboard

Every pad answers to two keys, and both are printed on its face:

```
 I     ii    iii    IV     V     vi    vii°
 A     S     D      F      G     H     J      <- home row, what you play with
 1     2     3      4      5     6     7      <- number row, matches the numerals
```

The home row is for playing — seven chords under seven fingers, no reaching.
The number row is for when the thing in your head is "chord five" rather than
"the G key", which is most of the time while you're still learning: a
progression written **I–V–vi–IV** is literally `1 5 6 4`.

Both rows hold for as long as the key is down and chord together the same way
the pads do. They're independent, so a chord held with `1` keeps ringing if you
tap and release `A`.

The looper is on keys too: **space** is the pedal and **escape** plays/stops.
Clear starts unbound on purpose — it wipes your loop with no undo, so it
shouldn't be one stray keystroke away. The arpeggiator is on **Q**.

To change any of them, hit **rebind keys**, tap a pad or a looper button, and
press the key you want. That *replaces* whatever that control answered to, so a
rebound pad has exactly the one key you chose — and taking a key from something
else leaves that thing unbound rather than double-booking it. Escape finishes.
Your layout is remembered in the browser.

## The looper

Works like a guitarist's loop pedal — one button that always does the next
obvious thing:

| press | what happens |
| --- | --- |
| **record** | arms; the loop starts on your first chord, so there's no dead air at the front |
| **close loop** | the loop is however long you just played for, and starts repeating immediately |
| **overdub** | play more on top; it joins the loop when the loop next comes round |
| **end overdub** | back to plain playback |

**Space** is the pedal, **escape** plays/stops — both rebindable. **Clear**
wipes the loop.

The bar under the pads shows where you are in the loop — worth watching, since
overdubbing in time is much easier when you can see it coming round again. It
turns red while you're overdubbing. Pads light up on their own for notes the
loop is playing, so you can see your own layers.

What's recorded is **what you played**, not audio: "chord 4 started 1.2 seconds
in and lasted 0.8 seconds". So overdubbing never degrades no matter how many
layers you stack, and a loop recorded today will pick up whatever the voice
sounds like after milestone 6.

There's no quantisation — the loop is exactly as loose or tight as you played
it. Timing on playback is sample-accurate regardless (measured at 0.0000 ms of
drift over six cycles); see [`src/looper.js`](src/looper.js) for why that takes
two clocks and not one `setInterval`.

## The arpeggiator

Hold a chord with the arp on and its notes take turns instead of sounding
together — one per eighth note, at whatever tempo you dial in, for as long as
you keep holding. **up** climbs, **down** descends, **up-down** bounces.

The harmony doesn't change at all; it's the same three pitches either way. What
changes is that three notes at once is a *texture* and three notes in a row is a
*pattern*, and a pattern has rhythm. This is the thing that makes the instrument
sound like finished music rather than someone leaning on an organ.

**Loops arpeggiate too.** Record a progression as block chords, then switch the
arp on, and the loop starts arpeggiating. What the looper stored was a span of
time and a set of pitches, which is exactly what the arpeggiator eats — so it
works on a recorded chord and a held finger identically.

Two details worth knowing: hold two pads at once and you get two arpeggios in
lockstep rather than one merged run; and toggling the arp lets go of anything
currently held, because a ringing block chord can't be turned into an arpeggio
halfway through a note.

Up-down deliberately plays `0 1 2 1`, not `0 1 2 2 1 0`. The naive version
sounds the top note twice in a row and the turnaround stumbles — you hear a limp
instead of a pulse.

## Octave

**OCT −** and **OCT +** (keys **-** and **=**) move the whole instrument up or
down an octave. The default sits between middle C and the F above it, which is
a good register to *hear* harmony in and a slightly high one to play under
anything — one octave down is usually the nicer place to live.

The range is deliberately lopsided, −2 to +1. Down is genuinely useful: the same
seven chords go from a bright organ to a bass bed. Up runs out fast, because a
triad already spans up to 17 semitones and another octave on top is shrill
rather than musical.

Your octave is remembered along with the key, and recorded loops keep whatever
octave they were played at — same rule as the key.

## Changing key

The two dropdowns pick the root (all 12) and the mode (major or minor). The
status line names the key you're in, and it's remembered across reloads.

**A recorded loop stays put.** It keeps the pitches it was played with, so you
can lay down a progression in C major, switch to A minor, and play over the top
of it without the bed shifting under you. Each recorded chord captures its own
key at the moment you play it — which also means you can overdub a part in a
different key from the one the loop was recorded in, and both keep their own.

Only your live playing follows the selector.

Minor cost exactly one line of theory:

```js
const MINOR_SCALE = [0, 2, 3, 5, 7, 8, 10];
```

No new functions, no branches, no chord table. Stacking every-other-note over
those offsets produces `i ii° III iv v VI VII` on its own — the payoff for
deriving chords rather than tabulating them.

Keys past F# drop an octave rather than climbing, so no key lands in a shrill
register.

## The sound

A single square wave through a fixed lowpass is a *beeper*. Five things separate
that from a synthesiser, and the native build now has all five:

| | why it matters |
|---|---|
| **Two detuned oscillators** | Two saws a few cents apart drift in and out of phase over about a second. That slow beating *is* what "thick" and "warm" are. One oscillator is perfectly static and reads as synthetic instantly. Biggest single win. |
| **A real ADSR** | Not just fade in and fade out. The decay — a dip from the initial peak down to the sustain level — is what makes a note sound *struck* rather than switched on. |
| **A resonant filter that moves** | A fixed lowpass only makes things duller. One that snaps open on the attack and closes over the next few hundred ms is the sound everyone recognises as "a synth". Resonance is a gain bump at the cutoff — a one-pole filter cannot produce it at all. |
| **Stereo** | The three notes of a chord are panned across the field instead of stacked in the middle, using equal-power panning so nothing dips in loudness crossing the centre. |
| **Reverb** | A dry chord happens inside your head; the same chord with a tail happens *somewhere*. Four comb filters make the tail, two allpasses smear it into a wash, and damping rolls the treble off each pass the way a real room absorbs it. |

The comb delay lengths deliberately share no common factors. If they did, the
echoes would line up and you would hear a pitch instead of a room.

## Latency, measured

Not guessed — measured on the same machine, same earbuds.

| | Web (Chrome) | Native (cpal) |
|---|---|---|
| JavaScript / UI thread | 0.1 ms | — |
| Audio buffer | 10 ms | **5.8 ms** (256 frames) |
| OS output path | 40 ms | driver-dependent, not self-inflicted |
| Attack ramp | 6 ms | 4 ms |
| **What the app chose** | **~56 ms** | **~10 ms** |

The browser's `latencyHint: 'interactive'` is a polite suggestion; cpal's
`BufferSize::Fixed(256)` is a number the device either accepts or refuses. That
is the entire difference, and it is why the native build exists.

Note the middle row honestly: the OS still adds its own path in both cases. The
native figure is the part *this program* controls, not a full round-trip
measurement.

## Tests

The theory layer is pure arithmetic with no audio or DOM in it, which makes it
the one part of this project that can be tested automatically. It is, using
Node's built-in test runner — no framework, nothing to install:

```bash
node --test
```

Twenty tests covering scale generation, triad stacking, octave wrapping in both
directions, MIDI→frequency, chord naming, arpeggiator patterns, and the quality
sequences below — in both major and minor, across all twelve keys.

The arpeggiator's *scheduler* isn't in there, because it needs an audio clock to
mean anything. It was verified separately against a fake clock before the UI
existed — note order, step spacing tracking tempo, release, bounded loop holds,
and no burst of missed notes after a stalled tab. See
[planning/PHASE_5.md](planning/PHASE_5.md).

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
and changing mode means changing one array — which is exactly what the key and
mode pickers do. Minor's seven chords come out `i ii° III iv v VI VII` without a
line of new theory code.

**Frequency.** `440 * 2 ** ((midi - 69) / 12)`. MIDI 69 is A4, tuned to 440 Hz
by convention; twelve semitones doubles the frequency, so one semitone
multiplies it by the twelfth root of two.

## Device quirks worth knowing

- **iOS silent switch.** If the physical mute switch on the side of an iPhone is
  flipped on, Safari may play Web Audio silently with no warning whatsoever. If
  the pads light up and nothing comes out, check the switch first — the app
  isn't broken.
- **Expect roughly 50ms between pressing and hearing.** Measured on a Windows
  laptop: ~0.1ms of JavaScript, ~10ms of audio-graph buffer, ~40ms of operating
  system output path, plus a 6ms attack ramp. Serving over localhost has nothing
  to do with it — the files load once, and after that it's all local. The 40ms
  is the browser handing samples to the sound device, and no amount of
  JavaScript touches it; a native audio backend is the only way under it. An
  embedded webview can be worse than a real browser window, so if it feels
  sluggish, try it in a normal tab or as the installed app.
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
- **Service workers need `http://` or `https://`.** The offline cache and the
  install prompt only exist when the page is served, not when it's opened as a
  `file://` double-click. That's deliberate — the registration is guarded on the
  protocol, so opening `index.html` off disk works exactly as it always did, it
  just doesn't install.
- **A backgrounded tab throttles the loop.** Browsers slow page timers right
  down when a tab isn't visible. The looper notices and picks back up at the
  right place, but a loop left running in a hidden tab will stutter while it's
  hidden. Keep the tab in front while you're playing.

## Layout

```
native/             the native build - Rust, cpal, egui
  src/theory.rs       the same theory, ported 1:1, same tests
  src/voice.rs        one note: oscillators, envelopes, filter, panning
  src/reverb.rs       the room, built out of combs and allpasses
  src/engine.rs       the audio thread and the mix; one clock, no scheduler
  src/main.rs         window, pads, keyboard
tools/install-native.ps1  build + install + shortcut, for pinning
index.html          the whole UI: markup and CSS
src/theory.js       music theory — pure functions, no audio, no DOM
src/app.js          the instrument — audio engine, input, key bindings
src/looper.js       record / loop / overdub, and the look-ahead scheduler
src/arp.js          the arpeggiator, and its own step clock
sw.js               offline cache; also what makes it installable
manifest.json       app name, colours, icons
icons/              generated by tools/make-icons.py, committed
tests/theory.test.js
planning/           milestones and progress
```

Five source files, no dependencies, deliberately. A framework here would add a
toolchain, a build step and a node_modules directory to a page whose entire job
is to draw seven rectangles and open an AudioContext; none of that would make
the audio code — the only genuinely tricky part — any simpler to read or debug.

## Deploying

GitHub Pages serves the repository root as-is — there is nothing to build. Push
to `main` and the live site updates a minute or so later.

## Milestones

- [x] **1** — Seven pads, C major, hold-to-sustain, one synth voice
- [x] **2** — Keyboard: home-row bindings, rebindable, remembered
- [x] **3** — Looper: record, loop, overdub layers
- [x] **4** — Bindable transport, and key/mode: all 12 keys, major and minor
- [x] **5** — Arpeggiator: on/off, tempo, up / down / up-down
- [ ] **6** — Sound shaping: waveform, filter cutoff, attack/release
- [ ] **7** — Modifiers: 7ths, octave shift, inversions

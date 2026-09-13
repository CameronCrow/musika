---
type: reference
tags: [repo/musika]
up: "[[musika]]"
---
# Planning — Musika

**Musika** is a seven-button chord organ - a native Rust app for Windows. Each pad plays a whole
triad; all seven triads are drawn from one key, so no sequence of presses can
sound wrong. Hold to sustain, polyphonic, multi-touch, synth voice.

Vanilla HTML/CSS/JS and the Web Audio API. No framework, no build step, no
dependencies. Static site, deployable to GitHub Pages, openable from disk.

Two hard rules the project is built around:

1. **Chords are derived, never tabulated.** No lookup table of "chord qualities
   per key". Triads are stacked from scale positions, so major/minor/diminished
   fall out of the interval pattern and the code generalises to any key or mode
   for free.
2. **The theory layer is pure.** `src/theory.js` has no audio and no DOM, so it
   is unit-testable — and it's the only part of the project that can be.

## Current State

**Musika is the native Rust app under `native/`, and it now does everything the
web build did.** Seven pads, all 12 keys, major and minor, octave shift; mouse,
keyboard and multi-touch; a looper with overdub and an arpeggiator, both running
sample-exact on the audio clock; every key rebindable; settings saved under
`%APPDATA%`. Eleven level-matched patches. A designed icon embedded in the exe.
94 tests. 5.8ms buffer latency.

**The web build is retired.** Still deployed at
<https://cameroncrow.github.io/musika/>, no longer developed.

Next: editable patches, 7ths and inversions, MIDI in.

## Phases

- [[Repos/musika/planning/PHASE_1|PHASE_1]] — playable core (done)
- [[Repos/musika/planning/PHASE_2|PHASE_2]] — keyboard bindings (done)
- [[Repos/musika/planning/PHASE_3|PHASE_3]] — looper (done)
- [[Repos/musika/planning/PHASE_4|PHASE_4]] — bindable transport, key and mode (done)
- Phase 5 — arpeggiator
- Phase 6 — sound shaping
- Phase 7 — chord modifiers

## Related

- [[Repos/musika/planning/PHASE_1|PHASE_1]]
- [[Repos/musika/planning/PHASE_2|PHASE_2]]
- [[Repos/musika/planning/PHASE_3|PHASE_3]]
- [[Repos/musika/planning/PHASE_4|PHASE_4]]
- [[Repos/musika/planning/TODO|TODO]]
- [[musika]]

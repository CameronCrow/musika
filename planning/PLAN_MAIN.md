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

**Musika is the native Rust app under `native/`.** Seven pads, all 12 keys,
major and minor, octave shift, mouse and keyboard, hold-to-sustain, and a real
synth voice: detuned dual oscillators, ADSR, a resonant filter with its own
envelope, equal-power stereo panning and a Schroeder reverb. Four patches,
including `raw` - the original square wave - kept for comparison. 43 tests.
5.8ms buffer latency. Installs to `%LOCALAPPDATA%\Musika` and pins to the
taskbar via `tools/install-native.ps1`.

**The web build is retired.** Still deployed at
<https://cameroncrow.github.io/musika/> and still the only place the looper and
arpeggiator exist, but no longer developed. Both are on the native roadmap.

Next: port the looper, then the arpeggiator.

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

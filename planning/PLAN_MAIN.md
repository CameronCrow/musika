---
type: reference
tags: [repo/musika]
up: "[[musika]]"
---
# Planning — Heptad

**Heptad** is a seven-button chord organ for the browser. Each pad plays a whole
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

**Phase 4 complete.** Seven pads in any of the 12 keys, major or minor,
hold-to-sustain, polyphonic and multi-touch via Pointer Events, one square-wave
voice per note through a lowpass and a limiter. Playable from the keyboard, with
every pad and every looper control rebindable and remembered. A loop pedal that
records, loops and overdubs on a look-ahead scheduler; recorded chords keep the
pitches they were played with, so a loop holds its ground when you change key to
play over it. Theory module covered
by 15 passing tests (`node --test`). Live at
<https://cameroncrow.github.io/musika/> via GitHub Pages, served from the
repository root on `main`.

Next: Phase 5, the arpeggiator - which reuses the looper's scheduler.

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

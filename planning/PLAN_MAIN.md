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

**Phase 1 complete.** Seven pads playing C major, hold-to-sustain, polyphonic
and multi-touch via Pointer Events, one square-wave voice per note through a
lowpass and a limiter. Theory module covered by 12 passing tests (`node --test`).
Awaiting a deployment target — the repo is private and GitHub Pages needs a
public repo or a paid plan.

Next: Phase 2, the arpeggiator.

## Phases

- [[Repos/musika/planning/PHASE_1|PHASE_1]] — playable core (done)
- Phase 2 — arpeggiator
- Phase 3 — key and mode selector
- Phase 4 — sound shaping
- Phase 5 — chord modifiers

## Related

- [[Repos/musika/planning/PHASE_1|PHASE_1]]
- [[Repos/musika/planning/TODO|TODO]]
- [[musika]]

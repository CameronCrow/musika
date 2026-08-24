---
type: reference
tags: [repo/musika]
up: "[[musika]]"
---
# TODO

Milestones for Heptad. One commit per phase.

- [x] **Phase 1 — Playable core.** Seven pads, C major, hold-to-sustain,
      polyphonic multi-touch, one square-wave voice. Theory module + unit tests.
      Deployed.
- [ ] **Phase 2 — Arpeggiator.** On/off, tempo control, up / down / up-down
      patterns. Look-ahead scheduler against `AudioContext.currentTime`.
- [ ] **Phase 3 — Key and mode.** All 12 keys, major and minor. Should need no
      new theory code, only a second scale pattern and a root selector.
- [ ] **Phase 4 — Sound shaping.** Waveform choice, filter cutoff,
      attack/release controls.
- [ ] **Phase 5 — Modifiers.** Add a 7th, octave shift, inversions.

## Related

- [[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]
- [[Repos/musika/planning/PHASE_1|PHASE_1]]
- [[musika]]

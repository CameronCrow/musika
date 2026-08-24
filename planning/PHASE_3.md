---
type: reference
tags: [repo/musika]
up: "[[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]"
---
# Phase 3 — Looper

Goal: stack sounds. Record a phrase, have it repeat, and play more on top of it.

## Checklist

- [x] Loop-pedal transport: one pedal button that always does the next thing
- [x] Loop starts on the first chord, not on the button press
- [x] Loop length set by the first pass
- [x] Overdub onto an existing loop, any number of times
- [x] Play / stop, and clear
- [x] Look-ahead scheduler against `AudioContext.currentTime`
- [x] `startChord` / `stopChord` take an optional scheduled time
- [x] Loop position bar, coloured differently while overdubbing
- [x] Pads light in time with the notes the loop is playing
- [x] Space is the pedal; escape stops
- [x] Recovers from a throttled timer without a burst of stale notes
- [x] Verified: 0.0000 ms drift across six cycles

## Decisions

**Record events, not audio.** A loop is a list of `{t, degree, dur}`. That's a
few hundred bytes rather than megabytes, never degrades however many times you
overdub, and means a recorded loop will pick up later changes to the voice, the
key, or the arpeggiator. Recording audio would freeze the sound as it was.

**Two clocks.** A 25ms `setInterval` that does no timing, and the audio clock
that everything is actually scheduled against. The timer only decides *when to
think ahead*; the hardware decides when to make a sound. Measured drift over six
cycles is zero, because every event time is computed as
`playStart + cycle * length + t` rather than accumulated.

**Overdubs merge at the loop wrap**, not immediately — that's the one moment the
event array can be re-sorted without confusing the scheduler's position. Cost:
anything played in the last 120ms of a pass joins on the following cycle.

**Held notes are truncated at the loop boundary** rather than wrapped, so a note
can't fight with the copy of itself starting at position 0.

**No quantisation.** It would need a tempo, and a tempo the player didn't choose
is a different instrument. Revisit alongside the arpeggiator in Phase 4, which
introduces a tempo anyway.

**Background tabs.** Browsers throttle `setInterval` to about 1 Hz in a hidden
tab while the audio clock keeps running, which would leave the scheduler far
behind and then flush every missed note at once. It resyncs to the current cycle
and drops past-due events instead.

## Related

- [[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]
- [[Repos/musika/planning/TODO|TODO]]

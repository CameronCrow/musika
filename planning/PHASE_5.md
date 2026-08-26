---
type: reference
tags: [repo/musika]
up: "[[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]"
---
# Phase 5 — Arpeggiator

The feature that makes the instrument sound like finished music rather than
someone leaning on an organ. Held chords stop being a block and start being a
pattern; the harmony is identical either way, but a pattern has rhythm.

## Checklist

- [x] `arpSequence` in theory.js — pure, no audio, tested
- [x] up / down / up-down patterns
- [x] Tempo control (40–240 BPM), eighth notes
- [x] On/off, bindable to a key like every other control
- [x] Look-ahead scheduler against `AudioContext.currentTime`, not `setInterval`
- [x] Survives a backgrounded tab without flushing a burst of missed notes
- [x] Settings remembered across reloads
- [x] Looped chords arpeggiate too
- [x] Old saved key bindings migrate instead of being discarded

## The one idea: a hold

Chords arrive from two unrelated places — your finger (starts now, ends at some
unknown future moment) and the looper (knows in advance that chord 4 runs from
t=12.80 to t=13.55). Rather than write the arpeggiator twice, both become the
same thing:

```js
{ degree, from, until }
```

A live hold just sits at `until = Infinity` until you let go.

That single shared idea is why **a loop recorded as block chords starts
arpeggiating the moment you switch the arp on** — the same reason it transposes
when you change key. The looper stores what you *did* ("held chord 4"), never
what it sounded like. Recording audio instead would have frozen both.

## Decisions

**Its own step grid, not the looper's scheduler.** The TODO optimistically said
this phase would share it. It shouldn't: the looper's position wraps at the end
of a loop, while the arp grid free-runs forever at the dialled tempo. They look
alike but they are not the same clock, and fusing them means one clock
pretending to be two — more code than having two.

**Eighth notes, fixed.** Sixteenths are frantic at any usable tempo and quarters
barely register as an arpeggio. A subdivision control is a knob nobody would
move; if that turns out wrong it's one constant.

**Each held pad arpeggiates independently, on a shared grid.** Hold two pads and
you get two arpeggios in lockstep rather than one merged six-note run. Simpler,
and it keeps each chord starting its pattern on its own first note.

**Up-down drops the repeated endpoints** — `0 1 2 1`, never `0 1 2 2 1 0`. The
naive version plays the top note twice in a row and the turnaround stumbles; you
hear a limp instead of a pulse.

**Toggling the arp releases everything held.** A ringing block chord can't be
converted into an arpeggio mid-note, so anything down when you flip it gets let
go. Predictable beats clever.

**Old bindings migrate rather than reset.** Adding `arp` to `ACTIONS` made the
saved array the wrong length again. Phase 4 discarded short saves; this one
keeps the layout and fills only genuinely new slots, and only when the default
key isn't already taken — two actions on one key means the later one silently
wins.

## Verification

`arpSequence` is covered by six unit tests (20 total): pattern order, the
four-note turnaround that matters once Phase 7 adds 7ths, degenerate 0/1/2-note
chords, an unknown stored pattern falling back to up instead of going silent,
and an exhaustive check that every step indexes a note that exists and that no
note is unreachable.

The scheduler itself can't be unit-tested — it needs an audio clock — so it was
driven through a throwaway harness with a fake clock and fake oscillators before
the UI existed: note order per pattern, step spacing tracking tempo, release,
hold garbage collection, two chords on one grid, bounded loop holds staying
inside their span, no burst after a 10-second stall, and a zero BPM not hanging
the tab. Twelve checks, all passing. Not committed — it stubs half the engine,
so it would rot into a test of its own mocks.

## Related

- [[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]
- [[Repos/musika/planning/PHASE_3|PHASE_3]] — the looper, whose holds this reuses
- [[Repos/musika/planning/TODO|TODO]]

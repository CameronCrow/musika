---
type: reference
tags: [repo/musika]
up: "[[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]"
---
# Phase 4 — Bindable transport, and the musical key

Two things asked for together: put the looper on a key you can see and change,
and let the instrument play in something other than C major.

## Checklist

- [x] One binding list covering the pads *and* the looper controls
- [x] Transport keys shown on their buttons (space, esc)
- [x] Rebind a looper button the same way as a pad — bind mode, tap, press
- [x] Clear defaults to unbound; it wipes your loop
- [x] `MINOR_SCALE` in theory.js, with tests across all 12 keys
- [x] Key picker: all 12 roots. Mode picker: major / minor
- [x] Key and mode remembered across reloads
- [x] Keys past F# drop an octave so no key sits in a shrill register
- [x] Status line names the current key
- [x] Verified: a running loop transposes when the key changes

## Decisions

**One `ACTIONS` list, not two systems.** Pads are identified by chord number,
looper controls by name, and both live in the same bindings array. Rebinding,
persistence, stealing a key from something else, and the on-screen labels are
written once. A saved binding from before the transport was bindable is the
wrong length, fails validation, and falls back to the defaults.

**`clear` ships unbound.** It destroys a loop with no undo, so it shouldn't be
one stray keystroke away. Bind it yourself if you want it.

**Native `<select>` for key and mode.** On a phone these open the OS picker,
which is far easier to hit with a thumb than anything worth hand-rolling.

**Keys past F# drop an octave.** Otherwise B major would start a semitone below
the C above middle C and put the whole instrument in a shrill register. This way
every key sits within about a fifth of middle C.

**A running loop transposes with the key**, and this took no code at all. The
looper records chord *numbers*, so "chord 5" means the fifth chord of whatever
key is selected now. Confirmed live: the same loop played C-E-G / G-B-D in C
major and A-C-E / E-G-B after switching to A minor.

## What minor cost

One array in theory.js:

```js
const MINOR_SCALE = [0, 2, 3, 5, 7, 8, 10];
```

No new functions, no branches, no chord table. Stacking every-other-note over
those offsets produces `i ii° III iv v VI VII` on its own, which is the payoff
for deriving chords in Phase 1 rather than tabulating them.

## Related

- [[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]
- [[Repos/musika/planning/TODO|TODO]]

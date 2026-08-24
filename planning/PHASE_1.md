---
type: reference
tags: [repo/musika]
up: "[[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]"
---
# Phase 1 — Playable core

Goal: something you can hold in your hands and play before anything else
exists. Seven pads, C major, hold-to-sustain, one voice. No arpeggiator, no key
selector, no sound controls.

## Checklist

- [x] Theory module (`src/theory.js`) — pure functions, no audio, no DOM
  - [x] `scaleNote` with octave wrapping in both directions
  - [x] `triad` by stacking scale positions n, n+2, n+4
  - [x] `quality` derived from the two interval gaps
  - [x] `midiToFreq`, `noteName`, `chordName`, `romanNumeral`
- [x] Unit tests (`node --test`) — 12 tests, including the
      major/minor/minor/major/major/minor/diminished sequence across all 12 keys
- [x] Seven pads, labelled with roman numeral, chord name and note letters
- [x] Hold-to-sustain, polyphonic, multi-touch (Pointer Events + capture)
- [x] Square-wave voice, one oscillator per note
- [x] AudioContext created/resumed inside a user gesture
- [x] Gain envelopes (ramp in, exponential release) — no clicks
- [x] Master gain, lowpass, and a compressor for headroom
- [x] `touch-action: none`, no zoom, no scroll, no long-press selection
- [x] Stuck-note guards on blur, tab hide, and pointercancel
- [x] Keyboard 1–7 for desktop
- [x] README: how to run, how the theory works, device quirks
- [ ] Deployed to GitHub Pages — blocked, see below

## Decisions

**No framework, no build step.** Three files, zero dependencies. The only hard
part of this project is the audio scheduling, and no framework makes that
easier to read.

**Plain `<script>`, not ES modules.** `type="module"` is blocked by browser
security when a page is opened from `file://`, and opening `index.html` off disk
was a requirement. `theory.js` declares plain functions (globals in a browser)
and exports them via a three-line `typeof module` guard for the Node test
runner. One file, both worlds, no bundler.

**Voice design.** One square oscillator per note — three per chord, up to 21 at
once with both hands down. Square is the chiptune sound asked for; a fixed
lowpass at 2.6 kHz keeps 21 of them from reading as hiss, and a compressor on
the master bus absorbs the peaks that would otherwise clip. The filter becomes
adjustable in Phase 4.

**Chords sit at middle C (MIDI 60).** Comfortable register, and the same
constant becomes the key root in Phase 3.

## Blocked

GitHub Pages needs a public repository or a paid plan; `musika` is private.
Either make it public or pick a different host.

## Related

- [[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]
- [[Repos/musika/planning/TODO|TODO]]

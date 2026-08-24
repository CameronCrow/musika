---
type: reference
tags: [repo/musika]
up: "[[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]"
---
# Phase 2 — Keyboard

Goal: play it properly at a laptop, with keys you chose rather than keys the
code chose.

## Checklist

- [x] Home row (A S D F G H J) as the default binding
- [x] Per-pad rebinding: tap "rebind keys", tap a pad, press a key
- [x] Bindings persist in localStorage, with a sanity check on load
- [x] Binding a key already in use takes it off the pad that had it
- [x] Each pad shows its own key on its face
- [x] Key auto-repeat doesn't restack voices
- [x] Ctrl/Cmd/Alt combinations pass through to the browser
- [x] Transport/settings bar in the layout, ready for Phase 3

## Decisions

**Home row, not the number row.** Seven chords under seven fingers with no
reaching. The number row was fine for proving audio worked; it's bad for
playing.

**localStorage, not a settings file or a URL parameter.** One key, one JSON
array, wrapped in try/catch because private browsing can make it throw. A
corrupt or wrong-length entry falls back to the default rather than leaving the
instrument unplayable.

**Pad bindings win over any future transport shortcut.** If you bind a key that
a transport shortcut also wants, the pad gets it and you use the on-screen
button instead. Simpler than a reserved-key list, and pads are the thing you
play.

## Related

- [[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]
- [[Repos/musika/planning/TODO|TODO]]

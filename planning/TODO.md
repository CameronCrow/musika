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
- [x] **Phase 2 — Keyboard.** Home-row bindings by default, rebindable per pad,
      remembered across reloads.
- [x] **Phase 3 — Looper.** Record what you play, loop it, overdub layers on top.
      Look-ahead scheduler against `AudioContext.currentTime`.
- [x] **Phase 4 — Bindable transport, and the musical key.** The looper's
      controls join the key-binding system; key and mode pickers for all 12
      keys, major and minor. Minor cost one array in theory.js.
- [x] **Phase 5 — Arpeggiator.** On/off, tempo control, up / down / up-down
      patterns, bindable to a key. Its own step clock, not the looper's - the
      looper's position wraps, the arp grid free-runs. Looped chords arpeggiate
      too, because both are just a chord held over a span of time.
- [ ] **Phase 6 — Sound shaping.** Waveform choice, filter cutoff,
      attack/release controls.
- [ ] **Phase 7 — Modifiers.** Add a 7th, inversions. *(Octave shift landed
      early, out of phase - see below.)*

## Outside the phases

- [x] **Native voice.** Detuned dual oscillators, ADSR, resonant filter with its
      own envelope, equal-power stereo spread, and a Schroeder reverb. Four
      patches; `--render` writes a WAV of all of them for judging by ear.
      Installable and pinnable via `tools/install-native.ps1`.
- [x] **Native build** (`native/`). Rust + cpal + egui. Milestone 1 equivalent:
      seven pads, 12 keys, major/minor, octave, mouse and keyboard,
      hold-to-sustain. 26 tests. Buffer latency 5.8ms against the web build's
      ~56ms. Looper, arpeggiator and multi-touch not yet ported.

- [x] **Octave shift.** Pulled forward out of Phase 7 on request: the default
      register is a little high to play under. Two bindable buttons, range -2
      to +1, stored with the key.
- [x] **Latency trimmed** where it was reachable: explicit `latencyHint` and a
      6ms attack instead of 12ms. ~62ms to ~56ms. The remaining ~40ms is the
      OS output path and is not addressable from JavaScript.

- [x] **Installable.** Web app manifest, generated icons, and a service worker
      that caches the eight files the instrument needs. Installs as a desktop
      app from Chrome/Edge and as a home-screen app on iOS, and plays offline.
      Chosen over Tauri: a Tauri shell would have been ~20 lines of Rust that
      open a window, leaving the audio identical, in exchange for a toolchain,
      a build step, and losing `file://`.

## Related

- [[Repos/musika/planning/PLAN_MAIN|PLAN_MAIN]]
- [[Repos/musika/planning/PHASE_1|PHASE_1]]
- [[Repos/musika/planning/PHASE_2|PHASE_2]]
- [[Repos/musika/planning/PHASE_3|PHASE_3]]
- [[Repos/musika/planning/PHASE_4|PHASE_4]]
- [[Repos/musika/planning/PHASE_5|PHASE_5]]
- [[musika]]

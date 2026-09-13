//! Musika - a seven-chord organ.
//!
//! Seven pads, each playing a whole chord, all seven belonging to one key, so
//! there is no wrong note to hit. Record what you play into a loop and stack
//! more on top; switch the arpeggiator on and held chords become patterns.
//!
//! Built as a Windows GUI program rather than a console one, which is what stops
//! a terminal flashing up behind the window. The cost is that a GUI process has
//! no stdout, so `--probe` and the progress lines from `--render` only print from
//! a debug build (`cargo run -- --probe`). `--render` still writes its WAV.
//!
//!   theory.rs    the music - pure integer arithmetic
//!   voice.rs     what one note sounds like - oscillators, envelopes, filter
//!   reverb.rs    the room it is played in
//!   looper.rs    record / overdub / loop, a state machine over sample numbers
//!   arp.rs       the arpeggiator, on the same sample clock
//!   engine.rs    the audio thread: all of the above, mixed
//!   settings.rs  what is remembered between launches
//!   main.rs      the instrument - window, pads, controls, keys, touch
//!
//! Run it with `cargo run --release`. Debug builds work but the audio thread
//! has far less headroom, so use release if you hear crackling.

// Debug builds keep a console so the developer flags can print; release builds
// are pure GUI, so launching never spawns a terminal.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod arp;
mod engine;
mod looper;
mod reverb;
mod settings;
mod theory;
mod voice;

use eframe::egui::{self, Color32, FontId, Key, Pos2, Rect, RichText};
use engine::{Engine, Msg, Status};
use looper::{BAR_CHOICES, LoopState, MAX_LAYERS};
use settings::{OCTAVE_MAX, OCTAVE_MIN, Settings};
use theory::{ArpPattern, Chord, Mode};
use voice::PATCHES;

// --- the look ----------------------------------------------------------------
//
// The same machined-aluminium language as the retired web build: a graphite
// shell, a dark pocket routed into it, coloured caps standing in the pocket, and
// exactly two lamps - amber for "engaged", red for "hot" (armed or recording).
// Nothing else glows, so the pads stay the loudest thing on the face.

const SHELL: Color32 = Color32::from_rgb(0x43, 0x47, 0x4e);
const SHELL_HI: Color32 = Color32::from_rgb(0x55, 0x5a, 0x63);
const SHELL_LO: Color32 = Color32::from_rgb(0x33, 0x36, 0x3c);
const PLATE: Color32 = Color32::from_rgb(0x1d, 0x20, 0x25);
const INK: Color32 = Color32::from_rgb(0xee, 0xf1, 0xf6);
const INK_DIM: Color32 = Color32::from_rgb(0xb6, 0xbc, 0xc6);
const LAMP: Color32 = Color32::from_rgb(0xff, 0xc2, 0x4a);
const HOT: Color32 = Color32::from_rgb(0xd8, 0x34, 0x1f);

/// How far a pad travels when pressed, in points. It bottoms out rather than
/// shrinking, which is what makes it read as a physical key.
const TRAVEL: f32 = 3.0;

// --- what a key can do ---------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    Pad(usize),
    Record,
    PlayStop,
    Undo,
    Clear,
    Arp,
    OctaveDown,
    OctaveUp,
}

/// Every bindable action, in the order bindings are stored.
const ACTIONS: [Action; 14] = [
    Action::Pad(0),
    Action::Pad(1),
    Action::Pad(2),
    Action::Pad(3),
    Action::Pad(4),
    Action::Pad(5),
    Action::Pad(6),
    Action::Record,
    Action::PlayStop,
    Action::Undo,
    Action::Clear,
    Action::Arp,
    Action::OctaveDown,
    Action::OctaveUp,
];

impl Action {
    /// The name used in the settings file.
    fn name(self) -> &'static str {
        const PADS: [&str; 7] = ["pad1", "pad2", "pad3", "pad4", "pad5", "pad6", "pad7"];
        match self {
            Action::Pad(i) => PADS[i.min(6)],
            Action::Record => "record",
            Action::PlayStop => "playstop",
            Action::Undo => "undo",
            Action::Clear => "clear",
            Action::Arp => "arp",
            Action::OctaveDown => "octave_down",
            Action::OctaveUp => "octave_up",
        }
    }

    /// The name shown to a person.
    fn label(self) -> String {
        match self {
            Action::Pad(i) => format!("pad {}", i + 1),
            Action::Record => "record".into(),
            Action::PlayStop => "play / stop".into(),
            Action::Undo => "undo layer".into(),
            Action::Clear => "clear".into(),
            Action::Arp => "arp".into(),
            Action::OctaveDown => "octave down".into(),
            Action::OctaveUp => "octave up".into(),
        }
    }

    fn index(self) -> usize {
        ACTIONS.iter().position(|a| *a == self).expect("every action is listed")
    }
}

/// The home row to play with, and the number row as well, because the pads are
/// labelled I..vii and sometimes the thing in your head is "chord five" rather
/// than "the G key". Clear starts unbound on purpose: it wipes every layer at
/// once, and that should not be one stray keystroke away. Undo, which takes back
/// one layer, is on backspace.
fn default_bindings() -> Vec<Vec<Key>> {
    use Key::*;
    vec![
        vec![A, Num1],
        vec![S, Num2],
        vec![D, Num3],
        vec![F, Num4],
        vec![G, Num5],
        vec![H, Num6],
        vec![J, Num7],
        vec![Space],
        vec![Escape],
        vec![Backspace],
        vec![],
        vec![Q],
        vec![Minus],
        vec![Equals],
    ]
}

/// Defaults, overridden by whatever the settings file says for each action.
fn bindings_from(settings: &Settings) -> Vec<Vec<Key>> {
    let mut bindings = default_bindings();
    for (i, action) in ACTIONS.iter().enumerate() {
        if let Some((_, names)) = settings.bindings.iter().find(|(a, _)| a == action.name()) {
            bindings[i] = names.iter().filter_map(|n| Key::from_name(n)).collect();
        }
    }
    bindings
}

/// Give `key` to one action, taking it off anything else that had it.
///
/// Rebinding REPLACES that action's keys rather than adding to them: "the key
/// you press is the key for this" is behaviour you can predict without being
/// told, and adding would grow the list with no way to shrink it.
fn bind(bindings: &mut [Vec<Key>], target: usize, key: Key) {
    for keys in bindings.iter_mut() {
        keys.retain(|k| *k != key);
    }
    bindings[target] = vec![key];
}

fn key_label(key: Key) -> String {
    match key {
        Key::Space => "space".into(),
        Key::Escape => "esc".into(),
        Key::Backspace => "bksp".into(),
        Key::Minus => "-".into(),
        Key::Equals => "=".into(),
        other => {
            let name = other.name();
            name.strip_prefix("Num").unwrap_or(name).to_string()
        }
    }
}

fn keys_label(keys: &[Key]) -> String {
    keys.iter().map(|k| key_label(*k)).collect::<Vec<_>>().join(" · ")
}

/// Where a note came from, so letting go of one input cannot silence another -
/// holding a pad with two fingers and lifting one must not stop the chord.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Source {
    Key(Key),
    Pointer,
    Touch(u64),
}

impl Source {
    /// A stable id the audio thread can match a NoteOff against.
    ///
    /// Always below `engine::AUTO_ID_BASE` (2^48), which marks a note as the
    /// looper's or the arpeggiator's rather than a finger's.
    fn id(self, degree: usize) -> u64 {
        let (kind, sub) = match self {
            Source::Key(k) => (1u64, k as u64),
            Source::Pointer => (2, 0),
            Source::Touch(t) => (3, t & 0xFF_FFFF),
        };
        (kind << 40) | (sub << 8) | degree as u64
    }
}

// --- the instrument ------------------------------------------------------------

struct Musika {
    engine: Option<Engine>,
    error: Option<String>,

    key_pitch: i32, // 0-11, 0 = C
    mode: Mode,
    octave: i32,
    patch_index: usize,
    chords: [[i32; 3]; 7],

    arp_on: bool,
    arp_pattern: ArpPattern,
    bpm: f32,
    bars: u8,

    bindings: Vec<Vec<Key>>,
    rebinding: bool,
    binding_target: Option<usize>,

    /// Which (source, pad) pairs are sounding right now.
    held: Vec<(Source, usize)>,
    /// Fingers on a touchscreen, and the pad each is on.
    touches: Vec<(u64, usize)>,
    /// Whether the current mouse press started on a pad - so dragging out of a
    /// dropdown and across the pads does not play them.
    pointer_on_pads: bool,
    pad_rects: [Rect; 7],
    /// Settings changed and not yet written.
    dirty: bool,
}

impl Musika {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        style(&cc.egui_ctx);

        let s = Settings::load();
        let patch_index = PATCHES.iter().position(|p| p.name == s.patch).unwrap_or(1);
        let (engine, error) = match Engine::start(patch_index) {
            Ok(e) => (Some(e), None),
            Err(e) => (None, Some(e)),
        };

        let mut app = Musika {
            engine,
            error,
            key_pitch: s.key_pitch,
            mode: s.mode,
            octave: s.octave,
            patch_index,
            chords: [[0; 3]; 7],
            arp_on: s.arp_on,
            arp_pattern: s.arp_pattern,
            bpm: s.bpm,
            bars: s.bars,
            bindings: bindings_from(&s),
            rebinding: false,
            binding_target: None,
            held: Vec::new(),
            touches: Vec::new(),
            pointer_on_pads: false,
            pad_rects: [Rect::NOTHING; 7],
            dirty: false,
        };
        app.rebuild_chords();
        app.send(Msg::SetTempo(app.bpm));
        app.send(Msg::SetBars(app.bars));
        app.send(Msg::SetArpPattern(app.arp_pattern));
        app.send(Msg::SetArp(app.arp_on));
        app
    }

    fn send(&self, msg: Msg) {
        if let Some(e) = &self.engine {
            e.send(msg);
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            key_pitch: self.key_pitch,
            mode: self.mode,
            octave: self.octave,
            patch: PATCHES[self.patch_index].name.to_string(),
            arp_on: self.arp_on,
            arp_pattern: self.arp_pattern,
            bpm: self.bpm,
            bars: self.bars,
            bindings: ACTIONS
                .iter()
                .zip(&self.bindings)
                .map(|(a, keys)| {
                    (a.name().to_string(), keys.iter().map(|k| k.name().to_string()).collect())
                })
                .collect(),
        }
    }

    /// Keys past F# drop an octave rather than climbing, so no key lands in a
    /// shrill register.
    fn root_midi(&self) -> i32 {
        let pitch = if self.key_pitch > 6 { self.key_pitch - 12 } else { self.key_pitch };
        60 + pitch + self.octave * 12
    }

    fn rebuild_chords(&mut self) {
        self.chords = theory::chords_in_key(self.root_midi(), &self.mode.pattern());
    }

    fn note_on(&mut self, source: Source, degree: usize) {
        if self.held.contains(&(source, degree)) {
            return;
        }
        self.held.push((source, degree));
        self.send(Msg::NoteOn {
            id: source.id(degree),
            chord: Chord::new(&self.chords[degree]),
            degree: degree as u8,
        });
    }

    fn note_off(&mut self, source: Source, degree: usize) {
        // Only if it was actually down: this is called for every unheld key on
        // every frame, and each message would be a heap allocation in the
        // channel for nothing.
        if let Some(i) = self.held.iter().position(|&h| h == (source, degree)) {
            self.held.remove(i);
            self.send(Msg::NoteOff { id: source.id(degree) });
        }
    }

    fn release_all(&mut self) {
        self.held.clear();
        self.touches.clear();
        self.send(Msg::ReleaseLive);
    }

    fn is_lit(&self, degree: usize) -> bool {
        self.held.iter().any(|&(_, d)| d == degree)
    }

    fn pad_at(&self, pos: Pos2) -> Option<usize> {
        self.pad_rects.iter().position(|r| r.contains(pos))
    }

    // Changing anything about the pitches releases what is held - a chord from
    // the old key ringing under the new one is just mud. A running loop is not
    // affected: it stores the pitches it was played with.

    fn set_key(&mut self, pitch: i32, mode: Mode) {
        if pitch != self.key_pitch || mode != self.mode {
            self.key_pitch = pitch;
            self.mode = mode;
            self.release_all();
            self.rebuild_chords();
            self.dirty = true;
        }
    }

    fn set_octave(&mut self, octave: i32) {
        let octave = octave.clamp(OCTAVE_MIN, OCTAVE_MAX);
        if octave != self.octave {
            self.octave = octave;
            self.release_all();
            self.rebuild_chords();
            self.dirty = true;
        }
    }

    fn set_patch(&mut self, index: usize) {
        if index != self.patch_index {
            self.patch_index = index;
            self.release_all();
            self.send(Msg::SetPatch(index));
            self.dirty = true;
        }
    }

    fn set_arp(&mut self, on: bool) {
        self.arp_on = on;
        self.held.clear();
        self.touches.clear();
        self.send(Msg::SetArp(on));
        self.dirty = true;
    }

    fn set_rebinding(&mut self, on: bool) {
        self.rebinding = on;
        self.binding_target = None;
        self.release_all();
    }

    fn do_action(&mut self, action: Action) {
        match action {
            Action::Pad(_) => {}
            Action::Record => self.send(Msg::Pedal),
            Action::PlayStop => self.send(Msg::PlayStop),
            Action::Undo => self.send(Msg::Undo),
            Action::Clear => self.send(Msg::ClearLoop),
            Action::Arp => self.set_arp(!self.arp_on),
            Action::OctaveDown => self.set_octave(self.octave - 1),
            Action::OctaveUp => self.set_octave(self.octave + 1),
        }
    }

    // --- input -----------------------------------------------------------------

    fn handle_keys(&mut self, ctx: &egui::Context) {
        // Key-up events never arrive while another window has focus, so a chord
        // held when you Alt-Tab away would ring forever. Let go instead.
        if !ctx.input(|i| i.focused) {
            let keyed: Vec<_> = self
                .held
                .iter()
                .copied()
                .filter(|(s, _)| matches!(s, Source::Key(_)))
                .collect();
            for (source, degree) in keyed {
                self.note_off(source, degree);
            }
            return;
        }

        let (presses, down) = ctx.input(|i| {
            let presses: Vec<Key> = i
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. }
                        // Leave shortcuts alone.
                        if !modifiers.ctrl && !modifiers.alt && !modifiers.command =>
                    {
                        Some(*key)
                    }
                    _ => None,
                })
                .collect();
            (presses, i.keys_down.clone())
        });

        if self.rebinding {
            for key in &presses {
                if *key == Key::Escape {
                    self.set_rebinding(false);
                    break;
                }
                if let Some(target) = self.binding_target.take() {
                    bind(&mut self.bindings, target, *key);
                    self.dirty = true;
                }
            }
            ctx.input_mut(|i| {
                for key in &presses {
                    i.consume_key(egui::Modifiers::NONE, *key);
                }
            });
            return;
        }

        let bindings = self.bindings.clone();
        for (i, action) in ACTIONS.iter().enumerate() {
            match *action {
                Action::Pad(degree) => {
                    for &key in &bindings[i] {
                        if down.contains(&key) {
                            self.note_on(Source::Key(key), degree);
                        } else {
                            self.note_off(Source::Key(key), degree);
                        }
                    }
                }
                other => {
                    if bindings[i].iter().any(|k| presses.contains(k)) {
                        self.do_action(other);
                    }
                }
            }
        }

        // Swallow the keys the instrument uses, so a button that happens to hold
        // keyboard focus does not react as well - otherwise space would record
        // *and* re-click whatever was clicked last.
        ctx.input_mut(|i| {
            for key in bindings.iter().flatten() {
                i.consume_key(egui::Modifiers::NONE, *key);
            }
        });
    }

    /// Fingers on a touchscreen, each one its own voice. Sliding a finger from
    /// one pad to the next moves the chord with it, like sliding along keys.
    fn handle_touches(&mut self, ui: &egui::Ui) -> bool {
        let events: Vec<(u64, egui::TouchPhase, Pos2)> = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Touch { id, phase, pos, .. } => Some((id.0, *phase, *pos)),
                    _ => None,
                })
                .collect()
        });

        for &(id, phase, pos) in &events {
            let slot = self.touches.iter().position(|t| t.0 == id);
            match phase {
                egui::TouchPhase::Start | egui::TouchPhase::Move => {
                    let over = self.pad_at(pos);
                    if self.rebinding {
                        if phase == egui::TouchPhase::Start {
                            if let Some(d) = over {
                                self.binding_target = Some(d);
                            }
                        }
                        continue;
                    }
                    let was = slot.map(|i| self.touches[i].1);
                    if was != over {
                        if let Some(i) = slot {
                            self.touches.remove(i);
                        }
                        if let Some(d) = was {
                            self.note_off(Source::Touch(id), d);
                        }
                        if let Some(d) = over {
                            self.note_on(Source::Touch(id), d);
                            self.touches.push((id, d));
                        }
                    }
                }
                egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                    if let Some(i) = slot {
                        let d = self.touches[i].1;
                        self.touches.remove(i);
                        self.note_off(Source::Touch(id), d);
                    }
                }
            }
        }

        // egui also turns the first finger into mouse events. While any finger
        // is down, ignore the mouse, or that finger would play its chord twice.
        !events.is_empty() || !self.touches.is_empty()
    }

    fn handle_pointer(&mut self, ui: &egui::Ui) {
        let (down, pressed, pos) = ui.input(|i| {
            (i.pointer.primary_down(), i.pointer.primary_pressed(), i.pointer.interact_pos())
        });
        // Only count the pointer as over a pad if nothing - an open dropdown,
        // say - is drawn on top of the pads at that spot.
        let over = pos
            .filter(|p| ui.ctx().layer_id_at(*p) == Some(ui.layer_id()))
            .and_then(|p| self.pad_at(p));

        if pressed {
            self.pointer_on_pads = over.is_some();
        }
        if !down {
            self.pointer_on_pads = false;
        }

        if self.rebinding {
            if pressed {
                if let Some(d) = over {
                    self.binding_target = Some(d);
                }
            }
            return;
        }

        let target = if down && self.pointer_on_pads { over } else { None };
        let current = self
            .held
            .iter()
            .find(|(s, _)| *s == Source::Pointer)
            .map(|&(_, d)| d);
        if current != target {
            if let Some(d) = current {
                self.note_off(Source::Pointer, d);
            }
            if let Some(d) = target {
                self.note_on(Source::Pointer, d);
            }
        }
    }

    // --- drawing ---------------------------------------------------------------

    /// What to press for `action`, for the legend - its key, or the button's
    /// name if it has no key.
    fn key_hint(&self, action: Action, button: &str) -> String {
        let keys = keys_label(&self.bindings[action.index()]);
        if keys.is_empty() { button.to_string() } else { keys }
    }

    fn legend(&self, status: Option<&Status>) -> (String, Color32) {
        if let Some(err) = &self.error {
            return (err.clone(), HOT);
        }
        if self.rebinding {
            let msg = match self.binding_target {
                Some(i) => format!(
                    "press the key for {}   ·   esc to finish",
                    ACTIONS[i].label()
                ),
                None => "click a pad or a control, then press a key   ·   esc to finish".into(),
            };
            return (msg, LAMP);
        }

        let mut text = format!(
            "MUSIKA   —   {} {}",
            theory::note_name(self.root_midi()),
            self.mode.name()
        );
        if self.octave != 0 {
            text += &format!("   ·   oct {:+}", self.octave);
        }
        if self.arp_on {
            text += "   ·   arp";
        }

        // Always say what to do next. Someone who has never used a looper
        // should be able to make one by reading this line and nothing else.
        let Some(s) = status else {
            return (text, INK_DIM);
        };
        let record = self.key_hint(Action::Record, "record");
        let layers = s.layers();
        let bars = s.loop_bars().max(1);
        let hint = match s.loop_state() {
            LoopState::Idle => format!("press {record} to start a loop"),
            LoopState::Armed => {
                format!("play a chord to start recording   ·   {}", bars_label(self.bars))
            }
            LoopState::Recording => {
                let bar = ((s.loop_position() * bars as f32) as u8 + 1).min(bars);
                format!("recording layer 1   ·   bar {bar} of {bars}")
            }
            LoopState::Playing if layers.len() >= MAX_LAYERS => {
                format!("{MAX_LAYERS} layers, the most there can be   ·   undo one to add another")
            }
            LoopState::Playing => format!("looping   ·   {record} records another layer on top"),
            LoopState::Overdub => {
                let n = match layers.last() {
                    Some(l) if l.recording => layers.len(),
                    _ => layers.len() + 1,
                };
                format!("recording layer {n}   ·   until the loop comes round again")
            }
            LoopState::Stopped => {
                format!("stopped   ·   {} to play", self.key_hint(Action::PlayStop, "play"))
            }
        };
        text += &format!("   ·   {hint}");
        (text, INK_DIM)
    }

    /// A control button that shows its bound key, and in rebind mode chooses
    /// itself as the thing to rebind instead of doing its job. Returns true when
    /// it should do its job.
    fn control(
        &mut self,
        ui: &mut egui::Ui,
        action: Action,
        label: &str,
        enabled: bool,
        lamp: Option<Color32>,
    ) -> bool {
        let idx = action.index();
        let targeted = self.rebinding && self.binding_target == Some(idx);
        let hint = keys_label(&self.bindings[idx]);
        let text = if targeted {
            format!("{label}   press a key")
        } else if hint.is_empty() {
            label.to_string()
        } else {
            format!("{label}   {hint}")
        };

        let fill = if targeted { Some(LAMP) } else { lamp };
        let ink = if fill.is_some() { Color32::BLACK } else { INK };
        let mut button = egui::Button::new(RichText::new(text).color(ink));
        if let Some(c) = fill {
            button = button.fill(c);
        }

        // While rebinding, every control can be picked, even greyed-out ones.
        if ui.add_enabled(enabled || self.rebinding, button).clicked() {
            if self.rebinding {
                self.binding_target = Some(idx);
                return false;
            }
            return true;
        }
        false
    }

    fn controls(&mut self, ui: &mut egui::Ui, loop_state: LoopState, layers: usize) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);

            // Transport.
            let (record_label, hot) = match loop_state {
                // Words only: egui's built-in fonts have no ● or ↶, and a red
                // fill says "recording" plainer than any symbol would.
                LoopState::Idle => ("record", false),
                LoopState::Armed => ("armed", true),
                LoopState::Recording => ("end early", true),
                LoopState::Playing | LoopState::Stopped => ("add layer", false),
                LoopState::Overdub => ("stop recording", true),
            };
            let full = matches!(loop_state, LoopState::Playing | LoopState::Stopped)
                && layers >= MAX_LAYERS;
            if self.control(ui, Action::Record, record_label, !full, hot.then_some(HOT)) {
                self.do_action(Action::Record);
            }
            let running = loop_state.is_running();
            let can_play = running || loop_state == LoopState::Stopped;
            let play_label = if running { "■ stop" } else { "▶ play" };
            if self.control(ui, Action::PlayStop, play_label, can_play, None) {
                self.do_action(Action::PlayStop);
            }
            let can_undo = loop_state != LoopState::Idle;
            if self.control(ui, Action::Undo, "undo layer", can_undo, None) {
                self.do_action(Action::Undo);
            }
            if self.control(ui, Action::Clear, "clear all", loop_state.has_loop(), None) {
                self.do_action(Action::Clear);
            }

            ui.separator();

            // Length and tempo belong to the loop: chosen before recording,
            // then fixed until it is gone.
            let free = !loop_state.has_loop();
            let scope = ui.add_enabled_ui(free, |ui| {
                let mut bars = self.bars;
                egui::ComboBox::from_id_salt("bars")
                    .selected_text(bars_label(bars))
                    .width(64.0)
                    .show_ui(ui, |ui| {
                        for b in BAR_CHOICES {
                            ui.selectable_value(&mut bars, b, bars_label(b));
                        }
                    });
                if bars != self.bars {
                    self.bars = bars;
                    self.send(Msg::SetBars(bars));
                    self.dirty = true;
                }
                let mut bpm = self.bpm;
                ui.add(
                    egui::Slider::new(&mut bpm, arp::MIN_BPM..=arp::MAX_BPM)
                        .step_by(1.0)
                        .fixed_decimals(0)
                        .suffix(" bpm"),
                );
                if bpm != self.bpm {
                    self.bpm = bpm;
                    self.send(Msg::SetTempo(bpm));
                    self.dirty = true;
                }
            });
            if !free {
                scope
                    .response
                    .on_hover_text("a loop keeps the length and tempo it was recorded at - clear it to change them");
            }

            ui.separator();

            // Sound.
            let mut patch = self.patch_index;
            egui::ComboBox::from_id_salt("patch")
                .selected_text(PATCHES[patch].name)
                .width(80.0)
                .show_ui(ui, |ui| {
                    for (i, p) in PATCHES.iter().enumerate() {
                        ui.selectable_value(&mut patch, i, p.name);
                    }
                });
            self.set_patch(patch);

            let mut pitch = self.key_pitch;
            egui::ComboBox::from_id_salt("key")
                .selected_text(theory::NOTE_NAMES[pitch as usize])
                .width(48.0)
                .show_ui(ui, |ui| {
                    for (i, name) in theory::NOTE_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut pitch, i as i32, *name);
                    }
                });
            let mut mode = self.mode;
            egui::ComboBox::from_id_salt("mode")
                .selected_text(mode.name())
                .width(64.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut mode, Mode::Major, "major");
                    ui.selectable_value(&mut mode, Mode::Minor, "minor");
                });
            self.set_key(pitch, mode);

            if self.control(ui, Action::OctaveDown, "oct −", self.octave > OCTAVE_MIN, None) {
                self.do_action(Action::OctaveDown);
            }
            if self.control(ui, Action::OctaveUp, "oct +", self.octave < OCTAVE_MAX, None) {
                self.do_action(Action::OctaveUp);
            }

            ui.separator();

            // Arpeggiator.
            if self.control(ui, Action::Arp, "arp", true, self.arp_on.then_some(LAMP)) {
                self.do_action(Action::Arp);
            }
            let mut pattern = self.arp_pattern;
            egui::ComboBox::from_id_salt("pattern")
                .selected_text(pattern_label(pattern))
                .width(72.0)
                .show_ui(ui, |ui| {
                    for p in [ArpPattern::Up, ArpPattern::Down, ArpPattern::UpDown] {
                        ui.selectable_value(&mut pattern, p, pattern_label(p));
                    }
                });
            if pattern != self.arp_pattern {
                self.arp_pattern = pattern;
                self.send(Msg::SetArpPattern(pattern));
                self.dirty = true;
            }

            ui.separator();

            let label = if self.rebinding { "done" } else { "rebind keys" };
            let mut button =
                egui::Button::new(RichText::new(label).color(if self.rebinding { Color32::BLACK } else { INK }));
            if self.rebinding {
                button = button.fill(LAMP);
            }
            if ui.add(button).clicked() {
                self.set_rebinding(!self.rebinding);
            }
        });
    }

    /// One button per layer: its number, its sound, whether it arpeggiates.
    /// Click to mute it, × to delete it.
    fn layers(&mut self, ui: &mut egui::Ui, status: Option<&Status>) {
        let layers = status.map(|s| s.layers()).unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            ui.label(RichText::new("LAYERS").monospace().color(INK_DIM));
            if layers.is_empty() {
                ui.label(
                    RichText::new(
                        "each thing you record becomes a layer here, keeping the sound you played it with",
                    )
                    .color(INK_DIM),
                );
                return;
            }
            for (k, layer) in layers.iter().enumerate() {
                let mut text = format!("{}  {}", k + 1, PATCHES[layer.patch.min(PATCHES.len() - 1)].name);
                if layer.arp {
                    text += " · arp";
                }
                let (fill, ink, hint) = if layer.recording {
                    (HOT, Color32::BLACK, "recording")
                } else if layer.muted {
                    (SHELL_LO, INK_DIM, "muted - click to hear it again")
                } else {
                    (SHELL_HI, INK, "click to mute")
                };
                let mut label = RichText::new(text).color(ink);
                if layer.muted {
                    label = label.strikethrough();
                }
                let button = ui.add(egui::Button::new(label).fill(fill)).on_hover_text(hint);
                if button.clicked() && !layer.recording {
                    self.send(Msg::ToggleMute(k as u8));
                }
                let delete = ui
                    .add(egui::Button::new(RichText::new("×").color(INK_DIM)).frame(false))
                    .on_hover_text("delete this layer");
                if delete.clicked() {
                    self.send(Msg::RemoveLayer(k as u8));
                }
            }
        });
    }

    fn pads(&mut self, ui: &mut egui::Ui, status: Option<&Status>) {
        let full = ui.available_rect_before_wrap();
        let bar_h = 6.0;
        let plate = Rect::from_min_max(full.min, egui::pos2(full.max.x, full.max.y - bar_h - 10.0));
        let inner = plate.shrink(12.0);
        let gap = 10.0;
        let w = (inner.width() - gap * 6.0) / 7.0;
        for degree in 0..7 {
            let x = inner.left() + degree as f32 * (w + gap);
            self.pad_rects[degree] = Rect::from_min_size(
                egui::pos2(x, inner.top()),
                egui::vec2(w, inner.height() - TRAVEL),
            );
        }

        let touching = self.handle_touches(ui);
        if !touching {
            self.handle_pointer(ui);
        }

        let painter = ui.painter().clone();
        painter.rect_filled(plate, 16.0, PLATE);

        for degree in 0..7 {
            let rect = self.pad_rects[degree];
            let lit = self.is_lit(degree);
            let looping = status.map(|s| s.pad_looping(degree)).unwrap_or(false);
            let targeted = self.rebinding && self.binding_target == Some(degree);
            let hue = degree as f32 * 360.0 / 7.0;

            // The side wall a cap stands on shows below it while it is up;
            // pressed, the cap drops onto it and the wall disappears.
            let cap = if lit { rect.translate(egui::vec2(0.0, TRAVEL)) } else { rect };
            if !lit {
                painter.rect_filled(rect.translate(egui::vec2(0.0, TRAVEL)), 12.0, hsl(hue, 0.35, 0.18));
            }
            if looping || targeted {
                painter.rect_filled(cap.expand(3.0), 14.0, LAMP.gamma_multiply(if targeted { 1.0 } else { 0.75 }));
            }
            let (s, l) = if lit {
                (0.94, 0.62)
            } else if looping {
                (0.60, 0.48)
            } else {
                (0.33, 0.37)
            };
            painter.rect_filled(cap, 12.0, hsl(hue, s, l));
            // Light catching the top edge of the cap.
            painter.rect_filled(
                Rect::from_min_size(cap.min + egui::vec2(10.0, 3.0), egui::vec2(cap.width() - 20.0, 2.0)),
                1.0,
                Color32::from_white_alpha(if lit { 90 } else { 45 }),
            );

            let chord = &self.chords[degree];
            let center = cap.center();
            painter.text(
                center + egui::vec2(0.0, -28.0),
                egui::Align2::CENTER_CENTER,
                theory::roman_numeral(chord, degree),
                FontId::proportional(32.0),
                INK,
            );
            painter.text(
                center + egui::vec2(0.0, 6.0),
                egui::Align2::CENTER_CENTER,
                theory::chord_name(chord),
                FontId::proportional(17.0),
                INK,
            );
            painter.text(
                center + egui::vec2(0.0, 28.0),
                egui::Align2::CENTER_CENTER,
                chord.iter().map(|&n| theory::note_name(n)).collect::<Vec<_>>().join(" "),
                FontId::monospace(12.0),
                INK.gamma_multiply(0.65),
            );
            let hint = if targeted {
                "press a key".to_string()
            } else {
                keys_label(&self.bindings[degree])
            };
            painter.text(
                egui::pos2(center.x, cap.bottom() - 18.0),
                egui::Align2::CENTER_CENTER,
                if hint.is_empty() { "--".to_string() } else { hint },
                FontId::monospace(11.0),
                INK.gamma_multiply(0.55),
            );
        }

        // Where the loop is, so playing in time is possible at all - red while
        // recording, amber while playing - with a notch at every bar line.
        let bar = Rect::from_min_max(
            egui::pos2(full.left() + 4.0, full.bottom() - bar_h),
            egui::pos2(full.right() - 4.0, full.bottom()),
        );
        painter.rect_filled(bar, 3.0, SHELL_LO);
        if let Some(s) = status {
            let state = s.loop_state();
            let (fraction, colour) = match state {
                LoopState::Playing => (s.loop_position(), LAMP),
                LoopState::Overdub | LoopState::Recording => (s.loop_position(), HOT),
                _ => (0.0, LAMP),
            };
            if fraction > 0.0 {
                let filled = Rect::from_min_size(bar.min, egui::vec2(bar.width() * fraction, bar.height()));
                painter.rect_filled(filled, 3.0, colour);
            }
            let bars = (if state.has_loop() { s.loop_bars() } else { self.bars }) as usize;
            for b in 1..bars {
                let x = bar.left() + bar.width() * b as f32 / bars as f32;
                painter.line_segment(
                    [egui::pos2(x, bar.top()), egui::pos2(x, bar.bottom())],
                    egui::Stroke::new(2.0, PLATE),
                );
            }
        }
    }
}

fn bars_label(bars: u8) -> String {
    if bars == 1 { "1 bar".into() } else { format!("{bars} bars") }
}

fn pattern_label(p: ArpPattern) -> &'static str {
    match p {
        ArpPattern::Up => "up",
        ArpPattern::Down => "down",
        ArpPattern::UpDown => "up-down",
    }
}

fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let (r, g, b) = hsl_to_rgb(h, s, l);
    Color32::from_rgb(r, g, b)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = (h % 360.0) / 360.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h * 6.0) as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = SHELL;
    v.window_fill = SHELL_LO;
    v.extreme_bg_color = PLATE;
    v.override_text_color = Some(INK);
    v.selection.bg_fill = LAMP.gamma_multiply(0.6);
    v.widgets.inactive.weak_bg_fill = SHELL_LO;
    v.widgets.inactive.bg_fill = SHELL_LO;
    v.widgets.hovered.weak_bg_fill = SHELL_HI;
    v.widgets.hovered.bg_fill = SHELL_HI;
    v.widgets.active.weak_bg_fill = SHELL_HI;
    v.widgets.active.bg_fill = SHELL_HI;
    ctx.set_visuals(v);
}

impl eframe::App for Musika {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_keys(&ctx);

        let status = self.engine.as_ref().map(|e| e.status.clone());
        let loop_state = status.as_ref().map(|s| s.loop_state()).unwrap_or(LoopState::Idle);

        let (legend, legend_colour) = self.legend(status.as_deref());
        egui::Panel::top("legend").show(ui, |ui| {
            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(legend).monospace().color(legend_colour));
            });
            ui.add_space(6.0);
        });

        let layer_count = status.as_ref().map(|s| s.layers().len()).unwrap_or(0);
        egui::Panel::bottom("controls").show(ui, |ui| {
            ui.add_space(8.0);
            self.controls(ui, loop_state, layer_count);
            ui.add_space(8.0);
        });
        // Added after the controls, so it sits just above them.
        egui::Panel::bottom("layers").show(ui, |ui| {
            ui.add_space(6.0);
            self.layers(ui, status.as_deref());
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ui, |ui| self.pads(ui, status.as_deref()));

        // Write settings once a change settles - not on every frame of a tempo
        // slider drag.
        if self.dirty && !ctx.input(|i| i.pointer.any_down()) {
            let _ = self.settings().save();
            self.dirty = false;
        }

        // Redraw continuously only while something is actually moving. egui
        // already wakes for every key press, key release and focus change, so
        // held keys do not need a redraw loop to be noticed - and redrawing
        // flat out cost an idle instrument about a sixth of a CPU core. A
        // playing loop does animate: the bar moves and pads light as it plays
        // them. Otherwise, glance at the audio thread's status a few times a
        // second in case it changed something on its own.
        if loop_state.is_running() || loop_state == LoopState::Recording {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

// --- the command line ---------------------------------------------------------

/// Minimal 16-bit stereo WAV. No dependency: a WAV is a 44-byte header and
/// then the samples, and everything here is `to_le_bytes`.
fn write_wav(path: &str, samples: &[f32], sample_rate: u32) -> Result<(), String> {
    let data_bytes = samples.len() as u32 * 2;
    let mut out: Vec<u8> = Vec::with_capacity(44 + data_bytes as usize);

    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // 1 = uncompressed PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // stereo
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 4).to_le_bytes()); // bytes per second
    out.extend_from_slice(&4u16.to_le_bytes()); // bytes per frame
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }

    std::fs::write(path, out).map_err(|e| format!("could not write {path}: {e}"))
}

/// Play I-V-vi-IV through every patch in turn and write it to a WAV.
fn render_demo(path: &str) -> Result<(), String> {
    const SR: f32 = 48_000.0;
    const HOLD: f32 = 1.15;

    // C major down an octave, the register the app starts in.
    let chords = theory::chords_in_key(48, &theory::MAJOR_SCALE);
    let progression = [0usize, 4, 5, 3];
    let tail = 1.6; // room for the release and the reverb behind it
    let per_patch = progression.len() as f32 * HOLD + tail;

    let mut all: Vec<f32> = Vec::new();
    for (index, patch) in PATCHES.iter().enumerate() {
        let mut events = Vec::new();
        for (i, &degree) in progression.iter().enumerate() {
            let t = i as f32 * HOLD;
            events.push((
                t,
                Msg::NoteOn { id: i as u64, chord: Chord::new(&chords[degree]), degree: degree as u8 },
            ));
            // Let go just before the next chord, as a person playing would.
            events.push((t + HOLD * 0.94, Msg::NoteOff { id: i as u64 }));
        }
        println!("rendering '{}'...", patch.name);
        all.extend_from_slice(&engine::render(index, SR, per_patch, events));
    }

    write_wav(path, &all, SR as u32)?;
    let secs = all.len() as f32 / 2.0 / SR;
    let order: Vec<&str> = PATCHES.iter().map(|p| p.name).collect();
    println!("wrote {path}  ({secs:.1}s, {:.1}s each)", per_patch);
    println!("order: {}", order.join(", "));
    Ok(())
}

/// The window, title bar, Alt-Tab and taskbar icon.
///
/// Drawn by tools/make-icons.py - the same generator that makes the .ico
/// embedded into the exe - so the running window can never disagree with the
/// Desktop and Start Menu shortcuts.
fn app_icon() -> egui::IconData {
    const RGBA: &[u8] = include_bytes!("../../icons/musika-64.rgba");
    egui::IconData { rgba: RGBA.to_vec(), width: 64, height: 64 }
}

fn main() -> eframe::Result<()> {
    // `musika --probe` opens the audio device, reports what it actually got,
    // and exits. "The window appeared" is not the same as "the sound card said
    // yes", and this is the difference.
    if std::env::args().any(|a| a == "--probe") {
        match Engine::start(1) {
            Ok(e) => {
                println!("device      {}", e.device_name);
                println!("sample rate {} Hz", e.sample_rate);
                match (e.buffer_frames, e.buffer_latency_ms()) {
                    (Some(n), Some(ms)) => println!("buffer      {n} frames ({ms:.2} ms)"),
                    _ => println!("buffer      device default"),
                }
                println!("status      audio stream running");
            }
            Err(err) => {
                println!("status      FAILED: {err}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // `musika --render out.wav` writes a demo of every patch and exits.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--render") {
        let path = args.get(i + 1).cloned().unwrap_or_else(|| "musika-demo.wav".into());
        if let Err(e) = render_demo(&path) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 600.0])
            .with_min_inner_size([760.0, 420.0])
            .with_icon(std::sync::Arc::new(app_icon()))
            .with_title("Musika"),
        ..Default::default()
    };
    eframe::run_native("Musika", options, Box::new(|cc| Ok(Box::new(Musika::new(cc)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_a_default_binding_slot() {
        assert_eq!(default_bindings().len(), ACTIONS.len());
    }

    #[test]
    fn no_key_starts_bound_to_two_things() {
        let all: Vec<Key> = default_bindings().into_iter().flatten().collect();
        let mut unique = all.clone();
        unique.sort_by_key(|k| *k as u32);
        unique.dedup();
        assert_eq!(all.len(), unique.len(), "a default key is bound twice");
    }

    #[test]
    fn default_keys_survive_being_saved_by_name() {
        // Bindings are stored as key names; one that did not read back would
        // silently vanish on the next launch.
        for key in default_bindings().into_iter().flatten() {
            assert_eq!(Key::from_name(key.name()), Some(key), "{key:?} does not round-trip");
        }
    }

    #[test]
    fn rebinding_steals_the_key_from_whatever_had_it() {
        let mut b = default_bindings();
        let arp = Action::Arp.index();
        bind(&mut b, arp, Key::A); // A was pad 1's
        assert_eq!(b[arp], vec![Key::A]);
        assert_eq!(b[0], vec![Key::Num1], "pad 1 should have lost A but kept 1");
    }

    #[test]
    fn bindings_saved_and_loaded_come_back_the_same() {
        let mut s = Settings::default();
        let mut b = default_bindings();
        bind(&mut b, Action::Clear.index(), Key::Delete);
        s.bindings = ACTIONS
            .iter()
            .zip(&b)
            .map(|(a, keys)| (a.name().to_string(), keys.iter().map(|k| k.name().to_string()).collect()))
            .collect();
        let reloaded = bindings_from(&Settings::parse(&s.to_text()));
        assert_eq!(reloaded, b);
    }

    #[test]
    fn input_ids_never_collide_with_the_engine_s_own() {
        let sources = [
            Source::Key(Key::Z),
            Source::Key(Key::Num9),
            Source::Pointer,
            Source::Touch(u64::MAX),
        ];
        let mut seen = std::collections::HashSet::new();
        for s in sources {
            for degree in 0..7 {
                let id = s.id(degree);
                assert!(id < engine::AUTO_ID_BASE, "{s:?} id {id} is in the engine's range");
                assert!(seen.insert(id), "{s:?} pad {degree} collides");
            }
        }
    }

    #[test]
    fn the_embedded_window_icon_is_the_right_size() {
        assert_eq!(app_icon().rgba.len(), 64 * 64 * 4);
    }
}

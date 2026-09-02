//! Heptad (native) - a seven-chord organ.
//!
//! Same instrument as the web version in `../index.html`, rebuilt on a real
//! audio backend. Seven pads, each a whole chord, all seven belonging to one
//! key, so there is no wrong note to hit.
//!
//! Three files:
//!   theory.rs  the music - pure integer arithmetic, ported 1:1 from the JS
//!   engine.rs  the sound - cpal, and the one clock that is the audio itself
//!   main.rs    the instrument - egui window, pads, keyboard
//!
//! Run it with `cargo run --release`. Debug builds work but the audio thread
//! has far less headroom, so use release if you hear crackling.

mod engine;
mod theory;

use eframe::egui;
use engine::{Engine, Msg};
use theory::Mode;

/// Default keys, matching the web build: home row to play with, number row
/// because the pads are labelled I..vii and sometimes the thing in your head is
/// "chord five" rather than "the G key".
const HOME_ROW: [egui::Key; 7] = [
    egui::Key::A,
    egui::Key::S,
    egui::Key::D,
    egui::Key::F,
    egui::Key::G,
    egui::Key::H,
    egui::Key::J,
];
const NUMBER_ROW: [egui::Key; 7] = [
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
    egui::Key::Num5,
    egui::Key::Num6,
    egui::Key::Num7,
];

/// Where a note-on came from, so releasing one input cannot silence another.
/// The web version learned this the hard way: holding a pad with both its keys
/// and letting go of one must not stop the chord.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Source {
    Key(usize),   // index into HOME_ROW / NUMBER_ROW
    Number(usize),
    Pointer,
}

impl Source {
    /// A stable id the audio thread can match a NoteOff against.
    fn id(self, degree: usize) -> u64 {
        let slot = match self {
            Source::Key(i) => i,
            Source::Number(i) => 100 + i,
            Source::Pointer => 200,
        };
        (slot as u64) << 8 | degree as u64
    }
}

struct Heptad {
    engine: Option<Engine>,
    error: Option<String>,

    key_pitch: i32, // 0-11, 0 = C
    mode: Mode,
    octave: i32,
    chords: [[i32; 3]; 7],

    /// Which (source, degree) pairs are sounding right now.
    held: Vec<(Source, usize)>,
}

impl Heptad {
    fn new() -> Self {
        let (engine, error) = match Engine::start() {
            Ok(e) => (Some(e), None),
            Err(e) => (None, Some(e)),
        };
        let mut app = Heptad {
            engine,
            error,
            key_pitch: 0,
            mode: Mode::Major,
            octave: -1, // an octave below the web default, which plays nicer
            chords: [[0; 3]; 7],
            held: Vec::new(),
        };
        app.rebuild_chords();
        app
    }

    /// Keys past F# drop an octave rather than climbing, so no key lands in a
    /// shrill register - same rule as the web build.
    fn root_midi(&self) -> i32 {
        60 + if self.key_pitch > 6 {
            self.key_pitch - 12
        } else {
            self.key_pitch
        } + self.octave * 12
    }

    fn rebuild_chords(&mut self) {
        self.chords = theory::chords_in_key(self.root_midi(), &self.mode.pattern());
    }

    fn note_on(&mut self, source: Source, degree: usize) {
        if self.held.contains(&(source, degree)) {
            return;
        }
        self.held.push((source, degree));
        if let Some(e) = &self.engine {
            e.send(Msg::NoteOn {
                id: source.id(degree),
                notes: self.chords[degree].to_vec(),
            });
        }
    }

    fn note_off(&mut self, source: Source, degree: usize) {
        self.held.retain(|&h| h != (source, degree));
        if let Some(e) = &self.engine {
            e.send(Msg::NoteOff {
                id: source.id(degree),
            });
        }
    }

    fn release_all(&mut self) {
        self.held.clear();
        if let Some(e) = &self.engine {
            e.send(Msg::AllOff);
        }
    }

    /// True while any input is holding this pad - what lights it up.
    fn is_lit(&self, degree: usize) -> bool {
        self.held.iter().any(|&(_, d)| d == degree)
    }
}

/// The seven pad hues, spread evenly round the colour wheel - the same formula
/// the web build and the app icon use, so all three look like one instrument.
fn pad_color(degree: usize, lit: bool) -> egui::Color32 {
    let hue = degree as f32 * 360.0 / 7.0;
    let (s, l) = if lit { (0.94, 0.65) } else { (0.33, 0.37) };
    let (r, g, b) = hsl_to_rgb(hue, s, l);
    egui::Color32::from_rgb(r, g, b)
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

impl eframe::App for Heptad {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_keys(&ctx);

        egui::Panel::top("legend").show(ui, |ui| {
            ui.add_space(4.0);
            ui.vertical_centered(|ui| {
                if let Some(err) = &self.error {
                    ui.colored_label(egui::Color32::from_rgb(216, 52, 31), err);
                } else {
                    ui.label(self.legend());
                }
            });
            ui.add_space(4.0);
        });

        egui::Panel::bottom("controls").show(ui, |ui| {
            ui.add_space(6.0);
            self.controls(ui);
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ui, |ui| self.pads(ui));

        // Keys are polled per frame, so the window must keep drawing even when
        // nothing has moved - otherwise a held chord would not register until
        // the mouse twitched.
        ctx.request_repaint();
    }
}

impl Heptad {
    fn legend(&self) -> String {
        let key = format!(
            "{} {}",
            theory::note_name(self.root_midi()),
            self.mode.name()
        );
        let oct = if self.octave == 0 {
            String::new()
        } else {
            format!("  ·  oct {:+}", self.octave)
        };
        let latency = match self.engine.as_ref().and_then(|e| e.buffer_latency_ms()) {
            Some(ms) => format!("  ·  buffer {ms:.1}ms"),
            None => String::new(),
        };
        format!("HEPTAD  —  key of {key}{oct}{latency}")
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        // egui hands us the set of keys currently down, which is exactly the
        // hold-to-sustain model: diff it against what we are already sounding.
        let down = ctx.input(|i| i.keys_down.clone());

        for degree in 0..7 {
            for (key, source) in [
                (HOME_ROW[degree], Source::Key(degree)),
                (NUMBER_ROW[degree], Source::Number(degree)),
            ] {
                if down.contains(&key) {
                    self.note_on(source, degree);
                } else {
                    self.note_off(source, degree);
                }
            }
        }

        // Octave, matching the web build's - and = keys.
        if ctx.input(|i| i.key_pressed(egui::Key::Minus)) {
            self.set_octave(self.octave - 1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Equals)) {
            self.set_octave(self.octave + 1);
        }
    }

    fn set_octave(&mut self, octave: i32) {
        let clamped = octave.clamp(-3, 1);
        if clamped != self.octave {
            self.octave = clamped;
            self.release_all();
            self.rebuild_chords();
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            if ui.add_enabled(self.octave > -3, egui::Button::new("oct −")).clicked() {
                self.set_octave(self.octave - 1);
            }
            if ui.add_enabled(self.octave < 1, egui::Button::new("oct +")).clicked() {
                self.set_octave(self.octave + 1);
            }

            ui.separator();

            let mut pitch = self.key_pitch;
            egui::ComboBox::from_id_salt("key")
                .selected_text(theory::NOTE_NAMES[pitch as usize])
                .width(56.0)
                .show_ui(ui, |ui| {
                    for (i, name) in theory::NOTE_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut pitch, i as i32, *name);
                    }
                });

            let mut mode = self.mode;
            egui::ComboBox::from_id_salt("mode")
                .selected_text(mode.name())
                .width(76.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut mode, Mode::Major, "major");
                    ui.selectable_value(&mut mode, Mode::Minor, "minor");
                });

            if pitch != self.key_pitch || mode != self.mode {
                self.key_pitch = pitch;
                self.mode = mode;
                self.release_all();
                self.rebuild_chords();
            }

            ui.separator();
            if let Some(e) = &self.engine {
                ui.weak(format!("{} @ {:.0}Hz", e.device_name, e.sample_rate));
            }
        });
    }

    fn pads(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let gap = 8.0;
        let w = (rect.width() - gap * 6.0) / 7.0;

        // Pointer input: one finger, tracked across pads so dragging from one to
        // the next behaves like sliding along a keyboard.
        let pointer_down = ui.input(|i| i.pointer.primary_down());
        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
        let mut pointer_target: Option<usize> = None;

        for degree in 0..7 {
            let x = rect.left() + degree as f32 * (w + gap);
            let pad = egui::Rect::from_min_size(
                egui::pos2(x, rect.top()),
                egui::vec2(w, rect.height()),
            );

            if pointer_down {
                if let Some(p) = pointer_pos {
                    if pad.contains(p) {
                        pointer_target = Some(degree);
                    }
                }
            }

            let lit = self.is_lit(degree);
            let painter = ui.painter();
            painter.rect_filled(
                pad.shrink(if lit { 2.0 } else { 0.0 }),
                10.0,
                pad_color(degree, lit),
            );

            let chord = &self.chords[degree];
            let cx = pad.center().x;
            let cy = pad.center().y;
            let ink = egui::Color32::from_rgb(238, 241, 246);

            painter.text(
                egui::pos2(cx, cy - 26.0),
                egui::Align2::CENTER_CENTER,
                theory::roman_numeral(chord, degree),
                egui::FontId::proportional(30.0),
                ink,
            );
            painter.text(
                egui::pos2(cx, cy + 6.0),
                egui::Align2::CENTER_CENTER,
                theory::chord_name(chord),
                egui::FontId::proportional(17.0),
                ink,
            );
            painter.text(
                egui::pos2(cx, cy + 28.0),
                egui::Align2::CENTER_CENTER,
                chord
                    .iter()
                    .map(|&n| theory::note_name(n))
                    .collect::<Vec<_>>()
                    .join(" "),
                egui::FontId::monospace(12.0),
                ink.gamma_multiply(0.65),
            );
            painter.text(
                egui::pos2(cx, cy + 50.0),
                egui::Align2::CENTER_CENTER,
                format!("{} / {}", pad_key_label(degree), degree + 1),
                egui::FontId::monospace(11.0),
                ink.gamma_multiply(0.5),
            );
        }

        // Apply the pointer last, so moving between pads releases the old one.
        let currently = self.held.iter().find(|&&(s, _)| s == Source::Pointer).map(|&(_, d)| d);
        if currently != pointer_target {
            if let Some(d) = currently {
                self.note_off(Source::Pointer, d);
            }
            if let Some(d) = pointer_target {
                self.note_on(Source::Pointer, d);
            }
        }
    }
}

fn pad_key_label(degree: usize) -> &'static str {
    ["A", "S", "D", "F", "G", "H", "J"][degree]
}

fn main() -> eframe::Result<()> {
    // `heptad --probe` opens the audio device, reports what it actually got,
    // and exits. "The window appeared" is not the same as "the sound card said
    // yes", and this is the difference.
    if std::env::args().any(|a| a == "--probe") {
        match Engine::start() {
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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 520.0])
            .with_min_inner_size([620.0, 360.0])
            .with_title("Heptad"),
        ..Default::default()
    };
    eframe::run_native(
        "Heptad",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(Heptad::new()))
        }),
    )
}

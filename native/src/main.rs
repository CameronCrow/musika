//! Heptad (native) - a seven-chord organ.
//!
//! Same instrument as the web version in `../index.html`, rebuilt on a real
//! audio backend. Seven pads, each a whole chord, all seven belonging to one
//! key, so there is no wrong note to hit.
//!
//! Five files:
//!   theory.rs  the music - pure integer arithmetic, ported 1:1 from the JS
//!   voice.rs   what one note sounds like - oscillators, envelopes, filter
//!   reverb.rs  the room it is played in
//!   engine.rs  the audio thread and the mix
//!   main.rs    the instrument - egui window, pads, keyboard
//!
//! Run it with `cargo run --release`. Debug builds work but the audio thread
//! has far less headroom, so use release if you hear crackling.

mod engine;
mod reverb;
mod theory;
mod voice;

use eframe::egui;
use engine::{Engine, Msg};
use theory::Mode;
use voice::PATCHES;

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
    patch_index: usize,
    chords: [[i32; 3]; 7],

    /// Which (source, degree) pairs are sounding right now.
    held: Vec<(Source, usize)>,
}

impl Heptad {
    fn new() -> Self {
        // Patch 1 is "warm" - detuned saws with a moving filter and a room
        // around them. Patch 0 is the bare square the engine started life with,
        // kept so the difference is audible rather than asserted.
        let patch_index = 1;
        let (engine, error) = match Engine::start(PATCHES[patch_index]) {
            Ok(e) => (Some(e), None),
            Err(e) => (None, Some(e)),
        };
        let mut app = Heptad {
            engine,
            error,
            key_pitch: 0,
            mode: Mode::Major,
            octave: -1, // an octave below the web default, which plays nicer
            patch_index,
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

            let mut patch = self.patch_index;
            egui::ComboBox::from_id_salt("patch")
                .selected_text(PATCHES[patch].name)
                .width(84.0)
                .show_ui(ui, |ui| {
                    for (i, p) in PATCHES.iter().enumerate() {
                        ui.selectable_value(&mut patch, i, p.name);
                    }
                });
            if patch != self.patch_index {
                self.patch_index = patch;
                self.release_all();
                if let Some(e) = &self.engine {
                    e.send(Msg::SetPatch(patch));
                }
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

/// The window and taskbar icon, drawn rather than loaded.
///
/// It is a picture of the instrument: seven bars in the seven pad hues, on the
/// plate colour. Generating it here costs one loop and buys two things - no
/// image-decoder dependency, and no asset file that could quietly drift out of
/// step with the pads, since it runs the same hue formula they do.
fn app_icon() -> egui::IconData {
    const N: i32 = 64;
    let inset = N as f32 * 0.10;
    let span = N as f32 - 2.0 * inset;
    let gap = span / 7.0 * 0.16;
    let bar_w = (span - gap * 6.0) / 7.0;

    // Precompute the seven bar colours rather than converting per pixel.
    let bars: Vec<(f32, (u8, u8, u8))> = (0..7)
        .map(|d| {
            let x0 = inset + d as f32 * (bar_w + gap);
            (x0, hsl_to_rgb(d as f32 * 360.0 / 7.0, 0.62, 0.52))
        })
        .collect();

    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32, y as f32);
            let mut px = (26u8, 29u8, 34u8); // the plate the pads sit in
            if fy >= inset && fy < N as f32 - inset {
                for &(x0, colour) in &bars {
                    if fx >= x0 && fx < x0 + bar_w {
                        px = colour;
                    }
                }
            }
            rgba.extend_from_slice(&[px.0, px.1, px.2, 255]);
        }
    }

    egui::IconData {
        rgba,
        width: N as u32,
        height: N as u32,
    }
}

fn pad_key_label(degree: usize) -> &'static str {
    ["A", "S", "D", "F", "G", "H", "J"][degree]
}

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
///
/// A synth patch is judged by ear, not by assertion, and this is how you hear
/// all four back to back without launching anything - including "raw", which is
/// what the instrument sounded like before it had a voice worth the name.
fn render_demo(path: &str) -> Result<(), String> {
    const SR: f32 = 48_000.0;
    const HOLD: f32 = 1.15;

    // C major down an octave, the register the app actually starts in.
    let chords = theory::chords_in_key(48, &theory::MAJOR_SCALE);
    // The progression in the README: I, V, vi, IV.
    let progression = [0usize, 4, 5, 3];
    let tail = 2.2; // room for the release and the reverb behind it
    let per_patch = progression.len() as f32 * HOLD + tail;

    let mut all: Vec<f32> = Vec::new();
    for patch in PATCHES.iter() {
        let mut events = Vec::new();
        for (i, &degree) in progression.iter().enumerate() {
            let t = i as f32 * HOLD;
            events.push((t, Msg::NoteOn { id: i as u64, notes: chords[degree].to_vec() }));
            // Let go just before the next chord, so they overlap slightly the
            // way a person playing would.
            events.push((t + HOLD * 0.94, Msg::NoteOff { id: i as u64 }));
        }
        println!("rendering '{}'...", patch.name);
        all.extend_from_slice(&engine::render(*patch, SR, per_patch, events));
    }

    write_wav(path, &all, SR as u32)?;
    let secs = all.len() as f32 / 2.0 / SR;
    println!("wrote {path}  ({secs:.1}s, order: raw, warm, chime, lo-fi)");
    Ok(())
}

fn main() -> eframe::Result<()> {
    // `heptad --probe` opens the audio device, reports what it actually got,
    // and exits. "The window appeared" is not the same as "the sound card said
    // yes", and this is the difference.
    if std::env::args().any(|a| a == "--probe") {
        match Engine::start(PATCHES[1]) {
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

    // `heptad --render out.wav` writes a demo of every patch and exits.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--render") {
        let path = args.get(i + 1).cloned().unwrap_or_else(|| "heptad-demo.wav".into());
        if let Err(e) = render_demo(&path) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 520.0])
            .with_min_inner_size([620.0, 360.0])
            .with_icon(std::sync::Arc::new(app_icon()))
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

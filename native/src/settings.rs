//! settings.rs - what Musika remembers between launches.
//!
//! A plain text file of `name = value` lines at %APPDATA%\Musika\settings.txt.
//! Not JSON and no serde: there are a dozen values, and a format a person can
//! open in Notepad and fix by hand is worth more here than a dependency.
//!
//! Loading is forgiving on purpose. A missing file, a garbled line, an unknown
//! name or an out-of-range number each fall back to the default for that one
//! value, because a corrupt settings file must never be the reason the
//! instrument will not open.

use std::path::PathBuf;

use crate::arp::{MAX_BPM, MIN_BPM};
use crate::theory::{ArpPattern, Mode};

pub const OCTAVE_MIN: i32 = -3;
pub const OCTAVE_MAX: i32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub key_pitch: i32,
    pub mode: Mode,
    pub octave: i32,
    pub patch: String,
    pub arp_on: bool,
    pub arp_pattern: ArpPattern,
    pub bpm: f32,
    /// Action name -> key names, for each action the file mentions. An action
    /// that is missing gets its default keys; one listed with no keys stays
    /// deliberately unbound.
    pub bindings: Vec<(String, Vec<String>)>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            key_pitch: 0,
            mode: Mode::Major,
            // An octave below middle C: the register that sits comfortably
            // under a melody rather than on top of it.
            octave: -1,
            patch: "warm".into(),
            arp_on: false,
            arp_pattern: ArpPattern::Up,
            bpm: 120.0,
            bindings: Vec::new(),
        }
    }
}

pub fn pattern_name(p: ArpPattern) -> &'static str {
    match p {
        ArpPattern::Up => "up",
        ArpPattern::Down => "down",
        ArpPattern::UpDown => "updown",
    }
}

fn pattern_from(name: &str) -> Option<ArpPattern> {
    match name {
        "up" => Some(ArpPattern::Up),
        "down" => Some(ArpPattern::Down),
        "updown" => Some(ArpPattern::UpDown),
        _ => None,
    }
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("Musika").join("settings.txt"))
    }

    pub fn load() -> Settings {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_text())
    }

    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let (name, value) = (name.trim(), value.trim());
            match name {
                "key" => {
                    if let Ok(v) = value.parse::<i32>() {
                        if (0..12).contains(&v) {
                            s.key_pitch = v;
                        }
                    }
                }
                "mode" => match value {
                    "major" => s.mode = Mode::Major,
                    "minor" => s.mode = Mode::Minor,
                    _ => {}
                },
                "octave" => {
                    if let Ok(v) = value.parse::<i32>() {
                        s.octave = v.clamp(OCTAVE_MIN, OCTAVE_MAX);
                    }
                }
                "patch" => {
                    if !value.is_empty() {
                        s.patch = value.to_string();
                    }
                }
                "arp" => match value {
                    "on" => s.arp_on = true,
                    "off" => s.arp_on = false,
                    _ => {}
                },
                "arp_pattern" => {
                    if let Some(p) = pattern_from(value) {
                        s.arp_pattern = p;
                    }
                }
                "bpm" => {
                    if let Ok(v) = value.parse::<f32>() {
                        if v.is_finite() {
                            s.bpm = v.clamp(MIN_BPM, MAX_BPM);
                        }
                    }
                }
                _ => {
                    if let Some(action) = name.strip_prefix("bind.") {
                        let keys = value
                            .split(',')
                            .map(str::trim)
                            .filter(|k| !k.is_empty())
                            .map(String::from)
                            .collect();
                        s.bindings.retain(|(a, _)| a != action);
                        s.bindings.push((action.to_string(), keys));
                    }
                }
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from(
            "# Musika settings. Safe to edit by hand; delete this file to reset.\n",
        );
        out += &format!("key = {}\n", self.key_pitch);
        out += &format!("mode = {}\n", self.mode.name());
        out += &format!("octave = {}\n", self.octave);
        out += &format!("patch = {}\n", self.patch);
        out += &format!("arp = {}\n", if self.arp_on { "on" } else { "off" });
        out += &format!("arp_pattern = {}\n", pattern_name(self.arp_pattern));
        out += &format!("bpm = {}\n", self.bpm.round());
        for (action, keys) in &self.bindings {
            out += &format!("bind.{action} = {}\n", keys.join(", "));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_a_round_trip() {
        let s = Settings {
            key_pitch: 9,
            mode: Mode::Minor,
            octave: -2,
            patch: "lo-fi".into(),
            arp_on: true,
            arp_pattern: ArpPattern::UpDown,
            bpm: 96.0,
            bindings: vec![
                ("pad1".into(), vec!["A".into(), "1".into()]),
                ("clear".into(), vec![]),
            ],
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
    }

    #[test]
    fn an_empty_or_missing_file_gives_the_defaults() {
        assert_eq!(Settings::parse(""), Settings::default());
    }

    #[test]
    fn garbage_falls_back_one_value_at_a_time() {
        // Every line here is broken in a different way. The good one in the
        // middle must still land; the rest must leave their defaults alone.
        let s = Settings::parse(
            "key = 14\nmode = sideways\nthis line has no equals\noctave = 99\n\
             patch = bell\nbpm = NaN\narp_pattern = diagonal\narp = maybe\n",
        );
        let d = Settings::default();
        assert_eq!(s.key_pitch, d.key_pitch, "key 14 is not a key");
        assert_eq!(s.mode, d.mode);
        assert_eq!(s.octave, OCTAVE_MAX, "octave is clamped, not rejected");
        assert_eq!(s.patch, "bell");
        assert_eq!(s.bpm, d.bpm);
        assert_eq!(s.arp_pattern, d.arp_pattern);
        assert_eq!(s.arp_on, d.arp_on);
    }

    #[test]
    fn tempo_is_clamped_into_the_arpeggiator_s_range() {
        assert_eq!(Settings::parse("bpm = 5").bpm, MIN_BPM);
        assert_eq!(Settings::parse("bpm = 9000").bpm, MAX_BPM);
    }

    #[test]
    fn an_unbound_action_stays_unbound_and_unmentioned_ones_are_not_invented() {
        let s = Settings::parse("bind.clear =\n");
        assert_eq!(s.bindings, vec![("clear".to_string(), vec![])]);
    }
}

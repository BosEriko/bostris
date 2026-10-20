use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::game::Settings;
use crate::input::{Action, Keymap, parse_key};

pub const APP_DIR: &str = "bostris";
const CONFIG_FILE: &str = "config.toml";

pub const DEFAULT_CONFIG: &str = r#"# bostris configuration

[gameplay]
# Level to start on (1-30)
start_level = 1
# Delayed Auto Shift: how long to hold left/right before auto-repeat starts (ms, 0-1000)
das_ms = 167
# Auto Repeat Rate: delay between auto-repeated moves (ms, 0-500; 0 moves instantly to the wall)
arr_ms = 33
# Soft drop speed as a multiple of gravity (1-100)
soft_drop_factor = 20
# Time a grounded piece waits before locking (ms, 100-5000)
lock_delay_ms = 500
# Moves or rotations that may reset the lock delay (0-100)
max_lock_resets = 15
# Duration of the line clear animation (ms, 0-1000)
line_clear_ms = 250
# Number of upcoming pieces shown (0-6)
preview_count = 5
# Show where the current piece will land
ghost = true

[display]
# "auto", "truecolor", "256" or "16"
color_mode = "auto"

[keys]
# Key names: Left, Right, Up, Down, Space, Esc, Enter, Tab, Backspace,
# Home, End, PageUp, PageDown, Insert, Delete, F1-F12 or a single character.
left = ["Left"]
right = ["Right"]
soft_drop = ["Down"]
hard_drop = ["Space"]
rotate_cw = ["Up", "x"]
rotate_ccw = ["z"]
hold = ["c"]
pause = ["p", "Esc"]
restart = ["r"]
quit = ["q"]
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    TrueColor,
    Ansi256,
    Ansi16,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub settings: Settings,
    pub das: Duration,
    pub arr: Duration,
    pub preview_count: usize,
    pub ghost: bool,
    pub color_mode: ColorMode,
    pub keymap: Keymap,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            das: Duration::from_millis(167),
            arr: Duration::from_millis(33),
            preview_count: 5,
            ghost: true,
            color_mode: ColorMode::Auto,
            keymap: Keymap::defaults(),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawConfig {
    gameplay: RawGameplay,
    display: RawDisplay,
    keys: BTreeMap<String, KeyList>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawGameplay {
    start_level: Option<i64>,
    das_ms: Option<i64>,
    arr_ms: Option<i64>,
    soft_drop_factor: Option<f64>,
    lock_delay_ms: Option<i64>,
    max_lock_resets: Option<i64>,
    line_clear_ms: Option<i64>,
    preview_count: Option<i64>,
    ghost: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawDisplay {
    color_mode: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum KeyList {
    One(String),
    Many(Vec<String>),
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(APP_DIR).join(CONFIG_FILE))
}

pub fn load() -> (Config, Vec<String>) {
    let Some(path) = config_path() else {
        return (Config::default(), Vec::new());
    };
    match fs::read_to_string(&path) {
        Ok(text) => parse(&text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            write_default(&path);
            (Config::default(), Vec::new())
        }
        Err(err) => (
            Config::default(),
            vec![format!("could not read {}: {err}", path.display())],
        ),
    }
}

fn write_default(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, DEFAULT_CONFIG);
}

fn ranged(
    warnings: &mut Vec<String>,
    name: &str,
    value: Option<i64>,
    min: i64,
    max: i64,
    default: i64,
) -> i64 {
    match value {
        None => default,
        Some(v) if (min..=max).contains(&v) => v,
        Some(v) => {
            let clamped = v.clamp(min, max);
            warnings.push(format!(
                "{name} = {v} is outside {min}-{max}; using {clamped}"
            ));
            clamped
        }
    }
}

fn millis(value: i64) -> Duration {
    Duration::from_millis(value as u64)
}

pub fn parse(text: &str) -> (Config, Vec<String>) {
    let mut warnings = Vec::new();
    let raw: RawConfig = match toml::from_str(text) {
        Ok(raw) => raw,
        Err(err) => {
            warnings.push(format!(
                "invalid configuration, using defaults: {}",
                err.message()
            ));
            return (Config::default(), warnings);
        }
    };
    let defaults = Config::default();
    let g = raw.gameplay;
    let w = &mut warnings;

    let soft_drop_factor = match g.soft_drop_factor {
        None => defaults.settings.soft_drop_factor,
        Some(v) if v.is_finite() && (1.0..=100.0).contains(&v) => v,
        Some(v) => {
            let clamped = if v.is_finite() {
                v.clamp(1.0, 100.0)
            } else {
                20.0
            };
            w.push(format!(
                "soft_drop_factor = {v} is outside 1-100; using {clamped}"
            ));
            clamped
        }
    };

    let settings = Settings {
        start_level: ranged(w, "start_level", g.start_level, 1, 30, 1) as u32,
        lock_delay: millis(ranged(w, "lock_delay_ms", g.lock_delay_ms, 100, 5000, 500)),
        max_lock_resets: ranged(w, "max_lock_resets", g.max_lock_resets, 0, 100, 15) as u32,
        line_clear_delay: millis(ranged(w, "line_clear_ms", g.line_clear_ms, 0, 1000, 250)),
        soft_drop_factor,
    };

    let das = millis(ranged(w, "das_ms", g.das_ms, 0, 1000, 167));
    let arr = millis(ranged(w, "arr_ms", g.arr_ms, 0, 500, 33));
    let preview_count = ranged(w, "preview_count", g.preview_count, 0, 6, 5) as usize;

    let color_mode = match raw
        .display
        .color_mode
        .as_deref()
        .map(str::to_ascii_lowercase)
    {
        None => ColorMode::Auto,
        Some(mode) => match mode.as_str() {
            "auto" => ColorMode::Auto,
            "truecolor" | "24bit" => ColorMode::TrueColor,
            "256" => ColorMode::Ansi256,
            "16" | "basic" => ColorMode::Ansi16,
            other => {
                w.push(format!(
                    "color_mode = \"{other}\" is not recognised; using auto"
                ));
                ColorMode::Auto
            }
        },
    };

    let keymap = build_keymap(raw.keys, w);

    (
        Config {
            settings,
            das,
            arr,
            preview_count,
            ghost: g.ghost.unwrap_or(true),
            color_mode,
            keymap,
        },
        warnings,
    )
}

fn build_keymap(mut keys: BTreeMap<String, KeyList>, warnings: &mut Vec<String>) -> Keymap {
    let mut keymap = Keymap::default();
    for action in Action::ALL {
        let names: Vec<String> = match keys.remove(action.name()) {
            Some(KeyList::One(name)) => vec![name],
            Some(KeyList::Many(names)) => names,
            None => action
                .default_keys()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        };
        let mut bound = false;
        for name in &names {
            match parse_key(name) {
                Some(code) => {
                    keymap.bind(code, action);
                    bound = true;
                }
                None => warnings.push(format!("unknown key \"{name}\" for {}", action.name())),
            }
        }
        if !bound {
            warnings.push(format!(
                "no valid keys for {}; using defaults",
                action.name()
            ));
            for name in action.default_keys() {
                if let Some(code) = parse_key(name) {
                    keymap.bind(code, action);
                }
            }
        }
    }
    for name in keys.keys() {
        warnings.push(format!("unknown action \"{name}\" in [keys]"));
    }
    keymap
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn default_file_matches_built_in_defaults() {
        let (config, warnings) = parse(DEFAULT_CONFIG);
        assert!(warnings.is_empty(), "{warnings:?}");
        let defaults = Config::default();
        assert_eq!(config.settings, defaults.settings);
        assert_eq!(config.das, defaults.das);
        assert_eq!(config.arr, defaults.arr);
        assert_eq!(config.preview_count, defaults.preview_count);
        for action in Action::ALL {
            let a: Vec<_> = config.keymap.keys_for(action).collect();
            let b: Vec<_> = defaults.keymap.keys_for(action).collect();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn empty_config_uses_defaults() {
        let (config, warnings) = parse("");
        assert!(warnings.is_empty());
        assert_eq!(config.settings, Settings::default());
        assert_eq!(
            config.keymap.action(&press(KeyCode::Char('c'))),
            Some(Action::Hold)
        );
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let (config, warnings) = parse(
            "[gameplay]\ndas_ms = -5\narr_ms = 9000\nstart_level = 99\nsoft_drop_factor = 0.0\n",
        );
        assert_eq!(warnings.len(), 4);
        assert_eq!(config.das, Duration::ZERO);
        assert_eq!(config.arr, Duration::from_millis(500));
        assert_eq!(config.settings.start_level, 30);
        assert_eq!(config.settings.soft_drop_factor, 1.0);
    }

    #[test]
    fn malformed_config_falls_back_to_defaults() {
        let (config, warnings) = parse("[gameplay\ndas_ms = ");
        assert_eq!(warnings.len(), 1);
        assert_eq!(config.das, Config::default().das);
        let (_, warnings) = parse("[gameplay]\ndas_ms = \"fast\"\n");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn custom_keys_replace_defaults() {
        let (config, warnings) =
            parse("[keys]\nhold = \"Tab\"\nleft = [\"a\", \"Left\"]\nrotate_cw = [\"nope\"]\n");
        assert_eq!(warnings.len(), 2);
        let keymap = &config.keymap;
        assert_eq!(keymap.action(&press(KeyCode::Tab)), Some(Action::Hold));
        assert_eq!(keymap.action(&press(KeyCode::Char('c'))), None);
        assert_eq!(
            keymap.action(&press(KeyCode::Char('a'))),
            Some(Action::Left)
        );
        assert_eq!(keymap.action(&press(KeyCode::Up)), Some(Action::RotateCw));
    }

    #[test]
    fn color_mode_is_validated() {
        assert_eq!(
            parse("[display]\ncolor_mode = \"256\"").0.color_mode,
            ColorMode::Ansi256
        );
        let (config, warnings) = parse("[display]\ncolor_mode = \"rainbow\"");
        assert_eq!(config.color_mode, ColorMode::Auto);
        assert_eq!(warnings.len(), 1);
    }
}

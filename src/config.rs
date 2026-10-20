use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::bot::Level;
use crate::game::Settings;
use crate::input::{Action, Keymap, key_name, parse_key};

pub const APP_DIR: &str = "bostris";
const CONFIG_FILE: &str = "config.toml";

pub const START_LEVEL: (i64, i64) = (1, 30);
pub const DAS_MS: (i64, i64) = (0, 1000);
pub const ARR_MS: (i64, i64) = (0, 500);
pub const SOFT_DROP_FACTOR: (f64, f64) = (1.0, 100.0);
pub const LOCK_DELAY_MS: (i64, i64) = (100, 5000);
pub const MAX_LOCK_RESETS: (i64, i64) = (0, 100);
pub const LINE_CLEAR_MS: (i64, i64) = (0, 1000);
pub const PREVIEW_COUNT: (i64, i64) = (0, 6);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    TrueColor,
    Ansi256,
    Ansi16,
}

impl ColorMode {
    pub fn name(self) -> &'static str {
        match self {
            ColorMode::Auto => "auto",
            ColorMode::TrueColor => "truecolor",
            ColorMode::Ansi256 => "256",
            ColorMode::Ansi16 => "16",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub settings: Settings,
    pub das: Duration,
    pub arr: Duration,
    pub preview_count: usize,
    pub ghost: bool,
    pub color_mode: ColorMode,
    pub keymap: Keymap,
    pub bot_level: Level,
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
            bot_level: Level::APlus,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawConfig {
    gameplay: RawGameplay,
    display: RawDisplay,
    versus: RawVersus,
    keys: BTreeMap<String, KeyList>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawVersus {
    bot_level: Option<String>,
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
    finesse_retry: Option<bool>,
    finesse_180: Option<bool>,
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
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let _ = write_file(&path, &to_toml(&Config::default()));
            (Config::default(), Vec::new())
        }
        Err(err) => (
            Config::default(),
            vec![format!("could not read {}: {err}", path.display())],
        ),
    }
}

pub fn save(config: &Config) -> io::Result<()> {
    match config_path() {
        Some(path) => write_file(&path, &to_toml(config)),
        None => Ok(()),
    }
}

fn write_file(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("toml.tmp");
    fs::write(&temp, text)?;
    fs::rename(&temp, path)
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn to_toml(config: &Config) -> String {
    let s = &config.settings;
    let mut out = String::with_capacity(2048);
    let _ = write!(
        out,
        "# bostris configuration\n\
         # Edit this file or use the in-game settings screen.\n\
         \n\
         [gameplay]\n\
         # Level to start on ({}-{})\n\
         start_level = {}\n\
         # Delayed Auto Shift: how long to hold left/right before auto-repeat starts (ms, {}-{})\n\
         das_ms = {}\n\
         # Auto Repeat Rate: delay between auto-repeated moves (ms, {}-{}; 0 moves instantly to the wall)\n\
         arr_ms = {}\n\
         # Soft drop speed as a multiple of gravity ({}-{})\n\
         soft_drop_factor = {}\n\
         # Time a grounded piece waits before locking (ms, {}-{})\n\
         lock_delay_ms = {}\n\
         # Moves or rotations that may reset the lock delay ({}-{})\n\
         max_lock_resets = {}\n\
         # Duration of the line clear animation (ms, {}-{})\n\
         line_clear_ms = {}\n\
         # Number of upcoming pieces shown ({}-{})\n\
         preview_count = {}\n\
         # Show where the current piece will land\n\
         ghost = {}\n\
         # Respawn the piece instead of locking it when it was placed with extra inputs\n\
         finesse_retry = {}\n\
         # Count 180 degree rotations as a single input when judging finesse\n\
         finesse_180 = {}\n\
         \n\
         [display]\n\
         # \"auto\", \"truecolor\", \"256\" or \"16\"\n\
         color_mode = {}\n\
         \n\
         [versus]\n\
         # Bot difficulty: D, C, B, A, A+, S or X\n\
         bot_level = {}\n\
         \n\
         [keys]\n\
         # Key names: Left, Right, Up, Down, Space, Esc, Enter, Tab, Backspace,\n\
         # Home, End, PageUp, PageDown, Insert, Delete, F1-F12 or a single character.\n",
        START_LEVEL.0,
        START_LEVEL.1,
        s.start_level,
        DAS_MS.0,
        DAS_MS.1,
        config.das.as_millis(),
        ARR_MS.0,
        ARR_MS.1,
        config.arr.as_millis(),
        SOFT_DROP_FACTOR.0,
        SOFT_DROP_FACTOR.1,
        s.soft_drop_factor,
        LOCK_DELAY_MS.0,
        LOCK_DELAY_MS.1,
        s.lock_delay.as_millis(),
        MAX_LOCK_RESETS.0,
        MAX_LOCK_RESETS.1,
        s.max_lock_resets,
        LINE_CLEAR_MS.0,
        LINE_CLEAR_MS.1,
        s.line_clear_delay.as_millis(),
        PREVIEW_COUNT.0,
        PREVIEW_COUNT.1,
        config.preview_count,
        config.ghost,
        s.finesse_retry,
        s.finesse_180,
        quote(config.color_mode.name()),
        quote(config.bot_level.name()),
    );
    for action in Action::ALL {
        let names: Vec<String> = config
            .keymap
            .keys_for(action)
            .filter_map(key_name)
            .map(|name| quote(&name))
            .collect();
        let _ = writeln!(out, "{} = [{}]", action.name(), names.join(", "));
    }
    out
}

fn ranged(
    warnings: &mut Vec<String>,
    name: &str,
    value: Option<i64>,
    (min, max): (i64, i64),
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

    let (min_factor, max_factor) = SOFT_DROP_FACTOR;
    let soft_drop_factor = match g.soft_drop_factor {
        None => defaults.settings.soft_drop_factor,
        Some(v) if v.is_finite() && (min_factor..=max_factor).contains(&v) => v,
        Some(v) => {
            let clamped = if v.is_finite() {
                v.clamp(min_factor, max_factor)
            } else {
                defaults.settings.soft_drop_factor
            };
            w.push(format!(
                "soft_drop_factor = {v} is outside {min_factor}-{max_factor}; using {clamped}"
            ));
            clamped
        }
    };

    let settings = Settings {
        start_level: ranged(w, "start_level", g.start_level, START_LEVEL, 1) as u32,
        lock_delay: millis(ranged(
            w,
            "lock_delay_ms",
            g.lock_delay_ms,
            LOCK_DELAY_MS,
            500,
        )),
        max_lock_resets: ranged(w, "max_lock_resets", g.max_lock_resets, MAX_LOCK_RESETS, 15)
            as u32,
        line_clear_delay: millis(ranged(
            w,
            "line_clear_ms",
            g.line_clear_ms,
            LINE_CLEAR_MS,
            250,
        )),
        soft_drop_factor,
        finesse_retry: g.finesse_retry.unwrap_or(false),
        finesse_180: g.finesse_180.unwrap_or(false),
    };

    let das = millis(ranged(w, "das_ms", g.das_ms, DAS_MS, 167));
    let arr = millis(ranged(w, "arr_ms", g.arr_ms, ARR_MS, 33));
    let preview_count = ranged(w, "preview_count", g.preview_count, PREVIEW_COUNT, 5) as usize;

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

    let bot_level = match raw.versus.bot_level.as_deref() {
        None => defaults.bot_level,
        Some(name) => Level::parse(name).unwrap_or_else(|| {
            w.push(format!(
                "bot_level = \"{name}\" is not recognised; using A+"
            ));
            defaults.bot_level
        }),
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
            bot_level,
        },
        warnings,
    )
}

fn build_keymap(mut keys: BTreeMap<String, KeyList>, warnings: &mut Vec<String>) -> Keymap {
    let mut keymap = Keymap::default();
    let mut missing = Vec::new();
    for action in Action::ALL {
        let names = match keys.remove(action.name()) {
            Some(KeyList::One(name)) => vec![name],
            Some(KeyList::Many(names)) => names,
            None => {
                missing.push(action);
                continue;
            }
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
            missing.push(action);
        }
    }
    for action in missing {
        for code in action
            .default_keys()
            .iter()
            .filter_map(|name| parse_key(name))
        {
            if keymap.owner(code).is_none() {
                keymap.bind(code, action);
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
        let (config, warnings) = parse(&to_toml(&Config::default()));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config, Config::default());
    }

    #[test]
    fn customised_config_round_trips() {
        let mut config = Config {
            das: Duration::from_millis(90),
            arr: Duration::ZERO,
            preview_count: 2,
            ghost: false,
            color_mode: ColorMode::Ansi256,
            bot_level: Level::S,
            settings: Settings {
                soft_drop_factor: 12.5,
                start_level: 7,
                finesse_retry: true,
                finesse_180: true,
                ..Settings::default()
            },
            ..Config::default()
        };
        config.keymap.clear(Action::Hold);
        config.keymap.bind(KeyCode::Char('"'), Action::Hold);
        config.keymap.bind(KeyCode::Tab, Action::Hold);
        let (parsed, warnings) = parse(&to_toml(&config));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(parsed, config);
    }

    #[test]
    fn empty_config_uses_defaults() {
        let (config, warnings) = parse("");
        assert!(warnings.is_empty());
        assert_eq!(config, Config::default());
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
    fn defaults_for_missing_actions_do_not_steal_keys() {
        let (config, warnings) = parse("[keys]\nleft = [\"a\"]\n");
        assert!(warnings.is_empty());
        assert_eq!(
            config.keymap.action(&press(KeyCode::Char('a'))),
            Some(Action::Left)
        );
        assert_eq!(config.keymap.keys_for(Action::Rotate180).count(), 0);
        assert_eq!(
            config.keymap.action(&press(KeyCode::Char('z'))),
            Some(Action::RotateCcw)
        );
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

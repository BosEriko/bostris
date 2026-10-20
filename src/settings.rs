use std::fmt::Write;
use std::time::Duration;

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::config::{
    ARR_MS, ColorMode, Config, DAS_MS, LINE_CLEAR_MS, LOCK_DELAY_MS, MAX_LOCK_RESETS,
    PREVIEW_COUNT, SOFT_DROP_FACTOR, START_LEVEL,
};
use crate::input::{Action, key_label, normalize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Das,
    Arr,
    SoftDrop,
    LockDelay,
    LockResets,
    LineClear,
    StartLevel,
    FinesseRetry,
    Finesse180,
    Preview,
    Ghost,
    Colors,
}

impl Field {
    pub fn is_toggle(self) -> bool {
        matches!(
            self,
            Field::Ghost | Field::FinesseRetry | Field::Finesse180 | Field::Colors
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Field::Das => "DAS",
            Field::Arr => "ARR",
            Field::SoftDrop => "Soft drop speed",
            Field::LockDelay => "Lock delay",
            Field::LockResets => "Lock resets",
            Field::LineClear => "Line clear time",
            Field::StartLevel => "Start level",
            Field::FinesseRetry => "Retry on fault",
            Field::Finesse180 => "180 in finesse",
            Field::Preview => "Next pieces",
            Field::Ghost => "Ghost piece",
            Field::Colors => "Colors",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Field::Das => "Hold time before left/right starts repeating",
            Field::Arr => "Time between repeated moves (0 = instant)",
            Field::SoftDrop => "Soft drop speed as a multiple of gravity",
            Field::LockDelay => "How long a landed piece waits before locking",
            Field::LockResets => "Moves that can reset the lock delay",
            Field::LineClear => "Length of the line clear animation",
            Field::StartLevel => "Level for new games",
            Field::FinesseRetry => "Redo a piece placed with more inputs than needed",
            Field::Finesse180 => "Count a 180° spin as one input when judging finesse",
            Field::Preview => "Upcoming pieces shown in the queue",
            Field::Ghost => "Show where the piece will land",
            Field::Colors => "Color palette (auto detects your terminal)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Header(&'static str),
    Field(Field),
    Key(Action),
    Reset,
    Back,
}

pub const ITEMS: [Item; 31] = [
    Item::Header("HANDLING"),
    Item::Field(Field::Das),
    Item::Field(Field::Arr),
    Item::Field(Field::SoftDrop),
    Item::Header("GAMEPLAY"),
    Item::Field(Field::LockDelay),
    Item::Field(Field::LockResets),
    Item::Field(Field::LineClear),
    Item::Field(Field::StartLevel),
    Item::Field(Field::FinesseRetry),
    Item::Field(Field::Finesse180),
    Item::Header("DISPLAY"),
    Item::Field(Field::Preview),
    Item::Field(Field::Ghost),
    Item::Field(Field::Colors),
    Item::Header("CONTROLS"),
    Item::Key(Action::Left),
    Item::Key(Action::Right),
    Item::Key(Action::SoftDrop),
    Item::Key(Action::HardDrop),
    Item::Key(Action::RotateCw),
    Item::Key(Action::RotateCcw),
    Item::Key(Action::Rotate180),
    Item::Key(Action::Hold),
    Item::Key(Action::Pause),
    Item::Key(Action::Restart),
    Item::Key(Action::Settings),
    Item::Key(Action::Quit),
    Item::Header(""),
    Item::Reset,
    Item::Back,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    Replace(Action),
    Add(Action),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    Close,
}

#[derive(Clone, Debug)]
pub struct SettingsMenu {
    config: Config,
    cursor: usize,
    capture: Option<Capture>,
    status: Option<String>,
}

fn step_millis(value: Duration, delta: i64, (min, max): (i64, i64)) -> Duration {
    let current = value.as_millis() as i64;
    Duration::from_millis((current + delta).clamp(min, max) as u64)
}

fn step_int(value: i64, delta: i64, (min, max): (i64, i64)) -> i64 {
    (value + delta).clamp(min, max)
}

impl SettingsMenu {
    pub fn new(config: Config) -> Self {
        let mut menu = Self {
            config,
            cursor: 0,
            capture: None,
            status: None,
        };
        menu.move_cursor(1);
        menu
    }

    #[cfg(test)]
    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn into_config(self) -> Config {
        self.config
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn capture(&self) -> Option<Capture> {
        self.capture
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn selected(&self) -> Item {
        ITEMS[self.cursor]
    }

    fn selectable(item: Item) -> bool {
        !matches!(item, Item::Header(_))
    }

    fn move_cursor(&mut self, delta: isize) {
        let len = ITEMS.len() as isize;
        let mut index = self.cursor as isize;
        for _ in 0..len {
            index = (index + delta).rem_euclid(len);
            if Self::selectable(ITEMS[index as usize]) {
                self.cursor = index as usize;
                return;
            }
        }
    }

    pub fn handle(&mut self, key: KeyEvent) -> Outcome {
        if let Some(capture) = self.capture.take() {
            self.apply_capture(capture, normalize(key.code));
            return Outcome::Stay;
        }
        self.status = None;
        match key.code {
            KeyCode::Up => self.move_cursor(-1),
            KeyCode::Down | KeyCode::Tab => self.move_cursor(1),
            KeyCode::BackTab => self.move_cursor(-1),
            KeyCode::Left => self.adjust(-1),
            KeyCode::Right => self.adjust(1),
            KeyCode::Enter | KeyCode::Char(' ') => return self.activate(),
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if let Item::Key(action) = self.selected() {
                    self.capture = Some(Capture::Add(action));
                }
            }
            KeyCode::Backspace | KeyCode::Delete => {
                if let Item::Key(action) = self.selected() {
                    self.reset_action(action);
                }
            }
            KeyCode::Esc => return Outcome::Close,
            _ => {}
        }
        Outcome::Stay
    }

    fn activate(&mut self) -> Outcome {
        match self.selected() {
            Item::Field(field) if field.is_toggle() => self.adjust(1),
            Item::Field(_) | Item::Header(_) => {}
            Item::Key(action) => self.capture = Some(Capture::Replace(action)),
            Item::Reset => {
                self.config = Config::default();
                self.status = Some("All settings restored to defaults".into());
            }
            Item::Back => return Outcome::Close,
        }
        Outcome::Stay
    }

    fn adjust(&mut self, delta: i64) {
        let Item::Field(field) = self.selected() else {
            return;
        };
        let config = &mut self.config;
        let settings = &mut config.settings;
        match field {
            Field::Das => config.das = step_millis(config.das, delta, DAS_MS),
            Field::Arr => config.arr = step_millis(config.arr, delta, ARR_MS),
            Field::SoftDrop => {
                let (min, max) = SOFT_DROP_FACTOR;
                settings.soft_drop_factor =
                    (settings.soft_drop_factor.round() + delta as f64).clamp(min, max);
            }
            Field::LockDelay => {
                settings.lock_delay = step_millis(settings.lock_delay, delta * 50, LOCK_DELAY_MS)
            }
            Field::LockResets => {
                settings.max_lock_resets =
                    step_int(settings.max_lock_resets.into(), delta, MAX_LOCK_RESETS) as u32
            }
            Field::LineClear => {
                settings.line_clear_delay =
                    step_millis(settings.line_clear_delay, delta * 25, LINE_CLEAR_MS)
            }
            Field::StartLevel => {
                settings.start_level =
                    step_int(settings.start_level.into(), delta, START_LEVEL) as u32
            }
            Field::FinesseRetry => settings.finesse_retry = !settings.finesse_retry,
            Field::Finesse180 => settings.finesse_180 = !settings.finesse_180,
            Field::Preview => {
                config.preview_count =
                    step_int(config.preview_count as i64, delta, PREVIEW_COUNT) as usize
            }
            Field::Ghost => config.ghost = !config.ghost,
            Field::Colors => {
                const ORDER: [ColorMode; 4] = [
                    ColorMode::Auto,
                    ColorMode::TrueColor,
                    ColorMode::Ansi256,
                    ColorMode::Ansi16,
                ];
                let index = ORDER
                    .iter()
                    .position(|&m| m == config.color_mode)
                    .unwrap_or(0) as i64;
                config.color_mode = ORDER[(index + delta).rem_euclid(4) as usize];
            }
        }
    }

    fn apply_capture(&mut self, capture: Capture, code: KeyCode) {
        let keymap = &mut self.config.keymap;
        let (action, replace) = match capture {
            Capture::Replace(action) => (action, true),
            Capture::Add(action) => (action, false),
        };
        let owner = keymap.owner(code);
        if owner == Some(action) && !replace {
            return;
        }
        let owner_keys = owner.map_or(0, |o| keymap.keys_for(o).count());
        let previous: Vec<KeyCode> = keymap.keys_for(action).collect();
        if let Some(other) = owner.filter(|&o| o != action && owner_keys == 1) {
            match previous
                .iter()
                .copied()
                .find(|&k| k != code)
                .filter(|_| replace)
            {
                Some(swap) => {
                    keymap.clear(action);
                    keymap.bind(code, action);
                    keymap.bind(swap, other);
                    self.status = Some(format!(
                        "Swapped: {} is now {}",
                        other.label(),
                        key_label(swap)
                    ));
                }
                None => {
                    self.status = Some(format!(
                        "{} is the only key for {}",
                        key_label(code),
                        other.label()
                    ));
                }
            }
            return;
        }
        if replace {
            keymap.clear(action);
        }
        keymap.bind(code, action);
        if let Some(other) = owner.filter(|&o| o != action) {
            self.status = Some(format!("{} moved from {}", key_label(code), other.label()));
        }
    }

    fn reset_action(&mut self, action: Action) {
        let keymap = &mut self.config.keymap;
        keymap.clear(action);
        keymap.bind_defaults(action);
        for other in Action::ALL {
            if other != action && keymap.keys_for(other).next().is_none() {
                keymap.bind_defaults(other);
            }
        }
        if keymap.keys_for(action).next().is_none() {
            keymap.bind_defaults(action);
        }
        self.status = Some(format!("{} restored to default", action.label()));
    }

    pub fn write_value(&self, field: Field, out: &mut String) {
        let config = &self.config;
        let settings = &config.settings;
        let on_off = |value: bool| if value { "On" } else { "Off" };
        let _ = match field {
            Field::Das => write!(out, "{} ms", config.das.as_millis()),
            Field::Arr => write!(out, "{} ms", config.arr.as_millis()),
            Field::SoftDrop => write!(out, "{}×", settings.soft_drop_factor),
            Field::LockDelay => write!(out, "{} ms", settings.lock_delay.as_millis()),
            Field::LockResets => write!(out, "{}", settings.max_lock_resets),
            Field::LineClear => write!(out, "{} ms", settings.line_clear_delay.as_millis()),
            Field::StartLevel => write!(out, "{}", settings.start_level),
            Field::FinesseRetry => write!(out, "{}", on_off(settings.finesse_retry)),
            Field::Finesse180 => write!(out, "{}", on_off(settings.finesse_180)),
            Field::Preview => write!(out, "{}", config.preview_count),
            Field::Ghost => write!(out, "{}", on_off(config.ghost)),
            Field::Colors => write!(
                out,
                "{}",
                match config.color_mode {
                    ColorMode::Auto => "Auto",
                    ColorMode::TrueColor => "True color",
                    ColorMode::Ansi256 => "256",
                    ColorMode::Ansi16 => "16",
                }
            ),
        };
    }

    pub fn write_keys(&self, action: Action, out: &mut String) {
        for (i, code) in self.config.keymap.keys_for(action).enumerate() {
            if i > 0 {
                out.push_str(" / ");
            }
            out.push_str(&key_label(code));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyEventKind, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    fn menu_at(item: Item) -> SettingsMenu {
        let mut menu = SettingsMenu::new(Config::default());
        while menu.selected() != item {
            menu.handle(key(KeyCode::Down));
        }
        menu
    }

    fn keys(menu: &SettingsMenu, action: Action) -> Vec<KeyCode> {
        menu.config().keymap.keys_for(action).collect()
    }

    #[test]
    fn cursor_skips_headers_and_wraps() {
        let mut menu = SettingsMenu::new(Config::default());
        assert_eq!(menu.selected(), Item::Field(Field::Das));
        menu.handle(key(KeyCode::Up));
        assert_eq!(menu.selected(), Item::Back);
        menu.handle(key(KeyCode::Down));
        assert_eq!(menu.selected(), Item::Field(Field::Das));
        for _ in 0..3 {
            menu.handle(key(KeyCode::Down));
        }
        assert_eq!(menu.selected(), Item::Field(Field::LockDelay));
    }

    #[test]
    fn values_adjust_within_range() {
        let mut menu = menu_at(Item::Field(Field::Das));
        menu.handle(key(KeyCode::Right));
        assert_eq!(menu.config().das, Duration::from_millis(168));
        for _ in 0..2000 {
            menu.handle(key(KeyCode::Left));
        }
        assert_eq!(menu.config().das, Duration::ZERO);
        let mut menu = menu_at(Item::Field(Field::LockDelay));
        menu.handle(key(KeyCode::Left));
        assert_eq!(
            menu.config().settings.lock_delay,
            Duration::from_millis(450)
        );
        let mut menu = menu_at(Item::Field(Field::StartLevel));
        for _ in 0..50 {
            menu.handle(key(KeyCode::Right));
        }
        assert_eq!(menu.config().settings.start_level, 30);
    }

    #[test]
    fn toggles_and_cycles() {
        let mut menu = menu_at(Item::Field(Field::FinesseRetry));
        menu.handle(key(KeyCode::Enter));
        assert!(menu.config().settings.finesse_retry);
        let mut menu = menu_at(Item::Field(Field::Colors));
        menu.handle(key(KeyCode::Left));
        assert_eq!(menu.config().color_mode, ColorMode::Ansi16);
        menu.handle(key(KeyCode::Right));
        menu.handle(key(KeyCode::Right));
        assert_eq!(menu.config().color_mode, ColorMode::TrueColor);
    }

    #[test]
    fn rebinding_replaces_keys() {
        let mut menu = menu_at(Item::Key(Action::Hold));
        menu.handle(key(KeyCode::Enter));
        assert_eq!(menu.capture(), Some(Capture::Replace(Action::Hold)));
        menu.handle(key(KeyCode::Char('V')));
        assert_eq!(keys(&menu, Action::Hold), vec![KeyCode::Char('v')]);
        assert_eq!(menu.capture(), None);
    }

    #[test]
    fn adding_keeps_existing_keys() {
        let mut menu = menu_at(Item::Key(Action::Left));
        menu.handle(key(KeyCode::Char('a')));
        menu.handle(key(KeyCode::Char('j')));
        assert_eq!(
            keys(&menu, Action::Left),
            vec![KeyCode::Left, KeyCode::Char('j')]
        );
    }

    #[test]
    fn capture_accepts_menu_keys() {
        let mut menu = menu_at(Item::Key(Action::Pause));
        menu.handle(key(KeyCode::Char('a')));
        menu.handle(key(KeyCode::Enter));
        assert!(keys(&menu, Action::Pause).contains(&KeyCode::Enter));
    }

    #[test]
    fn taking_an_only_key_swaps() {
        let mut menu = menu_at(Item::Key(Action::Hold));
        menu.handle(key(KeyCode::Enter));
        menu.handle(key(KeyCode::Char('z')));
        assert_eq!(keys(&menu, Action::Hold), vec![KeyCode::Char('z')]);
        assert_eq!(keys(&menu, Action::RotateCcw), vec![KeyCode::Char('c')]);
        assert!(menu.status().unwrap().contains("Swapped"));
    }

    #[test]
    fn taking_a_shared_key_moves_it() {
        let mut menu = menu_at(Item::Key(Action::Hold));
        menu.handle(key(KeyCode::Enter));
        menu.handle(key(KeyCode::Char('x')));
        assert_eq!(keys(&menu, Action::Hold), vec![KeyCode::Char('x')]);
        assert_eq!(keys(&menu, Action::RotateCw), vec![KeyCode::Up]);
    }

    #[test]
    fn adding_an_only_key_is_refused() {
        let mut menu = menu_at(Item::Key(Action::Hold));
        menu.handle(key(KeyCode::Char('a')));
        menu.handle(key(KeyCode::Char('q')));
        assert_eq!(keys(&menu, Action::Quit), vec![KeyCode::Char('q')]);
        assert_eq!(keys(&menu, Action::Hold), vec![KeyCode::Char('c')]);
        assert!(menu.status().is_some());
    }

    #[test]
    fn delete_restores_action_default() {
        let mut menu = menu_at(Item::Key(Action::Hold));
        menu.handle(key(KeyCode::Enter));
        menu.handle(key(KeyCode::Char('v')));
        menu.handle(key(KeyCode::Delete));
        assert_eq!(keys(&menu, Action::Hold), vec![KeyCode::Char('c')]);
    }

    #[test]
    fn reset_restores_everything_and_back_closes() {
        let mut menu = menu_at(Item::Field(Field::Ghost));
        menu.handle(key(KeyCode::Enter));
        assert!(!menu.config().ghost);
        while menu.selected() != Item::Reset {
            menu.handle(key(KeyCode::Down));
        }
        menu.handle(key(KeyCode::Enter));
        assert_eq!(menu.config(), &Config::default());
        menu.handle(key(KeyCode::Down));
        assert_eq!(menu.handle(key(KeyCode::Enter)), Outcome::Close);
        assert_eq!(menu.handle(key(KeyCode::Esc)), Outcome::Close);
    }
}

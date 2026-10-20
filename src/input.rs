use std::time::Duration;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Left,
    Right,
    SoftDrop,
    HardDrop,
    RotateCw,
    RotateCcw,
    Hold,
    Pause,
    Restart,
    Quit,
}

impl Action {
    pub const ALL: [Action; 10] = [
        Action::Left,
        Action::Right,
        Action::SoftDrop,
        Action::HardDrop,
        Action::RotateCw,
        Action::RotateCcw,
        Action::Hold,
        Action::Pause,
        Action::Restart,
        Action::Quit,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Action::Left => "left",
            Action::Right => "right",
            Action::SoftDrop => "soft_drop",
            Action::HardDrop => "hard_drop",
            Action::RotateCw => "rotate_cw",
            Action::RotateCcw => "rotate_ccw",
            Action::Hold => "hold",
            Action::Pause => "pause",
            Action::Restart => "restart",
            Action::Quit => "quit",
        }
    }

    pub fn default_keys(self) -> &'static [&'static str] {
        match self {
            Action::Left => &["Left"],
            Action::Right => &["Right"],
            Action::SoftDrop => &["Down"],
            Action::HardDrop => &["Space"],
            Action::RotateCw => &["Up", "x"],
            Action::RotateCcw => &["z"],
            Action::Hold => &["c"],
            Action::Pause => &["p", "Esc"],
            Action::Restart => &["r"],
            Action::Quit => &["q"],
        }
    }
}

pub fn parse_key(name: &str) -> Option<KeyCode> {
    let trimmed = name.trim();
    let mut chars = trimmed.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(KeyCode::Char(c.to_ascii_lowercase()));
    }
    let lower = trimmed.to_ascii_lowercase();
    let code = match lower.as_str() {
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "space" => KeyCode::Char(' '),
        "esc" | "escape" => KeyCode::Esc,
        "enter" | "return" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "insert" => KeyCode::Insert,
        "delete" => KeyCode::Delete,
        _ => {
            let number = lower.strip_prefix('f')?.parse::<u8>().ok()?;
            if (1..=12).contains(&number) {
                KeyCode::F(number)
            } else {
                return None;
            }
        }
    };
    Some(code)
}

pub fn key_label(code: KeyCode) -> String {
    match code {
        KeyCode::Left => "←".into(),
        KeyCode::Right => "→".into(),
        KeyCode::Up => "↑".into(),
        KeyCode::Down => "↓".into(),
        KeyCode::Char(' ') => "Space".into(),
        KeyCode::Char(c) => c.to_ascii_uppercase().to_string(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::Backspace => "Bksp".into(),
        KeyCode::F(n) => format!("F{n}"),
        other => format!("{other:?}"),
    }
}

#[derive(Clone, Debug, Default)]
pub struct Keymap {
    bindings: Vec<(KeyCode, Action)>,
}

impl Keymap {
    pub fn bind(&mut self, code: KeyCode, action: Action) {
        self.bindings.retain(|&(c, _)| c != code);
        self.bindings.push((code, action));
    }

    pub fn defaults() -> Self {
        let mut keymap = Self::default();
        for action in Action::ALL {
            for key in action.default_keys() {
                if let Some(code) = parse_key(key) {
                    keymap.bind(code, action);
                }
            }
        }
        keymap
    }

    pub fn action(&self, event: &KeyEvent) -> Option<Action> {
        if event.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(event.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            return Some(Action::Quit);
        }
        let code = match event.code {
            KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
            other => other,
        };
        self.bindings
            .iter()
            .find(|&&(c, _)| c == code)
            .map(|&(_, action)| action)
    }

    pub fn keys_for(&self, action: Action) -> impl Iterator<Item = KeyCode> + '_ {
        self.bindings
            .iter()
            .filter(move |&&(_, a)| a == action)
            .map(|&(c, _)| c)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
}

impl Direction {
    pub fn dx(self) -> i8 {
        match self {
            Direction::Left => -1,
            Direction::Right => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shift {
    None,
    Steps(u32),
    ToWall,
}

#[derive(Clone, Debug)]
pub struct AutoShift {
    das: Duration,
    arr: Duration,
    left_held: bool,
    right_held: bool,
    active: Option<Direction>,
    charge: Duration,
    repeat: Duration,
}

impl AutoShift {
    pub fn new(das: Duration, arr: Duration) -> Self {
        Self {
            das,
            arr,
            left_held: false,
            right_held: false,
            active: None,
            charge: Duration::ZERO,
            repeat: Duration::ZERO,
        }
    }

    pub fn active(&self) -> Option<Direction> {
        self.active
    }

    pub fn press(&mut self, direction: Direction) {
        match direction {
            Direction::Left => self.left_held = true,
            Direction::Right => self.right_held = true,
        }
        self.start(direction);
    }

    pub fn release(&mut self, direction: Direction) {
        match direction {
            Direction::Left => self.left_held = false,
            Direction::Right => self.right_held = false,
        }
        if self.active == Some(direction) {
            self.active = None;
            let other = match direction {
                Direction::Left if self.right_held => Some(Direction::Right),
                Direction::Right if self.left_held => Some(Direction::Left),
                _ => None,
            };
            if let Some(other) = other {
                self.start(other);
            }
        }
    }

    pub fn release_all(&mut self) {
        self.left_held = false;
        self.right_held = false;
        self.active = None;
    }

    fn start(&mut self, direction: Direction) {
        self.active = Some(direction);
        self.charge = Duration::ZERO;
        self.repeat = Duration::ZERO;
    }

    pub fn tick(&mut self, dt: Duration) -> Shift {
        if self.active.is_none() {
            return Shift::None;
        }
        if self.charge < self.das {
            self.charge += dt;
            if self.charge < self.das {
                return Shift::None;
            }
            self.repeat = self.charge - self.das;
            if self.arr.is_zero() {
                return Shift::ToWall;
            }
            let steps = 1 + (self.repeat.as_nanos() / self.arr.as_nanos()) as u32;
            self.repeat =
                Duration::from_nanos((self.repeat.as_nanos() % self.arr.as_nanos()) as u64);
            return Shift::Steps(steps);
        }
        if self.arr.is_zero() {
            return Shift::ToWall;
        }
        self.repeat += dt;
        let steps = (self.repeat.as_nanos() / self.arr.as_nanos()) as u32;
        if steps == 0 {
            return Shift::None;
        }
        self.repeat -= self.arr * steps;
        Shift::Steps(steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    const TICK: Duration = Duration::from_millis(10);

    fn run(shift: &mut AutoShift, ticks: u32) -> u32 {
        let mut total = 0;
        for _ in 0..ticks {
            match shift.tick(TICK) {
                Shift::None => {}
                Shift::Steps(n) => total += n,
                Shift::ToWall => total += 100,
            }
        }
        total
    }

    #[test]
    fn das_delays_auto_repeat() {
        let mut shift = AutoShift::new(Duration::from_millis(100), Duration::from_millis(20));
        shift.press(Direction::Left);
        assert_eq!(run(&mut shift, 9), 0);
        assert_eq!(run(&mut shift, 1), 1);
        assert_eq!(run(&mut shift, 4), 2);
        shift.release(Direction::Left);
        assert_eq!(run(&mut shift, 50), 0);
    }

    #[test]
    fn zero_arr_shifts_to_wall() {
        let mut shift = AutoShift::new(Duration::from_millis(50), Duration::ZERO);
        shift.press(Direction::Right);
        assert_eq!(run(&mut shift, 4), 0);
        assert_eq!(shift.tick(TICK), Shift::ToWall);
    }

    #[test]
    fn releasing_newer_direction_resumes_older() {
        let mut shift = AutoShift::new(Duration::from_millis(50), Duration::from_millis(10));
        shift.press(Direction::Left);
        shift.press(Direction::Right);
        assert_eq!(shift.active(), Some(Direction::Right));
        shift.release(Direction::Right);
        assert_eq!(shift.active(), Some(Direction::Left));
        assert_eq!(run(&mut shift, 4), 0);
        shift.release(Direction::Left);
        assert_eq!(shift.active(), None);
    }

    #[test]
    fn keys_parse_case_insensitively() {
        assert_eq!(parse_key("Left"), Some(KeyCode::Left));
        assert_eq!(parse_key("SPACE"), Some(KeyCode::Char(' ')));
        assert_eq!(parse_key("X"), Some(KeyCode::Char('x')));
        assert_eq!(parse_key("f5"), Some(KeyCode::F(5)));
        assert_eq!(parse_key("f13"), None);
        assert_eq!(parse_key("banana"), None);
    }

    #[test]
    fn default_keymap_matches_controls() {
        let keymap = Keymap::defaults();
        let event = |code| KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press);
        assert_eq!(keymap.action(&event(KeyCode::Up)), Some(Action::RotateCw));
        assert_eq!(
            keymap.action(&event(KeyCode::Char('X'))),
            Some(Action::RotateCw)
        );
        assert_eq!(
            keymap.action(&event(KeyCode::Char('z'))),
            Some(Action::RotateCcw)
        );
        assert_eq!(
            keymap.action(&event(KeyCode::Char(' '))),
            Some(Action::HardDrop)
        );
        assert_eq!(keymap.action(&event(KeyCode::Esc)), Some(Action::Pause));
        assert_eq!(keymap.action(&event(KeyCode::Char('k'))), None);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(keymap.action(&ctrl_c), Some(Action::Quit));
    }
}

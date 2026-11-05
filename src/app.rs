use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};

use crate::bag::Rng;
use crate::bot::Bot;
use crate::config::{self, Config};
use crate::game::{Game, Settings, TICK};
use crate::highscores::HighScores;
use crate::input::{Action, AutoShift, Direction, Shift, is_quit_chord};
use crate::settings::{Outcome, SettingsMenu};
use crate::training::Training;
use crate::ui::{self, Screen, Title, Versus, View};

const MAX_CATCH_UP_TICKS: u32 = 15;
const REPEAT_WINDOW: Duration = Duration::from_millis(550);

struct Match {
    opponent: Game,
    bot: Bot,
}

pub struct App {
    config: Config,
    game: Game,
    versus: Option<Match>,
    training: Option<Training>,
    title: Title,
    size: (u16, u16),
    screen: Screen,
    scores: HighScores,
    shifter: AutoShift,
    menu: Option<SettingsMenu>,
    settings_return: Screen,
    last_tap: Option<(Direction, Instant, u32)>,
    precise_keys: bool,
    fits: bool,
    dirty: bool,
    drawn_revision: (u64, u64),
    quit: bool,
    save_error: Option<String>,
    elapsed: Duration,
}

impl App {
    pub fn new(config: Config, scores: HighScores, precise_keys: bool) -> Self {
        let shifter = AutoShift::new(config.das, config.arr);
        let game = Game::new(config.settings.clone(), Rng::from_entropy());
        let title = Title {
            selected: 0,
            level: config.bot_level,
        };
        Self {
            config,
            game,
            versus: None,
            training: None,
            title,
            size: (0, 0),
            screen: Screen::Ready,
            scores,
            shifter,
            menu: None,
            settings_return: Screen::Ready,
            last_tap: None,
            precise_keys,
            fits: true,
            dirty: true,
            drawn_revision: (0, 0),
            quit: false,
            save_error: None,
            elapsed: Duration::ZERO,
        }
    }

    pub fn save_error(&self) -> Option<&str> {
        self.save_error.as_deref()
    }

    pub fn advance(&mut self, elapsed: Duration) {
        if !self.is_simulating() {
            self.elapsed = Duration::ZERO;
            return;
        }
        self.elapsed = (self.elapsed + elapsed).min(TICK * MAX_CATCH_UP_TICKS);
        while self.elapsed >= TICK && self.is_simulating() {
            self.step();
            self.elapsed -= TICK;
            self.check_game_over();
        }
    }

    pub fn needs_draw(&self) -> bool {
        self.dirty || self.revision() != self.drawn_revision
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn screen_name(&self) -> &'static str {
        match self.screen {
            Screen::Ready => "Title",
            Screen::Learn => "Learn",
            Screen::Playing => "Playing",
            Screen::Paused => "Paused",
            Screen::GameOver { .. } => "Game over",
            Screen::VersusOver { won: true } => "You win",
            Screen::VersusOver { won: false } => "You lose",
            Screen::Settings => "Settings",
        }
    }

    fn revision(&self) -> (u64, u64) {
        (
            self.game.revision(),
            self.versus.as_ref().map_or(0, |m| m.opponent.revision()),
        )
    }

    fn update_fits(&mut self) {
        let versus = if self.screen == Screen::Ready {
            self.title.selected == 1
        } else {
            self.versus.is_some()
        };
        self.fits = ui::fits(self.size.0, self.size.1, versus);
    }

    pub fn is_simulating(&self) -> bool {
        self.screen == Screen::Playing
            && self.fits
            && self.training.as_ref().is_none_or(Training::is_active)
    }

    pub fn view(&self) -> View<'_> {
        View {
            game: &self.game,
            screen: self.screen,
            scores: &self.scores,
            ghost: self.config.ghost,
            preview_count: self.config.preview_count,
            menu: self.menu.as_ref(),
            versus: self.versus.as_ref().map(|m| Versus {
                opponent: &m.opponent,
                level: m.bot.level(),
            }),
            title: self.title,
            config: &self.config,
            training: self.training.as_ref(),
        }
    }

    pub fn mark_drawn(&mut self) {
        self.drawn_revision = self.revision();
        self.dirty = false;
    }

    fn step(&mut self) {
        if let Some(direction) = self.shifter.active() {
            match self.shifter.tick(TICK) {
                Shift::None => {}
                Shift::Steps(n) => {
                    for _ in 0..n {
                        if !self.game.shift(direction.dx()) {
                            break;
                        }
                    }
                }
                Shift::ToWall => while self.game.shift(direction.dx()) {},
            }
        }
        if self.training.is_some() {
            self.game.practice_tick(TICK);
        } else {
            self.game.tick(TICK);
        }
        if let Some(versus) = &mut self.versus {
            versus.bot.tick(&mut versus.opponent, TICK);
            versus.opponent.tick(TICK);
            versus.opponent.receive_garbage(self.game.take_outgoing());
            self.game.receive_garbage(versus.opponent.take_outgoing());
        }
    }

    fn check_game_over(&mut self) {
        if self.screen != Screen::Playing {
            return;
        }
        if let Some(training) = &mut self.training {
            let was_active = training.is_active();
            training.observe(&mut self.game);
            if was_active && !training.is_active() {
                self.release_keys();
                self.dirty = true;
            }
            return;
        }
        if let Some(versus) = &self.versus {
            let lost = self.game.is_over();
            if lost || versus.opponent.is_over() {
                self.release_keys();
                self.screen = Screen::VersusOver { won: !lost };
                self.dirty = true;
            }
            return;
        }
        if !self.game.is_over() {
            return;
        }
        let scoring = self.game.scoring();
        let rank = self
            .scores
            .record(scoring.score, scoring.lines, scoring.level());
        if rank.is_some()
            && let Err(err) = self.scores.save()
        {
            self.save_error = Some(format!("could not save high scores: {err}"));
        }
        self.release_keys();
        self.screen = Screen::GameOver { rank };
        self.dirty = true;
    }

    fn set_screen(&mut self, screen: Screen) {
        self.release_keys();
        self.screen = screen;
        self.elapsed = Duration::ZERO;
        self.dirty = true;
    }

    fn release_keys(&mut self) {
        self.shifter.release_all();
        self.game.set_soft_drop(false);
    }

    fn restart(&mut self) {
        if let Some(training) = &self.training {
            if let Some((training, game)) = training.retry(self.config.settings.clone()) {
                self.training = Some(training);
                self.game = game;
                self.set_screen(Screen::Playing);
                self.update_fits();
            }
        } else if self.versus.is_some() {
            self.start_versus();
        } else {
            self.start_marathon();
        }
    }

    fn start_marathon(&mut self) {
        self.training = None;
        self.versus = None;
        self.game = Game::new(self.config.settings.clone(), Rng::from_entropy());
        self.set_screen(Screen::Playing);
        self.update_fits();
    }

    fn bot_settings(&self) -> Settings {
        Settings {
            finesse_retry: false,
            ..self.config.settings.clone()
        }
    }

    fn start_versus(&mut self) {
        self.training = None;
        let seed = Rng::from_entropy().next_u64();
        let gravity = Some(self.config.settings.start_level);
        self.game = Game::new(self.config.settings.clone(), Rng::new(seed));
        self.game.set_gravity_level(gravity);
        let mut opponent = Game::new(self.bot_settings(), Rng::new(seed));
        opponent.set_gravity_level(gravity);
        let bot = Bot::new(self.title.level, Rng::from_entropy());
        self.versus = Some(Match { opponent, bot });
        self.set_screen(Screen::Playing);
        self.update_fits();
    }

    fn open_title(&mut self) {
        self.training = None;
        self.versus = None;
        self.game = Game::new(self.config.settings.clone(), Rng::from_entropy());
        self.set_screen(Screen::Ready);
        self.update_fits();
    }

    fn open_learn(&mut self) {
        if self.screen == Screen::Settings {
            self.close_settings();
        }
        self.training = None;
        self.versus = None;
        self.set_screen(Screen::Learn);
        self.update_fits();
    }

    fn start_training(&mut self, index: usize) {
        if let Some((training, game)) = Training::start(index, self.config.settings.clone()) {
            self.training = Some(training);
            self.versus = None;
            self.game = game;
            self.set_screen(Screen::Playing);
            self.update_fits();
        }
    }

    fn start_selected(&mut self) {
        if self.title.selected == 1 {
            if self.title.level != self.config.bot_level {
                self.config.bot_level = self.title.level;
                if let Err(err) = config::save(&self.config) {
                    self.save_error = Some(format!("could not save settings: {err}"));
                }
            }
            self.start_versus();
        } else {
            self.start_marathon();
        }
    }

    fn handle_title_key(&mut self, key: KeyEvent, action: Option<Action>) -> bool {
        match (key.code, action) {
            (KeyCode::Up, _) | (KeyCode::Down, _) | (KeyCode::Tab, _) => {
                self.title.selected = 1 - self.title.selected;
            }
            (KeyCode::Left, _) | (_, Some(Action::Left)) => {
                self.title.level = self.title.level.step(-1);
                self.title.selected = 1;
            }
            (KeyCode::Right, _) | (_, Some(Action::Right)) => {
                self.title.level = self.title.level.step(1);
                self.title.selected = 1;
            }
            (KeyCode::Enter, _) | (_, Some(Action::HardDrop)) => {
                if self.fits {
                    self.start_selected();
                }
            }
            _ => return false,
        }
        self.update_fits();
        self.dirty = true;
        true
    }

    fn open_settings(&mut self) {
        self.settings_return = match self.screen {
            Screen::Playing => Screen::Paused,
            Screen::Settings => return,
            other => other,
        };
        self.menu = Some(SettingsMenu::new(self.config.clone()));
        self.set_screen(Screen::Settings);
    }

    fn close_settings(&mut self) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        let updated = menu.into_config();
        if updated != self.config {
            self.config = updated;
            if let Err(err) = config::save(&self.config) {
                self.save_error = Some(format!("could not save settings: {err}"));
            }
        }
        self.shifter = AutoShift::new(self.config.das, self.config.arr);
        if self.settings_return == Screen::Ready {
            self.game = Game::new(self.config.settings.clone(), Rng::from_entropy());
        } else {
            let mut settings = self.config.settings.clone();
            if self.training.is_some() {
                settings.finesse_retry = false;
                settings.line_clear_delay = Duration::ZERO;
            }
            self.game.set_settings(settings);
            let bot_settings = self.bot_settings();
            if let Some(versus) = &mut self.versus {
                versus.opponent.set_settings(bot_settings);
            }
        }
        self.set_screen(self.settings_return);
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if is_quit_chord(&key) {
            self.quit = true;
            return;
        }
        let outcome = self
            .menu
            .as_mut()
            .map_or(Outcome::Close, |menu| menu.handle(key));
        self.dirty = true;
        if outcome == Outcome::Close {
            self.close_settings();
        }
    }

    pub fn handle_ui(&mut self, action: &str, index: Option<usize>, delta: i64) {
        match action {
            "learn" => self.open_learn(),
            "practice" if self.screen == Screen::Learn => {
                if let Some(index) = index {
                    self.start_training(index);
                }
            }
            "retry-lesson" if self.training.is_some() => self.restart(),
            "select-mode" if self.screen == Screen::Ready => {
                if let Some(index @ 0..=1) = index {
                    self.title.selected = index;
                    self.update_fits();
                    self.dirty = true;
                }
            }
            "level" if self.screen == Screen::Ready => {
                self.title.level = self.title.level.step(delta.clamp(-1, 1));
                self.dirty = true;
            }
            "start" if self.screen == Screen::Ready && self.fits => self.start_selected(),
            "pause" if self.screen == Screen::Playing => self.set_screen(Screen::Paused),
            "resume" if self.screen == Screen::Paused && self.fits => {
                self.set_screen(Screen::Playing)
            }
            "restart"
                if !matches!(
                    self.screen,
                    Screen::Ready | Screen::Learn | Screen::Settings
                ) =>
            {
                self.restart()
            }
            "title" if !matches!(self.screen, Screen::Ready | Screen::Settings) => {
                self.open_title()
            }
            "settings" => self.open_settings(),
            "close-settings" if self.screen == Screen::Settings => self.close_settings(),
            "setting" | "rebind" | "add-key" | "reset-key" | "reset-settings"
                if self.screen == Screen::Settings =>
            {
                use crate::settings::Item;
                let Some(menu) = self.menu.as_mut() else {
                    return;
                };
                let index = if action == "reset-settings" {
                    crate::settings::ITEMS
                        .iter()
                        .position(|item| *item == Item::Reset)
                } else {
                    index
                };
                let Some(index) = index else { return };
                let Some(item) = crate::settings::ITEMS.get(index) else {
                    return;
                };
                let code = match (action, item) {
                    ("setting", Item::Field(_)) => {
                        if delta < 0 {
                            KeyCode::Left
                        } else {
                            KeyCode::Right
                        }
                    }
                    ("rebind", Item::Key(_)) | ("reset-settings", Item::Reset) => KeyCode::Enter,
                    ("add-key", Item::Key(_)) => KeyCode::Char('a'),
                    ("reset-key", Item::Key(_)) => KeyCode::Delete,
                    _ => return,
                };
                if menu.select(index) {
                    self.handle_settings_key(KeyEvent::new(
                        code,
                        crossterm::event::KeyModifiers::NONE,
                    ));
                }
            }
            _ => {}
        }
    }

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Resize(width, height) => {
                self.size = (width, height);
                self.update_fits();
                if !self.fits && self.screen == Screen::Playing {
                    self.set_screen(Screen::Paused);
                }
                self.dirty = true;
            }
            Event::FocusLost => {
                if self.screen == Screen::Playing {
                    self.set_screen(Screen::Paused);
                } else {
                    self.release_keys();
                }
            }
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if self.screen == Screen::Learn
            && key.kind != KeyEventKind::Release
            && key.code == KeyCode::Esc
        {
            self.open_title();
            return;
        }
        if self.screen == Screen::Settings {
            self.handle_settings_key(key);
            return;
        }
        let action = self.config.keymap.action(&key);
        if key.kind != KeyEventKind::Release
            && !matches!(action, Some(Action::Quit | Action::Settings))
        {
            match self.screen {
                Screen::Ready => {
                    if self.handle_title_key(key, action) {
                        return;
                    }
                }
                Screen::Paused if key.code == KeyCode::Backspace => {
                    self.open_title();
                    return;
                }
                _ => {}
            }
        }
        match key.kind {
            KeyEventKind::Press => self.on_press(action),
            KeyEventKind::Repeat => {
                if !self.precise_keys {
                    self.on_press(action);
                }
            }
            KeyEventKind::Release => {
                self.precise_keys = true;
                self.on_release(action);
            }
        }
    }

    fn on_press(&mut self, action: Option<Action>) {
        if action == Some(Action::Quit) {
            self.quit = true;
            return;
        }
        if action == Some(Action::Settings) {
            self.open_settings();
            return;
        }
        match self.screen {
            Screen::Ready | Screen::Learn => {}
            Screen::Paused => match action {
                Some(Action::Pause) if self.fits => self.set_screen(Screen::Playing),
                Some(Action::Restart) => self.restart(),
                _ => {}
            },
            Screen::GameOver { .. } | Screen::VersusOver { .. } => match action {
                Some(Action::Restart) => self.restart(),
                Some(Action::Pause) => self.open_title(),
                _ => {}
            },
            Screen::Settings => {}
            Screen::Playing => {
                if let Some(action) = action {
                    self.play(action);
                }
            }
        }
    }

    fn play(&mut self, action: Action) {
        if let Some(training) = &self.training {
            if !training.is_active()
                && !matches!(
                    action,
                    Action::Pause | Action::Restart | Action::Settings | Action::Quit
                )
            {
                return;
            }
            if action == Action::Hold && !training.sandbox() {
                return;
            }
        }
        match action {
            Action::Left | Action::Right => {
                let direction = if action == Action::Left {
                    Direction::Left
                } else {
                    Direction::Right
                };
                if self.is_new_tap(direction) {
                    self.game.note_input();
                }
                self.game.shift(direction.dx());
                if self.precise_keys {
                    self.shifter.press(direction);
                }
            }
            Action::SoftDrop => {
                self.game.soft_drop_step();
                if self.precise_keys {
                    self.game.set_soft_drop(true);
                }
            }
            Action::HardDrop => self.game.hard_drop(),
            Action::RotateCw => {
                self.game.note_input();
                self.game.rotate(true);
            }
            Action::RotateCcw => {
                self.game.note_input();
                self.game.rotate(false);
            }
            Action::Rotate180 => {
                self.game.note_input();
                self.game.rotate_180();
            }
            Action::Hold => {
                self.game.hold();
            }
            Action::Pause => self.set_screen(Screen::Paused),
            Action::Restart => self.restart(),
            Action::Settings => self.open_settings(),
            Action::Quit => self.quit = true,
        }
        self.check_game_over();
    }

    fn is_new_tap(&mut self, direction: Direction) -> bool {
        let now = Instant::now();
        let piece = self.game.spawn_id();
        let previous = self.last_tap.replace((direction, now, piece));
        self.precise_keys
            || !matches!(previous, Some((d, t, p))
                if d == direction && p == piece && now.duration_since(t) < REPEAT_WINDOW)
    }

    fn on_release(&mut self, action: Option<Action>) {
        match action {
            Some(Action::Left) => self.shifter.release(Direction::Left),
            Some(Action::Right) => self.shifter.release(Direction::Right),
            Some(Action::SoftDrop) => self.game.set_soft_drop(false),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_clock_uses_fixed_steps_and_bounds_catch_up() {
        let mut app = App::new(Config::default(), HighScores::load(None), true);
        app.handle_event(Event::Resize(1280, 800));
        app.start_marathon();
        app.advance(TICK / 2);
        assert_eq!(app.game.elapsed(), Duration::ZERO);
        app.advance(TICK - TICK / 2);
        assert_eq!(app.game.elapsed(), TICK);
        app.advance(Duration::from_secs(60));
        assert_eq!(app.game.elapsed(), TICK * (MAX_CATCH_UP_TICKS + 1));
        app.handle_event(Event::FocusLost);
        app.advance(Duration::from_secs(60));
        assert_eq!(app.game.elapsed(), TICK * (MAX_CATCH_UP_TICKS + 1));
        app.set_screen(Screen::Playing);
        app.advance(TICK);
        assert_eq!(app.game.elapsed(), TICK * (MAX_CATCH_UP_TICKS + 2));
    }
}

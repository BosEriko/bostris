use std::io;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};

use crate::bag::Rng;
use crate::bot::Bot;
use crate::config::{self, Config};
use crate::game::{Game, Settings, TICK};
use crate::highscores::HighScores;
use crate::input::{Action, AutoShift, Direction, Shift, is_quit_chord};
use crate::settings::{Outcome, SettingsMenu};
use crate::ui::{self, Renderer, Screen, Title, Versus, View};

const FRAME: Duration = Duration::from_nanos(16_666_667);
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
    title: Title,
    size: (u16, u16),
    screen: Screen,
    scores: HighScores,
    renderer: Renderer,
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
}

impl App {
    pub fn new(config: Config, scores: HighScores, precise_keys: bool) -> Self {
        let renderer = Renderer::new(config.color_mode, &config.keymap);
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
            title,
            size: (0, 0),
            screen: Screen::Ready,
            scores,
            renderer,
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
        }
    }

    pub fn save_error(&self) -> Option<&str> {
        self.save_error.as_deref()
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let size = terminal.size()?;
        self.size = (size.width, size.height);
        self.update_fits();
        let mut next_tick = Instant::now();
        let mut last_draw: Option<Instant> = None;
        let mut was_simulating = false;

        while !self.quit {
            let now = Instant::now();
            let needs_draw = self.dirty || self.revision() != self.drawn_revision;
            let mut draw_wait = None;
            if needs_draw {
                match last_draw.map(|t| now.duration_since(t)) {
                    Some(since) if since < FRAME => draw_wait = Some(FRAME - since),
                    _ => {
                        self.draw(terminal)?;
                        last_draw = Some(Instant::now());
                    }
                }
            }

            let simulating = self.is_simulating();
            let tick_wait = simulating.then(|| next_tick.saturating_duration_since(Instant::now()));
            let timeout = match (tick_wait, draw_wait) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };

            let ready = match timeout {
                Some(timeout) => event::poll(timeout)?,
                None => true,
            };
            if ready {
                self.handle_event(event::read()?);
                while event::poll(Duration::ZERO)? {
                    self.handle_event(event::read()?);
                }
            }

            let simulating = self.is_simulating();
            let now = Instant::now();
            if simulating && !was_simulating {
                next_tick = now + TICK;
            } else if simulating {
                let mut steps = 0;
                while next_tick <= now && steps < MAX_CATCH_UP_TICKS {
                    self.step();
                    next_tick += TICK;
                    steps += 1;
                }
                if steps == MAX_CATCH_UP_TICKS {
                    next_tick = now + TICK;
                }
            }
            was_simulating = simulating;
            self.check_game_over();
        }
        Ok(())
    }

    fn revision(&self) -> (u64, u64) {
        (
            self.game.revision(),
            self.versus.as_ref().map_or(0, |m| m.opponent.revision()),
        )
    }

    fn update_fits(&mut self) {
        let versus = self.versus.is_some() && self.screen != Screen::Ready;
        self.fits = ui::fits(self.size.0, self.size.1, versus);
    }

    fn is_simulating(&self) -> bool {
        self.screen == Screen::Playing && self.fits
    }

    fn draw(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let view = View {
            game: &self.game,
            screen: self.screen,
            scores: &self.scores,
            ghost: self.config.ghost,
            preview_count: self.config.preview_count,
            menu: self.menu.as_ref(),
            key_releases: self.precise_keys,
            versus: self.versus.as_ref().map(|m| Versus {
                opponent: &m.opponent,
                level: m.bot.level(),
            }),
            title: self.title,
        };
        let renderer = &mut self.renderer;
        terminal.draw(|frame| renderer.render(frame, &view))?;
        self.drawn_revision = self.revision();
        self.dirty = false;
        Ok(())
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
        self.game.tick(TICK);
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
        self.dirty = true;
    }

    fn release_keys(&mut self) {
        self.shifter.release_all();
        self.game.set_soft_drop(false);
    }

    fn restart(&mut self) {
        if self.versus.is_some() {
            self.start_versus();
        } else {
            self.start_marathon();
        }
    }

    fn start_marathon(&mut self) {
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
        self.versus = None;
        self.game = Game::new(self.config.settings.clone(), Rng::from_entropy());
        self.set_screen(Screen::Ready);
        self.update_fits();
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
        self.renderer = Renderer::new(self.config.color_mode, &self.config.keymap);
        if self.settings_return == Screen::Ready {
            self.game = Game::new(self.config.settings.clone(), Rng::from_entropy());
        } else {
            self.game.set_settings(self.config.settings.clone());
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

    fn handle_event(&mut self, event: Event) {
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
            Screen::Ready => {}
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

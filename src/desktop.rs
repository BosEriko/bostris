use std::sync::Mutex;
use std::time::Instant;

use crossterm::event::{Event, KeyEvent, KeyEventKind, KeyModifiers};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::app::App;
use crate::config;
use crate::highscores::{self, HighScores};
use crate::input::parse_key;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum KeyKind {
    Press,
    Repeat,
    Release,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum InputEvent {
    Key {
        key: String,
        kind: KeyKind,
        #[serde(default)]
        control: bool,
    },
    Focus {
        blur: bool,
    },
    Ui {
        action: String,
        index: Option<usize>,
        delta: Option<i64>,
    },
}

impl InputEvent {
    fn into_event(self) -> Option<Event> {
        match self {
            Self::Key { key, kind, control } => {
                let code = parse_key(&key)?;
                let kind = match kind {
                    KeyKind::Press => KeyEventKind::Press,
                    KeyKind::Repeat => KeyEventKind::Repeat,
                    KeyKind::Release => KeyEventKind::Release,
                };
                let modifiers = if control {
                    KeyModifiers::CONTROL
                } else {
                    KeyModifiers::NONE
                };
                Some(Event::Key(KeyEvent::new_with_kind(code, modifiers, kind)))
            }
            Self::Focus { blur: true } => Some(Event::FocusLost),
            Self::Focus { blur: false } | Self::Ui { .. } => None,
        }
    }
}

#[derive(Serialize)]
struct Frame {
    view: Option<crate::ui::Snapshot>,
    width: u16,
    height: u16,
    playing: bool,
    quit: bool,
    error: Option<String>,
    screen: &'static str,
}

struct Desktop {
    app: App,
    size: (u16, u16),
    last_frame: Instant,
    warnings: Vec<String>,
}

impl Desktop {
    fn new(mut app: App, warnings: Vec<String>) -> Self {
        app.handle_event(Event::Resize(1, 1));
        Self {
            app,
            size: (1, 1),
            last_frame: Instant::now(),
            warnings,
        }
    }

    fn frame(&mut self, width: u16, height: u16, events: Vec<InputEvent>) -> Result<Frame, String> {
        let now = Instant::now();
        self.app.advance(now.duration_since(self.last_frame));
        self.last_frame = now;
        let width = width.clamp(1, 8192);
        let height = height.clamp(1, 8192);
        if self.size != (width, height) {
            self.size = (width, height);
            self.app.handle_event(Event::Resize(width, height));
        }
        for event in events {
            match event {
                InputEvent::Ui {
                    action,
                    index,
                    delta,
                } => self.app.handle_ui(&action, index, delta.unwrap_or(1)),
                event => {
                    if let Some(event) = event.into_event() {
                        self.app.handle_event(event);
                    }
                }
            }
        }
        let view = self
            .app
            .needs_draw()
            .then(|| crate::ui::snapshot(&self.app.view(), self.app.screen_name()));
        self.app.mark_drawn();
        let mut errors = self.warnings.clone();
        if let Some(error) = self.app.save_error() {
            errors.push(error.to_owned());
        }
        Ok(Frame {
            view,
            width,
            height,
            playing: self.app.is_simulating(),
            quit: self.app.should_quit(),
            error: (!errors.is_empty()).then(|| errors.join("; ")),
            screen: self.app.screen_name(),
        })
    }
}

#[tauri::command]
fn frame(
    width: u16,
    height: u16,
    events: Vec<InputEvent>,
    state: State<'_, Mutex<Desktop>>,
    handle: tauri::AppHandle,
) -> Result<Frame, String> {
    let result = state
        .lock()
        .map_err(|_| "The game state is unavailable. Please restart bostris.".to_owned())?
        .frame(width, height, events)?;
    if result.quit {
        handle.exit(0);
    }
    Ok(result)
}

pub fn run() {
    let (config, warnings) = config::load();
    let scores = HighScores::load(highscores::default_path());
    tauri::Builder::default()
        .manage(Mutex::new(Desktop::new(
            App::new(config, scores, true),
            warnings,
        )))
        .invoke_handler(tauri::generate_handler![frame])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Focused(false))
                && let Ok(mut desktop) = window.state::<Mutex<Desktop>>().lock()
            {
                desktop.app.handle_event(Event::FocusLost);
            }
        })
        .run(tauri::generate_context!())
        .expect("could not run bostris");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn desktop() -> Desktop {
        Desktop::new(
            App::new(config::Config::default(), HighScores::load(None), true),
            Vec::new(),
        )
    }

    fn key(key: &str, kind: KeyKind) -> InputEvent {
        InputEvent::Key {
            key: key.into(),
            kind,
            control: false,
        }
    }

    fn action(action: &str, index: Option<usize>) -> InputEvent {
        InputEvent::Ui {
            action: action.into(),
            index,
            delta: None,
        }
    }

    #[test]
    fn returns_graphical_title_and_stops_idle_updates() {
        let mut desktop = desktop();
        let frame = desktop.frame(1152, 828, vec![]).unwrap();
        let view = frame.view.unwrap();
        assert_eq!(view.player.board.len(), 20);
        assert_eq!(view.controls.len(), 12);
        assert_eq!(frame.screen, "Title");
        assert!(!frame.playing);
        assert!(desktop.frame(1152, 828, vec![]).unwrap().view.is_none());
    }

    #[test]
    fn mouse_actions_start_versus_and_open_settings() {
        let mut desktop = desktop();
        let frame = desktop
            .frame(
                1152,
                828,
                vec![action("select-mode", Some(1)), action("start", None)],
            )
            .unwrap();
        assert!(frame.playing);
        assert!(frame.view.unwrap().opponent.is_some());
        desktop.app.advance(Duration::from_secs(1));
        let frame = desktop
            .frame(1152, 828, vec![action("settings", None)])
            .unwrap();
        let settings = frame.view.unwrap().settings.unwrap();
        assert_eq!(settings.items.len(), 31);
        assert!(!frame.playing);
        let before = settings.items[1].value.clone();
        let frame = desktop
            .frame(1152, 828, vec![action("setting", Some(1))])
            .unwrap();
        assert_ne!(frame.view.unwrap().settings.unwrap().items[1].value, before);
    }

    #[test]
    fn key_events_move_and_focus_loss_pauses() {
        let mut desktop = desktop();
        let frame = desktop
            .frame(1152, 828, vec![key("Enter", KeyKind::Press)])
            .unwrap();
        let before = frame.view.unwrap().player.active[0].x;
        let frame = desktop
            .frame(
                1152,
                828,
                vec![key("Left", KeyKind::Press), key("Left", KeyKind::Release)],
            )
            .unwrap();
        assert_eq!(frame.view.unwrap().player.active[0].x, before - 1);
        let frame = desktop
            .frame(1152, 828, vec![InputEvent::Focus { blur: true }])
            .unwrap();
        assert_eq!(frame.screen, "Paused");
        assert!(!frame.playing);
        desktop.app.advance(Duration::from_secs(10));
        assert!(!desktop.app.needs_draw());
    }

    #[test]
    fn resize_pauses_and_resume_requires_enough_space() {
        let mut desktop = desktop();
        desktop
            .frame(1152, 828, vec![action("start", None)])
            .unwrap();
        assert_eq!(desktop.frame(200, 200, vec![]).unwrap().screen, "Paused");
        assert!(
            !desktop
                .frame(200, 200, vec![action("resume", None)])
                .unwrap()
                .playing
        );
        assert!(
            desktop
                .frame(1152, 828, vec![action("resume", None)])
                .unwrap()
                .playing
        );
    }

    #[test]
    fn settings_clicks_cannot_capture_unrelated_rows() {
        let mut desktop = desktop();
        desktop
            .frame(1152, 828, vec![action("settings", None)])
            .unwrap();
        let frame = desktop
            .frame(1152, 828, vec![action("rebind", Some(1))])
            .unwrap();
        assert!(frame.view.is_none());
        let frame = desktop
            .frame(1152, 828, vec![action("rebind", Some(16))])
            .unwrap();
        assert!(frame.view.unwrap().settings.unwrap().capturing);
        let frame = desktop
            .frame(1152, 828, vec![key("j", KeyKind::Press)])
            .unwrap();
        let menu = frame.view.unwrap().settings.unwrap();
        assert!(!menu.capturing);
        assert_eq!(menu.items[16].value, "J");
    }
}

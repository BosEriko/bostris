use serde::Serialize;

use crate::board::{VISIBLE_HEIGHT, WIDTH};
use crate::bot::Level;
use crate::config::Config;
use crate::game::{Game, ToastEvent};
use crate::highscores::{Entry, HighScores};
use crate::input::{Action, key_label};
use crate::piece::{Kind, Piece};
use crate::scoring::Spin;
use crate::settings::{Field, ITEMS, Item, SettingsMenu};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Ready,
    Learn,
    Playing,
    Paused,
    GameOver { rank: Option<usize> },
    VersusOver { won: bool },
    Settings,
}

#[derive(Clone, Copy)]
pub struct Versus<'a> {
    pub opponent: &'a Game,
    pub level: Level,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Title {
    pub selected: usize,
    pub level: Level,
}

pub struct View<'a> {
    pub game: &'a Game,
    pub screen: Screen,
    pub scores: &'a HighScores,
    pub ghost: bool,
    pub preview_count: usize,
    pub menu: Option<&'a SettingsMenu>,
    pub versus: Option<Versus<'a>>,
    pub title: Title,
    pub config: &'a Config,
    pub training: Option<&'a crate::training::Training>,
}

#[derive(Serialize)]
pub struct Block {
    pub x: i8,
    pub y: i8,
    pub kind: &'static str,
}

#[derive(Serialize)]
pub struct GameView {
    pub board: Vec<Vec<Option<&'static str>>>,
    pub active: Vec<Block>,
    pub ghost: Vec<Block>,
    pub hold: Option<&'static str>,
    pub next: Vec<&'static str>,
    pub can_hold: bool,
    pub score: u64,
    pub lines: u32,
    pub level: u32,
    pub elapsed: u64,
    pub pieces: u32,
    pub finesse: f32,
    pub faults: u32,
    pub garbage: u32,
    pub attack: u32,
    pub toast: Option<String>,
    pub clearing: Vec<usize>,
}

#[derive(Serialize)]
pub struct SettingView {
    pub index: usize,
    pub kind: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub value: String,
    pub toggle: bool,
    pub selected: bool,
}

#[derive(Serialize)]
pub struct MenuView {
    pub items: Vec<SettingView>,
    pub capturing: bool,
    pub status: Option<String>,
}

#[derive(Serialize)]
pub struct ControlView {
    pub label: &'static str,
    pub keys: String,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub screen: &'static str,
    pub mode: usize,
    pub bot_level: &'static str,
    pub player: GameView,
    pub opponent: Option<GameView>,
    pub best: u64,
    pub high_scores: Vec<Entry>,
    pub settings: Option<MenuView>,
    pub controls: Vec<ControlView>,
    pub palette: &'static str,
    pub lessons: Vec<crate::training::Lesson>,
    pub training: Option<crate::training::TrainingView>,
}

pub fn fits(width: u16, height: u16, versus: bool) -> bool {
    width >= if versus { 540 } else { 300 } && height >= 360
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::I => "I",
        Kind::O => "O",
        Kind::T => "T",
        Kind::S => "S",
        Kind::Z => "Z",
        Kind::J => "J",
        Kind::L => "L",
        Kind::Garbage => "Garbage",
    }
}

fn blocks(piece: Option<Piece>) -> Vec<Block> {
    piece.map_or_else(Vec::new, |piece| {
        piece
            .cells()
            .filter(|&(x, y)| x >= 0 && x < WIDTH as i8 && y >= 0 && y < VISIBLE_HEIGHT as i8)
            .map(|(x, y)| Block {
                x,
                y: VISIBLE_HEIGHT as i8 - 1 - y,
                kind: kind_name(piece.kind),
            })
            .collect()
    })
}

fn game_view(game: &Game, ghost: bool, preview_count: usize, hidden: bool) -> GameView {
    let scoring = game.scoring();
    let finesse = game.finesse();
    let toast = game.toast().map(|toast| match toast.event {
        ToastEvent::Fault { fault, retry } => format!(
            "{} · {} extra inputs",
            if retry { "Try again" } else { "Finesse fault" },
            fault.used - fault.optimal
        ),
        ToastEvent::Clear(clear) => {
            let label = if clear.perfect {
                "Perfect clear"
            } else if clear.spin == Spin::Full {
                "T-spin"
            } else if clear.spin == Spin::Mini {
                "T-spin mini"
            } else {
                match clear.lines {
                    1 => "Single",
                    2 => "Double",
                    3 => "Triple",
                    _ => "Tetris",
                }
            };
            format!("{label} +{}", clear.points)
        }
    });
    GameView {
        board: (0..VISIBLE_HEIGHT)
            .rev()
            .map(|y| {
                game.board()
                    .row(y)
                    .iter()
                    .map(|&kind| if hidden { None } else { kind.map(kind_name) })
                    .collect()
            })
            .collect(),
        active: if hidden {
            Vec::new()
        } else {
            blocks(game.active().copied())
        },
        ghost: if ghost && !hidden {
            blocks(game.ghost())
        } else {
            Vec::new()
        },
        hold: game.hold_piece().map(kind_name),
        next: game.preview().take(preview_count).map(kind_name).collect(),
        can_hold: game.can_hold(),
        score: scoring.score,
        lines: scoring.lines,
        level: scoring.level(),
        elapsed: game.elapsed().as_secs(),
        pieces: game.pieces_placed(),
        finesse: finesse.accuracy().unwrap_or(100.0),
        faults: finesse.faults,
        garbage: game.pending_garbage().1,
        attack: game.attack_total(),
        toast,
        clearing: game
            .line_clear_progress()
            .map_or_else(Vec::new, |(mask, _)| {
                (0..VISIBLE_HEIGHT)
                    .filter(|y| mask & (1 << y) != 0)
                    .map(|y| VISIBLE_HEIGHT - 1 - y)
                    .collect()
            }),
    }
}

fn menu_view(menu: &SettingsMenu) -> MenuView {
    MenuView {
        items: ITEMS
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let mut value = String::new();
                let (kind, label, description, toggle) = match *item {
                    Item::Header(label) => ("header", label, "", false),
                    Item::Field(field) => {
                        menu.write_value(field, &mut value);
                        (
                            "field",
                            field.label(),
                            field.description(),
                            matches!(
                                field,
                                Field::Ghost | Field::FinesseRetry | Field::Finesse180
                            ),
                        )
                    }
                    Item::Key(action) => {
                        menu.write_keys(action, &mut value);
                        ("key", action.label(), "", false)
                    }
                    Item::Reset => (
                        "reset",
                        "Restore defaults",
                        "Reset all settings and controls",
                        false,
                    ),
                    Item::Back => ("back", "Done", "", false),
                };
                SettingView {
                    index,
                    kind,
                    label,
                    description,
                    value,
                    toggle,
                    selected: index == menu.cursor(),
                }
            })
            .collect(),
        capturing: menu.capture().is_some(),
        status: menu.status().map(str::to_owned),
    }
}

pub fn snapshot(view: &View<'_>, screen: &'static str) -> Snapshot {
    Snapshot {
        screen,
        mode: view.title.selected,
        bot_level: view
            .versus
            .map_or(view.title.level, |versus| versus.level)
            .name(),
        player: game_view(
            view.game,
            view.ghost,
            view.preview_count,
            view.screen == Screen::Paused,
        ),
        opponent: view.versus.map(|versus| {
            game_view(
                versus.opponent,
                false,
                view.preview_count,
                view.screen == Screen::Paused,
            )
        }),
        best: view.scores.best(),
        high_scores: view.scores.entries().to_vec(),
        settings: view.menu.map(menu_view),
        controls: Action::ALL
            .into_iter()
            .map(|action| ControlView {
                label: action.label(),
                keys: view
                    .config
                    .keymap
                    .keys_for(action)
                    .map(key_label)
                    .collect::<Vec<_>>()
                    .join(" / "),
            })
            .collect(),
        palette: view.config.color_mode.name(),
        lessons: if view.screen == Screen::Learn {
            crate::training::lessons().to_vec()
        } else {
            Vec::new()
        },
        training: view.training.map(crate::training::Training::view),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bag::Rng;
    use crate::piece::Rotation;

    #[test]
    fn graphical_board_is_top_down_and_previews_respect_settings() {
        let game = Game::new(Default::default(), Rng::new(1));
        let view = game_view(&game, true, 3, false);
        assert_eq!(view.board.len(), 20);
        assert!(view.board.iter().all(|row| row.len() == 10));
        assert_eq!(view.next.len(), 3);
        assert_eq!(view.ghost.len(), 4);
        assert!(view.ghost.iter().all(|cell| (0..20).contains(&cell.y)));
        let piece = Piece {
            kind: Kind::O,
            rotation: Rotation::Spawn,
            x: 0,
            y: -1,
        };
        let cells = blocks(Some(piece));
        assert!(cells.iter().any(|cell| cell.y == 19));
        assert!(
            game_view(&game, false, 0, true)
                .board
                .iter()
                .flatten()
                .all(Option::is_none)
        );
        assert!(game_view(&game, false, 0, true).active.is_empty());
    }
}

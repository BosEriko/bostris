use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::bag::Rng;
use crate::board::{Board, VISIBLE_HEIGHT, WIDTH};
use crate::game::{Game, Settings};
use crate::piece::{Kind, Piece, Rotation};
use crate::scoring::Spin;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Placement {
    pub kind: String,
    pub rotation: u8,
    pub x: i8,
    pub y: i8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub category: String,
    pub difficulty: String,
    pub summary: String,
    pub when: String,
    pub steps: Vec<String>,
    pub tips: Vec<String>,
    pub source: String,
    pub diagram: Option<Vec<String>>,
    pub practice: Option<String>,
    #[serde(default)]
    pub placements: Vec<Placement>,
}

#[derive(Clone, Serialize)]
pub struct Target {
    pub x: i8,
    pub y: i8,
    pub kind: String,
}

#[derive(Clone, Serialize)]
pub struct TrainingView {
    pub lesson: Lesson,
    pub status: String,
    pub feedback: String,
    pub hint: String,
    pub attempts: u32,
    pub target: Vec<Target>,
    pub sandbox: bool,
    pub step: usize,
    pub total_steps: usize,
}

enum Goal {
    Spin {
        spin: Spin,
        lines: u32,
        target: Piece,
    },
    Placement {
        target: Piece,
        kick: bool,
    },
    Opener(Vec<Piece>),
    Sandbox,
}

pub struct Training {
    index: usize,
    goal: Goal,
    status: &'static str,
    feedback: String,
    hint: String,
    attempts: u32,
    step: usize,
    observed: u32,
}

pub fn lessons() -> &'static [Lesson] {
    static LESSONS: OnceLock<Vec<Lesson>> = OnceLock::new();
    LESSONS.get_or_init(|| {
        serde_json::from_str(include_str!("../frontend/lessons.json"))
            .expect("the bundled lesson catalog must contain valid lessons")
    })
}

fn kind(name: &str) -> Option<Kind> {
    Some(match name {
        "I" => Kind::I,
        "O" => Kind::O,
        "T" => Kind::T,
        "S" => Kind::S,
        "Z" => Kind::Z,
        "J" => Kind::J,
        "L" => Kind::L,
        _ => return None,
    })
}

fn piece(kind: Kind, rotation: Rotation, x: i8, y: i8) -> Piece {
    Piece {
        kind,
        rotation,
        x,
        y,
    }
}

fn same_cells(a: Piece, b: Piece) -> bool {
    let mut a_cells: Vec<_> = a.cells().collect();
    let mut b_cells: Vec<_> = b.cells().collect();
    a_cells.sort_unstable();
    b_cells.sort_unstable();
    a.kind == b.kind && a_cells == b_cells
}

fn slot(double: bool, mini: bool, zero: bool) -> Board {
    let mut board = Board::default();
    for x in 0..WIDTH as i8 {
        if x != 4 && (!zero || x == 3 || x == 5) {
            board.set(x, 0, Some(Kind::Garbage));
        }
        if !zero && !(3..=5).contains(&x) && (double || x != 8) {
            board.set(x, 1, Some(Kind::Garbage));
        }
    }
    board.set(3, 2, Some(Kind::Garbage));
    if double {
        board.set(2, 2, Some(Kind::Garbage));
        board.set(0, 2, Some(Kind::Garbage));
    }
    if mini {
        board.set(2, 1, Some(Kind::Garbage));
    }
    board
}

fn mirrored(board: &Board) -> Board {
    let mut result = Board::default();
    for y in 0..VISIBLE_HEIGHT {
        for (x, &cell) in board.row(y).iter().enumerate() {
            result.set((WIDTH - 1 - x) as i8, y as i8, cell);
        }
    }
    result
}

impl Training {
    pub fn start(index: usize, settings: Settings) -> Option<(Self, Game)> {
        let lesson = lessons().get(index)?;
        let practice = lesson.practice.as_deref()?;
        let mut board = Board::default();
        let active;
        let goal;
        let hint;
        match practice {
            "opener-sandbox" => {
                let game = Game::new(
                    Settings {
                        finesse_retry: false,
                        line_clear_delay: std::time::Duration::ZERO,
                        ..settings
                    },
                    Rng::new(0xB057_215),
                );
                return Some((
                    Self {
                        index,
                        goal: Goal::Sandbox,
                        status: "active",
                        feedback: "Ungraded practice.".into(),
                        hint: "No gravity. Soft drop to lower; hard drop to lock.".into(),
                        attempts: 1,
                        step: 0,
                        observed: 0,
                    },
                    game,
                ));
            }
            "opener-build" => {
                let targets: Vec<_> = lesson
                    .placements
                    .iter()
                    .map(|placement| {
                        let rotation = match placement.rotation {
                            0 => Rotation::Spawn,
                            1 => Rotation::Right,
                            2 => Rotation::Reverse,
                            3 => Rotation::Left,
                            _ => return None,
                        };
                        Some(piece(
                            kind(&placement.kind)?,
                            rotation,
                            placement.x,
                            placement.y,
                        ))
                    })
                    .collect::<Option<_>>()?;
                let first = *targets.first()?;
                active = Piece::spawn(first.kind);
                hint = format!(
                    "Place {} in the outline. Hard drop to lock.",
                    lesson.placements[0].kind
                );
                goal = Goal::Opener(targets);
            }
            "t-spin-double" | "t-spin-single" | "t-spin-zero" => {
                let lines = match practice {
                    "t-spin-double" => 2,
                    "t-spin-single" => 1,
                    _ => 0,
                };
                board = slot(lines == 2, false, lines == 0);
                active = piece(Kind::T, Rotation::Right, 3, 0);
                goal = Goal::Spin {
                    spin: Spin::Full,
                    lines,
                    target: piece(Kind::T, Rotation::Reverse, 3, 0),
                };
                hint = "Rotate clockwise once, then hard drop. Do not move after rotating.".into();
            }
            "t-spin-double-mirror" => {
                board = mirrored(&slot(true, false, false));
                active = piece(Kind::T, Rotation::Left, 4, 0);
                goal = Goal::Spin {
                    spin: Spin::Full,
                    lines: 2,
                    target: piece(Kind::T, Rotation::Reverse, 4, 0),
                };
                hint = "Rotate counterclockwise once, then hard drop.".into();
            }
            "t-spin-mini" => {
                board = slot(false, true, false);
                active = piece(Kind::T, Rotation::Spawn, 3, 0);
                goal = Goal::Spin {
                    spin: Spin::Mini,
                    lines: 1,
                    target: piece(Kind::T, Rotation::Right, 3, 0),
                };
                hint = "Rotate clockwise once, then hard drop.".into();
            }
            "i-wall-kick" => {
                active = piece(Kind::I, Rotation::Left, 8, 0);
                goal = Goal::Placement {
                    target: piece(Kind::I, Rotation::Reverse, 6, -1),
                    kick: true,
                };
                hint = "Rotate counterclockwise to kick left, then hard drop.".into();
            }
            "t-floor-kick" => {
                active = piece(Kind::T, Rotation::Spawn, 4, -1);
                goal = Goal::Placement {
                    target: piece(Kind::T, Rotation::Right, 3, 0),
                    kick: true,
                };
                hint = "Rotate clockwise to kick up and left, then hard drop.".into();
            }
            "j-tuck" | "l-tuck" => {
                let mirror = practice == "l-tuck";
                for x in 0..WIDTH as i8 {
                    if !(3..=4).contains(&x) {
                        board.set(x, 0, Some(Kind::Garbage));
                    }
                }
                board.set(5, 2, Some(Kind::Garbage));
                if mirror {
                    board = mirrored(&board);
                    active = piece(Kind::L, Rotation::Right, 4, 0);
                    goal = Goal::Placement {
                        target: piece(Kind::L, Rotation::Spawn, 4, 0),
                        kick: false,
                    };
                    hint = "Rotate counterclockwise under the overhang, then hard drop.".into();
                } else {
                    active = piece(Kind::J, Rotation::Left, 3, 0);
                    goal = Goal::Placement {
                        target: piece(Kind::J, Rotation::Spawn, 3, 0),
                        kick: false,
                    };
                    hint = "Rotate clockwise under the overhang, then hard drop.".into();
                }
            }
            _ => return None,
        }
        if board.collides(&active) {
            return None;
        }
        let queue = match &goal {
            Goal::Opener(targets) => {
                Some(targets.iter().skip(1).map(|target| target.kind).collect())
            }
            _ => None,
        };
        let mut game = Game::practice(settings, board, active);
        if let Some(queue) = queue {
            game.set_practice_queue(queue);
        }
        Some((
            Self {
                index,
                goal,
                status: "active",
                feedback: "Match the outline.".into(),
                hint,
                attempts: 1,
                step: 0,
                observed: 0,
            },
            game,
        ))
    }

    pub fn retry(&self, settings: Settings) -> Option<(Self, Game)> {
        let (mut training, game) = Self::start(self.index, settings)?;
        training.attempts = self.attempts + 1;
        Some((training, game))
    }

    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    pub fn sandbox(&self) -> bool {
        matches!(self.goal, Goal::Sandbox)
    }

    pub fn observe(&mut self, game: &mut Game) {
        if !self.is_active() {
            return;
        }
        if game.is_over() {
            self.status = "retry";
            self.feedback = "Top out. Retry.".into();
            return;
        }
        if game.pieces_placed() == self.observed {
            return;
        }
        self.observed = game.pieces_placed();
        if self.sandbox() {
            return;
        }
        let Some(placement) = game.last_placement() else {
            return;
        };
        let success = match &self.goal {
            Goal::Spin {
                spin,
                lines,
                target,
            } => {
                same_cells(placement.piece, *target)
                    && placement.spin == *spin
                    && placement.lines == *lines
            }
            Goal::Placement { target, kick } => {
                same_cells(placement.piece, *target)
                    && placement.rotations > 0
                    && (!kick || placement.kicks > 0)
            }
            Goal::Opener(targets) => targets
                .get(self.step)
                .is_some_and(|target| same_cells(placement.piece, *target)),
            Goal::Sandbox => true,
        };
        if !success {
            self.status = "retry";
            self.feedback = match &self.goal {
                Goal::Spin { .. } => {
                    "Spin not matched. Retry: rotate into the outline, then hard drop.".into()
                }
                Goal::Placement { kick: true, .. } => {
                    "Kick not matched. Retry and lock in the outline.".into()
                }
                _ => "Placement not matched. Retry.".into(),
            };
            return;
        }
        self.step += 1;
        if let Goal::Opener(targets) = &self.goal {
            if let Some(next) = targets.get(self.step) {
                self.feedback = format!(
                    "{} placed. Next: {}.",
                    self.step,
                    lessons()[self.index].placements[self.step].kind
                );
                self.hint = format!(
                    "Place {} in the outline, then hard drop.",
                    lessons()[self.index].placements[self.step].kind
                );
                if game.active().is_some_and(|piece| piece.kind != next.kind) {
                    self.status = "retry";
                    self.feedback = "Queue changed. Retry.".into();
                }
                return;
            }
        }
        self.status = "success";
        self.feedback = if matches!(self.goal, Goal::Opener(_)) {
            "First bag complete.".into()
        } else {
            "Challenge complete.".into()
        };
    }

    pub fn view(&self) -> TrainingView {
        let target = match &self.goal {
            Goal::Spin { target, .. } | Goal::Placement { target, .. } => Some(*target),
            Goal::Opener(targets) => targets.get(self.step).copied(),
            Goal::Sandbox => None,
        };
        let targets = target.map_or_else(Vec::new, |piece| {
            piece
                .cells()
                .filter(|&(x, y)| x >= 0 && x < WIDTH as i8 && y >= 0 && y < VISIBLE_HEIGHT as i8)
                .map(|(x, y)| Target {
                    x,
                    y: VISIBLE_HEIGHT as i8 - 1 - y,
                    kind: format!("{:?}", piece.kind),
                })
                .collect()
        });
        TrainingView {
            lesson: lessons()[self.index].clone(),
            status: self.status.into(),
            feedback: self.feedback.clone(),
            hint: self.hint.clone(),
            attempts: self.attempts,
            target: targets,
            sandbox: self.sandbox(),
            step: self.step,
            total_steps: match &self.goal {
                Goal::Opener(targets) => targets.len(),
                Goal::Sandbox => 0,
                _ => 1,
            },
        }
    }
}

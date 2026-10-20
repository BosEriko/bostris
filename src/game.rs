use std::time::Duration;

use crate::bag::{Randomizer, Rng};
use crate::board::{Board, VISIBLE_HEIGHT};
use crate::finesse::{self, Fault, Stats as FinesseStats};
use crate::piece::{Kind, Piece, kicks};
use crate::scoring::{ClearEvent, Scoring, gravity_cells_per_second};

pub const TICK: Duration = Duration::from_nanos(16_666_667);
const MAX_CELLS_PER_TICK: f64 = 20.0;
const TOAST_DURATION: Duration = Duration::from_millis(1800);

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub start_level: u32,
    pub lock_delay: Duration,
    pub max_lock_resets: u32,
    pub line_clear_delay: Duration,
    pub soft_drop_factor: f64,
    pub finesse_retry: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_level: 1,
            lock_delay: Duration::from_millis(500),
            max_lock_resets: 15,
            line_clear_delay: Duration::from_millis(250),
            soft_drop_factor: 20.0,
            finesse_retry: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Falling,
    Clearing { rows: u64, elapsed: Duration },
    Over,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastEvent {
    Clear(ClearEvent),
    Fault { fault: Fault, retry: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toast {
    pub event: ToastEvent,
    pub age: Duration,
}

#[derive(Clone, Debug)]
pub struct Game {
    settings: Settings,
    board: Board,
    active: Piece,
    hold: Option<Kind>,
    hold_used: bool,
    randomizer: Randomizer,
    scoring: Scoring,
    phase: Phase,
    gravity_acc: f64,
    lock_timer: Duration,
    lock_resets: u32,
    lowest_y: i8,
    soft_drop: bool,
    elapsed: Duration,
    toast: Option<Toast>,
    pieces: u32,
    inputs: u32,
    spawns: u32,
    drop_points: u64,
    finesse: FinesseStats,
    revision: u64,
}

impl Game {
    pub fn new(settings: Settings, rng: Rng) -> Self {
        let mut randomizer = Randomizer::new(rng);
        let first = randomizer.next();
        let scoring = Scoring::new(settings.start_level);
        let mut game = Self {
            settings,
            board: Board::default(),
            active: Piece::spawn(first),
            hold: None,
            hold_used: false,
            randomizer,
            scoring,
            phase: Phase::Falling,
            gravity_acc: 0.0,
            lock_timer: Duration::ZERO,
            lock_resets: 0,
            lowest_y: 0,
            soft_drop: false,
            elapsed: Duration::ZERO,
            toast: None,
            pieces: 0,
            inputs: 0,
            spawns: 0,
            drop_points: 0,
            finesse: FinesseStats::default(),
            revision: 0,
        };
        game.spawn(first);
        game
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn active(&self) -> Option<&Piece> {
        matches!(self.phase, Phase::Falling).then_some(&self.active)
    }

    pub fn ghost(&self) -> Option<Piece> {
        self.active()
            .map(|piece| piece.shifted(0, -self.board.drop_distance(piece)))
    }

    pub fn hold_piece(&self) -> Option<Kind> {
        self.hold
    }

    pub fn can_hold(&self) -> bool {
        !self.hold_used
    }

    pub fn preview(&self) -> impl Iterator<Item = Kind> + '_ {
        self.randomizer.preview()
    }

    pub fn scoring(&self) -> &Scoring {
        &self.scoring
    }

    #[cfg(test)]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn is_over(&self) -> bool {
        self.phase == Phase::Over
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub fn toast(&self) -> Option<&Toast> {
        self.toast.as_ref()
    }

    pub fn pieces_placed(&self) -> u32 {
        self.pieces
    }

    pub fn finesse(&self) -> FinesseStats {
        self.finesse
    }

    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
        self.touch();
    }

    pub fn spawn_id(&self) -> u32 {
        self.spawns
    }

    pub fn note_input(&mut self) {
        if self.phase == Phase::Falling {
            self.inputs += 1;
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn line_clear_progress(&self) -> Option<(u64, f32)> {
        match self.phase {
            Phase::Clearing { rows, elapsed } => {
                let total = self
                    .settings
                    .line_clear_delay
                    .as_secs_f32()
                    .max(f32::EPSILON);
                Some((rows, (elapsed.as_secs_f32() / total).min(1.0)))
            }
            _ => None,
        }
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    fn spawn(&mut self, kind: Kind) {
        self.active = Piece::spawn(kind);
        self.gravity_acc = 0.0;
        self.lock_timer = Duration::ZERO;
        self.lock_resets = 0;
        self.lowest_y = self.active.y;
        self.inputs = 0;
        self.spawns = self.spawns.wrapping_add(1);
        self.drop_points = 0;
        self.phase = if self.board.collides(&self.active) {
            Phase::Over
        } else {
            Phase::Falling
        };
        self.touch();
    }

    fn spawn_next(&mut self) {
        let kind = self.randomizer.next();
        self.spawn(kind);
    }

    fn on_ground(&self) -> bool {
        self.board.collides(&self.active.shifted(0, -1))
    }

    fn place(&mut self, candidate: Piece) {
        let was_grounded = self.on_ground();
        self.active = candidate;
        if self.active.y < self.lowest_y {
            self.lowest_y = self.active.y;
            self.lock_resets = 0;
        }
        if (was_grounded || self.on_ground()) && self.lock_resets < self.settings.max_lock_resets {
            self.lock_resets += 1;
            self.lock_timer = Duration::ZERO;
        }
        self.touch();
    }

    pub fn shift(&mut self, dx: i8) -> bool {
        if self.phase != Phase::Falling {
            return false;
        }
        let candidate = self.active.shifted(dx, 0);
        if self.board.collides(&candidate) {
            return false;
        }
        self.place(candidate);
        true
    }

    pub fn rotate(&mut self, clockwise: bool) -> bool {
        if self.phase != Phase::Falling {
            return false;
        }
        let from = self.active.rotation;
        let to = if clockwise { from.cw() } else { from.ccw() };
        let rotated = Piece {
            rotation: to,
            ..self.active
        };
        for (dx, dy) in kicks(self.active.kind, from, to) {
            let candidate = rotated.shifted(dx, dy);
            if !self.board.collides(&candidate) {
                self.place(candidate);
                return true;
            }
        }
        false
    }

    fn fall(&mut self) -> bool {
        let candidate = self.active.shifted(0, -1);
        if self.board.collides(&candidate) {
            return false;
        }
        self.active = candidate;
        if self.active.y < self.lowest_y {
            self.lowest_y = self.active.y;
            self.lock_resets = 0;
        }
        self.lock_timer = Duration::ZERO;
        self.touch();
        true
    }

    pub fn soft_drop_step(&mut self) -> bool {
        if self.phase != Phase::Falling || !self.fall() {
            return false;
        }
        self.gravity_acc = 0.0;
        self.add_soft_drop_point();
        true
    }

    fn add_soft_drop_point(&mut self) {
        self.scoring.soft_drop(1);
        self.drop_points += 1;
    }

    pub fn set_soft_drop(&mut self, active: bool) {
        self.soft_drop = active;
    }

    pub fn hard_drop(&mut self) {
        if self.phase != Phase::Falling {
            return;
        }
        let distance = self.board.drop_distance(&self.active);
        self.active = self.active.shifted(0, -distance);
        if self.lock_piece() {
            self.scoring.hard_drop(distance as u32);
        }
    }

    pub fn hold(&mut self) -> bool {
        if self.phase != Phase::Falling || self.hold_used {
            return false;
        }
        let current = self.active.kind;
        match self.hold.replace(current) {
            Some(kind) => self.spawn(kind),
            None => self.spawn_next(),
        }
        self.hold_used = true;
        true
    }

    fn check_finesse(&mut self) -> Option<Fault> {
        let result = finesse::evaluate(&self.board, &self.active, self.inputs)?;
        self.finesse.evaluated += 1;
        let fault = result.err()?;
        self.finesse.faults += 1;
        self.toast = Some(Toast {
            event: ToastEvent::Fault {
                fault,
                retry: self.settings.finesse_retry,
            },
            age: Duration::ZERO,
        });
        self.touch();
        Some(fault)
    }

    fn lock_piece(&mut self) -> bool {
        let fault = self.check_finesse();
        if fault.is_some() && self.settings.finesse_retry {
            self.scoring.revoke(self.drop_points);
            let kind = self.active.kind;
            self.spawn(kind);
            return false;
        }
        let locked_out = self
            .active
            .cells()
            .all(|(_, y)| y as usize >= VISIBLE_HEIGHT);
        self.board.lock(&self.active);
        self.hold_used = false;
        self.pieces += 1;
        self.touch();
        if locked_out {
            self.phase = Phase::Over;
            return true;
        }
        let rows = self.board.full_rows();
        if let Some(event) = self.scoring.lock(rows.count_ones())
            && fault.is_none()
        {
            self.toast = Some(Toast {
                event: ToastEvent::Clear(event),
                age: Duration::ZERO,
            });
        }
        if rows == 0 {
            self.spawn_next();
        } else if self.settings.line_clear_delay.is_zero() {
            self.finish_clear(rows);
        } else {
            self.phase = Phase::Clearing {
                rows,
                elapsed: Duration::ZERO,
            };
        }
        true
    }

    fn finish_clear(&mut self, rows: u64) {
        self.board.clear_rows(rows);
        self.spawn_next();
    }

    pub fn tick(&mut self, dt: Duration) {
        if self.phase == Phase::Over {
            return;
        }
        let previous_second = self.elapsed.as_secs();
        self.elapsed += dt;
        if self.elapsed.as_secs() != previous_second {
            self.touch();
        }
        if let Some(toast) = &mut self.toast {
            toast.age += dt;
            if toast.age >= TOAST_DURATION {
                self.toast = None;
                self.touch();
            }
        }
        match self.phase {
            Phase::Clearing { rows, elapsed } => {
                let elapsed = elapsed + dt;
                if elapsed >= self.settings.line_clear_delay {
                    self.finish_clear(rows);
                } else {
                    self.phase = Phase::Clearing { rows, elapsed };
                    self.touch();
                }
            }
            Phase::Falling => self.apply_gravity(dt),
            Phase::Over => {}
        }
    }

    fn apply_gravity(&mut self, dt: Duration) {
        let mut cells_per_second = gravity_cells_per_second(self.scoring.level());
        if self.soft_drop {
            cells_per_second *= self.settings.soft_drop_factor;
        }
        self.gravity_acc =
            (self.gravity_acc + cells_per_second * dt.as_secs_f64()).min(MAX_CELLS_PER_TICK);
        while self.gravity_acc >= 1.0 {
            if !self.fall() {
                break;
            }
            self.gravity_acc -= 1.0;
            if self.soft_drop {
                self.add_soft_drop_point();
            }
        }
        if !self.on_ground() {
            return;
        }
        self.gravity_acc = 0.0;
        self.lock_timer += dt;
        let resets_exhausted =
            self.settings.max_lock_resets > 0 && self.lock_resets >= self.settings.max_lock_resets;
        if self.lock_timer >= self.settings.lock_delay || resets_exhausted {
            self.lock_piece();
        }
    }

    #[cfg(test)]
    fn with_board(mut self, board: Board) -> Self {
        self.board = board;
        self
    }

    #[cfg(test)]
    fn set_active(&mut self, piece: Piece) {
        self.active = piece;
        self.lowest_y = piece.y;
        self.lock_resets = 0;
        self.lock_timer = Duration::ZERO;
        self.inputs = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::WIDTH;
    use crate::piece::Rotation;

    fn settings() -> Settings {
        Settings {
            line_clear_delay: Duration::ZERO,
            ..Settings::default()
        }
    }

    fn game() -> Game {
        Game::new(settings(), Rng::new(1))
    }

    fn piece(kind: Kind, rotation: Rotation, x: i8, y: i8) -> Piece {
        Piece {
            kind,
            rotation,
            x,
            y,
        }
    }

    fn sorted_cells(piece: &Piece) -> Vec<(i8, i8)> {
        let mut cells: Vec<_> = piece.cells().collect();
        cells.sort();
        cells
    }

    fn fill_row(board: &mut Board, y: i8, gap: Option<i8>) {
        for x in 0..WIDTH as i8 {
            if Some(x) != gap {
                board.set(x, y, Some(Kind::Z));
            }
        }
    }

    #[test]
    fn piece_moves_until_wall() {
        let mut game = game();
        game.set_active(piece(Kind::O, Rotation::Spawn, 3, 10));
        let mut moves = 0;
        while game.shift(-1) {
            moves += 1;
        }
        assert_eq!(moves, 4);
        assert_eq!(game.active().unwrap().x, -1);
        let mut moves = 0;
        while game.shift(1) {
            moves += 1;
        }
        assert_eq!(moves, 8);
    }

    #[test]
    fn piece_cannot_move_into_stack() {
        let mut board = Board::default();
        board.set(7, 11, Some(Kind::L));
        let mut game = game().with_board(board);
        game.set_active(piece(Kind::O, Rotation::Spawn, 3, 10));
        assert!(game.shift(1));
        assert!(!game.shift(1));
        assert_eq!(game.active().unwrap().x, 4);
    }

    #[test]
    fn rotation_in_open_space_uses_no_kick() {
        let mut game = game();
        game.set_active(piece(Kind::T, Rotation::Spawn, 4, 10));
        assert!(game.rotate(true));
        let active = *game.active().unwrap();
        assert_eq!(
            (active.x, active.y, active.rotation),
            (4, 10, Rotation::Right)
        );
        assert!(game.rotate(false));
        assert_eq!(game.active().unwrap().rotation, Rotation::Spawn);
    }

    #[test]
    fn t_kicks_off_left_wall() {
        let mut game = game();
        game.set_active(piece(Kind::T, Rotation::Right, -1, 10));
        assert!(game.rotate(true));
        let active = *game.active().unwrap();
        assert_eq!(active.rotation, Rotation::Reverse);
        assert_eq!(active.x, 0);
        assert!(sorted_cells(&active).iter().all(|&(x, _)| x >= 0));
    }

    #[test]
    fn i_kicks_off_right_wall() {
        let mut game = game();
        game.set_active(piece(Kind::I, Rotation::Left, 8, 10));
        assert!(game.rotate(false));
        let active = *game.active().unwrap();
        assert_eq!(active.rotation, Rotation::Reverse);
        assert_eq!(active.x, 6);
        assert_eq!(
            sorted_cells(&active),
            vec![(6, 11), (7, 11), (8, 11), (9, 11)]
        );
    }

    #[test]
    fn floor_kick_lifts_piece() {
        let mut game = game();
        game.set_active(piece(Kind::T, Rotation::Reverse, 4, -1));
        assert!(game.rotate(true));
        let active = *game.active().unwrap();
        assert_eq!(active.rotation, Rotation::Left);
        assert!(sorted_cells(&active).iter().all(|&(_, y)| y >= 0));
    }

    #[test]
    fn kick_falls_through_to_fifth_test() {
        let mut board = Board::default();
        board.set(4, 10, Some(Kind::Z));
        board.set(3, 12, Some(Kind::Z));
        let mut game = game().with_board(board);
        game.set_active(piece(Kind::T, Rotation::Spawn, 3, 10));
        assert!(game.rotate(true));
        let active = *game.active().unwrap();
        assert_eq!(
            (active.x, active.y, active.rotation),
            (2, 8, Rotation::Right)
        );
        assert_eq!(sorted_cells(&active), vec![(3, 8), (3, 9), (3, 10), (4, 9)]);
    }

    #[test]
    fn blocked_rotation_fails() {
        let mut board = Board::default();
        for y in 0..20 {
            board.set(0, y, Some(Kind::J));
            board.set(2, y, Some(Kind::J));
        }
        let mut game = game().with_board(board);
        game.set_active(piece(Kind::I, Rotation::Left, 0, 5));
        assert!(!game.rotate(true));
        assert_eq!(game.active().unwrap().rotation, Rotation::Left);
    }

    #[test]
    fn hard_drop_lands_on_ghost_and_scores() {
        let mut board = Board::default();
        board.set(4, 0, Some(Kind::S));
        let mut game = game().with_board(board);
        game.set_active(piece(Kind::O, Rotation::Spawn, 3, 10));
        let ghost = game.ghost().unwrap();
        assert_eq!(ghost.y, 0);
        game.hard_drop();
        assert_eq!(game.board().get(4, 1), Some(Kind::O));
        assert_eq!(game.board().get(5, 2), Some(Kind::O));
        assert_eq!(game.scoring().score, 20);
        assert_eq!(game.pieces_placed(), 1);
    }

    #[test]
    fn ghost_follows_active_piece() {
        let mut game = game();
        game.set_active(piece(Kind::I, Rotation::Spawn, 0, 10));
        assert_eq!(game.ghost().unwrap().y, -2);
        game.shift(1);
        let ghost = game.ghost().unwrap();
        assert_eq!((ghost.x, ghost.y), (1, -2));
        assert!(!game.board().collides(&ghost));
        assert!(game.board().collides(&ghost.shifted(0, -1)));
    }

    #[test]
    fn soft_drop_moves_down_and_scores() {
        let mut game = game();
        let start = game.active().unwrap().y;
        assert!(game.soft_drop_step());
        assert_eq!(game.active().unwrap().y, start - 1);
        assert_eq!(game.scoring().score, 1);
    }

    #[test]
    fn hold_is_limited_to_once_per_piece() {
        let mut game = game();
        let first = game.active().unwrap().kind;
        let next = game.preview().next().unwrap();
        assert!(game.hold());
        assert_eq!(game.hold_piece(), Some(first));
        assert_eq!(game.active().unwrap().kind, next);
        assert!(!game.can_hold());
        assert!(!game.hold());
        assert_eq!(game.active().unwrap().kind, next);
        game.hard_drop();
        assert!(game.can_hold());
        let current = game.active().unwrap().kind;
        assert!(game.hold());
        assert_eq!(game.active().unwrap().kind, first);
        assert_eq!(game.hold_piece(), Some(current));
        assert_eq!(game.active().unwrap().y, first.spawn_position().1);
    }

    #[test]
    fn completed_lines_are_cleared() {
        let mut board = Board::default();
        for y in 0..4 {
            fill_row(&mut board, y, Some(9));
        }
        board.set(1, 4, Some(Kind::Z));
        let mut game = game().with_board(board);
        game.set_active(piece(Kind::I, Rotation::Right, 7, 5));
        game.hard_drop();
        assert_eq!(game.scoring().lines, 4);
        assert_eq!(game.scoring().score, 800 + 2 * 5);
        assert_eq!(game.board().get(0, 0), None);
        assert_eq!(game.board().get(1, 0), Some(Kind::Z));
        assert_eq!(game.board().full_rows(), 0);
        assert!(matches!(
            game.toast().unwrap().event,
            ToastEvent::Clear(ClearEvent { lines: 4, .. })
        ));
    }

    #[test]
    fn line_clear_delay_holds_spawn() {
        let mut board = Board::default();
        fill_row(&mut board, 0, Some(4));
        let mut game = Game::new(Settings::default(), Rng::new(9)).with_board(board);
        game.set_active(piece(Kind::I, Rotation::Right, 2, 5));
        game.hard_drop();
        assert!(matches!(game.phase(), Phase::Clearing { .. }));
        assert!(game.active().is_none());
        assert!(!game.shift(1));
        let mut ticks = 0;
        while game.active().is_none() {
            game.tick(TICK);
            ticks += 1;
            assert!(ticks < 100);
        }
        assert_eq!(game.board().get(4, 0), Some(Kind::I));
        assert_eq!(game.board().get(4, 3), None);
        assert_eq!(game.scoring().lines, 1);
    }

    #[test]
    fn piece_locks_after_lock_delay() {
        let mut game = game();
        game.set_active(piece(Kind::O, Rotation::Spawn, 3, -1));
        let ticks_for_delay = (Duration::from_millis(500).as_nanos() / TICK.as_nanos()) as u32;
        for _ in 0..ticks_for_delay - 1 {
            game.tick(TICK);
        }
        assert_eq!(game.pieces_placed(), 0);
        game.tick(TICK);
        game.tick(TICK);
        assert_eq!(game.pieces_placed(), 1);
        assert_eq!(game.board().get(4, 0), Some(Kind::O));
    }

    #[test]
    fn movement_resets_lock_delay() {
        let mut game = game();
        game.set_active(piece(Kind::O, Rotation::Spawn, 3, -1));
        for step in 0..10 {
            for _ in 0..20 {
                game.tick(TICK);
            }
            game.shift(if step % 2 == 0 { 1 } else { -1 });
        }
        assert_eq!(game.pieces_placed(), 0);
    }

    #[test]
    fn lock_resets_are_limited() {
        let mut game = game();
        game.set_active(piece(Kind::O, Rotation::Spawn, 3, -1));
        for step in 0..15 {
            game.tick(TICK);
            assert!(game.shift(if step % 2 == 0 { 1 } else { -1 }));
        }
        assert_eq!(game.pieces_placed(), 0);
        game.tick(TICK);
        assert_eq!(game.pieces_placed(), 1);
    }

    #[test]
    fn zero_lock_resets_still_waits_for_lock_delay() {
        let mut game = Game::new(
            Settings {
                max_lock_resets: 0,
                ..settings()
            },
            Rng::new(1),
        );
        game.set_active(piece(Kind::O, Rotation::Spawn, 3, -1));
        game.tick(TICK);
        assert!(game.shift(1));
        for _ in 0..20 {
            game.tick(TICK);
        }
        assert_eq!(game.pieces_placed(), 0);
        for _ in 0..15 {
            game.tick(TICK);
        }
        assert_eq!(game.pieces_placed(), 1);
    }

    #[test]
    fn gravity_pulls_piece_down() {
        let mut game = game();
        let start = game.active().unwrap().y;
        for _ in 0..61 {
            game.tick(TICK);
        }
        assert_eq!(game.active().unwrap().y, start - 1);
        game.set_soft_drop(true);
        for _ in 0..6 {
            game.tick(TICK);
        }
        assert!(game.active().unwrap().y <= start - 2);
    }

    #[test]
    fn blocked_spawn_ends_game() {
        let mut board = Board::default();
        for y in 0..19 {
            fill_row(&mut board, y, Some(y % 10));
        }
        for x in 3..6 {
            board.set(x, 19, Some(Kind::J));
        }
        let mut game = game().with_board(board);
        game.set_active(piece(Kind::O, Rotation::Spawn, -1, 20));
        game.hard_drop();
        assert!(game.is_over());
        assert!(game.active().is_none());
        assert!(!game.hold());
    }

    #[test]
    fn locking_above_visible_field_ends_game() {
        let mut board = Board::default();
        for y in 0..20 {
            board.set(0, y, Some(Kind::J));
            board.set(1, y, Some(Kind::J));
        }
        let mut game = game().with_board(board);
        game.set_active(piece(Kind::O, Rotation::Spawn, -1, 21));
        game.hard_drop();
        assert!(game.is_over());
    }

    #[test]
    fn game_over_stops_simulation() {
        let mut game = game();
        game.phase = Phase::Over;
        let elapsed = game.elapsed();
        game.tick(TICK);
        assert_eq!(game.elapsed(), elapsed);
    }

    fn finesse_game(retry: bool) -> Game {
        Game::new(
            Settings {
                finesse_retry: retry,
                ..settings()
            },
            Rng::new(3),
        )
    }

    #[test]
    fn optimal_placement_is_not_a_fault() {
        let mut game = finesse_game(false);
        game.set_active(piece(Kind::T, Rotation::Spawn, 3, 10));
        game.note_input();
        game.shift(-1);
        game.hard_drop();
        let stats = game.finesse();
        assert_eq!((stats.evaluated, stats.faults), (1, 0));
        assert!(game.toast().is_none());
    }

    #[test]
    fn extra_inputs_count_as_fault() {
        let mut game = finesse_game(false);
        game.set_active(piece(Kind::T, Rotation::Spawn, 3, 10));
        for _ in 0..3 {
            game.note_input();
            game.rotate(true);
        }
        game.hard_drop();
        let stats = game.finesse();
        assert_eq!((stats.evaluated, stats.faults), (1, 1));
        assert_eq!(game.pieces_placed(), 1);
        assert!(matches!(
            game.toast().unwrap().event,
            ToastEvent::Fault {
                fault: Fault {
                    used: 3,
                    optimal: 1
                },
                retry: false
            }
        ));
    }

    #[test]
    fn retry_on_fault_respawns_same_piece() {
        let mut game = finesse_game(true);
        game.set_active(piece(Kind::T, Rotation::Spawn, 3, 10));
        let next = game.preview().next().unwrap();
        game.soft_drop_step();
        for _ in 0..3 {
            game.note_input();
            game.shift(-1);
        }
        game.note_input();
        game.shift(1);
        game.hard_drop();
        assert_eq!(game.pieces_placed(), 0);
        assert_eq!(game.scoring().score, 0);
        assert_eq!(game.board(), &Board::default());
        let active = *game.active().unwrap();
        assert_eq!(active, Piece::spawn(Kind::T));
        assert_eq!(game.preview().next(), Some(next));
        assert_eq!(game.finesse().faults, 1);
        game.note_input();
        game.shift(-1);
        game.hard_drop();
        assert_eq!(game.pieces_placed(), 1);
        assert_eq!(game.finesse().faults, 1);
        assert_eq!(game.finesse().evaluated, 2);
    }

    #[test]
    fn spawn_resets_input_count() {
        let mut game = finesse_game(false);
        for _ in 0..5 {
            game.note_input();
        }
        assert!(game.hold());
        game.hard_drop();
        assert_eq!(game.finesse().faults, 0);
    }
}

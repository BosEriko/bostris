use std::collections::VecDeque;
use std::time::Duration;

use crate::bag::{Randomizer, Rng};
use crate::board::{Board, VISIBLE_HEIGHT, WIDTH};
use crate::finesse::{self, Fault, Stats as FinesseStats};
use crate::piece::{Kind, Piece, Rotation, kicks, kicks_180};
use crate::scoring::{Clear, ClearEvent, Scoring, Spin, gravity_cells_per_second};

pub const TICK: Duration = Duration::from_nanos(16_666_667);
const MAX_CELLS_PER_TICK: f64 = 20.0;
const TOAST_DURATION: Duration = Duration::from_millis(1800);
pub const GARBAGE_DELAY: Duration = Duration::from_millis(500);
pub const GARBAGE_CAP: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Garbage {
    lines: u32,
    column: u8,
    wait: Duration,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub start_level: u32,
    pub lock_delay: Duration,
    pub max_lock_resets: u32,
    pub line_clear_delay: Duration,
    pub soft_drop_factor: f64,
    pub finesse_retry: bool,
    pub finesse_180: bool,
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
            finesse_180: false,
        }
    }
}

pub fn t_spin(board: &Board, piece: &Piece, spin_kick: Option<bool>) -> Spin {
    let Some(fifth_kick) = spin_kick else {
        return Spin::None;
    };
    if piece.kind != Kind::T {
        return Spin::None;
    }
    let Piece { x, y, rotation, .. } = *piece;
    let filled = |dx: i8, dy: i8| board.is_blocked(x + dx, y + dy);
    let (top_left, top_right, bottom_left, bottom_right) =
        (filled(0, 2), filled(2, 2), filled(0, 0), filled(2, 0));
    let corners = [top_left, top_right, bottom_left, bottom_right]
        .iter()
        .filter(|&&c| c)
        .count();
    if corners < 3 {
        return Spin::None;
    }
    let front = match rotation {
        Rotation::Spawn => top_left && top_right,
        Rotation::Right => top_right && bottom_right,
        Rotation::Reverse => bottom_left && bottom_right,
        Rotation::Left => top_left && bottom_left,
    };
    if front || fifth_kick {
        Spin::Full
    } else {
        Spin::Mini
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
    spin_kick: Option<bool>,
    pending: VecDeque<Garbage>,
    garbage_rng: Rng,
    outgoing: u32,
    attack_total: u32,
    gravity_level: Option<u32>,
    revision: u64,
}

impl Game {
    pub fn new(settings: Settings, mut rng: Rng) -> Self {
        let garbage_rng = Rng::new(rng.next_u64());
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
            spin_kick: None,
            pending: VecDeque::with_capacity(16),
            garbage_rng,
            outgoing: 0,
            attack_total: 0,
            gravity_level: None,
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

    pub fn set_gravity_level(&mut self, level: Option<u32>) {
        self.gravity_level = level;
    }

    pub fn receive_garbage(&mut self, lines: u32) {
        if lines == 0 || self.phase == Phase::Over {
            return;
        }
        let column = self.garbage_rng.below(WIDTH as u32) as u8;
        self.pending.push_back(Garbage {
            lines,
            column,
            wait: GARBAGE_DELAY,
        });
        self.touch();
    }

    pub fn take_outgoing(&mut self) -> u32 {
        std::mem::take(&mut self.outgoing)
    }

    pub fn attack_total(&self) -> u32 {
        self.attack_total
    }

    pub fn pending_garbage(&self) -> (u32, u32) {
        self.pending.iter().fold((0, 0), |(ready, total), g| {
            let ready = if g.wait.is_zero() {
                ready + g.lines
            } else {
                ready
            };
            (ready, total + g.lines)
        })
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
        self.spin_kick = None;
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
        self.spin_kick = None;
        true
    }

    pub fn rotate(&mut self, clockwise: bool) -> bool {
        let from = self.active.rotation;
        let to = if clockwise { from.cw() } else { from.ccw() };
        self.try_rotate(to, &kicks(self.active.kind, from, to), true)
    }

    pub fn rotate_180(&mut self) -> bool {
        let from = self.active.rotation;
        self.try_rotate(from.flip(), kicks_180(self.active.kind, from), false)
    }

    fn try_rotate(&mut self, to: Rotation, kicks: &[(i8, i8)], quarter: bool) -> bool {
        if self.phase != Phase::Falling {
            return false;
        }
        let rotated = Piece {
            rotation: to,
            ..self.active
        };
        for (index, &(dx, dy)) in kicks.iter().enumerate() {
            let candidate = rotated.shifted(dx, dy);
            if !self.board.collides(&candidate) {
                self.place(candidate);
                self.spin_kick = Some(quarter && index == 4);
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
        self.spin_kick = None;
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
        if distance > 0 {
            self.active = self.active.shifted(0, -distance);
            self.spin_kick = None;
        }
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
        let result = finesse::evaluate(
            &self.board,
            &self.active,
            self.inputs,
            self.settings.finesse_180,
        )?;
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
        let spin = t_spin(&self.board, &self.active, self.spin_kick);
        self.board.lock(&self.active);
        self.hold_used = false;
        self.pieces += 1;
        self.touch();
        if locked_out {
            self.phase = Phase::Over;
            return true;
        }
        let rows = self.board.full_rows();
        let perfect = rows != 0 && {
            let mut after = self.board.clone();
            after.clear_rows(rows);
            after.is_empty()
        };
        let clear = Clear {
            lines: rows.count_ones(),
            spin,
            perfect,
        };
        if let Some(event) = self.scoring.lock(clear) {
            self.send_attack(event.attack);
            if fault.is_none() {
                self.toast = Some(Toast {
                    event: ToastEvent::Clear(event),
                    age: Duration::ZERO,
                });
            }
        }
        if rows == 0 {
            if self.apply_garbage() {
                self.phase = Phase::Over;
                return true;
            }
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

    fn send_attack(&mut self, attack: u32) {
        self.attack_total += attack;
        let mut remaining = attack;
        while remaining > 0
            && let Some(front) = self.pending.front_mut()
        {
            let cancelled = remaining.min(front.lines);
            front.lines -= cancelled;
            remaining -= cancelled;
            if front.lines == 0 {
                self.pending.pop_front();
            }
        }
        self.outgoing += remaining;
    }

    fn apply_garbage(&mut self) -> bool {
        let mut budget = GARBAGE_CAP;
        let mut overflow = false;
        while budget > 0
            && let Some(front) = self.pending.front_mut()
            && front.wait.is_zero()
        {
            let lines = front.lines.min(budget);
            overflow |= self
                .board
                .push_garbage(lines as usize, usize::from(front.column));
            front.lines -= lines;
            budget -= lines;
            if front.lines == 0 {
                self.pending.pop_front();
            }
        }
        if budget != GARBAGE_CAP {
            self.touch();
        }
        overflow
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
        for garbage in &mut self.pending {
            if !garbage.wait.is_zero() {
                garbage.wait = garbage.wait.saturating_sub(dt);
                if garbage.wait.is_zero() {
                    self.revision = self.revision.wrapping_add(1);
                }
            }
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
        let level = self.gravity_level.unwrap_or_else(|| self.scoring.level());
        let mut cells_per_second = gravity_cells_per_second(level);
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
    fn rotate_180_flips_in_place() {
        let mut game = game();
        game.set_active(piece(Kind::T, Rotation::Spawn, 4, 10));
        assert!(game.rotate_180());
        let active = *game.active().unwrap();
        assert_eq!(
            (active.x, active.y, active.rotation),
            (4, 10, Rotation::Reverse)
        );
        assert!(game.rotate_180());
        assert_eq!(game.active().unwrap().rotation, Rotation::Spawn);
    }

    #[test]
    fn rotate_180_kicks_up_from_floor() {
        let mut game = game();
        game.set_active(piece(Kind::T, Rotation::Spawn, 4, -1));
        assert!(game.rotate_180());
        let active = *game.active().unwrap();
        assert_eq!(active.rotation, Rotation::Reverse);
        assert_eq!(active.y, 0);
        assert!(active.cells().all(|(_, y)| y >= 0));
    }

    #[test]
    fn finesse_counts_180_only_when_enabled() {
        for (allow, faults) in [(false, 0), (true, 1)] {
            let mut game = Game::new(
                Settings {
                    finesse_180: allow,
                    ..settings()
                },
                Rng::new(3),
            );
            game.set_active(piece(Kind::T, Rotation::Spawn, 3, 10));
            for _ in 0..2 {
                game.note_input();
                game.rotate(true);
            }
            game.hard_drop();
            assert_eq!(game.finesse().faults, faults);
        }
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

    fn tsd_board() -> Board {
        let mut board = Board::default();
        fill_row(&mut board, 0, Some(4));
        for x in 0..WIDTH as i8 {
            if !(3..=5).contains(&x) {
                board.set(x, 1, Some(Kind::Z));
            }
        }
        board.set(3, 2, Some(Kind::Z));
        board.set(2, 2, Some(Kind::Z));
        board
    }

    #[test]
    fn t_spin_double_is_detected() {
        let mut game = game().with_board(tsd_board());
        game.set_active(piece(Kind::T, Rotation::Right, 3, 0));
        assert!(game.rotate(true));
        game.hard_drop();
        assert_eq!(game.scoring().lines, 2);
        assert_eq!(game.attack_total(), 4);
        assert_eq!(game.take_outgoing(), 4);
        assert_eq!(game.take_outgoing(), 0);
        assert!(matches!(
            game.toast().unwrap().event,
            ToastEvent::Clear(ClearEvent {
                spin: Spin::Full,
                lines: 2,
                ..
            })
        ));
    }

    #[test]
    fn spin_needs_rotation_as_last_move() {
        let mut board = tsd_board();
        board.set(3, 2, None);
        board.set(2, 2, None);
        board.set(0, 2, Some(Kind::Z));
        let mut game = game().with_board(board);
        game.set_active(piece(Kind::T, Rotation::Reverse, 3, 3));
        game.hard_drop();
        assert_eq!(game.scoring().lines, 2);
        assert_eq!(game.attack_total(), 1);
    }

    #[test]
    fn garbage_waits_then_rises_on_next_lock() {
        let mut game = game();
        game.receive_garbage(3);
        assert_eq!(game.pending_garbage(), (0, 3));
        game.set_active(piece(Kind::O, Rotation::Spawn, 3, 10));
        game.hard_drop();
        assert_eq!(game.pending_garbage(), (0, 3));
        while game.pending_garbage().0 == 0 {
            game.tick(TICK);
        }
        let mut board_before_hole = 0;
        game.set_active(piece(Kind::O, Rotation::Spawn, -1, 10));
        game.hard_drop();
        assert_eq!(game.pending_garbage(), (0, 0));
        for y in 0..3 {
            let empty = (0..WIDTH as i8)
                .filter(|&x| game.board().get(x, y).is_none())
                .count();
            assert_eq!(empty, 1);
            board_before_hole += 1;
        }
        assert_eq!(board_before_hole, 3);
        assert_eq!(game.board().get(4, 3), Some(Kind::O));
    }

    #[test]
    fn attack_cancels_incoming_garbage() {
        let mut board = Board::default();
        for y in 0..4 {
            fill_row(&mut board, y, Some(9));
        }
        board.set(0, 4, Some(Kind::Z));
        let mut game = game().with_board(board);
        game.receive_garbage(3);
        game.receive_garbage(2);
        game.set_active(piece(Kind::I, Rotation::Right, 7, 5));
        game.hard_drop();
        assert_eq!(game.attack_total(), 4);
        assert_eq!(game.pending_garbage(), (0, 1));
        assert_eq!(game.take_outgoing(), 0);
    }

    #[test]
    fn garbage_is_capped_per_piece() {
        let mut game = game();
        game.receive_garbage(10);
        for _ in 0..40 {
            game.tick(TICK);
        }
        game.set_active(piece(Kind::O, Rotation::Spawn, -1, 30));
        game.hard_drop();
        assert_eq!(game.pending_garbage(), (2, 2));
        let garbage_rows = (0..12)
            .filter(|&y| game.board().get(9, y).is_some() || game.board().get(8, y).is_some())
            .count();
        assert!(garbage_rows >= 8);
    }

    #[test]
    fn perfect_clear_sends_bonus_attack() {
        let mut board = Board::default();
        for y in 0..4 {
            fill_row(&mut board, y, Some(9));
        }
        let mut game = game().with_board(board);
        game.set_active(piece(Kind::I, Rotation::Right, 7, 5));
        game.hard_drop();
        assert_eq!(game.attack_total(), 14);
        assert!(game.board().is_empty());
    }

    #[test]
    fn fixed_gravity_level_ignores_lines() {
        let mut game = game();
        game.set_gravity_level(Some(1));
        let start = game.active().unwrap().y;
        for _ in 0..30 {
            game.tick(TICK);
        }
        assert_eq!(game.active().unwrap().y, start);
    }
}

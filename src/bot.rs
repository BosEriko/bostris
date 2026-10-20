use std::time::Duration;

use crate::bag::Rng;
use crate::board::{Board, HEIGHT, WIDTH};
use crate::game::{Game, TICK, t_spin};
use crate::piece::{Kind, Piece, Rotation, kicks, kicks_180};
use crate::scoring::{Clear, Spin, attack};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    D,
    C,
    B,
    A,
    APlus,
    S,
    X,
}

impl Level {
    pub const ALL: [Level; 7] = [
        Level::D,
        Level::C,
        Level::B,
        Level::A,
        Level::APlus,
        Level::S,
        Level::X,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Level::D => "D",
            Level::C => "C",
            Level::B => "B",
            Level::A => "A",
            Level::APlus => "A+",
            Level::S => "S",
            Level::X => "X",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|level| level.name().eq_ignore_ascii_case(name.trim()))
    }

    pub fn step(self, delta: i64) -> Self {
        let index = Self::ALL.iter().position(|&l| l == self).unwrap_or(0) as i64;
        let last = Self::ALL.len() as i64 - 1;
        Self::ALL[(index + delta).clamp(0, last) as usize]
    }

    pub fn profile(self) -> Profile {
        let (pps, depth, beam, noise, technique) = match self {
            Level::D => (0.75, 1, 1, 220, 0.0),
            Level::C => (1.1, 1, 2, 120, 0.25),
            Level::B => (1.25, 2, 3, 60, 0.55),
            Level::A => (1.55, 3, 6, 20, 0.85),
            Level::APlus => (2.0, 4, 12, 0, 1.0),
            Level::S => (2.4, 5, 16, 0, 1.0),
            Level::X => (3.4, 5, 24, 0, 1.0),
        };
        Profile {
            pps,
            depth,
            beam,
            noise,
            technique,
            use_180: matches!(self, Level::S | Level::X),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub pps: f64,
    pub depth: usize,
    pub beam: usize,
    pub noise: i32,
    pub technique: f64,
    pub use_180: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Move {
    Left,
    Right,
    Cw,
    Ccw,
    Flip,
    Down,
}

const MOVES: [Move; 6] = [
    Move::Left,
    Move::Right,
    Move::Cw,
    Move::Ccw,
    Move::Flip,
    Move::Down,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Hold,
    Move { action: Move, from: Piece },
    HardDrop,
}

const X_RANGE: usize = 16;
const Y_RANGE: usize = HEIGHT + 4;
const STATES: usize = X_RANGE * Y_RANGE * 4;
const NONE: u16 = u16::MAX;

fn state_index(piece: &Piece) -> Option<usize> {
    let x = usize::try_from(piece.x + 3).ok()?;
    let y = usize::try_from(piece.y + 3).ok()?;
    (x < X_RANGE && y < Y_RANGE).then(|| (x * Y_RANGE + y) * 4 + piece.rotation as usize)
}

fn apply(board: &Board, piece: &Piece, action: Move) -> Option<(Piece, Option<bool>)> {
    let rotate = |to: Rotation, table: &[(i8, i8)], quarter: bool| {
        let turned = Piece {
            rotation: to,
            ..*piece
        };
        table.iter().enumerate().find_map(|(i, &(dx, dy))| {
            let candidate = turned.shifted(dx, dy);
            (!board.collides(&candidate)).then_some((candidate, Some(quarter && i == 4)))
        })
    };
    match action {
        Move::Left | Move::Right | Move::Down => {
            let (dx, dy) = match action {
                Move::Left => (-1, 0),
                Move::Right => (1, 0),
                _ => (0, -1),
            };
            let candidate = piece.shifted(dx, dy);
            (!board.collides(&candidate)).then_some((candidate, None))
        }
        Move::Cw => {
            let to = piece.rotation.cw();
            rotate(to, &kicks(piece.kind, piece.rotation, to), true)
        }
        Move::Ccw => {
            let to = piece.rotation.ccw();
            rotate(to, &kicks(piece.kind, piece.rotation, to), true)
        }
        Move::Flip => rotate(
            piece.rotation.flip(),
            kicks_180(piece.kind, piece.rotation),
            false,
        ),
    }
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    piece: Piece,
    spin: Spin,
    state: u16,
    inputs: u8,
}

#[derive(Clone, Debug)]
struct Pathfinder {
    stamp: Vec<u32>,
    generation: u32,
    parent: Vec<u16>,
    via: Vec<Move>,
    piece: Vec<Piece>,
    depth: Vec<u8>,
    kick: Vec<Option<bool>>,
    queue: Vec<u16>,
}

impl Pathfinder {
    fn new() -> Self {
        let blank = Piece::spawn(Kind::T);
        Self {
            stamp: vec![0; STATES],
            generation: 0,
            parent: vec![NONE; STATES],
            via: vec![Move::Down; STATES],
            piece: vec![blank; STATES],
            depth: vec![0; STATES],
            kick: vec![None; STATES],
            queue: Vec::with_capacity(STATES),
        }
    }

    fn search(&mut self, board: &Board, start: Piece, use_180: bool, out: &mut Vec<Candidate>) {
        out.clear();
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.stamp.fill(0);
            self.generation = 1;
        }
        self.queue.clear();
        let Some(origin) = state_index(&start) else {
            return;
        };
        if board.collides(&start) {
            return;
        }
        self.visit(origin, NONE, Move::Down, start, 0, None);
        let mut head = 0;
        while head < self.queue.len() {
            let index = self.queue[head] as usize;
            head += 1;
            let piece = self.piece[index];
            let depth = self.depth[index];
            if board.collides(&piece.shifted(0, -1)) {
                self.record(board, index, out);
            }
            for action in MOVES {
                if action == Move::Flip && !use_180 {
                    continue;
                }
                let Some((next, kick)) = apply(board, &piece, action) else {
                    continue;
                };
                let Some(next_index) = state_index(&next) else {
                    continue;
                };
                if self.stamp[next_index] != self.generation {
                    self.visit(
                        next_index,
                        index as u16,
                        action,
                        next,
                        depth.saturating_add(1),
                        kick,
                    );
                }
            }
        }
    }

    fn visit(
        &mut self,
        index: usize,
        parent: u16,
        via: Move,
        piece: Piece,
        depth: u8,
        kick: Option<bool>,
    ) {
        self.stamp[index] = self.generation;
        self.parent[index] = parent;
        self.via[index] = via;
        self.piece[index] = piece;
        self.depth[index] = depth;
        self.kick[index] = kick;
        self.queue.push(index as u16);
    }

    fn record(&self, board: &Board, index: usize, out: &mut Vec<Candidate>) {
        let piece = self.piece[index];
        let spin = t_spin(board, &piece, self.kick[index]);
        let footprint = footprint(&piece);
        if piece.kind != Kind::T
            && out
                .iter()
                .any(|c| c.spin == spin && footprint == self::footprint(&c.piece))
        {
            return;
        }
        out.push(Candidate {
            piece,
            spin,
            state: index as u16,
            inputs: self.depth[index],
        });
    }

    fn path(&self, state: u16) -> Vec<Step> {
        let mut steps = Vec::new();
        let mut index = state;
        while self.parent[index as usize] != NONE {
            let parent = self.parent[index as usize];
            steps.push(Step::Move {
                action: self.via[index as usize],
                from: self.piece[parent as usize],
            });
            index = parent;
        }
        steps.reverse();
        while matches!(
            steps.last(),
            Some(Step::Move {
                action: Move::Down,
                ..
            })
        ) {
            steps.pop();
        }
        steps.push(Step::HardDrop);
        steps
    }
}

fn footprint(piece: &Piece) -> [(i8, i8); 4] {
    let mut cells = [(0, 0); 4];
    for (slot, cell) in cells.iter_mut().zip(piece.cells()) {
        *slot = cell;
    }
    cells.sort_unstable();
    cells
}

#[derive(Clone, Copy, Debug)]
struct Weights {
    back_to_back: i32,
    bumpiness: i32,
    bumpiness_sq: i32,
    row_transitions: i32,
    height: i32,
    top_half: i32,
    top_quarter: i32,
    jeopardy: i32,
    cavity_cells: i32,
    cavity_cells_sq: i32,
    covered_cells: i32,
    covered_cells_sq: i32,
    tslot: [i32; 4],
    well_depth: i32,
    max_well_depth: i32,
    move_time: i32,
    wasted_t: i32,
    b2b_clear: i32,
    clear: [i32; 5],
    tspin: [i32; 4],
    mini_tspin: [i32; 3],
    perfect_clear: i32,
    combo_garbage: i32,
}

impl Weights {
    fn for_profile(profile: &Profile) -> Self {
        let t = |value: i32| (f64::from(value) * profile.technique) as i32;
        Self {
            back_to_back: t(150),
            bumpiness: -24,
            bumpiness_sq: -7,
            row_transitions: -5,
            height: -39,
            top_half: -150,
            top_quarter: -511,
            jeopardy: -11,
            cavity_cells: -173,
            cavity_cells_sq: -3,
            covered_cells: -17,
            covered_cells_sq: -1,
            tslot: [0, t(150), t(400), t(600)],
            well_depth: t(57),
            max_well_depth: 17,
            move_time: -3,
            wasted_t: t(-152),
            b2b_clear: t(200),
            clear: [0, -300, -220, -120, 390],
            tspin: [0, t(121), t(650), t(602)],
            mini_tspin: [0, t(-158), t(-93)],
            perfect_clear: 999,
            combo_garbage: 150,
        }
    }
}

fn heights(board: &Board) -> [i32; WIDTH] {
    let mut heights = [0; WIDTH];
    for (x, height) in heights.iter_mut().enumerate() {
        for y in (0..HEIGHT).rev() {
            if board.row(y)[x].is_some() {
                *height = y as i32 + 1;
                break;
            }
        }
    }
    heights
}

fn evaluate(board: &Board, w: &Weights, back_to_back: bool, pending: u32) -> i32 {
    let heights = heights(board);
    let max_height = heights.iter().copied().max().unwrap_or(0);
    let mut score = 0;
    if back_to_back {
        score += w.back_to_back;
    }
    score += w.height * max_height;
    score += w.top_half * (max_height - 10).max(0);
    score += w.top_quarter * (max_height - 15).max(0);
    score += w.jeopardy * pending as i32 * max_height / 4;

    let well = (0..WIDTH).min_by_key(|&x| heights[x]).unwrap_or(0);
    let mut bumpiness = 0;
    let mut bumpiness_sq = 0;
    let mut previous: Option<i32> = None;
    for (x, &height) in heights.iter().enumerate() {
        if x == well {
            continue;
        }
        if let Some(p) = previous {
            let diff = (height - p).abs();
            bumpiness += diff;
            bumpiness_sq += diff * diff;
        }
        previous = Some(height);
    }
    score += w.bumpiness * bumpiness + w.bumpiness_sq * bumpiness_sq;

    let mut cavities = 0;
    let mut covered = 0;
    let mut covered_sq = 0;
    for (x, &height) in heights.iter().enumerate() {
        let mut above = 0;
        for y in (0..height as usize).rev() {
            if board.row(y)[x].is_some() {
                above += 1;
            } else {
                cavities += 1;
                let cover = above.min(6);
                covered += cover;
                covered_sq += cover * cover;
            }
        }
    }
    score += w.cavity_cells * cavities + w.cavity_cells_sq * cavities * cavities;
    score += w.covered_cells * covered + w.covered_cells_sq * covered_sq;

    let mut transitions = 0;
    for y in 0..max_height as usize {
        let row = board.row(y);
        let mut last = true;
        for cell in row {
            if cell.is_some() != last {
                transitions += 1;
            }
            last = cell.is_some();
        }
        if !last {
            transitions += 1;
        }
    }
    score += w.row_transitions * transitions;

    let mut well_depth = 0;
    for y in heights[well] as usize..HEIGHT {
        let row = board.row(y);
        let ready = row
            .iter()
            .enumerate()
            .all(|(x, cell)| x == well || cell.is_some());
        if !ready {
            break;
        }
        well_depth += 1;
    }
    score += w.well_depth * well_depth.min(w.max_well_depth.max(1));

    score += w.tslot[best_tslot(board, &heights)];
    score
}

fn best_tslot(board: &Board, heights: &[i32; WIDTH]) -> usize {
    let mut best = 0;
    for bx in 0..WIDTH as i8 - 2 {
        let by = heights[bx as usize + 1] as i8;
        let filled = |x: i8, y: i8| board.is_blocked(x, y);
        let empty = |x: i8, y: i8| !board.is_blocked(x, y);
        let fits = empty(bx + 1, by)
            && empty(bx, by + 1)
            && empty(bx + 1, by + 1)
            && empty(bx + 2, by + 1);
        if !fits || !filled(bx, by) || !filled(bx + 2, by) {
            continue;
        }
        if !(filled(bx, by + 2) || filled(bx + 2, by + 2)) {
            continue;
        }
        let mut lines = 0;
        for (y, holes) in [(by, &[bx + 1][..]), (by + 1, &[bx, bx + 1, bx + 2][..])] {
            if y < 0 || y as usize >= HEIGHT {
                continue;
            }
            let row = board.row(y as usize);
            let full = (0..WIDTH as i8).all(|x| holes.contains(&x) || row[x as usize].is_some());
            if full {
                lines += 1;
            }
        }
        best = best.max(lines);
    }
    best
}

#[derive(Clone, Copy, Debug)]
struct Outcome {
    reward: i32,
    back_to_back: bool,
    combo: Option<u32>,
}

fn place(
    board: &mut Board,
    candidate: &Candidate,
    back_to_back: bool,
    combo: Option<u32>,
    w: &Weights,
) -> Outcome {
    board.lock(&candidate.piece);
    let rows = board.full_rows();
    let lines = rows.count_ones();
    board.clear_rows(rows);
    let clear = Clear {
        lines,
        spin: candidate.spin,
        perfect: lines > 0 && board.is_empty(),
    };
    let mut reward = w.move_time * i32::from(candidate.inputs.min(30));
    if lines == 0 {
        if candidate.piece.kind == Kind::T && candidate.spin == Spin::None {
            reward += w.wasted_t;
        }
        return Outcome {
            reward,
            back_to_back,
            combo: None,
        };
    }
    let index = lines.min(4) as usize;
    reward += match clear.spin {
        Spin::Full => w.tspin[index.min(3)],
        Spin::Mini => w.mini_tspin[index.min(2)],
        Spin::None => w.clear[index],
    };
    if candidate.piece.kind == Kind::T && clear.spin == Spin::None {
        reward += w.wasted_t;
    }
    let difficult = clear.is_difficult();
    if difficult && back_to_back {
        reward += w.b2b_clear;
    }
    if clear.perfect {
        reward += w.perfect_clear;
    }
    let next_combo = combo.map_or(0, |c| c + 1);
    let combo_attack = attack(clear, false, next_combo) - attack(clear, false, 0);
    reward += w.combo_garbage * combo_attack as i32;
    Outcome {
        reward,
        back_to_back: difficult || (back_to_back && lines == 0),
        combo: Some(next_combo),
    }
}

#[derive(Clone, Debug)]
struct Node {
    board: Board,
    hold: Option<Kind>,
    next: usize,
    back_to_back: bool,
    combo: Option<u32>,
    reward: i32,
    root: usize,
    score: i32,
}

#[derive(Clone, Copy, Debug)]
struct RootChoice {
    hold: bool,
    candidate: Candidate,
}

pub struct Bot {
    level: Level,
    profile: Profile,
    weights: Weights,
    rng: Rng,
    pathfinder: Pathfinder,
    candidates: Vec<Candidate>,
    roots: Vec<RootChoice>,
    beam: Vec<Node>,
    children: Vec<Node>,
    plan: Vec<Step>,
    cursor: usize,
    planned_spawn: Option<u32>,
    wait: Duration,
    clock: Duration,
    due: Duration,
    replans: u8,
}

const QUEUE: usize = 6;

impl Bot {
    pub fn new(level: Level, rng: Rng) -> Self {
        let profile = level.profile();
        Self {
            level,
            weights: Weights::for_profile(&profile),
            profile,
            rng,
            pathfinder: Pathfinder::new(),
            candidates: Vec::with_capacity(128),
            roots: Vec::with_capacity(256),
            beam: Vec::with_capacity(64),
            children: Vec::with_capacity(1024),
            plan: Vec::with_capacity(32),
            cursor: 0,
            planned_spawn: None,
            wait: Duration::ZERO,
            clock: Duration::ZERO,
            due: Duration::ZERO,
            replans: 0,
        }
    }

    pub fn level(&self) -> Level {
        self.level
    }

    pub fn tick(&mut self, game: &mut Game, dt: Duration) {
        self.clock += dt;
        let Some(active) = game.active().copied() else {
            return;
        };
        if self.planned_spawn != Some(game.spawn_id()) {
            self.planned_spawn = Some(game.spawn_id());
            self.plan.clear();
            self.cursor = 0;
            self.replans = 0;
            let budget = Duration::from_secs_f64(1.0 / self.profile.pps);
            self.due = self.due.max(self.clock.saturating_sub(budget)) + budget;
            self.wait = self.due.saturating_sub(self.clock).saturating_sub(TICK * 4);
        }
        if !self.wait.is_zero() {
            self.wait = self.wait.saturating_sub(dt);
            return;
        }
        if self.plan.is_empty() {
            self.think(game);
        }
        let Some(&step) = self.plan.get(self.cursor) else {
            game.hard_drop();
            return;
        };
        self.cursor += 1;
        match step {
            Step::Hold => {
                game.hold();
                self.planned_spawn = Some(game.spawn_id());
            }
            Step::Move { action, from } => {
                if active != from || !perform(game, action) {
                    self.replan(game);
                }
            }
            Step::HardDrop => game.hard_drop(),
        }
    }

    fn replan(&mut self, game: &mut Game) {
        self.replans += 1;
        if self.replans > 3 {
            game.hard_drop();
            return;
        }
        self.think(game);
    }

    #[cfg(test)]
    pub fn play_piece(&mut self, game: &mut Game) {
        if game.active().is_none() {
            return;
        }
        self.think(game);
        for step in std::mem::take(&mut self.plan) {
            match step {
                Step::Hold => {
                    game.hold();
                }
                Step::Move { action, .. } => {
                    perform(game, action);
                }
                Step::HardDrop => game.hard_drop(),
            }
        }
    }

    fn noise(&mut self) -> i32 {
        if self.profile.noise == 0 {
            return 0;
        }
        let span = self.profile.noise as u32 * 2 + 1;
        self.rng.below(span) as i32 - self.profile.noise
    }

    fn expand(
        &mut self,
        node: &Node,
        start: Piece,
        hold: Option<Kind>,
        next: usize,
        root: Option<bool>,
        pending: u32,
    ) {
        self.pathfinder.search(
            &node.board,
            start,
            self.profile.use_180,
            &mut self.candidates,
        );
        for i in 0..self.candidates.len() {
            let candidate = self.candidates[i];
            let mut board = node.board.clone();
            let outcome = place(
                &mut board,
                &candidate,
                node.back_to_back,
                node.combo,
                &self.weights,
            );
            let reward = node.reward + outcome.reward;
            let score = reward
                + evaluate(&board, &self.weights, outcome.back_to_back, pending)
                + self.noise();
            let root_index = match root {
                Some(hold) => {
                    self.roots.push(RootChoice { hold, candidate });
                    self.roots.len() - 1
                }
                None => node.root,
            };
            self.children.push(Node {
                board,
                hold,
                next,
                back_to_back: outcome.back_to_back,
                combo: outcome.combo,
                reward,
                root: root_index,
                score,
            });
        }
    }

    fn think(&mut self, game: &Game) {
        self.plan.clear();
        self.cursor = 0;
        let Some(active) = game.active().copied() else {
            return;
        };
        let mut queue = [Kind::T; QUEUE];
        let mut queue_len = 0;
        for (slot, kind) in queue.iter_mut().zip(game.preview()) {
            *slot = kind;
            queue_len += 1;
        }
        let pending = game.pending_garbage().1;
        let root = Node {
            board: game.board().clone(),
            hold: game.hold_piece(),
            next: 0,
            back_to_back: game.scoring().back_to_back(),
            combo: game.scoring().combo(),
            reward: 0,
            root: 0,
            score: 0,
        };
        self.roots.clear();
        self.children.clear();
        self.expand(&root, active, root.hold, 0, Some(false), pending);
        if game.can_hold() {
            let held = root
                .hold
                .or_else(|| queue.first().copied().filter(|_| queue_len > 0));
            if let Some(kind) = held {
                let next = if root.hold.is_some() { 0 } else { 1 };
                self.expand(
                    &root,
                    Piece::spawn(kind),
                    Some(active.kind),
                    next,
                    Some(true),
                    pending,
                );
            }
        }
        self.select_beam();
        for _ in 1..self.profile.depth {
            self.children.clear();
            let beam = std::mem::take(&mut self.beam);
            for node in &beam {
                if node.next < queue_len {
                    let current = queue[node.next];
                    self.expand(
                        node,
                        Piece::spawn(current),
                        node.hold,
                        node.next + 1,
                        None,
                        pending,
                    );
                    match node.hold {
                        Some(held) if held != current => {
                            self.expand(
                                node,
                                Piece::spawn(held),
                                Some(current),
                                node.next + 1,
                                None,
                                pending,
                            );
                        }
                        None if node.next + 1 < queue_len => {
                            let swapped = queue[node.next + 1];
                            self.expand(
                                node,
                                Piece::spawn(swapped),
                                Some(current),
                                node.next + 2,
                                None,
                                pending,
                            );
                        }
                        _ => {}
                    }
                }
            }
            self.beam = beam;
            if self.children.is_empty() {
                break;
            }
            self.select_beam();
        }
        let Some(best) = self.beam.iter().max_by_key(|node| node.score) else {
            return;
        };
        let choice = self.roots[best.root];
        let start = if choice.hold {
            self.plan.push(Step::Hold);
            let kind = game.hold_piece().or_else(|| game.preview().next());
            match kind {
                Some(kind) => Piece::spawn(kind),
                None => return,
            }
        } else {
            active
        };
        self.pathfinder.search(
            game.board(),
            start,
            self.profile.use_180,
            &mut self.candidates,
        );
        let target = self
            .candidates
            .iter()
            .find(|c| c.piece == choice.candidate.piece)
            .map(|c| c.state);
        match target {
            Some(state) => self.plan.extend(self.pathfinder.path(state)),
            None => self.plan.push(Step::HardDrop),
        }
    }

    fn select_beam(&mut self) {
        self.children
            .sort_unstable_by_key(|node| std::cmp::Reverse(node.score));
        self.children.truncate(self.profile.beam.max(1));
        self.beam.clear();
        self.beam.append(&mut self.children);
    }
}

fn perform(game: &mut Game, action: Move) -> bool {
    match action {
        Move::Left => game.shift(-1),
        Move::Right => game.shift(1),
        Move::Cw => game.rotate(true),
        Move::Ccw => game.rotate(false),
        Move::Flip => game.rotate_180(),
        Move::Down => game.soft_drop_step(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Settings;

    fn quiet_settings() -> Settings {
        Settings {
            line_clear_delay: Duration::ZERO,
            ..Settings::default()
        }
    }

    #[test]
    fn finds_every_t_placement_on_empty_board() {
        let mut pathfinder = Pathfinder::new();
        let mut out = Vec::new();
        pathfinder.search(&Board::default(), Piece::spawn(Kind::T), false, &mut out);
        assert_eq!(out.len(), 34);
        assert!(out.iter().all(|c| c.spin == Spin::None));
    }

    #[test]
    fn symmetric_pieces_are_deduplicated() {
        let mut pathfinder = Pathfinder::new();
        let mut out = Vec::new();
        pathfinder.search(&Board::default(), Piece::spawn(Kind::O), false, &mut out);
        assert_eq!(out.len(), 9);
        pathfinder.search(&Board::default(), Piece::spawn(Kind::I), false, &mut out);
        assert_eq!(out.len(), 17);
    }

    #[test]
    fn paths_reach_their_targets() {
        let mut pathfinder = Pathfinder::new();
        let mut out = Vec::new();
        let board = Board::default();
        pathfinder.search(&board, Piece::spawn(Kind::L), true, &mut out);
        for candidate in out.clone() {
            let mut piece = Piece::spawn(Kind::L);
            for step in pathfinder.path(candidate.state) {
                match step {
                    Step::Move { action, from } => {
                        assert_eq!(piece, from);
                        piece = apply(&board, &piece, action).unwrap().0;
                    }
                    Step::HardDrop => piece = piece.shifted(0, -board.drop_distance(&piece)),
                    Step::Hold => unreachable!(),
                }
            }
            assert_eq!(piece, candidate.piece);
        }
    }

    #[test]
    fn finds_t_spin_double() {
        let mut board = Board::default();
        for x in 0..WIDTH as i8 {
            if x != 4 {
                board.set(x, 0, Some(Kind::Z));
            }
            if !(3..=5).contains(&x) {
                board.set(x, 1, Some(Kind::Z));
            }
        }
        board.set(2, 2, Some(Kind::Z));
        board.set(3, 2, Some(Kind::Z));
        board.set(0, 2, Some(Kind::Z));
        let mut pathfinder = Pathfinder::new();
        let mut out = Vec::new();
        pathfinder.search(&board, Piece::spawn(Kind::T), false, &mut out);
        let tsd = out
            .iter()
            .find(|c| c.spin == Spin::Full && c.piece.rotation == Rotation::Reverse)
            .expect("t-spin slot reachable");
        assert_eq!((tsd.piece.x, tsd.piece.y), (3, 0));
    }

    #[test]
    fn evaluation_prefers_clean_boards() {
        let w = Weights::for_profile(&Level::APlus.profile());
        let mut flat = Board::default();
        let mut holey = Board::default();
        for x in 0..9 {
            flat.set(x, 0, Some(Kind::J));
            holey.set(x, 1, Some(Kind::J));
        }
        assert!(evaluate(&flat, &w, false, 0) > evaluate(&holey, &w, false, 0));
    }

    fn run(level: Level, pieces: u32, seed: u64) -> Game {
        let mut game = Game::new(quiet_settings(), Rng::new(seed));
        let mut bot = Bot::new(level, Rng::new(seed ^ 0xABCD));
        while !game.is_over() && game.pieces_placed() < pieces {
            bot.play_piece(&mut game);
        }
        game
    }

    #[test]
    fn strong_bot_survives_and_attacks() {
        let game = run(Level::A, 120, 11);
        assert!(!game.is_over());
        assert!(game.scoring().lines >= 40, "lines {}", game.scoring().lines);
        assert!(game.attack_total() >= 25, "attack {}", game.attack_total());
    }

    #[test]
    fn weakest_bot_still_plays() {
        let game = run(Level::D, 60, 5);
        assert!(game.pieces_placed() > 20);
    }

    #[test]
    fn levels_parse_and_step() {
        assert_eq!(Level::parse("a+"), Some(Level::APlus));
        assert_eq!(Level::parse("zz"), None);
        assert_eq!(Level::D.step(-1), Level::D);
        assert_eq!(Level::S.step(5), Level::X);
        assert_eq!(Level::A.step(1), Level::APlus);
    }

    #[test]
    #[ignore = "benchmark: cargo test --release -- --ignored --nocapture"]
    fn benchmark_levels() {
        for level in Level::ALL {
            let mut attack = 0;
            let mut pieces = 0;
            let mut deaths = 0;
            let started = std::time::Instant::now();
            for seed in 0..4 {
                let game = run(level, 400, seed);
                attack += game.attack_total();
                pieces += game.pieces_placed();
                deaths += u32::from(game.is_over());
            }
            let app = f64::from(attack) / f64::from(pieces);
            let pps = level.profile().pps;
            println!(
                "{:>3}  APP {:.2}  PPS {:.2}  ≈APM {:>5.1}  deaths {}  think {:.2} ms/piece",
                level.name(),
                app,
                pps,
                app * pps * 60.0,
                deaths,
                started.elapsed().as_secs_f64() * 1000.0 / f64::from(pieces)
            );
        }
    }
}

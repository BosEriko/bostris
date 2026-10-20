use crate::board::Board;
use crate::piece::{Kind, Piece, Rotation, kicks, kicks_180};

const X_OFFSET: i8 = 3;
const X_STATES: usize = 16;
const STATES: usize = X_STATES * 4;
const PROBE_Y: i8 = 20;

type Footprint = [(i8, i8); 4];

fn footprint(piece: &Piece) -> Footprint {
    let mut cells: Footprint = [(0, 0); 4];
    for (slot, cell) in cells.iter_mut().zip(piece.cells()) {
        *slot = cell;
    }
    let min_y = cells.iter().map(|&(_, y)| y).min().unwrap_or(0);
    for cell in &mut cells {
        cell.1 -= min_y;
    }
    cells.sort_unstable();
    cells
}

fn index(piece: &Piece) -> Option<usize> {
    let x = usize::try_from(piece.x + X_OFFSET).ok()?;
    (x < X_STATES).then(|| x * 4 + piece.rotation as usize)
}

fn rotated(board: &Board, piece: &Piece, to: Rotation, kicks: &[(i8, i8)]) -> Option<Piece> {
    let turned = Piece {
        rotation: to,
        ..*piece
    };
    kicks
        .iter()
        .map(|&(dx, dy)| turned.shifted(dx, dy))
        .find(|candidate| !board.collides(candidate))
}

fn shifted(board: &Board, piece: &Piece, dx: i8, repeat: bool) -> Option<Piece> {
    let mut current = *piece;
    let mut moved = false;
    loop {
        let next = current.shifted(dx, 0);
        if board.collides(&next) {
            break;
        }
        current = next;
        moved = true;
        if !repeat {
            break;
        }
    }
    moved.then_some(current)
}

pub fn optimal_inputs(kind: Kind, rotation: Rotation, x: i8, allow_180: bool) -> Option<u32> {
    let board = Board::default();
    let target = footprint(&Piece {
        kind,
        rotation,
        x,
        y: PROBE_Y,
    });
    let start = Piece {
        y: PROBE_Y,
        ..Piece::spawn(kind)
    };
    let mut distance = [u8::MAX; STATES];
    let mut queue = [start; STATES];
    let (mut head, mut tail) = (0, 1);
    distance[index(&start)?] = 0;
    while head < tail {
        let piece = queue[head];
        head += 1;
        let cost = distance[index(&piece)?];
        if footprint(&piece) == target {
            return Some(u32::from(cost));
        }
        let moves = [
            shifted(&board, &piece, -1, false),
            shifted(&board, &piece, 1, false),
            shifted(&board, &piece, -1, true),
            shifted(&board, &piece, 1, true),
            rotated(
                &board,
                &piece,
                piece.rotation.cw(),
                &kicks(kind, piece.rotation, piece.rotation.cw()),
            ),
            rotated(
                &board,
                &piece,
                piece.rotation.ccw(),
                &kicks(kind, piece.rotation, piece.rotation.ccw()),
            ),
            allow_180
                .then(|| {
                    rotated(
                        &board,
                        &piece,
                        piece.rotation.flip(),
                        kicks_180(kind, piece.rotation),
                    )
                })
                .flatten(),
        ];
        for next in moves.into_iter().flatten() {
            let Some(i) = index(&next) else {
                continue;
            };
            if distance[i] == u8::MAX && tail < STATES {
                distance[i] = cost + 1;
                queue[tail] = Piece { y: PROBE_Y, ..next };
                tail += 1;
            }
        }
    }
    None
}

pub fn is_droppable(board: &Board, piece: &Piece) -> bool {
    let mut probe = Piece {
        y: Piece::spawn(piece.kind).y,
        ..*piece
    };
    if probe.y < piece.y {
        return true;
    }
    while probe.y > piece.y {
        if board.collides(&probe) {
            return false;
        }
        probe = probe.shifted(0, -1);
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault {
    pub used: u32,
    pub optimal: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub evaluated: u32,
    pub faults: u32,
}

impl Stats {
    pub fn accuracy(&self) -> Option<f32> {
        (self.evaluated > 0)
            .then(|| (self.evaluated - self.faults) as f32 / self.evaluated as f32 * 100.0)
    }
}

pub fn evaluate(
    board: &Board,
    piece: &Piece,
    used: u32,
    allow_180: bool,
) -> Option<Result<(), Fault>> {
    if !is_droppable(board, piece) {
        return None;
    }
    let optimal = optimal_inputs(piece.kind, piece.rotation, piece.x, allow_180)?;
    Some(if used > optimal {
        Err(Fault { used, optimal })
    } else {
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(kind: Kind, rotation: Rotation, x: i8) -> u32 {
        optimal_inputs(kind, rotation, x, false).unwrap()
    }

    #[test]
    fn spin_180_shortens_reverse_placements() {
        assert_eq!(optimal_inputs(Kind::T, Rotation::Reverse, 3, true), Some(1));
        assert_eq!(optimal_inputs(Kind::T, Rotation::Reverse, 0, true), Some(2));
        assert_eq!(optimal_inputs(Kind::T, Rotation::Right, 3, true), Some(1));
    }

    #[test]
    fn spawn_position_needs_no_input() {
        for kind in Kind::ALL {
            let spawn = Piece::spawn(kind);
            assert_eq!(at(kind, Rotation::Spawn, spawn.x), 0);
        }
    }

    #[test]
    fn walls_are_one_das_away() {
        assert_eq!(at(Kind::T, Rotation::Spawn, 0), 1);
        assert_eq!(at(Kind::T, Rotation::Spawn, 7), 1);
        assert_eq!(at(Kind::I, Rotation::Spawn, 0), 1);
        assert_eq!(at(Kind::O, Rotation::Spawn, -1), 1);
    }

    #[test]
    fn known_finesse_values() {
        assert_eq!(at(Kind::T, Rotation::Spawn, 2), 1);
        assert_eq!(at(Kind::T, Rotation::Spawn, 1), 2);
        assert_eq!(at(Kind::T, Rotation::Right, 3), 1);
        assert_eq!(at(Kind::T, Rotation::Reverse, 3), 2);
        assert_eq!(at(Kind::J, Rotation::Left, 3), 1);
        assert_eq!(at(Kind::I, Rotation::Right, -2), 2);
        assert_eq!(at(Kind::I, Rotation::Left, 1), 2);
    }

    #[test]
    fn symmetric_rotations_share_footprints() {
        let right = at(Kind::I, Rotation::Right, 3);
        let left = at(Kind::I, Rotation::Left, 4);
        assert_eq!(right, left);
        assert_eq!(at(Kind::O, Rotation::Reverse, 3), 0);
        assert_eq!(at(Kind::S, Rotation::Reverse, 3), 0);
    }

    #[test]
    fn evaluation_flags_extra_inputs() {
        let board = Board::default();
        let piece = Piece {
            kind: Kind::T,
            rotation: Rotation::Spawn,
            x: 0,
            y: 0,
        };
        assert_eq!(evaluate(&board, &piece, 1, false), Some(Ok(())));
        assert_eq!(
            evaluate(&board, &piece, 3, false),
            Some(Err(Fault {
                used: 3,
                optimal: 1
            }))
        );
    }

    #[test]
    fn tucks_are_not_evaluated() {
        let mut board = Board::default();
        board.set(1, 4, Some(Kind::Z));
        let piece = Piece {
            kind: Kind::T,
            rotation: Rotation::Spawn,
            x: 0,
            y: 0,
        };
        assert!(!is_droppable(&board, &piece));
        assert_eq!(evaluate(&board, &piece, 9, false), None);
    }

    #[test]
    fn accuracy_is_percentage_of_clean_pieces() {
        let stats = Stats {
            evaluated: 4,
            faults: 1,
        };
        assert_eq!(stats.accuracy(), Some(75.0));
        assert_eq!(Stats::default().accuracy(), None);
    }
}

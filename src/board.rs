use crate::piece::{Kind, Piece};

pub const WIDTH: usize = 10;
pub const HEIGHT: usize = 40;
pub const VISIBLE_HEIGHT: usize = 20;

pub type Row = [Option<Kind>; WIDTH];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    rows: [Row; HEIGHT],
}

impl Default for Board {
    fn default() -> Self {
        Self {
            rows: [[None; WIDTH]; HEIGHT],
        }
    }
}

impl Board {
    #[cfg(test)]
    pub fn get(&self, x: i8, y: i8) -> Option<Kind> {
        if Self::in_bounds(x, y) {
            self.rows[y as usize][x as usize]
        } else {
            None
        }
    }

    pub fn set(&mut self, x: i8, y: i8, kind: Option<Kind>) {
        if Self::in_bounds(x, y) {
            self.rows[y as usize][x as usize] = kind;
        }
    }

    pub fn row(&self, y: usize) -> &Row {
        &self.rows[y]
    }

    fn in_bounds(x: i8, y: i8) -> bool {
        x >= 0 && (x as usize) < WIDTH && y >= 0 && (y as usize) < HEIGHT
    }

    pub fn is_blocked(&self, x: i8, y: i8) -> bool {
        !Self::in_bounds(x, y) || self.rows[y as usize][x as usize].is_some()
    }

    pub fn collides(&self, piece: &Piece) -> bool {
        piece.cells().any(|(x, y)| self.is_blocked(x, y))
    }

    pub fn drop_distance(&self, piece: &Piece) -> i8 {
        let mut distance = 0;
        while !self.collides(&piece.shifted(0, -(distance + 1))) {
            distance += 1;
        }
        distance
    }

    pub fn lock(&mut self, piece: &Piece) {
        for (x, y) in piece.cells() {
            self.set(x, y, Some(piece.kind));
        }
    }

    pub fn full_rows(&self) -> u64 {
        let mut mask = 0;
        for (y, row) in self.rows.iter().enumerate() {
            if row.iter().all(Option::is_some) {
                mask |= 1 << y;
            }
        }
        mask
    }

    pub fn is_empty(&self) -> bool {
        self.rows.iter().all(|row| row.iter().all(Option::is_none))
    }

    pub fn push_garbage(&mut self, lines: usize, hole: usize) -> bool {
        let lines = lines.min(HEIGHT);
        let overflow = self.rows[HEIGHT - lines..]
            .iter()
            .any(|row| row.iter().any(Option::is_some));
        self.rows.copy_within(0..HEIGHT - lines, lines);
        let mut garbage = [Some(Kind::Garbage); WIDTH];
        garbage[hole.min(WIDTH - 1)] = None;
        for row in &mut self.rows[..lines] {
            *row = garbage;
        }
        overflow
    }

    pub fn clear_rows(&mut self, mask: u64) -> u32 {
        let mut write = 0;
        for read in 0..HEIGHT {
            if mask & (1 << read) == 0 {
                self.rows[write] = self.rows[read];
                write += 1;
            }
        }
        let cleared = HEIGHT - write;
        for row in &mut self.rows[write..] {
            *row = [None; WIDTH];
        }
        cleared as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::Rotation;

    fn fill_row(board: &mut Board, y: i8, except: Option<i8>) {
        for x in 0..WIDTH as i8 {
            if Some(x) != except {
                board.set(x, y, Some(Kind::J));
            }
        }
    }

    #[test]
    fn walls_and_floor_collide() {
        let board = Board::default();
        let piece = Piece::spawn(Kind::T);
        assert!(!board.collides(&piece));
        assert!(board.collides(&piece.shifted(-4, 0)));
        assert!(board.collides(&piece.shifted(8, 0)));
        assert!(board.collides(&piece.shifted(0, -19)));
        assert!(!board.collides(&piece.shifted(0, -18)));
    }

    #[test]
    fn filled_cells_collide() {
        let mut board = Board::default();
        board.set(4, 10, Some(Kind::O));
        let piece = Piece {
            kind: Kind::O,
            rotation: Rotation::Spawn,
            x: 3,
            y: 9,
        };
        assert!(board.collides(&piece));
        assert!(!board.collides(&piece.shifted(0, 1)));
    }

    #[test]
    fn drop_distance_reaches_floor_or_stack() {
        let mut board = Board::default();
        let piece = Piece::spawn(Kind::I);
        assert_eq!(board.drop_distance(&piece), 18);
        board.set(5, 3, Some(Kind::S));
        assert_eq!(board.drop_distance(&piece), 14);
    }

    #[test]
    fn garbage_rises_from_the_bottom() {
        let mut board = Board::default();
        board.set(3, 0, Some(Kind::T));
        assert!(!board.push_garbage(2, 7));
        assert_eq!(board.get(3, 2), Some(Kind::T));
        assert_eq!(board.get(7, 0), None);
        assert_eq!(board.get(7, 1), None);
        assert_eq!(board.get(0, 1), Some(Kind::Garbage));
        assert_eq!(board.full_rows(), 0);
        assert!(!board.is_empty());
        assert!(Board::default().is_empty());
    }

    #[test]
    fn garbage_overflow_is_reported() {
        let mut board = Board::default();
        board.set(0, HEIGHT as i8 - 1, Some(Kind::I));
        assert!(board.push_garbage(1, 0));
    }

    #[test]
    fn clearing_rows_shifts_stack_down() {
        let mut board = Board::default();
        fill_row(&mut board, 0, None);
        fill_row(&mut board, 1, Some(3));
        fill_row(&mut board, 2, None);
        board.set(7, 3, Some(Kind::T));
        let mask = board.full_rows();
        assert_eq!(mask, 0b101);
        assert_eq!(board.clear_rows(mask), 2);
        assert_eq!(board.get(3, 0), None);
        assert_eq!(board.get(4, 0), Some(Kind::J));
        assert_eq!(board.get(7, 1), Some(Kind::T));
        assert_eq!(board.get(7, 2), None);
        assert_eq!(board.full_rows(), 0);
    }
}

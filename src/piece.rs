#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::I,
        Kind::O,
        Kind::T,
        Kind::S,
        Kind::Z,
        Kind::J,
        Kind::L,
    ];

    fn box_size(self) -> i8 {
        match self {
            Kind::I | Kind::O => 4,
            _ => 3,
        }
    }

    fn spawn_cells(self) -> [(i8, i8); 4] {
        match self {
            Kind::I => [(0, 2), (1, 2), (2, 2), (3, 2)],
            Kind::O => [(1, 1), (2, 1), (1, 2), (2, 2)],
            Kind::T => [(1, 2), (0, 1), (1, 1), (2, 1)],
            Kind::S => [(1, 2), (2, 2), (0, 1), (1, 1)],
            Kind::Z => [(0, 2), (1, 2), (1, 1), (2, 1)],
            Kind::J => [(0, 2), (0, 1), (1, 1), (2, 1)],
            Kind::L => [(2, 2), (0, 1), (1, 1), (2, 1)],
        }
    }

    pub fn cells(self, rotation: Rotation) -> [(i8, i8); 4] {
        let n = self.box_size();
        let mut cells = self.spawn_cells();
        for _ in 0..rotation as u8 {
            for cell in &mut cells {
                *cell = (cell.1, n - 1 - cell.0);
            }
        }
        cells
    }

    pub fn spawn_position(self) -> (i8, i8) {
        match self {
            Kind::I => (3, 16),
            _ => (3, 17),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    Spawn = 0,
    Right = 1,
    Reverse = 2,
    Left = 3,
}

impl Rotation {
    pub fn cw(self) -> Self {
        Self::from_index(self as u8 + 1)
    }

    pub fn flip(self) -> Self {
        Self::from_index(self as u8 + 2)
    }

    pub fn ccw(self) -> Self {
        Self::from_index(self as u8 + 3)
    }

    fn from_index(index: u8) -> Self {
        match index % 4 {
            0 => Rotation::Spawn,
            1 => Rotation::Right,
            2 => Rotation::Reverse,
            _ => Rotation::Left,
        }
    }
}

type Kicks = [(i8, i8); 5];

const JLSTZ_OFFSETS: [Kicks; 4] = [
    [(0, 0); 5],
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
    [(0, 0); 5],
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
];

const I_KICKS_CW: [Kicks; 4] = [
    [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
    [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
    [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
    [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
];

const KICKS_180: [[(i8, i8); 6]; 4] = [
    [(0, 0), (0, 1), (1, 1), (-1, 1), (1, 0), (-1, 0)],
    [(0, 0), (1, 0), (1, 2), (1, 1), (0, 2), (0, 1)],
    [(0, 0), (0, -1), (-1, -1), (1, -1), (-1, 0), (1, 0)],
    [(0, 0), (-1, 0), (-1, 2), (-1, 1), (0, 2), (0, 1)],
];

pub fn kicks_180(kind: Kind, from: Rotation) -> &'static [(i8, i8)] {
    match kind {
        Kind::O => &[(0, 0)],
        _ => &KICKS_180[from as usize],
    }
}

pub fn kicks(kind: Kind, from: Rotation, to: Rotation) -> Kicks {
    match kind {
        Kind::O => [(0, 0); 5],
        Kind::I => {
            if to == from.cw() {
                I_KICKS_CW[from as usize]
            } else {
                let mut table = I_KICKS_CW[to as usize];
                for kick in &mut table {
                    *kick = (-kick.0, -kick.1);
                }
                table
            }
        }
        _ => {
            let a = JLSTZ_OFFSETS[from as usize];
            let b = JLSTZ_OFFSETS[to as usize];
            let mut table = [(0, 0); 5];
            for i in 0..5 {
                table[i] = (a[i].0 - b[i].0, a[i].1 - b[i].1);
            }
            table
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: Kind,
    pub rotation: Rotation,
    pub x: i8,
    pub y: i8,
}

impl Piece {
    pub fn spawn(kind: Kind) -> Self {
        let (x, y) = kind.spawn_position();
        Self {
            kind,
            rotation: Rotation::Spawn,
            x,
            y,
        }
    }

    pub fn cells(&self) -> impl Iterator<Item = (i8, i8)> + '_ {
        self.kind
            .cells(self.rotation)
            .into_iter()
            .map(move |(cx, cy)| (self.x + cx, self.y + cy))
    }

    pub fn shifted(&self, dx: i8, dy: i8) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..*self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_rotations_return_to_spawn() {
        for kind in Kind::ALL {
            let mut r = Rotation::Spawn;
            for _ in 0..4 {
                r = r.cw();
            }
            assert_eq!(r, Rotation::Spawn);
            assert_eq!(Rotation::Spawn.cw().ccw(), Rotation::Spawn);
            let mut a = kind.cells(Rotation::Spawn);
            let mut b = kind.cells(Rotation::Reverse.cw().cw());
            a.sort();
            b.sort();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn o_piece_is_rotation_invariant() {
        let mut spawn = Kind::O.cells(Rotation::Spawn);
        spawn.sort();
        for r in [Rotation::Right, Rotation::Reverse, Rotation::Left] {
            let mut cells = Kind::O.cells(r);
            cells.sort();
            assert_eq!(cells, spawn);
        }
    }

    #[test]
    fn t_rotations_match_srs() {
        let mut right = Kind::T.cells(Rotation::Right);
        right.sort();
        assert_eq!(right, [(1, 0), (1, 1), (1, 2), (2, 1)]);
        let mut reverse = Kind::T.cells(Rotation::Reverse);
        reverse.sort();
        assert_eq!(reverse, [(0, 1), (1, 0), (1, 1), (2, 1)]);
    }

    #[test]
    fn i_rotations_match_srs() {
        let mut right = Kind::I.cells(Rotation::Right);
        right.sort();
        assert_eq!(right, [(2, 0), (2, 1), (2, 2), (2, 3)]);
        let mut left = Kind::I.cells(Rotation::Left);
        left.sort();
        assert_eq!(left, [(1, 0), (1, 1), (1, 2), (1, 3)]);
    }

    #[test]
    fn flip_is_two_rotations() {
        for r in [
            Rotation::Spawn,
            Rotation::Right,
            Rotation::Reverse,
            Rotation::Left,
        ] {
            assert_eq!(r.flip(), r.cw().cw());
            assert_eq!(r.flip().flip(), r);
        }
        assert_eq!(kicks_180(Kind::O, Rotation::Spawn), &[(0, 0)]);
        assert_eq!(kicks_180(Kind::T, Rotation::Spawn)[1], (0, 1));
    }

    #[test]
    fn kick_tables_match_srs() {
        assert_eq!(
            kicks(Kind::T, Rotation::Spawn, Rotation::Right),
            [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)]
        );
        assert_eq!(
            kicks(Kind::J, Rotation::Left, Rotation::Spawn),
            [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)]
        );
        assert_eq!(
            kicks(Kind::I, Rotation::Right, Rotation::Spawn),
            [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)]
        );
        assert_eq!(
            kicks(Kind::I, Rotation::Spawn, Rotation::Left),
            [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)]
        );
    }
}

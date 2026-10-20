#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClearEvent {
    pub lines: u32,
    pub points: u64,
    pub back_to_back: bool,
    pub combo: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scoring {
    pub score: u64,
    pub lines: u32,
    start_level: u32,
    combo: Option<u32>,
    back_to_back: bool,
}

pub const LINES_PER_LEVEL: u32 = 10;

impl Scoring {
    pub fn new(start_level: u32) -> Self {
        Self {
            score: 0,
            lines: 0,
            start_level: start_level.max(1),
            combo: None,
            back_to_back: false,
        }
    }

    pub fn level(&self) -> u32 {
        self.start_level + self.lines / LINES_PER_LEVEL
    }

    pub fn soft_drop(&mut self, cells: u32) {
        self.score += u64::from(cells);
    }

    pub fn hard_drop(&mut self, cells: u32) {
        self.score += 2 * u64::from(cells);
    }

    pub fn lock(&mut self, lines: u32) -> Option<ClearEvent> {
        if lines == 0 {
            self.combo = None;
            return None;
        }
        let level = u64::from(self.level());
        let base: u64 = match lines {
            1 => 100,
            2 => 300,
            3 => 500,
            _ => 800,
        };
        let difficult = lines >= 4;
        let back_to_back = difficult && self.back_to_back;
        let mut points = base * level;
        if back_to_back {
            points = points * 3 / 2;
        }
        let combo = self.combo.map_or(0, |c| c + 1);
        points += 50 * u64::from(combo) * level;
        self.combo = Some(combo);
        self.back_to_back = difficult;
        self.score += points;
        self.lines += lines;
        Some(ClearEvent {
            lines,
            points,
            back_to_back,
            combo,
        })
    }
}

pub fn gravity_cells_per_second(level: u32) -> f64 {
    let level = f64::from(level.clamp(1, 30));
    let seconds_per_row = (0.8 - (level - 1.0) * 0.007).powf(level - 1.0);
    1.0 / seconds_per_row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_clear_points_scale_with_level() {
        let mut scoring = Scoring::new(1);
        assert_eq!(scoring.lock(1).unwrap().points, 100);
        scoring.lock(0);
        assert_eq!(scoring.lock(2).unwrap().points, 300);
        scoring.lock(0);
        assert_eq!(scoring.lock(3).unwrap().points, 500);
        scoring.lock(0);
        assert_eq!(scoring.lock(4).unwrap().points, 800);
        assert_eq!(scoring.score, 1700);
        assert_eq!(scoring.lines, 10);
        assert_eq!(scoring.level(), 2);
        scoring.lock(0);
        assert_eq!(scoring.lock(1).unwrap().points, 200);
    }

    #[test]
    fn back_to_back_tetris_earns_bonus() {
        let mut scoring = Scoring::new(1);
        scoring.lock(4);
        scoring.lock(0);
        let event = scoring.lock(4).unwrap();
        assert!(event.back_to_back);
        assert_eq!(event.points, 1200);
        scoring.lock(0);
        scoring.lock(1);
        scoring.lock(0);
        assert!(!scoring.lock(4).unwrap().back_to_back);
    }

    #[test]
    fn consecutive_clears_build_combo() {
        let mut scoring = Scoring::new(1);
        assert_eq!(scoring.lock(1).unwrap().combo, 0);
        let second = scoring.lock(1).unwrap();
        assert_eq!(second.combo, 1);
        assert_eq!(second.points, 150);
        assert_eq!(scoring.lock(1).unwrap().combo, 2);
        assert!(scoring.lock(0).is_none());
        assert_eq!(scoring.lock(1).unwrap().combo, 0);
    }

    #[test]
    fn drops_add_points() {
        let mut scoring = Scoring::new(1);
        scoring.soft_drop(5);
        scoring.hard_drop(10);
        assert_eq!(scoring.score, 25);
    }

    #[test]
    fn level_progression_respects_start_level() {
        let mut scoring = Scoring::new(5);
        assert_eq!(scoring.level(), 5);
        for _ in 0..9 {
            scoring.lock(1);
        }
        assert_eq!(scoring.level(), 5);
        scoring.lock(1);
        assert_eq!(scoring.level(), 6);
        assert_eq!(Scoring::new(0).level(), 1);
    }

    #[test]
    fn gravity_increases_with_level() {
        assert!((gravity_cells_per_second(1) - 1.0).abs() < 1e-9);
        let mut previous = 0.0;
        for level in 1..=30 {
            let g = gravity_cells_per_second(level);
            assert!(g > previous);
            previous = g;
        }
        assert_eq!(gravity_cells_per_second(99), gravity_cells_per_second(30));
    }
}

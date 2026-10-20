#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spin {
    #[default]
    None,
    Mini,
    Full,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clear {
    pub lines: u32,
    pub spin: Spin,
    pub perfect: bool,
}

impl Clear {
    #[cfg(test)]
    pub fn lines(lines: u32) -> Self {
        Self {
            lines,
            ..Self::default()
        }
    }

    pub fn is_difficult(&self) -> bool {
        self.lines >= 4 || (self.lines > 0 && self.spin != Spin::None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClearEvent {
    pub lines: u32,
    pub spin: Spin,
    pub perfect: bool,
    pub points: u64,
    pub back_to_back: bool,
    pub combo: u32,
    pub attack: u32,
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
const COMBO_ATTACK: [u32; 11] = [0, 1, 1, 2, 2, 3, 3, 4, 4, 4, 5];
const PERFECT_CLEAR_ATTACK: u32 = 10;

pub fn attack(clear: Clear, back_to_back: bool, combo: u32) -> u32 {
    if clear.lines == 0 {
        return 0;
    }
    let base = match (clear.spin, clear.lines) {
        (Spin::Full, 1) => 2,
        (Spin::Full, 2) => 4,
        (Spin::Full, _) => 6,
        (Spin::Mini, 1) => 0,
        (Spin::Mini, _) => 1,
        (Spin::None, 1) => 0,
        (Spin::None, 2) => 1,
        (Spin::None, 3) => 2,
        (Spin::None, _) => 4,
    };
    let combo_bonus = COMBO_ATTACK[(combo as usize).min(COMBO_ATTACK.len() - 1)];
    let perfect = if clear.perfect {
        PERFECT_CLEAR_ATTACK
    } else {
        0
    };
    base + u32::from(back_to_back) + combo_bonus + perfect
}

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

    pub fn back_to_back(&self) -> bool {
        self.back_to_back
    }

    pub fn combo(&self) -> Option<u32> {
        self.combo
    }

    pub fn soft_drop(&mut self, cells: u32) {
        self.score += u64::from(cells);
    }

    pub fn revoke(&mut self, points: u64) {
        self.score = self.score.saturating_sub(points);
    }

    pub fn hard_drop(&mut self, cells: u32) {
        self.score += 2 * u64::from(cells);
    }

    pub fn lock(&mut self, clear: Clear) -> Option<ClearEvent> {
        let level = u64::from(self.level());
        let base: u64 = match (clear.spin, clear.lines) {
            (Spin::None, 0) => {
                self.combo = None;
                return None;
            }
            (Spin::None, 1) => 100,
            (Spin::None, 2) => 300,
            (Spin::None, 3) => 500,
            (Spin::None, _) => 800,
            (Spin::Mini, 0) => 100,
            (Spin::Mini, 1) => 200,
            (Spin::Mini, _) => 400,
            (Spin::Full, 0) => 400,
            (Spin::Full, 1) => 800,
            (Spin::Full, 2) => 1200,
            (Spin::Full, _) => 1600,
        };
        let mut points = base * level;
        let (back_to_back, combo) = if clear.lines == 0 {
            self.combo = None;
            (false, 0)
        } else {
            let difficult = clear.is_difficult();
            let back_to_back = difficult && self.back_to_back;
            if back_to_back {
                points = points * 3 / 2;
            }
            let combo = self.combo.map_or(0, |c| c + 1);
            points += 50 * u64::from(combo) * level;
            if clear.perfect {
                let bonus: u64 = match clear.lines {
                    1 => 800,
                    2 => 1200,
                    3 => 1800,
                    _ => 2000,
                };
                points += bonus * level;
            }
            self.combo = Some(combo);
            self.back_to_back = difficult;
            (back_to_back, combo)
        };
        self.score += points;
        self.lines += clear.lines;
        Some(ClearEvent {
            lines: clear.lines,
            spin: clear.spin,
            perfect: clear.perfect,
            points,
            back_to_back,
            combo,
            attack: attack(clear, back_to_back, combo),
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

    fn spin(lines: u32, spin: Spin) -> Clear {
        Clear {
            lines,
            spin,
            perfect: false,
        }
    }

    #[test]
    fn line_clear_points_scale_with_level() {
        let mut scoring = Scoring::new(1);
        assert_eq!(scoring.lock(Clear::lines(1)).unwrap().points, 100);
        scoring.lock(Clear::lines(0));
        assert_eq!(scoring.lock(Clear::lines(2)).unwrap().points, 300);
        scoring.lock(Clear::lines(0));
        assert_eq!(scoring.lock(Clear::lines(3)).unwrap().points, 500);
        scoring.lock(Clear::lines(0));
        assert_eq!(scoring.lock(Clear::lines(4)).unwrap().points, 800);
        assert_eq!(scoring.score, 1700);
        assert_eq!(scoring.lines, 10);
        assert_eq!(scoring.level(), 2);
        scoring.lock(Clear::lines(0));
        assert_eq!(scoring.lock(Clear::lines(1)).unwrap().points, 200);
    }

    #[test]
    fn back_to_back_tetris_earns_bonus() {
        let mut scoring = Scoring::new(1);
        scoring.lock(Clear::lines(4));
        scoring.lock(Clear::lines(0));
        let event = scoring.lock(Clear::lines(4)).unwrap();
        assert!(event.back_to_back);
        assert_eq!(event.points, 1200);
        scoring.lock(Clear::lines(0));
        scoring.lock(Clear::lines(1));
        scoring.lock(Clear::lines(0));
        assert!(!scoring.lock(Clear::lines(4)).unwrap().back_to_back);
    }

    #[test]
    fn t_spins_score_and_chain_back_to_back() {
        let mut scoring = Scoring::new(1);
        let tsd = scoring.lock(spin(2, Spin::Full)).unwrap();
        assert_eq!(tsd.points, 1200);
        assert_eq!(tsd.attack, 4);
        scoring.lock(Clear::lines(0));
        let tetris = scoring.lock(Clear::lines(4)).unwrap();
        assert!(tetris.back_to_back);
        assert_eq!(tetris.attack, 5);
        let zero = scoring.lock(spin(0, Spin::Full)).unwrap();
        assert_eq!(zero.points, 400);
        assert_eq!(zero.attack, 0);
        assert!(scoring.back_to_back());
        assert_eq!(scoring.combo(), None);
    }

    #[test]
    fn mini_spins_are_weaker() {
        let mut scoring = Scoring::new(1);
        let mini = scoring.lock(spin(1, Spin::Mini)).unwrap();
        assert_eq!(mini.points, 200);
        assert_eq!(mini.attack, 0);
        assert!(scoring.back_to_back());
    }

    #[test]
    fn consecutive_clears_build_combo() {
        let mut scoring = Scoring::new(1);
        assert_eq!(scoring.lock(Clear::lines(1)).unwrap().combo, 0);
        let second = scoring.lock(Clear::lines(1)).unwrap();
        assert_eq!(second.combo, 1);
        assert_eq!(second.points, 150);
        assert_eq!(second.attack, 1);
        assert_eq!(scoring.lock(Clear::lines(1)).unwrap().combo, 2);
        assert!(scoring.lock(Clear::lines(0)).is_none());
        assert_eq!(scoring.lock(Clear::lines(1)).unwrap().combo, 0);
    }

    #[test]
    fn attack_table_matches_versus_rules() {
        assert_eq!(attack(Clear::lines(1), false, 0), 0);
        assert_eq!(attack(Clear::lines(2), false, 0), 1);
        assert_eq!(attack(Clear::lines(3), false, 0), 2);
        assert_eq!(attack(Clear::lines(4), false, 0), 4);
        assert_eq!(attack(spin(3, Spin::Full), true, 0), 7);
        assert_eq!(attack(Clear::lines(1), false, 10), 5);
        assert_eq!(attack(Clear::lines(1), false, 50), 5);
        let perfect = Clear {
            lines: 4,
            spin: Spin::None,
            perfect: true,
        };
        assert_eq!(attack(perfect, false, 0), 14);
    }

    #[test]
    fn perfect_clear_adds_points() {
        let mut scoring = Scoring::new(1);
        let event = scoring
            .lock(Clear {
                lines: 4,
                spin: Spin::None,
                perfect: true,
            })
            .unwrap();
        assert_eq!(event.points, 2800);
        assert!(event.perfect);
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
            scoring.lock(Clear::lines(1));
        }
        assert_eq!(scoring.level(), 5);
        scoring.lock(Clear::lines(1));
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

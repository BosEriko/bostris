use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::piece::Kind;

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self((seed ^ 0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn from_entropy() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let stack = &nanos as *const u64 as u64;
        Self::new(nanos ^ stack.rotate_left(32) ^ u64::from(std::process::id()))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, bound: u32) -> u32 {
        (((self.next_u64() >> 32) * u64::from(bound)) >> 32) as u32
    }
}

#[derive(Clone, Debug)]
pub struct Randomizer {
    rng: Rng,
    queue: VecDeque<Kind>,
}

impl Randomizer {
    pub fn new(rng: Rng) -> Self {
        let mut randomizer = Self {
            rng,
            queue: VecDeque::with_capacity(14),
        };
        randomizer.refill();
        randomizer
    }

    fn refill(&mut self) {
        while self.queue.len() < 7 {
            let mut bag = Kind::ALL;
            for i in (1..bag.len()).rev() {
                let j = self.rng.below(i as u32 + 1) as usize;
                bag.swap(i, j);
            }
            self.queue.extend(bag);
        }
    }

    pub fn next(&mut self) -> Kind {
        let kind = self.queue.pop_front().unwrap_or(Kind::T);
        self.refill();
        kind
    }

    pub fn preview(&self) -> impl Iterator<Item = Kind> + '_ {
        self.queue.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bag_contains_each_piece_once() {
        let mut randomizer = Randomizer::new(Rng::new(42));
        for _ in 0..100 {
            let mut seen = [false; 7];
            for _ in 0..7 {
                let kind = randomizer.next();
                let index = Kind::ALL.iter().position(|&k| k == kind).unwrap();
                assert!(!seen[index]);
                seen[index] = true;
            }
            assert!(seen.iter().all(|&s| s));
        }
    }

    #[test]
    fn preview_matches_upcoming_pieces() {
        let mut randomizer = Randomizer::new(Rng::new(7));
        let preview: Vec<Kind> = randomizer.preview().take(6).collect();
        assert!(randomizer.preview().count() >= 7);
        let drawn: Vec<Kind> = (0..6).map(|_| randomizer.next()).collect();
        assert_eq!(preview, drawn);
    }

    #[test]
    fn different_seeds_produce_different_sequences() {
        let mut a = Randomizer::new(Rng::new(1));
        let mut b = Randomizer::new(Rng::new(2));
        let a: Vec<Kind> = (0..28).map(|_| a.next()).collect();
        let b: Vec<Kind> = (0..28).map(|_| b.next()).collect();
        assert_ne!(a, b);
    }

    #[test]
    fn rng_bound_is_respected() {
        let mut rng = Rng::new(3);
        for bound in 1..20 {
            for _ in 0..200 {
                assert!(rng.below(bound) < bound);
            }
        }
    }
}

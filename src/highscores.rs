use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::APP_DIR;

pub const MAX_ENTRIES: usize = 10;
const FILE_NAME: &str = "highscores.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub score: u64,
    pub lines: u32,
    pub level: u32,
    #[serde(default)]
    pub timestamp: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    entries: Vec<Entry>,
}

#[derive(Debug, Default)]
pub struct HighScores {
    path: Option<PathBuf>,
    entries: Vec<Entry>,
}

pub fn default_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(APP_DIR).join(FILE_NAME))
}

impl HighScores {
    pub fn load(path: Option<PathBuf>) -> Self {
        let entries = path
            .as_deref()
            .and_then(|p| fs::read_to_string(p).ok())
            .map(|text| parse(&text))
            .unwrap_or_default();
        Self { path, entries }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn best(&self) -> u64 {
        self.entries.first().map_or(0, |e| e.score)
    }

    pub fn record(&mut self, score: u64, lines: u32, level: u32) -> Option<usize> {
        if score == 0 {
            return None;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let entry = Entry {
            score,
            lines,
            level,
            timestamp,
        };
        let rank = self
            .entries
            .iter()
            .position(|e| e.score < score)
            .unwrap_or(self.entries.len());
        if rank >= MAX_ENTRIES {
            return None;
        }
        self.entries.insert(rank, entry);
        self.entries.truncate(MAX_ENTRIES);
        Some(rank)
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        save_to(path, &self.entries)
    }
}

fn parse(text: &str) -> Vec<Entry> {
    let mut entries = toml::from_str::<File>(text)
        .map(|file| file.entries)
        .unwrap_or_default();
    entries.sort_by_key(|e| std::cmp::Reverse(e.score));
    entries.truncate(MAX_ENTRIES);
    entries
}

fn save_to(path: &Path, entries: &[Entry]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File {
        entries: entries.to_vec(),
    };
    let text = toml::to_string(&file).map_err(io::Error::other)?;
    let temp = path.with_extension("toml.tmp");
    fs::write(&temp, text)?;
    fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("bostris-test-{}-{name}", std::process::id()))
            .join(FILE_NAME)
    }

    #[test]
    fn scores_are_ranked_and_truncated() {
        let mut scores = HighScores::default();
        assert_eq!(scores.record(0, 0, 1), None);
        for score in 1..=12u64 {
            scores.record(score * 100, 1, 1);
        }
        assert_eq!(scores.entries().len(), MAX_ENTRIES);
        assert_eq!(scores.best(), 1200);
        assert_eq!(scores.record(50, 1, 1), None);
        assert_eq!(scores.record(650, 1, 1), Some(6));
        assert_eq!(scores.record(5000, 1, 1), Some(0));
        assert_eq!(scores.entries().last().unwrap().score, 500);
    }

    #[test]
    fn scores_round_trip_through_disk() {
        let path = temp_path("roundtrip");
        let mut scores = HighScores::load(Some(path.clone()));
        assert!(scores.entries().is_empty());
        scores.record(1234, 12, 2);
        scores.record(99, 1, 1);
        scores.save().unwrap();
        let loaded = HighScores::load(Some(path.clone()));
        assert_eq!(loaded.entries().len(), 2);
        assert_eq!(loaded.best(), 1234);
        assert_eq!(loaded.entries()[0].lines, 12);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupted_file_is_ignored() {
        let path = temp_path("corrupt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "\u{0}garbage = [[[").unwrap();
        let mut scores = HighScores::load(Some(path.clone()));
        assert!(scores.entries().is_empty());
        scores.record(10, 1, 1);
        scores.save().unwrap();
        assert_eq!(HighScores::load(Some(path.clone())).best(), 10);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn unsorted_file_is_sorted_on_load() {
        let entries = parse(
            "[[entries]]\nscore = 5\nlines = 0\nlevel = 1\n\n[[entries]]\nscore = 50\nlines = 1\nlevel = 1\n",
        );
        assert_eq!(entries[0].score, 50);
        assert_eq!(entries[1].score, 5);
    }
}

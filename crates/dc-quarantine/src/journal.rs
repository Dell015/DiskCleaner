//! The journal of one cleanup in one holding folder: one JSON object per line, appended and
//! flushed before each step. The state of a cleanup is whatever its journal says, read back.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What an item looked like when it was held.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Where it was.
    pub original: PathBuf,
    /// Where it is held.
    pub held: PathBuf,
    /// A folder.
    pub is_dir: bool,
    /// Bytes (file lengths).
    pub bytes: u64,
    /// Files (1 for a file).
    pub files: u64,
    /// "safe", "probably_safe" or "your_call".
    pub group: String,
    /// Short title.
    pub title: String,
    /// Reason id.
    pub reason: String,
}

/// One journal line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Line {
    /// The cleanup started.
    Start {
        /// When (Unix seconds).
        time: i64,
    },
    /// About to move item `n` in one step.
    Hold {
        /// Item number.
        n: u32,
        /// What it is.
        record: Record,
    },
    /// Item `n` moved.
    Held {
        /// Item number.
        n: u32,
    },
    /// Item `n` could not be moved as a whole; its files are moved one by one.
    Split {
        /// Item number.
        n: u32,
    },
    /// About to move one file of a split item (path relative to the item).
    File {
        /// Item number.
        n: u32,
        /// Relative path.
        rel: String,
    },
    /// That file moved.
    FileHeld {
        /// Item number.
        n: u32,
        /// Relative path.
        rel: String,
    },
    /// Item `n` was not moved (nothing of it, or nothing more).
    Failed {
        /// Item number.
        n: u32,
        /// Why.
        why: String,
    },
    /// About to move item `n` back to `to`.
    Restore {
        /// Item number.
        n: u32,
        /// Where to.
        to: PathBuf,
    },
    /// One file of a split item is back (path relative to the item).
    FileRestored {
        /// Item number.
        n: u32,
        /// Relative path.
        rel: String,
    },
    /// Item `n` is back.
    Restored {
        /// Item number.
        n: u32,
        /// Where it went.
        to: PathBuf,
    },
    /// About to delete the held items for good.
    Empty {
        /// When.
        time: i64,
    },
    /// The held items are gone.
    Emptied {
        /// When.
        time: i64,
    },
}

/// Read a journal. Every record that can be read is kept, even when a damaged write left two
/// records on one line (a lost line break) or a half-written record in front of a good one.
/// Pieces that cannot be read are skipped: the step they announced is judged by the disk.
pub fn read(path: &Path) -> Vec<Line> {
    let text = std::fs::read(path).unwrap_or_default();
    parse(&String::from_utf8_lossy(&text))
}

/// The records in a journal's text (see [`read`]).
pub fn parse(text: &str) -> Vec<Line> {
    // Every record starts with `{"op":` (the tag is written first). That text cannot appear
    // inside a record: quotes inside strings are escaped.
    const START: &str = "{\"op\":";
    let mut out = Vec::new();
    for line in text.lines() {
        let mut at = 0;
        while let Some(found) = line[at..].find(START) {
            let begin = at + found;
            let mut stream = serde_json::Deserializer::from_str(&line[begin..]).into_iter::<Line>();
            match stream.next() {
                Some(Ok(record)) => {
                    out.push(record);
                    at = begin + stream.byte_offset();
                }
                _ => at = begin + START.len(),
            }
        }
    }
    out
}

/// A line as text.
pub fn text(line: &Line) -> String {
    serde_json::to_string(line).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_survive_a_lost_line_break_and_a_torn_write() {
        let a = text(&Line::Start { time: 7 });
        let b = text(&Line::Held { n: 1 });
        let c = text(&Line::Hold {
            n: 2,
            record: Record {
                original: PathBuf::from(r#"C:\x\{"op":odd}"#),
                held: PathBuf::from(r"C:\h\2\y"),
                is_dir: false,
                bytes: 1,
                files: 1,
                group: "safe".into(),
                title: "t".into(),
                reason: "r".into(),
            },
        });
        // b lost its line break; a half record sits in front of c.
        let joined = format!("{a}\n{b}{c}\n{{\"op\":\"held\",\"n\"{c}\n");
        let lines = parse(&joined);
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert_eq!(lines[0], Line::Start { time: 7 });
        assert_eq!(lines[1], Line::Held { n: 1 });
        assert!(matches!(lines[2], Line::Hold { n: 2, .. }));
        assert!(matches!(lines[3], Line::Hold { n: 2, .. }));
        assert!(parse("garbage\n{\"op\":\n").is_empty());
    }
}

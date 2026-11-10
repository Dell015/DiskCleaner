//! Questions about a finished scan, answered from its view file: where things are, what they
//! belong to, what the checks say, what would happen if they were deleted or moved. Used by the
//! scan report and by the MCP server that lets a person's own AI assistant look at the scan.
//!
//! Read-only and offline: it reads one view file and never touches the disk otherwise.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Flag bits as written in the view file (see [`crate::view::flag`]).
use crate::view::flag;

const DAY: i64 = 86_400;

/// An owner as stored in a view file.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OwnerInfo {
    /// Display name.
    pub name: String,
    /// "app", "vendor" or "windows".
    pub kind: String,
    /// Publisher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// Version text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Install date (YYYY-MM-DD).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed: Option<String>,
    /// Size the installer stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stated_bytes: Option<u64>,
    /// Has an uninstaller.
    #[serde(default)]
    pub uninstaller: bool,
    /// Last started (Unix seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<i64>,
    /// Times started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs: Option<u32>,
    /// Registered install folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

/// A piece of evidence.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EvidenceInfo {
    /// Points it adds.
    pub points: u32,
    /// Plain words.
    pub text: String,
}

/// A mark (what a check says) as stored in a view file.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MarkInfo {
    /// The entry.
    pub node: u32,
    /// "safe", "probably_safe" or "dont_touch".
    pub risk: String,
    /// "rule", "leftover" or "refused".
    pub source: String,
    /// Short title.
    pub title: String,
    /// Stable reason id.
    pub reason: String,
    /// Plain-language detail.
    pub detail: String,
    /// "low", "medium", "high" (leftovers).
    #[serde(default)]
    pub confidence: Option<String>,
    /// Why, point by point.
    #[serde(default)]
    pub evidence: Vec<EvidenceInfo>,
    /// Offered as a suggestion.
    pub offered: bool,
    /// Size on disk.
    pub bytes: u64,
}

#[derive(Debug, Deserialize)]
struct RawColumns {
    name: Vec<String>,
    parent: Vec<i64>,
    kind: Vec<u8>,
    size: Vec<u64>,
    mtime: Vec<i64>,
    flags: Vec<u32>,
    #[serde(default)]
    cat: Vec<i8>,
}

#[derive(Debug, Deserialize)]
struct RawView {
    schema: String,
    schema_version: u32,
    #[serde(default)]
    created_unix: i64,
    #[serde(default)]
    categories: Vec<String>,
    nodes: RawColumns,
    #[serde(default)]
    owners: Vec<OwnerInfo>,
    #[serde(default)]
    assignments: Vec<[u32; 3]>,
    #[serde(default)]
    marks: Vec<MarkInfo>,
}

/// Why a view file could not be used.
#[derive(Debug, thiserror::Error)]
pub enum InspectError {
    /// Not JSON, or not the expected shape.
    #[error("not a DiskCleaner view file: {0}")]
    Format(String),
    /// The columns or links do not agree.
    #[error("the view file is damaged: {0}")]
    Damaged(String),
}

/// A finished scan, indexed for questions.
#[derive(Debug)]
pub struct Inspector {
    /// When the scan was made (Unix seconds).
    pub created: i64,
    names: Vec<String>,
    parent: Vec<i64>,
    kind: Vec<u8>,
    mtime: Vec<i64>,
    flags: Vec<u32>,
    cat: Vec<i8>,
    categories: Vec<String>,
    /// Owners (index = owner id).
    pub owners: Vec<OwnerInfo>,
    /// What the checks say.
    pub marks: Vec<MarkInfo>,
    children: Vec<Vec<u32>>,
    agg: Vec<u64>,
    files: Vec<u64>,
    newest: Vec<i64>,
    /// Owner set on the entry itself: (owner, confidence).
    owner_here: Vec<Option<(u32, u32)>>,
    /// Owner of the entry or its nearest owned folder.
    owner: Vec<Option<(u32, u32)>>,
    /// Mark set on the entry itself.
    mark_here: Vec<Option<u32>>,
    by_path: std::sync::OnceLock<HashMap<String, u32>>,
}

/// One entry, described.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    /// Full path.
    pub path: String,
    /// "file", "folder", "link" or "other".
    pub kind: &'static str,
    /// Size on disk, with everything inside.
    pub bytes: u64,
    /// Files inside (1 for a file).
    pub files: u64,
    /// Last change of the entry or anything inside (YYYY-MM-DD), if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed: Option<String>,
}

/// A Unix time as a civil date (YYYY-MM-DD), UTC; `None` for unknown (0 or less).
#[must_use]
pub fn date_text(secs: i64) -> Option<String> {
    if secs <= 0 {
        return None;
    }
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = secs.div_euclid(DAY) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// Risk order: higher is more careful.
fn rank(risk: &str) -> u8 {
    match risk {
        "safe" => 1,
        "probably_safe" => 2,
        _ => 3,
    }
}

impl Inspector {
    /// Read a view file's text.
    pub fn from_json(text: &str) -> Result<Self, InspectError> {
        let raw: RawView =
            serde_json::from_str(text).map_err(|e| InspectError::Format(e.to_string()))?;
        if raw.schema != "dc-view" || raw.schema_version != 1 {
            return Err(InspectError::Format(format!(
                "{} version {}",
                raw.schema, raw.schema_version
            )));
        }
        let c = raw.nodes;
        let n = c.name.len();
        if [
            c.parent.len(),
            c.kind.len(),
            c.size.len(),
            c.mtime.len(),
            c.flags.len(),
        ]
        .iter()
        .any(|&l| l != n)
            || (!c.cat.is_empty() && c.cat.len() != n)
            || n == 0
        {
            return Err(InspectError::Damaged("columns of different lengths".into()));
        }
        let mut children: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (i, &p) in c.parent.iter().enumerate().skip(1) {
            let ok = usize::try_from(p).is_ok_and(|p| p < i);
            if !ok {
                return Err(InspectError::Damaged(format!("entry {i} has a bad parent")));
            }
            children[p as usize].push(
                u32::try_from(i).map_err(|_| InspectError::Damaged("too many entries".into()))?,
            );
        }
        let mut agg = vec![0u64; n];
        let mut files = vec![0u64; n];
        let mut newest = vec![0i64; n];
        for i in (0..n).rev() {
            let counted = c.flags[i] & flag::NOT_COUNTED == 0;
            if c.kind[i] == 0 {
                agg[i] = agg[i].saturating_add(if counted { c.size[i] } else { 0 });
                files[i] += 1;
            }
            newest[i] = newest[i].max(c.mtime[i]);
            if i > 0 {
                let p = c.parent[i] as usize;
                agg[p] = agg[p].saturating_add(agg[i]);
                files[p] += files[i];
                newest[p] = newest[p].max(newest[i]);
            }
        }
        let mut owner_here = vec![None; n];
        for a in &raw.assignments {
            let (node, owner) = (a[0] as usize, a[1] as usize);
            if node >= n || owner >= raw.owners.len() {
                return Err(InspectError::Damaged("an assignment points nowhere".into()));
            }
            owner_here[node] = Some((a[1], a[2]));
        }
        let mut owner = owner_here.clone();
        for i in 1..n {
            if owner[i].is_none() {
                owner[i] = owner[c.parent[i] as usize];
            }
        }
        let mut mark_here: Vec<Option<u32>> = vec![None; n];
        for (k, m) in raw.marks.iter().enumerate() {
            let node = m.node as usize;
            if node >= n {
                return Err(InspectError::Damaged("a mark points nowhere".into()));
            }
            let k = u32::try_from(k).unwrap_or(u32::MAX);
            let replace = mark_here[node]
                .is_none_or(|old| rank(&m.risk) > rank(&raw.marks[old as usize].risk));
            if replace {
                mark_here[node] = Some(k);
            }
        }
        let cat = if c.cat.is_empty() { vec![-1; n] } else { c.cat };
        Ok(Self {
            created: raw.created_unix,
            names: c.name,
            parent: c.parent,
            kind: c.kind,
            mtime: c.mtime,
            flags: c.flags,
            cat,
            categories: raw.categories,
            owners: raw.owners,
            marks: raw.marks,
            children,
            agg,
            files,
            newest,
            owner_here,
            owner,
            mark_here,
            by_path: std::sync::OnceLock::new(),
        })
    }

    /// Number of entries (including the shared top "This PC").
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Never true for a loaded view (it always has the shared top).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The scanned folders (children of the shared top).
    #[must_use]
    pub fn roots(&self) -> &[u32] {
        &self.children[0]
    }

    /// Full path of an entry.
    #[must_use]
    pub fn path(&self, i: u32) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut at = i as usize;
        while at > 0 {
            parts.push(&self.names[at]);
            let p = self.parent[at];
            if p <= 0 {
                break;
            }
            at = p as usize;
        }
        parts.reverse();
        let Some((first, rest)) = parts.split_first() else {
            return "This PC".to_owned();
        };
        let sep = if first.contains('/') && !first.contains('\\') {
            '/'
        } else {
            '\\'
        };
        let mut out = (*first).to_owned();
        for p in rest {
            if !out.ends_with(sep) {
                out.push(sep);
            }
            out.push_str(p);
        }
        out
    }

    fn key(path: &str) -> String {
        let mut k = path.trim().replace('/', "\\").to_lowercase();
        while k.len() > 3 && k.ends_with('\\') {
            k.pop();
        }
        if k.len() > 1 && k.ends_with('\\') && !k.ends_with(":\\") {
            k.pop();
        }
        k
    }

    /// The entry at a path (any case, either slash), if it was scanned.
    #[must_use]
    pub fn find(&self, path: &str) -> Option<u32> {
        let map = self.by_path.get_or_init(|| {
            let mut m = HashMap::with_capacity(self.len());
            for i in 1..self.len() {
                let i = u32::try_from(i).unwrap_or(u32::MAX);
                m.insert(Self::key(&self.path(i)), i);
            }
            m
        });
        map.get(&Self::key(path)).copied()
    }

    /// Size, files and last change of an entry.
    #[must_use]
    pub fn entry(&self, i: u32) -> Entry {
        let k = i as usize;
        let changed = if self.kind[k] == 1 {
            self.newest[k]
        } else {
            self.mtime[k]
        };
        Entry {
            path: self.path(i),
            kind: match self.kind[k] {
                0 => "file",
                1 => "folder",
                2 => "link",
                _ => "other",
            },
            bytes: self.agg[k],
            files: self.files[k],
            changed: date_text(changed),
        }
    }

    /// Bytes of an entry and everything inside.
    #[must_use]
    pub fn bytes(&self, i: u32) -> u64 {
        self.agg[i as usize]
    }

    /// Direct children, biggest first.
    #[must_use]
    pub fn children(&self, i: u32) -> Vec<u32> {
        let mut c = self.children[i as usize].clone();
        c.sort_by(|a, b| {
            self.agg[*b as usize]
                .cmp(&self.agg[*a as usize])
                .then(a.cmp(b))
        });
        c
    }

    /// Name of an entry.
    #[must_use]
    pub fn name(&self, i: u32) -> &str {
        &self.names[i as usize]
    }

    /// Is it a folder?
    #[must_use]
    pub fn is_dir(&self, i: u32) -> bool {
        self.kind[i as usize] == 1
    }

    /// Is it a file?
    #[must_use]
    pub fn is_file(&self, i: u32) -> bool {
        self.kind[i as usize] == 0
    }

    /// Last change of the entry itself (files) or of anything inside (folders), Unix seconds.
    #[must_use]
    pub fn changed(&self, i: u32) -> i64 {
        let k = i as usize;
        if self.kind[k] == 1 {
            self.newest[k]
        } else {
            self.mtime[k]
        }
    }

    /// Flag bits.
    #[must_use]
    pub fn flags(&self, i: u32) -> u32 {
        self.flags[i as usize]
    }

    /// File category id, if known.
    #[must_use]
    pub fn category(&self, i: u32) -> Option<&str> {
        usize::try_from(self.cat[i as usize])
            .ok()
            .and_then(|c| self.categories.get(c))
            .map(String::as_str)
    }

    /// The owner of an entry (its own, or its nearest owned folder's) and the confidence
    /// (0 low, 1 medium, 2 high), and whether it is set on this entry itself.
    #[must_use]
    pub fn owner(&self, i: u32) -> Option<(&OwnerInfo, u32, bool)> {
        let k = i as usize;
        let (o, conf) = self.owner[k]?;
        Some((&self.owners[o as usize], conf, self.owner_here[k].is_some()))
    }

    /// The owner's index, and whether it is set on this entry itself.
    #[must_use]
    pub fn owner_id(&self, i: u32) -> Option<(u32, bool)> {
        let k = i as usize;
        self.owner[k].map(|(o, _)| (o, self.owner_here[k].is_some()))
    }

    /// The most careful mark on the entry or a folder above it, and the entry it is set on.
    #[must_use]
    pub fn mark(&self, i: u32) -> Option<(&MarkInfo, u32)> {
        let mut best: Option<(u32, u32)> = None;
        let mut at = i as usize;
        loop {
            if let Some(m) = self.mark_here[at] {
                let better = best.is_none_or(|(b, _)| {
                    rank(&self.marks[m as usize].risk) > rank(&self.marks[b as usize].risk)
                });
                if better {
                    best = Some((m, u32::try_from(at).unwrap_or(0)));
                }
            }
            let p = self.parent[at];
            if p < 0 || at == 0 {
                break;
            }
            at = p as usize;
        }
        best.map(|(m, at)| (&self.marks[m as usize], at))
    }

    /// Marks set inside a folder (not above it), biggest first.
    #[must_use]
    pub fn marks_inside(&self, i: u32) -> Vec<(&MarkInfo, u32)> {
        let mut out: Vec<(&MarkInfo, u32)> = self
            .marks
            .iter()
            .filter(|m| m.node != i && self.is_inside(m.node, i))
            .map(|m| (m, m.node))
            .collect();
        out.sort_by_key(|m| std::cmp::Reverse(m.0.bytes));
        out
    }

    /// Is `i` inside folder `folder` (or the folder itself)?
    #[must_use]
    pub fn is_inside(&self, i: u32, folder: u32) -> bool {
        let mut at = i as i64;
        while at >= 0 {
            if at == i64::from(folder) {
                return true;
            }
            at = self.parent[at as usize];
        }
        false
    }

    /// Every entry, in order (parents before children), without the shared top.
    pub fn all(&self) -> impl Iterator<Item = u32> + '_ {
        (1..self.len()).filter_map(|i| u32::try_from(i).ok())
    }

    /// Parent of an entry (None for the shared top).
    #[must_use]
    pub fn parent(&self, i: u32) -> Option<u32> {
        u32::try_from(self.parent[i as usize]).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::date_text as date;

    #[test]
    fn dates_are_civil_dates() {
        assert_eq!(date(0), None);
        assert_eq!(date(1_735_689_600).as_deref(), Some("2025-01-01"));
        assert_eq!(date(951_782_400).as_deref(), Some("2000-02-29"));
        assert_eq!(date(1_762_000_000).as_deref(), Some("2025-11-01"));
    }
}

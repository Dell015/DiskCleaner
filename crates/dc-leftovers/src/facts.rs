//! Facts about every folder, computed in one pass over the tree: the newest modified time
//! inside, whether programs are inside, and what looks like personal data. Names, extensions,
//! sizes and times only; no file is opened.

use dc_core::{Kind, Node, Tree};

/// What looks like personal data. Bits of [`Facts::personal`].
pub mod personal {
    /// Documents.
    pub const DOCUMENTS: u16 = 1;
    /// Large pictures or media.
    pub const MEDIA: u16 = 2;
    /// Databases.
    pub const DATABASES: u16 = 4;
    /// Keys or wallets.
    pub const KEYS: u16 = 8;
    /// Mail.
    pub const MAIL: u16 = 16;
    /// Virtual disks.
    pub const DISKS: u16 = 32;
    /// A folder named like game saves or profiles.
    pub const SAVES: u16 = 64;

    /// Words for the bits that are set.
    #[must_use]
    pub fn describe(bits: u16) -> Vec<String> {
        let table = [
            (DOCUMENTS, "documents"),
            (MEDIA, "large pictures or media"),
            (DATABASES, "databases"),
            (KEYS, "keys or wallets"),
            (MAIL, "mail"),
            (DISKS, "virtual disks"),
            (SAVES, "a folder that looks like game saves or profiles"),
        ];
        table
            .iter()
            .filter(|(bit, _)| bits & bit != 0)
            .map(|(_, text)| (*text).to_owned())
            .collect()
    }
}

const MIB: u64 = 1_048_576;

/// Per-node facts, indexed by node id.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// Newest modified time inside (and of the node itself), or `None` if no time is known.
    pub newest: Vec<Option<i64>>,
    /// Personal-data bits inside.
    pub personal: Vec<u16>,
    /// A program (`.exe` or `.dll`) is inside.
    pub programs: Vec<bool>,
}

fn own_personal_bits(node: &Node) -> u16 {
    if node.kind == Kind::Dir {
        let name = node.name.to_lowercase();
        return if matches!(
            name.as_str(),
            "saves" | "savegames" | "save games" | "savegame" | "profiles"
        ) {
            personal::SAVES
        } else {
            0
        };
    }
    let ext = node.ext.as_deref().unwrap_or("");
    let big = node.logical_size > MIB;
    match ext {
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "pdf" | "odt" | "ods" | "rtf" => {
            personal::DOCUMENTS
        }
        "txt" | "csv" if big => personal::DOCUMENTS,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "heic" | "webp" | "tif" | "tiff" | "mp3"
        | "mp4" | "mkv" | "avi" | "mov" | "wav" | "flac"
            if big =>
        {
            personal::MEDIA
        }
        "db" | "sqlite" | "sqlite3" | "mdb" | "accdb" | "kdbx" => personal::DATABASES,
        "pem" | "key" | "pfx" | "ppk" | "p12" | "wallet" => personal::KEYS,
        _ if node.name.eq_ignore_ascii_case("wallet.dat") => personal::KEYS,
        "pst" | "ost" => personal::MAIL,
        "vhd" | "vhdx" | "vmdk" => personal::DISKS,
        _ => 0,
    }
}

/// Compute the facts for every node. Children come after their parents in the tree, so a
/// single backwards pass pushes each node's facts up to its parent.
#[must_use]
pub fn compute(tree: &Tree) -> Facts {
    let nodes = tree.nodes();
    let n = nodes.len();
    let mut facts = Facts {
        newest: vec![None; n],
        personal: vec![0; n],
        programs: vec![false; n],
    };
    for (i, node) in nodes.iter().enumerate() {
        if let Some(slot) = facts.newest.get_mut(i) {
            *slot = node.mtime.map(|t| t.secs);
        }
        if let Some(slot) = facts.personal.get_mut(i) {
            *slot = own_personal_bits(node);
        }
        if let Some(slot) = facts.programs.get_mut(i) {
            *slot = matches!(node.ext.as_deref(), Some("exe" | "dll"));
        }
    }
    for i in (0..n).rev() {
        let Some(parent) = nodes.get(i).and_then(|node| node.parent) else {
            continue;
        };
        let p = parent as usize;
        if p >= i {
            continue; // a broken tree gives no information (fail closed elsewhere)
        }
        let (newest, bits, programs) = (
            facts.newest.get(i).copied().flatten(),
            facts.personal.get(i).copied().unwrap_or(0),
            facts.programs.get(i).copied().unwrap_or(false),
        );
        if let Some(slot) = facts.newest.get_mut(p) {
            *slot = match (*slot, newest) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
        }
        if let Some(slot) = facts.personal.get_mut(p) {
            *slot |= bits;
        }
        if let Some(slot) = facts.programs.get_mut(p) {
            *slot |= programs;
        }
    }
    facts
}

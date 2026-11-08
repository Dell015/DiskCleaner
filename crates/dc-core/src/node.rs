//! The building blocks of a scanned tree: [`Node`], [`Kind`], [`Flags`], [`Category`].

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Index of a node inside a [`crate::Tree`].
pub type NodeId = u32;

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A regular file.
    File,
    /// A directory that was (or could be) listed.
    Dir,
    /// A symlink, junction or other reparse-point directory. Never followed.
    Link,
    /// Anything else (devices, sockets, unknown).
    Other,
}

/// Attribute flags. Plain booleans keep the JSON readable and diff-friendly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Flags {
    /// Has a reparse point (symlink, junction, cloud placeholder, ...).
    #[serde(default, skip_serializing_if = "is_false")]
    pub reparse: bool,
    /// Another path to the same file was already counted.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hardlink: bool,
    /// Cloud placeholder: content is not stored locally. Never hydrated by this tool.
    #[serde(default, skip_serializing_if = "is_false")]
    pub cloud: bool,
    /// Sparse file.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sparse: bool,
    /// NTFS-compressed file.
    #[serde(default, skip_serializing_if = "is_false")]
    pub compressed: bool,
    /// System attribute.
    #[serde(default, skip_serializing_if = "is_false")]
    pub system: bool,
    /// Hidden attribute.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// Read-only attribute.
    #[serde(default, skip_serializing_if = "is_false")]
    pub readonly: bool,
}

// serde's `skip_serializing_if` needs a function taking a reference.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// A coarse type derived from the file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Video files.
    Video,
    /// Images.
    Image,
    /// Audio.
    Audio,
    /// Archives and disk images.
    Archive,
    /// Source code and project files.
    Code,
    /// Documents.
    Document,
    /// Game asset containers and engine data.
    GameAsset,
    /// Installers and packages.
    Installer,
    /// Executables and libraries.
    Executable,
    /// Logs, caches and temporary files.
    Temporary,
    /// Everything else.
    Other,
}

impl Category {
    /// Every category, in a stable order.
    pub const ALL: [Category; 11] = [
        Category::Video,
        Category::Image,
        Category::Audio,
        Category::Archive,
        Category::Code,
        Category::Document,
        Category::GameAsset,
        Category::Installer,
        Category::Executable,
        Category::Temporary,
        Category::Other,
    ];

    /// Category for a lower-case extension without the dot.
    #[must_use]
    pub fn from_extension(ext: &str) -> Category {
        match ext {
            "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg" => {
                Category::Video
            }
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "heic" | "psd"
            | "svg" | "ico" | "raw" | "cr2" | "nef" => Category::Image,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" => Category::Audio,
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "img" | "vhd" | "vhdx"
            | "vmdk" | "cab" => Category::Archive,
            "rs" | "c" | "h" | "cpp" | "hpp" | "cs" | "java" | "kt" | "py" | "js" | "ts"
            | "tsx" | "jsx" | "go" | "rb" | "php" | "swift" | "lua" | "sh" | "ps1" | "bat"
            | "cmd" | "html" | "css" | "json" | "toml" | "yaml" | "yml" | "xml" | "sln"
            | "csproj" | "lock" => Category::Code,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "rtf"
            | "odt" | "ods" | "csv" | "epub" => Category::Document,
            "pak" | "bundle" | "unity3d" | "assets" | "uasset" | "umap" | "wad" | "vpk" | "bsa"
            | "ba2" | "pck" | "bank" | "dat" | "bin" | "blk" | "cache" => Category::GameAsset,
            "msi" | "msix" | "appx" | "msp" | "pkg" | "deb" | "rpm" | "apk" => Category::Installer,
            "exe" | "dll" | "sys" | "ocx" | "scr" | "drv" | "so" | "dylib" => Category::Executable,
            "tmp" | "temp" | "log" | "bak" | "old" | "dmp" | "etl" | "crdownload" | "part" => {
                Category::Temporary
            }
            _ => Category::Other,
        }
    }
}

/// A point in time, seconds and nanoseconds since the Unix epoch (UTC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp {
    /// Whole seconds since 1970-01-01T00:00:00Z (negative before).
    pub secs: i64,
    /// Nanoseconds within the second.
    pub nanos: u32,
}

impl Timestamp {
    /// Convert a [`SystemTime`]. Returns `None` if it does not fit.
    #[must_use]
    pub fn from_system_time(time: SystemTime) -> Option<Timestamp> {
        match time.duration_since(UNIX_EPOCH) {
            Ok(after) => Some(Timestamp {
                secs: i64::try_from(after.as_secs()).ok()?,
                nanos: after.subsec_nanos(),
            }),
            Err(err) => {
                let before = err.duration();
                let secs = i64::try_from(before.as_secs()).ok()?;
                let nanos = before.subsec_nanos();
                if nanos == 0 {
                    Some(Timestamp {
                        secs: -secs,
                        nanos: 0,
                    })
                } else {
                    Some(Timestamp {
                        secs: -secs - 1,
                        nanos: 1_000_000_000 - nanos,
                    })
                }
            }
        }
    }
}

/// Totals for a node and everything below it. Filled by [`crate::Tree::aggregate`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aggregate {
    /// Sum of logical sizes below (a file: its own size).
    pub total_logical: u64,
    /// Sum of sizes on disk below.
    pub total_on_disk: u64,
    /// Counted files below.
    pub file_count: u64,
    /// Directories below (not counting the node itself).
    pub dir_count: u64,
    /// Links below.
    pub link_count: u64,
    /// Direct counted children (before any pruning).
    pub child_count: u64,
}

/// One file, directory or link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// Index in the tree. Set by [`crate::Tree`].
    pub id: NodeId,
    /// Parent index; `None` only for the root.
    pub parent: Option<NodeId>,
    /// Name (lossy text; the root holds the scanned path as given).
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// File length in bytes (0 for directories and links).
    pub logical_size: u64,
    /// Space allocated on disk.
    pub size_on_disk: u64,
    /// Last-modified time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<Timestamp>,
    /// Creation time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctime: Option<Timestamp>,
    /// Last-access time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atime: Option<Timestamp>,
    /// Attribute flags.
    #[serde(default)]
    pub flags: Flags,
    /// Lower-case extension without the dot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext: Option<String>,
    /// Category derived from the extension (files only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,
    /// Distance from the root (root = 0).
    pub depth: u32,
    /// `false` for a duplicate hardlink path: shown, but not counted in totals.
    pub counted: bool,
    /// Where a link points, when readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    /// Totals for this node and below.
    #[serde(flatten)]
    pub agg: Aggregate,
}

impl Node {
    /// A node with neutral defaults. `id` and `parent` are set when it is added to a tree.
    #[must_use]
    pub fn new(name: impl Into<String>, kind: Kind) -> Node {
        Node {
            id: 0,
            parent: None,
            name: name.into(),
            kind,
            logical_size: 0,
            size_on_disk: 0,
            mtime: None,
            ctime: None,
            atime: None,
            flags: Flags::default(),
            ext: None,
            category: None,
            depth: 0,
            counted: true,
            link_target: None,
            agg: Aggregate::default(),
        }
    }

    /// Set `ext` and `category` from the file name.
    pub fn set_extension_from_name(&mut self) {
        let ext = self
            .name
            .rsplit_once('.')
            .filter(|(stem, ext)| !stem.is_empty() && !ext.is_empty())
            .map(|(_, ext)| ext.to_lowercase());
        self.category = ext.as_deref().map(Category::from_extension);
        self.ext = ext;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_is_lowercased_and_dotfiles_have_none() {
        let mut node = Node::new("Movie.MKV", Kind::File);
        node.set_extension_from_name();
        assert_eq!(node.ext.as_deref(), Some("mkv"));
        assert_eq!(node.category, Some(Category::Video));

        for name in [".gitignore", "README", "trailing."] {
            let mut node = Node::new(name, Kind::File);
            node.set_extension_from_name();
            assert_eq!(node.ext, None, "{name}");
        }
    }

    #[test]
    fn category_table_spot_checks() {
        assert_eq!(Category::from_extension("exe"), Category::Executable);
        assert_eq!(Category::from_extension("pak"), Category::GameAsset);
        assert_eq!(Category::from_extension("zip"), Category::Archive);
        assert_eq!(Category::from_extension("rs"), Category::Code);
        assert_eq!(Category::from_extension("tmp"), Category::Temporary);
        assert_eq!(Category::from_extension("zzz"), Category::Other);
    }

    #[test]
    fn timestamps_round_trip_around_the_epoch() {
        let t = Timestamp::from_system_time(UNIX_EPOCH).unwrap();
        assert_eq!(t, Timestamp { secs: 0, nanos: 0 });
        let before = UNIX_EPOCH - std::time::Duration::from_millis(1500);
        let t = Timestamp::from_system_time(before).unwrap();
        assert_eq!(
            t,
            Timestamp {
                secs: -2,
                nanos: 500_000_000
            }
        );
    }
}

//! Windows file-attribute bits, decoded into [`Flags`]. Pure, so it is tested everywhere.

use dc_core::Flags;

const READONLY: u32 = 0x0000_0001;
const HIDDEN: u32 = 0x0000_0002;
const SYSTEM: u32 = 0x0000_0004;
const SPARSE_FILE: u32 = 0x0000_0200;
const REPARSE_POINT: u32 = 0x0000_0400;
const COMPRESSED: u32 = 0x0000_0800;
const OFFLINE: u32 = 0x0000_1000;
const RECALL_ON_OPEN: u32 = 0x0004_0000;
const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

/// Decode `FILE_ATTRIBUTE_*` bits. "Cloud" means the content is not stored locally.
#[must_use]
pub fn flags_from_windows_attributes(attributes: u32) -> Flags {
    Flags {
        readonly: attributes & READONLY != 0,
        hidden: attributes & HIDDEN != 0,
        system: attributes & SYSTEM != 0,
        sparse: attributes & SPARSE_FILE != 0,
        reparse: attributes & REPARSE_POINT != 0,
        compressed: attributes & COMPRESSED != 0,
        cloud: attributes & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0,
        hardlink: false,
    }
}

/// `true` if the reparse-point bit is set.
#[must_use]
pub fn is_reparse(attributes: u32) -> bool {
    attributes & REPARSE_POINT != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_each_bit() {
        assert!(flags_from_windows_attributes(READONLY).readonly);
        assert!(flags_from_windows_attributes(HIDDEN).hidden);
        assert!(flags_from_windows_attributes(SYSTEM).system);
        assert!(flags_from_windows_attributes(SPARSE_FILE).sparse);
        assert!(flags_from_windows_attributes(REPARSE_POINT).reparse);
        assert!(flags_from_windows_attributes(COMPRESSED).compressed);
        assert!(flags_from_windows_attributes(OFFLINE).cloud);
        assert!(flags_from_windows_attributes(RECALL_ON_OPEN).cloud);
        assert!(flags_from_windows_attributes(RECALL_ON_DATA_ACCESS).cloud);
        assert_eq!(flags_from_windows_attributes(0x20), Flags::default()); // ARCHIVE only
    }

    #[test]
    fn combined_bits() {
        let f = flags_from_windows_attributes(HIDDEN | SYSTEM | REPARSE_POINT);
        assert!(f.hidden && f.system && f.reparse && !f.cloud);
        assert!(is_reparse(REPARSE_POINT | HIDDEN));
        assert!(!is_reparse(HIDDEN));
    }
}

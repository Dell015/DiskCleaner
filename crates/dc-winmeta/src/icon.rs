//! App icons: where an app's icon is ("C:\x\app.exe,0" as Windows records it) and its pixels.
//! Reading an icon opens the file for reading only; nothing is run or changed.

use serde::Serialize;

/// An icon's pixels: `width * height` pixels, 4 bytes each (red, green, blue, alpha).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Icon {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixels, row by row from the top.
    pub rgba: Vec<u8>,
}

/// Split an icon location as Windows records it (`"C:\x\app.exe",0`, `C:\x\app.ico`,
/// `C:\x\app.exe,-101`) into the file and the icon's index. `None` if there is no file.
#[must_use]
pub fn parse_icon_location(text: &str) -> Option<(String, i32)> {
    let text = text.trim();
    let (file, index) = match text.rsplit_once(',') {
        Some((file, index)) if index.trim().parse::<i32>().is_ok() => {
            (file, index.trim().parse::<i32>().unwrap_or(0))
        }
        _ => (text, 0),
    };
    let file = file.trim().trim_matches('"').trim();
    // Only real files: a full path with a drive or a share.
    let full = file.as_bytes().get(1) == Some(&b':') || file.starts_with(r"\\");
    (full && !file.is_empty()).then(|| (file.to_owned(), index))
}

/// The icon at `location` (see [`parse_icon_location`]), `size` pixels wide (16 to 256).
/// Windows only; elsewhere always `None`.
#[must_use]
pub fn extract_icon(location: &str, size: u32) -> Option<Icon> {
    let (file, index) = parse_icon_location(location)?;
    #[cfg(windows)]
    {
        crate::windows::icon::extract(&file, index, size.clamp(16, 256))
    }
    #[cfg(not(windows))]
    {
        let _ = (file, index, size);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_locations_as_windows_writes_them() {
        assert_eq!(
            parse_icon_location(r#""C:\Program Files\App\app.exe",0"#),
            Some((r"C:\Program Files\App\app.exe".into(), 0))
        );
        assert_eq!(
            parse_icon_location(r"C:\HoYoPlay\1.19.0.382\ico\hk4e_global.ico"),
            Some((r"C:\HoYoPlay\1.19.0.382\ico\hk4e_global.ico".into(), 0))
        );
        assert_eq!(
            parse_icon_location(r"C:\x\a, b\app.exe,-101"),
            Some((r"C:\x\a, b\app.exe".into(), -101))
        );
        assert_eq!(
            parse_icon_location(r"C:\x\a, b\app.exe"),
            Some((r"C:\x\a, b\app.exe".into(), 0))
        );
        assert_eq!(parse_icon_location("app.exe,0"), None);
        assert_eq!(parse_icon_location("  "), None);
        assert_eq!(parse_icon_location(r#""","#), None);
    }
}

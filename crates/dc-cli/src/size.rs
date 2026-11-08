//! Parsing and printing byte sizes. All units are binary (1 KB = 1024 bytes), like Explorer.

use anyhow::{bail, Context, Result};

/// Parse `"4096"`, `"10KB"`, `"1.5 GiB"`, `"2m"` into bytes.
pub fn parse_size(text: &str) -> Result<u64> {
    let text = text.trim();
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    if number.is_empty() {
        bail!("not a size: {text:?}");
    }
    let value: f64 = number
        .parse()
        .with_context(|| format!("not a number: {number:?}"))?;
    let multiplier: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        other => bail!("unknown size unit {other:?} in {text:?}"),
    };
    let bytes = value * multiplier;
    if !bytes.is_finite() || !(0.0..1.8e19).contains(&bytes) {
        bail!("size out of range: {text:?}");
    }
    // Truncation is intended: sizes are whole bytes. Range was checked above.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(bytes as u64)
}

/// `1536` -> `"1.50 KiB"`.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.2} {}", UNITS.get(unit).copied().unwrap_or("B"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_forms() {
        assert_eq!(parse_size("4096").unwrap(), 4096);
        assert_eq!(parse_size("10KB").unwrap(), 10 * 1024);
        assert_eq!(parse_size("1.5 GiB").unwrap(), 1_610_612_736);
        assert_eq!(parse_size("2m").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_size("0").unwrap(), 0);
    }

    #[test]
    fn rejects_nonsense() {
        for bad in ["", "KB", "abc", "10XB", "-5", "1..2", "99999999999999TB"] {
            assert!(parse_size(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1536), "1.50 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.00 GiB");
    }
}

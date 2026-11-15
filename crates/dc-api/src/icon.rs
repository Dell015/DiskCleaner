//! App icons as small PNG pictures for the page (`data:image/png;base64,...`). The PNG is written
//! uncompressed (stored blocks): icons are tiny, and this needs no extra library.

/// The icon at `location` (as Windows records it) as a PNG data URL, 32 pixels wide.
#[must_use]
pub fn data_url(location: &str) -> Option<String> {
    let icon = dc_winmeta::extract_icon(location, 32)?;
    let png = png(icon.width, icon.height, &icon.rgba)?;
    Some(format!("data:image/png;base64,{}", base64(&png)))
}

fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for part in parts {
        for &b in *part {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
    }
    !crc
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Option<()> {
    out.extend_from_slice(&u32::try_from(data.len()).ok()?.to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
    Some(())
}

/// An RGBA picture as a PNG file. `None` if the sizes do not add up.
#[must_use]
pub fn png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let row = usize::try_from(width).ok()?.checked_mul(4)?;
    if width == 0 || height == 0 || rgba.len() != row.checked_mul(usize::try_from(height).ok()?)? {
        return None;
    }
    // Each row starts with filter 0 (none).
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for line in rgba.chunks_exact(row) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    // zlib stream of stored blocks.
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = u16::try_from(block.len()).ok()?;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, no interlace
    chunk(&mut out, b"IHDR", &header)?;
    chunk(&mut out, b"IDAT", &z)?;
    chunk(&mut out, b"IEND", &[])?;
    Some(out)
}

/// Standard base64 with padding.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let n = group
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= group.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_png_is_well_formed() {
        // 2x1: one red, one see-through pixel.
        let p = png(2, 1, &[255, 0, 0, 255, 0, 0, 0, 0]).unwrap();
        assert_eq!(&p[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&p[12..16], b"IHDR");
        // The known CRC of IEND.
        assert_eq!(&p[p.len() - 4..], &[0xAE, 0x42, 0x60, 0x82]);
        assert!(png(2, 2, &[0; 8]).is_none());
        assert!(png(0, 1, &[]).is_none());
        // Bigger than one stored block.
        assert!(png(200, 100, &vec![7; 200 * 100 * 4]).is_some());
    }
}

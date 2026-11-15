//! Reading an icon's pixels through the shell (`SHDefExtractIconW`), which loads the file as data
//! only. Each handle and bitmap is released exactly once.

use std::ffi::c_void;
use std::ptr;

use windows_sys::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows_sys::Win32::UI::Shell::SHDefExtractIconW;
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

use crate::icon::Icon;

/// An icon handle, destroyed when dropped.
struct IconHandle(HICON);
impl Drop for IconHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from SHDefExtractIconW, is non-null and is destroyed once.
        unsafe {
            DestroyIcon(self.0);
        }
    }
}

/// A bitmap handle, deleted when dropped (null is allowed and skipped).
struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the bitmap came from GetIconInfo, which hands ownership to the caller; it
            // is deleted once.
            unsafe {
                DeleteObject(self.0);
            }
        }
    }
}

/// The pixels of `bitmap` as 32-bit top-down BGRA, with its width and height.
fn pixels(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let mut bm = BITMAP::default();
    let size = i32::try_from(std::mem::size_of::<BITMAP>()).ok()?;
    // SAFETY: `bm` is a writable BITMAP of `size` bytes; `bitmap` is a valid bitmap handle.
    let got = unsafe { GetObjectW(bitmap, size, ptr::addr_of_mut!(bm).cast::<c_void>()) };
    if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 || bm.bmWidth > 256 || bm.bmHeight > 512 {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight);
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).ok()?,
            biWidth: w,
            biHeight: -h, // top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let (wu, hu) = (u32::try_from(w).ok()?, u32::try_from(h).ok()?);
    let mut out = vec![0u8; (wu as usize) * (hu as usize) * 4];
    // SAFETY: a screen device context, released below.
    let dc = unsafe { GetDC(ptr::null_mut()) };
    if dc.is_null() {
        return None;
    }
    // SAFETY: `out` holds exactly `h` rows of `w` 32-bit pixels as `info` describes; `dc` and
    // `bitmap` are valid; the bitmap is not selected into any device context.
    let lines = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            hu,
            out.as_mut_ptr().cast::<c_void>(),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: `dc` came from GetDC(null) above and is released once.
    unsafe {
        ReleaseDC(ptr::null_mut(), dc);
    }
    (lines == h).then_some((wu, hu, out))
}

/// The icon `index` of `file`, about `size` pixels wide, as RGBA.
pub fn extract(file: &str, index: i32, size: u32) -> Option<Icon> {
    let name: Vec<u16> = file.encode_utf16().chain(std::iter::once(0)).collect();
    let mut large: HICON = ptr::null_mut();
    // SAFETY: `name` is NUL-terminated and outlives the call; one icon handle is written to
    // `large` (no small icon is asked for); the size asks for `size` pixels.
    let hr = unsafe {
        SHDefExtractIconW(
            name.as_ptr(),
            index,
            0,
            &mut large,
            ptr::null_mut(),
            size & 0xFFFF,
        )
    };
    if hr != 0 || large.is_null() {
        return None;
    }
    let icon = IconHandle(large);
    let mut info = ICONINFO::default();
    // SAFETY: `icon.0` is a valid icon; `info` is writable. The two bitmaps it returns are ours.
    if unsafe { GetIconInfo(icon.0, &mut info) } == 0 {
        return None;
    }
    let color = Bitmap(info.hbmColor);
    let mask = Bitmap(info.hbmMask);
    if color.0.is_null() {
        return None; // black-and-white icons are not worth showing
    }
    let (width, height, mut px) = pixels(color.0)?;
    // Old icons have no alpha; their mask says what is transparent (white = see-through).
    if px.chunks_exact(4).all(|p| p[3] == 0) {
        let (mw, mh, m) = pixels(mask.0)?;
        if mw != width || mh < height {
            return None;
        }
        for (p, q) in px.chunks_exact_mut(4).zip(m.chunks_exact(4)) {
            p[3] = if q[0] == 0 { 255 } else { 0 };
        }
    }
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2); // BGRA to RGBA
    }
    drop(icon);
    Some(Icon {
        width,
        height,
        rgba: px,
    })
}

//! App-icon extraction. Pulls an executable's embedded icon and returns it as a
//! PNG `data:` URI for the details pane. Windows-only; any failure (empty path,
//! no icon, UWP stub, unreadable file) returns `None` so the UI falls back to the
//! two-letter monogram rather than showing a broken or wrong image.

#[cfg(windows)]
pub fn extract_icon_data_uri(exe_path: &str) -> Option<String> {
    if exe_path.is_empty() {
        return None;
    }
    let icon = unsafe { extract_rgba(exe_path)? };
    encode_data_uri(&icon)
}

#[cfg(not(windows))]
pub fn extract_icon_data_uri(_exe_path: &str) -> Option<String> {
    None
}

#[cfg(windows)]
struct IconRgba {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

#[cfg(windows)]
unsafe fn extract_rgba(exe_path: &str) -> Option<IconRgba> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

    let wide: Vec<u16> = std::ffi::OsStr::new(exe_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // First large icon embedded in the executable (typically 32x32).
    let mut hicon = HICON::default();
    let count = ExtractIconExW(PCWSTR(wide.as_ptr()), 0, Some(&mut hicon), None, 1);
    if count == 0 || hicon.is_invalid() {
        return None;
    }

    let result = read_icon_pixels(hicon);
    let _ = DestroyIcon(hicon);
    result
}

#[cfg(windows)]
unsafe fn read_icon_pixels(hicon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<IconRgba> {
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    let mut info = ICONINFO::default();
    GetIconInfo(hicon, &mut info).ok()?;
    let color = info.hbmColor;
    let mask = info.hbmMask;

    let free_bitmaps = || {
        if !color.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(color.0));
        }
        if !mask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(mask.0));
        }
    };

    let mut bmp = BITMAP::default();
    let got = GetObjectW(
        HGDIOBJ(color.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bmp as *mut _ as *mut core::ffi::c_void),
    );
    if got == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
        free_bitmaps();
        return None;
    }
    let w = bmp.bmWidth;
    let h = bmp.bmHeight;
    let px_count = (w * h) as usize;

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h; // negative height => top-down rows
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB.0 as u32;

    let hdc = GetDC(None);
    let mut bgra = vec![0u8; px_count * 4];
    let lines = GetDIBits(
        hdc,
        color,
        0,
        h as u32,
        Some(bgra.as_mut_ptr() as *mut core::ffi::c_void),
        &mut bmi,
        DIB_RGB_COLORS,
    );

    // Legacy icons carry no per-pixel alpha; rebuild it from the AND mask
    // (0 = opaque, non-zero = transparent) so they aren't drawn as black squares.
    let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
    let mut mask_bgra = vec![0u8; px_count * 4];
    if !has_alpha {
        let _ = GetDIBits(
            hdc,
            mask,
            0,
            h as u32,
            Some(mask_bgra.as_mut_ptr() as *mut core::ffi::c_void),
            &mut bmi,
            DIB_RGB_COLORS,
        );
    }
    ReleaseDC(None, hdc);
    free_bitmaps();

    if lines == 0 {
        return None;
    }

    let mut rgba = vec![0u8; px_count * 4];
    for i in 0..px_count {
        rgba[i * 4] = bgra[i * 4 + 2]; // R <- B
        rgba[i * 4 + 1] = bgra[i * 4 + 1]; // G
        rgba[i * 4 + 2] = bgra[i * 4]; // B <- R
        rgba[i * 4 + 3] = if has_alpha {
            bgra[i * 4 + 3]
        } else if mask_bgra[i * 4] == 0 {
            255
        } else {
            0
        };
    }

    Some(IconRgba {
        width: w as u32,
        height: h as u32,
        pixels: rgba,
    })
}

#[cfg(windows)]
fn encode_data_uri(icon: &IconRgba) -> Option<String> {
    use base64::Engine;
    use image::codecs::png::PngEncoder;
    use image::{ExtendedColorType, ImageEncoder};

    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&icon.pixels, icon.width, icon.height, ExtendedColorType::Rgba8)
        .ok()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Some(format!("data:image/png;base64,{b64}"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn extracts_a_known_system_icon_as_png() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let uri = extract_icon_data_uri(&format!("{windir}\\explorer.exe"))
            .expect("explorer.exe must yield an icon");
        assert!(uri.starts_with("data:image/png;base64,"));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&uri["data:image/png;base64,".len()..])
            .expect("payload must be valid base64");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "payload must be a PNG");
    }

    #[test]
    fn unreadable_paths_fall_back_to_none() {
        assert!(extract_icon_data_uri("").is_none());
        assert!(extract_icon_data_uri("C:\\nope\\missing_zzz.exe").is_none());
    }
}

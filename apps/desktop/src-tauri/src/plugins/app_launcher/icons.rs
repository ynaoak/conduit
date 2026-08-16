//! Extract the real application icon of an .exe / .lnk as a base64 PNG.
//! Returns None when extraction fails; callers fall back to an emoji icon.

use std::cell::Cell;
use std::mem::size_of;

use base64::Engine;
use windows::core::{Interface, PCWSTR};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::UI::Shell::{
    ExtractIconExW, IShellLinkW, SHGetFileInfoW, ShellLink, SHFILEINFOW, SHGFI_ICON,
    SHGFI_LARGEICON,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

thread_local! {
    /// SHGetFileInfoW requires COM to be initialized on the calling thread
    static COM_INITIALIZED: Cell<bool> = const { Cell::new(false) };
}

fn ensure_com_initialized() {
    COM_INITIALIZED.with(|initialized| {
        if !initialized.get() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            initialized.set(true);
        }
    });
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Extract the application icon of `path` as a base64-encoded PNG.
///
/// Shortcuts are resolved through IShellLinkW first: querying the shell for a
/// .lnk directly returns the icon with the link-overlay arrow baked in, so we
/// extract from the link's icon location (or its target) instead.
pub fn extract_icon_base64(path: &str) -> Option<String> {
    ensure_com_initialized();

    if path.to_lowercase().ends_with(".lnk") {
        if let Some(icon) = extract_from_shortcut(path) {
            return Some(icon);
        }
    }
    shell_icon_base64(path)
}

/// Resolve a .lnk and pull the icon from its icon location or target —
/// both are overlay-free.
fn extract_from_shortcut(path: &str) -> Option<String> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = link.cast().ok()?;
        let wide = to_wide(path);
        persist.Load(PCWSTR(wide.as_ptr()), STGM_READ).ok()?;

        // An explicit icon location on the shortcut takes precedence
        let mut icon_path = [0u16; 260];
        let mut icon_index = 0i32;
        if link
            .GetIconLocation(&mut icon_path, &mut icon_index)
            .is_ok()
        {
            let location = expand_env(&from_wide(&icon_path));
            if !location.is_empty() {
                if let Some(icon) = extract_icon_from_file(&location, icon_index) {
                    return Some(icon);
                }
            }
        }

        // Otherwise use the resolved target's own icon
        let mut target = [0u16; 260];
        if link
            .GetPath(&mut target, std::ptr::null_mut(), 0)
            .is_ok()
        {
            let target_path = from_wide(&target);
            if !target_path.is_empty() {
                return shell_icon_base64(&target_path);
            }
        }
    }
    None
}

fn expand_env(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let wide = to_wide(s);
    let mut buf = [0u16; 512];
    let written = unsafe { ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), Some(&mut buf)) };
    if written == 0 {
        s.to_string()
    } else {
        from_wide(&buf)
    }
}

/// Icon from an explicit "file, index" location (e.g. shell32.dll,42)
fn extract_icon_from_file(path: &str, index: i32) -> Option<String> {
    let wide = to_wide(path);
    unsafe {
        let mut hicon = HICON::default();
        let count = ExtractIconExW(PCWSTR(wide.as_ptr()), index, Some(&mut hicon), None, 1);
        if count == 0 || hicon.is_invalid() {
            return None;
        }
        let png = hicon_to_png_base64(hicon);
        let _ = DestroyIcon(hicon);
        png
    }
}

/// Plain shell icon of a path (no link overlay for non-.lnk files)
fn shell_icon_base64(path: &str) -> Option<String> {
    let wide = to_wide(path);
    let mut file_info = SHFILEINFOW::default();

    unsafe {
        let result = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut file_info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if result == 0 || file_info.hIcon.is_invalid() {
            return None;
        }
        let png = hicon_to_png_base64(file_info.hIcon);
        let _ = DestroyIcon(file_info.hIcon);
        png
    }
}

unsafe fn hicon_to_png_base64(hicon: HICON) -> Option<String> {
    let mut icon_info = ICONINFO::default();
    unsafe { GetIconInfo(hicon, &mut icon_info).ok()? };
    let color_bitmap = icon_info.hbmColor;
    let mask_bitmap = icon_info.hbmMask;

    let encoded = (|| unsafe {
        let mut bitmap = BITMAP::default();
        if GetObjectW(
            color_bitmap.into(),
            size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut _ as *mut _),
        ) == 0
        {
            return None;
        }
        let (width, height) = (bitmap.bmWidth, bitmap.bmHeight);
        if width <= 0 || height <= 0 || width > 512 || height > 512 {
            return None;
        }

        // Request 32bpp top-down BGRA
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let hdc = GetDC(None);
        let lines = GetDIBits(
            hdc,
            color_bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, hdc);
        if lines == 0 {
            return None;
        }

        // BGRA -> RGBA; legacy icons without an alpha channel become opaque
        let mut has_alpha = false;
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            if pixel[3] != 0 {
                has_alpha = true;
            }
        }
        if !has_alpha {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }

        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(&pixels).ok()?;
        }
        Some(base64::engine::general_purpose::STANDARD.encode(&png_bytes))
    })();

    unsafe {
        let _ = DeleteObject(color_bitmap.into());
        let _ = DeleteObject(mask_bitmap.into());
    }
    encoded
}

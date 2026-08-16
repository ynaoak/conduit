//! Capturing pixels on Windows.
//!
//! `BitBlt` from the screen device context into a DIB section: the same
//! path the color picker's `GetPixel` takes, one rectangle at a time
//! instead of one pixel. It reads what is composited on screen, so no
//! capture permission and no window enumeration are involved — and,
//! because a recording asks for the same rectangle ten times a second,
//! it has to stay cheap. A DIB section hands the bits back as memory we
//! can read directly rather than through a second copy.

use std::ffi::c_void;

use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HGDIOBJ,
    ROP_CODE, SRCCOPY,
};

use super::{Rect, Shot};

/// Layered windows (the launcher is one) are left out of a plain BitBlt,
/// which would punch a hole in any screenshot taken while something of
/// ours is on screen.
const COPY: ROP_CODE = ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0);

pub fn capture(area: Rect) -> Result<Shot, String> {
    let width = area.width as i32;
    let height = area.height as i32;

    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err("failed to open the screen device context".into());
        }
        // Everything below has to release the two DCs and the bitmap on
        // every exit, so the body is a closure and the cleanup is here.
        let result = (|| {
            let memory = CreateCompatibleDC(Some(screen));
            if memory.is_invalid() {
                return Err("failed to create a capture device context".to_string());
            }

            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    // Negative: a top-down DIB, so row 0 is the top row.
                    // A bottom-up one would come back mirrored.
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| format!("failed to allocate the capture bitmap: {}", e));

            let outcome = match bitmap {
                Err(e) => Err(e),
                Ok(bitmap) => {
                    let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
                    let blit = BitBlt(memory, 0, 0, width, height, Some(screen), area.x, area.y, COPY)
                        .map_err(|e| format!("failed to read the screen: {}", e));
                    let shot = blit.and_then(|()| {
                        if bits.is_null() {
                            return Err("the capture bitmap has no pixels".into());
                        }
                        // GDI writes BGRA, and the alpha byte of a screen
                        // copy is whatever happened to be in the source —
                        // usually zero, which would make a fully
                        // transparent PNG.
                        let length = (area.width as usize) * (area.height as usize) * 4;
                        let bgra = std::slice::from_raw_parts(bits as *const u8, length);
                        let mut rgba = Vec::with_capacity(length);
                        for pixel in bgra.chunks_exact(4) {
                            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
                        }
                        Ok(Shot::new(area.width, area.height, rgba))
                    });
                    SelectObject(memory, previous);
                    let _ = DeleteObject(HGDIOBJ(bitmap.0));
                    shot
                }
            };
            let _ = DeleteDC(memory);
            outcome
        })();

        ReleaseDC(None, screen);
        result
    }
}

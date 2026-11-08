//! The Windows half of a pick session.
//!
//! Windows ships no system color sampler, so the session is assembled here:
//! a low-level mouse hook takes the next click before the window under the
//! pointer sees it, and `GetPixel` reads the screen device context, which
//! reports whatever is actually composited on screen — no capture
//! permission, no window enumeration.

use std::sync::Mutex;

use tokio::sync::oneshot;
use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetPixel,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLR_INVALID, DIB_RGB_COLORS,
    HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, GetMessageW, PostQuitMessage, SetWindowsHookExW, UnhookWindowsHookEx,
    HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONDOWN, WM_SYSKEYDOWN,
};

use super::Outcome;
use crate::color::Rgb;

const VK_ESCAPE: u32 = 0x1B;

/// The sender for the session currently in flight, if any.
///
/// A hook procedure is a bare `extern "system" fn`, so the only way to reach
/// the awaiting task is through a static. `Option` doubles as the "is a
/// session running" flag the hooks check before swallowing anyone's click.
static SESSION: Mutex<Option<oneshot::Sender<Outcome>>> = Mutex::new(None);

/// Finish the session with `outcome` and stop the hook thread. Returns false
/// if no session was running, in which case the hooks must not interfere.
fn finish(outcome: Outcome) -> bool {
    let Ok(mut guard) = SESSION.lock() else {
        return false;
    };
    let Some(sender) = guard.take() else {
        return false;
    };
    // The receiver is gone if the caller stopped waiting; the session still
    // has to end, so the result is dropped rather than propagated.
    let _ = sender.send(outcome);
    unsafe { PostQuitMessage(0) };
    true
}

fn session_is_running() -> bool {
    SESSION.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// Swallowing the click matters as much as reading the pixel: without it,
/// picking a color out of another app also presses whatever was under the
/// pointer. Button-up is swallowed too, or the app underneath sees a
/// release it never got a press for.
unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && session_is_running() {
        match wparam.0 as u32 {
            WM_LBUTTONDOWN => {
                let point = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) }.pt;
                match pixel_at(point.x, point.y) {
                    Some(color) => finish(Outcome::Picked(color)),
                    // A pixel that cannot be read is not worth reporting as
                    // a color; ending as cancelled keeps the app honest.
                    None => finish(Outcome::Cancelled),
                };
                return LRESULT(1);
            }
            WM_LBUTTONUP => return LRESULT(1),
            WM_RBUTTONDOWN => {
                finish(Outcome::Cancelled);
                return LRESULT(1);
            }
            _ => {}
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn key_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && session_is_running() {
        let message = wparam.0 as u32;
        if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
            let key = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) }.vkCode;
            if key == VK_ESCAPE {
                finish(Outcome::Cancelled);
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Read one pixel from the screen device context.
pub fn pixel_at(x: i32, y: i32) -> Option<Rgb> {
    unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            return None;
        }
        let value = GetPixel(dc, x, y);
        ReleaseDC(None, dc);
        // COLORREF is 0x00bbggrr, which is the reverse of how it reads
        let raw = value.0;
        if raw == CLR_INVALID {
            return None;
        }
        Some(Rgb::new(
            (raw & 0xff) as u8,
            ((raw >> 8) & 0xff) as u8,
            ((raw >> 16) & 0xff) as u8,
        ))
    }
}

/// Read a square of pixels for the magnifier in one blit.
///
/// `GetPixel` reads the screen device context a pixel at a time, and each
/// of those is a readback that costs on the order of a millisecond. Two
/// hundred of them per frame is what turns a magnifier into a slideshow —
/// so the region is copied into a memory bitmap once, and read out of
/// ordinary memory afterwards.
///
/// The blit is a plain `SRCCOPY`: `CAPTUREBLT` would also pick up layered
/// windows, but it makes the screen flicker on some machines, which is not
/// a trade worth making sixty times a second.
///
/// Pixels off the edge of the desktop come back black; the magnifier draws
/// them as-is, which is what a user sees aiming at a screen corner.
pub fn capture_square(center_x: i32, center_y: i32, size: u32) -> Vec<Rgb> {
    let size = size.max(1) as i32;
    let half = size / 2;
    let count = (size * size) as usize;

    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Vec::new();
        }
        let memory = CreateCompatibleDC(Some(screen));
        if memory.is_invalid() {
            ReleaseDC(None, screen);
            return Vec::new();
        }

        // A negative height asks for a top-down bitmap, so the rows arrive
        // in the order they are read rather than bottom-up.
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap = CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0);
        let Ok(bitmap) = bitmap else {
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            return Vec::new();
        };

        let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
        let copied = BitBlt(
            memory,
            0,
            0,
            size,
            size,
            Some(screen),
            center_x - half,
            center_y - half,
            SRCCOPY,
        )
        .is_ok();

        let mut pixels = Vec::with_capacity(count);
        if copied && !bits.is_null() {
            // 32-bit DIB rows are BGRX and already 4-byte aligned, so the
            // whole square is one contiguous run.
            let raw = std::slice::from_raw_parts(bits as *const u8, count * 4);
            for pixel in raw.chunks_exact(4) {
                pixels.push(Rgb::new(pixel[2], pixel[1], pixel[0]));
            }
        }

        SelectObject(memory, previous);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        pixels
    }
}

pub fn cursor_position() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some((point.x, point.y))
}

/// Install the hooks and wait for the click.
///
/// Low-level hooks are delivered to the thread that installed them, and only
/// while that thread pumps messages — so the session owns a thread for its
/// lifetime and takes it down again on the way out. Leaving a hook installed
/// would keep swallowing clicks system-wide.
pub async fn pick() -> Result<Outcome, String> {
    let (sender, receiver) = oneshot::channel();
    {
        let mut guard = SESSION.lock().map_err(|_| "color picker state is poisoned")?;
        if guard.is_some() {
            return Err("a color pick is already in progress".into());
        }
        *guard = Some(sender);
    }

    std::thread::spawn(|| unsafe {
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), None, 0);
        let keys = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_hook), None, 0);
        match (&mouse, &keys) {
            (Ok(_), Ok(_)) => {
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).as_bool() {}
            }
            _ => {
                // Without both hooks the session cannot end on its own
                finish(Outcome::Cancelled);
            }
        }
        for hook in [mouse, keys].into_iter().flatten() {
            let _ = UnhookWindowsHookEx(HHOOK(hook.0));
        }
    });

    receiver.await.map_err(|_| {
        // The sender is only dropped without sending if the thread died
        SESSION.lock().ok().and_then(|mut g| g.take());
        "the color pick ended without a result".to_string()
    })
}

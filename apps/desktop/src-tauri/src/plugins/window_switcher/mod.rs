use async_trait::async_trait;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

const MAX_RESULTS: usize = 15;

#[derive(Clone)]
struct OpenWindow {
    hwnd: isize,
    title: String,
}

pub struct WindowSwitcherPlugin {
    manifest: PluginManifest,
    matcher: SkimMatcherV2,
}

impl WindowSwitcherPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest {
                id: "conduit.window-switcher".into(),
                name: "Window Switcher".into(),
                description: "Switch to open windows".into(),
                icon: "window".into(),
                keyword: Some("w".into()),
                keyword_only: false,
            },
            matcher: SkimMatcherV2::default(),
        }
    }

    fn window_to_result(win: &OpenWindow, score: f64, match_indices: Vec<usize>) -> SearchResult {
        SearchResult {
            id: format!("conduit.window-switcher:{}", win.hwnd),
            plugin_id: "conduit.window-switcher".into(),
            title: win.title.clone(),
            subtitle: Some(crate::i18n::t("plugins", "window_subtitle").into()),
            icon: ResultIcon::Named("select_window".into()),
            score,
            actions: vec![
                Action {
                    id: "focus".into(),
                    title: crate::i18n::t("plugins", "switch").into(),
                    shortcut: Some("Enter".into()),
                },
                Action {
                    id: "close".into(),
                    title: crate::i18n::t("plugins", "close_window").into(),
                    shortcut: None,
                },
            ],
            match_indices,
        }
    }
}

#[cfg(windows)]
mod win32 {
    use super::OpenWindow;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow,
        GWL_EXSTYLE, SW_RESTORE, WS_EX_TOOLWINDOW,
    };

    /// Enumerate switchable top-level windows (visible, titled, not a tool
    /// window, not DWM-cloaked, not our own process)
    pub fn list_windows() -> Vec<OpenWindow> {
        let mut windows: Vec<OpenWindow> = Vec::new();

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let windows = unsafe { &mut *(lparam.0 as *mut Vec<OpenWindow>) };

            unsafe {
                if !IsWindowVisible(hwnd).as_bool() {
                    return BOOL(1);
                }

                // Skip tool windows (floating palettes etc.)
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
                    return BOOL(1);
                }

                // Skip DWM-cloaked windows (suspended UWP apps, other desktops)
                let mut cloaked: u32 = 0;
                let _ = DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_CLOAKED,
                    &mut cloaked as *mut u32 as *mut _,
                    std::mem::size_of::<u32>() as u32,
                );
                if cloaked != 0 {
                    return BOOL(1);
                }

                // Skip our own windows (the launcher itself)
                let mut pid: u32 = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == std::process::id() {
                    return BOOL(1);
                }

                let len = GetWindowTextLengthW(hwnd);
                if len == 0 {
                    return BOOL(1);
                }
                let mut buf = vec![0u16; len as usize + 1];
                let copied = GetWindowTextW(hwnd, &mut buf);
                if copied > 0 {
                    windows.push(OpenWindow {
                        hwnd: hwnd.0 as isize,
                        title: String::from_utf16_lossy(&buf[..copied as usize]),
                    });
                }
            }
            BOOL(1)
        }

        unsafe {
            let _ = EnumWindows(
                Some(enum_proc),
                LPARAM(&mut windows as *mut Vec<OpenWindow> as isize),
            );
        }
        windows
    }

    pub fn focus_window(hwnd: isize) -> anyhow::Result<()> {
        unsafe {
            let hwnd = HWND(hwnd as *mut _);
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            if !SetForegroundWindow(hwnd).as_bool() {
                anyhow::bail!("failed to focus window");
            }
        }
        Ok(())
    }

    pub fn close_window(hwnd: isize) -> anyhow::Result<()> {
        use windows::Win32::Foundation::WPARAM;
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
        unsafe {
            let hwnd = HWND(hwnd as *mut _);
            PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0))
                .map_err(|e| anyhow::anyhow!("failed to close window: {}", e))?;
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod win32 {
    use super::OpenWindow;

    pub fn list_windows() -> Vec<OpenWindow> {
        Vec::new()
    }

    pub fn focus_window(_hwnd: isize) -> anyhow::Result<()> {
        anyhow::bail!("window switching is only supported on Windows")
    }

    pub fn close_window(_hwnd: isize) -> anyhow::Result<()> {
        anyhow::bail!("window switching is only supported on Windows")
    }
}

#[async_trait]
impl ConduitPlugin for WindowSwitcherPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let query = query.trim().to_string();
        // Enumeration is cheap (~ms); do it fresh each search
        let windows = tokio::task::spawn_blocking(win32::list_windows)
            .await
            .unwrap_or_default();

        if query.is_empty() {
            // Bare "w" keyword or the default (empty query) dashboard view
            return windows
                .iter()
                .take(MAX_RESULTS)
                .enumerate()
                .map(|(i, w)| Self::window_to_result(w, 0.65 - i as f64 * 0.03, vec![]))
                .collect();
        }

        let mut results: Vec<SearchResult> = windows
            .iter()
            .filter_map(|w| {
                self.matcher
                    .fuzzy_indices(&w.title, &query)
                    .map(|(score, indices)| {
                        let normalized = (score as f64 / 100.0).min(0.88).max(0.0);
                        Self::window_to_result(w, normalized, indices)
                    })
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(MAX_RESULTS);
        results
    }

    async fn execute(&self, result_id: &str, action_id: &str) -> anyhow::Result<()> {
        let hwnd: isize = result_id
            .strip_prefix("conduit.window-switcher:")
            .unwrap_or(result_id)
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid window id: {}", result_id))?;

        match action_id {
            "close" => win32::close_window(hwnd),
            _ => win32::focus_window(hwnd),
        }
    }
}

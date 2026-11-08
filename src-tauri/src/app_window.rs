//! BloogBase's own window in fullscreen: switched from the sidebar, and on
//! start when the settings page says so. While it is in fullscreen, games
//! start in fullscreen too (see [`dosconf::launch_conf`](crate::dosconf::launch_conf)).
//! On Windows its title bar wears the app's theme.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

use crate::db::Db;
use crate::error::Result;
use crate::settings;

/// Whether BloogBase starts in fullscreen; off unless the player turned it on.
const START_FULLSCREEN: &str = "app.startFullscreen";

pub fn start_fullscreen(db: &Db) -> Result<bool> {
    Ok(settings::get(db, START_FULLSCREEN)?.is_some_and(|v| v == "1"))
}

pub fn set_start_fullscreen(db: &Db, on: bool) -> Result<()> {
    settings::set(db, START_FULLSCREEN, if on { "1" } else { "0" })
}

fn main_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window("main")
}

/// Whether the main window is in fullscreen now.
pub fn is_fullscreen<R: Runtime>(app: &AppHandle<R>) -> bool {
    main_window(app).is_some_and(|w| w.is_fullscreen().unwrap_or(false))
}

pub fn set_fullscreen<R: Runtime>(app: &AppHandle<R>, on: bool) -> Result<()> {
    if let Some(window) = main_window(app) {
        window.set_fullscreen(on)?;
    }
    Ok(())
}

/// Back to the window before BloogBase quits, so the window state plugin
/// keeps the window's own size and position rather than the screen's.
pub fn leave_fullscreen<R: Runtime>(app: &AppHandle<R>) {
    if is_fullscreen(app)
        && let Err(e) = set_fullscreen(app, false)
    {
        tracing::warn!("could not leave fullscreen: {e}");
    }
}

/// Whether the app shows its light theme; the title bar follows it.
static LIGHT: AtomicBool = AtomicBool::new(false);

/// Title bar colours from DESIGN.md, as 0xRRGGBB. Only Windows takes them.
#[cfg(windows)]
struct TitleBar {
    caption: u32,
    text: u32,
    inactive_text: u32,
    border: u32,
}

#[cfg(windows)]
const NIGHT_TITLE_BAR: TitleBar = TitleBar {
    caption: 0x181631,       // night-surface, as the sidebar
    text: 0xf2f0ff,          // starlight
    inactive_text: 0xa19dc4, // dusk-muted
    border: 0x34305e,        // night-border
};

#[cfg(windows)]
const DAY_TITLE_BAR: TitleBar = TitleBar {
    caption: 0xcfc6ad,       // day-sidebar
    text: 0x221e1a,          // day-ink
    inactive_text: 0x544c3f, // day-muted
    border: 0xb2a68a,        // day-border
};

/// Windows wants colours as COLORREF, 0x00BBGGRR.
#[cfg_attr(not(windows), allow(dead_code))]
fn colorref(rgb: u32) -> u32 {
    ((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff)
}

/// Colours the main window's title bar in the app's theme rather than
/// Windows' (light/dark mode and accent colour). Windows 11 takes the colours;
/// Windows 10 only light or dark. Nothing to do on other systems.
pub fn set_title_bar<R: Runtime>(app: &AppHandle<R>, light: bool) {
    LIGHT.store(light, Ordering::Relaxed);
    if let Some(window) = main_window(app) {
        let focused = window.is_focused().unwrap_or(true);
        paint_title_bar(&window, focused);
    }
}

/// Dims the title text while another window is active, as Windows does.
pub fn title_bar_focus<R: Runtime>(window: &tauri::Window<R>, focused: bool) {
    if window.label() == "main"
        && let Some(window) = main_window(window.app_handle())
    {
        paint_title_bar(&window, focused);
    }
}

#[cfg(windows)]
fn paint_title_bar<R: Runtime>(window: &WebviewWindow<R>, focused: bool) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
        DwmSetWindowAttribute,
    };
    /// DWMWA_USE_IMMERSIVE_DARK_MODE before Windows 10 20H1.
    const OLD_DARK_MODE: i32 = 19;

    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = hwnd.0;
    let light = LIGHT.load(Ordering::Relaxed);
    let colors = if light {
        &DAY_TITLE_BAR
    } else {
        &NIGHT_TITLE_BAR
    };
    let text = if focused {
        colors.text
    } else {
        colors.inactive_text
    };
    let set = |attribute: i32, value: u32| {
        // SAFETY: a live window of this process and a 4-byte value.
        unsafe { DwmSetWindowAttribute(hwnd, attribute as u32, (&raw const value).cast(), 4) }
    };
    // Light or dark buttons and frame, also on Windows 10.
    let dark = u32::from(!light);
    if set(DWMWA_USE_IMMERSIVE_DARK_MODE, dark) != 0 {
        set(OLD_DARK_MODE, dark);
    }
    // Windows 11 only; Windows 10 turns these down, which is fine.
    set(DWMWA_CAPTION_COLOR, colorref(colors.caption));
    set(DWMWA_TEXT_COLOR, colorref(text));
    set(DWMWA_BORDER_COLOR, colorref(colors.border));
}

#[cfg(not(windows))]
fn paint_title_bar<R: Runtime>(_window: &WebviewWindow<R>, _focused: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorref_swaps_red_and_blue() {
        assert_eq!(colorref(0x0d0c1d), 0x1d0c0d);
        assert_eq!(colorref(0xd9d1bc), 0xbcd1d9);
    }

    #[test]
    fn start_fullscreen_is_off_until_turned_on() {
        let db = Db::open_in_memory().unwrap();
        assert!(!start_fullscreen(&db).unwrap());
        set_start_fullscreen(&db, true).unwrap();
        assert!(start_fullscreen(&db).unwrap());
        set_start_fullscreen(&db, false).unwrap();
        assert!(!start_fullscreen(&db).unwrap());
    }
}

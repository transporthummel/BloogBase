//! BloogBase's own window in fullscreen: switched from the sidebar, and on
//! start when the settings page says so. While it is in fullscreen, games
//! start in fullscreen too (see [`dosconf::launch_conf`](crate::dosconf::launch_conf)).

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

#[cfg(test)]
mod tests {
    use super::*;

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

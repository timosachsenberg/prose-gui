//! The window and the system around it: the menu's state, the page's answer to a close request, links
//! opened in the browser.

use super::*;
use crate::menu;
use tauri::{Manager, State};

/// Opens `url` (https only: the Help page's links) in the system's browser.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") || url.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err(format!("{url} is not an https link."));
    }
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "linux")]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    cmd.arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("The browser cannot be opened ({e})."))
}

/// Which menu items apply (`menu`): Export with an open result, its CSV and Excel with a table, the theme's mark.
#[tauri::command]
pub fn menu_state(app: AppHandle, state: menu::MenuState) -> Result<(), String> {
    match app.try_state::<menu::MenuHandles<tauri::Wry>>() {
        Some(handles) => handles.apply(&state).map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

/// The page's answer to `app://close-requested`: close (the running search ends, as with any close), or
/// keep the window.
#[tauri::command]
pub fn answer_close(
    window: tauri::Window,
    state: State<'_, AppState>,
    close: bool,
) -> Result<(), String> {
    {
        let mut closing = state.closing.lock().unwrap();
        closing.asked = false;
        closing.confirmed = close;
    }
    if close {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

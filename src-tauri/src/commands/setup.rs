//! Search setups (`.prose-setup.json`): saved, opened (also from File › Open Recent) and the window's
//! current one, whose unsaved changes make closing the window ask first.

use super::*;
use crate::{menu, recent_dir, setup_file};
use tauri::{Manager, State};
use tauri_plugin_dialog::{MessageDialogButtons, MessageDialogKind};

/// The setup file `path` is the current one: Save writes there again, it is first under Open Recent, and the
/// window's title names it.
fn use_setup_path(app: &AppHandle, state: &AppState, path: &Path) {
    *state.setup_path.lock().unwrap() = Some(path.to_path_buf());
    if let Some(dir) = recent_dir(app) {
        let list = setup_file::remember(&dir, path);
        if let Some(handles) = app.try_state::<menu::MenuHandles<tauri::Wry>>() {
            let _ = handles.set_recent(app, list);
        }
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title(&format!("{} — ProSE", setup_file::setup_stem(&name)));
    }
}

/// Saves a setup (`contents`, the page's JSON): to the current setup file when `ask` is false and there is
/// one, else where the user chooses in the save dialog (suggesting `name`). -> `{ path, name }`, or null if
/// cancelled.
#[tauri::command]
pub async fn save_setup_file(
    app: AppHandle,
    name: String,
    contents: String,
    ask: bool,
) -> Result<Option<serde_json::Value>, String> {
    let handle = app.clone();
    let written = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let current = if ask {
            None
        } else {
            state.setup_path.lock().unwrap().clone()
        };
        let target = match current {
            Some(path) => path,
            None => {
                let suggested = Path::new(&name)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let suggested = setup_file::with_setup_extension(Path::new(&suggested));
                let suggested = suggested.to_string_lossy().into_owned();
                let chosen = match test_dialog_path() {
                    Some(path) => Some(path),
                    None => match handle
                        .dialog()
                        .file()
                        .set_title("Save the search setup: its files, database and settings, and the views (no results)")
                        .set_file_name(&suggested)
                        .add_filter("ProSE setup", &["json"])
                        .blocking_save_file()
                    {
                        None => None,
                        Some(p) => Some(p.into_path().map_err(|e| e.to_string())?),
                    },
                };
                let Some(path) = chosen else { return Ok(None) };
                // the file gets the setup's extension: one of that name the dialog did not ask about is not
                // replaced without asking (`exp.json` chosen, `exp.prose-setup.json` there)
                let written = setup_file::with_setup_extension(&path);
                if written != path
                    && written.exists()
                    && test_dialog_path().is_none()
                    && !handle
                        .dialog()
                        .message(format!(
                            "{} already exists. Replace it with this setup?",
                            written.display()
                        ))
                        .title("Replace the setup?")
                        .kind(MessageDialogKind::Warning)
                        .buttons(MessageDialogButtons::OkCancelCustom(
                            "Replace".to_string(),
                            "Cancel".to_string(),
                        ))
                        .blocking_show()
                {
                    return Ok(None);
                }
                path
            }
        };
        setup_file::write_setup(&target, &contents).map(Some)
    })
    .await
    .map_err(|e| e.to_string())??;
    let Some(path) = written else { return Ok(None) };
    use_setup_path(&app, &app.state::<AppState>(), &path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Some(serde_json::json!({ "path": path, "name": name })))
}

/// Opens a setup file: the open dialog, or with `recent` the `recent`-th entry of File › Open Recent (the page
/// never names a path). -> `{ path, text }`, or null if cancelled. A recent entry that cannot be read (moved,
/// deleted) is taken off Open Recent, and the error says so.
#[tauri::command]
pub async fn open_setup_file(
    app: AppHandle,
    recent: Option<usize>,
) -> Result<Option<serde_json::Value>, String> {
    let from_recent = match recent {
        None => None,
        Some(n) => {
            let handles = app.try_state::<menu::MenuHandles<tauri::Wry>>();
            let listed = handles.and_then(|h| h.recent_paths.lock().unwrap().get(n).cloned());
            let listed = listed
                .or_else(|| recent_dir(&app).and_then(|d| setup_file::recent(&d).get(n).cloned()));
            Some(listed.ok_or("That recent setup is no longer listed.")?)
        }
    };
    let recent_path = from_recent.clone();
    let handle = app.clone();
    let read = tauri::async_runtime::spawn_blocking(move || {
        let chosen = match from_recent.or_else(test_dialog_path) {
            Some(path) => Some(path),
            None => match handle
                .dialog()
                .file()
                .set_title("Open a setup (.prose-setup.json)")
                .add_filter("ProSE setup", &["json"])
                .blocking_pick_file()
            {
                None => None,
                Some(p) => Some(p.into_path().map_err(|e| e.to_string())?),
            },
        };
        let Some(path) = chosen else { return Ok(None) };
        let text = setup_file::read_setup(&path)?;
        Ok::<_, String>(Some((path, text)))
    })
    .await
    .map_err(|e| e.to_string())?;
    let read = match (read, recent_path) {
        (Ok(read), _) => read,
        (Err(e), None) => return Err(e),
        (Err(e), Some(gone)) => {
            if let Some(dir) = recent_dir(&app) {
                let list = setup_file::forget(&dir, &gone);
                if let Some(handles) = app.try_state::<menu::MenuHandles<tauri::Wry>>() {
                    let _ = handles.set_recent(&app, list);
                }
            }
            return Err(format!("{e} It is no longer listed under Open Recent."));
        }
    };
    let Some((path, text)) = read else {
        return Ok(None);
    };
    // (the setup's file once the page has read it as a setup: `use_opened_setup_file`)
    *app.state::<AppState>().opened_setup.lock().unwrap() = Some(path.clone());
    Ok(Some(serde_json::json!({ "path": path, "text": text })))
}

/// The setup file Open Setup read last is the window's setup file, now that the page has read it as one: Save
/// writes there, it is first under Open Recent, and the window's title names it.
#[tauri::command]
pub fn use_opened_setup_file(app: AppHandle, state: State<'_, AppState>) {
    let opened = state.opened_setup.lock().unwrap().take();
    if let Some(path) = opened {
        use_setup_path(&app, &state, &path);
    }
}

/// File › New Search: no current setup any more (the next Save asks where), and the window's title is the app's.
#[tauri::command]
pub fn forget_setup_file(app: AppHandle, state: State<'_, AppState>) {
    *state.setup_path.lock().unwrap() = None;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title("ProSE");
    }
}

/// The page's setup has changes not saved to its file (`unsaved`), or none: closing the window asks first while it
/// has ([`close_requested`](crate::close_requested)).
#[tauri::command]
pub fn set_setup_unsaved(state: State<'_, AppState>, unsaved: bool) {
    state
        .setup_unsaved
        .store(unsaved, std::sync::atomic::Ordering::SeqCst);
}

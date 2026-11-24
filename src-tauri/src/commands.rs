//! The commands the front end invokes (`invoke`), by what they serve; [`crate::run()`] registers them. The
//! list of them with what they take and return is the crate's documentation (`lib.rs`). Below: what
//! several of them need (the searches folder, the system's dialogs or the end-to-end specs' stand-ins for
//! them).

pub mod databases;
pub mod exports;
pub mod history;
pub mod location;
pub mod search;
pub mod setup;
pub mod window;

use crate::location::Locations;
use crate::{data_dir, AppState};
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// The searches folder new searches go to (`location`).
fn searches_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(Locations::load(&data_dir(app)?).current)
}

/// The file that the end-to-end specs choose in place of the system's dialogs (`PROSE_TEST_DIALOG_PATH`):
/// WebDriver cannot reach a dialog of the system. Debug builds only; a release build has no such way
/// (this is `None` there), and the page never names a path in either.
#[cfg(debug_assertions)]
fn test_dialog_path() -> Option<PathBuf> {
    std::env::var_os("PROSE_TEST_DIALOG_PATH")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

#[cfg(not(debug_assertions))]
fn test_dialog_path() -> Option<PathBuf> {
    None
}

/// The folder that the end-to-end specs choose in place of the system's folder dialog of the searches folder
/// (`PROSE_TEST_FOLDER_DIALOG_PATH`). Debug builds only, as [`test_dialog_path`].
#[cfg(debug_assertions)]
fn test_folder_dialog_path() -> Option<PathBuf> {
    std::env::var_os("PROSE_TEST_FOLDER_DIALOG_PATH")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

#[cfg(not(debug_assertions))]
fn test_folder_dialog_path() -> Option<PathBuf> {
    None
}

/// The system's save dialog for `name`; None if the user cancelled.
fn ask_save_path(app: &AppHandle, name: &str) -> Result<Option<PathBuf>, String> {
    ask_save_path_titled(app, name, None)
}

/// The same, with the dialog's title.
fn ask_save_path_titled(
    app: &AppHandle,
    name: &str,
    title: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    if let Some(path) = test_dialog_path() {
        return Ok(Some(path));
    }
    let extension = Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().into_owned());
    let mut dialog = app.dialog().file().set_file_name(name);
    if let Some(title) = title {
        dialog = dialog.set_title(title);
    }
    if let Some(ext) = extension.as_deref() {
        dialog = dialog.add_filter(ext.to_uppercase(), &[ext]);
    }
    match dialog.blocking_save_file() {
        None => Ok(None),
        Some(path) => path.into_path().map(Some).map_err(|e| e.to_string()),
    }
}

/// Where the searches are (`location`) and the blocking work with it, off the async runtime's threads.
async fn on_locations<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(Locations) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let data = data_dir(app)?;
    tauri::async_runtime::spawn_blocking(move || work(Locations::load(&data)))
        .await
        .map_err(|e| e.to_string())?
}

/// The searches folder new searches and opened results archives go to, and the blocking work on it.
async fn on_searches<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(PathBuf) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    on_locations(app, |locations| work(locations.current)).await
}

/// The searches folder that holds search `id` (`Locations::root_of`), and the blocking work on it.
async fn on_search<T: Send + 'static>(
    app: &AppHandle,
    id: &str,
    work: impl FnOnce(PathBuf) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let id = id.to_string();
    on_locations(app, move |locations| work(locations.root_of(&id))).await
}

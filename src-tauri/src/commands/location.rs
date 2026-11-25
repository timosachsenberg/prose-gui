//! Where the searches are: the searches folder new searches go to, chosen by the user or the app's own.

use super::*;
use crate::location;

/// Where new searches go (`location`).
#[tauri::command]
pub async fn searches_location(app: AppHandle) -> Result<location::Location, String> {
    on_locations(&app, |locations| Ok(locations.location())).await
}

/// The title of the folder dialog of the searches folder: what the folder is for.
pub const SEARCHES_DIALOG_TITLE: &str =
    "Choose a folder for ProSE's searches (an empty one, or one with ProSE's searches)";

/// The system's folder dialog (in Rust: the page never names a path), then the folder chosen for new searches and
/// opened results archives, checked (`location::check_folder`) and kept. -> the new location, or null if cancelled.
#[tauri::command]
pub async fn choose_searches_folder(app: AppHandle) -> Result<Option<location::Location>, String> {
    let data = data_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let chosen = match test_folder_dialog_path() {
            Some(path) => Some(path),
            None => {
                let current = Locations::load(&data).current;
                let mut dialog = app.dialog().file().set_title(SEARCHES_DIALOG_TITLE);
                // (the dialog starts beside the current folder, where a new one is easily made)
                if let Some(parent) = current.parent().filter(|p| p.is_dir()) {
                    dialog = dialog.set_directory(parent);
                }
                match dialog.blocking_pick_folder() {
                    None => None,
                    Some(path) => Some(path.into_path().map_err(|e| e.to_string())?),
                }
            }
        };
        let Some(folder) = chosen else {
            return Ok(None);
        };
        location::choose(&data, &folder).map(|locations| Some(locations.location()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// New searches go to the app's own searches folder again (those of the folder chosen stay in History).
#[tauri::command]
pub async fn use_default_searches_folder(app: AppHandle) -> Result<location::Location, String> {
    let data = data_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        location::use_default(&data).map(|locations| locations.location())
    })
    .await
    .map_err(|e| e.to_string())?
}

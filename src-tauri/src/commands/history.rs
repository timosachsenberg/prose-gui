//! History: the searches kept, their records, names and curation, results archives (saved and opened)
//! and QPX results folders opened as searches.

use super::*;
use crate::{archive, history};
use serde::Serialize;
use tauri::Emitter;

/// The searches of all the folders the app keeps them in (`location`), the newest first; a search of one id once (as
/// the commands find it: in the first folder).
#[tauri::command]
pub async fn history_list(app: AppHandle) -> Result<Vec<history::HistoryEntry>, String> {
    on_locations(&app, |locations| {
        let mut seen = std::collections::HashSet::new();
        let mut entries = Vec::new();
        for root in locations.roots() {
            for entry in history::list_in(&root, locations.is_own(&root)) {
                if seen.insert(entry.id.clone()) {
                    entries.push(entry);
                }
            }
        }
        history::sort_entries(&mut entries);
        Ok(entries)
    })
    .await
}

#[tauri::command]
pub async fn history_save_record(
    app: AppHandle,
    search_id: String,
    record: String,
) -> Result<(), String> {
    on_search(&app, &search_id.clone(), move |dir| {
        history::save_record(&dir, &search_id, &record)
    })
    .await
}

#[tauri::command]
pub async fn history_save_proteins(
    app: AppHandle,
    search_id: String,
    proteins: String,
) -> Result<(), String> {
    on_search(&app, &search_id.clone(), move |dir| {
        history::save_proteins(&dir, &search_id, &proteins)
    })
    .await
}

#[tauri::command]
pub async fn save_curation(
    app: AppHandle,
    search_id: String,
    curation: String,
) -> Result<(), String> {
    on_search(&app, &search_id.clone(), move |dir| {
        history::save_curation(&dir, &search_id, &curation)
    })
    .await
}

#[tauri::command]
pub async fn history_rename(app: AppHandle, search_id: String, name: String) -> Result<(), String> {
    on_search(&app, &search_id.clone(), move |dir| {
        history::rename(&dir, &search_id, &name)
    })
    .await
}

#[tauri::command]
pub async fn history_delete(app: AppHandle, search_id: String) -> Result<(), String> {
    on_locations(&app, move |locations| {
        let dir = locations.root_of(&search_id);
        // (in a folder the user chose, only a search: History lists nothing else of it)
        let folder = dir.join(&search_id);
        if !locations.is_own(&dir)
            && std::fs::symlink_metadata(&folder).is_ok()
            && !history::is_search_folder(&folder)
        {
            return Err(format!("{} is not a search of ProSE.", folder.display()));
        }
        history::delete(&dir, &search_id)
    })
    .await
}

#[tauri::command]
pub async fn history_files(
    app: AppHandle,
    search_id: String,
) -> Result<Vec<history::FolderFile>, String> {
    on_search(&app, &search_id.clone(), move |dir| {
        history::folder_files(&dir, &search_id)
    })
    .await
}

/// How far the opening of a results archive is (`archive://import-progress`): the bytes unpacked.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportProgress {
    written: u64,
    total: u64,
}

/// A results archive as a new search: the open dialog (in Rust: the page never names a path), then the file
/// the user chose unpacked entry by entry into a fresh search folder (`history::import_archive`: nothing of
/// it is held in memory), with `archive://import-progress` as it goes (at most once per percent). -> the new
/// id, or null if cancelled. The dialog offers `.prose` files first and any file second (what the file holds
/// is checked as it is read).
#[tauri::command]
pub async fn import_archive_file(app: AppHandle) -> Result<Option<String>, String> {
    let data = data_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = match test_dialog_path() {
            Some(path) => path,
            None => {
                let chosen = app
                    .dialog()
                    .file()
                    .add_filter("ProSE results archive", &["prose"])
                    .add_filter("All files", &["*"])
                    .set_title("Open a results archive")
                    .blocking_pick_file();
                match chosen {
                    None => return Ok(None),
                    Some(path) => path.into_path().map_err(|e| e.to_string())?,
                }
            }
        };
        let dir = Locations::load(&data).current;
        let mut shown = None;
        let mut progress = |written: u64, total: u64| {
            let percent = written.saturating_mul(100) / total.max(1);
            if shown != Some(percent) {
                shown = Some(percent);
                let _ = app.emit(
                    "archive://import-progress",
                    ImportProgress { written, total },
                );
            }
        };
        history::import_archive(&dir, &path, &mut progress).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A QPX results folder (ProSE run by hand or on a cluster): the folder dialog, then its runs' stems
/// (`history::qpx_stems`) -> `{ folder, stems }`, or null if cancelled.
#[tauri::command]
pub async fn open_qpx_folder(app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let chosen = match test_dialog_path() {
            Some(path) => Some(path),
            None => match app
                .dialog()
                .file()
                .set_title("Open a QPX results folder")
                .blocking_pick_folder()
            {
                None => None,
                Some(p) => Some(p.into_path().map_err(|e| e.to_string())?),
            },
        };
        let Some(folder) = chosen else {
            return Ok(None);
        };
        let stems = history::qpx_stems(&folder)?;
        // (the run and provenance views, small, for the record: PSMs, cutoffs, database)
        let qpx = history::qpx_dir(&folder).ok_or("The folder has no QPX results.")?;
        let read = |name: &str| std::fs::read(qpx.join(name)).map_err(|e| format!("{name}: {e}"));
        Ok(Some(serde_json::json!({
            "folder": folder,
            "stems": stems,
            "run": read("quantms.run.parquet")?,
            "provenance": read("quantms.provenance.parquet")?,
        })))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The QPX folder `folder` (from `open_qpx_folder`) as a new search, with the page's `record` and `inputs`
/// (search.json and inputs.json) -> the new id.
#[tauri::command]
pub async fn history_import_qpx(
    app: AppHandle,
    folder: String,
    record: String,
    inputs: String,
) -> Result<String, String> {
    on_searches(&app, move |dir| {
        history::import_qpx(&dir, Path::new(&folder), &record, &inputs)
    })
    .await
}

/// The title of the save dialog of a results archive: what it holds, beyond the results.
pub const ARCHIVE_DIALOG_TITLE: &str =
    "Save the results archive: the search's results, including folder paths of this computer";

/// How far the writing of a results archive is (`archive://progress`): the bytes of the search's files read.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveProgress {
    search_id: String,
    written: u64,
    total: u64,
}

/// A results archive of search `search_id`: what it would hold is checked first, then the system's save
/// dialog (suggesting `name`, a file name), then the zip written there entry by entry (`archive`), with
/// `archive://progress` as it goes (at most once per percent). -> the path, or null if cancelled.
#[tauri::command]
pub async fn save_archive_file(
    app: AppHandle,
    search_id: String,
    name: String,
) -> Result<Option<String>, String> {
    // (only a file name is suggested)
    let name = Path::new(&name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "prose.prose".to_string());
    let data = data_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let dir = Locations::load(&data).root_of(&search_id);
        archive::archive_files(&dir, &search_id)?;
        // (the path as chosen: the dialog's filter and the suggested name carry the extension; the title says
        // what an archive holds, as README.md does)
        let Some(path) = ask_save_path_titled(&app, &name, Some(ARCHIVE_DIALOG_TITLE))? else {
            return Ok(None);
        };
        let mut shown = None;
        let mut progress = |written: u64, total: u64| {
            let percent = written.saturating_mul(100) / total.max(1);
            if shown != Some(percent) {
                shown = Some(percent);
                let search_id = search_id.clone();
                let _ = app.emit(
                    "archive://progress",
                    ArchiveProgress {
                        search_id,
                        written,
                        total,
                    },
                );
            }
        };
        archive::write_archive(
            &dir,
            &search_id,
            &path,
            std::time::SystemTime::now(),
            &mut progress,
        )?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}

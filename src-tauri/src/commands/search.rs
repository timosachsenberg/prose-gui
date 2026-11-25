//! Searching: the tool, a search's start and cancel (its events: `search://…`), the files New search is
//! given, and a search's files for the results.

use super::*;
use crate::args::SearchRequest;
use crate::run::{SearchEvent, Sink};
use crate::{files, resource_dir, spectra, tool};
use serde::Serialize;
use std::sync::Arc;
use tauri::ipc::Response;
use tauri::{Emitter, Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResult {
    search_id: String,
}

/// Sends the shim's events to the front end.
fn event_sink(app: AppHandle) -> Sink {
    Arc::new(move |event| {
        let _ = match event {
            SearchEvent::Progress(p) => app.emit("search://progress", p),
            SearchEvent::Done(d) => app.emit("search://done", d),
            SearchEvent::Failed(f) => app.emit("search://failed", f),
        };
    })
}

#[tauri::command]
pub async fn tool_info(app: AppHandle) -> Result<tool::ToolInfo, String> {
    let resources = resource_dir(&app);
    tauri::async_runtime::spawn_blocking(move || tool::probe(&tool::resolve(resources.as_deref())?))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn start_search(
    app: AppHandle,
    state: State<'_, AppState>,
    request: SearchRequest,
) -> Result<StartResult, String> {
    state.closing.lock().unwrap().search_started();
    let resources = resource_dir(&app);
    let tool_path = tool::resolve(resources.as_deref())?;
    let searches_dir = searches_dir(&app)?;
    let crap = tool::bundled_crap(resources.as_deref());
    let searches = state.searches.clone();
    let sink = event_sink(app);
    let search_id = tauri::async_runtime::spawn_blocking(move || {
        searches.start(&tool_path, &searches_dir, crap.as_deref(), request, sink)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(StartResult { search_id })
}

/// Returns when the process is gone and the partial folder is removed.
#[tauri::command]
pub async fn cancel_search(state: State<'_, AppState>, search_id: String) -> Result<(), String> {
    let searches = state.searches.clone();
    tauri::async_runtime::spawn_blocking(move || searches.cancel(&search_id))
        .await
        .map_err(|e| e.to_string())?
}

/// A file of a search (see `files::read_search_file`), as raw bytes.
#[tauri::command]
pub async fn read_search_file(
    app: AppHandle,
    search_id: String,
    name: String,
    tail: Option<u64>,
    offset: Option<u64>,
    length: Option<u64>,
) -> Result<Response, files::ReadError> {
    let range = files::ByteRange::of(tail, offset, length)?;
    let data = data_dir(&app).map_err(|message| files::ReadError {
        not_found: false,
        message,
    })?;
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let dir = Locations::load(&data).root_of(&search_id);
        files::read_search_file(&dir, &search_id, &name, range)
    })
    .await
    .map_err(|e| files::ReadError {
        not_found: false,
        message: e.to_string(),
    })??;
    Ok(Response::new(bytes))
}

/// What New search shows of the files it was given (sizes, counts and what ProSE cannot search; never
/// contents).
#[tauri::command]
pub async fn inspect_files(paths: Vec<String>) -> Result<Vec<files::FileFacts>, String> {
    tauri::async_runtime::spawn_blocking(move || files::inspect_all(&paths))
        .await
        .map_err(|e| e.to_string())
}

/// What the first MS2 spectra of each mzML say (`spectra`, guidance G1), off the add-files path: New search asks
/// once the rows are there. None for a file that is no mzML (or cannot be read).
#[tauri::command]
pub async fn inspect_spectra(
    paths: Vec<String>,
) -> Result<Vec<Option<spectra::SpectraFacts>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths.iter().map(|p| spectra::inspect_cached(p)).collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// The cores ProSE's threads may use: the threads' default ("all"). The webview's own count is capped
/// (WebKit reports at most 8).
#[tauri::command]
pub fn cpu_count() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

#[tauri::command]
pub async fn example_files(app: AppHandle) -> Result<files::ExampleFiles, String> {
    let source = files::bundled_example(resource_dir(&app).as_deref())
        .ok_or("The example is not part of this installation.")?;
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || files::example_files(&source, &data))
        .await
        .map_err(|e| e.to_string())?
}

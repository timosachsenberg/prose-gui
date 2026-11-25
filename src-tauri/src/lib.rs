//! ProSE desktop: the Tauri shell around the ProSE tool of OpenMS.
//!
//! The shim (`tool`, `args`, `progress`, `run`) runs the tool for a search request and reports its
//! progress; the commands and events below are what the front end sees:
//!
//! - `tool_info` -> `{ version, path, onnx }`
//! - `start_search(request)` -> `{ searchId }`; `cancel_search(searchId)`
//! - events `search://progress` `{ searchId, label, file, fraction }`, `search://done`
//!   `{ searchId, folder }` and `search://failed` `{ searchId, message, stage, file?, ended?, detail?, log? }`
//! - `read_search_file(searchId, name, tail?, offset?, length?)` -> the bytes of a file of the search's folder
//!   (with `tail`: its last `tail` bytes; with `offset` and `length`: those bytes), or of the database its database.json records (`files`; rejects `{ notFound, message }`)
//! - `inspect_files(paths)` -> `[{ path, name, size, spectra?, proteins?, residues?, error?, problem?, warnings }]`
//! - `inspect_spectra(paths)` -> per path the facts of its first MS2 spectra, or null (`spectra`)
//! - `cpu_count()` -> the cores the search's threads may use
//! - `example_files()` -> `{ spectra: [path, path], database }`
//! - `save_text(name, contents)` and `save_search_file(searchId, name, suggested)`: a save dialog of
//!   the system, then the file written where the user chose (-> the path, or null if cancelled)
//! - History (`history`): `history_list()` -> entries (a folder with a broken record, or none, has
//!   `error`), `history_save_record(searchId, record)` (a finished search gets its `search.json`, right
//!   after `search://done`), `history_save_proteins(searchId, proteins)` (its `proteins.fasta`, once the
//!   results are read; the folder's `database.fasta` goes), `history_rename(searchId, name)`,
//!   `history_delete(searchId)`, `history_files(searchId)` (the regular files of the folder, for a results
//!   archive), `save_archive_file(searchId, name)` (a save dialog, then the search's archive zip written there
//!   entry by entry: `archive`; events `archive://progress` `{ searchId, written, total }`),
//!   `import_archive_file()` (the open dialog, then the archive chosen unpacked entry by entry into a fresh
//!   folder -> the new id, or null if cancelled; events `archive://import-progress` `{ written, total }`),
//!   `save_curation(searchId, curation)` (the user's marks of the PSMs: the folder's `curation.json`). At
//!   startup the partial folders of interrupted searches are swept (`history::sweep_in`).
//! - where the searches are (`location`): `searches_location()` -> `{ folder, isDefault, defaultFolder }` (where new
//!   searches and opened results archives go), `choose_searches_folder()` (the system's folder dialog, then the folder chosen
//!   for them, checked: `location::check_folder` -> the new location, or null if cancelled) and
//!   `use_default_searches_folder()` (back to `<app data>/searches`). History lists the searches of all the folders
//!   the app has kept them in, and the commands above find a search by its id in them.
//!
//! - Search setups (`setup_file`): `save_setup_file(name, contents, ask)` (the save dialog unless `ask` is false and a
//!   setup was opened or saved before: then that file again) -> `{ path, name }` or null;
//!   `open_setup_file(recent?)` (the open dialog, or the `recent`-th entry of File › Open Recent) -> `{ path,
//!   text }` or null, and `use_opened_setup_file()` once the page has read that file as a setup (only then is it
//!   the one Save writes to); both refuse other files than `.prose-setup.json`
//!   and more than 4 MB, and put the file first in the recent setups; Save asks before it replaces a file whose
//!   name it gave the setup's extension; `forget_setup_file()` detaches the current one (New Search). `menu_state({ result, table, theme })`: which menu items apply (`menu`); a click on one
//!   of the page's items is the event `menu://action` with its id.
//! - closing the window while a search runs, or while the setup has changes not saved to its file (the page
//!   says so with `set_setup_unsaved(unsaved)`): the window stays and `app://close-requested` asks the page,
//!   which answers with `answer_close(close)` ([`close_requested`])
//!
//! `--smoke-test` on the command line is the packaged app's start test ([`SMOKE_TEST`]); `--watch-search`
//! makes the executable the watchdog of a search (macOS: [`watchdog`]).
//!
//! The commands are in `commands`, a module per part of the app (`commands/*.rs`).

pub mod archive;
pub mod args;
pub mod contaminants;
pub mod content;
pub mod decoys;
pub mod files;
pub mod history;
pub mod location;
pub mod menu;
pub mod net;
pub mod progress;
pub mod run;
pub mod setup_file;
pub mod spectra;
pub mod taxonomy;
pub mod tool;
#[cfg(unix)]
pub mod watchdog;

mod commands;

pub use commands::history::ARCHIVE_DIALOG_TITLE;
pub use commands::location::SEARCHES_DIALOG_TITLE;

use location::Locations;
use run::Searches;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager, RunEvent};

struct AppState {
    searches: Arc<Searches>,
    closing: std::sync::Mutex<Closing>,
    /// The setup file opened or saved last (`<name>.prose-setup.json`): Save writes there again.
    setup_path: std::sync::Mutex<Option<PathBuf>>,
    /// The setup file Open Setup read last, until the page has taken it (`use_opened_setup_file`): a file the
    /// page refuses never becomes the one Save writes to.
    opened_setup: std::sync::Mutex<Option<PathBuf>>,
    /// The page's setup has changes not saved to its file (`set_setup_unsaved`): closing the window asks first.
    setup_unsaved: std::sync::atomic::AtomicBool,
}

impl Default for AppState {
    /// On macOS each search's tool is watched by the app's own executable (`watchdog`): nothing else ends
    /// it when the app dies.
    fn default() -> AppState {
        let searches = Searches::default();
        #[cfg(target_os = "macos")]
        let searches = match std::env::current_exe() {
            Ok(exe) => searches.with_watchdog(exe),
            Err(_) => searches,
        };
        AppState {
            searches: Arc::new(searches),
            closing: Default::default(),
            setup_path: Default::default(),
            opened_setup: Default::default(),
            setup_unsaved: Default::default(),
        }
    }
}

/// The window's close while a search runs ([`close_requested`]).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Closing {
    /// The page has been asked to confirm, and has not answered yet.
    asked: bool,
    /// The user confirmed: the next close request closes.
    confirmed: bool,
}

impl Closing {
    /// A search starts: a question about an earlier one that was never answered (the page could not answer, or
    /// the search ended meanwhile) is forgotten, so that closing the window during this one asks again.
    pub fn search_started(&mut self) {
        self.asked = false;
    }
}

/// What a close request of the window does: `true` lets the window close; `false` keeps it and asks the
/// page to confirm (`app://close-requested`). Without a running search it closes as always. While the
/// page has not answered, a second request closes too: the user insists (or the page cannot answer).
pub fn close_requested(closing: &mut Closing, search_running: bool) -> bool {
    if closing.confirmed || !search_running || closing.asked {
        closing.confirmed = true;
        return true;
    }
    closing.asked = true;
    false
}

/// The app's resource folder: Tauri's; on macOS, where Tauri refuses to name one when the app's path has
/// a symbolic link in it (tauri-utils' StartingBinary, against relaunch tricks), the Resources folder of
/// the bundle the executable really is in (`tool::bundle_resources`). Only read from (the bundled tool,
/// the example, cRAP).
fn resource_dir(app: &AppHandle) -> Option<PathBuf> {
    match app.path().resource_dir() {
        Ok(dir) => Some(dir),
        Err(_) if cfg!(target_os = "macos") => std::env::current_exe()
            .ok()
            .and_then(|exe| tool::bundle_resources(&exe)),
        Err(_) => None,
    }
}

/// The app's data folder (its own searches folder, the choice of another: `location`).
fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// The folder of the recent setups' list (the app's data folder), if there is one.
fn recent_dir<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok()
}

/// The flag of the packaged app's start test (CI: `native/scripts/start-test.sh`). The app starts as
/// usual; once its page has loaded it prints one line of JSON, `{"page": <url>, "resources": <the
/// resource folder the app uses>, "tauriResources": <Tauri's, or why there is none>, "tool":
/// <tool_info>}` (or `"error"` in place of `"tool"`), and exits: 0, or 1 when the tool cannot be used. A page that has not loaded after [`SMOKE_TIMEOUT`]
/// ends the app with 2. It reads nothing and writes nothing else.
pub const SMOKE_TEST: &str = "--smoke-test";
const SMOKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// What `--smoke-test` reports, and the exit code.
fn smoke_report(app: &AppHandle, page: &str) -> (i32, String) {
    let tool = tool::resolve(resource_dir(app).as_deref()).and_then(|path| tool::probe(&path));
    let (code, key, value) = match tool {
        Ok(info) => (0, "tool", serde_json::to_value(info).unwrap_or_default()),
        Err(e) => (1, "error", serde_json::Value::String(e)),
    };
    let resources = resource_dir(app).map(|dir| dir.to_string_lossy().into_owned());
    let tauri_resources = match app.path().resource_dir() {
        Ok(dir) => dir.to_string_lossy().into_owned(),
        Err(e) => format!("(none: {e})"),
    };
    let line = serde_json::json!({
        "page": page,
        "resources": resources,
        "tauriResources": tauri_resources,
        key: value,
    })
    .to_string();
    (code, line)
}

pub fn run() {
    // the watchdog of a search (macOS: the app starts its own executable for it), before anything of Tauri
    #[cfg(unix)]
    if let Some(code) = watchdog::from_args(&std::env::args_os().collect::<Vec<_>>()) {
        std::process::exit(code);
    }
    let smoke = std::env::args_os().any(|a| a == SMOKE_TEST);
    let reported = std::sync::atomic::AtomicBool::new(false);
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        // the menu bar (macOS) or the window's menus (Windows, Linux): `menu`
        .menu(|handle| {
            let (bar, handles) = menu::build(handle)?;
            let recent = recent_dir(handle)
                .map(|d| setup_file::recent(&d))
                .unwrap_or_default();
            let _ = handles.set_recent(handle, recent);
            handle.manage(handles);
            Ok(bar)
        })
        .on_menu_event(|handle, event| menu::on_event(handle, event, recent_dir(handle)))
        // closing the window during a search, or with unsaved changes to the setup: the page asks first
        // (CloseGuard.svelte, `answer_close`)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let ask = state.searches.any_running()
                    || state
                        .setup_unsaved
                        .load(std::sync::atomic::Ordering::SeqCst);
                let close = close_requested(&mut state.closing.lock().unwrap(), ask);
                if !close {
                    api.prevent_close();
                    let _ = window.emit("app://close-requested", ());
                }
            }
        })
        .on_page_load(move |webview, payload| {
            use std::sync::atomic::Ordering;
            if !smoke
                || payload.event() != PageLoadEvent::Finished
                || reported.swap(true, Ordering::SeqCst)
            {
                return;
            }
            let app = webview.app_handle().clone();
            let page = payload.url().to_string();
            std::thread::spawn(move || {
                use std::io::Write;
                let (code, line) = smoke_report(&app, &page);
                println!("{line}");
                let _ = std::io::stdout().flush();
                app.exit(code);
            });
        })
        .setup(move |app| {
            if smoke {
                std::thread::spawn(|| {
                    std::thread::sleep(SMOKE_TIMEOUT);
                    eprintln!("smoke test: the page did not load within {SMOKE_TIMEOUT:?}");
                    std::process::exit(2);
                });
            }
            // what interrupted searches left, in every searches folder (no search runs yet); a failure to sweep is no
            // failure to start
            if let Ok(data) = data_dir(app.handle()) {
                std::thread::spawn(move || {
                    let locations = Locations::load(&data);
                    let now = std::time::SystemTime::now();
                    for root in locations.roots() {
                        let own = locations.is_own(&root);
                        history::sweep_in(&root, now, history::SWEEP_GRACE, own);
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::search::tool_info,
            commands::search::start_search,
            commands::search::cancel_search,
            commands::search::read_search_file,
            commands::search::inspect_files,
            commands::search::inspect_spectra,
            commands::search::cpu_count,
            commands::search::example_files,
            commands::exports::save_text,
            commands::databases::download_proteome,
            commands::databases::downloaded_proteomes,
            commands::databases::find_proteomes,
            commands::window::open_url,
            commands::exports::save_bytes,
            commands::exports::save_search_file,
            commands::history::history_list,
            commands::history::history_save_record,
            commands::history::history_save_proteins,
            commands::history::history_rename,
            commands::history::history_delete,
            commands::history::history_files,
            commands::history::save_curation,
            commands::history::import_archive_file,
            commands::history::open_qpx_folder,
            commands::history::history_import_qpx,
            commands::history::save_archive_file,
            commands::setup::save_setup_file,
            commands::setup::open_setup_file,
            commands::setup::use_opened_setup_file,
            commands::setup::forget_setup_file,
            commands::setup::set_setup_unsaved,
            commands::window::menu_state,
            commands::window::answer_close,
            commands::location::searches_location,
            commands::location::choose_searches_folder,
            commands::location::use_default_searches_folder
        ])
        .build(tauri::generate_context!())
        .expect("error while building the ProSE desktop app");
    app.run(|handle, event| {
        // no ProSE outlives the app
        if let RunEvent::Exit = event {
            handle.state::<AppState>().searches.cancel_all();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{close_requested, Closing};

    #[test]
    fn closing_the_window_asks_first_while_a_search_runs() {
        // no search: closes
        let mut closing = Closing::default();
        assert!(close_requested(&mut closing, false));
        // a search runs: kept, the page asked
        let mut closing = Closing::default();
        assert!(!close_requested(&mut closing, true));
        assert!(closing.asked && !closing.confirmed);
        // asked again before an answer: closes (the user insists, or the page cannot answer)
        assert!(close_requested(&mut closing, true));
        // the page said "keep searching": the next request asks again
        let mut closing = Closing::default();
        assert!(!close_requested(&mut closing, true));
        closing = Closing {
            asked: false,
            confirmed: false,
        };
        assert!(!close_requested(&mut closing, true));
        // confirmed: closes, also while the search still runs
        let mut closing = Closing {
            asked: false,
            confirmed: true,
        };
        assert!(close_requested(&mut closing, true));
    }

    #[test]
    fn a_question_left_open_does_not_outlive_its_search() {
        // asked, never answered (the search ended as the dialog was open, and the page could not answer)
        let mut closing = Closing::default();
        assert!(!close_requested(&mut closing, true));
        assert!(closing.asked);
        // no search runs: the window just closes
        assert!(close_requested(&mut closing.clone(), false));
        // the next search asks again (and does not close at the first request)
        closing.search_started();
        assert!(!close_requested(&mut closing, true));
        assert!(closing.asked && !closing.confirmed);
    }
}

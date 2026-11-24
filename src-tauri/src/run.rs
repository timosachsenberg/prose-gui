//! Running a search: the search folder, the process, its progress, and cancel.
//!
//! A search lives in `<searches>/<id>.partial/` while it runs. Beside the tool's outputs it holds
//!
//! - the inputs as links `<n>__<name>` to the user's files (never copies) and `inputs.json`, the
//!   mapping from link to the user's path (see [`crate::args::InputRecord`]). On Windows, where a
//!   symbolic link needs a privilege, ProSE gets the user's paths themselves when the inputs' names
//!   differ, and hard links only for inputs that share a name, removed again when the search ends
//!   ([`crate::args::InputLinks`]);
//! - `database.fasta`, the user's FASTA with the contaminants, if they were asked for;
//! - `prose.log`, the tool's output.
//!
//! The folder holds `.lock`, locked exclusively (`history::lock_partial`) for as long as the search
//! lives, so that the startup sweep of any app instance can tell a live folder from a stale one.
//!
//! On success the folder is renamed to `<searches>/<id>/` and `done` is sent; on failure or cancel it is
//! removed (a failure sends `failed`, a cancel nothing: `cancel` returns when the process is gone). The
//! log of a failed search is kept, as `<app data>/logs/last-failure.log` ([`LAST_FAILURE_LOG`]; the
//! searches folder's sibling), replacing the one before.
//!
//! The tool runs in a process group of its own, started from a thread of its own that waits for it.
//! Cancel signals the group (SIGTERM, then SIGKILL after a grace period), and the group is swept with
//! SIGKILL after the tool has ended, so nothing it started outlives it. If the app dies:
//! - Linux: the tool gets SIGKILL (`PR_SET_PDEATHSIG`; the thread that starts it lives as long as it runs);
//! - Windows: the group is a job object, closed with the app, which ends whatever is still in it (cancel
//!   terminates the job: Windows has no SIGTERM for a console tool);
//! - macOS has no death signal: a watchdog process per search (the app's own executable, started with
//!   `--watch-search`: [`crate::watchdog`]) ends the tool's group within a second of the app's end. Without
//!   it (it could not be started) the tool would end at its next write to stdout or stderr, whose reading
//!   ends died with the app (SIGPIPE, at its default in the child: `Command` resets it, and OpenMS sets no
//!   handler), at the end of its current step; a later start of the app sweeps the folder.
//!
//! The parts: `folder` makes and settles the search folder, `failure` tells a failure from the tool's
//! stderr and exit status, `sys` holds the platform's process groups (`run/sys_unix.rs`,
//! `run/sys_windows.rs`).

mod failure;
mod folder;
#[cfg(unix)]
#[path = "run/sys_unix.rs"]
mod sys;
#[cfg(windows)]
#[path = "run/sys_windows.rs"]
mod sys;

use crate::args::{
    build_args_with, humanize, input_placements, input_records_with, with_idxml_option, InputLinks,
    InputRecord, Placement, SearchRequest,
};
use crate::history::{finish_partial, lock_partial, with_backoff};
use crate::progress::{ProgressTracker, STARTING};
use crate::tool;
pub use failure::how_it_ended;
use failure::StderrDigest;
use folder::{discard_partial, keep_failure_log, prepare_folder, write_ini_and_command};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a tool gets to end after SIGTERM before it is killed.
const GRACE: Duration = Duration::from_secs(3);
/// How long cancel waits for the kill to take effect.
const KILL_WAIT: Duration = Duration::from_secs(10);
/// The longest line of the tool's output that is read (bytes); the rest of a longer line is dropped (a
/// 300 MB line on stderr was otherwise read whole, logged and sent as an event).
const MAX_LINE: usize = 64 * 1024;
/// The log of the last failed search, in the folder `logs` beside the searches folder.
pub const LAST_FAILURE_LOG: &str = "last-failure.log";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub search_id: String,
    pub label: String,
    /// The input of the step (0-based); -1 outside the files.
    pub file: i32,
    pub fraction: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DoneEvent {
    pub search_id: String,
    /// The search folder, `<searches>/<id>`.
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FailedEvent {
    pub search_id: String,
    /// What went wrong: the informative line(s) of the tool's stderr (`StderrDigest::message`), or the
    /// shim's own message; empty when the tool said nothing that explains it.
    pub message: String,
    /// The last progress label.
    pub stage: String,
    /// The input of the last progress report; absent outside the files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<i32>,
    /// How the tool ended, when it ended badly: "ProSE crashed (signal 11, SIGSEGV)", "ProSE ended with
    /// exit code 3" ([`how_it_ended`]); the message leads with it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    /// The last lines the tool wrote to stderr (the user's names for the inputs), for the details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Where the search's whole log was kept ([`LAST_FAILURE_LOG`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchEvent {
    Progress(ProgressEvent),
    Done(DoneEvent),
    Failed(FailedEvent),
}

/// Where a search's events go (the app emits them as `search://…` events).
pub type Sink = Arc<dyn Fn(SearchEvent) + Send + Sync>;

/// A running search, as `cancel` sees it.
struct Handle {
    /// The tool's process group (see `sys::Group`); set before `start` returns, taken when the search
    /// is over.
    group: Mutex<Option<Arc<sys::Group>>>,
    cancelled: AtomicBool,
    /// Set when the search is over and its folder dealt with.
    finished: Mutex<bool>,
    changed: Condvar,
}

impl Handle {
    fn group(&self) -> Option<Arc<sys::Group>> {
        self.group.lock().unwrap().clone()
    }

    fn finish(&self) {
        *self.finished.lock().unwrap() = true;
        self.changed.notify_all();
    }

    fn wait_finished(&self, timeout: Duration) -> bool {
        let guard = self.finished.lock().unwrap();
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, timeout, |done| !*done)
            .unwrap();
        *guard
    }
}

/// The searches running in this app.
pub struct Searches {
    running: Mutex<HashMap<String, Arc<Handle>>>,
    counter: AtomicU64,
    /// How the inputs reach the tool: this platform's way, unless a test chooses.
    links: InputLinks,
    /// The executable that watches each search's tool ([`crate::watchdog`]; macOS: the app's own), or
    /// none. (Unix only: Windows' job object ends the tool with the app.)
    #[cfg_attr(not(unix), allow(dead_code))]
    watchdog: Option<PathBuf>,
}

impl Default for Searches {
    fn default() -> Searches {
        Searches::with_links(InputLinks::PLATFORM)
    }
}

/// Lines of a stream, lossily decoded, without the line end; a line longer than [`MAX_LINE`] bytes is cut
/// there (and ends with "…"), the rest of it dropped unread into memory.
fn read_lines(stream: impl Read, mut each: impl FnMut(&str)) {
    let mut reader = BufReader::new(stream);
    let mut line: Vec<u8> = Vec::new();
    let mut cut = false;
    let mut emit = |line: &mut Vec<u8>, cut: &mut bool| {
        let text = String::from_utf8_lossy(line);
        let text = text.trim_end_matches(['\n', '\r']);
        if *cut {
            each(&format!("{text}…"));
        } else {
            each(text);
        }
        line.clear();
        *cut = false;
    };
    loop {
        let buffer = match reader.fill_buf() {
            Ok([]) => break,
            Ok(buffer) => buffer,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let (taken, ends) = match buffer.iter().position(|&b| b == b'\n') {
            Some(at) => (at + 1, true),
            None => (buffer.len(), false),
        };
        let room = MAX_LINE.saturating_sub(line.len());
        line.extend_from_slice(&buffer[..taken.min(room)]);
        cut |= taken > room;
        reader.consume(taken);
        if ends {
            emit(&mut line, &mut cut);
        }
    }
    if !line.is_empty() {
        emit(&mut line, &mut cut);
    }
}

/// Everything the supervising thread needs.
struct Job {
    id: String,
    tool: PathBuf,
    args: Vec<std::ffi::OsString>,
    partial: PathBuf,
    done_dir: PathBuf,
    records: Vec<InputRecord>,
    placements: Vec<Placement>,
    sink: Sink,
    handle: Arc<Handle>,
    /// The lock of the partial folder (see `history::lock_partial`), held until the folder is renamed
    /// or removed.
    lock: Option<fs::File>,
}

impl Searches {
    /// Searches whose inputs reach the tool as `links` says (the default: this platform's way).
    pub fn with_links(links: InputLinks) -> Searches {
        Searches {
            running: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(0),
            links,
            watchdog: None,
        }
    }

    /// The same, each search's tool watched by `exe --watch-search` ([`crate::watchdog`]; Unix): the app's
    /// own executable on macOS, where nothing else ends the tool with the app.
    pub fn with_watchdog(mut self, exe: PathBuf) -> Searches {
        self.watchdog = Some(exe);
        self
    }

    fn new_id(&self, dir: &Path) -> String {
        loop {
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let n = self.counter.fetch_add(1, Ordering::Relaxed);
            let id = format!("{millis}-{n}");
            if !dir.join(&id).exists() && !dir.join(format!("{id}.partial")).exists() {
                return id;
            }
        }
    }

    /// Checks the request, makes the search folder and starts the tool; returns the search id. The
    /// events go to `sink`. `crap` is the contaminant FASTA, needed if the request asks for them.
    pub fn start(
        self: &Arc<Self>,
        tool_path: &Path,
        searches_dir: &Path,
        crap: Option<&Path>,
        request: SearchRequest,
        sink: Sink,
    ) -> Result<String, String> {
        if request.contaminants && crap.is_none() {
            return Err(
                "The contaminant database (cRAP) is not available in this installation."
                    .to_string(),
            );
        }
        fs::create_dir_all(searches_dir).map_err(|e| {
            format!(
                "The searches folder {} cannot be made ({e}).",
                searches_dir.display()
            )
        })?;
        let id = self.new_id(searches_dir);
        let partial = searches_dir.join(format!("{id}.partial"));
        let done_dir = searches_dir.join(&id);
        // everything that can be wrong with the request is found before a folder exists
        // (a one-input search's idXML: the option the tool lists, `-out` or, before OpenMS 3.7, `-out_idxml`)
        let args = with_idxml_option(
            build_args_with(&request, &partial, self.links)?,
            tool::idxml_option_of(tool_path),
        );

        fs::create_dir(&partial).map_err(|e| {
            format!(
                "The search folder {} cannot be made ({e}).",
                partial.display()
            )
        })?;
        let lock = match lock_partial(&partial) {
            Ok(lock) => lock,
            Err(e) => {
                discard_partial(None, &partial, None);
                return Err(format!("The search folder cannot be locked ({e})."));
            }
        };
        let records = input_records_with(&request, self.links, &partial);
        let placements = input_placements(&request, self.links, &partial);
        if let Err(e) = prepare_folder(&partial, &request, &records, &placements, crap) {
            discard_partial(Some(lock), &partial, None);
            return Err(e);
        }
        // ProSE's parameters go in through an INI in the folder, which stays as the record of them (guidance G8),
        // with the command line, its paths as placeholders
        let args = match write_ini_and_command(&partial, args) {
            Ok(args) => args,
            Err(e) => {
                discard_partial(Some(lock), &partial, None);
                return Err(e);
            }
        };

        let handle = Arc::new(Handle {
            group: Mutex::new(None),
            cancelled: AtomicBool::new(false),
            finished: Mutex::new(false),
            changed: Condvar::new(),
        });
        self.running
            .lock()
            .unwrap()
            .insert(id.clone(), handle.clone());
        let job = Job {
            id: id.clone(),
            tool: tool_path.to_path_buf(),
            args,
            partial: partial.clone(),
            done_dir,
            records,
            placements,
            sink,
            handle,
            lock: Some(lock),
        };
        let (started_tx, started_rx) = mpsc::channel();
        let searches = self.clone();
        let spawned = thread::Builder::new()
            .name(format!("prose-{id}"))
            .spawn(move || supervise(searches, job, started_tx));
        let result = match spawned {
            Ok(_) => started_rx.recv().unwrap_or_else(|_| {
                Err("The search thread ended before the tool started.".to_string())
            }),
            Err(e) => Err(format!("The search thread cannot be started ({e}).")),
        };
        if let Err(e) = result {
            self.running.lock().unwrap().remove(&id);
            // (the thread has ended and released the lock)
            discard_partial(None, &partial, None);
            return Err(e);
        }
        Ok(id)
    }

    /// Stops a search: its process group gets SIGTERM, and SIGKILL if it is still there after a few
    /// seconds (Windows: its job is terminated at once). Returns when the process is gone and the
    /// partial folder is removed. A search that is not running (finished, or never existed) is not an
    /// error.
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let Some(handle) = self.running.lock().unwrap().get(id).cloned() else {
            return Ok(());
        };
        handle.cancelled.store(true, Ordering::SeqCst);
        let group = handle.group();
        if let Some(group) = &group {
            group.terminate();
        }
        if handle.wait_finished(GRACE) {
            return Ok(());
        }
        if let Some(group) = &group {
            group.kill();
        }
        if handle.wait_finished(KILL_WAIT) {
            Ok(())
        } else {
            let pid = group.map(|g| g.pid()).unwrap_or(0);
            Err(format!("ProSE (process {pid}) did not stop."))
        }
    }

    /// Whether a search is running (its tool has not ended yet).
    pub fn any_running(&self) -> bool {
        !self.running.lock().unwrap().is_empty()
    }

    /// Cancels every running search (the app is closing).
    pub fn cancel_all(&self) {
        let handles: Vec<Arc<Handle>> = self.running.lock().unwrap().values().cloned().collect();
        let running: Vec<(Arc<Handle>, Option<Arc<sys::Group>>)> = handles
            .into_iter()
            .map(|h| {
                h.cancelled.store(true, Ordering::SeqCst);
                let group = h.group();
                if let Some(group) = &group {
                    group.terminate();
                }
                (h, group)
            })
            .collect();
        for (handle, group) in running {
            if !handle.wait_finished(GRACE) {
                if let Some(group) = &group {
                    group.kill();
                }
                handle.wait_finished(KILL_WAIT);
            }
        }
    }
}

/// Runs the tool, passes on its progress, and settles the folder. Runs on the thread that started the
/// tool, which is what ties the tool to the app on Linux (`PR_SET_PDEATHSIG`).
fn supervise(searches: Arc<Searches>, job: Job, started: mpsc::Sender<Result<(), String>>) {
    let Job {
        id,
        tool: tool_path,
        args,
        partial,
        done_dir,
        records,
        placements,
        sink,
        handle,
        mut lock,
    } = job;
    let mut command = tool::command(&tool_path);
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    sys::prepare(&mut command);
    let mut child: Child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = started.send(Err(format!(
                "The ProSE tool {} cannot be started ({e}).",
                tool_path.display()
            )));
            return;
        }
    };
    let group = Arc::new(sys::Group::new(&child));
    *handle.group.lock().unwrap() = Some(group.clone());
    // (macOS) the tool ends with the app, through a watchdog of its own
    #[cfg(unix)]
    let mut watchdog = searches
        .watchdog
        .as_deref()
        .and_then(|exe| crate::watchdog::start(exe, group.pid()));
    let _ = started.send(Ok(()));
    let emit = |event: SearchEvent| (sink)(event);
    emit(SearchEvent::Progress(ProgressEvent {
        search_id: id.clone(),
        label: STARTING.to_string(),
        file: -1,
        fraction: 0.0,
    }));

    let log = fs::File::create(partial.join("prose.log"))
        .ok()
        .map(|f| Arc::new(Mutex::new(f)));
    if let Some(log) = &log {
        let _ = writeln!(
            log.lock().unwrap(),
            "$ {} {}",
            tool_path.display(),
            args.iter()
                .map(|a| a.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    let names: Vec<String> = records.iter().map(|r| r.name.clone()).collect();
    let stdout = child.stdout.take();
    let out_reader = {
        let (sink, id, log) = (sink.clone(), id.clone(), log.clone());
        thread::spawn(move || {
            let mut tracker = ProgressTracker::new(names);
            if let Some(stdout) = stdout {
                read_lines(stdout, |line| {
                    if let Some(log) = &log {
                        let _ = writeln!(log.lock().unwrap(), "{line}");
                    }
                    if let Some(p) = tracker.feed(line) {
                        sink(SearchEvent::Progress(ProgressEvent {
                            search_id: id.clone(),
                            label: p.label,
                            file: p.file,
                            fraction: p.fraction,
                        }));
                    }
                });
            }
            tracker
        })
    };
    let stderr = child.stderr.take();
    let err_reader = {
        let log = log.clone();
        thread::spawn(move || {
            let mut digest = StderrDigest::default();
            if let Some(stderr) = stderr {
                read_lines(stderr, |line| {
                    if line.starts_with("Schema error") {
                        return;
                    }
                    if let Some(log) = &log {
                        let _ = writeln!(log.lock().unwrap(), "{line}");
                    }
                    digest.feed(line);
                });
            }
            digest
        })
    };

    let status = child.wait();
    // whatever the tool left running goes with it (and holds no pipe open for the readers)
    group.kill();
    #[cfg(unix)]
    if let Some(watchdog) = watchdog.as_mut() {
        let _ = watchdog.kill();
        let _ = watchdog.wait();
    }
    let tracker = out_reader
        .join()
        .unwrap_or_else(|_| ProgressTracker::new(Vec::new()));
    let digest = err_reader.join().unwrap_or_default();
    drop(log);

    // the hard links to the inputs (Windows) have served: they do not belong to the search, and would
    // keep a file the user deletes on the disk
    for (record, placement) in records.iter().zip(&placements) {
        if *placement == Placement::HardLink {
            let _ = with_backoff(|| fs::remove_file(partial.join(&record.link)));
        }
    }
    let outcome = if handle.cancelled.load(Ordering::SeqCst) {
        discard_partial(lock.take(), &partial, None);
        None
    } else {
        match status {
            Ok(status) if status.success() => {
                match finish_partial(lock.take(), &partial, &done_dir) {
                    Ok(()) => Some(SearchEvent::Done(DoneEvent {
                        search_id: id.clone(),
                        folder: done_dir.to_string_lossy().into_owned(),
                    })),
                    Err(e) => {
                        let log = keep_failure_log(&partial);
                        discard_partial(None, &partial, log.as_deref());
                        Some(failed(
                            &id,
                            format!("The search folder cannot be finished ({e})."),
                            &tracker,
                            Failure {
                                log,
                                ..Failure::default()
                            },
                        ))
                    }
                }
            }
            Ok(status) => {
                // a crash: a signal, or on Windows an exception's status (how_it_ended)
                let signalled = sys::exit_signal(&status).is_some()
                    || (cfg!(windows) && status.code().is_some_and(|c| (c as u32) >= 0xC000_0000));
                let message = humanize(&digest.message(signalled), &partial, &records);
                let detail = (!digest.tail.is_empty()).then(|| {
                    let lines: Vec<&str> = digest.tail.iter().map(String::as_str).collect();
                    humanize(&lines.join("\n"), &partial, &records)
                });
                let log = keep_failure_log(&partial);
                discard_partial(lock.take(), &partial, log.as_deref());
                Some(failed(
                    &id,
                    message,
                    &tracker,
                    Failure {
                        ended: how_it_ended(&status),
                        detail,
                        log,
                    },
                ))
            }
            Err(e) => {
                let log = keep_failure_log(&partial);
                discard_partial(lock.take(), &partial, log.as_deref());
                Some(failed(
                    &id,
                    format!("Waiting for ProSE failed ({e})."),
                    &tracker,
                    Failure {
                        log,
                        ..Failure::default()
                    },
                ))
            }
        }
    };
    searches.running.lock().unwrap().remove(&id);
    // (Windows: closing the job ends anything still in it; the process's own handle goes too, so that
    // it is gone from the process list when cancel returns)
    handle.group.lock().unwrap().take();
    drop(group);
    drop(child);
    if let Some(event) = outcome {
        emit(event);
    }
    handle.finish();
}

/// What a failure says beyond its message (see [`FailedEvent`]).
#[derive(Default)]
struct Failure {
    ended: Option<String>,
    detail: Option<String>,
    log: Option<PathBuf>,
}

fn failed(id: &str, message: String, tracker: &ProgressTracker, more: Failure) -> SearchEvent {
    SearchEvent::Failed(FailedEvent {
        search_id: id.to_string(),
        message,
        stage: tracker.label().to_string(),
        file: tracker.file(),
        ended: more.ended,
        detail: more.detail,
        log: more.log.map(|p| p.to_string_lossy().into_owned()),
    })
}

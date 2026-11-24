//! Searches against the fake tool: the folder, the events, cancel, failures, the watchdog.

use super::*;
use prose_desktop_lib::args::{
    humanize, input_placements, input_records_with, input_stem, max_stem, Placement, MAX_STEM,
};
use prose_desktop_lib::run::{SearchEvent, Searches};
use std::fs;
use std::sync::Arc;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(20);

/// A searches folder of its own and the searches run in it (also used by `contaminants`).
pub(super) struct Run {
    work: TempDir,
    searches: Arc<Searches>,
}

impl Run {
    pub(super) fn new() -> Run {
        Run {
            work: TempDir::new("run"),
            searches: Arc::new(Searches::default()),
        }
    }

    fn dir(&self) -> PathBuf {
        self.work.path().join("searches")
    }

    pub(super) fn start(
        &self,
        req: SearchRequest,
        crap: Option<&Path>,
    ) -> (
        Result<String, String>,
        std::sync::mpsc::Receiver<SearchEvent>,
    ) {
        let (sink, rx) = channel_sink();
        (
            self.searches
                .start(&fake_prose(), &self.dir(), crap, req, sink),
            rx,
        )
    }

    pub(super) fn listing(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.dir())
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

impl Run {
    /// As `new`, with the inputs reaching the tool as `links` says (Windows' way on any platform).
    fn with_links(links: InputLinks) -> Run {
        Run {
            work: TempDir::new("run"),
            searches: Arc::new(Searches::with_links(links)),
        }
    }
}

fn done_folder(events: &[SearchEvent]) -> PathBuf {
    match events.last() {
        Some(SearchEvent::Done(d)) => PathBuf::from(&d.folder),
        other => panic!("not done: {other:?} in {events:?}"),
    }
}

#[test]
fn a_search_of_the_users_database_records_its_path_for_the_reader() {
    let run = Run::new();
    let i = inputs();
    let (id, rx) = run.start(request(&[&i.mzml], &i.fasta, false, json!({})), None);
    let id = id.unwrap();
    let folder = done_folder(&until_terminal(&rx, WAIT));
    assert_eq!(folder, run.dir().join(&id));
    assert!(!folder.join("database.fasta").exists());
    let record: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(folder.join("database.json")).unwrap()).unwrap();
    assert_eq!(record["path"].as_str(), i.fasta.to_str());
    assert_eq!(
        prose_desktop_lib::files::recorded_database(&folder).as_deref(),
        i.fasta.to_str()
    );
}

#[test]
fn a_search_ends_with_done_and_the_partial_folder_becomes_the_search_folder() {
    let run = Run::new();
    let i = inputs();
    let crap = i.dir.write("crap.fasta", ">ALBU\nMKW\n");
    let (id, rx) = run.start(request(&[&i.mzml], &i.fasta, true, json!({})), Some(&crap));
    let id = id.unwrap();
    let events = until_terminal(&rx, WAIT);
    let folder = done_folder(&events);

    assert_eq!(folder, run.dir().join(&id));
    assert_eq!(
        run.listing(),
        vec![id.clone()],
        "no .partial folder is left"
    );
    assert!(folder.join("qpx/quantms.psm.parquet").is_file());
    assert!(
        !folder.join(".lock").exists(),
        "the lock file does not belong to the search"
    );
    assert!(fs::read_to_string(folder.join("database.fasta"))
        .unwrap()
        .contains(">CON__ALBU"));
    // ProSE searched the folder's own database: no path outside the folder is recorded
    assert!(!folder.join("database.json").exists());
    // every event carries the search id; progress stays below 1 (the search is not done before `done`)
    for event in &events {
        match event {
            SearchEvent::Progress(p) => {
                assert_eq!(p.search_id, id);
                assert!(p.fraction < 1.0, "{p:?}");
            }
            SearchEvent::Done(d) => assert_eq!(d.search_id, id),
            SearchEvent::Failed(f) => panic!("{f:?}"),
        }
    }
    assert!(
        matches!(&events[0], SearchEvent::Progress(p) if p.file == -1 && p.fraction == 0.0),
        "{:?}",
        events[0]
    );
    assert!(events.iter().any(|e| matches!(e, SearchEvent::Progress(p) if p.label == "Searching a.mzML (1 of 1)..." && p.file == 0)));
    // the fake ran with exactly the arguments build_args makes for the partial folder
    let argv = fs::read_to_string(folder.join("argv.txt")).unwrap();
    let input = match InputLinks::PLATFORM {
        InputLinks::Symlinks => run.dir().join(format!("{id}.partial")).join("1__a.mzML"),
        InputLinks::PathsOrHardLinks => i.mzml.clone(),
    };
    assert!(
        argv.lines().any(|l| Path::new(l) == input),
        "{input:?} in {argv}"
    );
    // nothing was written next to the input
    let mut data: Vec<_> = fs::read_dir(i.mzml.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    data.sort();
    assert_eq!(data, ["a.mzML", "db.fasta"]);
}

#[cfg(unix)]
#[test]
fn two_inputs_with_the_same_name_from_different_folders_are_both_searched() {
    let run = Run::new();
    let i = inputs();
    let first = i.dir.write("run1/sample.mzML", "<mzML>first</mzML>");
    let second = i.dir.write("run2/sample.mzML", "<mzML>second</mzML>");
    let (id, rx) = run.start(
        request(&[&first, &second], &i.fasta, false, json!({})),
        None,
    );
    let id = id.unwrap();
    let folder = done_folder(&until_terminal(&rx, WAIT));
    assert_eq!(folder, run.dir().join(&id));

    // the tool got two differently named files, each one a link to its own original
    let resolved = fs::read_to_string(folder.join("inputs.txt")).unwrap();
    let lines: Vec<&str> = resolved.lines().collect();
    assert_eq!(lines.len(), 2, "{resolved}");
    assert!(
        lines[0].contains("/1__sample.mzML ->")
            && lines[0].ends_with(first.canonicalize().unwrap().to_str().unwrap()),
        "{resolved}"
    );
    assert!(
        lines[1].contains("/2__sample.mzML ->")
            && lines[1].ends_with(second.canonicalize().unwrap().to_str().unwrap()),
        "{resolved}"
    );
    assert!(
        folder.join("qpx/runs/1__sample.psm.parquet").is_file()
            && folder.join("qpx/runs/2__sample.psm.parquet").is_file()
    );

    // links, not copies; the mapping is recorded for the results
    for (n, original) in [(1, &first), (2, &second)] {
        let link = folder.join(format!("{n}__sample.mzML"));
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "{link:?} is a link"
        );
        assert_eq!(
            fs::read_link(&link).unwrap(),
            original.canonicalize().unwrap()
        );
    }
    let mapping: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("inputs.json")).unwrap()).unwrap();
    assert_eq!(
        mapping,
        json!([
            { "id": "f0", "index": 0, "link": "1__sample.mzML", "stem": "1__sample", "name": "sample.mzML", "path": first },
            { "id": "f1", "index": 1, "link": "2__sample.mzML", "stem": "2__sample", "name": "sample.mzML", "path": second },
        ])
    );
}

#[cfg(unix)]
#[test]
fn gz_inputs_keep_their_suffix_in_the_link() {
    let run = Run::new();
    let i = inputs();
    let gz = i.dir.write("zipped/x.mzML.gz", "gz");
    let (id, rx) = run.start(request(&[&gz], &i.fasta, false, json!({})), None);
    id.unwrap();
    let folder = done_folder(&until_terminal(&rx, WAIT));
    assert!(fs::symlink_metadata(folder.join("1__x.mzML.gz"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(folder.join("qpx/runs/1__x.psm.parquet").is_file());
    let mapping: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("inputs.json")).unwrap()).unwrap();
    assert_eq!(mapping[0]["link"], "1__x.mzML.gz");
    assert_eq!(mapping[0]["name"], "x.mzML.gz");
}

#[test]
fn an_input_of_a_255_byte_name_is_searched_and_maps_back_to_its_file() {
    let run = Run::new();
    let i = inputs();
    let name = format!("{}.mzML", "n".repeat(250));
    let long = i.dir.write(&format!("long/{name}"), "<mzML/>");
    let (id, rx) = run.start(request(&[&long], &i.fasta, false, json!({})), None);
    let id = id.unwrap();
    let folder = done_folder(&until_terminal(&rx, WAIT));
    let mapping: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("inputs.json")).unwrap()).unwrap();
    let link = mapping[0]["link"].as_str().unwrap().to_string();
    let stem = mapping[0]["stem"].as_str().unwrap().to_string();
    assert_eq!(mapping[0]["name"], name.as_str());
    assert_eq!(mapping[0]["path"], long.to_str().unwrap());
    assert_eq!(stem, input_stem(&link));
    // the stem as long as the folder allows: MAX_STEM, on Windows less (MAX_PATH)
    let partial = run.dir().join(format!("{id}.partial"));
    let max = max_stem(InputLinks::PLATFORM, &partial);
    assert!(max <= MAX_STEM && max > 20, "{max}");
    assert!(link.len() <= 255 && stem.len() == max, "{link}");
    assert!(
        link.starts_with("1__nnn") && link.ends_with(".mzML"),
        "{link}"
    );
    if cfg!(windows) {
        // a hard link (the name is too long for ProSE's outputs), which the finished search drops
        assert!(!folder.join(&link).exists(), "{link} is gone");
    } else {
        assert!(fs::symlink_metadata(folder.join(&link))
            .unwrap()
            .file_type()
            .is_symlink());
    }
    // the reader finds the input's outputs by the stem inputs.json records
    assert!(folder
        .join(format!("qpx/runs/{stem}.psm.parquet"))
        .is_file());
    // Windows: such an input goes as a (shortened) link, not by its own path
    let req = request(&[&long, &i.mzml], &i.fasta, false, json!({}));
    let windows =
        Path::new(r"C:\Users\u\AppData\Roaming\org.openms.prose.desktop\searches\1-0.partial");
    assert_eq!(
        input_placements(&req, InputLinks::PathsOrHardLinks, windows),
        vec![Placement::HardLink, Placement::OwnPath]
    );
    let records = input_records_with(&req, InputLinks::PathsOrHardLinks, windows);
    assert_eq!(
        records[0].stem.len(),
        max_stem(InputLinks::PathsOrHardLinks, windows)
    );
    assert!(records[0].link.starts_with("1__nnn") && records[0].link.ends_with(".mzML"));
    assert_eq!(records[1].link, "a.mzML");
}

/// Windows' MAX_PATH (259 characters): the stems of the links are as short as the search folder's path needs,
/// so that ProSE's longest output (`mods\<stem>_ModificationAnalysis_DeltaMassStats.tsv`) and every link stay
/// within it; a name that fits MAX_STEM but not the folder goes as a shortened link too; a folder of too long a
/// path is refused before anything is made.
#[test]
fn on_windows_the_search_folders_paths_stay_within_max_path() {
    let windows = InputLinks::PathsOrHardLinks;
    let dir = TempDir::new("maxpath");
    let fasta = dir.write("db.fasta", ">a\nMK\n");
    // a name of 150 bytes: within MAX_STEM, too long under a folder of 100 characters
    let name = format!("{}.mzML", "m".repeat(145));
    let long = dir.write(&format!("in/{name}"), "<mzML/>");
    let short = dir.write("in/b.mzML", "<mzML/>");
    let twin = dir.write("other/b.mzML", "<mzML/>");
    let req = request(&[&long, &short, &twin], &fasta, false, json!({}));
    let folder = format!(
        r"C:\{}\searches\1792841343610-0.partial",
        "d".repeat(100 - 36)
    );
    let folder = Path::new(&folder);
    assert_eq!(folder.as_os_str().len(), 100);
    let max = max_stem(windows, folder);
    assert_eq!(
        max,
        259 - 100 - r"\mods\".len() - "_ModificationAnalysis_DeltaMassStats.tsv".len()
    );
    // elsewhere only the file name limit counts
    assert_eq!(max_stem(InputLinks::Symlinks, folder), MAX_STEM);
    let records = input_records_with(&req, windows, folder);
    assert_eq!(
        input_placements(&req, windows, folder),
        [
            Placement::HardLink,
            Placement::HardLink,
            Placement::HardLink
        ]
    );
    let links: Vec<&str> = records.iter().map(|r| r.link.as_str()).collect();
    assert_eq!(links[1..], ["2__b.mzML", "3__b.mzML"]);
    assert_eq!(records[0].stem.len(), max);
    assert!(links[0].starts_with("1__mmm") && links[0].ends_with(".mzML"));
    assert_eq!(records[0].name, name);
    for r in &records {
        let longest = format!(
            r"{}\mods\{}_ModificationAnalysis_DeltaMassStats.tsv",
            folder.display(),
            r.stem
        );
        let link = format!(r"{}\{}", folder.display(), r.link);
        assert!(
            longest.encode_utf16().count() <= 259 && link.encode_utf16().count() <= 259,
            "{longest}"
        );
    }
    let args = build_args_with(&req, folder, windows).unwrap();
    assert_eq!(
        values_of(&args, "-in")[0],
        folder.join(&records[0].link).into_os_string()
    );
    // a multi-byte name: the stem's bytes bound its UTF-16 units
    let umlauts = dir.write(&format!("in/{}.mzML", "ä".repeat(100)), "<mzML/>");
    let req = request(&[&umlauts], &fasta, false, json!({}));
    let r = &input_records_with(&req, windows, folder)[0];
    assert!(
        r.stem.len() <= max && r.link.ends_with(".mzML"),
        "{}",
        r.link
    );
    // a name that fits stays the user's own path
    let req = request(&[&short], &fasta, false, json!({}));
    assert_eq!(
        input_placements(&req, windows, folder),
        [Placement::OwnPath]
    );
    // a folder that leaves no room for ProSE's outputs is refused
    let deep = format!(r"C:\{}\1-0.partial", "d".repeat(220));
    let err = build_args_with(&req, Path::new(&deep), windows).unwrap_err();
    assert!(
        err.contains("too long a path") && err.contains("259"),
        "{err}"
    );
}

/// Windows' way (`InputLinks::PathsOrHardLinks`), run on every platform: inputs of different names go to
/// the tool by their own paths, and nothing of them is in the search folder.
#[test]
fn inputs_of_different_names_go_to_the_tool_by_their_own_paths() {
    let run = Run::with_links(InputLinks::PathsOrHardLinks);
    let i = inputs();
    let b = i.dir.write("other/b.mzML.gz", "gz");
    let (id, rx) = run.start(request(&[&i.mzml, &b], &i.fasta, false, json!({})), None);
    let id = id.unwrap();
    let folder = done_folder(&until_terminal(&rx, WAIT));
    assert_eq!(folder, run.dir().join(&id));
    let argv = fs::read_to_string(folder.join("argv.txt")).unwrap();
    let ins: Vec<&str> = argv.lines().skip(1).take(2).collect();
    assert_eq!(
        ins.iter().map(PathBuf::from).collect::<Vec<_>>(),
        [i.mzml.clone(), b.clone()],
        "{argv}"
    );
    assert!(
        folder.join("qpx/runs/a.psm.parquet").is_file()
            && folder.join("qpx/runs/b.psm.parquet").is_file()
    );
    let mapping: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("inputs.json")).unwrap()).unwrap();
    assert_eq!(
        mapping,
        json!([
            { "id": "f0", "index": 0, "link": "a.mzML", "stem": "a", "name": "a.mzML", "path": i.mzml },
            { "id": "f1", "index": 1, "link": "b.mzML.gz", "stem": "b", "name": "b.mzML.gz", "path": b },
        ])
    );
    let names: Vec<String> = fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names
            .iter()
            .any(|n| n.ends_with(".mzML") || n.ends_with(".gz")),
        "{names:?}"
    );
}

/// Windows' way, run on every platform: two Bruker folders of one name would need links in the search folder,
/// which Windows makes of files only; the search is refused before anything runs, and says to rename one.
#[test]
fn bruker_folders_of_one_name_are_refused_with_windows_links() {
    let run = Run::with_links(InputLinks::PathsOrHardLinks);
    let i = inputs();
    let first = i.dir.path().join("run1/sample.d");
    let second = i.dir.path().join("run2/sample.d");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    let (id, _rx) = run.start(
        request(&[&first, &second], &i.fasta, false, json!({})),
        None,
    );
    let e = id.unwrap_err();
    assert!(
        e.contains("Bruker folder") && e.contains("same name") && e.contains("Rename the folder"),
        "{e}"
    );
}

/// Windows' way, run on every platform: two inputs of one name go to the tool as hard links in the
/// search folder, which are gone when the search is done; the user's files stay as they are.
#[test]
fn inputs_of_one_name_go_as_hard_links_that_the_finished_search_drops() {
    let run = Run::with_links(InputLinks::PathsOrHardLinks);
    let i = inputs();
    let first = i.dir.write("run1/sample.mzML", "<mzML>first</mzML>");
    let second = i.dir.write("run2/Sample.mzML", "<mzML>second</mzML>");
    let (id, rx) = run.start(
        request(&[&first, &second], &i.fasta, false, json!({})),
        None,
    );
    let id = id.unwrap();
    let folder = done_folder(&until_terminal(&rx, WAIT));
    assert_eq!(folder, run.dir().join(&id));
    let resolved = fs::read_to_string(folder.join("inputs.txt")).unwrap();
    let partial = run.dir().join(format!("{id}.partial"));
    for (n, name) in [(1, "1__sample.mzML"), (2, "2__Sample.mzML")] {
        let line = resolved.lines().nth(n - 1).unwrap_or_default();
        assert!(
            line.starts_with(&format!("{} ->", partial.join(name).display())),
            "{resolved}"
        );
        assert!(!folder.join(name).exists(), "{name} is gone");
    }
    assert!(
        folder.join("qpx/runs/1__sample.psm.parquet").is_file()
            && folder.join("qpx/runs/2__Sample.psm.parquet").is_file()
    );
    assert_eq!(fs::read_to_string(&first).unwrap(), "<mzML>first</mzML>");
    assert_eq!(fs::read_to_string(&second).unwrap(), "<mzML>second</mzML>");
    let mapping: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("inputs.json")).unwrap()).unwrap();
    assert_eq!(mapping[0]["link"], "1__sample.mzML");
    assert_eq!(mapping[1]["stem"], "2__Sample");
    assert_eq!(mapping[1]["path"], json!(second));
}

/// Windows' way: a hard link cannot leave its volume. Two inputs of one name, one of them on another
/// volume than the searches folder (on the CI runner: the checkout on D:, the temporary folder on C:),
/// are refused with a message that asks to rename one; nothing is copied, no folder is left.
#[test]
fn inputs_of_one_name_on_another_volume_are_refused() {
    let elsewhere =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("volume-{}", std::process::id()));
    fs::create_dir_all(&elsewhere).unwrap();
    let far = elsewhere.join("sample.mzML");
    fs::write(&far, "<mzML>far</mzML>").unwrap();
    let run = Run::with_links(InputLinks::PathsOrHardLinks);
    fs::create_dir_all(run.dir()).unwrap();
    let probe = run.dir().join("probe");
    let same_volume = fs::hard_link(&far, &probe).is_ok();
    let _ = fs::remove_file(&probe);
    if same_volume {
        let _ = fs::remove_dir_all(&elsewhere);
        eprintln!(
            "skipped: {} and {} are on one volume",
            elsewhere.display(),
            run.dir().display()
        );
        return;
    }
    let i = inputs();
    let near = i.dir.write("near/sample.mzML", "<mzML>near</mzML>");
    let (result, _rx) = run.start(request(&[&near, &far], &i.fasta, false, json!({})), None);
    let err = result.unwrap_err();
    let _ = fs::remove_dir_all(&elsewhere);
    assert!(
        err.contains("Two inputs are named sample.mzML")
            && err.contains("Rename one of the files")
            && err.contains(&far.display().to_string()),
        "{err}"
    );
    assert!(
        run.listing().is_empty(),
        "no folder is left: {:?}",
        run.listing()
    );
    // a lone input of stem quantms on the other volume: its own reason
    let reserved = elsewhere.join("QuantMS.mzML");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(&reserved, "<mzML/>").unwrap();
    let (result, _rx) = run.start(request(&[&reserved], &i.fasta, false, json!({})), None);
    let err = result.unwrap_err();
    let _ = fs::remove_dir_all(&elsewhere);
    assert!(
        err.contains("ProSE cannot search a file named QuantMS.mzML")
            && err.contains("Rename the file")
            && !err.contains("Two inputs"),
        "{err}"
    );
    // a lone input whose name is too long for ProSE's outputs in the search folder: its own reason too
    let long = elsewhere.join(format!("{}.mzML", "l".repeat(240)));
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(&long, "<mzML/>").unwrap();
    let (result, _rx) = run.start(request(&[&long], &i.fasta, false, json!({})), None);
    let err = result.unwrap_err();
    let _ = fs::remove_dir_all(&elsewhere);
    assert!(
        err.contains("is too long for ProSE's result files")
            && err.contains("Rename the file to a shorter name")
            && !err.contains("Two inputs"),
        "{err}"
    );
}

/// ProSE's lines of a full disk: the first one is the message, with the outputs named as in the search folder
/// (which is gone) and after the user's file.
#[test]
fn a_failure_names_the_cause_and_the_outputs_without_the_search_folder() {
    let run = Run::new();
    let i = inputs();
    let full = i.dir.write("in/diskfull.mzML", "<mzML/>");
    let (id, rx) = run.start(request(&[&full], &i.fasta, false, json!({})), None);
    id.unwrap();
    let events = until_terminal(&rx, WAIT);
    let Some(SearchEvent::Failed(f)) = events.last() else {
        panic!("{events:?}")
    };
    assert_eq!(
        f.message,
        "Failed to write qpx/runs/diskfull.psm.parquet: IOError: Error writing bytes to file. Detail: [errno 28] No \
         space left on device"
    );
    assert_eq!(f.ended.as_deref(), Some("ProSE ended with exit code 12"));
    let detail = f.detail.as_deref().unwrap();
    assert!(
        detail.ends_with("ProSE finished with 1 file(s) failing out of 1.")
            && !detail.contains(".partial"),
        "{detail}"
    );
}

#[test]
fn humanize_gives_the_users_names_and_drops_the_search_folder() {
    let dir = Path::new("/s/1-0.partial");
    let req = request(
        &[Path::new("/data/a.mzML"), Path::new("/other/a.mzML")],
        Path::new("/data/db.fasta"),
        false,
        json!({}),
    );
    let records = input_records_with(&req, InputLinks::Symlinks, dir);
    assert_eq!(
        humanize(
            "Reading /s/1-0.partial/2__a.mzML; writing /s/1-0.partial/qpx/runs/2__a.pg.parquet: row 3 reuses run '1__a'; \
             1__a.mzML; 11__a stays",
            dir,
            &records
        ),
        "Reading /other/a.mzML; writing qpx/runs/a.pg.parquet: row 3 reuses run 'a'; a.mzML; 11__a stays"
    );
    // a Windows search folder, as ProSE may write it: with '\\', with '/' (a path it put together), mixed, and
    // the drive letter in the other case
    let dir = Path::new(r"C:\Users\u\AppData\Roaming\prose\searches\1-0.partial");
    let req = request(
        // (the inputs with '/', which is a separator on every platform)
        &[Path::new("D:/data/a.mzML"), Path::new("D:/other/a.mzML")],
        Path::new("D:/data/db.fasta"),
        false,
        json!({}),
    );
    let records = input_records_with(&req, InputLinks::PathsOrHardLinks, dir);
    assert_eq!(
        humanize(
            r"Reading C:\Users\u\AppData\Roaming\prose\searches\1-0.partial\2__a.mzML; Reading c:/Users/u/AppData/Roaming/prose/searches/1-0.partial/1__a.mzML; writing C:\Users\u\AppData\Roaming\prose\searches\1-0.partial\qpx/runs/2__a.pg.parquet and C:/Users/u/AppData/Roaming/prose/searches/1-0.partial/merged.idXML",
            dir,
            &records
        ),
        "Reading D:/other/a.mzML; Reading D:/data/a.mzML; writing qpx/runs/a.pg.parquet and merged.idXML"
    );
}

#[test]
fn a_crash_becomes_search_failed_with_the_last_stage() {
    let run = Run::new();
    let i = inputs();
    let crash = i.dir.write("in/crash.mzML", "<mzML/>");
    let (id, rx) = run.start(
        request(&[&crash, &i.mzml], &i.fasta, false, json!({})),
        None,
    );
    let id = id.unwrap();
    let events = until_terminal(&rx, WAIT);
    let Some(SearchEvent::Failed(f)) = events.last() else {
        panic!("{events:?}")
    };
    assert_eq!(f.search_id, id);
    assert_eq!(
        f.stage, "Rescoring with Percolator...",
        "the last progress label"
    );
    assert_eq!(f.file, Some(0), "the input of the last report");
    assert_eq!(
        f.message,
        format!(
            "Error: Unable to read file (the input broke {})",
            crash.display()
        ),
        "the message names the user's file, not the link; noise lines are skipped"
    );
    // how ProSE ended leads the message; the stderr tail is the detail, the log is kept
    assert_eq!(f.ended.as_deref(), Some("ProSE ended with exit code 3"));
    assert_eq!(
        f.detail.as_deref(),
        Some(
            format!(
                "Error: Unable to read file (the input broke {})",
                crash.display()
            )
            .as_str()
        )
    );
    let log = PathBuf::from(f.log.as_deref().expect("the log is kept"));
    assert_eq!(log, run.work.path().join("logs").join("last-failure.log"));
    let text = fs::read_to_string(&log).unwrap();
    assert!(text.starts_with("$ "), "the command line first: {text}");
    assert!(text.contains("Error: Unable to read file"), "{text}");
    assert!(
        run.listing().is_empty(),
        "the partial folder is removed: {:?}",
        run.listing()
    );
}

#[test]
fn a_crash_by_a_signal_says_so_and_keeps_the_log_of_the_last_failure() {
    let run = Run::new();
    let i = inputs();
    // a failure before: its log is replaced
    let crash = i.dir.write("in/crash.mzML", "<mzML/>");
    let (_, rx) = run.start(request(&[&crash], &i.fasta, false, json!({})), None);
    until_terminal(&rx, WAIT);
    let segv = i.dir.write("in/segv.mzML", "<mzML/>");
    let (_, rx) = run.start(request(&[&segv], &i.fasta, false, json!({})), None);
    let events = until_terminal(&rx, WAIT);
    let Some(SearchEvent::Failed(f)) = events.last() else {
        panic!("{events:?}")
    };
    let expected = if cfg!(windows) {
        "ProSE crashed (exception 0xC0000005)"
    } else {
        "ProSE crashed (signal 11, SIGSEGV)"
    };
    assert_eq!(f.ended.as_deref(), Some(expected));
    // the last line of a crashed tool says nothing about the crash (Percolator's "Split 3:" after a SIGKILL):
    // it is in the details only
    assert_eq!(f.message, "", "no message of its own");
    assert_eq!(
        f.detail.as_deref(),
        Some(format!("Reading {} ...", segv.display()).as_str())
    );
    let text = fs::read_to_string(f.log.as_deref().unwrap()).unwrap();
    assert!(text.contains("segv.mzML"), "{text}");
    assert!(!text.contains("crash.mzML"), "the last failure's: {text}");
    let v = serde_json::to_value(f).unwrap();
    assert_eq!(v["ended"], expected);
    assert!(v["detail"].is_string() && v["log"].is_string());
}

#[test]
fn a_failure_reaches_the_front_end_with_camel_case_fields() {
    let run = Run::new();
    let i = inputs();
    let crash = i.dir.write("in/crash.mzML", "<mzML/>");
    let (_, rx) = run.start(request(&[&crash], &i.fasta, false, json!({})), None);
    let events = until_terminal(&rx, WAIT);
    let Some(SearchEvent::Failed(f)) = events.last() else {
        panic!("{events:?}")
    };
    let v = serde_json::to_value(f).unwrap();
    assert_eq!(v["searchId"], f.search_id.as_str());
    assert!(v.get("stage").is_some() && v.get("message").is_some());
    assert_eq!(v["file"], 0);
    let p = serde_json::to_value(match &events[0] {
        SearchEvent::Progress(p) => p,
        e => panic!("{e:?}"),
    })
    .unwrap();
    assert!(
        p.get("searchId").is_some()
            && p.get("label").is_some()
            && p.get("file").is_some()
            && p.get("fraction").is_some()
    );
}

#[test]
fn invalid_requests_fail_before_anything_is_created() {
    let run = Run::new();
    let i = inputs();
    let (id, _rx) = run.start(
        request(&[&i.mzml], &i.fasta, false, json!({ "bogus": 1 })),
        None,
    );
    assert!(id.unwrap_err().contains("bogus"));
    let (id, _rx) = run.start(request(&[&i.mzml], &i.fasta, true, json!({})), None);
    assert!(
        id.unwrap_err().to_lowercase().contains("contaminant"),
        "no cRAP file given"
    );
    assert!(run.listing().is_empty());
}

#[test]
fn a_tool_that_cannot_start_is_an_error_and_leaves_no_folder() {
    let run = Run::new();
    let i = inputs();
    let (sink, _rx) = channel_sink();
    let err = run
        .searches
        .start(
            Path::new("/nonexistent/ProSE"),
            &run.dir(),
            None,
            request(&[&i.mzml], &i.fasta, false, json!({})),
            sink,
        )
        .unwrap_err();
    assert!(err.contains("/nonexistent/ProSE"), "{err}");
    assert!(run.listing().is_empty());
}

fn start_hanging(
    run: &Run,
    i: &Inputs,
    name: &str,
) -> (String, std::sync::mpsc::Receiver<SearchEvent>, PathBuf) {
    let hang = i.dir.write(&format!("in/{name}.mzML"), "<mzML/>");
    let (id, rx) = run.start(request(&[&hang], &i.fasta, false, json!({})), None);
    let id = id.unwrap();
    first_progress(&rx, WAIT);
    let partial = run.dir().join(format!("{id}.partial"));
    assert!(wait_for(WAIT, || partial.join("children.pid").is_file()
        && read_pids(&partial.join("children.pid")).len() == 2));
    (id, rx, partial)
}

#[test]
fn a_running_search_holds_the_lock_of_its_partial_folder_and_the_sweep_leaves_it() {
    use prose_desktop_lib::history::{sweep, SWEEP_GRACE};
    let run = Run::new();
    let i = inputs();
    let (id, _rx, partial) = start_hanging(&run, &i, "hang");
    // another app instance's startup sweep, however late: the partial folder is live
    let much_later = std::time::SystemTime::now() + SWEEP_GRACE * 1000;
    assert!(sweep(&run.dir(), much_later, SWEEP_GRACE).is_empty());
    assert!(partial.join("prose.log").exists() || partial.join("prose.pid").exists());
    // the lock is taken, by the search
    let file = fs::File::options()
        .read(true)
        .write(true)
        .open(partial.join(".lock"))
        .unwrap();
    assert!(file.try_lock().is_err(), "the search holds the lock");
    drop(file);
    // cancelled: the folder is removed, and the lock goes with it
    run.searches.cancel(&id).unwrap();
    assert!(!partial.exists());
}

#[test]
fn cancel_ends_the_process_and_its_children() {
    let run = Run::new();
    let i = inputs();
    let (id, rx, partial) = start_hanging(&run, &i, "hang");
    let tool_pid = read_pids(&partial.join("prose.pid"))[0];
    let children = read_pids(&partial.join("children.pid"));
    assert!(process_alive(tool_pid) && children.iter().all(|p| process_alive(*p)));

    let started = std::time::Instant::now();
    run.searches.cancel(&id).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "SIGTERM was enough: {:?}",
        started.elapsed()
    );

    assert!(!process_alive(tool_pid), "the tool is gone");
    assert!(
        wait_for(Duration::from_secs(5), || children
            .iter()
            .all(|p| !process_alive(*p))),
        "its children are gone"
    );
    assert!(
        run.listing().is_empty(),
        "the partial folder is removed: {:?}",
        run.listing()
    );
    // a cancelled search sends neither done nor failed
    assert!(rx.try_iter().all(|e| matches!(e, SearchEvent::Progress(_))));
    // cancelling a finished or unknown search is not an error
    run.searches.cancel(&id).unwrap();
    run.searches.cancel("nonexistent").unwrap();
}

/// The watchdogs (`--watch-search <this process> <group>`) of the tool's process `group`, by `ps`.
#[cfg(unix)]
fn watchdogs_of(group: u32) -> Vec<String> {
    let out = std::process::Command::new("ps")
        .args(["-A", "-ww", "-o", "pid=,command="])
        .output()
        .unwrap();
    let needle = format!(
        "{} {} {group}",
        prose_desktop_lib::watchdog::FLAG,
        std::process::id()
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.ends_with(&needle))
        .map(str::to_string)
        .collect()
}

/// (macOS's way; the code is the same on every Unix) a watched search starts the watchdog of its tool's
/// group with the tool, and ends it with the search.
#[cfg(unix)]
#[test]
fn a_watched_search_starts_its_watchdog_and_ends_it_with_the_search() {
    let run = Run {
        work: TempDir::new("run"),
        searches: Arc::new(
            Searches::default().with_watchdog(PathBuf::from(env!("CARGO_BIN_EXE_prose-desktop"))),
        ),
    };
    let i = inputs();
    let (id, _rx, partial) = start_hanging(&run, &i, "hang");
    let tool_pid = read_pids(&partial.join("prose.pid"))[0];
    assert!(
        wait_for(WAIT, || watchdogs_of(tool_pid).len() == 1),
        "one watchdog of the tool's group: {:?}",
        watchdogs_of(tool_pid)
    );
    run.searches.cancel(&id).unwrap();
    assert!(
        watchdogs_of(tool_pid).is_empty(),
        "the watchdog ended with the search"
    );
    // a search that finishes ends its watchdog too
    let (id, rx) = run.start(request(&[&i.mzml], &i.fasta, false, json!({})), None);
    let id = id.unwrap();
    let folder = done_folder(&until_terminal(&rx, WAIT));
    let pid = read_pids(&folder.join("prose.pid"))[0];
    assert!(watchdogs_of(pid).is_empty(), "{id}");
}

#[test]
fn cancel_kills_a_process_that_ignores_sigterm() {
    let run = Run::new();
    let i = inputs();
    let (id, _rx, partial) = start_hanging(&run, &i, "stubborn");
    let tool_pid = read_pids(&partial.join("prose.pid"))[0];
    let children = read_pids(&partial.join("children.pid"));
    run.searches.cancel(&id).unwrap();
    assert!(!process_alive(tool_pid));
    assert!(wait_for(Duration::from_secs(5), || children
        .iter()
        .all(|p| !process_alive(*p))));
    assert!(run.listing().is_empty());
}

#[test]
fn cancel_all_stops_every_running_search() {
    let run = Run::new();
    let i = inputs();
    let (_, _rx1, p1) = start_hanging(&run, &i, "hang");
    let (_, _rx2, p2) = start_hanging(&run, &i, "stubborn");
    let pids: Vec<u32> = [&p1, &p2]
        .iter()
        .flat_map(|p| read_pids(&p.join("prose.pid")))
        .collect();
    run.searches.cancel_all();
    assert!(pids.iter().all(|p| !process_alive(*p)));
    assert!(run.listing().is_empty());
}

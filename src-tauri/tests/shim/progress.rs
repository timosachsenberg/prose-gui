//! ProSE's log lines (fixtures of real runs) as progress events.

use super::*;
use prose_desktop_lib::progress::ProgressTracker;

type Step = (String, i32, f64);

fn steps_of(fixture_name: &str, names: &[&str]) -> Vec<Step> {
    let mut tracker = ProgressTracker::new(names.iter().map(|n| n.to_string()).collect());
    let mut steps = Vec::new();
    for line in fixture(fixture_name).lines() {
        if let Some(p) = tracker.feed(line) {
            steps.push((p.label, p.file, p.fraction));
        }
    }
    steps
}

fn labels(steps: &[Step]) -> Vec<&str> {
    steps.iter().map(|s| s.0.as_str()).collect()
}

fn assert_monotone(steps: &[Step]) {
    for pair in steps.windows(2) {
        assert!(pair[1].2 >= pair[0].2, "the fraction goes back: {pair:?}");
    }
    assert!(
        steps.iter().all(|s| (0.0..=1.0).contains(&s.2)),
        "{steps:?}"
    );
}

#[test]
fn progress_lines_become_events_with_file_and_fraction() {
    // real output of the pinned ProSE: two files, PeptDeep and Percolator on, closed search
    let steps = steps_of(
        "prose-closed-two-files-peptdeep.stdout.txt",
        &["DatabaseSuitability_1.mzML", "DatabaseSuitability_2.mzML"],
    );
    assert_eq!(
        labels(&steps),
        [
            "Searching DatabaseSuitability_1.mzML (1 of 2)...",
            "Building the fragment index...",
            "Searching DatabaseSuitability_1.mzML (1 of 2)...",
            "Adding PeptDeep rescoring features...",
            "Mapping peptides to proteins...",
            "Rescoring with Percolator...",
            "Searching DatabaseSuitability_2.mzML (2 of 2)...",
            "Adding PeptDeep rescoring features...",
            "Mapping peptides to proteins...",
            "Rescoring with Percolator...",
            // (a ProSE without -qpx_extended: the views' lines are not there)
            "Preparing the spectrum view (file 1 of 2)...",
            "Pooling the files...",
            "Writing the results...",
        ]
    );
    let files: Vec<i32> = steps.iter().map(|s| s.1).collect();
    assert_eq!(files, [0, 0, 0, 0, 0, 0, 1, 1, 1, 1, -1, -1, -1]);
    assert_eq!(steps[0].2, 0.0, "the first file starts at 0");
    // the second file's step starts where the first one's share ends: half of the 35 % for the files' search
    assert!((steps[6].2 - 0.175).abs() < 1e-9, "{:?}", steps[6]);
    // the spectrum views follow the files' search, not 95 %
    assert!((steps[10].2 - 0.35).abs() < 1e-9, "{:?}", steps[10]);
    assert!(
        (steps[11].2 - 0.95).abs() < 1e-9,
        "pooling holds at 95 %: {:?}",
        steps[11]
    );
    assert_monotone(&steps);
}

#[test]
fn progress_names_the_file_by_the_original_name_not_the_symlink() {
    // the fixture names the files "1__DatabaseSuitability_1.mzML" (the shim's link); the label shows the user's name
    let steps = steps_of(
        "prose-closed-two-files-peptdeep.stdout.txt",
        &["First run.mzML", "Zweite Probe ä.mzML.gz"],
    );
    assert_eq!(steps[0].0, "Searching First run.mzML (1 of 2)...");
    assert!(labels(&steps).contains(&"Searching Zweite Probe ä.mzML.gz (2 of 2)..."));
    // without names, the line's own file name is used
    let plain = steps_of("prose-closed-two-files-peptdeep.stdout.txt", &[]);
    assert_eq!(
        plain[0].0,
        "Searching 1__DatabaseSuitability_1.mzML (1 of 2)..."
    );
}

#[test]
fn progress_of_a_single_open_search_ends_with_the_modification_analysis() {
    let steps = steps_of(
        "prose-open-one-file.stdout.txt",
        &["DatabaseSuitability_1.mzML"],
    );
    assert_eq!(
        labels(&steps),
        [
            "Searching DatabaseSuitability_1.mzML (1 of 1)...",
            "Building the fragment index...",
            "Searching DatabaseSuitability_1.mzML (1 of 1)...",
            "Mapping peptides to proteins...",
            "Rescoring with Percolator...",
            "Analysing modifications...",
            "Writing the results...",
        ]
    );
    assert_eq!(steps.last().unwrap().1, -1);
    assert_monotone(&steps);
}

#[test]
fn progress_of_several_open_files_pools_after_the_last_modification_analysis() {
    let steps = steps_of("prose-open-two-files.stdout.txt", &["a.mzML", "b.mzML"]);
    let l = labels(&steps);
    assert_eq!(
        &l[l.len() - 4..],
        [
            "Analysing modifications...",
            "Preparing the spectrum view (file 1 of 2)...",
            "Pooling the files...",
            "Writing the results..."
        ]
    );
    assert_eq!(
        l.iter()
            .filter(|x| **x == "Analysing modifications...")
            .count(),
        2
    );
    assert_eq!(
        steps
            .iter()
            .find(|s| s.0 == "Pooling the files...")
            .unwrap()
            .1,
        -1
    );
    assert_monotone(&steps);
}

const CHUNK_NAMES: [&str; 2] = ["a.mzML", "b.mzML"];

fn chunk_labels(n: usize) -> Vec<String> {
    (1..=n)
        .map(|k| format!("Searching database chunk {k} of {n}..."))
        .collect()
}

#[test]
fn a_chunked_search_of_two_files_reports_its_chunks_and_each_files_rescoring() {
    // real output: `database:chunk_size 20` on two inputs prints no "Searching" line (chunk-major mode)
    let steps = steps_of("prose-chunked-two-files-peptdeep.stdout.txt", &CHUNK_NAMES);
    let mut expected = vec!["Reading the spectra...".to_string()];
    expected.extend(chunk_labels(17));
    expected.extend(
        [
            "Adding PeptDeep rescoring features...",
            "Mapping peptides to proteins...",
            "Rescoring with Percolator...",
            "Adding PeptDeep rescoring features...",
            "Mapping peptides to proteins...",
            "Rescoring with Percolator...",
            // (no end marker of the last file in a closed chunked search, and this ProSE had no -qpx_extended)
            "Pooling the files...",
            "Writing the results...",
        ]
        .map(String::from),
    );
    assert_eq!(
        labels(&steps),
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );
    let files: Vec<i32> = steps.iter().map(|s| s.1).collect();
    assert!(
        files[..18].iter().all(|f| *f == -1),
        "reading and chunks are outside the files"
    );
    assert_eq!(files[18..], [0, 0, 0, 1, 1, 1, -1, -1]);
    // the chunks take the first half of the files' 35 % of the bar, in 17 equal steps
    assert_eq!(steps[1].2, 0.0);
    assert!(
        (steps[17].2 - 16.0 / 17.0 * 0.5 * 0.35).abs() < 1e-9,
        "{:?}",
        steps[17]
    );
    // the files' rescoring follows, file 1 after file 0
    assert!(
        steps[18].2 >= 0.175 && steps[23].2 > steps[20].2 && steps[23].2 < 0.35,
        "{steps:?}"
    );
    assert!((steps[24].2 - 0.95).abs() < 1e-9 && (steps[25].2 - 0.97).abs() < 1e-9);
    assert_monotone(&steps);
}

#[test]
fn a_chunked_search_of_one_file_reports_its_chunks() {
    let steps = steps_of(
        "prose-chunked-one-file-peptdeep.stdout.txt",
        &CHUNK_NAMES[..1],
    );
    let mut expected = vec!["Reading the spectra...".to_string()];
    expected.extend(chunk_labels(17));
    expected.extend(
        [
            "Adding PeptDeep rescoring features...",
            "Mapping peptides to proteins...",
            "Rescoring with Percolator...",
            "Writing the results...",
        ]
        .map(String::from),
    );
    assert_eq!(
        labels(&steps),
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(
        steps[18..21].iter().map(|s| s.1).collect::<Vec<_>>(),
        [0, 0, 0]
    );
    assert_eq!(steps.last().unwrap().1, -1);
    assert_monotone(&steps);
}

#[test]
fn a_chunked_open_search_pools_after_the_aggregate_modification_analysis() {
    // no PeptDeep: the mapping line starts each file's rescoring
    let steps = steps_of("prose-chunked-open-two-files.stdout.txt", &CHUNK_NAMES);
    let l = labels(&steps);
    assert_eq!(l[0], "Reading the spectra...");
    assert_eq!(l[1], "Searching database chunk 1 of 4...");
    assert_eq!(
        l[5..],
        [
            "Mapping peptides to proteins...",
            "Rescoring with Percolator...",
            "Mapping peptides to proteins...",
            "Rescoring with Percolator...",
            "Preparing the spectrum view (file 1 of 2)...",
            "Pooling the files...",
            "Writing the results...",
        ]
    );
    assert_eq!(
        steps[5..12].iter().map(|s| s.1).collect::<Vec<_>>(),
        [0, 0, 1, 1, -1, -1, -1]
    );
    assert_monotone(&steps);
}

#[test]
fn progress_never_stalls_on_lines_it_does_not_know() {
    // unknown lines between the known ones change nothing, in every mode
    for (fixture, names) in [
        (
            "prose-closed-two-files-peptdeep.stdout.txt",
            &CHUNK_NAMES[..],
        ),
        (
            "prose-chunked-two-files-peptdeep.stdout.txt",
            &CHUNK_NAMES[..],
        ),
        (
            "prose-chunked-one-file-peptdeep.stdout.txt",
            &CHUNK_NAMES[..1],
        ),
        ("prose-open-one-file.stdout.txt", &CHUNK_NAMES[..1]),
    ] {
        let plain = steps_of(fixture, names);
        let mut tracker = ProgressTracker::new(names.iter().map(|n| n.to_string()).collect());
        let mut noisy = Vec::new();
        for line in fixture_lines(fixture) {
            for junk in [
                "",
                "[ProSE] something new",
                "Chunk",
                "Chunk x/y",
                "[ProSE] Chunk-major",
                "Mapping",
                "[PeptDeepRescoring",
            ] {
                assert!(tracker.feed(junk).is_none(), "{junk:?} in {fixture}");
            }
            if let Some(p) = tracker.feed(&line) {
                noisy.push((p.label, p.file, p.fraction));
            }
        }
        assert_eq!(noisy, plain, "{fixture}");
    }
}

fn fixture_lines(name: &str) -> Vec<String> {
    fixture(name).lines().map(String::from).collect()
}

#[test]
fn a_chunked_search_pools_on_the_protein_inference_line_of_current_prose() {
    // Since OpenMS #10707 the pooled protein inference and FDR logs "[ProSE] Protein inference on …" (the recorded runs
    // have the earlier "[ProSE] Merged protein inference + FDR …"): the same steps
    let fixture_name = "prose-chunked-two-files-peptdeep.stdout.txt";
    let mut tracker = ProgressTracker::new(CHUNK_NAMES.iter().map(|n| n.to_string()).collect());
    let mut steps = Vec::new();
    let mut replaced = 0;
    for line in fixture_lines(fixture_name) {
        let line = if line.starts_with("[ProSE] Merged protein inference") {
            replaced += 1;
            "[ProSE] Protein inference on 1792 PSMs (2 of them up to FDR:protein_inference_PSM, not reported), protein \
             score PEP: 51 target and 3 decoy proteins."
                .to_string()
        } else {
            line
        };
        if let Some(p) = tracker.feed(&line) {
            steps.push((p.label, p.file, p.fraction));
        }
    }
    assert_eq!(replaced, 1);
    assert_eq!(steps, steps_of(fixture_name, &CHUNK_NAMES));
    assert!(labels(&steps).contains(&"Pooling the files..."));
}

#[test]
fn progress_reports_the_calibration_pass() {
    let steps = steps_of(
        "prose-calibration-one-file.stdout.txt",
        &["DatabaseSuitability_1.mzML"],
    );
    assert!(
        labels(&steps).contains(&"Running calibration pass..."),
        "{:?}",
        labels(&steps)
    );
    assert!(
        !labels(&steps).contains(&"Rescoring with Percolator..."),
        "rescoring was off in this run"
    );
    assert_monotone(&steps);
}

#[test]
fn lines_without_a_step_give_no_event() {
    let mut tracker = ProgressTracker::new(vec!["a.mzML".into()]);
    for line in [
        "",
        "Generating fragments...",
        "Collapsed 12 repeated peptidoform occurrences.",
        "random text",
        "[ProSE] open_search=false (precursor tolerance [-10, +10] ppm)",
    ] {
        assert!(tracker.feed(line).is_none(), "{line:?}");
    }
}

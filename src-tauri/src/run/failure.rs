//! What a failed search says: the tool's stderr, digested as it comes ([`StderrDigest`]: the lines a
//! message is made of, without OpenMS' source locations, ProSE's command-line hints in the app's words),
//! and how the tool ended ([`how_it_ended`]).

use super::sys;
use std::collections::VecDeque;
use std::process::ExitStatus;

/// How many of the tool's last stderr lines are kept for a failure message.
const STDERR_TAIL: usize = 60;
/// How many of the tool's error lines are kept (the first ones: a cause comes before its consequences).
const ERROR_LINES: usize = 20;
/// The longest line a failure shows (characters).
const MAX_SHOWN: usize = 1000;

fn is_noise(line: &str) -> bool {
    line.trim().is_empty() || line.starts_with("Schema error") || line.starts_with("Warning:")
}

/// ProSE's own summary of failed files: the last resort of a message.
fn is_summary(line: &str) -> bool {
    line.starts_with("ProSE finished with") || line.starts_with("ProSE failed to write")
}

/// Where a file could not be parsed (OpenMS' XML handlers: "While loading '<file>': ... (in line 4303 column 2)").
fn is_location(line: &str) -> bool {
    line.starts_with("While loading '")
}

/// A line that tells of an error: OpenMS' "Error: ..." of a tool that stops, and ProSE's and OpenMS' error
/// lines of a step that failed (a file that cannot be written or read, a value that is not valid).
fn is_error(line: &str) -> bool {
    const STARTS: &[&str] = &[
        "Error",
        "Invalid parameter",
        "Cannot ",
        "Unable to ",
        "Failed to ",
        "refusing ",
        "terminate called",
        "Percolator could rescore only some",
    ];
    const HAS: &[&str] = &[
        "No space left on device",
        "Disk quota exceeded",
        "File too large",
        "Read-only file system",
        "Permission denied",
        "std::bad_alloc",
        "could not be created",
        "could not be written",
    ];
    !is_summary(line)
        && (STARTS.iter().any(|s| line.starts_with(s)) || HAS.iter().any(|s| line.contains(s)))
}

/// ProSE's hints in command-line words, in the app's (the app has no command line).
const HINTS: &[(&str, &str)] = &[
    (
        "Use '-Search:rescore false', or",
        "Turn off Percolator rescoring, or",
    ),
    ("Set 'rescore' to 'false'", "Turn off Percolator rescoring"),
    (
        "Set -Search:decoy_prefix to",
        "Set the decoy prefix (decoy_prefix) to",
    ),
    ("Use '-Search:decoys ignore'", "Set decoys to ignore"),
    ("Use '-Search:decoys generate'", "Set decoys to generate"),
];

/// A line of the tool's stderr as a failure shows it: without OpenMS' source locations (paths of the
/// machine that built ProSE) and exception boilerplate, ProSE's command-line hints in the app's words, and
/// at most [`MAX_SHOWN`] characters.
fn clean_line(line: &str) -> String {
    let mut text = line.trim().to_string();
    // "/home/runner/.../XMLHandler.cpp(67): While loading ...": the source location of a message
    if let Some(at) = text.find("): ") {
        let head = &text[..at];
        if !head.contains(' ')
            && [".cpp(", ".h(", ".hpp(", ".cc("]
                .iter()
                .any(|e| head.contains(e))
        {
            text = text[at + 3..].to_string();
        }
    }
    // OpenMS' exceptions: "<message> (- due to that error of type Parse Error in: <source>@<line>-<function>)"
    if let Some(at) = text.find("due to that error of type ") {
        let kind = &text[at + "due to that error of type ".len()..];
        let kind = kind
            .split(" in:")
            .next()
            .unwrap_or(kind)
            .trim_end_matches(')')
            .trim();
        let head = text[..at]
            .trim_end_matches([' ', '-'])
            .trim_end_matches('(')
            .trim_end();
        text = format!("{head} ({kind})");
    } else if let Some(at) = text.rfind(" in: ") {
        // "... Please check the file! in: )", "... in: <source>@<line>-<function>": the location only
        let after = text[at + 5..].trim();
        if after.is_empty() || after == ")" || after.contains('@') {
            let head = &text[..at];
            let open = head.matches('(').count() > head.matches(')').count();
            text = format!(
                "{head}{}",
                if open && after.ends_with(')') {
                    ")"
                } else {
                    ""
                }
            );
        }
    }
    text = text.replace("( in line ", " (in line ");
    if let Some(rest) = text.strip_prefix("ArrowIOHelpers: ") {
        text = rest.to_string();
    }
    // the merged idXML of a search that Percolator could rescore only in part (ProSE.cpp): in plain words
    if text.contains("Percolator rescored only some of the input files")
        && text.starts_with("Cannot write the merged output")
    {
        text = "Percolator could rescore only some of the files, and their PSMs cannot be pooled with the search \
                engine scores of the others. Turn off Percolator rescoring, or leave out the files that could not \
                be rescored."
            .to_string();
    }
    // an InvalidValue's repetition of the message: "(the value '1 of 2 files rescored' was used but is not valid; ...)"
    if let Some(at) = text.find(" (the value '") {
        if text[at..].contains("' was used but is not valid") {
            text.truncate(at);
        }
    }
    for (from, to) in HINTS {
        text = text.replace(from, to);
    }
    if text.chars().count() > MAX_SHOWN {
        text = format!("{}…", text.chars().take(MAX_SHOWN).collect::<String>());
    }
    text
}

/// What a failure is told with, gathered from the tool's stderr as it comes: its last lines (the details)
/// and the lines a message is made of.
#[derive(Default)]
pub(super) struct StderrDigest {
    /// The last [`STDERR_TAIL`] lines (not noise), each at most [`MAX_SHOWN`] characters.
    pub(super) tail: VecDeque<String>,
    /// The first [`ERROR_LINES`] error lines ([`is_error`]), cleaned ([`clean_line`]).
    errors: Vec<String>,
    /// The last "Error..." line, with the parse location ([`is_location`]) that came last before it.
    fatal: Option<(String, Option<String>)>,
    location: Option<String>,
    /// ProSE's last summary of failed files.
    summary: Option<String>,
}

impl StderrDigest {
    pub(super) fn feed(&mut self, line: &str) {
        if is_noise(line) {
            return;
        }
        if self.tail.len() == STDERR_TAIL {
            self.tail.pop_front();
        }
        let shown: String = if line.chars().count() > MAX_SHOWN {
            format!("{}…", line.chars().take(MAX_SHOWN).collect::<String>())
        } else {
            line.to_string()
        };
        self.tail.push_back(shown);
        let clean = clean_line(line);
        if clean.starts_with("Error") {
            self.fatal = Some((clean.clone(), self.location.clone()));
        } else if is_location(&clean) {
            self.location = Some(clean.clone());
        } else if is_summary(&clean) {
            self.summary = Some(clean.clone());
        }
        if is_error(&clean) && self.errors.len() < ERROR_LINES {
            self.errors.push(clean);
        }
    }

    /// The message of a failure: OpenMS' last "Error..." line (one of reading a file with the place in the
    /// file before it), else the first error line ProSE wrote (the cause; the next ones are its consequences),
    /// else ProSE's summary ("ProSE finished with 1 file(s) failing out of 1."), else, for a tool that ended
    /// with an exit code, its last line. Empty for a tool killed by a signal without an error: its last line
    /// (Percolator's "Split 3:") would only mislead.
    pub(super) fn message(&self, signalled: bool) -> String {
        if let Some((error, location)) = &self.fatal {
            return match location {
                Some(location) if error.contains("Unable to read") => {
                    format!("{error}. {location}")
                }
                _ => error.clone(),
            };
        }
        if let Some(error) = self.errors.first().or(self.summary.as_ref()) {
            return error.clone();
        }
        if signalled {
            return String::new();
        }
        self.tail.back().map(|l| clean_line(l)).unwrap_or_default()
    }
}

/// The name of a signal that ends a process (Linux and macOS agree on these numbers).
fn signal_name(signal: i32) -> Option<&'static str> {
    Some(match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        4 => "SIGILL",
        6 => "SIGABRT",
        8 => "SIGFPE",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        15 => "SIGTERM",
        // the system's limits of a process (ulimit -t, -f)
        24 => "SIGXCPU",
        25 => "SIGXFSZ",
        _ => return None,
    })
}

/// How a tool that did not succeed ended, the lead of its failure: "ProSE crashed (signal 11, SIGSEGV)"
/// (SIGKILL: the system may have ended it for lack of memory), on Windows "ProSE crashed (exception
/// 0xC0000005)" for an exception's status, else "ProSE ended with exit code 3"; None for success.
pub fn how_it_ended(status: &ExitStatus) -> Option<String> {
    if status.success() {
        return None;
    }
    if let Some(signal) = sys::exit_signal(status) {
        let name = signal_name(signal)
            .map(|n| format!(", {n}"))
            .unwrap_or_default();
        let oom = match signal {
            9 => "; the system may have ended it for lack of memory",
            24 => "; it used more CPU time than the system allows it",
            25 => "; a file grew larger than the system allows, or the disk is full",
            _ => "",
        };
        return Some(format!("ProSE crashed (signal {signal}{name}{oom})"));
    }
    Some(match status.code() {
        // an NTSTATUS error (0xC…): an access violation, a stack overflow, ...
        Some(code) if cfg!(windows) && (code as u32) >= 0xC000_0000 => {
            format!("ProSE crashed (exception 0x{:08X})", code as u32)
        }
        Some(code) => format!("ProSE ended with exit code {code}"),
        None => "ProSE ended unexpectedly".to_string(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::how_it_ended;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    /// (wait statuses: a signal in the low seven bits, an exit code in the next byte)
    #[test]
    fn how_a_tool_ended_leads_its_failure() {
        assert_eq!(how_it_ended(&ExitStatus::from_raw(0)), None);
        assert_eq!(
            how_it_ended(&ExitStatus::from_raw(3 << 8)).as_deref(),
            Some("ProSE ended with exit code 3")
        );
        assert_eq!(
            how_it_ended(&ExitStatus::from_raw(11)).as_deref(),
            Some("ProSE crashed (signal 11, SIGSEGV)")
        );
        assert_eq!(
            how_it_ended(&ExitStatus::from_raw(6)).as_deref(),
            Some("ProSE crashed (signal 6, SIGABRT)")
        );
        assert_eq!(
            how_it_ended(&ExitStatus::from_raw(9)).as_deref(),
            Some("ProSE crashed (signal 9, SIGKILL; the system may have ended it for lack of memory)")
        );
        assert_eq!(
            how_it_ended(&ExitStatus::from_raw(31)).as_deref(),
            Some("ProSE crashed (signal 31)")
        );
        // the system's limits (ulimit -f, -t): a run with ulimit -f 50 ended with "signal 25" alone
        assert_eq!(
            how_it_ended(&ExitStatus::from_raw(25)).as_deref(),
            Some("ProSE crashed (signal 25, SIGXFSZ; a file grew larger than the system allows, or the disk is full)")
        );
        assert_eq!(
            how_it_ended(&ExitStatus::from_raw(24)).as_deref(),
            Some("ProSE crashed (signal 24, SIGXCPU; it used more CPU time than the system allows it)")
        );
    }
}

#[cfg(test)]
mod digest_tests {
    use super::super::{read_lines, MAX_LINE};
    use super::{clean_line, StderrDigest, MAX_SHOWN};

    fn digest(lines: &[&str]) -> StderrDigest {
        let mut d = StderrDigest::default();
        for l in lines {
            d.feed(l);
        }
        d
    }

    const CI: &str = "/home/runner/work/prose-gui/prose-gui/openms/src/openms/source/FORMAT/HANDLERS/XMLHandler.cpp";

    /// A corrupt mzML (in_truncated_mzML, multi_one_corrupt): the place in the file comes before OpenMS'
    /// error, and neither path of the machine that built ProSE is shown.
    #[test]
    fn a_file_that_cannot_be_read_says_where() {
        let location = format!("{CI}(67): While loading '/data/truncated.mzML': input ended before all started tags were ended; last tag started is 'spectrum'( in line 4303 column 2)");
        let error = format!("Error: Unable to read file (- due to that error of type Parse Error in: {CI}@68-void OpenMS::Internal::XMLHandler::fatalError(ActionMode, const std::string&, OpenMS::UInt, OpenMS::UInt) const)");
        let d = digest(&[
            &location,
            "[ProSE] Ion priors: 0 and 0 confident training PSMs in the two halves of the spectra, fewer than 100; the ion_prior_* features are 0 for this file.",
            &error,
        ]);
        assert_eq!(
            d.message(false),
            "Error: Unable to read file (Parse Error). While loading '/data/truncated.mzML': input ended before all \
             started tags were ended; last tag started is 'spectrum' (in line 4303 column 2)"
        );
        // an OpenMS error without a location: as it is, without its empty "in: "
        let fasta = digest(&["Error: Unable to read file (Error while parsing FASTA file! Only 1280 proteins could be read. Parsing next record failed. Please check the file! in: )"]);
        assert_eq!(
            fasta.message(false),
            "Error: Unable to read file (Error while parsing FASTA file! Only 1280 proteins could be read. Parsing next record failed. Please check the file!)"
        );
        let empty = digest(&[
            "Cannot read input file given from parameter '-in'!",
            "Error: File empty (the file '/data/empty.mzML' is empty)",
        ]);
        assert_eq!(
            empty.message(false),
            "Error: File empty (the file '/data/empty.mzML' is empty)"
        );
    }

    /// A full disk (devfull): the first line that says so, not ProSE's closing summary.
    #[test]
    fn a_full_disk_is_named() {
        let d = digest(&[
            "Progress of 'Predicting fragment intensities and retention times...':",
            "-- done [took 0.02 s (CPU), 0.00 s (Wall)] -- ",
            "Omitted writing of 4 peptide identifications due to empty hits.",
            "ArrowIOHelpers: Failed to write /s/1-0.partial/qpx/runs/1__a.psm.parquet: IOError: Error writing bytes to file. Detail: [errno 28] No space left on device",
            "Failed to write the QPX psm view -> /s/1-0.partial/qpx/runs/1__a.psm.parquet",
            "ArrowIOHelpers: Failed to write /s/1-0.partial/qpx/quantms.psm.parquet: IOError: Error writing bytes to file. Detail: [errno 28] No space left on device",
            "Failed to write the QPX psm view -> /s/1-0.partial/qpx/quantms.psm.parquet",
            "ProSE finished with 1 file(s) failing out of 1.",
        ]);
        assert_eq!(
            d.message(false),
            "Failed to write /s/1-0.partial/qpx/runs/1__a.psm.parquet: IOError: Error writing bytes to file. Detail: [errno 28] No space left on device"
        );
        let idxml = digest(&[
            "Skipping Percolator rescoring for /data/a.mzML: Only 6 PSMs, Percolator rescoring needs at least 100. Keeping the search engine scores.",
            "Percolator rescoring is on (Search:rescore) but no input file could be rescored; all outputs carry the search engine scores.",
            "Failed to write idXML output for /data/a.mzML -> /s/search.idXML: the file '/s/search.idXML' could not be created. writing the file failed (e.g. disk full or I/O error) (writing '/s/search.idXML' did not complete: a partial file may remain)",
            "ProSE finished with 1 file(s) failing out of 1.",
        ]);
        assert!(
            idxml
                .message(false)
                .starts_with("Failed to write idXML output for /data/a.mzML"),
            "{}",
            idxml.message(false)
        );
        // without a line that says more, ProSE's summary
        let summary = digest(&[
            "Omitted writing of 31 peptide identifications due to empty hits.",
            "ProSE finished with 1 file(s) failing out of 1.",
        ]);
        assert_eq!(
            summary.message(false),
            "ProSE finished with 1 file(s) failing out of 1."
        );
    }

    /// Percolator rescored one of two files: the merged idXML cannot be written. In the app's words.
    #[test]
    fn a_partial_rescoring_is_told_in_the_apps_words() {
        let d = digest(&[
            "-qpx_extended: no NCE selected for /s/1-4.partial/2__small.mzML; the predicted_* columns are null.",
            "Cannot write the merged output -> /s/1-4.partial/merged.idXML: Percolator rescored only some of the input files (see the warnings above), and their PSMs cannot be pooled with the search engine scores of the others. Per-file outputs were written. Use '-Search:rescore false', or leave out the files that could not be rescored. (the value '1 of 2 files rescored' was used but is not valid; Percolator rescored only some of the input files, and their PSMs cannot be pooled with the search engine scores of the others.)",
            "ProSE failed to write the merged idXML output -> /s/1-4.partial/merged.idXML.",
        ]);
        let message = d.message(false);
        assert!(
            message.starts_with("Percolator could rescore only some of the files"),
            "{message}"
        );
        assert!(
            message.contains("Turn off Percolator rescoring"),
            "{message}"
        );
        assert!(
            !message.contains("-Search:") && !message.contains("partial"),
            "{message}"
        );
        assert_eq!(
            clean_line("[ProSE] Not pooling the identifications of the input files: Percolator rescored only some of them, and runs of different search engines are not merged. Set 'rescore' to 'false' to pool them with ProSE's scores."),
            "[ProSE] Not pooling the identifications of the input files: Percolator rescored only some of them, and runs of different search engines are not merged. Turn off Percolator rescoring to pool them with ProSE's scores."
        );
    }

    /// A value ProSE refuses: its line, not the last line of the help it prints after it.
    #[test]
    fn a_refused_value_is_named_and_not_the_help() {
        let mut lines = vec![
            "Invalid parameter values (ConversionError): Could not convert string '3000000000' to an integer value. Aborting!".to_string(),
            "stty: 'standard input': Inappropriate ioctl for device".to_string(),
        ];
        for k in 0..200 {
            lines.push(format!(
                "  -Search:option{k} <number>   An option (default: '1')"
            ));
        }
        lines.push("  --helphelp                                               Shows all options (including advanced)".to_string());
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let d = digest(&refs);
        assert_eq!(
            d.message(false),
            "Invalid parameter values (ConversionError): Could not convert string '3000000000' to an integer value. Aborting!"
        );
        assert_eq!(d.tail.len(), super::STDERR_TAIL);
    }

    /// Killed (signal 9) during Percolator: its last line ("Split 3:") would only mislead.
    #[test]
    fn a_killed_tool_without_an_error_has_no_message() {
        let d = digest(&[
            "Selecting Cneg by cross-validation.",
            "Split 1:\tSelected feature 15 as initial direction. Could separate 51 training set positives with q<0.01 in that direction.",
            "Split 3:",
        ]);
        assert_eq!(d.message(true), "");
        // ended with an exit code: its last line is all there is
        assert_eq!(d.message(false), "Split 3:");
        // the Percolator note before a SIGXFSZ (fsize_50) is no error either
        let note = digest(&["Percolator rescoring of /data/a.mzML: Percolator's sanity check rejected the learned model (on the held-out folds it found no target PSM at q < 0.01, or fewer than the best single feature; typical for blank runs and runs with few confident PSMs), so the PSMs are scored with that feature alone."]);
        assert_eq!(note.message(true), "");
    }

    /// A 300 MB line (fake_longline) is cut where it is read, and shown shorter still.
    #[test]
    fn long_lines_are_cut() {
        let long = format!(
            "Error: {}\nnext\n{}",
            "x".repeat(3 * MAX_LINE),
            "y".repeat(10)
        );
        let mut lines = Vec::new();
        read_lines(long.as_bytes(), |l| lines.push(l.to_string()));
        assert_eq!(lines.len(), 3);
        assert!(
            lines[0].ends_with('…') && lines[0].len() <= MAX_LINE + '…'.len_utf8(),
            "{}",
            lines[0].len()
        );
        assert_eq!(lines[1], "next");
        assert_eq!(lines[2], "yyyyyyyyyy", "a last line without a line end");
        let d = digest(&[&lines[0]]);
        assert_eq!(d.message(false).chars().count(), MAX_SHOWN + 1);
        assert_eq!(d.tail[0].chars().count(), MAX_SHOWN + 1);
        // CRLF line ends and invalid UTF-8
        let mut crlf = Vec::new();
        read_lines(&b"a\r\nb\xff\n"[..], |l| crlf.push(l.to_string()));
        assert_eq!(crlf, vec!["a".to_string(), "b\u{fffd}".to_string()]);
    }
}

#[cfg(all(test, windows))]
mod tests_windows {
    use super::how_it_ended;
    use std::os::windows::process::ExitStatusExt;
    use std::process::ExitStatus;

    #[test]
    fn how_a_tool_ended_leads_its_failure() {
        assert_eq!(how_it_ended(&ExitStatus::from_raw(0)), None);
        assert_eq!(
            how_it_ended(&ExitStatus::from_raw(3)).as_deref(),
            Some("ProSE ended with exit code 3")
        );
        assert_eq!(
            how_it_ended(&ExitStatus::from_raw(0xC000_0005)).as_deref(),
            Some("ProSE crashed (exception 0xC0000005)")
        );
    }
}

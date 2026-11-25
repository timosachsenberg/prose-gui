//! The contaminants (cRAP) appended to the user's database: the search folder's `database.fasta`, the
//! user's FASTA with the cRAP entries it does not have, accessions prefixed `CON__`, and decoys of them
//! when the database has decoys ([`crate::decoys`]).

use crate::decoys::{decoy_sequence, DecoyRules, DecoyStatistics};
use std::fs;
use std::io;
use std::path::Path;

/// The accession of a FASTA header line (`>` and the text up to the first white space, as OpenMS' FASTAFile
/// reads it); None if the line is no header. A byte order mark before the first header is skipped.
fn fasta_accession(line: &str) -> Option<&str> {
    let header = line.trim_start_matches('\u{feff}').strip_prefix('>')?;
    let header = header.trim_start();
    Some(header.split(char::is_whitespace).next().unwrap_or(""))
}

/// The contaminant prefix of the app's cRAP accessions.
const CONTAMINANT_PREFIX: &str = "CON__";

/// One entry of the cRAP FASTA, as the app appends it.
struct CrapEntry {
    /// with [`CONTAMINANT_PREFIX`]
    accession: String,
    /// the header after the accession (the description, with the white space before it)
    rest: String,
    /// its sequence lines, with their line ends
    lines: Vec<String>,
}

fn crap_entries(text: &str) -> Vec<CrapEntry> {
    let mut entries: Vec<CrapEntry> = Vec::new();
    for line in text.split_inclusive('\n') {
        match fasta_accession(line) {
            Some(accession) => {
                let at = line.find(accession).unwrap_or(1) + accession.len();
                let rest = line[at..].trim_end_matches(['\n', '\r']).to_string();
                let accession = if accession.starts_with(CONTAMINANT_PREFIX) {
                    accession.to_string()
                } else {
                    format!("{CONTAMINANT_PREFIX}{accession}")
                };
                entries.push(CrapEntry {
                    accession,
                    rest,
                    lines: Vec::new(),
                });
            }
            None => {
                if let Some(entry) = entries.last_mut() {
                    entry.lines.push(line.to_string());
                }
            }
        }
    }
    entries
}

/// Writes `dest`: the user's FASTA, then the entries of `crap` with `CON__` before their accession
/// (unless it is there already), as the web app's `withContaminants` does. A newline is added after
/// the user's last line if it has none. The user's file is only read.
///
/// - A cRAP entry whose accession (with `CON__`) the user's FASTA has already (the `database.fasta` of an
///   earlier search, which holds them) is left out: ProSE would get the accession twice.
/// - When the user's FASTA has decoys of its own, which ProSE then uses instead of generating any (`decoys`
///   `auto`), each appended entry gets its decoy, with the database's marker, as ProSE would generate it
///   ([`crate::decoys`]).
/// - A FASTA without any entry is refused (`InvalidData`, with the message for the user): the search would
///   search the contaminants alone.
pub fn with_contaminants(
    user: &Path,
    crap: &Path,
    dest: &Path,
    rules: &DecoyRules,
) -> io::Result<()> {
    use io::{BufRead, Write};
    // (InvalidData is the user's database's: not the bundled file's)
    let crap = crap_entries(&fs::read_to_string(crap).map_err(io::Error::other)?);
    let wanted: std::collections::HashMap<&str, usize> = crap
        .iter()
        .enumerate()
        .map(|(k, e)| (e.accession.as_str(), k))
        .collect();
    let mut present = vec![false; crap.len()];
    let mut stats = DecoyStatistics::default();
    let mut entries = 0usize;

    let mut out = io::BufWriter::new(fs::File::create(dest)?);
    let mut input = io::BufReader::with_capacity(1 << 20, fs::File::open(user)?);
    let mut last = b'\n';
    let mut line = Vec::new();
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        out.write_all(&line)?;
        last = line[line.len() - 1];
        if line.first() == Some(&b'>') || line.starts_with("\u{feff}>".as_bytes()) {
            let text = String::from_utf8_lossy(&line);
            if let Some(accession) = fasta_accession(&text) {
                entries += 1;
                stats.count(accession, &rules.decoy_prefix);
                if let Some(&k) = wanted.get(accession) {
                    present[k] = true;
                }
            }
        }
    }
    if entries == 0 {
        drop(out);
        let _ = fs::remove_file(dest);
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "The database {} has no proteins (no line starting with \">\"), so the search would find \
                 contaminants only.",
                user.display()
            ),
        ));
    }
    if last != b'\n' {
        out.write_all(b"\n")?;
    }
    let appended: Vec<&CrapEntry> = crap
        .iter()
        .zip(&present)
        .filter(|(_, there)| !**there)
        .map(|(e, _)| e)
        .collect();
    for entry in &appended {
        writeln!(out, ">{}{}", entry.accession, entry.rest)?;
        for line in &entry.lines {
            out.write_all(line.as_bytes())?;
        }
    }
    // ProSE reuses the database's decoys (decoys auto): the appended entries need theirs. The marker is the
    // user's database's; ProSE detects it in the whole database the same way (the appended targets and their
    // decoys only add to its share)
    let marker = if rules.mode == "auto" {
        stats.marker(&rules.decoy_prefix)
    } else {
        None
    };
    if let Some(marker) = marker {
        for entry in &appended {
            let sequence: String = entry
                .lines
                .iter()
                .flat_map(|l| l.chars().filter(|c| !c.is_whitespace()))
                .collect();
            let decoy = decoy_sequence(&sequence, rules);
            write!(
                out,
                ">{}{}\n{decoy}\n",
                marker.decoy_of(&entry.accession),
                entry.rest
            )?;
        }
    }
    out.flush()
}

//! The inputs in the search folder: the names of their links (`<n>__<name>`, shortened to the stems ProSE
//! can name its outputs after), how each reaches ProSE ([`InputLinks`], [`Placement`]), and `inputs.json`,
//! which maps a link back to the user's file ([`InputRecord`], [`humanize`]).

use super::{is_spectrum_file, suffix_start, SearchRequest};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(super) fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The longest stem (bytes) an input may have where ProSE names outputs after it. Its longest output name is
/// `<stem>_ModificationAnalysis_DeltaMassStats.tsv` (open search), and a file name has at most 255 bytes (Linux,
/// macOS; NTFS 255 UTF-16 units, which are never more than the bytes).
pub const MAX_STEM: usize = 255 - LONGEST_OUTPUT_SUFFIX.len();

/// The end of ProSE's longest output name after an input's stem (open search, in `mods/`).
const LONGEST_OUTPUT_SUFFIX: &str = "_ModificationAnalysis_DeltaMassStats.tsv";
/// The folder of that output in the search folder, with the separators around it (`mods\`, `qpx\runs\` is
/// shorter with its `.psm.parquet`).
const LONGEST_OUTPUT_FOLDER: &str = "\\mods\\";
/// Windows' MAX_PATH: a path has at most 259 UTF-16 units (260 with its terminating null), unless the
/// program opts into long paths, which ProSE (OpenMS, Arrow) is not known to do.
pub(super) const WINDOWS_MAX_PATH: usize = 259;

/// The longest stem (bytes) an input may have in the search folder `folder` (its `.partial` path), for links
/// `links`: [`MAX_STEM`]; on Windows ([`InputLinks::PathsOrHardLinks`]) also short enough that the path of
/// ProSE's longest output in the folder (`<folder>\mods\<stem>_ModificationAnalysis_DeltaMassStats.tsv`) and of
/// every link (`<folder>\<n>__<stem>.mzML.gz`, shorter) stays within MAX_PATH. A stem's UTF-16 units are never
/// more than its bytes, so the bytes bound it. 0 if not even the fixed part of that path fits.
pub fn max_stem(links: InputLinks, folder: &Path) -> usize {
    match links {
        InputLinks::Symlinks => MAX_STEM,
        InputLinks::PathsOrHardLinks => {
            let fixed = folder.as_os_str().to_string_lossy().encode_utf16().count()
                + LONGEST_OUTPUT_FOLDER.len()
                + LONGEST_OUTPUT_SUFFIX.len();
            WINDOWS_MAX_PATH.saturating_sub(fixed).min(MAX_STEM)
        }
    }
}

/// The shortest stem limit that still gives the `count` inputs link names of their own (`<n>__` and a character
/// of the name).
pub(super) fn shortest_stem(count: usize) -> usize {
    format!("{count}__").len() + 1
}

/// The name of input `index` (0-based) in the search folder: `<n>__<name>` with `n` from 1. ProSE names
/// its per-input outputs after the input and refuses two inputs of one name, so an input goes in as
/// a link with a name of its own (see [`InputLinks`]); the suffix (`.mzML.gz`) stays as it is. A name
/// whose stem would be longer than [`MAX_STEM`] is shortened (at a character) to that length: `<n>__`
/// keeps the stems unique, `inputs.json` maps the link back to the user's file ([`InputRecord`]).
pub fn link_name(index: usize, path: &str) -> String {
    link_name_within(index, path, MAX_STEM)
}

/// As [`link_name`], the stem at most `max_stem` bytes ([`max_stem`]).
pub fn link_name_within(index: usize, path: &str, max_stem: usize) -> String {
    let link = format!("{}__{}", index + 1, file_name(path));
    let stem = input_stem(&link).len();
    if stem <= max_stem {
        return link;
    }
    let mut end = max_stem;
    while !link.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &link[..end], &link[stem..])
}

/// Whether ProSE's outputs of an input of this name would have names (or, on Windows, paths) too long for the
/// file system, with stems of at most `max_stem` bytes.
pub fn stem_too_long(name: &str, max_stem: usize) -> bool {
    input_stem(name).len() > max_stem
}

/// Whether `name`, a name in the top level of a search folder, is one of the links to the inputs
/// (`<n>__<name>.mzML[.gz]`): never one of the search's own files.
pub fn is_input_link(name: &str) -> bool {
    name.split_once("__").is_some_and(|(n, rest)| {
        !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && is_spectrum_file(rest)
    })
}

/// How the inputs reach ProSE, which names its outputs after each input's stem and refuses two inputs
/// of one stem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputLinks {
    /// Every input as a symbolic link `<n>__<name>` in the search folder (Linux, macOS).
    Symlinks,
    /// Windows, where a symbolic link needs a privilege (developer mode): the inputs by their own paths
    /// when their stems differ (ignoring case, as NTFS does); those that share a stem as hard links
    /// `<n>__<name>` in the search folder, which works on the same volume only; never a copy.
    PathsOrHardLinks,
}

impl InputLinks {
    /// This platform's way.
    pub const PLATFORM: InputLinks = if cfg!(windows) {
        InputLinks::PathsOrHardLinks
    } else {
        InputLinks::Symlinks
    };
}

/// How one input reaches ProSE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// A symbolic link `<n>__<name>` in the search folder.
    Symlink,
    /// A hard link `<n>__<name>` in the search folder, removed again when the search ends.
    HardLink,
    /// The user's path itself.
    OwnPath,
}

/// The stem ProSE refuses for an input with `-out_qpx` (the merged views are `quantms.*.parquet`).
const RESERVED_STEM: &str = "quantms";

/// Whether ProSE refuses an input of this name as it is (its stem is `quantms`, ignoring case).
pub fn reserved_stem(name: &str) -> bool {
    stem_key(name) == RESERVED_STEM
}

/// The stem ProSE gives an input of this name, compared as NTFS compares names.
fn stem_key(name: &str) -> String {
    input_stem(name).to_lowercase()
}

fn all_unique(names: impl IntoIterator<Item = String>) -> bool {
    let mut seen = std::collections::HashSet::new();
    names.into_iter().all(|n| seen.insert(stem_key(&n)))
}

/// How each input of `req` reaches ProSE (in order), in the search folder `folder`.
pub fn input_placements(req: &SearchRequest, links: InputLinks, folder: &Path) -> Vec<Placement> {
    let max = max_stem(links, folder);
    match links {
        InputLinks::Symlinks => vec![Placement::Symlink; req.files.len()],
        InputLinks::PathsOrHardLinks => {
            let keys: Vec<String> = req
                .files
                .iter()
                .map(|f| stem_key(&file_name(&f.path)))
                .collect();
            let placements: Vec<Placement> = keys
                .iter()
                .zip(&req.files)
                .map(|(k, f)| {
                    // ProSE refuses an input of stem `quantms` (its QPX outputs would clash with the merged
                    // `quantms.*` views): such an input goes as a link `<n>__<name>` too, as does one whose
                    // name is too long for ProSE's outputs (the link's is shortened)
                    if keys.iter().filter(|other| *other == k).count() > 1
                        || k == RESERVED_STEM
                        || stem_too_long(&file_name(&f.path), max)
                    {
                        Placement::HardLink
                    } else {
                        Placement::OwnPath
                    }
                })
                .collect();
            // a file whose own name looks like a link (`1__a.mzML` beside two `a.mzML`): all as links
            let names = req
                .files
                .iter()
                .enumerate()
                .zip(&placements)
                .map(|((i, f), p)| seen_name(i, &f.path, *p, max));
            if all_unique(names) {
                placements
            } else {
                vec![Placement::HardLink; req.files.len()]
            }
        }
    }
}

/// The name ProSE sees for input `index`: the link's (its stem at most `max_stem` bytes), or the file's own.
fn seen_name(index: usize, path: &str, placement: Placement, max_stem: usize) -> String {
    match placement {
        Placement::OwnPath => file_name(path),
        Placement::Symlink | Placement::HardLink => link_name_within(index, path, max_stem),
    }
}

/// What ProSE calls an input in its outputs (`runs/<stem>.psm.parquet`): the link's name without `.gz`
/// and `.mzML`, ignoring ASCII case (OpenMS's `File::stemName`).
pub fn input_stem(link: &str) -> String {
    let mut stem = link;
    if let Some(end) = suffix_start(stem, ".gz") {
        stem = &stem[..end];
    }
    for suffix in [".mzml", ".mzpeak", ".raw", ".d"] {
        if let Some(end) = suffix_start(stem, suffix) {
            stem = &stem[..end];
            break;
        }
    }
    stem.to_string()
}

/// The record of one input in `inputs.json`: how the search folder's link maps to the user's file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InputRecord {
    /// The front end's id of the file.
    pub id: String,
    /// Position in the request.
    pub index: usize,
    /// The name ProSE saw: the link in the search folder (`<n>__<name>`), or the file's own name when
    /// it was given its own path (Windows, [`InputLinks::PathsOrHardLinks`]).
    pub link: String,
    /// What ProSE calls the input in its outputs.
    pub stem: String,
    /// The user's file name.
    pub name: String,
    /// The user's path, as given.
    pub path: String,
}

/// The records of `inputs.json` of the inputs of `req` in the search folder `folder` (its `.partial` path).
pub fn input_records_with(
    req: &SearchRequest,
    links: InputLinks,
    folder: &Path,
) -> Vec<InputRecord> {
    let max = max_stem(links, folder);
    let placements = input_placements(req, links, folder);
    req.files
        .iter()
        .zip(placements)
        .enumerate()
        .map(|(index, (f, placement))| {
            let link = seen_name(index, &f.path, placement, max);
            InputRecord {
                id: f.id.clone(),
                index,
                stem: input_stem(&link),
                link,
                name: file_name(&f.path),
                path: f.path.clone(),
            }
        })
        .collect()
}

/// `text` with the search folder's names for the inputs replaced by the user's: the path of a link by
/// the user's path, the bare name of a link by the user's file name, the stem ProSE names an input's
/// outputs after by the user's (`qpx/runs/1__a.psm.parquet` -> `qpx/runs/a.psm.parquet`). The path of an
/// output in the search folder `dir` loses the folder (`<dir>/merged.idXML` -> `merged.idXML`): the folder
/// of a search that failed is gone. Only whole names are replaced (`1__a.mzML` inside `11__a.mzML` stays).
/// The folder is recognised in each way a tool may write it (Windows: `\\` or `/` after any part, the drive
/// letter in either case), whatever the platform.
pub fn humanize(text: &str, dir: &Path, records: &[InputRecord]) -> String {
    let mut text = text.to_string();
    let folders = path_forms(&dir.to_string_lossy());
    for r in records {
        for folder in &folders {
            for sep in ['/', '\\'] {
                text = replace_whole(&text, &format!("{folder}{sep}{}", r.link), &r.path);
            }
        }
    }
    for folder in &folders {
        for sep in ['/', '\\'] {
            text = text.replace(&format!("{folder}{sep}"), "");
        }
    }
    for r in records {
        text = replace_whole(&text, &r.link, &r.name);
        if r.link != r.name {
            text = replace_whole(&text, &r.stem, &input_stem(&r.name));
        }
    }
    text
}

/// The ways a path can be written: as it is, with `/` or `\\` between all its parts, and with a drive letter
/// (`C:`) in the other case too; without a trailing separator. None for an empty path.
fn path_forms(path: &str) -> Vec<String> {
    let path = path.trim_end_matches(['/', '\\']);
    if path.is_empty() {
        return Vec::new();
    }
    let mut forms = Vec::new();
    for form in [
        path.to_string(),
        path.replace('\\', "/"),
        path.replace('/', "\\"),
    ] {
        let mut chars = form.chars();
        let other_drive = match (chars.next(), chars.next()) {
            (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => {
                let flipped = if letter.is_ascii_uppercase() {
                    letter.to_ascii_lowercase()
                } else {
                    letter.to_ascii_uppercase()
                };
                Some(format!("{flipped}{}", &form[1..]))
            }
            _ => None,
        };
        for f in std::iter::once(form).chain(other_drive) {
            if !forms.contains(&f) {
                forms.push(f);
            }
        }
    }
    forms
}

fn replace_whole(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_string();
    }
    let boundary = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut before: Option<char> = None;
    while let Some(at) = rest.find(from) {
        let prev = rest[..at].chars().next_back().or(before);
        let after = rest[at + from.len()..].chars().next();
        out.push_str(&rest[..at]);
        if boundary(prev) && boundary(after) {
            out.push_str(to);
        } else {
            out.push_str(from);
        }
        before = from.chars().next_back();
        rest = &rest[at + from.len()..];
    }
    out.push_str(rest);
    out
}

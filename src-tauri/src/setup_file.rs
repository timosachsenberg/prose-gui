//! Search setups (`<name>.prose-setup.json`): the setup of a search and the state of the app's views as JSON,
//! written and read here for the page (which never names a path), and the list of recent setups in the app's
//! data folder (`recent-setups.json`). The page makes and checks the JSON itself (`src/lib/setup/setup.ts`
//! documents its fields); this side only checks that a file is one by its name and size, so that a stray large file
//! is never read into the page. Not the results archive (`.prose`, a zip of a search: `archive`).

use std::path::{Path, PathBuf};

/// The extension of a setup file.
pub const SETUP_EXTENSION: &str = ".prose-setup.json";

/// The largest setup file read or written: a setup is a few kilobytes (a thousand inputs with their design
/// fit in well under a megabyte).
pub const MAX_SETUP_BYTES: u64 = 4 * 1024 * 1024;

/// How many recent setups the File menu lists.
pub const MAX_RECENT: usize = 10;

/// The file of the recent setups in the app's data folder.
pub const RECENT_FILE: &str = "recent-setups.json";

/// `path` names a setup file (`.prose-setup.json`, in any case).
pub fn is_setup_name(path: &Path) -> bool {
    path.file_name()
        .map(|n| {
            let n = n.to_string_lossy().to_lowercase();
            n.ends_with(SETUP_EXTENSION)
        })
        .unwrap_or(false)
}

/// The path a setup is written to: `path` with `.prose-setup.json` (the save dialog gives what the user typed:
/// `run`, `run.json` and `run.prose-setup.json` all become `run.prose-setup.json`).
pub fn with_setup_extension(path: &Path) -> PathBuf {
    if is_setup_name(path) {
        return path.to_path_buf();
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = name
        .strip_suffix(".json")
        .or_else(|| name.strip_suffix(".JSON"))
        .unwrap_or(&name);
    let stem = if stem.is_empty() { "search" } else { stem };
    path.with_file_name(format!("{stem}{SETUP_EXTENSION}"))
}

/// The text of the setup file at `path`: refused unless it is named as one ([`is_setup_name`]), it is a file of
/// at most [`MAX_SETUP_BYTES`] and it is UTF-8.
pub fn read_setup(path: &Path) -> Result<String, String> {
    let shown = path.display();
    if !is_setup_name(path) {
        return Err(format!(
            "{shown} is not a ProSE setup file (its name does not end with {SETUP_EXTENSION})."
        ));
    }
    let meta = std::fs::metadata(path).map_err(|e| format!("{shown} cannot be read ({e})."))?;
    if !meta.is_file() {
        return Err(format!("{shown} is not a file."));
    }
    if meta.len() > MAX_SETUP_BYTES {
        return Err(format!(
            "{shown} is too large for a setup file ({} bytes; at most {MAX_SETUP_BYTES}).",
            meta.len()
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{shown} cannot be read ({e})."))?;
    String::from_utf8(bytes).map_err(|_| format!("{shown} is not a text file."))
}

/// Writes `contents` to `path` (with the extension, [`with_setup_extension`]) through a temporary file beside it,
/// so that a failed write leaves the earlier version; -> the path written. More than [`MAX_SETUP_BYTES`] is refused.
pub fn write_setup(path: &Path, contents: &str) -> Result<PathBuf, String> {
    if contents.len() as u64 > MAX_SETUP_BYTES {
        return Err(format!(
            "The setup is too large to save ({} bytes; at most {MAX_SETUP_BYTES}).",
            contents.len()
        ));
    }
    let path = with_setup_extension(path);
    let shown = path.display();
    let partial = path.with_file_name(format!(
        ".{}.partial",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    std::fs::write(&partial, contents).map_err(|e| format!("{shown} cannot be written ({e})."))?;
    if let Err(e) = std::fs::rename(&partial, &path) {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("{shown} cannot be written ({e})."));
    }
    Ok(path)
}

/// The recent setups listed in `dir` (the app's data folder), the latest first; none if the list is missing or
/// broken (it is a convenience).
pub fn recent(dir: &Path) -> Vec<PathBuf> {
    std::fs::read(dir.join(RECENT_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && is_setup_name(p))
        .take(MAX_RECENT)
        .collect()
}

/// Puts `path` first in the recent setups of `dir` (once; at most [`MAX_RECENT`]); -> the list.
pub fn remember(dir: &Path, path: &Path) -> Vec<PathBuf> {
    let mut list = recent(dir);
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(MAX_RECENT);
    save_recent(dir, &list);
    list
}

/// Takes `path` off the recent setups of `dir` (a file that cannot be opened any more); -> the list.
pub fn forget(dir: &Path, path: &Path) -> Vec<PathBuf> {
    let mut list = recent(dir);
    list.retain(|p| p != path);
    save_recent(dir, &list);
    list
}

/// The setup's name in a file name: `exp.prose-setup.json` (in any case) -> `exp`; another name as it is.
pub fn setup_stem(file_name: &str) -> &str {
    file_name
        .len()
        .checked_sub(SETUP_EXTENSION.len())
        .and_then(|cut| {
            let ext = file_name.get(cut..)?;
            ext.eq_ignore_ascii_case(SETUP_EXTENSION)
                .then(|| &file_name[..cut])
        })
        .unwrap_or(file_name)
}

/// Empties the recent setups of `dir`.
pub fn forget_all(dir: &Path) {
    save_recent(dir, &[]);
}

fn save_recent(dir: &Path, list: &[PathBuf]) {
    let names: Vec<String> = list
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(
            dir.join(RECENT_FILE),
            serde_json::to_vec(&names).unwrap_or_default(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("prose-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_extension_is_checked_and_added() {
        assert!(is_setup_name(Path::new("/a/run.prose-setup.json")));
        assert!(is_setup_name(Path::new("/a/RUN.PROSE-SETUP.JSON")));
        assert!(!is_setup_name(Path::new("/a/run.json")));
        assert!(!is_setup_name(Path::new("/a/run.prose")));
        assert_eq!(
            with_setup_extension(Path::new("/a/run")),
            PathBuf::from("/a/run.prose-setup.json")
        );
        assert_eq!(
            with_setup_extension(Path::new("/a/run.json")),
            PathBuf::from("/a/run.prose-setup.json")
        );
        assert_eq!(
            with_setup_extension(Path::new("/a/run.prose-setup.json")),
            PathBuf::from("/a/run.prose-setup.json")
        );
    }

    #[test]
    fn a_setup_is_written_and_read_back() {
        let dir = scratch("roundtrip");
        let written = write_setup(&dir.join("exp"), "{\"format\":\"prose-setup\"}").unwrap();
        assert_eq!(written, dir.join("exp.prose-setup.json"));
        assert_eq!(
            read_setup(&written).unwrap(),
            "{\"format\":\"prose-setup\"}"
        );
        // (no partial file is left)
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn other_files_and_large_ones_are_refused() {
        let dir = scratch("refused");
        let other = dir.join("data.json");
        std::fs::write(&other, "{}").unwrap();
        assert!(read_setup(&other)
            .unwrap_err()
            .contains("not a ProSE setup file"));
        assert!(read_setup(&dir.join("gone.prose-setup.json"))
            .unwrap_err()
            .contains("cannot be read"));
        let large = dir.join("large.prose-setup.json");
        std::fs::write(&large, vec![b' '; MAX_SETUP_BYTES as usize + 1]).unwrap();
        assert!(read_setup(&large).unwrap_err().contains("too large"));
        let folder = dir.join("folder.prose-setup.json");
        std::fs::create_dir_all(&folder).unwrap();
        assert!(read_setup(&folder).unwrap_err().contains("not a file"));
        let binary = dir.join("binary.prose-setup.json");
        std::fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
        assert!(read_setup(&binary).unwrap_err().contains("not a text file"));
        let too_much = " ".repeat(MAX_SETUP_BYTES as usize + 1);
        assert!(write_setup(&dir.join("big"), &too_much)
            .unwrap_err()
            .contains("too large"));
        assert!(!dir.join("big.prose-setup.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recent_setups_are_kept_latest_first_once_each() {
        let dir = scratch("recent");
        assert!(recent(&dir).is_empty());
        let a = dir.join("a.prose-setup.json");
        let b = dir.join("b.prose-setup.json");
        remember(&dir, &a);
        remember(&dir, &b);
        assert_eq!(remember(&dir, &a), vec![a.clone(), b.clone()]);
        for k in 0..20 {
            remember(&dir, &dir.join(format!("p{k}.prose-setup.json")));
        }
        assert_eq!(recent(&dir).len(), MAX_RECENT);
        assert_eq!(recent(&dir)[0], dir.join("p19.prose-setup.json"));
        // a broken list is no list; entries that are no setup files are left out
        std::fs::write(dir.join(RECENT_FILE), "not json").unwrap();
        assert!(recent(&dir).is_empty());
        std::fs::write(
            dir.join(RECENT_FILE),
            "[\"relative.prose-setup.json\", \"/x/y.txt\"]",
        )
        .unwrap();
        assert!(recent(&dir).is_empty());
        forget_all(&dir);
        assert!(recent(&dir).is_empty());
        // one entry taken off (its file is gone), the others kept in their order
        let c = dir.join("c.prose-setup.json");
        remember(&dir, &a);
        remember(&dir, &b);
        remember(&dir, &c);
        assert_eq!(forget(&dir, &b), vec![c.clone(), a.clone()]);
        assert_eq!(recent(&dir), vec![c, a]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_setups_name_is_its_file_name_without_the_extension_in_any_case() {
        assert_eq!(setup_stem("exp.prose-setup.json"), "exp");
        assert_eq!(setup_stem("Exp.PROSE-SETUP.JSON"), "Exp");
        assert_eq!(setup_stem("Ölmühle.prose-setup.json"), "Ölmühle");
        assert_eq!(setup_stem("exp.json"), "exp.json");
        assert_eq!(setup_stem("json"), "json");
        // (a multi-byte character where the extension would start: no panic)
        assert_eq!(setup_stem("ééééééééé"), "ééééééééé");
    }
}

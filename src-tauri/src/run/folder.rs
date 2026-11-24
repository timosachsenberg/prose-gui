//! The search folder around the tool: the inputs' links, `inputs.json`, the database and the INI put in
//! before it runs ([`prepare_folder`], [`write_ini_and_command`]); the log of a failure kept and the
//! folder of a search that failed or was cancelled removed ([`keep_failure_log`], [`discard_partial`]).

use super::{sys, LAST_FAILURE_LOG};
use crate::args::{
    command_line, ini_of, reserved_stem, split_search_args, with_contaminants, InputRecord,
    Placement, SearchRequest, COMMAND_FILE, DATABASE_FILE, DATABASE_RECORD, DATABASE_SOURCE,
    INI_FILE,
};
use crate::decoys::DecoyRules;
use crate::history::remove_partial;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The links, `inputs.json` and, with contaminants, `database.fasta`.
pub(super) fn prepare_folder(
    dir: &Path,
    request: &SearchRequest,
    records: &[InputRecord],
    placements: &[Placement],
    crap: Option<&Path>,
) -> Result<(), String> {
    for ((file, record), placement) in request.files.iter().zip(records).zip(placements) {
        let original = fs::canonicalize(&file.path).unwrap_or_else(|_| PathBuf::from(&file.path));
        let link = dir.join(&record.link);
        match placement {
            Placement::OwnPath => {}
            Placement::Symlink => sys::link_input(&original, &link).map_err(|e| {
                format!(
                    "The input {} cannot be linked into the search folder ({e}).",
                    file.path
                )
            })?,
            // (Windows) a hard link is of a file only: a Bruker .d folder that needs another name in the search
            // folder cannot get one there (the reasons as below; nothing is ever copied)
            Placement::HardLink if original.is_dir() => {
                let why = if reserved_stem(&record.name) {
                    "ProSE's own result files are called quantms"
                } else if !record.link.ends_with(&record.name) {
                    "its name is too long for ProSE's result files in the search folder"
                } else {
                    "another input has the same name, and ProSE names its results after the inputs"
                };
                return Err(format!(
                    "ProSE needs the Bruker folder {} under another name in the search folder, because {why}. On \
                     Windows the app can give a file another name there (a hard link), but not a folder. Rename \
                     the folder and start the search again.",
                    file.path
                ));
            }
            // (Windows) a hard link works on the folder's own volume only; nothing is ever copied
            Placement::HardLink => fs::hard_link(&original, &link).map_err(|e| {
                // (a name too long for ProSE's outputs in the search folder: its link's name is shortened)
                if !record.link.ends_with(&record.name) && !reserved_stem(&record.name) {
                    return format!(
                        "The name of {} is too long for ProSE's result files in the app's data folder ({}), whose \
                         names and paths it makes from the file's name. The app links such a file into the search \
                         folder under a shorter name, but Windows can make that link only on the drive of the app's \
                         data and on NTFS. {} cannot be linked there ({e}). Rename the file to a shorter name and \
                         start the search again.",
                        record.name,
                        dir.parent().unwrap_or(dir).display(),
                        file.path
                    );
                }
                if reserved_stem(&record.name) {
                    return format!(
                        "ProSE cannot search a file named {} under that name, because its own result files are \
                         called quantms. The app links such a file into the search folder under another name, but \
                         Windows can make that link only on the drive of the app's data ({}) and on NTFS. {} cannot \
                         be linked there ({e}). Rename the file and start the search again.",
                        record.name,
                        dir.parent().unwrap_or(dir).display(),
                        file.path
                    );
                }
                format!(
                    "Two inputs are named {}. ProSE names its results after the files, so files of one \
                     name are searched together only through links in the search folder, which Windows \
                     can make only on the drive of the app's data ({}) and on NTFS; {} cannot be linked \
                     there ({e}). Rename one of the files and start the search again.",
                    record.name,
                    dir.parent().unwrap_or(dir).display(),
                    file.path
                )
            })?,
        }
    }
    let json = serde_json::to_string_pretty(records).map_err(|e| e.to_string())?;
    fs::write(dir.join("inputs.json"), json)
        .map_err(|e| format!("inputs.json cannot be written ({e})."))?;
    if request.contaminants {
        let crap =
            crap.ok_or("The contaminant database (cRAP) is not available in this installation.")?;
        let rules = DecoyRules::of(&request.settings.parameters);
        with_contaminants(
            Path::new(&request.database),
            crap,
            &dir.join(DATABASE_FILE),
            &rules,
        )
        .map_err(|e| match e.kind() {
            // (the user's database: the message says what is wrong with it)
            std::io::ErrorKind::InvalidData => e.to_string(),
            _ => format!("The database with contaminants cannot be written ({e})."),
        })?;
        // where the user's FASTA was, for History's Re-run only (never read as a database, and not in results archives)
        let source = serde_json::json!({ "path": request.database }).to_string();
        fs::write(dir.join(DATABASE_SOURCE), source)
            .map_err(|e| format!("{DATABASE_SOURCE} cannot be written ({e})."))?;
    } else {
        // the one file outside the folder that the results reader may read (files::resolve_search_file)
        let record = serde_json::json!({ "path": request.database }).to_string();
        fs::write(dir.join(DATABASE_RECORD), record)
            .map_err(|e| format!("{DATABASE_RECORD} cannot be written ({e})."))?;
    }
    Ok(())
}

/// Writes the folder's [`INI_FILE`] of the `-Search:` arguments of `args` and its [`COMMAND_FILE`]; the arguments
/// for the tool: the others, and `-ini` with the file.
pub(super) fn write_ini_and_command(
    folder: &Path,
    args: Vec<std::ffi::OsString>,
) -> Result<Vec<std::ffi::OsString>, String> {
    let (mut tool_args, search) = split_search_args(&args);
    let ini = folder.join(INI_FILE);
    fs::write(&ini, ini_of(&search)?)
        .map_err(|e| format!("{INI_FILE} cannot be written ({e})."))?;
    tool_args.push("-ini".into());
    tool_args.push(ini.into_os_string());
    let after = |flag: &str| -> Vec<std::ffi::OsString> {
        let at = tool_args
            .iter()
            .position(|a| a == flag)
            .map_or(tool_args.len(), |k| k + 1);
        tool_args[at..]
            .iter()
            .take_while(|a| !a.to_string_lossy().starts_with('-'))
            .cloned()
            .collect()
    };
    let inputs = after("-in");
    let database = after("-database").into_iter().next().unwrap_or_default();
    let line = command_line("ProSE", &tool_args, &inputs, &database, folder);
    fs::write(folder.join(COMMAND_FILE), format!("{line}\n"))
        .map_err(|e| format!("{COMMAND_FILE} cannot be written ({e})."))?;
    Ok(tool_args)
}

/// Keeps the log of a failed search (`prose.log` of its partial folder) as `<app data>/logs/`
/// [`LAST_FAILURE_LOG`], replacing the one before; the path, or None if there is none.
pub(super) fn keep_failure_log(partial: &Path) -> Option<PathBuf> {
    let logs = partial.parent()?.parent()?.join("logs");
    fs::create_dir_all(&logs).ok()?;
    let kept = logs.join(LAST_FAILURE_LOG);
    fs::copy(partial.join("prose.log"), &kept).ok()?;
    Some(kept)
}

/// Removes the partial folder of a search that failed or was cancelled. A folder that cannot be removed
/// stays for the sweep of the app's next start (`history::sweep`); that is said on stderr and in the kept
/// log of the failure (`kept`), so that the folder's disk space is not lost unnoticed.
pub(super) fn discard_partial(lock: Option<fs::File>, partial: &Path, kept: Option<&Path>) {
    let Err(e) = remove_partial(lock, partial) else {
        return;
    };
    let note = format!(
        "The search folder {} could not be removed ({e}); the next start of the app removes it.",
        partial.display()
    );
    eprintln!("{note}");
    if let Some(kept) = kept {
        if let Ok(mut file) = fs::OpenOptions::new().append(true).open(kept) {
            let _ = writeln!(file, "{note}");
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    /// A partial folder that cannot be removed is said in the kept log of the failure.
    #[test]
    fn a_folder_that_cannot_be_removed_is_noted_in_the_failure_log() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("prose-discard-{}", std::process::id()));
        let searches = base.join("searches");
        let partial = searches.join("1-0.partial");
        std::fs::create_dir_all(partial.join("qpx")).unwrap();
        std::fs::write(partial.join("qpx/a.parquet"), "x").unwrap();
        let kept = base.join("last-failure.log");
        std::fs::write(&kept, "$ ProSE ...\n").unwrap();
        // a folder whose parent may not be written cannot be removed
        std::fs::set_permissions(&searches, std::fs::Permissions::from_mode(0o555)).unwrap();
        super::discard_partial(None, &partial, Some(&kept));
        std::fs::set_permissions(&searches, std::fs::Permissions::from_mode(0o755)).unwrap();
        let log = std::fs::read_to_string(&kept).unwrap();
        let removed = !partial.exists();
        std::fs::remove_dir_all(&base).unwrap();
        if removed {
            return; // (root may remove it anyway)
        }
        assert!(
            log.contains("could not be removed") && log.contains("1-0.partial"),
            "{log}"
        );
    }
}

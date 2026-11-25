//! Reference proteomes from UniProt (found through NCBI Taxonomy) as databases, on the user's request.

use super::*;
use crate::{net, taxonomy};
use tauri::Manager;

/// UniProt proteomes of the organisms NCBI Taxonomy finds for `name` (`taxonomy`): only on the user's request.
#[tauri::command]
pub async fn find_proteomes(name: String) -> Result<Vec<taxonomy::ProteomeHit>, String> {
    tauri::async_runtime::spawn_blocking(move || taxonomy::find_proteomes(&name))
        .await
        .map_err(|e| e.to_string())?
}

/// The proteomes downloaded into the app's `databases/` folder: their file names (`UP…[_reviewed].fasta`).
#[tauri::command]
pub fn downloaded_proteomes(app: AppHandle) -> Result<Vec<String>, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("databases");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("UP") && n.ends_with(".fasta"))
        .collect();
    names.sort();
    Ok(names)
}

/// The reference proteome `proteome` (a UniProt proteome id, `UP` and nine digits) as a FASTA in the app's
/// `databases/` folder, downloaded from UniProt's REST API with curl (macOS, Windows 10 and Linux have it) unless it
/// is there already; `reviewed`: Swiss-Prot entries only. Only on this request of the user (like the PRIDE and OLS
/// contacts of the SDRF features). -> the FASTA's path.
#[tauri::command]
pub async fn download_proteome(
    app: AppHandle,
    proteome: String,
    reviewed: bool,
) -> Result<String, String> {
    if !(proteome.len() == 11
        && proteome.starts_with("UP")
        && proteome[2..].bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(format!(
            "{proteome} is not a UniProt proteome id (UP and nine digits, e.g. UP000005640)."
        ));
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("databases");
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{} cannot be made ({e}).", dir.display()))?;
        let path = dir.join(format!("{proteome}{}.fasta", if reviewed { "_reviewed" } else { "" }));
        if std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
            return Ok(path.to_string_lossy().into_owned());
        }
        let query = format!("%28proteome%3A{proteome}%29{}", if reviewed { "%20AND%20%28reviewed%3Atrue%29" } else { "" });
        let url = format!("https://rest.uniprot.org/uniprotkb/stream?compressed=false&format=fasta&query={query}");
        let partial = path.with_extension("fasta.partial");
        // (a stream of unknown size: `net::fetch` with size 0, HTTPS only)
        let _ = std::fs::remove_file(&partial);
        let fetched = net::fetch(&url, &partial, 0, |_| {});
        if let Err(e) = fetched.and_then(|_| if net::len(&partial) == 0 { Err("no data".to_string()) } else { Ok(()) }) {
            let _ = std::fs::remove_file(&partial);
            return Err(format!(
                "The download from UniProt failed ({e}). Check the internet connection, or download the FASTA from https://www.uniprot.org/proteomes/{proteome}."
            ));
        }
        std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

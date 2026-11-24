//! The shim against a fake ProSE (tests/fake-prose.sh; on Windows examples/fake_prose.rs) and fixtures of
//! real ProSE output. One test binary (`cargo test --test shim`), a module per part of the shim:
//!
//! - `arguments`: the search request as ProSE's command line (`args`), the contaminants;
//! - `progress`: ProSE's log lines as progress events (`progress`);
//! - `tool_resolution`: finding and probing the tool (`tool`);
//! - `running`: searches against the fake tool (`run`).
//!
//! The helpers below (a request, its arguments, the inputs) are shared by them.

#[path = "../common/mod.rs"]
mod common;

mod arguments;
mod progress;
mod running;
mod tool_resolution;

use common::*;
use prose_desktop_lib::args::{build_args_with, InputLinks, SearchRequest};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A request as the front end sends it (camelCase JSON).
fn request(
    files: &[&Path],
    database: &Path,
    contaminants: bool,
    parameters: Value,
) -> SearchRequest {
    serde_json::from_value(json!({
        "files": files.iter().enumerate().map(|(i, p)| json!({ "id": format!("f{i}"), "path": p })).collect::<Vec<_>>(),
        "database": database,
        "contaminants": contaminants,
        "settings": { "parameters": parameters, "rescore": true, "peptdeep": false, "threads": 2 },
    }))
    .unwrap()
}

/// The arguments with every input as a link `<n>__<name>` (Linux, macOS; Windows: the `on_windows_` tests).
fn args_of(req: &SearchRequest, out: &Path) -> Vec<OsString> {
    build_args_with(req, out, InputLinks::Symlinks).unwrap()
}

/// The values after `flag` up to the next option.
fn values_of(args: &[OsString], flag: &str) -> Vec<OsString> {
    let at = args
        .iter()
        .position(|a| a == flag)
        .unwrap_or_else(|| panic!("{flag} is not in {args:?}"));
    args[at + 1..]
        .iter()
        .take_while(|a| {
            !a.to_string_lossy().starts_with('-') || a.to_string_lossy().parse::<f64>().is_ok()
        })
        .cloned()
        .collect()
}

fn has(args: &[OsString], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// A folder with an mzML (empty contents are enough: the shim reads nothing) and a FASTA.
struct Inputs {
    dir: TempDir,
    mzml: PathBuf,
    fasta: PathBuf,
}

fn inputs() -> Inputs {
    let dir = TempDir::new("inputs");
    let mzml = dir.write("data/a.mzML", "<mzML/>");
    let fasta = dir.write("data/db.fasta", ">sp|P1|ONE\nMKWVTFISLLK\n");
    Inputs { dir, mzml, fasta }
}

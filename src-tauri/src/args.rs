//! The search request and its translation into ProSE's command line.
//!
//! The front end sends a [`SearchRequest`] (camelCase JSON). [`build_args`] checks it and turns it into
//! arguments for the tool. Nothing here runs a process or touches the search folder; the folder is made
//! by [`mod@crate::run`], which also creates the links and the database that these arguments name.
//!
//! Parameters are ProSE's `Search:` parameters by their name without the prefix, as in the web app
//! (`peptide:missed_cleavages`, `FDR:PSM`). Only the names in [`PARAMETERS`] pass, each with the type
//! it has in ProSE, so that nothing from the front end can become another option of the tool
//! (`-in`, `-ini`, an output of its own choosing).
//!
//! The parts: `inputs` names the inputs' links in the search folder and records them (`inputs.json`),
//! `ini` writes the INI and the command line of a search. Their items are re-exported here.

mod ini;
mod inputs;

pub use ini::{command_line, ini_of, split_search_args, COMMAND_FILE, INI_FILE};
use inputs::{file_name, shortest_stem, WINDOWS_MAX_PATH};
pub use inputs::{
    humanize, input_placements, input_records_with, input_stem, is_input_link, link_name,
    link_name_within, max_stem, reserved_stem, stem_too_long, InputLinks, InputRecord, Placement,
    MAX_STEM,
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::ffi::OsString;
use std::fs;
use std::path::Path;

/// One input file: the front end's id for it and its path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFile {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSettings {
    /// ProSE parameters by name, without the `Search:` prefix (see [`PARAMETERS`]).
    pub parameters: Map<String, Value>,
    /// Percolator rescoring (`Search:rescore`).
    pub rescore: bool,
    /// PeptDeep features (`Search:peptdeep:enable`); ProSE finds its models itself.
    pub peptdeep: bool,
    /// `-threads` (0: all cores).
    pub threads: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub files: Vec<SearchFile>,
    pub database: String,
    /// Append the bundled cRAP to the database, with `CON__` accessions.
    pub contaminants: bool,
    pub settings: SearchSettings,
}

/// The type of a ProSE parameter, which decides how a JSON value becomes arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An integer (`<number>` in ProSE's help).
    Int,
    /// A real number (`<value>`).
    Float,
    /// One of ProSE's choices, a string; the `true`/`false` ones also take a JSON boolean.
    Choice,
    /// Free text; empty means unset.
    Text,
    /// A list of strings, one argument each (an empty list clears the default).
    List,
    /// A switch without a value: `-Search:name` when true, nothing when false.
    Flag,
}

use Kind::*;

/// Every `Search:` parameter a search may set, with its type. The four that the request's own fields
/// set (`rescore`, `peptdeep:enable` and the two PeptDeep model paths) are not in the list.
/// The list is ProSE's `--helphelp` of the pinned build (`native/OPENMS_COMMIT`).
pub const PARAMETERS: &[(&str, Kind)] = &[
    ("enzyme", Choice),
    ("decoys", Choice),
    ("decoy_prefix", Text),
    ("decoy_method", Choice),
    ("low_memory", Flag),
    ("precursor:mass_tolerance_lower", Float),
    ("precursor:mass_tolerance_upper", Float),
    ("precursor:mass_tolerance_unit", Choice),
    ("precursor:min_charge", Int),
    ("precursor:max_charge", Int),
    ("precursor:recorded_charge_outside_range", Choice),
    ("precursor:isotope_error_min", Int),
    ("precursor:isotope_error_max", Int),
    ("fragment:mass_tolerance", Float),
    ("fragment:mass_tolerance_unit", Choice),
    ("fragment:deisotope", Choice),
    ("fragment:deisotope_min_peaks", Int),
    ("fragment:deisotope_charge_cap", Choice),
    ("fragment:deisotope_sum_intensity", Choice),
    ("fragment:min_mz", Int),
    ("fragment:max_mz", Int),
    ("fragment:min_ion_index", Int),
    ("fragment:query_spectrum", Choice),
    ("fragment:min_matched_ions", Int),
    ("fragment:max_charge", Int),
    ("peaks:keep_n", Int),
    ("peaks:window_top", Int),
    ("peaks:window_type", Choice),
    ("modifications:fixed", List),
    ("modifications:variable", List),
    ("modifications:variable_max_per_peptide", Int),
    ("annotate:PSM", List),
    ("annotate:self_trained_ion_priors", Choice),
    ("annotate:ion_prior_model", Choice),
    ("annotate:ion_prior_peaks", Choice),
    ("annotate:ion_prior_max_fragment_charge", Int),
    ("annotate:ion_prior_train_fdr", Float),
    ("annotate:ion_prior_min_psms", Int),
    ("annotate:ion_prior_features", List),
    ("annotate:local_fragment_evidence", Choice),
    ("peptdeep:instrument", Choice),
    ("peptdeep:nce", Float),
    ("peptdeep:rt_model_type", Choice),
    ("peptdeep:batch_size", Int),
    ("peptide:min_size", Int),
    ("peptide:max_size", Int),
    ("peptide:missed_cleavages", Int),
    ("peptide:clip_nterm_methionine", Choice),
    ("peptide:deduplicate", Choice),
    ("peptide:protein_mapping", Choice),
    ("peptide:enzyme_specificity", Choice),
    ("peptide:motif", Text),
    ("peptide:min_mass", Int),
    ("peptide:max_mass", Int),
    ("report:top_hits", Int),
    ("report:isotope_error_convention", Choice),
    ("percolator:max_train_psms", Int),
    ("rescoring:terminal_features", Choice),
    ("rescoring:stratum_q_values", Choice),
    ("rescoring:peptide_level", Choice),
    ("FDR:PSM", Float),
    ("FDR:PSM_groups", Choice),
    ("FDR:protein", Float),
    ("FDR:protein_inference_PSM", Float),
    ("scoring:max_candidates_per_spectrum", Int),
    ("scoring:fragment_charges", Choice),
    ("scoring:method", Choice),
    ("scoring:mass_error_sd", Float),
    ("ions:add_y_ions", Choice),
    ("ions:add_b_ions", Choice),
    ("ions:add_a_ions", Flag),
    ("ions:add_c_ions", Flag),
    ("ions:add_x_ions", Flag),
    ("ions:add_z_ions", Flag),
    ("ions:add_zp1_ions", Flag),
    ("ions:by_activation", Choice),
    ("calibration:enabled", Choice),
    ("calibration:subset_ratio", Float),
    ("calibration:min_psms", Int),
    ("calibration:sigma_factor", Float),
    ("calibration:precursor_sigma_factor", Float),
    ("calibration:min_precursor_tolerance", Float),
    ("calibration:min_fragment_tolerance", Float),
    ("calibration:recalibrate", Choice),
    ("calibration:recalibrate_rt_chunks", Int),
    ("database:chunk_size", Int),
    ("chimeric:enabled", Flag),
    ("chimeric:max_coisolated", Int),
    ("chimeric:max_kl", Float),
    ("chimeric:top_hits", Int),
    ("chimeric:mz_tolerance", Float),
];

/// The name of the database in the search folder when contaminants are appended.
pub const DATABASE_FILE: &str = "database.fasta";

/// The record of the user's database in the search folder when ProSE searches it as it is (no contaminants):
/// `{"path": "<absolute path>"}`, the one absolute path the reader may read (`files::resolve_search_file`).
pub const DATABASE_RECORD: &str = "database.json";
/// The user's FASTA of a search with contaminants (which searched the folder's copy): for Re-run only.
pub const DATABASE_SOURCE: &str = "database-source.json";

/// The QPX output of ProSE in the search folder: `-out_qpx`, with `-qpx_extended` (the columns and the footer
/// the results reader needs, docs/parity.md).
pub const QPX_DIR: &str = "qpx";

/// The peak filter of the stored spectra (`-qpx_peak_window`, `-qpx_peak_top`; ProSE's QPX mz view): the 100
/// most intense peaks per 100 m/z and every peak ProSE annotated (the user's choice). On an Orbitrap HCD run
/// (~1,100 peaks per MS2 spectrum) it keeps 631 peaks per spectrum and removes 6.2 % of the peaks the PSM view's
/// annotation matches (b/y/a/c/x/z, charge 1-2), nearly all of intensity 0; of b/y matches 1.2 % (docs/parity.md).
pub const PEAK_WINDOW: f64 = 100.0;
pub const PEAK_TOP: u32 = 100;

/// The idXML of a search of several inputs (`-out_merged`), and of one input (`-out`; `-out_idxml` before OpenMS 3.7:
/// [`with_idxml_option`]), in the search
/// folder (the Export menu's idXML: `src/lib/results/exports.ts`).
pub const MERGED_IDXML: &str = "merged.idXML";
pub const SINGLE_IDXML: &str = "search.idXML";

/// The FDR the shim uses when the request has no `FDR:PSM`: ProSE's default (0) filters nothing and
/// leaves a search without q-values, which the results need. Not for a search without decoys
/// ([`FDR_PARAMETERS`]), which has no FDR.
pub const DEFAULT_PSM_FDR: f64 = 0.01;

/// The FDR parameters, which a search without decoys (`decoys` 'ignore') leaves out: ProSE cannot estimate an FDR
/// then and would skip a PSM or protein FDR above 0 with only a warning in its log (and the inference's PSM cutoff,
/// a q-value, has nothing to cut). The front end turns them off then; the shim leaves them out too, so that no
/// search claims an FDR it did not apply.
pub const FDR_PARAMETERS: [&str; 3] = ["FDR:PSM", "FDR:protein", "FDR:protein_inference_PSM"];

/// Whether the parameters search without decoys (`decoys` 'ignore'): no FDR then ([`FDR_PARAMETERS`]).
pub fn without_decoys(params: &Map<String, Value>) -> bool {
    params.get("decoys").and_then(Value::as_str) == Some("ignore")
}

/// The most threads a search may ask for (`-threads`): far beyond any computer the app runs on, but a typo of
/// a few more digits is refused here, before ProSE starts that many OpenMP threads (or refuses the number).
pub const MAX_THREADS: u32 = 1024;

/// Where `suffix` (ASCII, with the dot) begins at the end of `name`, compared ignoring ASCII case as
/// OpenMS compares extensions (`FileNameUtils`); None if `name` does not end with it. The original's own
/// bytes are compared: a lower-cased copy can be longer or shorter (`İ`, `Ⱥ`, `ẞ`, the Kelvin sign), and
/// its byte offsets do not fit the original.
pub fn suffix_start(name: &str, suffix: &str) -> Option<usize> {
    let start = name.len().checked_sub(suffix.len())?;
    (name.is_char_boundary(start) && name[start..].eq_ignore_ascii_case(suffix)).then_some(start)
}

/// Extension check: `name` ends with one of `suffixes` (with the dot), ignoring ASCII case.
fn has_suffix(name: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|s| suffix_start(name, s).is_some())
}

/// The inputs ProSE reads: mzML (also gzipped), mzPeak, Thermo .raw and Bruker .d (a folder), as the
/// ProSE of the app is built (`native/scripts/build-openms.sh`: mzPeak, Thermo RAW and OpenTIMS on).
pub const SPECTRUM_SUFFIXES: &[&str] = &[".mzml", ".mzml.gz", ".mzpeak", ".raw", ".d"];

pub fn is_spectrum_file(name: &str) -> bool {
    has_suffix(name, SPECTRUM_SUFFIXES)
}

/// A Bruker .d input: a folder, not a file.
pub fn is_bruker_d(name: &str) -> bool {
    has_suffix(name, &[".d"])
}

pub fn is_fasta_file(name: &str) -> bool {
    has_suffix(name, &[".fasta", ".fa", ".faa"])
}

fn check_file(
    kind: &str,
    path: &str,
    accepted: fn(&str) -> bool,
    expected: &str,
) -> Result<(), String> {
    if path.is_empty() {
        return Err(format!("No {kind} was given."));
    }
    // an absolute path cannot be taken for an option (a relative "-in" would be), and does not depend
    // on the folder the app was started in
    if !Path::new(path).is_absolute() {
        return Err(format!("The {kind} {path} is not an absolute path."));
    }
    let meta =
        fs::metadata(path).map_err(|e| format!("The {kind} {path} cannot be read ({e})."))?;
    // a Bruker .d is a folder (and only a folder); every other input a file
    if is_bruker_d(&file_name(path)) != meta.is_dir() || !(meta.is_file() || meta.is_dir()) {
        return Err(format!(
            "The {kind} {path} is not a {}.",
            if is_bruker_d(&file_name(path)) {
                "Bruker .d folder"
            } else {
                "file"
            }
        ));
    }
    if !accepted(&file_name(path)) {
        return Err(format!("The {kind} {path} is not {expected}."));
    }
    Ok(())
}

/// A string for the command line: not empty (unless `allow_empty`), no control characters, and not
/// starting with `-`, which ProSE would take for an option.
fn text_value(name: &str, value: &str, allow_empty: bool) -> Result<String, String> {
    if value.is_empty() && !allow_empty {
        return Err(format!("The value of {name} is empty."));
    }
    if value.starts_with('-') {
        return Err(format!(
            "The value of {name} may not start with \"-\": {value}"
        ));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(format!("The value of {name} has control characters."));
    }
    Ok(value.to_string())
}

fn number(name: &str, value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|f| f.is_finite())
        .ok_or_else(|| format!("The value of {name} must be a number, not {value}."))
}

/// The arguments one parameter becomes.
fn parameter_args(name: &str, kind: Kind, value: &Value) -> Result<Vec<OsString>, String> {
    let flag = OsString::from(format!("-Search:{name}"));
    let one = |v: String| vec![flag.clone(), OsString::from(v)];
    Ok(match kind {
        Int => {
            let f = number(name, value)?;
            if f.fract() != 0.0 {
                return Err(format!(
                    "The value of {name} must be a whole number, not {value}."
                ));
            }
            // ProSE's integers are 32-bit: a larger one makes it print its whole help and stop
            if f < f64::from(i32::MIN) || f > f64::from(i32::MAX) {
                return Err(format!(
                    "The value of {name} is outside the range ProSE accepts: {value} (whole numbers from {} to {}).",
                    i32::MIN,
                    i32::MAX
                ));
            }
            one(format!("{}", f as i32))
        }
        Float => one(format!("{}", number(name, value)?)),
        Choice => match value {
            Value::String(s) => one(text_value(name, s, false)?),
            Value::Bool(b) => one(b.to_string()),
            _ => return Err(format!("The value of {name} must be text, not {value}.")),
        },
        Text => match value {
            Value::String(s) if s.is_empty() => vec![],
            Value::String(s) => one(text_value(name, s, false)?),
            _ => return Err(format!("The value of {name} must be text, not {value}.")),
        },
        List => {
            let Value::Array(items) = value else {
                return Err(format!("The value of {name} must be a list, not {value}."));
            };
            let mut args = vec![flag];
            for item in items {
                let Value::String(s) = item else {
                    return Err(format!("The items of {name} must be text, not {item}."));
                };
                args.push(OsString::from(text_value(name, s, false)?));
            }
            args
        }
        Flag => match value {
            Value::Bool(true) => vec![flag],
            Value::Bool(false) => vec![],
            _ => {
                return Err(format!(
                    "The value of {name} must be true or false, not {value}."
                ))
            }
        },
    })
}

/// A parameter of the request as a number (for the open-search rule), or the default.
fn number_or(params: &Map<String, Value>, name: &str, default: f64) -> f64 {
    params.get(name).and_then(Value::as_f64).unwrap_or(default)
}

/// ProSE's rule (`FragmentIndex::isOpenSearchMode`): a precursor window of more than 1000 ppm or 1 Da on
/// either side is an open search, which gets the modification analysis.
pub fn is_open_search(params: &Map<String, Value>) -> bool {
    let ppm = params
        .get("precursor:mass_tolerance_unit")
        .and_then(Value::as_str)
        .unwrap_or("ppm")
        == "ppm";
    let lower = number_or(params, "precursor:mass_tolerance_lower", 10.0);
    let upper = number_or(params, "precursor:mass_tolerance_upper", 10.0);
    lower.max(upper) > if ppm { 1000.0 } else { 1.0 }
}

/// `args` (of [`build_args`]) with `option` for a one-input search's idXML in place of `-out`: `-out_idxml` for a ProSE
/// older than OpenMS 3.7 (`tool::idxml_option_of`).
pub fn with_idxml_option(mut args: Vec<OsString>, option: &str) -> Vec<OsString> {
    if let Some(arg) = args.iter_mut().find(|a| *a == "-out") {
        *arg = option.into();
    }
    args
}

/// The command line of a search whose folder is `out_dir`, with this platform's [`InputLinks`].
pub fn build_args(req: &SearchRequest, out_dir: &Path) -> Result<Vec<OsString>, String> {
    build_args_with(req, out_dir, InputLinks::PLATFORM)
}

/// The command line of a search whose folder is `out_dir`: the inputs as the links `<n>__<name>` in it
/// or by their own paths ([`input_placements`]), the database (the folder's `database.fasta` with
/// contaminants, else the user's file), and the outputs the app reads (`docs/parity.md`, "How the app
/// runs ProSE"). Checks the request first.
pub fn build_args_with(
    req: &SearchRequest,
    out_dir: &Path,
    links: InputLinks,
) -> Result<Vec<OsString>, String> {
    if req.files.is_empty() {
        return Err("There is no input file.".to_string());
    }
    for file in &req.files {
        check_file(
            "input file",
            &file.path,
            is_spectrum_file,
            "a spectrum file ProSE reads (.mzML, .mzML.gz, .mzpeak, .raw, or a Bruker .d folder)",
        )?;
    }
    check_file(
        "database",
        &req.database,
        is_fasta_file,
        "a FASTA file (.fasta, .fa or .faa)",
    )?;

    let mut params = req.settings.parameters.clone();
    let mut threads = req.settings.threads;
    let mut parameter_args_list: Vec<OsString> = Vec::new();
    // ProSE ignores co-isolated precursors with database chunks (with a warning): the front end leaves the
    // switch out then, and so does the shim, so that the record of the search does not claim them
    let chunked = params
        .get("database:chunk_size")
        .and_then(Value::as_f64)
        .is_some_and(|n| n > 0.0);
    let no_decoys = without_decoys(&params);
    // validate every parameter before using any of them, in the order the front end gave
    for (name, value) in &params {
        if name == "threads" {
            let n = number(name, value)?;
            if n < 0.0 || n.fract() != 0.0 || n > f64::from(MAX_THREADS) {
                return Err(format!(
                    "The value of threads must be a whole number from 0 (all cores) to {MAX_THREADS}, not {value}."
                ));
            }
            threads = n as u32;
            continue;
        }
        let Some((_, kind)) = PARAMETERS.iter().find(|(n, _)| n == name) else {
            return Err(format!(
                "The parameter {name} is not one of ProSE's search parameters the app sets."
            ));
        };
        let args = parameter_args(name, *kind, value)?;
        if (chunked && name == "chimeric:enabled")
            || (no_decoys && FDR_PARAMETERS.contains(&name.as_str()))
        {
            continue;
        }
        parameter_args_list.extend(args);
    }
    if threads > MAX_THREADS {
        return Err(format!(
            "The value of threads must be a whole number from 0 (all cores) to {MAX_THREADS}, not {threads}."
        ));
    }
    match params.get("FDR:PSM") {
        // without decoys there is no FDR (left out above)
        _ if no_decoys => {}
        Some(v) => {
            if number("FDR:PSM", v)? <= 0.0 {
                return Err(
                    "FDR:PSM must be above 0: without it the search has no q-values, which the results need.".to_string()
                );
            }
        }
        None => {
            params.insert("FDR:PSM".to_string(), Value::from(DEFAULT_PSM_FDR));
            parameter_args_list.extend(parameter_args(
                "FDR:PSM",
                Float,
                &Value::from(DEFAULT_PSM_FDR),
            )?);
        }
    }

    // (Windows) a search folder whose path leaves no room for ProSE's outputs within MAX_PATH
    let max = max_stem(links, out_dir);
    if max < shortest_stem(req.files.len()) {
        return Err(format!(
            "The app's data folder ({}) has too long a path: Windows limits a path to {WINDOWS_MAX_PATH} \
             characters, and ProSE's result files in it would be longer. Move the app's data to a folder of a \
             shorter path.",
            out_dir.parent().unwrap_or(out_dir).display()
        ));
    }
    let mut args: Vec<OsString> = vec!["-in".into()];
    args.extend(
        req.files
            .iter()
            .zip(input_placements(req, links, out_dir))
            .enumerate()
            .map(|(i, (f, placement))| match placement {
                Placement::OwnPath => OsString::from(&f.path),
                Placement::Symlink | Placement::HardLink => out_dir
                    .join(link_name_within(i, &f.path, max))
                    .into_os_string(),
            }),
    );
    args.push("-database".into());
    args.push(if req.contaminants {
        out_dir.join(DATABASE_FILE).into_os_string()
    } else {
        OsString::from(&req.database)
    });
    args.push("-out_qpx".into());
    args.push(out_dir.join(QPX_DIR).into_os_string());
    args.push("-qpx_extended".into());
    args.push("-qpx_peak_window".into());
    args.push(PEAK_WINDOW.to_string().into());
    args.push("-qpx_peak_top".into());
    args.push(PEAK_TOP.to_string().into());
    args.push("-keep_decoys".into());
    // the idXML of the Export menu: the merged one of several inputs, the one input's own
    if req.files.len() > 1 {
        args.push("-out_merged".into());
        args.push(out_dir.join(MERGED_IDXML).into_os_string());
    } else {
        args.push("-out".into());
        args.push(out_dir.join(SINGLE_IDXML).into_os_string());
    }
    if is_open_search(&params) {
        args.push("-out_mod_analysis_dir".into());
        args.push(out_dir.join("mods").into_os_string());
    }
    args.push("-threads".into());
    args.push(threads.to_string().into());
    args.push("-Search:rescore".into());
    args.push(req.settings.rescore.to_string().into());
    if req.settings.peptdeep {
        args.push("-Search:peptdeep:enable".into());
    }
    args.extend(parameter_args_list);
    Ok(args)
}

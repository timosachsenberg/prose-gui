//! The search request as ProSE's command line: parameters, inputs and their links, the contaminants.

use super::running::Run;
use super::*;
use prose_desktop_lib::args::{
    build_args, input_placements, input_records_with, input_stem, is_input_link, link_name,
    with_contaminants, Placement, MAX_STEM,
};
use prose_desktop_lib::decoys::DecoyRules;
use std::fs;

#[test]
fn arguments_keep_paths_with_spaces_and_umlauts_intact() {
    let dir = TempDir::new("umlauts");
    let mzml = dir.write("Mäß daten/ünï cøde 1.mzML", "<mzML/>");
    let gz = dir.write("two words/Probe ä.mzML.gz", "gz");
    let fasta = dir.write("Datenbank ö/prot ein.fasta", ">a\nMK\n");
    let out = dir.path().join("out dir ü/s.partial");
    let args = args_of(&request(&[&mzml, &gz], &fasta, false, json!({})), &out);

    assert_eq!(
        values_of(&args, "-in"),
        vec![
            out.join("1__ünï cøde 1.mzML").into_os_string(),
            out.join("2__Probe ä.mzML.gz").into_os_string()
        ],
        "inputs are the symlinks of the search folder, each one argument"
    );
    assert_eq!(
        values_of(&args, "-database"),
        vec![fasta.clone().into_os_string()]
    );
    assert_eq!(
        values_of(&args, "-out_qpx"),
        vec![out.join("qpx").into_os_string()]
    );
    assert!(!has(&args, "-summary_out"));
}

#[test]
fn the_app_outputs_are_always_requested() {
    let i = inputs();
    let out = i.dir.path().join("o");
    let args = args_of(&request(&[&i.mzml], &i.fasta, false, json!({})), &out);
    // the QPX files with the columns and the footer of -qpx_extended are all the app reads
    assert_eq!(
        values_of(&args, "-out_qpx"),
        vec![out.join("qpx").into_os_string()]
    );
    assert!(has(&args, "-qpx_extended"));
    assert_eq!(
        values_of(&args, "-qpx_peak_window"),
        vec![OsString::from("100")]
    );
    assert_eq!(
        values_of(&args, "-qpx_peak_top"),
        vec![OsString::from("100")]
    );
    for flag in ["-out_parquet", "-out_psm_spectra", "-summary_out"] {
        assert!(!has(&args, flag), "{flag}");
    }
    assert!(has(&args, "-keep_decoys"));
    assert_eq!(values_of(&args, "-threads"), vec![OsString::from("2")]);
    assert_eq!(
        values_of(&args, "-Search:rescore"),
        vec![OsString::from("true")]
    );
    assert!(!has(&args, "-Search:peptdeep:enable"));
    // one input: its own idXML (the Export menu's), no pooled output; a closed search: no modification analysis
    assert_eq!(
        values_of(&args, "-out"),
        vec![out.join("search.idXML").into_os_string()]
    );
    assert!(!has(&args, "-out_merged"));
    assert!(!has(&args, "-out_mod_analysis_dir"));
}

#[test]
fn several_inputs_ask_for_the_merged_output_and_open_search_for_the_modification_tables() {
    let i = inputs();
    let b = i.dir.write("other/b.mzML", "<mzML/>");
    let out = i.dir.path().join("o");
    let open = json!({
        "precursor:mass_tolerance_lower": 500, "precursor:mass_tolerance_upper": 500,
        "precursor:mass_tolerance_unit": "Da",
    });
    let args = args_of(&request(&[&i.mzml, &b], &i.fasta, false, open), &out);
    assert_eq!(
        values_of(&args, "-out_merged"),
        vec![out.join("merged.idXML").into_os_string()]
    );
    assert!(!has(&args, "-out"));
    assert_eq!(
        values_of(&args, "-out_mod_analysis_dir"),
        vec![out.join("mods").into_os_string()]
    );

    // ProSE's own rule: more than 1000 ppm or 1 Da on either side
    let wide_ppm = json!({ "precursor:mass_tolerance_upper": 1500 });
    assert!(has(
        &args_of(&request(&[&i.mzml], &i.fasta, false, wide_ppm), &out),
        "-out_mod_analysis_dir"
    ));
    let narrow_da = json!({ "precursor:mass_tolerance_unit": "Da", "precursor:mass_tolerance_lower": 0.5, "precursor:mass_tolerance_upper": 1 });
    assert!(!has(
        &args_of(&request(&[&i.mzml], &i.fasta, false, narrow_da), &out),
        "-out_mod_analysis_dir"
    ));
}

#[test]
fn parameters_are_typed_and_use_the_search_prefix() {
    let i = inputs();
    let out = i.dir.path().join("o");
    let params = json!({
        "enzyme": "Trypsin/P",
        "peptide:missed_cleavages": 2,
        "precursor:isotope_error_min": -1,
        "precursor:mass_tolerance_lower": 10,
        "fragment:mass_tolerance": 0.02,
        "FDR:PSM": 0.01,
        "modifications:fixed": ["Carbamidomethyl (C)"],
        "modifications:variable": ["Oxidation (M)", "Acetyl (Protein N-term)"],
        "annotate:self_trained_ion_priors": false,
        "calibration:enabled": false,
        "low_memory": true,
        "ions:add_a_ions": false,
        "database:chunk_size": 100000,
        "peptdeep:instrument": "timsTOF",
    });
    let args = args_of(&request(&[&i.mzml], &i.fasta, false, params), &out);
    let one = |flag: &str| values_of(&args, flag);
    assert_eq!(one("-Search:enzyme"), vec![OsString::from("Trypsin/P")]);
    assert_eq!(
        one("-Search:peptide:missed_cleavages"),
        vec![OsString::from("2")]
    );
    assert_eq!(
        one("-Search:precursor:isotope_error_min"),
        vec![OsString::from("-1")]
    );
    assert_eq!(
        one("-Search:precursor:mass_tolerance_lower"),
        vec![OsString::from("10")]
    );
    assert_eq!(
        one("-Search:fragment:mass_tolerance"),
        vec![OsString::from("0.02")]
    );
    assert_eq!(one("-Search:FDR:PSM"), vec![OsString::from("0.01")]);
    assert_eq!(
        one("-Search:modifications:fixed"),
        vec![OsString::from("Carbamidomethyl (C)")]
    );
    assert_eq!(
        one("-Search:modifications:variable"),
        vec![
            OsString::from("Oxidation (M)"),
            OsString::from("Acetyl (Protein N-term)")
        ],
        "a list is one argument per item"
    );
    assert_eq!(
        one("-Search:annotate:self_trained_ion_priors"),
        vec![OsString::from("false")],
        "a true/false choice takes its value"
    );
    assert_eq!(
        one("-Search:database:chunk_size"),
        vec![OsString::from("100000")]
    );
    assert_eq!(
        one("-Search:peptdeep:instrument"),
        vec![OsString::from("timsTOF")]
    );
    // a switch: present when on, absent when off (ProSE's flags take no value)
    assert!(has(&args, "-Search:low_memory"));
    assert!(!has(&args, "-Search:ions:add_a_ions"));
    // the calibration is on unless it is switched off (the tool's default since OpenMS #10804): a choice, not a switch
    assert_eq!(
        one("-Search:calibration:enabled"),
        vec![OsString::from("false")]
    );
}

#[test]
fn an_empty_list_clears_the_default_and_peptdeep_is_a_switch() {
    let i = inputs();
    let out = i.dir.path().join("o");
    let mut req = request(
        &[&i.mzml],
        &i.fasta,
        false,
        json!({ "modifications:variable": [] }),
    );
    req.settings.peptdeep = true;
    req.settings.rescore = false;
    let args = args_of(&req, &out);
    let at = args
        .iter()
        .position(|a| a == "-Search:modifications:variable")
        .unwrap();
    assert!(
        args.get(at + 1)
            .is_none_or(|next| next.to_string_lossy().starts_with('-')),
        "no value follows: {args:?}"
    );
    assert!(has(&args, "-Search:peptdeep:enable"));
    assert_eq!(
        values_of(&args, "-Search:rescore"),
        vec![OsString::from("false")]
    );
}

#[test]
fn unknown_parameters_are_refused() {
    let i = inputs();
    let out = i.dir.path().join("o");
    for name in [
        "bogus",
        "Search:enzyme",
        "in",
        "out_idxml",
        "ini",
        "peptdeep:ms2_model",
        "rescore",
        "peptdeep:enable",
    ] {
        let err = build_args(
            &request(&[&i.mzml], &i.fasta, false, json!({ name: "x" })),
            &out,
        )
        .unwrap_err();
        assert!(
            err.contains(name),
            "the error names the parameter {name}: {err}"
        );
    }
}

#[test]
fn parameter_values_of_the_wrong_type_are_refused() {
    let i = inputs();
    let out = i.dir.path().join("o");
    for params in [
        json!({ "peptide:missed_cleavages": "two" }),
        json!({ "peptide:missed_cleavages": 1.5 }),
        json!({ "peptide:missed_cleavages": null }),
        json!({ "FDR:protein": "0.01" }),
        json!({ "enzyme": 3 }),
        json!({ "enzyme": "-in" }),
        json!({ "enzyme": "" }),
        json!({ "modifications:fixed": "Carbamidomethyl (C)" }),
        json!({ "modifications:fixed": ["-database"] }),
        json!({ "modifications:fixed": [1] }),
        json!({ "low_memory": "yes" }),
        json!({ "enzyme": "Tryp\nsin" }),
    ] {
        assert!(
            build_args(&request(&[&i.mzml], &i.fasta, false, params.clone()), &out).is_err(),
            "{params} is refused"
        );
    }
}

#[test]
fn integers_beyond_proses_32_bits_and_absurd_threads_are_refused() {
    let i = inputs();
    let out = i.dir.path().join("o");
    // ProSE reads its integers as 32-bit: 3000000000 made it print its 152 KB help and stop
    for (params, words) in [
        (
            json!({ "precursor:max_charge": 3_000_000_000_u64 }),
            "precursor:max_charge",
        ),
        (
            json!({ "precursor:isotope_error_min": -2_147_483_649_i64 }),
            "precursor:isotope_error_min",
        ),
        (
            json!({ "peptide:missed_cleavages": 9.0e15 }),
            "peptide:missed_cleavages",
        ),
        (json!({ "threads": 4_294_967_295_u64 }), "threads"),
        (json!({ "threads": 1025 }), "threads"),
    ] {
        let err =
            build_args(&request(&[&i.mzml], &i.fasta, false, params.clone()), &out).unwrap_err();
        assert!(err.contains(words), "{params}: {err}");
    }
    let err = build_args(
        &request(
            &[&i.mzml],
            &i.fasta,
            false,
            json!({ "precursor:max_charge": 3_000_000_000_u64 }),
        ),
        &out,
    )
    .unwrap_err();
    assert!(
        err.contains("2147483647"),
        "the message names the range: {err}"
    );
    // the request's own field too
    let mut req = request(&[&i.mzml], &i.fasta, false, json!({}));
    req.settings.threads = 100_000;
    assert!(build_args(&req, &out).unwrap_err().contains("1024"));
    // the bounds themselves pass
    let args = args_of(
        &request(
            &[&i.mzml],
            &i.fasta,
            false,
            json!({ "precursor:max_charge": 2_147_483_647_i64, "precursor:isotope_error_min": -2_147_483_648_i64, "threads": 1024 }),
        ),
        &out,
    );
    assert_eq!(
        values_of(&args, "-Search:precursor:max_charge"),
        vec![OsString::from("2147483647")]
    );
    assert_eq!(
        values_of(&args, "-Search:precursor:isotope_error_min"),
        vec![OsString::from("-2147483648")]
    );
    assert_eq!(values_of(&args, "-threads"), vec![OsString::from("1024")]);
}

#[test]
fn co_isolated_precursors_are_left_out_with_database_chunks() {
    let i = inputs();
    let out = i.dir.path().join("o");
    let with = |chunk: i64| {
        args_of(
            &request(
                &[&i.mzml],
                &i.fasta,
                false,
                json!({ "chimeric:enabled": true, "database:chunk_size": chunk }),
            ),
            &out,
        )
    };
    assert!(has(&with(0), "-Search:chimeric:enabled"));
    let chunked = with(1000);
    assert!(
        !has(&chunked, "-Search:chimeric:enabled"),
        "ProSE would ignore it"
    );
    assert_eq!(
        values_of(&chunked, "-Search:database:chunk_size"),
        vec![OsString::from("1000")]
    );
    // a wrong value is still refused
    assert!(build_args(
        &request(
            &[&i.mzml],
            &i.fasta,
            false,
            json!({ "chimeric:enabled": "yes", "database:chunk_size": 1000 })
        ),
        &out
    )
    .is_err());
}

#[test]
fn the_psm_fdr_is_above_zero() {
    let i = inputs();
    let out = i.dir.path().join("o");
    // absent: the shim sends 1 %, because ProSE's default (0) leaves a search without q-values
    let args = args_of(&request(&[&i.mzml], &i.fasta, false, json!({})), &out);
    assert_eq!(
        values_of(&args, "-Search:FDR:PSM"),
        vec![OsString::from("0.01")]
    );
    for fdr in [json!(0), json!(-0.5)] {
        let err = build_args(
            &request(&[&i.mzml], &i.fasta, false, json!({ "FDR:PSM": fdr })),
            &out,
        )
        .unwrap_err();
        assert!(err.contains("FDR:PSM"), "{err}");
    }
}

#[test]
fn a_search_without_decoys_has_no_fdr() {
    let i = inputs();
    let out = i.dir.path().join("o");
    let fdr = json!({ "FDR:PSM": 0.01, "FDR:protein": 0.05, "FDR:protein_inference_PSM": 0.1 });
    let mut without = fdr.clone();
    without["decoys"] = json!("ignore");
    // decoys 'ignore': no FDR is sent (ProSE would skip it), not even the shim's default PSM FDR, and 0 is no error
    for params in [
        without,
        json!({ "decoys": "ignore" }),
        json!({ "decoys": "ignore", "FDR:PSM": 0 }),
    ] {
        let args = args_of(&request(&[&i.mzml], &i.fasta, false, params.clone()), &out);
        for name in [
            "-Search:FDR:PSM",
            "-Search:FDR:protein",
            "-Search:FDR:protein_inference_PSM",
        ] {
            assert!(!has(&args, name), "{name} sent with {params}");
        }
        assert_eq!(
            values_of(&args, "-Search:decoys"),
            vec![OsString::from("ignore")]
        );
    }
    // with decoys they are sent
    for decoys in ["auto", "generate"] {
        let mut params = fdr.clone();
        params["decoys"] = json!(decoys);
        let args = args_of(&request(&[&i.mzml], &i.fasta, false, params), &out);
        assert_eq!(
            values_of(&args, "-Search:FDR:PSM"),
            vec![OsString::from("0.01")]
        );
        assert_eq!(
            values_of(&args, "-Search:FDR:protein"),
            vec![OsString::from("0.05")]
        );
    }
    // a value of the wrong type is still refused
    assert!(build_args(
        &request(
            &[&i.mzml],
            &i.fasta,
            false,
            json!({ "decoys": "ignore", "FDR:PSM": "x" })
        ),
        &out
    )
    .is_err());
}

#[test]
fn input_paths_are_validated() {
    let i = inputs();
    let out = i.dir.path().join("o");
    let missing = i.dir.path().join("data/missing.mzML");
    let folder = i.dir.path().join("data");
    let raw = i.dir.write("data/a.wiff", "x");
    let text = i.dir.write("data/db.txt", ">a\nMK\n");
    assert!(
        build_args(&request(&[&missing], &i.fasta, false, json!({})), &out)
            .unwrap_err()
            .contains("missing.mzML")
    );
    assert!(
        build_args(&request(&[&folder], &i.fasta, false, json!({})), &out).is_err(),
        "a folder is not a file"
    );
    assert!(
        build_args(&request(&[&raw], &i.fasta, false, json!({})), &out)
            .unwrap_err()
            .contains("a.wiff")
    );
    // Thermo .raw and mzPeak files and Bruker .d folders are inputs ProSE reads
    let thermo = i.dir.write("data/b.raw", "x");
    assert!(build_args(&request(&[&thermo], &i.fasta, false, json!({})), &out).is_ok());
    let bruker = i.dir.path().join("data/c.d");
    std::fs::create_dir_all(&bruker).unwrap();
    assert!(build_args(&request(&[&bruker], &i.fasta, false, json!({})), &out).is_ok());
    // a regular file named .d is no Bruker folder
    let fake = i.dir.write("data/fake.d", "x");
    assert!(
        build_args(&request(&[&fake], &i.fasta, false, json!({})), &out)
            .unwrap_err()
            .contains("Bruker .d folder")
    );
    assert!(
        build_args(&request(&[&i.mzml], &text, false, json!({})), &out)
            .unwrap_err()
            .contains("db.txt")
    );
    assert!(build_args(
        &request(
            &[&i.mzml],
            &i.dir.path().join("none.fasta"),
            false,
            json!({})
        ),
        &out
    )
    .is_err());
    assert!(
        build_args(&request(&[], &i.fasta, false, json!({})), &out).is_err(),
        "no input file"
    );
    // the extensions the app takes
    let gz = i.dir.write("data/b.MZML.GZ", "gz");
    let fa = i.dir.write("data/db.fa", ">a\nMK\n");
    let faa = i.dir.write("data/db.faa", ">a\nMK\n");
    for db in [&fa, &faa] {
        assert!(build_args(&request(&[&i.mzml, &gz], db, false, json!({})), &out).is_ok());
    }
}

#[test]
fn relative_paths_are_refused_so_none_can_pass_for_an_option() {
    let i = inputs();
    let out = i.dir.path().join("o");
    let json = |files: Value, database: Value| -> SearchRequest {
        serde_json::from_value(json!({
            "files": files, "database": database, "contaminants": false,
            "settings": { "parameters": {}, "rescore": true, "peptdeep": false, "threads": 1 },
        }))
        .unwrap()
    };
    let good_file = json!([{ "id": "f", "path": i.mzml }]);
    let good_db = json!(i.fasta);
    for path in [
        "-in.mzML",
        "-out_qpx",
        "data/a.mzML",
        "a.mzML",
        "./a.mzML",
        "../a.mzML",
    ] {
        let err = build_args(
            &json(json!([{ "id": "f", "path": path }]), good_db.clone()),
            &out,
        )
        .unwrap_err();
        assert!(
            err.contains("absolute") && err.contains(path),
            "{path}: {err}"
        );
    }
    for path in ["-database.fasta", "-ini", "db.fasta", "sub/db.fasta"] {
        let err = build_args(&json(good_file.clone(), json!(path)), &out).unwrap_err();
        assert!(
            err.contains("absolute") && err.contains(path),
            "{path}: {err}"
        );
    }
    assert!(build_args(&json(good_file, good_db), &out).is_ok());
}

#[test]
fn contaminants_swap_the_database_for_the_search_folders_copy() {
    let i = inputs();
    let out = i.dir.path().join("o");
    let with = args_of(&request(&[&i.mzml], &i.fasta, true, json!({})), &out);
    assert_eq!(
        values_of(&with, "-database"),
        vec![out.join("database.fasta").into_os_string()]
    );
    let without = args_of(&request(&[&i.mzml], &i.fasta, false, json!({})), &out);
    assert_eq!(
        values_of(&without, "-database"),
        vec![i.fasta.clone().into_os_string()]
    );
}

#[test]
fn contaminants_are_appended_with_con_accessions() {
    let dir = TempDir::new("crap");
    // the user's FASTA has no final newline; one cRAP entry carries the prefix already
    let user = dir.write("user.fasta", ">sp|P1|ONE desc\nMKWV\nTFIS");
    let crap = dir.write(
        "crap.fasta",
        ">sp|ALBU_BOVIN|\nMKWVT\nFISL\n>CON__kept\nAAAA\n>gi|2|keratin\nGGGG\n",
    );
    let dest = dir.path().join("database.fasta");
    with_contaminants(&user, &crap, &dest, &DecoyRules::default()).unwrap();
    assert_eq!(
        fs::read_to_string(&dest).unwrap(),
        ">sp|P1|ONE desc\nMKWV\nTFIS\n>CON__sp|ALBU_BOVIN|\nMKWVT\nFISL\n>CON__kept\nAAAA\n>CON__gi|2|keratin\nGGGG\n"
    );
    // the user's file is untouched, and a final newline is not doubled
    assert_eq!(
        fs::read_to_string(&user).unwrap(),
        ">sp|P1|ONE desc\nMKWV\nTFIS"
    );
    let user2 = dir.write("user2.fasta", ">x\nMK\n");
    with_contaminants(&user2, &crap, &dest, &DecoyRules::default()).unwrap();
    assert!(fs::read_to_string(&dest)
        .unwrap()
        .starts_with(">x\nMK\n>CON__sp|ALBU_BOVIN|\n"));
}

#[test]
fn the_bundled_crap_has_116_entries_and_gets_the_prefix() {
    let dir = TempDir::new("bundled-crap");
    let crap = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fasta/crap.fasta");
    let user = dir.write("user.fasta", ">a\nMK\n");
    let dest = dir.path().join("database.fasta");
    with_contaminants(&user, &crap, &dest, &DecoyRules::default()).unwrap();
    let text = fs::read_to_string(&dest).unwrap();
    let headers: Vec<&str> = text.lines().filter(|l| l.starts_with('>')).collect();
    assert_eq!(headers.len(), 117);
    assert_eq!(headers[0], ">a");
    assert!(headers[1..].iter().all(|h| h.starts_with(">CON__")));
}

fn bundled_crap() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fasta/crap.fasta")
}

fn headers(text: &str) -> Vec<&str> {
    text.lines().filter(|l| l.starts_with('>')).collect()
}

#[test]
fn contaminants_the_database_has_already_are_not_appended_again() {
    let dir = TempDir::new("crap-twice");
    let crap = dir.write(
        "crap.fasta",
        ">sp|ALBU_BOVIN| serum albumin\nMKWVT\n>gi|2|keratin\nGGGG\n",
    );
    // one of them (with the prefix, and a description of its own) is in the user's FASTA
    let user = dir.write(
        "user.fasta",
        ">sp|P1|ONE\nMKWV\n>CON__sp|ALBU_BOVIN| own text\r\nMKWVT\r\n",
    );
    let dest = dir.path().join("database.fasta");
    with_contaminants(&user, &crap, &dest, &DecoyRules::default()).unwrap();
    assert_eq!(
        fs::read_to_string(&dest).unwrap(),
        ">sp|P1|ONE\nMKWV\n>CON__sp|ALBU_BOVIN| own text\r\nMKWVT\r\n>CON__gi|2|keratin\nGGGG\n"
    );
    // the database.fasta of an earlier search, searched again with contaminants: nothing is added
    let first = dir.path().join("first.fasta");
    let again = dir.path().join("again.fasta");
    let plain = dir.write("plain.fasta", ">a\nMK\n");
    with_contaminants(&plain, &bundled_crap(), &first, &DecoyRules::default()).unwrap();
    with_contaminants(&first, &bundled_crap(), &again, &DecoyRules::default()).unwrap();
    assert_eq!(
        fs::read_to_string(&again).unwrap(),
        fs::read_to_string(&first).unwrap()
    );
    let text = fs::read_to_string(&again).unwrap();
    let all = headers(&text);
    let unique: std::collections::HashSet<&&str> = all.iter().collect();
    assert_eq!((all.len(), unique.len()), (117, 117));
}

#[test]
fn a_database_with_decoys_gets_decoys_of_the_contaminants_too() {
    let dir = TempDir::new("crap-decoys");
    let crap = dir.write("crap.fasta", ">sp|TRY|trypsin\nMABCKDE\n>CON__kept\nAKPR\n");
    let dest = dir.path().join("database.fasta");
    // a prefix as DecoyHelper knows it, spelled as the database spells it
    let user = dir.write(
        "decoys.fasta",
        ">P1 one\nMKWV\n>P2\nAAAK\n>rev_P1 one\nMVWK\n>rev_P2\nAAAK\n",
    );
    // (decoys auto: ProSE reuses the database's decoys; the tool's default, generate, makes them all itself)
    let auto = DecoyRules {
        mode: "auto".into(),
        ..DecoyRules::default()
    };
    with_contaminants(&user, &crap, &dest, &auto).unwrap();
    let text = fs::read_to_string(&dest).unwrap();
    assert!(
        text.ends_with(
            ">CON__sp|TRY|trypsin\nMABCKDE\n>CON__kept\nAKPR\n>rev_CON__sp|TRY|trypsin\nMCBAKED\n>rev_CON__kept\nRPKA\n"
        ),
        "{text}"
    );
    // targets and decoys pair up again
    let decoys = headers(&text)
        .iter()
        .filter(|h| h.starts_with(">rev_"))
        .count();
    assert_eq!(headers(&text).len(), 2 * decoys);
    // a suffix, and Trypsin/P without keeping the initial Met
    let suffixed = dir.write("suffixed.fasta", ">P1\nMK\n>P1_REVERSED\nKM\n");
    let rules = DecoyRules::of(
        json!({ "decoys": "auto", "enzyme": "Trypsin/P", "peptide:clip_nterm_methionine": false })
            .as_object()
            .unwrap(),
    );
    with_contaminants(&suffixed, &crap, &dest, &rules).unwrap();
    let text = fs::read_to_string(&dest).unwrap();
    assert!(
        text.ends_with(">CON__sp|TRY|trypsin_REVERSED\nCBAMKED\n>CON__kept_REVERSED\nAKRP\n"),
        "{text}"
    );
    // ProSE removes the database's decoys and generates all of them (generate, the default; ignore), or the database
    // has none: no decoys from the app
    for (db, params) in [
        (&user, json!({})),
        (&user, json!({ "decoys": "generate" })),
        (&user, json!({ "decoys": "ignore" })),
        (
            &dir.write("targets.fasta", ">P1\nMK\n>P2\nAK\n"),
            json!({ "decoys": "auto" }),
        ),
    ] {
        with_contaminants(
            db,
            &crap,
            &dest,
            &DecoyRules::of(params.as_object().unwrap()),
        )
        .unwrap();
        let text = fs::read_to_string(&dest).unwrap();
        assert!(
            headers(&text)
                .iter()
                .all(|h| !h.contains("rev_CON") && !h.contains("REVERSED")),
            "{params}: {text}"
        );
    }
}

#[test]
fn a_database_without_proteins_is_refused_with_contaminants() {
    let dir = TempDir::new("crap-empty");
    let dest = dir.path().join("database.fasta");
    for (name, contents) in [
        ("empty.fasta", ""),
        ("blank.fasta", "\n\n  \n"),
        ("nohdr.fasta", "MKWVTFISLLK\n"),
    ] {
        let user = dir.write(name, contents);
        let err =
            with_contaminants(&user, &bundled_crap(), &dest, &DecoyRules::default()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "{name}");
        assert!(err.to_string().contains("has no proteins"), "{name}: {err}");
        assert!(!dest.exists(), "{name}: no half-written copy is left");
    }
    // a byte order mark before the first header is no problem
    let bom = dir.write("bom.fasta", "\u{feff}>P1\nMK\n");
    with_contaminants(&bom, &bundled_crap(), &dest, &DecoyRules::default()).unwrap();
    // through start_search: refused, and nothing is left
    let run = Run::new();
    let i = inputs();
    let empty = i.dir.write("data/empty.fasta", "");
    let (id, _rx) = run.start(
        request(&[&i.mzml], &empty, true, json!({})),
        Some(&bundled_crap()),
    );
    let err = id.unwrap_err();
    assert!(
        err.contains("has no proteins") && err.contains("empty.fasta"),
        "{err}"
    );
    assert!(run.listing().is_empty(), "{:?}", run.listing());
}

#[test]
fn on_windows_inputs_go_by_their_own_paths_unless_their_names_clash() {
    let dir = TempDir::new("placements");
    let a = dir.write("one/a.mzML", "<mzML/>");
    let b = dir.write("one/b.mzML.gz", "gz");
    let a_upper = dir.write("two/A.MZML", "<mzML/>");
    let a_gz = dir.write("three/a.mzML.gz", "gz");
    let fasta = dir.write("db.fasta", ">a\nMK\n");
    let out = dir.path().join("s.partial");
    let windows = InputLinks::PathsOrHardLinks;
    let plan = |files: &[&Path]| {
        let req = request(files, &fasta, false, json!({}));
        let args = build_args_with(&req, &out, windows).unwrap();
        let records = input_records_with(&req, windows, &out);
        (
            input_placements(&req, windows, &out),
            values_of(&args, "-in"),
            records
                .iter()
                .map(|r| (r.link.clone(), r.stem.clone()))
                .collect::<Vec<_>>(),
        )
    };

    // names that differ: the user's paths themselves, and ProSE names the outputs after them
    let (placements, ins, records) = plan(&[&a, &b]);
    assert_eq!(placements, [Placement::OwnPath, Placement::OwnPath]);
    assert_eq!(
        ins,
        [a.clone().into_os_string(), b.clone().into_os_string()]
    );
    assert_eq!(
        records,
        [
            ("a.mzML".to_string(), "a".to_string()),
            ("b.mzML.gz".to_string(), "b".to_string())
        ]
    );
    // one stem, ignoring case (NTFS) or the .gz: the clashing ones as hard links in the folder
    let (placements, ins, records) = plan(&[&a, &b, &a_upper, &a_gz]);
    use Placement::*;
    assert_eq!(placements, [HardLink, OwnPath, HardLink, HardLink]);
    assert_eq!(
        ins,
        [
            out.join("1__a.mzML").into_os_string(),
            b.clone().into_os_string(),
            out.join("3__A.MZML").into_os_string(),
            out.join("4__a.mzML.gz").into_os_string(),
        ]
    );
    assert_eq!(
        records
            .iter()
            .map(|(_, stem)| stem.as_str())
            .collect::<Vec<_>>(),
        ["1__a", "b", "3__A", "4__a"]
    );
    // a file whose own name looks like a link beside the clash it would meet: all of them as links
    let tricky = dir.write("four/1__a.mzML", "<mzML/>");
    let (placements, _, records) = plan(&[&a, &a_upper, &tricky]);
    assert_eq!(placements, [HardLink, HardLink, HardLink]);
    assert_eq!(
        records
            .iter()
            .map(|(link, _)| link.as_str())
            .collect::<Vec<_>>(),
        ["1__a.mzML", "2__A.MZML", "3__1__a.mzML"]
    );
    // an input of stem `quantms`, which ProSE refuses (its merged QPX views are quantms.*): as a link
    let quantms = dir.write("five/QuantMS.mzML", "<mzML/>");
    let (placements, ins, records) = plan(&[&b, &quantms]);
    assert_eq!(placements, [OwnPath, HardLink]);
    assert_eq!(ins[1], out.join("2__QuantMS.mzML").into_os_string());
    assert_eq!(records[1].1, "2__QuantMS");
    // elsewhere every input is a symbolic link
    let req = request(&[&a, &b], &fasta, false, json!({}));
    assert_eq!(
        input_placements(&req, InputLinks::Symlinks, &out),
        [Symlink, Symlink]
    );
    assert_eq!(
        InputLinks::PLATFORM,
        if cfg!(windows) {
            windows
        } else {
            InputLinks::Symlinks
        }
    );
}

#[test]
fn input_links_are_known_by_their_names() {
    for name in ["1__a.mzML", "12__x y.mzML.gz", "3__1__a.MZML"] {
        assert!(is_input_link(name), "{name}");
    }
    for name in [
        "a.mzML",
        "__a.mzML",
        "x__a.mzML",
        "1__a.fasta",
        "1__",
        "summary.yaml",
        "1_a.mzML",
    ] {
        assert!(!is_input_link(name), "{name}");
    }
}

#[test]
fn link_names_keep_the_name_and_the_gz_suffix() {
    assert_eq!(link_name(0, "/data/x.mzML"), "1__x.mzML");
    assert_eq!(link_name(9, "/data/x.mzML.gz"), "10__x.mzML.gz");
    assert_eq!(link_name(0, "/Daten ä/ünï cøde.mzML"), "1__ünï cøde.mzML");
    assert_eq!(input_stem("1__x.mzML"), "1__x");
    assert_eq!(input_stem("10__x.mzML.gz"), "10__x");
    assert_eq!(input_stem("2__a.b.MZML.GZ"), "2__a.b");
}

#[test]
fn link_names_of_long_file_names_are_shortened_to_a_unique_stem_with_the_suffix() {
    // 255 bytes: the longest name a file can have, and `1__` makes the link too long for the file system
    let long = format!("{}.mzML", "x".repeat(250));
    let link = link_name(0, &format!("/data/{long}"));
    assert!(
        link.starts_with("1__xxx") && link.ends_with(".mzML"),
        "{link}"
    );
    assert_eq!(input_stem(&link).len(), MAX_STEM, "{link}");
    // ProSE's longest output name fits
    assert!(
        format!(
            "{}_ModificationAnalysis_DeltaMassStats.tsv",
            input_stem(&link)
        )
        .len()
            <= 255
    );
    assert!(is_input_link(&link));
    // `<n>__` keeps two such inputs apart, and the suffix stays as it is
    let gz = link_name(11, &format!("/data/{}.MZML.GZ", "x".repeat(246)));
    assert!(
        gz.starts_with("12__xxx") && gz.ends_with(".MZML.GZ"),
        "{gz}"
    );
    assert_ne!(
        input_stem(&link),
        input_stem(&link_name(1, &format!("/data/{long}")))
    );
    // shortened at a character, never inside one
    let umlauts = link_name(0, &format!("/data/{}.mzML", "ä".repeat(125)));
    assert!(
        input_stem(&umlauts).len() <= MAX_STEM && umlauts.ends_with(".mzML"),
        "{umlauts}"
    );
    assert!(umlauts
        .trim_start_matches("1__")
        .trim_end_matches(".mzML")
        .chars()
        .all(|c| c == 'ä'));
    // a name that fits stays as it is
    let fits = format!("{}.mzML", "y".repeat(MAX_STEM - 3));
    assert_eq!(link_name(0, &format!("/data/{fits}")), format!("1__{fits}"));
}

/// Names whose lower-cased form has another length in bytes: `İ` (2 bytes, lower case 3), `Ⱥ` (2 -> 3),
/// `ẞ` (3 -> 2) and the Kelvin sign (3 -> 1). The stem is ProSE's (`File::stemName`: ASCII suffixes,
/// ignoring their case), and no name makes the shim panic.
#[test]
fn input_stems_of_names_whose_lower_case_has_another_length() {
    for (link, stem) in [
        ("1__İstanbul_run.mzML", "1__İstanbul_run"),
        ("1__İstanbul_run.mzML.gz", "1__İstanbul_run"),
        ("1__Ⱥ.mzML.gz", "1__Ⱥ"),
        ("1__Ⱥ.mzML", "1__Ⱥ"),
        ("2__ẞ.mzML", "2__ẞ"),
        ("2__ẞẞ.MzMl.gZ", "2__ẞẞ"),
        ("3__\u{212A}.mzML", "3__\u{212A}"),
        ("3__\u{212A}\u{212A}.mzML.GZ", "3__\u{212A}\u{212A}"),
        ("4__İ.mZmL", "4__İ"),
        ("5__a.mzML.mzML", "5__a.mzML"),
        ("6__Ⱥ.gz", "6__Ⱥ"),
        (".mzML", ""),
        ("İ", "İ"),
        ("", ""),
    ] {
        assert_eq!(input_stem(link), stem, "{link}");
    }
    for name in ["İstanbul.mzML", "Ⱥ.MZML.GZ", "ẞ.mzml", "\u{212A}.mZmL.Gz"] {
        assert!(prose_desktop_lib::args::is_spectrum_file(name), "{name}");
    }
    for name in ["Ⱥ.mzM", "x.mzml.gzip", "x.gz", "ẞ"] {
        assert!(!prose_desktop_lib::args::is_spectrum_file(name), "{name}");
    }
    // the records of a search with such inputs carry these stems (the reader looks for <stem>.psm.parquet)
    let req = request(
        &[
            Path::new("/data/İstanbul_run.mzML"),
            Path::new("/data/Ⱥ.mzML.gz"),
            Path::new("/data/ẞ.MZML"),
        ],
        Path::new("/db/x.fasta"),
        false,
        json!({}),
    );
    let folder = Path::new("/s/1-0.partial");
    let stems: Vec<String> = input_records_with(&req, InputLinks::Symlinks, folder)
        .into_iter()
        .map(|r| r.stem)
        .collect();
    assert_eq!(stems, ["1__İstanbul_run", "2__Ⱥ", "3__ẞ"]);
    let own: Vec<String> = input_records_with(&req, InputLinks::PathsOrHardLinks, folder)
        .into_iter()
        .map(|r| r.stem)
        .collect();
    assert_eq!(own, ["İstanbul_run", "Ⱥ", "ẞ"]);
}

//! Finding the ProSE tool and probing it: version, path, ONNX support, its environment.

use super::*;
use prose_desktop_lib::args::with_idxml_option;
use prose_desktop_lib::tool;
use std::ffi::OsStr;
use std::fs;

fn executable(dir: &TempDir, relative: &str) -> PathBuf {
    let path = dir.write(relative, "#!/bin/sh\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[test]
fn the_tool_is_found_through_prose_bin_then_the_bundle_then_path() {
    let dir = TempDir::new("resolve");
    let env_bin = executable(&dir, "env/ProSE");
    let bundled = executable(&dir, &format!("res/resources/prose/bin/{}", tool::EXE));
    let on_path = executable(&dir, &format!("pathdir/{}", tool::EXE));
    let res = dir.path().join("res");
    let path_var = OsString::from(dir.path().join("pathdir"));

    let all = |bin: Option<&Path>| {
        tool::resolve_with(bin.map(Path::as_os_str), Some(&res), Some(&path_var))
    };
    assert_eq!(all(Some(&env_bin)).unwrap(), env_bin);
    assert_eq!(all(None).unwrap(), bundled);
    fs::remove_file(&bundled).unwrap();
    assert_eq!(all(None).unwrap(), on_path);
    fs::remove_file(&on_path).unwrap();
    let err = all(None).unwrap_err();
    assert!(err.contains("ProSE"), "{err}");

    // a PROSE_BIN that does not exist is an error, not a reason to look elsewhere
    let err = tool::resolve_with(
        Some(OsStr::new("/nonexistent/ProSE")),
        Some(&res),
        Some(&path_var),
    )
    .unwrap_err();
    assert!(
        err.contains("PROSE_BIN") && err.contains("/nonexistent/ProSE"),
        "{err}"
    );
}

/// macOS: Tauri names no resource folder when the app's path goes through a symbolic link; the app then
/// takes the Resources folder of the bundle its executable really is in.
#[cfg(unix)]
#[test]
fn the_resources_of_an_app_bundle_are_found_through_a_link() {
    let dir = TempDir::new("bundle");
    let exe = executable(&dir, "real/ProSE.app/Contents/MacOS/prose-desktop");
    dir.write(
        "real/ProSE.app/Contents/Resources/resources/prose/build.json",
        "{}",
    );
    let resources = dir
        .path()
        .join("real/ProSE.app/Contents/Resources")
        .canonicalize()
        .unwrap();
    assert_eq!(tool::bundle_resources(&exe), Some(resources.clone()));
    // the app through a linked folder, and as a linked bundle
    std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("linked")).unwrap();
    std::os::unix::fs::symlink(
        dir.path().join("real/ProSE.app"),
        dir.path().join("Linked.app"),
    )
    .unwrap();
    for through in [
        "linked/ProSE.app/Contents/MacOS/prose-desktop",
        "Linked.app/Contents/MacOS/prose-desktop",
    ] {
        assert_eq!(
            tool::bundle_resources(&dir.path().join(through)),
            Some(resources.clone()),
            "{through}"
        );
    }
    // not in a bundle: no Resources folder, none
    let loose = executable(&dir, "bin/prose-desktop");
    assert_eq!(tool::bundle_resources(&loose), None);
    assert_eq!(tool::bundle_resources(&dir.path().join("nothing")), None);
}

#[test]
fn the_tools_version_is_read_from_its_help() {
    let help = "ProSE -- Annotates bottom-up MS/MS spectra using ProSE.\nFull documentation: http://x\nVersion: 3.7.0-pre-exported-20261020 Oct 20 2026, 22:16:02\nTo cite OpenMS:\n";
    assert_eq!(
        tool::parse_version(help).as_deref(),
        Some("3.7.0-pre-exported-20261020")
    );
    assert_eq!(tool::parse_version("nothing"), None);
}

/// A one-input search's idXML goes by the option the tool lists: `-out`, or `-out_idxml` for a ProSE older than
/// OpenMS 3.7 (whose help lists that and no `-out`); the command line then has that option in place of `-out`.
#[test]
fn a_one_input_searchs_idxml_takes_the_option_the_tool_lists() {
    let new = "Options (mandatory options marked with '*'):\n  -in <files>*        Input files\n  -out <file>         Output idXML\n  -out_qpx <dir>      QPX\n";
    let old = "Options (mandatory options marked with '*'):\n  -in <files>*        Input files\n  -out_idxml <file>   Output idXML\n  -out_qpx <dir>      QPX\n";
    assert_eq!(tool::idxml_option(new), "-out");
    assert_eq!(tool::idxml_option(old), "-out_idxml");
    assert_eq!(tool::idxml_option("Version: 9.9.9"), "-out");
    let i = inputs();
    let out = i.dir.path().join("o");
    let args = with_idxml_option(
        args_of(&request(&[&i.mzml], &i.fasta, false, json!({})), &out),
        "-out_idxml",
    );
    assert_eq!(
        values_of(&args, "-out_idxml"),
        vec![out.join("search.idXML").into_os_string()]
    );
    assert!(!has(&args, "-out"));
    // the fake tool lists neither: `-out`
    assert_eq!(tool::idxml_option_of(&fake_prose()), "-out");
}

/// `tool::probe` of a script this test process has just written. Another test's child, forked while the script was
/// open for writing, holds that descriptor until its own exec; meanwhile exec of the script fails with ETXTBSY ("Text
/// file busy", os error 26). A test-only race (the app never runs a file it wrote): retried for up to 2 s.
fn probe_written_script(script: &Path) -> Result<tool::ToolInfo, String> {
    let mut result = tool::probe(script);
    for _ in 0..100 {
        match &result {
            Err(e) if e.contains("os error 26") => {
                std::thread::sleep(std::time::Duration::from_millis(20));
                result = tool::probe(script);
            }
            _ => break,
        }
    }
    result
}

#[test]
fn probing_a_tool_reports_its_version_path_and_onnx_support() {
    let dir = TempDir::new("probe");
    // a script on Unix; on Windows the fake tool, which answers --helphelp (what probe asks) the same way
    #[cfg(unix)]
    let script = executable(&dir, "ProSE");
    #[cfg(unix)]
    fs::write(&script, "#!/bin/sh\necho 'Version: 9.9.9 Oct 20 2026'\n").unwrap();
    #[cfg(not(unix))]
    let script = fake_prose_exe();
    let info = probe_written_script(&script).unwrap();
    assert_eq!(info.version, "9.9.9");
    assert_eq!(info.path, script.to_string_lossy());
    assert!(!info.onnx, "a tool without a build manifest has no ONNX");
    assert!(tool::probe(&dir.path().join("none")).is_err());
}

#[test]
fn onnx_support_comes_from_the_bundles_manifest_and_its_models() {
    let dir = TempDir::new("onnx");
    let bin = executable(&dir, "prose/bin/ProSE");
    let models = [
        "share/OpenMS/models/peptdeep_ms2_dynamic.onnx",
        "share/OpenMS/models/peptdeep_rt_dynamic.onnx",
    ];
    let manifest = |with_onnx: bool| {
        dir.write(
            "prose/build.json",
            &serde_json::json!({ "tool": "ProSE", "withOnnx": with_onnx, "models": models })
                .to_string(),
        );
    };

    // no manifest (a tool from a build tree or the PATH): no ONNX, whatever it links
    assert!(!tool::has_onnx(&bin));
    // a build with ONNX, but the models are missing
    manifest(true);
    assert!(!tool::has_onnx(&bin));
    for model in models {
        dir.write(&format!("prose/{model}"), "onnx");
    }
    assert!(tool::has_onnx(&bin));
    // one model gone
    fs::remove_file(dir.path().join("prose").join(models[1])).unwrap();
    assert!(!tool::has_onnx(&bin));
    dir.write(&format!("prose/{}", models[1]), "onnx");
    // a build without ONNX, models or not
    manifest(false);
    assert!(!tool::has_onnx(&bin));
    // a manifest that names no models, or cannot be read
    dir.write("prose/build.json", r#"{"withOnnx": true, "models": []}"#);
    assert!(!tool::has_onnx(&bin));
    dir.write("prose/build.json", "not json");
    assert!(!tool::has_onnx(&bin));
    // a model path that leaves the bundle does not count
    dir.write("outside.onnx", "onnx");
    dir.write(
        "prose/build.json",
        r#"{"withOnnx": true, "models": ["../outside.onnx"]}"#,
    );
    assert!(!tool::has_onnx(&bin));
}

#[test]
fn the_tool_runs_with_one_blas_thread_unless_set() {
    let cmd = tool::command_with(Path::new("/x/ProSE"), None);
    let env: Vec<_> = cmd.get_envs().collect();
    assert!(env.contains(&(OsStr::new("OPENBLAS_NUM_THREADS"), Some(OsStr::new("1")))));
    let cmd = tool::command_with(Path::new("/x/ProSE"), Some(OsString::from("4")));
    assert!(
        cmd.get_envs().all(|(k, _)| k != "OPENBLAS_NUM_THREADS"),
        "the user's value stays"
    );
    // macOS: the loader's search variables never reach the bundled tool
    for name in tool::LOADER_VARIABLES {
        let removed = cmd.get_envs().any(|(k, v)| k == *name && v.is_none());
        assert_eq!(removed, cfg!(target_os = "macos"), "{name}");
    }
}

#[test]
fn the_tool_runs_on_its_bundled_dotnet_if_it_has_one() {
    let dir = TempDir::new("dotnet");
    let tool_path = dir.write("prose/bin/ProSE", "");
    let dotnet_root = |cmd: &std::process::Command| {
        cmd.get_envs()
            .find(|(k, _)| *k == "DOTNET_ROOT")
            .map(|(_, v)| v.map(PathBuf::from))
    };
    // no bundled runtime: DOTNET_ROOT is left as it is (a system .NET is found by the Thermo bridge)
    assert_eq!(tool::bundled_dotnet(&tool_path), None);
    assert_eq!(dotnet_root(&tool::command_with(&tool_path, None)), None);
    // a folder without host/fxr is no runtime
    fs::create_dir_all(dir.path().join("prose/dotnet/shared")).unwrap();
    assert_eq!(tool::bundled_dotnet(&tool_path), None);
    fs::create_dir_all(dir.path().join("prose/dotnet/host/fxr/10.0.14")).unwrap();
    let root = dir.path().join("prose/dotnet");
    assert_eq!(tool::bundled_dotnet(&tool_path), Some(root.clone()));
    assert_eq!(
        dotnet_root(&tool::command_with(&tool_path, None)),
        Some(Some(root))
    );
}

#[test]
fn the_tool_never_runs_the_update_check_of_openms() {
    for openblas in [None, Some(OsString::from("4"))] {
        let cmd = tool::command_with(Path::new("/x/ProSE"), openblas);
        let set: Vec<_> = cmd
            .get_envs()
            .filter(|(k, _)| *k == "OPENMS_DISABLE_UPDATE_CHECK")
            .collect();
        // set explicitly, so a user's OPENMS_DISABLE_UPDATE_CHECK=OFF (which turns the check on) is replaced
        assert_eq!(
            set,
            vec![(
                OsStr::new("OPENMS_DISABLE_UPDATE_CHECK"),
                Some(OsStr::new("ON"))
            )]
        );
    }
    assert_eq!(tool::NO_UPDATE_CHECK, ("OPENMS_DISABLE_UPDATE_CHECK", "ON"));
}

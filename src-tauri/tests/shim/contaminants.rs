//! The contaminants (cRAP) appended to the user's database, with decoys of them where the database has decoys.

use super::running::Run;
use super::*;
use prose_desktop_lib::contaminants::with_contaminants;
use prose_desktop_lib::decoys::DecoyRules;
use std::fs;

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

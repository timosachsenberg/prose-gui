//! Results archives: a search folder written as a zip entry by entry (`archive`), read back here as the front
//! end reads it: the central directory, then each entry's local header and data (stored or deflated, its CRC).
//! The zip of the fixture folder below is `src/lib/history/fixtures/rust-archive.prose`, which the front end's
//! tests open with `unpackArchive` (fflate): the round trip. `PROSE_WRITE_FIXTURES=1 cargo test --test archive`
//! writes it again after a change of the writer.

mod common;

use common::TempDir;
use flate2::read::DeflateDecoder;
use prose_desktop_lib::archive::{
    archive_files, dos_time, extract, read_directory, without_paths, write_archive, ZipEntry,
    ZipWriter, INPUTS, RECORD,
};
use prose_desktop_lib::history::{import_archive, list, save_record};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 2026-10-24 12:34:56 UTC: the time of the fixture's entries.
fn fixture_time() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_792_845_296)
}

/// A saved search folder as the app keeps it, with what an archive leaves out.
fn folder(dir: &Path, id: &str) {
    let folder = dir.join(id);
    fs::create_dir_all(folder.join("qpx/runs")).unwrap();
    fs::create_dir_all(folder.join("mods")).unwrap();
    // (Parquet: stored; text: deflated)
    let parquet: Vec<u8> = (0..5000u32)
        .flat_map(|i| (i * 7919).to_le_bytes())
        .collect();
    fs::write(folder.join("qpx/quantms.psm.parquet"), &parquet).unwrap();
    fs::write(folder.join("qpx/runs/1__a.mz.parquet"), b"PAR1 mz PAR1").unwrap();
    fs::write(
        folder.join("proteins.fasta"),
        ">P1 first protein\nPEPTIDEKPEPTIDEKPEPTIDEK\n".repeat(50),
    )
    .unwrap();
    fs::write(
        folder.join("mods/1__a_PTMStats.tsv"),
        "name\tcount\nOxidation\t3\n",
    )
    .unwrap();
    fs::write(folder.join("empty.txt"), "").unwrap();
    fs::write(
        folder.join(INPUTS),
        r#"[{"id":"f1","name":"a.mzML","path":"/home/me/runs/a.mzML"},{"id":"f2","path":"C:\\data\\b.mzML"}]"#,
    )
    .unwrap();
    // left out: the database copy, its path, the log
    fs::write(folder.join("database.fasta"), ">P1\nPEPTIDEK\n").unwrap();
    fs::write(
        folder.join("database.json"),
        r#"{"path":"/home/me/db.fasta"}"#,
    )
    .unwrap();
    fs::write(folder.join("prose.log"), "ProSE -in /home/me/runs/a.mzML").unwrap();
    let record = format!(
        r#"{{"schema":1,"id":"{id}","name":"Fixture","createdAt":"2026-10-24T12:34:56.000Z","appVersion":"0.1.3","toolVersion":"3.7.0","files":["a.mzML","b.mzML"],"database":"db.fasta","psms":2,"psmFdr":0.01,"proteinFdr":0.01,"seconds":1,"design":{{"rows":[],"samples":[],"conditions":[]}},"settings":{{"parameters":{{}},"fixedMods":[],"rescore":true,"peptdeep":false,"contaminants":true}}}}"#
    );
    save_record(dir, id, &record).unwrap();
}

struct Unzipped {
    name: String,
    method: u16,
    data: Vec<u8>,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// The entries of a zip, read as the front end does: the end record, the central directory, each entry's
/// local header and data, its size and CRC checked; no zip64, no data descriptor, UTF-8 names.
fn unzip(bytes: &[u8]) -> Vec<Unzipped> {
    let eocd = bytes.len() - 22;
    assert_eq!(
        u32_at(bytes, eocd),
        0x0605_4b50,
        "the end record ends the file (no comment)"
    );
    let count = u16_at(bytes, eocd + 10) as usize;
    let size = u32_at(bytes, eocd + 12) as usize;
    let start = u32_at(bytes, eocd + 16) as usize;
    assert_eq!(start + size, eocd);
    let mut at = start;
    let mut out = Vec::new();
    for _ in 0..count {
        assert_eq!(u32_at(bytes, at), 0x0201_4b50);
        assert_eq!(
            u16_at(bytes, at + 4) >> 8,
            0,
            "made on MS-DOS: no Unix modes, no links"
        );
        let flags = u16_at(bytes, at + 8);
        assert_eq!(
            flags, 0x0800,
            "a UTF-8 name, no data descriptor, no encryption"
        );
        let method = u16_at(bytes, at + 10);
        let crc = u32_at(bytes, at + 16);
        let compressed = u32_at(bytes, at + 20) as usize;
        let length = u32_at(bytes, at + 24) as usize;
        let name_length = u16_at(bytes, at + 28) as usize;
        assert_eq!(u16_at(bytes, at + 30), 0, "no extra field");
        let offset = u32_at(bytes, at + 42) as usize;
        let name = String::from_utf8(bytes[at + 46..at + 46 + name_length].to_vec()).unwrap();
        // the local header says the same
        assert_eq!(u32_at(bytes, offset), 0x0403_4b50);
        assert_eq!(u16_at(bytes, offset + 6), flags);
        assert_eq!(u16_at(bytes, offset + 8), method);
        assert_eq!(u32_at(bytes, offset + 14), crc);
        assert_eq!(u32_at(bytes, offset + 18) as usize, compressed);
        assert_eq!(u32_at(bytes, offset + 22) as usize, length);
        assert_eq!(u16_at(bytes, offset + 26) as usize, name_length);
        let data_at = offset + 30 + name_length + u16_at(bytes, offset + 28) as usize;
        let raw = &bytes[data_at..data_at + compressed];
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut data = Vec::new();
                DeflateDecoder::new(raw).read_to_end(&mut data).unwrap();
                data
            }
            other => panic!("method {other}"),
        };
        assert_eq!(data.len(), length, "{name}");
        assert_eq!(crc32fast::hash(&data), crc, "{name}");
        out.push(Unzipped { name, method, data });
        at += 46 + name_length;
    }
    out
}

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/history/fixtures/rust-archive.prose")
}

#[test]
fn an_archive_is_the_folder_as_a_zip_written_entry_by_entry() {
    let tmp = TempDir::new("archive");
    let dir = tmp.path().join("searches");
    folder(&dir, "1-0");
    let target = tmp.path().join("out/Fixture.prose");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let mut reports = Vec::new();
    write_archive(
        &dir,
        "1-0",
        &target,
        fixture_time(),
        &mut |written, total| reports.push((written, total)),
    )
    .unwrap();
    let bytes = fs::read(&target).unwrap();
    // nothing else is left beside it
    assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 1);

    let entries = unzip(&bytes);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // the record first, the others by name; without the database copy, its path and the log
    assert_eq!(
        names,
        [
            RECORD,
            "empty.txt",
            INPUTS,
            "mods/1__a_PTMStats.tsv",
            "proteins.fasta",
            "qpx/quantms.psm.parquet",
            "qpx/runs/1__a.mz.parquet"
        ]
    );
    let get = |name: &str| entries.iter().find(|e| e.name == name).unwrap();
    // the files as they are; Parquet stored, the rest deflated
    for e in &entries {
        if e.name != INPUTS {
            assert_eq!(
                e.data,
                fs::read(dir.join("1-0").join(&e.name)).unwrap(),
                "{}",
                e.name
            );
        }
        assert_eq!(
            e.method,
            if e.name.ends_with(".parquet") { 0 } else { 8 },
            "{}",
            e.name
        );
    }
    assert!(bytes.len() < 20_000 + 2_500, "proteins.fasta is deflated");
    // the searched files by name only
    let inputs: serde_json::Value = serde_json::from_slice(&get(INPUTS).data).unwrap();
    assert_eq!(inputs[0]["path"], "a.mzML");
    assert_eq!(inputs[0]["id"], "f1");
    assert_eq!(inputs[1]["path"], "b.mzML");
    // the progress: up to the total of the files, never back
    let total = reports[0].1;
    assert!(reports
        .windows(2)
        .all(|w| w[0].0 <= w[1].0 && w[1].1 == total));
    assert_eq!(reports.last().unwrap().0, total);

    // the front end's round trip reads this zip (history.test.ts): it is the fixture
    if std::env::var_os("PROSE_WRITE_FIXTURES").is_some() {
        fs::write(fixture_path(), &bytes).unwrap();
    }
    assert!(
        fs::read(fixture_path()).unwrap() == bytes,
        "the writer changed: PROSE_WRITE_FIXTURES=1 cargo test --test archive writes {} again",
        fixture_path().display()
    );
}

#[test]
fn an_archive_is_refused_before_anything_is_written() {
    let tmp = TempDir::new("archive-refused");
    let dir = tmp.path().join("searches");
    // a folder without record: not saved, so no archive
    fs::create_dir_all(dir.join("2-0/qpx")).unwrap();
    fs::write(dir.join("2-0/qpx/x.parquet"), "x").unwrap();
    let message = archive_files(&dir, "2-0").unwrap_err();
    assert!(message.contains("not saved to History"), "{message}");
    let target = tmp.path().join("x.prose");
    assert!(write_archive(&dir, "2-0", &target, fixture_time(), &mut |_, _| {}).is_err());
    assert!(!target.exists());
    // no such search, or not a search
    assert!(archive_files(&dir, "9-9").is_err());
    assert!(archive_files(&dir, "../searches").is_err());
}

#[test]
fn an_archive_that_cannot_be_made_leaves_nothing_and_keeps_what_was_there() {
    let tmp = TempDir::new("archive-failed");
    let dir = tmp.path().join("searches");
    folder(&dir, "1-0");
    fs::write(dir.join("1-0").join(INPUTS), "{ not a list").unwrap();
    let target = tmp.path().join("old.prose");
    fs::write(&target, "the old archive").unwrap();
    let message = write_archive(&dir, "1-0", &target, fixture_time(), &mut |_, _| {}).unwrap_err();
    assert!(message.contains("inputs.json cannot be read"), "{message}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "the old archive");
    assert_eq!(
        fs::read_dir(tmp.path()).unwrap().count(),
        2,
        "no .part file is left"
    );
    // a folder that cannot be written to
    folder(&dir, "3-0");
    let message = write_archive(
        &dir,
        "3-0",
        &tmp.path().join("absent/x.prose"),
        fixture_time(),
        &mut |_, _| {},
    )
    .unwrap_err();
    assert!(message.contains("cannot be written"), "{message}");
}

#[test]
fn inputs_json_names_the_files_without_their_paths() {
    let reduced = without_paths(
        br#"[{"name":"a.mzML","path":"/x/a.mzML","size":3},{"path":"/y/b.mzML.gz"},{"path":"D:\\z\\c.mzML"},{}]"#,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&reduced).unwrap();
    assert_eq!(value[0]["path"], "a.mzML");
    assert_eq!(value[0]["size"], 3);
    assert_eq!(value[1]["path"], "b.mzML.gz");
    assert_eq!(value[2]["path"], "c.mzML");
    assert_eq!(value[3]["path"], "");
    assert!(without_paths(b"{ no")
        .unwrap_err()
        .contains("cannot be read"));
    assert!(without_paths(b"{}").unwrap_err().contains("not a list"));
    assert!(without_paths(b"[1]").unwrap_err().contains("not a list"));
}

#[test]
fn the_entries_have_the_time_of_the_archive_in_ms_dos_format() {
    // 2026-10-24 12:34:56
    let (time, date) = dos_time(fixture_time());
    assert_eq!(time, (12 << 11) | (34 << 5) | 28);
    assert_eq!(date, ((2026 - 1980) << 9) | (10 << 5) | 24);
    // a leap day, and the start of the format
    let (_, date) = dos_time(UNIX_EPOCH + Duration::from_secs(951_782_400));
    assert_eq!(date, ((2000 - 1980) << 9) | (2 << 5) | 29);
    assert_eq!(dos_time(UNIX_EPOCH), (0, (1 << 5) | 1));
}

#[test]
fn an_empty_zip_is_a_zip() {
    let zip = ZipWriter::new(Cursor::new(Vec::new()), fixture_time());
    let bytes = zip.finish().unwrap().into_inner();
    assert_eq!(bytes.len(), 22);
    assert!(unzip(&bytes).is_empty());
}

//------------------------------------------------------------------------------------------------
// Opening an archive

/// The files of a folder, by their names relative to it.
fn files_of(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((name, fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// A zip of `entries` (name, data, stored), plain or zip64.
fn zip_of(entries: &[(&str, &[u8], bool)], zip64: bool) -> Vec<u8> {
    let mut zip = ZipWriter::with_format(Cursor::new(Vec::new()), fixture_time(), zip64);
    for (name, data, store) in entries {
        zip.add(name, &mut &data[..], *store, &mut |_| {}).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn directory_of(bytes: &[u8]) -> Result<Vec<ZipEntry>, String> {
    read_directory(&mut Cursor::new(bytes), bytes.len() as u64)
}

fn unpacked(bytes: &[u8], entry: &ZipEntry) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    extract(
        &mut Cursor::new(bytes),
        bytes.len() as u64,
        entry,
        &mut out,
        &mut |_| {},
    )?;
    Ok(out)
}

const SMALL_RECORD: &str = r#"{"schema":1,"id":"7-0","name":"Small"}"#;

#[test]
fn an_archive_opens_as_the_search_it_was_made_of() {
    let tmp = TempDir::new("archive-import");
    let dir = tmp.path().join("searches");
    folder(&dir, "1-0");
    let target = tmp.path().join("Fixture.prose");
    write_archive(&dir, "1-0", &target, fixture_time(), &mut |_, _| {}).unwrap();

    let other = tmp.path().join("elsewhere");
    let mut reports = Vec::new();
    let id = import_archive(&other, &target, &mut |written, total| {
        reports.push((written, total))
    })
    .unwrap();
    assert_ne!(id, "1-0");
    // the folder has what the archive holds: the files as they were, but the record (its id, the import
    // mark) and inputs.json (names only), and without what an archive leaves out
    let got = files_of(&other.join(&id));
    let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "empty.txt",
            INPUTS,
            "mods/1__a_PTMStats.tsv",
            "proteins.fasta",
            "qpx/quantms.psm.parquet",
            "qpx/runs/1__a.mz.parquet",
            RECORD
        ]
    );
    for (name, data) in &got {
        if name != RECORD && name != INPUTS {
            assert_eq!(
                data,
                &fs::read(dir.join("1-0").join(name)).unwrap(),
                "{name}"
            );
        }
    }
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(other.join(&id).join(RECORD)).unwrap()).unwrap();
    assert_eq!(record["id"], id.as_str());
    assert_eq!(record["imported"], true);
    assert_eq!(record["name"], "Fixture");
    // History lists it, and nothing else is left in the searches folder (no partial folder)
    let entries = list(&other);
    assert_eq!(entries.len(), 1);
    assert_eq!(fs::read_dir(&other).unwrap().count(), 1);
    // the progress: up to the total of the files, never back
    let total = reports[0].1;
    assert!(total > 0);
    assert!(reports
        .windows(2)
        .all(|w| w[0].0 <= w[1].0 && w[1].1 == total));
    assert!(reports.last().unwrap().0 <= total);
}

#[test]
fn a_zip64_archive_is_written_and_read() {
    let big: Vec<u8> = (0..300_000u32)
        .flat_map(|i| (i % 251).to_le_bytes())
        .collect();
    let entries: [(&str, &[u8], bool); 4] = [
        (RECORD, SMALL_RECORD.as_bytes(), false),
        ("qpx/quantms.psm.parquet", &big, true),
        ("proteins.fasta", &big, false),
        ("empty.txt", b"", false),
    ];
    let plain = zip_of(&entries, false);
    let zip64 = zip_of(&entries, true);
    // the plain zip is the one earlier versions read; the zip64 one has the zip64 end record and its locator
    // before the end record, whose counts and offsets are all ones
    assert!(unzip(&plain).len() == 4);
    let eocd = zip64.len() - 22;
    assert_eq!(u32_at(&zip64, eocd), 0x0605_4b50);
    assert_eq!(u16_at(&zip64, eocd + 10), 0xffff);
    assert_eq!(u32_at(&zip64, eocd + 16), 0xffff_ffff);
    assert_eq!(u32_at(&zip64, eocd - 20), 0x0706_4b50);
    assert_eq!(u32_at(&zip64, eocd - 20 - 56), 0x0606_4b50);
    for bytes in [&plain, &zip64] {
        let directory = directory_of(bytes).unwrap();
        assert_eq!(
            directory
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>(),
            entries.iter().map(|e| e.0).collect::<Vec<_>>()
        );
        for (entry, (_, data, store)) in directory.iter().zip(&entries) {
            assert_eq!(entry.size, data.len() as u64);
            assert_eq!(entry.method, if *store { 0 } else { 8 });
            assert_eq!(&unpacked(bytes, entry).unwrap(), data, "{}", entry.name);
        }
    }
    // and it opens as a search
    let tmp = TempDir::new("archive-zip64");
    let file = tmp.path().join("big.prose");
    fs::write(&file, &zip64).unwrap();
    let dir = tmp.path().join("searches");
    let id = import_archive(&dir, &file, &mut |_, _| {}).unwrap();
    assert_eq!(fs::read(dir.join(&id).join("proteins.fasta")).unwrap(), big);
    assert_eq!(
        fs::read(dir.join(&id).join("qpx/quantms.psm.parquet")).unwrap(),
        big
    );
}

/// The entries of sizes beyond 4 GiB: the zip64 fields are 64 bits wide where they are written. (A sparse
/// source and a sink that only counts: nothing of that size is held or written.)
#[test]
fn a_zip64_entry_holds_more_than_four_gib() {
    struct Zeros(u64);
    impl Read for Zeros {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = (buf.len() as u64).min(self.0) as usize;
            buf[..n].fill(0);
            self.0 -= n as u64;
            Ok(n)
        }
    }
    /// A seekable sink that keeps only the last bytes written (the directory and the end records).
    struct Tail {
        at: u64,
        end: u64,
        kept: Vec<(u64, Vec<u8>)>,
    }
    impl std::io::Write for Tail {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            // (the entry's data is zeros: only what is not all zeros is kept)
            if buf.iter().any(|&b| b != 0) || buf.len() < 4096 {
                self.kept.push((self.at, buf.to_vec()));
            }
            self.at += buf.len() as u64;
            self.end = self.end.max(self.at);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl std::io::Seek for Tail {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            self.at = match pos {
                std::io::SeekFrom::Start(n) => n,
                std::io::SeekFrom::Current(n) => (self.at as i64 + n) as u64,
                std::io::SeekFrom::End(n) => (self.end as i64 + n) as u64,
            };
            Ok(self.at)
        }
    }
    let size = (4u64 << 30) + 123;
    let sink = Tail {
        at: 0,
        end: 0,
        kept: Vec::new(),
    };
    let mut zip = ZipWriter::with_format(sink, fixture_time(), true);
    zip.add("qpx/a.parquet", &mut Zeros(size), true, &mut |_| {})
        .unwrap();
    zip.add("after.txt", &mut &b"after"[..], true, &mut |_| {})
        .unwrap();
    let sink = zip.finish().unwrap();
    // what was written, as a sparse file: read back through the reader
    struct Sparse {
        at: u64,
        end: u64,
        kept: Vec<(u64, Vec<u8>)>,
    }
    impl Read for Sparse {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = (buf.len() as u64).min(self.end - self.at.min(self.end)) as usize;
            buf[..n].fill(0);
            // later writes win (the sizes patched into a header)
            for (start, data) in &self.kept {
                let (from, to) = (
                    self.at.max(*start),
                    (self.at + n as u64).min(start + data.len() as u64),
                );
                if from < to {
                    buf[(from - self.at) as usize..(to - self.at) as usize]
                        .copy_from_slice(&data[(from - start) as usize..(to - start) as usize]);
                }
            }
            self.at += n as u64;
            Ok(n)
        }
    }
    impl std::io::Seek for Sparse {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            self.at = match pos {
                std::io::SeekFrom::Start(n) => n,
                std::io::SeekFrom::Current(n) => (self.at as i64 + n) as u64,
                std::io::SeekFrom::End(n) => (self.end as i64 + n) as u64,
            };
            Ok(self.at)
        }
    }
    let mut file = Sparse {
        at: 0,
        end: sink.end,
        kept: sink.kept,
    };
    assert!(file.end > size);
    let end = file.end;
    let directory = read_directory(&mut file, end).unwrap();
    assert_eq!(directory.len(), 2);
    assert_eq!(directory[0].size, size);
    assert_eq!(directory[0].compressed, size);
    assert_eq!(directory[0].offset, 0);
    assert!(directory[1].offset > u64::from(u32::MAX));
    let mut out = Vec::new();
    extract(&mut file, end, &directory[1], &mut out, &mut |_| {}).unwrap();
    assert_eq!(out, b"after");
}

#[test]
fn a_file_that_is_not_an_archive_is_refused_and_leaves_nothing() {
    let tmp = TempDir::new("archive-refused-import");
    let dir = tmp.path().join("searches");
    let refused = |name: &str, bytes: &[u8]| -> String {
        let file = tmp.path().join(name);
        fs::write(&file, bytes).unwrap();
        let message = import_archive(&dir, &file, &mut |_, _| {}).unwrap_err();
        // no folder, partial or finished
        assert!(
            !dir.exists() || fs::read_dir(&dir).unwrap().count() == 0,
            "{name} left something"
        );
        message
    };
    let record = SMALL_RECORD.as_bytes();
    assert!(refused(
        "text.prose",
        b"hello, this is text and not a zip file at all"
    )
    .contains("not a zip file"));
    assert!(refused("short.prose", b"PK").contains("not a zip file"));
    let good = zip_of(
        &[(RECORD, record, false), ("qpx/a.parquet", b"PAR1", true)],
        false,
    );
    // cut short: no end record, or a directory that does not lie in the file
    assert!(refused("cut.prose", &good[..good.len() - 30]).contains("zip directory cannot be read"));
    // no record, a record that is none, a newer one
    assert!(
        refused("none.prose", &zip_of(&[("a.txt", b"x", false)], false)).contains("no search.json")
    );
    assert!(
        refused("json.prose", &zip_of(&[(RECORD, b"{ no", false)], false))
            .contains("not valid JSON")
    );
    assert!(
        refused("schema.prose", &zip_of(&[(RECORD, b"{}", false)], false)).contains("no schema")
    );
    assert!(refused(
        "newer.prose",
        &zip_of(&[(RECORD, br#"{"schema":99,"id":"1-0"}"#, false)], false)
    )
    .contains("newer version"));
    // names that are not plain names inside the folder, or twice, or a file that is also a folder
    for name in ["../x", "/abs/x", "a/../../x", "C:/x", "a\\b", ".lock"] {
        let message = refused(
            "name.prose",
            &zip_of(&[(RECORD, record, false), (name, b"x", false)], false),
        );
        assert!(
            message.contains("not a plain file name"),
            "{name}: {message}"
        );
    }
    assert!(refused(
        "twice.prose",
        &zip_of(
            &[
                (RECORD, record, false),
                ("a", b"x", false),
                ("a", b"y", false)
            ],
            false
        )
    )
    .contains("twice"));
    assert!(refused(
        "clash.prose",
        &zip_of(
            &[
                (RECORD, record, false),
                ("a", b"x", false),
                ("a/b", b"y", false)
            ],
            false
        )
    )
    .contains("a file and a folder"));
}

#[test]
fn a_damaged_or_dishonest_archive_is_refused_as_it_is_unpacked() {
    let tmp = TempDir::new("archive-damaged");
    let dir = tmp.path().join("searches");
    let data: Vec<u8> = b"PEPTIDEK".repeat(4000);
    let good = zip_of(
        &[
            (RECORD, SMALL_RECORD.as_bytes(), false),
            ("proteins.fasta", &data, false),
            ("qpx/a.parquet", &data, true),
        ],
        false,
    );
    let directory = directory_of(&good).unwrap();
    let central = |bytes: &[u8], name: &str| -> usize {
        // the central record of `name`
        let needle = name.as_bytes();
        (0..bytes.len() - needle.len())
            .rev()
            .find(|&at| &bytes[at..at + needle.len()] == needle)
            .unwrap()
            - 46
    };
    let refused = |name: &str, bytes: &[u8]| -> String {
        let file = tmp.path().join(name);
        fs::write(&file, bytes).unwrap();
        let message = import_archive(&dir, &file, &mut |_, _| {}).unwrap_err();
        assert!(
            !dir.exists() || fs::read_dir(&dir).unwrap().count() == 0,
            "{name} left something"
        );
        message
    };
    // a flipped byte of the stored data: another checksum
    let mut flipped = good.clone();
    let at = directory[2].offset as usize + 30 + "qpx/a.parquet".len() + 100;
    flipped[at] ^= 0xff;
    assert!(refused("crc.prose", &flipped).contains("another checksum"));
    // a deflated entry that unpacks to more than the directory says (a zip bomb's lie): nothing beyond is written
    let mut smaller = good.clone();
    let record = central(&smaller, "proteins.fasta");
    smaller[record + 24..record + 28].copy_from_slice(&100u32.to_le_bytes());
    assert!(refused("bomb.prose", &smaller).contains("larger than the zip directory says"));
    // ... or to less
    let mut larger = good.clone();
    larger[record + 24..record + 28].copy_from_slice(&(data.len() as u32 + 1).to_le_bytes());
    assert!(refused("short.prose", &larger).contains("shorter than the zip directory says"));
    // an encrypted entry, a symbolic link, another compression
    let mut encrypted = good.clone();
    encrypted[record + 8] |= 1;
    assert!(refused("encrypted.prose", &encrypted).contains("encrypted"));
    let mut link = good.clone();
    link[record + 5] = 3; // made on Unix
    link[record + 38..record + 42].copy_from_slice(&(0o120777u32 << 16).to_le_bytes());
    assert!(refused("link.prose", &link).contains("contains a link"));
    let mut method = good.clone();
    method[record + 10] = 12; // bzip2
    assert!(refused("method.prose", &method).contains("not supported"));
    // an entry beyond the file
    let mut beyond = good.clone();
    beyond[record + 42..record + 46].copy_from_slice(&(good.len() as u32).to_le_bytes());
    assert!(refused("beyond.prose", &beyond).contains("does not lie in the file"));
    // and the good one opens
    let file = tmp.path().join("good.prose");
    fs::write(&file, &good).unwrap();
    import_archive(&dir, &file, &mut |_, _| {}).unwrap();
}

#[test]
fn a_zip_of_another_tool_opens() {
    // made on Unix with folders listed, a data descriptor's flag and a comment: read by the directory
    let record = SMALL_RECORD.as_bytes();
    let mut bytes = zip_of(
        &[
            (RECORD, record, false),
            ("qpx/", b"", true),
            ("qpx/a.parquet", b"PAR1", true),
        ],
        false,
    );
    let comment = b"made elsewhere";
    let eocd = bytes.len() - 22;
    bytes[eocd + 20..eocd + 22].copy_from_slice(&(comment.len() as u16).to_le_bytes());
    bytes.extend_from_slice(comment);
    let directory = directory_of(&bytes).unwrap();
    assert_eq!(
        directory
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        [RECORD, "qpx/a.parquet"]
    );
    let tmp = TempDir::new("archive-other");
    let file = tmp.path().join("other.prose");
    fs::write(&file, &bytes).unwrap();
    let dir = tmp.path().join("searches");
    let id = import_archive(&dir, &file, &mut |_, _| {}).unwrap();
    assert_eq!(
        fs::read(dir.join(&id).join("qpx/a.parquet")).unwrap(),
        b"PAR1"
    );
}

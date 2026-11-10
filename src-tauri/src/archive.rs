//! Results archives (`.prose`): a search folder as one zip, written entry by entry from the folder's files
//! straight to the file the user chose (`save_archive_file`), so that nothing of it is held in memory and
//! the save dialog comes first. What an archive holds is what `src/lib/history/history.ts` describes: the
//! folder's regular files ([`history::folder_files`]) but [`LEFT_OUT`], `search.json` first, and
//! `inputs.json` with the searched files by name only ([`without_paths`]).
//!
//! The zip of a search of at most [`PLAIN_BYTES`] and [`PLAIN_ENTRIES`] files is a plain one, which every
//! version of the app opens (and the browser build, with fflate: `unpackArchive`): no zip64, names in UTF-8
//! (flag bit 11), each entry stored (Parquet, gzip and zstd files are compressed already) or deflated at the
//! quickest level, its sizes and CRC in the local header (written after the data: the file is seekable), no
//! data descriptors, no extra fields. A larger search is written as zip64 (every entry with the zip64 extra
//! field, the zip64 end record and its locator), which this version and later ones open.
//!
//! An archive is opened here too ([`read_directory`], [`extract`]): its central directory is read (with
//! zip64), and each entry is unpacked from the file straight to the search folder, its size and CRC checked,
//! so that nothing of it is held in memory either (`history::import_archive`).

use crate::files::resolve_search_file;
use crate::history::{folder_files, FolderFile};
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use std::fs;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The record, the first entry of an archive.
pub const RECORD: &str = "search.json";
/// The searched files, by name only in an archive.
pub const INPUTS: &str = "inputs.json";
/// Files of the folder that an archive leaves out: the database copy (`proteins.fasta` has the sequences),
/// the path of a database searched without contaminants, and the tool's log (its first line is the full
/// command line, with paths of this computer).
pub const LEFT_OUT: [&str; 4] = [
    "database.fasta",
    "database.json",
    "database-source.json",
    "prose.log",
];
/// The largest search (the sum of its files) written as a plain zip; a larger one is written as zip64. (A
/// plain zip holds 4 GiB; earlier versions of the app open at most this much.)
pub const PLAIN_BYTES: u64 = 3 * 1024 * 1024 * 1024;
/// The most entries of a plain zip.
pub const PLAIN_ENTRIES: usize = 0xffff;
/// The most files of an archive (what `history::import_archive` opens).
pub const MAX_ENTRIES: usize = 100_000;
/// The largest offset or size of a zip without zip64.
const MAX_ZIP: u64 = u32::MAX as u64;

/// Why a search makes no archive: it has more files than an archive opens with.
pub const TOO_MANY: &str =
    "This search has too many files for a results archive: a results archive holds at most \
     100,000 files. The search stays in History on this computer.";
/// A plain zip whose entries outgrew it: the search's files grew while they were read.
const OUTGREW: &str =
    "The search's files changed while the archive was written. Save the archive again.";

/// Whether a file is stored as it is: compressed already.
fn stored(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".parquet", ".gz", ".zst"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// The files of search `id` that its archive holds, in the order of the zip: `search.json` first, the
/// others by name. Refused (with the front end's messages) if the search has no record or too many files.
pub fn archive_files(dir: &Path, id: &str) -> Result<Vec<FolderFile>, String> {
    let listed = folder_files(dir, id)?;
    if !listed.iter().any(|f| f.name == RECORD) {
        return Err(format!(
            "The search is not saved to History (it has no {RECORD}), so there is no archive to make."
        ));
    }
    let mut kept: Vec<FolderFile> = listed
        .into_iter()
        .filter(|f| !LEFT_OUT.contains(&f.name.as_str()))
        .collect();
    if kept.len() > MAX_ENTRIES {
        return Err(TOO_MANY.to_string());
    }
    kept.sort_by(|a, b| (a.name != RECORD, &a.name).cmp(&(b.name != RECORD, &b.name)));
    Ok(kept)
}

/// `inputs.json` as an archive holds it: each searched file's `path` replaced by its name (its `name`, else
/// the last part of its path), not where it was on this computer.
pub fn without_paths(inputs: &[u8]) -> Result<Vec<u8>, String> {
    let not_a_list =
        || format!("{INPUTS} is not a list of the searched files, so the archive cannot be made.");
    let value: serde_json::Value = serde_json::from_slice(inputs)
        .map_err(|_| format!("{INPUTS} cannot be read, so the archive cannot be made."))?;
    let serde_json::Value::Array(records) = value else {
        return Err(not_a_list());
    };
    let mut reduced = Vec::with_capacity(records.len());
    for record in records {
        let serde_json::Value::Object(mut map) = record else {
            return Err(not_a_list());
        };
        let name = match map.get("name").and_then(|n| n.as_str()) {
            Some(name) => name.to_string(),
            None => map
                .get("path")
                .and_then(|p| p.as_str())
                .and_then(|p| p.rsplit(['/', '\\']).next())
                .unwrap_or("")
                .to_string(),
        };
        map.insert("path".to_string(), serde_json::Value::String(name));
        reduced.push(serde_json::Value::Object(map));
    }
    serde_json::to_vec_pretty(&reduced).map_err(|e| e.to_string())
}

/// The MS-DOS time and date of `time` (UTC; 1980 to 2107), as a zip has them.
pub fn dos_time(time: SystemTime) -> (u16, u16) {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // the civil date of a day count (Howard Hinnant's days_from_civil, inverted)
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    if year < 1980 {
        return (0, (1 << 5) | 1);
    }
    let year = year.min(2107);
    let (h, m, s) = (rest / 3600, rest % 3600 / 60, rest % 60);
    let time = ((h << 11) | (m << 5) | (s / 2)) as u16;
    let date = (((year - 1980) << 9) | (month << 5) | day) as u16;
    (time, date)
}

/// An entry written: what its central directory record needs.
struct Entry {
    name: String,
    method: u16,
    crc: u32,
    compressed: u64,
    size: u64,
    offset: u64,
}

/// A writer that counts what goes through it.
struct Counting<'a, W: Write> {
    inner: &'a mut W,
    count: u64,
}

impl<W: Write> Write for Counting<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.count += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// A zip written entry by entry to a seekable file.
pub struct ZipWriter<W: Write + Seek> {
    out: W,
    entries: Vec<Entry>,
    time: u16,
    date: u16,
    zip64: bool,
}

fn outgrew() -> io::Error {
    io::Error::other(OUTGREW)
}

fn u32_of(n: u64) -> io::Result<u32> {
    if n >= MAX_ZIP {
        return Err(outgrew());
    }
    Ok(n as u32)
}

/// The version a reader needs: 2.0, or 4.5 for zip64.
fn version_needed(zip64: bool) -> u16 {
    if zip64 {
        45
    } else {
        20
    }
}

impl<W: Write + Seek> ZipWriter<W> {
    /// A plain zip (no zip64) whose entries have the time `time`.
    pub fn new(out: W, time: SystemTime) -> ZipWriter<W> {
        ZipWriter::with_format(out, time, false)
    }

    /// A zip whose entries have the time `time`: plain, or (`zip64`) with the zip64 extra field of every entry
    /// and the zip64 end record, for more than a plain zip holds.
    pub fn with_format(out: W, time: SystemTime, zip64: bool) -> ZipWriter<W> {
        let (time, date) = dos_time(time);
        ZipWriter {
            out,
            entries: Vec::new(),
            time,
            date,
            zip64,
        }
    }

    /// Adds the entry `name` with the bytes of `source`, deflated unless `store`. `read` is called with the
    /// bytes read from `source` so far.
    pub fn add(
        &mut self,
        name: &str,
        source: &mut dyn Read,
        store: bool,
        read: &mut dyn FnMut(u64),
    ) -> io::Result<()> {
        let zip64 = self.zip64;
        if name.len() > u16::MAX as usize {
            return Err(io::Error::other("a name is too long for a zip"));
        }
        if !zip64 && self.entries.len() >= PLAIN_ENTRIES {
            return Err(outgrew());
        }
        let offset = self.out.stream_position()?;
        if !zip64 {
            u32_of(offset)?;
        }
        let method: u16 = if store { 0 } else { 8 };
        // the local header, its CRC and sizes written once the data is (zip64: the sizes in its extra field)
        let mut header = Vec::with_capacity(30 + name.len() + 20);
        header.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        header.extend_from_slice(&version_needed(zip64).to_le_bytes());
        header.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 name
        header.extend_from_slice(&method.to_le_bytes());
        header.extend_from_slice(&self.time.to_le_bytes());
        header.extend_from_slice(&self.date.to_le_bytes());
        header.extend_from_slice(&[0; 4]); // CRC
        header.extend_from_slice(&if zip64 { [0xff; 8] } else { [0; 8] }); // compressed size, size
        header.extend_from_slice(&(name.len() as u16).to_le_bytes());
        header.extend_from_slice(&(if zip64 { 20u16 } else { 0 }).to_le_bytes()); // the extra field
        header.extend_from_slice(name.as_bytes());
        if zip64 {
            header.extend_from_slice(&0x0001u16.to_le_bytes());
            header.extend_from_slice(&16u16.to_le_bytes());
            header.extend_from_slice(&[0; 16]); // size, compressed size
        }
        self.out.write_all(&header)?;

        let mut crc = crc32fast::Hasher::new();
        let mut size = 0u64;
        let mut buf = vec![0u8; 1 << 20];
        let compressed = {
            let mut counting = Counting {
                inner: &mut self.out,
                count: 0,
            };
            if store {
                loop {
                    let n = source.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    crc.update(&buf[..n]);
                    size += n as u64;
                    counting.write_all(&buf[..n])?;
                    read(size);
                }
                counting.count
            } else {
                let mut encoder = DeflateEncoder::new(&mut counting, Compression::new(1));
                loop {
                    let n = source.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    crc.update(&buf[..n]);
                    size += n as u64;
                    encoder.write_all(&buf[..n])?;
                    read(size);
                }
                encoder.finish()?;
                counting.count
            }
        };
        let end = self.out.stream_position()?;
        let crc = crc.finalize();
        self.out.seek(SeekFrom::Start(offset + 14))?;
        self.out.write_all(&crc.to_le_bytes())?;
        if zip64 {
            self.out
                .seek(SeekFrom::Start(offset + 30 + name.len() as u64 + 4))?;
            self.out.write_all(&size.to_le_bytes())?;
            self.out.write_all(&compressed.to_le_bytes())?;
        } else {
            u32_of(end)?;
            self.out.write_all(&u32_of(compressed)?.to_le_bytes())?;
            self.out.write_all(&u32_of(size)?.to_le_bytes())?;
        }
        self.out.seek(SeekFrom::Start(end))?;
        self.entries.push(Entry {
            name: name.to_string(),
            method,
            crc,
            compressed,
            size,
            offset,
        });
        Ok(())
    }

    /// Writes the central directory and its end record (zip64: its zip64 end record and locator before it);
    /// returns the writer.
    pub fn finish(mut self) -> io::Result<W> {
        let zip64 = self.zip64;
        let start = self.out.stream_position()?;
        for e in &self.entries {
            let mut record = Vec::with_capacity(46 + e.name.len() + 28);
            record.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            // made by: 2.0 (zip64: 4.5), MS-DOS (no Unix modes, no links)
            record.extend_from_slice(&version_needed(zip64).to_le_bytes());
            record.extend_from_slice(&version_needed(zip64).to_le_bytes());
            record.extend_from_slice(&0x0800u16.to_le_bytes());
            record.extend_from_slice(&e.method.to_le_bytes());
            record.extend_from_slice(&self.time.to_le_bytes());
            record.extend_from_slice(&self.date.to_le_bytes());
            record.extend_from_slice(&e.crc.to_le_bytes());
            if zip64 {
                record.extend_from_slice(&[0xff; 8]); // compressed size, size: in the extra field
            } else {
                record.extend_from_slice(&u32_of(e.compressed)?.to_le_bytes());
                record.extend_from_slice(&u32_of(e.size)?.to_le_bytes());
            }
            record.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
            record.extend_from_slice(&(if zip64 { 28u16 } else { 0 }).to_le_bytes()); // the extra field
            record.extend_from_slice(&[0; 6]); // comment, disk, internal attributes
            record.extend_from_slice(&0u32.to_le_bytes()); // external attributes
            if zip64 {
                record.extend_from_slice(&[0xff; 4]); // the offset: in the extra field
            } else {
                record.extend_from_slice(&u32_of(e.offset)?.to_le_bytes());
            }
            record.extend_from_slice(e.name.as_bytes());
            if zip64 {
                record.extend_from_slice(&0x0001u16.to_le_bytes());
                record.extend_from_slice(&24u16.to_le_bytes());
                record.extend_from_slice(&e.size.to_le_bytes());
                record.extend_from_slice(&e.compressed.to_le_bytes());
                record.extend_from_slice(&e.offset.to_le_bytes());
            }
            self.out.write_all(&record)?;
        }
        let end = self.out.stream_position()?;
        let size = end - start;
        let count = self.entries.len() as u64;
        if zip64 {
            let mut record = Vec::with_capacity(56 + 20);
            // the zip64 end record
            record.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
            record.extend_from_slice(&44u64.to_le_bytes()); // its size from here on
            record.extend_from_slice(&45u16.to_le_bytes()); // made by
            record.extend_from_slice(&45u16.to_le_bytes()); // version needed
            record.extend_from_slice(&[0; 8]); // this disk, the directory's disk
            record.extend_from_slice(&count.to_le_bytes());
            record.extend_from_slice(&count.to_le_bytes());
            record.extend_from_slice(&size.to_le_bytes());
            record.extend_from_slice(&start.to_le_bytes());
            // its locator
            record.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
            record.extend_from_slice(&0u32.to_le_bytes()); // the disk of the zip64 end record
            record.extend_from_slice(&end.to_le_bytes());
            record.extend_from_slice(&1u32.to_le_bytes()); // one disk
            self.out.write_all(&record)?;
        } else {
            u32_of(end)?;
        }
        let mut record = Vec::with_capacity(22);
        record.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        record.extend_from_slice(&[0; 4]); // this disk, the directory's disk
        if zip64 {
            record.extend_from_slice(&[0xff; 12]); // the counts, the size, the offset: in the zip64 end record
        } else {
            record.extend_from_slice(&(count as u16).to_le_bytes());
            record.extend_from_slice(&(count as u16).to_le_bytes());
            record.extend_from_slice(&u32_of(size)?.to_le_bytes());
            record.extend_from_slice(&u32_of(start)?.to_le_bytes());
        }
        record.extend_from_slice(&0u16.to_le_bytes()); // no comment
        self.out.write_all(&record)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

/// `<target>.part`: where the archive is written before it takes the name the user chose.
fn part_of(target: &Path) -> PathBuf {
    let mut part = target.as_os_str().to_owned();
    part.push(".part");
    PathBuf::from(part)
}

/// Writes the archive of search `id` to `target` (the path the user chose), entry by entry, the entries'
/// time `time`; `progress` is told the bytes of the folder's files read so far, and their total. The zip is
/// written as `<target>.part` and renamed when complete, so that a failure leaves nothing (and an existing
/// file at `target` as it was).
pub fn write_archive(
    dir: &Path,
    id: &str,
    target: &Path,
    time: SystemTime,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let files = archive_files(dir, id)?;
    let total: u64 = files.iter().map(|f| f.size).sum();
    let part = part_of(target);
    let shown = target.display();
    let written = (|| -> Result<(), String> {
        let file =
            fs::File::create(&part).map_err(|e| format!("{shown} cannot be written ({e})."))?;
        let zip64 = total > PLAIN_BYTES || files.len() > PLAIN_ENTRIES;
        let mut zip = ZipWriter::with_format(BufWriter::with_capacity(1 << 20, file), time, zip64);
        let mut done = 0u64;
        for f in &files {
            let path = resolve_search_file(dir, id, &f.name).map_err(|e| e.message)?;
            let fail = |e: io::Error| {
                if e.to_string() == OUTGREW {
                    OUTGREW.to_string()
                } else {
                    format!("{} cannot be added to the archive ({e}).", f.name)
                }
            };
            let mut read = |n: u64| progress(done + n, total);
            if f.name == INPUTS {
                let bytes = fs::read(&path).map_err(fail)?;
                let reduced = without_paths(&bytes)?;
                zip.add(&f.name, &mut reduced.as_slice(), false, &mut |_| {})
                    .map_err(fail)?;
            } else {
                let mut source = fs::File::open(&path).map_err(fail)?;
                zip.add(&f.name, &mut source, stored(&f.name), &mut read)
                    .map_err(fail)?;
            }
            done += f.size;
            progress(done.min(total), total);
        }
        let out = zip.finish().map_err(|e| {
            if e.to_string() == OUTGREW {
                OUTGREW.to_string()
            } else {
                format!("{shown} cannot be written ({e}).")
            }
        })?;
        let file = out
            .into_inner()
            .map_err(|e| format!("{shown} cannot be written ({}).", e.error()))?;
        file.sync_all()
            .map_err(|e| format!("{shown} cannot be written ({e})."))?;
        drop(file);
        fs::rename(&part, target).map_err(|e| format!("{shown} cannot be written ({e})."))
    })();
    if written.is_err() {
        let _ = fs::remove_file(&part);
    }
    written
}

//------------------------------------------------------------------------------------------------
// Opening an archive

/// An entry of an archive's central directory.
#[derive(Debug, Clone, PartialEq)]
pub struct ZipEntry {
    pub name: String,
    /// 0: stored, 8: deflated.
    pub method: u16,
    pub crc: u32,
    pub compressed: u64,
    pub size: u64,
    /// Where its local header is.
    pub offset: u64,
}

const NOT_A_ZIP: &str = "This file is not a results archive: it is not a zip file.";
const NO_DIRECTORY: &str = "This file is not a results archive: its zip directory cannot be read.";
const DAMAGED: &str = "This results archive is damaged";
/// The most a central directory takes: [`MAX_ENTRIES`] records with names and extra fields.
const MAX_DIRECTORY: u64 = 256 << 20;

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn le64(b: &[u8], at: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(bytes)
}

/// `length` bytes at `offset` of `source`.
fn read_at<R: Read + Seek>(source: &mut R, offset: u64, length: usize) -> io::Result<Vec<u8>> {
    let mut bytes = vec![0u8; length];
    source.seek(SeekFrom::Start(offset))?;
    source.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// The files of the zip `source` (of `length` bytes), as its central directory lists them, in its order; the
/// folders it may list (names ending in `/`) are left out. Read with zip64. Refused, with why: a file that is
/// not a zip, a zip of several disks, more than [`MAX_ENTRIES`] entries, an encrypted entry, a symbolic link,
/// a compression other than stored and deflate, and an entry that does not lie in the file. The names are as
/// the zip has them (UTF-8 if the entry says so, else Latin-1): whether they are names of a search folder is
/// the caller's check.
pub fn read_directory<R: Read + Seek>(
    source: &mut R,
    length: u64,
) -> Result<Vec<ZipEntry>, String> {
    let broken = || NO_DIRECTORY.to_string();
    if length < 22 {
        return Err(NOT_A_ZIP.to_string());
    }
    let magic = read_at(source, 0, 2).map_err(|_| NOT_A_ZIP.to_string())?;
    if magic != b"PK" {
        return Err(NOT_A_ZIP.to_string());
    }
    // the end record: the last one in the file's last 64 KiB (a comment may follow it)
    let tail_length = length.min(22 + 0xffff);
    let tail_at = length - tail_length;
    let tail = read_at(source, tail_at, tail_length as usize).map_err(|_| broken())?;
    let eocd = (0..=tail.len() - 22)
        .rev()
        .find(|&at| le32(&tail, at) == 0x0605_4b50)
        .ok_or_else(broken)?;
    let mut count = u64::from(le16(&tail, eocd + 10));
    let mut size = u64::from(le32(&tail, eocd + 12));
    let mut start = u64::from(le32(&tail, eocd + 16));
    let mut disks_ok = le16(&tail, eocd + 4) == 0 && le16(&tail, eocd + 6) == 0;
    // zip64: the locator before the end record names the zip64 end record
    let located = eocd >= 20 && le32(&tail, eocd - 20) == 0x0706_4b50;
    if located {
        let locator = &tail[eocd - 20..eocd];
        let at = le64(locator, 8);
        if at.checked_add(56).is_none_or(|end| end > length) {
            return Err(broken());
        }
        let record = read_at(source, at, 56).map_err(|_| broken())?;
        if le32(&record, 0) != 0x0606_4b50 {
            return Err(broken());
        }
        disks_ok = le32(locator, 4) == 0 && le32(&record, 16) == 0 && le32(&record, 20) == 0;
        count = le64(&record, 32);
        size = le64(&record, 40);
        start = le64(&record, 48);
    } else if count == 0xffff || size == 0xffff_ffff || start == 0xffff_ffff {
        return Err(broken());
    }
    if !disks_ok {
        return Err(
            "This results archive is a zip of several parts, which is not supported.".to_string(),
        );
    }
    if count > MAX_ENTRIES as u64 {
        return Err("This results archive has too many files.".to_string());
    }
    if size > MAX_DIRECTORY || start.checked_add(size).is_none_or(|end| end > length) {
        return Err(broken());
    }
    let directory = read_at(source, start, size as usize).map_err(|_| broken())?;
    let mut entries = Vec::with_capacity(count as usize);
    let mut at = 0usize;
    for _ in 0..count {
        if at + 46 > directory.len() || le32(&directory, at) != 0x0201_4b50 {
            return Err(broken());
        }
        let made_by = le16(&directory, at + 4);
        let flags = le16(&directory, at + 8);
        let method = le16(&directory, at + 10);
        let crc = le32(&directory, at + 16);
        let mut compressed = u64::from(le32(&directory, at + 20));
        let mut size = u64::from(le32(&directory, at + 24));
        let name_length = le16(&directory, at + 28) as usize;
        let extra_length = le16(&directory, at + 30) as usize;
        let comment_length = le16(&directory, at + 32) as usize;
        let attributes = le32(&directory, at + 38);
        let mut offset = u64::from(le32(&directory, at + 42));
        let name_at = at + 46;
        let extra_at = name_at + name_length;
        let next = extra_at + extra_length + comment_length;
        if next > directory.len() {
            return Err(broken());
        }
        let raw = &directory[name_at..extra_at];
        let name = if flags & 0x800 != 0 {
            String::from_utf8(raw.to_vec()).map_err(|_| broken())?
        } else {
            raw.iter().map(|&b| b as char).collect()
        };
        // the zip64 extra field: the values that are all ones in the record, in this order
        let extra = &directory[extra_at..extra_at + extra_length];
        let mut e = 0usize;
        while e + 4 <= extra.len() {
            let (id, len) = (le16(extra, e), le16(extra, e + 2) as usize);
            let field = extra.get(e + 4..e + 4 + len).ok_or_else(broken)?;
            if id == 0x0001 {
                let mut f = 0usize;
                for value in [&mut size, &mut compressed, &mut offset] {
                    if *value == 0xffff_ffff {
                        if f + 8 > field.len() {
                            return Err(broken());
                        }
                        *value = le64(field, f);
                        f += 8;
                    }
                }
            }
            e += 4 + len;
        }
        at = next;
        if flags & 1 != 0 {
            return Err("This results archive is encrypted.".to_string());
        }
        // made on Unix (host 3) with the mode of a symbolic link in the upper half of the attributes
        if made_by >> 8 == 3 && (attributes >> 16) & 0xf000 == 0xa000 {
            return Err(
                "This results archive contains a link, which an archive never has. It is not opened."
                    .to_string(),
            );
        }
        if name.ends_with('/') {
            continue; // a folder
        }
        if method != 0 && method != 8 {
            return Err(format!(
                "{name} in the results archive is compressed in a way that is not supported (method {method})."
            ));
        }
        if offset
            .checked_add(30)
            .and_then(|n| n.checked_add(compressed))
            .is_none_or(|end| end > length)
            || (method == 0 && compressed != size)
        {
            return Err(format!("{DAMAGED}: {name} does not lie in the file."));
        }
        entries.push(ZipEntry {
            name,
            method,
            crc,
            compressed,
            size,
            offset,
        });
    }
    Ok(entries)
}

/// Unpacks `entry` of the zip `source` (of `length` bytes) to `out`, and checks it as it goes: the local
/// header is this entry's, the data unpacks to the size the directory declares, never more (nothing beyond
/// it is written), and has its CRC. `written` is called with the bytes written so far.
pub fn extract<R: Read + Seek>(
    source: &mut R,
    length: u64,
    entry: &ZipEntry,
    out: &mut dyn Write,
    written: &mut dyn FnMut(u64),
) -> Result<(), String> {
    let name = &entry.name;
    let damaged = |what: &str| format!("{DAMAGED}: {name} {what}.");
    let unreadable = |e: io::Error| format!("{name} of the results archive cannot be read ({e}).");
    let header = read_at(source, entry.offset, 30).map_err(unreadable)?;
    if le32(&header, 0) != 0x0403_4b50 {
        return Err(damaged("has no header"));
    }
    if le16(&header, 6) & 1 != 0 {
        return Err("This results archive is encrypted.".to_string());
    }
    if le16(&header, 8) != entry.method {
        return Err(damaged("is not as the zip directory describes it"));
    }
    let name_length = le16(&header, 26) as u64;
    let extra_length = le16(&header, 28) as u64;
    let data_at = entry.offset + 30 + name_length + extra_length;
    if data_at
        .checked_add(entry.compressed)
        .is_none_or(|end| end > length)
    {
        return Err(damaged("does not lie in the file"));
    }
    source.seek(SeekFrom::Start(data_at)).map_err(unreadable)?;
    let mut data = source.take(entry.compressed);
    let mut inflater;
    let reader: &mut dyn Read = if entry.method == 8 {
        inflater = DeflateDecoder::new(&mut data);
        &mut inflater
    } else {
        &mut data
    };
    let mut crc = crc32fast::Hasher::new();
    let mut total = 0u64;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|_| damaged("cannot be unpacked"))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > entry.size {
            return Err(damaged("is larger than the zip directory says"));
        }
        crc.update(&buf[..n]);
        out.write_all(&buf[..n])
            .map_err(|e| format!("{name} cannot be written ({e})."))?;
        written(total);
    }
    if total != entry.size {
        return Err(damaged("is shorter than the zip directory says"));
    }
    if crc.finalize() != entry.crc {
        return Err(damaged("has another checksum than the zip directory says"));
    }
    Ok(())
}

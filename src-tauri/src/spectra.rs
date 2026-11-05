//! What an mzML says about its spectra before a search (guidance G1, the interim inspector): the facts of its
//! header (a peak-picking step among its data processing, the analyzer of each instrument configuration) and of
//! its first MS2 spectra, read in one bounded pass (the MS level, the centroid or profile flag, the precursor's
//! charge, the activation, the isolation window, the filter string). Flags only: the peaks are not looked at,
//! so a spectrum flagged centroid is reported as "flagged", not verified (a mislabelled file passes; ProSE's own
//! inspector, `-inspect_out`, is to look at the peaks). `.mzML` and `.mzML.gz` only; the results are cached by
//! path, size and modification time, so that a file is read once per session.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

/// The MS2 spectra looked at: ProSE itself reads the first 8 MiB for activation terms; 1,000 spectra cover two or
/// more cycles of a DIA method with up to 300 windows (Astral narrow-window DIA), and many duty cycles of DDA.
pub const MAX_MS2: u64 = 1000;
/// The most bytes read (decompressed): a profile MS1 spectrum takes up to a MiB, so the first MS2 can sit several
/// MiB into a file (the code review found 1 MiB too small).
pub const MAX_BYTES: u64 = 64 << 20;
/// An isolation target read this often counts as repeated.
pub const REPEATED_TARGET: u64 = 3;

/// What the inspector found in an mzML.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpectraFacts {
    /// A data processing step names peak picking (MS:1000035): the file was centroided by a converter.
    pub peak_picking: bool,
    /// The analyzers of the header's instrument configurations (PSI-MS names, e.g. "orbitrap").
    pub analyzers: Vec<String>,
    /// Spectra read, by MS level (1, 2, 3, ...).
    pub levels: BTreeMap<u32, u64>,
    /// The pass reached the end of the spectrum list (the counts are the whole file's).
    pub complete: bool,
    /// Of the MS2 spectra read: flagged centroid (MS:1000127), flagged profile (MS:1000128).
    pub ms2_centroid: u64,
    pub ms2_profile: u64,
    /// Of the MS2 spectra read: with a precursor charge (MS:1000041), with a precursor m/z.
    pub ms2_with_charge: u64,
    pub ms2_with_precursor: u64,
    /// The activation terms of the MS2 spectra read, with their counts (e.g. "beam-type collision-induced
    /// dissociation").
    pub activation: BTreeMap<String, u64>,
    /// Distinct isolation-window targets (m/z, to 0.01) among the MS2 spectra read, and the widest window (m/z).
    pub isolation_centres: u64,
    pub isolation_width: Option<f64>,
    /// Of the MS2 spectra read: those whose isolation target occurs at least [`REPEATED_TARGET`] times (DIA cycles
    /// through the same windows; DDA rarely picks the very same target m/z again).
    pub ms2_repeated_target: u64,
    /// The MS2 analyzer from the filter strings ("ion trap" for ITMS, "orbitrap" for FTMS, "astral" for ASTMS),
    /// counted; empty without filter strings.
    pub ms2_analyzer: BTreeMap<String, u64>,
    /// The first MS2 filter string, as an example.
    pub filter: Option<String>,
}

/// The value of attribute `key` of the tag text `tag`.
fn attr<'a>(tag: &'a str, key: &str) -> Option<&'a str> {
    let at = tag.find(&format!("{key}=\""))? + key.len() + 2;
    Some(&tag[at..at + tag[at..].find('"')?])
}

/// The cvParams (accession, name, value) of `text`.
fn params(text: &str) -> impl Iterator<Item = (&str, &str, &str)> {
    text.split("<cvParam").skip(1).filter_map(|t| {
        let t = &t[..t.find('>').unwrap_or(t.len())];
        Some((
            attr(t, "accession")?,
            attr(t, "name")?,
            attr(t, "value").unwrap_or(""),
        ))
    })
}

/// The header facts: peak picking among the data processing, the analyzer of each instrument configuration.
fn header(head: &str, facts: &mut SpectraFacts) {
    if let Some(at) = head.find("<dataProcessingList") {
        let end = head[at..]
            .find("</dataProcessingList")
            .map_or(head.len(), |e| at + e);
        facts.peak_picking = params(&head[at..end]).any(|(acc, _, _)| acc == "MS:1000035");
    }
    let mut rest = head;
    while let Some(at) = rest.find("<analyzer") {
        let block = &rest[at..];
        let end = block.find("</analyzer").unwrap_or(block.len());
        // the analyzer's type: the term that is no property of it (accuracy, resolution, ...)
        const PROPERTIES: [&str; 6] = [
            "MS:1000014",
            "MS:1000022",
            "MS:1000024",
            "MS:1000025",
            "MS:1000011",
            "MS:1000105",
        ];
        if let Some((_, name, _)) =
            params(&block[..end]).find(|(acc, _, _)| !PROPERTIES.contains(acc))
        {
            facts.analyzers.push(name.to_string());
        }
        rest = &block[end..];
    }
}

/// The MS2 analyzer a Thermo filter string names.
fn filter_analyzer(filter: &str) -> Option<&'static str> {
    let first = filter.split_whitespace().next()?;
    match first {
        "ITMS" => Some("ion trap"),
        "FTMS" => Some("orbitrap"),
        "ASTMS" => Some("astral"),
        "TOFMS" => Some("time-of-flight"),
        _ => None,
    }
}

/// Adds one `<spectrum>` (its text without the binary data) to the facts.
fn spectrum(text: &str, facts: &mut SpectraFacts, centres: &mut HashMap<i64, u64>) {
    let mut level = 0u32;
    let (mut centroid, mut profile, mut charge, mut precursor) = (false, false, false, false);
    let mut target: Option<f64> = None;
    let (mut lower, mut upper) = (0.0, 0.0);
    let activation_at = text.find("<activation");
    for (acc, name, value) in params(text) {
        match acc {
            "MS:1000511" => level = value.parse().unwrap_or(0),
            "MS:1000127" => centroid = true,
            "MS:1000128" => profile = true,
            "MS:1000041" => charge = true,
            "MS:1000744" => precursor = true,
            "MS:1000827" => target = value.parse().ok(),
            "MS:1000828" => lower = value.parse().unwrap_or(0.0),
            "MS:1000829" => upper = value.parse().unwrap_or(0.0),
            _ => {}
        }
        let _ = name;
    }
    *facts.levels.entry(level).or_default() += 1;
    if level != 2 {
        return;
    }
    facts.ms2_centroid += u64::from(centroid);
    facts.ms2_profile += u64::from(profile);
    facts.ms2_with_charge += u64::from(charge);
    facts.ms2_with_precursor += u64::from(precursor || target.is_some());
    if let Some(t) = target {
        *centres.entry((t * 100.0).round() as i64).or_default() += 1;
        let width = lower + upper;
        if width > 0.0 && facts.isolation_width.is_none_or(|w| width > w) {
            facts.isolation_width = Some(width);
        }
    }
    if let Some(at) = activation_at {
        let block = &text[at..text[at..]
            .find("</activation")
            .map_or(text.len(), |e| at + e)];
        // the dissociation method: the terms that are not an energy
        for (acc, name, _) in params(block) {
            if !matches!(
                acc,
                "MS:1000045" | "MS:1000509" | "MS:1002013" | "MS:1000138" | "MS:1002680"
            ) {
                *facts.activation.entry(name.to_string()).or_default() += 1;
            }
        }
    }
    if let Some(filter) = text.find("name=\"filter string\"").and_then(|at| {
        let tag = &text[at..];
        attr(&tag[..tag.find('>').unwrap_or(tag.len())], "value")
    }) {
        if facts.filter.is_none() {
            facts.filter = Some(filter.to_string());
        }
        if let Some(a) = filter_analyzer(filter) {
            *facts.ms2_analyzer.entry(a.to_string()).or_default() += 1;
        }
    }
}

/// The first `needle` in `hay` at or after `from`.
fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|at| at + from)
}

/// Where a look-up that found nothing goes on once more bytes are there: a marker may have begun in the last
/// bytes, so these are searched again (the markers are shorter than 16 bytes).
fn resume(len: usize, from: usize) -> usize {
    len.saturating_sub(16).max(from)
}

/// The facts of an mzML read from `reader` (decompressed), at most [`MAX_BYTES`] and [`MAX_MS2`] MS2 spectra.
///
/// The markup is found in the bytes (the markers are ASCII), and only the header and each spectrum's own markup
/// are made text: a byte that is no UTF-8 (a Latin-1 sample name) cannot shift a position, and every byte is
/// searched about once, also through a spectrum of many megabytes.
pub fn inspect_reader(reader: impl Read) -> io::Result<SpectraFacts> {
    let mut facts = SpectraFacts::default();
    let mut centres = HashMap::new();
    let mut reader = reader.take(MAX_BYTES);
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 1 << 20];
    let mut header_done = false;
    // where the next look-up in `buf` starts, and the start of a spectrum whose end is not there yet
    let mut from = 0;
    let mut open: Option<usize> = None;
    loop {
        let n = reader.read(&mut chunk)?;
        buf.extend_from_slice(&chunk[..n]);
        if !header_done {
            match find(&buf, b"<spectrumList", from) {
                Some(at) => {
                    header(&String::from_utf8_lossy(&buf[..at]), &mut facts);
                    header_done = true;
                    from = at;
                }
                None if n == 0 => {
                    header(&String::from_utf8_lossy(&buf), &mut facts);
                    header_done = true;
                    from = buf.len();
                }
                None => from = resume(buf.len(), from),
            }
        }
        if header_done {
            loop {
                let Some(start) = open else {
                    match find(&buf, b"<spectrum ", from) {
                        Some(at) => {
                            open = Some(at);
                            from = at;
                            continue;
                        }
                        None => {
                            if find(&buf, b"</spectrumList", from).is_some() {
                                facts.complete = true;
                            }
                            from = resume(buf.len(), from);
                            break;
                        }
                    }
                };
                let Some(end) = find(&buf, b"</spectrum>", from) else {
                    from = resume(buf.len(), from.max(start));
                    break;
                };
                let body = &buf[start..end];
                let body = &body[..find(body, b"<binaryDataArrayList", 0).unwrap_or(body.len())];
                spectrum(&String::from_utf8_lossy(body), &mut facts, &mut centres);
                open = None;
                from = end + b"</spectrum>".len();
                if facts.levels.get(&2).copied().unwrap_or(0) >= MAX_MS2 {
                    count_centres(&mut facts, &centres);
                    return Ok(facts);
                }
            }
        }
        // what is done goes: all before an open spectrum (its start stays), else all before the next look-up
        if header_done {
            let done = open.unwrap_or(from).min(buf.len());
            buf.drain(..done);
            from -= done.min(from);
            open = open.map(|start| start - done);
        }
        if n == 0 {
            break;
        }
    }
    count_centres(&mut facts, &centres);
    Ok(facts)
}

/// The distinct isolation targets, and the MS2 spectra of the repeated ones.
fn count_centres(facts: &mut SpectraFacts, centres: &HashMap<i64, u64>) {
    facts.isolation_centres = centres.len() as u64;
    facts.ms2_repeated_target = centres.values().filter(|&&n| n >= REPEATED_TARGET).sum();
}

/// The facts of the mzML at `path` (`.mzML` or `.mzML.gz`); None for other files.
pub fn inspect(path: &Path) -> io::Result<Option<SpectraFacts>> {
    let name = path.to_string_lossy().to_lowercase();
    let file = fs::File::open(path)?;
    if name.ends_with(".mzml.gz") {
        inspect_reader(flate2::read::MultiGzDecoder::new(file)).map(Some)
    } else if name.ends_with(".mzml") {
        inspect_reader(io::BufReader::new(file)).map(Some)
    } else {
        Ok(None)
    }
}

type Key = (String, u64, Option<SystemTime>);
static CACHE: Mutex<Option<HashMap<Key, Option<SpectraFacts>>>> = Mutex::new(None);

/// [`inspect`] of `path`, cached by path, size and modification time; None for a file that is not an mzML or
/// cannot be read (New search has said so already).
pub fn inspect_cached(path: &str) -> Option<SpectraFacts> {
    let meta = fs::metadata(path).ok()?;
    let key = (path.to_string(), meta.len(), meta.modified().ok());
    if let Some(hit) = CACHE
        .lock()
        .ok()?
        .get_or_insert_with(HashMap::new)
        .get(&key)
    {
        return hit.clone();
    }
    let facts = inspect(Path::new(path)).ok().flatten();
    if let Ok(mut cache) = CACHE.lock() {
        cache
            .get_or_insert_with(HashMap::new)
            .insert(key, facts.clone());
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/example/Ecoli_MS2_small.mzML")
    }

    #[test]
    fn the_example_is_centroid_cid_ion_trap_ms2() {
        let f = inspect(&example()).unwrap().unwrap();
        assert!(f.complete, "{f:?}");
        assert_eq!(f.levels.get(&2), Some(&139));
        assert_eq!(f.ms2_centroid, 139);
        assert_eq!(f.ms2_profile, 0);
        assert_eq!(f.ms2_with_charge, 139);
        assert_eq!(
            f.activation.get("collision-induced dissociation"),
            Some(&139)
        );
        assert_eq!(f.ms2_analyzer.get("ion trap"), Some(&139));
        assert_eq!(f.analyzers, vec!["orbitrap".to_string()]);
        assert!(f.isolation_centres > 100);
        assert_eq!(f.isolation_width, Some(2.0));
        assert!(f.filter.as_deref().unwrap().starts_with("ITMS"));
    }

    #[test]
    fn gzip_cache_and_limits() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("prose-spectra-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let gz = dir.join("a.mzML.gz");
        let mut enc = flate2::write::GzEncoder::new(
            fs::File::create(&gz).unwrap(),
            flate2::Compression::fast(),
        );
        enc.write_all(&fs::read(example()).unwrap()).unwrap();
        enc.finish().unwrap();
        let path = gz.to_string_lossy().into_owned();
        assert_eq!(inspect_cached(&path).unwrap().levels.get(&2), Some(&139));
        assert_eq!(inspect_cached(&path), inspect(&gz).unwrap());
        fs::write(dir.join("a.fasta"), ">x\nPEPTIDE\n").unwrap();
        assert_eq!(inspect(&dir.join("a.fasta")).unwrap(), None);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A reader that gives at most `step` bytes per read: every marker and character falls across reads somewhere.
    struct Trickle<'a> {
        data: &'a [u8],
        step: usize,
    }

    impl Read for Trickle<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let n = self.step.min(out.len()).min(self.data.len());
            out[..n].copy_from_slice(&self.data[..n]);
            self.data = &self.data[n..];
            Ok(n)
        }
    }

    #[test]
    fn bytes_that_are_no_utf8_and_markers_across_reads_change_nothing() {
        // a Latin-1 é (no UTF-8) and multi-byte characters in the header and the spectra, around the markers
        let spectrum = |i: usize| {
            format!(
                "<spectrum index=\"{i}\" id=\"µ-scan={i} ö\"><cvParam accession=\"MS:1000511\" name=\"ms level\" value=\"2\"/>\
                 <cvParam accession=\"MS:1000127\" name=\"centroid spectrum\"/>\
                 <cvParam accession=\"MS:1000041\" name=\"charge state\" value=\"2\"/>\
                 <binaryDataArrayList><binary>AAAA€€</binary></binaryDataArrayList></spectrum>ü"
            )
        };
        let mut text: Vec<u8> = b"<mzML><sample name=\"Caf".to_vec();
        text.push(0xE9);
        text.extend_from_slice("\"/>µµ<spectrumList count=\"40\">".as_bytes());
        for i in 0..40 {
            text.extend_from_slice(spectrum(i).as_bytes());
        }
        text.extend_from_slice("</spectrumList>ü</mzML>".as_bytes());
        let whole = inspect_reader(&text[..]).unwrap();
        assert_eq!(whole.levels.get(&2), Some(&40));
        assert_eq!(whole.ms2_centroid, 40);
        assert_eq!(whole.ms2_with_charge, 40);
        assert!(whole.complete);
        for step in [1, 2, 3, 7, 13, 64] {
            let read = inspect_reader(Trickle { data: &text, step }).unwrap();
            assert_eq!(read, whole, "{step} bytes per read");
        }
    }

    #[test]
    fn a_spectrum_of_many_megabytes_is_read_once() {
        // one MS1 spectrum of 40 MB of binary data before the MS2 spectrum: read in 1 MiB chunks, in about the time
        // of one pass (it was decoded again for every chunk)
        let mut text = b"<mzML><spectrumList count=\"2\"><spectrum index=\"0\"><cvParam accession=\"MS:1000511\" name=\"ms level\" value=\"1\"/><binaryDataArrayList><binary>".to_vec();
        text.extend(std::iter::repeat_n(b'A', 40 << 20));
        text.extend_from_slice(b"</binary></binaryDataArrayList></spectrum><spectrum index=\"1\"><cvParam accession=\"MS:1000511\" name=\"ms level\" value=\"2\"/></spectrum></spectrumList></mzML>");
        let started = std::time::Instant::now();
        let f = inspect_reader(&text[..]).unwrap();
        assert_eq!(f.levels.get(&1), Some(&1));
        assert_eq!(f.levels.get(&2), Some(&1));
        assert!(f.complete);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn profile_flags_dia_windows_and_peak_picking() {
        let spectrum = |i: usize, target: f64| {
            format!(
                "<spectrum index=\"{i}\"><cvParam accession=\"MS:1000511\" name=\"ms level\" value=\"2\"/>\
                 <cvParam accession=\"MS:1000128\" name=\"profile spectrum\"/>\
                 <isolationWindow><cvParam accession=\"MS:1000827\" name=\"isolation window target m/z\" value=\"{target}\"/>\
                 <cvParam accession=\"MS:1000828\" name=\"isolation window lower offset\" value=\"12.5\"/>\
                 <cvParam accession=\"MS:1000829\" name=\"isolation window upper offset\" value=\"12.5\"/></isolationWindow>\
                 <activation><cvParam accession=\"MS:1000422\" name=\"beam-type collision-induced dissociation\"/></activation>\
                 <binaryDataArrayList><binary>AAAA</binary></binaryDataArrayList></spectrum>"
            )
        };
        let body: String = (0..1200)
            .map(|i| spectrum(i, 400.0 + 25.0 * (i % 24) as f64))
            .collect();
        let text = format!(
            "<mzML><dataProcessingList><cvParam accession=\"MS:1000035\" name=\"peak picking\"/></dataProcessingList>\
             <spectrumList count=\"1200\">{body}</spectrumList></mzML>"
        );
        let f = inspect_reader(text.as_bytes()).unwrap();
        assert!(f.peak_picking);
        // stops at MAX_MS2
        assert_eq!(f.levels.get(&2), Some(&MAX_MS2));
        assert!(!f.complete);
        assert_eq!(f.ms2_profile, MAX_MS2);
        assert_eq!(f.ms2_with_charge, 0);
        assert_eq!(f.isolation_centres, 24);
        assert_eq!(f.ms2_repeated_target, MAX_MS2);
        assert_eq!(f.isolation_width, Some(25.0));
    }
}

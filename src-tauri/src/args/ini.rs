//! The search's parameters as the INI ProSE reads with `-ini` ([`ini_of`]), and the command line of the
//! search with its paths as placeholders ([`command_line`]).

use super::Kind::*;
use super::PARAMETERS;
use std::ffi::OsString;
use std::path::Path;

/// The INI file of a search in its folder: ProSE's `Search:` parameters of the request, passed with `-ini` instead
/// of on the command line (guidance G8), so that the file ProSE read is the record of its parameters. It holds
/// only what the app sets; ProSE takes its defaults for the rest (they come with its version, which the record has).
pub const INI_FILE: &str = "prose.ini";
/// The command line of a search in its folder, the paths as placeholders (`<input 1>`, `<database>`,
/// `<search folder>`), for "Copy command line".
pub const COMMAND_FILE: &str = "command.txt";

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The arguments of `args` (of [`build_args`](super::build_args)) up to its first `-Search:` one, and the `-Search:` ones from there
/// (build_args puts them last).
pub fn split_search_args(args: &[OsString]) -> (Vec<OsString>, Vec<OsString>) {
    let at = args
        .iter()
        .position(|a| a.to_string_lossy().starts_with("-Search:"))
        .unwrap_or(args.len());
    (args[..at].to_vec(), args[at..].to_vec())
}

/// A tree of the INI's nodes: parameters by path.
#[derive(Default)]
struct IniNode {
    items: Vec<String>,
    children: Vec<(String, IniNode)>,
}

impl IniNode {
    fn at(&mut self, path: &[&str]) -> &mut IniNode {
        let Some((first, rest)) = path.split_first() else {
            return self;
        };
        let k = match self.children.iter().position(|(n, _)| n == first) {
            Some(k) => k,
            None => {
                self.children.push((first.to_string(), IniNode::default()));
                self.children.len() - 1
            }
        };
        self.children[k].1.at(rest)
    }

    fn write(&self, name: &str, depth: usize, out: &mut String) {
        let pad = "  ".repeat(depth);
        out.push_str(&format!(
            "{pad}<NODE name=\"{}\" description=\"\">\n",
            xml_escape(name)
        ));
        for item in &self.items {
            out.push_str(&format!("{pad}  {item}\n"));
        }
        for (child, node) in &self.children {
            node.write(child, depth + 1, out);
        }
        out.push_str(&format!("{pad}</NODE>\n"));
    }
}

/// The INI of the `-Search:` arguments of [`build_args`](super::build_args) (each with the type ProSE gives it): ProSE's TOPP INI
/// format, its parameters under `ProSE:1:Search:`.
pub fn ini_of(search_args: &[OsString]) -> Result<String, String> {
    let kind_of = |name: &str| match name {
        "rescore" => Some(Choice),
        "peptdeep:enable" => Some(Flag),
        _ => PARAMETERS.iter().find(|(n, _)| *n == name).map(|(_, k)| *k),
    };
    let mut root = IniNode::default();
    let mut i = 0;
    while i < search_args.len() {
        let arg = search_args[i].to_string_lossy().into_owned();
        let name = arg
            .strip_prefix("-Search:")
            .ok_or_else(|| format!("{arg} is not a search parameter."))?;
        let kind = kind_of(name)
            .ok_or_else(|| format!("{name} is not a search parameter the app sets."))?;
        i += 1;
        let mut values = Vec::new();
        match kind {
            Flag => {}
            List => {
                while i < search_args.len()
                    && !search_args[i].to_string_lossy().starts_with("-Search:")
                {
                    values.push(search_args[i].to_string_lossy().into_owned());
                    i += 1;
                }
            }
            _ => {
                let v = search_args
                    .get(i)
                    .ok_or_else(|| format!("{name} has no value."))?;
                values.push(v.to_string_lossy().into_owned());
                i += 1;
            }
        }
        let parts: Vec<&str> = name.split(':').collect();
        let (leaf, path) = parts.split_last().unwrap();
        let leaf = xml_escape(leaf);
        let item = match kind {
            List => {
                let items: String = values
                    .iter()
                    .map(|v| format!("<LISTITEM value=\"{}\"/>", xml_escape(v)))
                    .collect();
                format!("<ITEMLIST name=\"{leaf}\" type=\"string\" description=\"\" required=\"false\" advanced=\"false\">{items}</ITEMLIST>")
            }
            Flag => format!("<ITEM name=\"{leaf}\" value=\"true\" type=\"bool\" description=\"\" required=\"false\" advanced=\"false\" />"),
            _ => {
                let ty = match kind {
                    Int => "int",
                    Float => "double",
                    _ => "string",
                };
                format!(
                    "<ITEM name=\"{leaf}\" value=\"{}\" type=\"{ty}\" description=\"\" required=\"false\" advanced=\"false\" />",
                    xml_escape(&values[0])
                )
            }
        };
        root.at(path).items.push(item);
    }
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<PARAMETERS version=\"1.8.0\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n  <NODE name=\"ProSE\" description=\"\">\n    <NODE name=\"1\" description=\"\">\n",
    );
    let mut body = String::new();
    root.write("Search", 3, &mut body);
    out.push_str(&body);
    out.push_str("    </NODE>\n  </NODE>\n</PARAMETERS>\n");
    Ok(out)
}

/// The command line of `args` with the paths as placeholders: each input (`inputs`, the paths ProSE was given)
/// `<input n>`, the database `<database>`, the search folder `<search folder>`; quoted where it has a blank.
pub fn command_line(
    tool: &str,
    args: &[OsString],
    inputs: &[OsString],
    database: &OsString,
    folder: &Path,
) -> String {
    let folder = folder.to_string_lossy().into_owned();
    let mut words = vec![tool.to_string()];
    for a in args {
        let mut w = a.to_string_lossy().into_owned();
        if let Some(k) = inputs.iter().position(|p| p == a) {
            w = format!("<input {}>", k + 1);
        } else if a == database {
            w = "<database>".to_string();
        } else if let Some(rest) = w.strip_prefix(&folder) {
            w = format!("<search folder>{rest}");
        }
        if w.contains(' ') && !w.starts_with('<') {
            w = format!("\"{w}\"");
        }
        words.push(w);
    }
    words.join(" ")
}

#[cfg(test)]
mod ini_tests {
    use super::*;

    fn os(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_search_arguments_become_an_ini_by_their_types() {
        let args = os(&[
            "-in",
            "/f/1__a.mzML",
            "-database",
            "/db/h.fasta",
            "-threads",
            "0",
            "-Search:rescore",
            "true",
            "-Search:peptdeep:enable",
            "-Search:enzyme",
            "Trypsin/P",
            "-Search:precursor:mass_tolerance_lower",
            "7.5",
            "-Search:peptide:missed_cleavages",
            "2",
            "-Search:modifications:variable",
            "Oxidation (M)",
            "Acetyl (Protein N-term)",
            "-Search:modifications:fixed",
        ]);
        let (tool, search) = split_search_args(&args);
        assert_eq!(
            tool,
            os(&[
                "-in",
                "/f/1__a.mzML",
                "-database",
                "/db/h.fasta",
                "-threads",
                "0"
            ])
        );
        let ini = ini_of(&search).unwrap();
        for part in [
            "<NODE name=\"ProSE\"",
            "<NODE name=\"1\"",
            "<NODE name=\"Search\"",
            "<ITEM name=\"rescore\" value=\"true\" type=\"string\"",
            "<NODE name=\"peptdeep\" description=\"\">\n          <ITEM name=\"enable\" value=\"true\" type=\"bool\"",
            "<ITEM name=\"enzyme\" value=\"Trypsin/P\" type=\"string\"",
            "<ITEM name=\"mass_tolerance_lower\" value=\"7.5\" type=\"double\"",
            "<ITEM name=\"missed_cleavages\" value=\"2\" type=\"int\"",
            "<LISTITEM value=\"Oxidation (M)\"/><LISTITEM value=\"Acetyl (Protein N-term)\"/>",
            // an empty list clears the default, as on the command line
            "<ITEMLIST name=\"fixed\" type=\"string\" description=\"\" required=\"false\" advanced=\"false\"></ITEMLIST>",
        ] {
            assert!(ini.contains(part), "{part}\n{ini}");
        }
        // one node per name
        assert_eq!(ini.matches("<NODE name=\"modifications\"").count(), 1);
        assert!(ini_of(&os(&["-Search:not_a_parameter", "1"])).is_err());
        assert!(ini_of(&os(&["-Search:enzyme"])).is_err());
    }

    #[test]
    fn the_command_line_names_paths_by_placeholders() {
        let folder = Path::new("/data/searches/1-0.partial");
        let args = os(&[
            "-in",
            "/data/searches/1-0.partial/1__a b.mzML",
            "-database",
            "/db/h.fasta",
            "-out_qpx",
            "/data/searches/1-0.partial/qpx",
            "-ini",
            "/data/searches/1-0.partial/prose.ini",
        ]);
        let line = command_line("ProSE", &args, &args[1..2], &args[3], folder);
        assert_eq!(line, "ProSE -in <input 1> -database <database> -out_qpx <search folder>/qpx -ini <search folder>/prose.ini");
    }
}

//! The record `import module` leaves inside the module it copied in: which
//! collection it came from, what pinned that collection, and what the directory
//! hashed to at the time.
//!
//! It is a sibling of `module.kdl` and never part of it. `module.kdl` is the
//! author's file, and rewriting it on import would fork it from upstream and
//! break the very comparison the hash exists to make.

use crate::diag::{Issues, Source};
use crate::parse::schema::{check_doc, Arg, Node, Say, NEEDS_VALUE};
use crate::parse::{kids, string_arg, syntax_issue, text};
use crate::provenance::evidence::{self, Role, PIN};
use crate::provenance::{Evidence, Tracker};
use kdl::KdlDocument;
use std::path::Path;

/// The record's filename inside the module directory. Excluded from the hash it
/// carries, since a file cannot be inside its own hash.
pub const RECORD: &str = "provenance.kdl";

/// The record's grammar, and the whole of the file.
#[rustfmt::skip]
pub const IMPORTED: Node = Node::new("imported",
    "Where this module was copied from, and what its content hashed to then. Written by \
     `tect import module`; the module's author does not maintain it.")
    .arg(Arg::Str, Say::new("`imported` needs a collection name", "no collection given",
        "`imported \"tectonic-os\"`, the name the collection is declared under in `sources`"))
    .once("")
    .missing(Say::new("`{}` declares no `imported`", "nothing recorded",
        "the file records one import; delete it if the module was not imported"))
    .children(&[
        Node::new("content",
            "What the module directory hashed to when it was imported, every file in it except \
             this one.")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .missing(Say::new("`{}` records no `content` hash", "nothing to compare against",
                "the sha256 of the imported directory, which is what makes a later edit visible")),
        PIN,
    ], Say::new("unknown node `{}` in an import record", "not part of the schema",
        "an import record holds `content` and the `pin` the collection was fetched at"));

/// The file, whose one top-level node is the record.
const RECORD_FILE: Node = Node::new("", "").children(&[IMPORTED], Say::NONE);

/// One module's import record.
pub struct Record {
    pub collection: String,
    /// The directory hash as it stood at import.
    pub content: String,
    /// The collection's pin, as it stood at import.
    pub pin: Evidence,
}

/// The record beside a module, when there is one. A module nobody imported has
/// none, which is the ordinary case for a module the repository wrote itself.
pub fn read(dir: &Path, issues: &mut Issues) -> Option<Record> {
    let path = dir.join(RECORD);
    let raw = std::fs::read_to_string(&path).ok()?;
    let name = path.display().to_string();
    let src = Source::new(&name, raw.clone());
    let doc: KdlDocument = match raw.parse() {
        Ok(doc) => doc,
        Err(err) => {
            issues.push(syntax_issue(&err, &name, &src));
            return None;
        }
    };
    check_doc(&doc, &RECORD_FILE, &src, issues);

    let node = doc
        .nodes()
        .iter()
        .find(|n| n.name().value() == "imported")?;
    let pin = kids(node)
        .iter()
        .find(|n| n.name().value() == "pin")
        .map(|n| evidence::read(n, Role::Record, &src, issues))
        .unwrap_or_else(|| Evidence::new(node.name().span().into()));

    Some(Record {
        collection: string_arg(node).unwrap_or_default().to_string(),
        content: text(node, "content"),
        pin,
    })
}

/// The record as the file holds it. The tool writes whole files, so this is the
/// whole of one.
pub fn write(collection: &str, pin: Option<&Evidence>, content: &str) -> String {
    let mut body = format!("    content {}\n", quoted(content));
    let Some(pin) = pin else {
        return format!("imported {} {{\n{body}}}\n", quoted(collection));
    };
    body.push_str("    pin {\n");
    match &pin.tracker {
        Tracker::Renovate {
            datasource,
            dep_name,
        } => body.push_str(&format!(
            "        renovate datasource={} depName={}\n",
            quoted(datasource),
            quoted(dep_name)
        )),
        Tracker::Manual(why) => body.push_str(&format!("        manual {}\n", quoted(why))),
        Tracker::Unpinned(why) => body.push_str(&format!("        unpinned {}\n", quoted(why))),
        Tracker::None => {}
    }
    for (name, value) in [
        ("version", pin.version.as_deref()),
        ("url", pin.url.as_deref()),
        ("sha256", pin.sha256.as_deref()),
        ("path", pin.path.as_deref()),
    ] {
        if let Some(value) = value {
            body.push_str(&format!("        {name} {}\n", quoted(value)));
        }
    }
    body.push_str("    }\n");
    format!("imported {} {{\n{body}}}\n", quoted(collection))
}

/// A KDL string, which every value here is: the pins are already held to a
/// charset that excludes a quote or a backslash, and a reason is prose.
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// What a module directory hashes to: every file under it except the record
/// itself, which cannot be inside its own hash. Path and content both, so
/// renaming a file changes it.
pub fn hash(dir: &Path) -> Option<String> {
    let record = dir.join(RECORD);
    let mut listed: Vec<String> = Vec::new();
    for path in crate::tracked(dir) {
        if path == record {
            continue;
        }
        let rel = path.strip_prefix(dir).ok()?;
        listed.push(rel.to_string_lossy().into_owned());
    }
    if listed.is_empty() {
        return None;
    }
    crate::runtime::sha256_tree(dir, &listed).ok()
}

/// Every imported module whose content no longer matches what its record says.
/// Forking a module is legitimate, so this is a read-out rather than a
/// diagnostic; what it buys is that the fork is visible.
pub fn modified(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for dir in dirs(&root.join("modules")) {
        let Ok(raw) = std::fs::read_to_string(dir.join(RECORD)) else {
            continue;
        };
        let Some(recorded) = raw.parse::<KdlDocument>().ok().and_then(|doc| {
            let node = doc
                .nodes()
                .iter()
                .find(|n| n.name().value() == "imported")?;
            Some(text(node, "content"))
        }) else {
            continue;
        };
        if hash(&dir).is_some_and(|found| found != recorded) {
            let name = dir.strip_prefix(root).unwrap_or(&dir);
            out.push(name.to_string_lossy().into_owned());
        }
    }
    out.sort();
    out
}

/// Every module directory under `modules/`, at whatever depth an owner puts it.
fn dirs(from: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(from) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for path in entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        match path.join("module.kdl").is_file() {
            true => out.push(path),
            false => out.extend(dirs(&path)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::ShaFrom;

    /// The pin half of a record, which the golden corpus never writes: it
    /// imports from directories on this machine, and those have no pin.
    #[test]
    fn a_written_pin_reads_back_as_what_was_written() {
        let dir = std::env::temp_dir().join(format!("tect-record-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let written = Evidence {
            url: Some("https://host/owner/modules/{version}.tar.gz".into()),
            version: Some("v1.0.0".into()),
            sha256: Some("b7c2".into()),
            from: ShaFrom::Asset,
            path: Some("modules".into()),
            tracker: Tracker::Renovate {
                datasource: "github-tags".into(),
                dep_name: "owner/modules".into(),
            },
            span: Default::default(),
        };
        std::fs::write(
            dir.join(RECORD),
            write("tectonic-os", Some(&written), "abc"),
        )
        .unwrap();

        let mut issues = Issues::default();
        let read = read(&dir, &mut issues).expect("the record is there");
        assert!(issues.is_empty(), "{}", issues.plain());
        assert_eq!(read.collection, "tectonic-os");
        assert_eq!(read.content, "abc");
        assert_eq!(read.pin.url, written.url);
        assert_eq!(read.pin.version, written.version);
        assert_eq!(read.pin.sha256, written.sha256);
        assert_eq!(read.pin.path, written.path);
        assert_eq!(read.pin.tracker.as_str(), "renovate");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

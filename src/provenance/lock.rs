//! `import module` writes the lock at the root of `modules/`. For each imported
//! module, the lock records the library it came from, the library's revision,
//! whether upstream is tracked, and what the module directory hashed to.
//!
//! The lock is one tool-owned file. A file inside a module directory would sit
//! outside its own hash.

use crate::diag::{Issues, Source};
use crate::layout;
use crate::parse::schema::{check_doc, Arg, Node, Say, NEEDS_VALUE};
use crate::parse::{child, kids, string_arg, syntax_issue, text};
use crate::provenance::Evidence;
use kdl::KdlDocument;
use std::path::Path;

/// Declares one imported module, keyed by its path under `modules/`.
#[rustfmt::skip]
pub const LOCKED: Node = Node::new("module",
    "One imported module, by its path under `modules/`.").example("\"browser\"").minimal()
    .arg(Arg::Str, Say::new("`module` needs the path of the module", "no path given",
        "`module \"browser\"`, the directory under `modules/`"))
    .unique(Say::new("`{}` is locked twice", "a second entry would disagree",
        "the lock holds one entry for each module; run `tect import module` again to rewrite it"))
    .children(&[
        Node::new("library", "The library the module was imported from, as `sources` in `repo.kdl` names it.").example("\"tectonic-os\"")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .missing(Say::new("a lock entry records no `{}`", "origin unknown",
                "the name the library is declared under in `sources`")),
        Node::new("path", "The path of the module inside the library. It differs from the entry's own path when the module was imported under another name.").example("\"browser\"")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .missing(Say::new("a lock entry records no `{}`", "origin unknown",
                "the path of the module inside its library")),
        Node::new("url", "The Git URL of the library, for a library that `tect` fetched.").example("\"https://github.com/tectonic-os/library.git\"")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("component-version", "The revision of the library that the module was taken at, for a library that `tect` fetched.").example("\"v0.6.51\"")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("content", "The hash that the module directory had at import.").example("\"b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3\"")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .missing(Say::new("a lock entry records no `{}` hash", "nothing to compare against",
                "the sha256 of the imported directory, which is what makes a later edit visible")),
        Node::new("track", "Marks a copy that tracks its library. An untracked copy is the repository's own.")
            .arg(Arg::None, Say::new("`track` takes no value", "unexpected value",
                "write `track` alone, or leave it out for a module the repository owns"))
            .once(""),
    ], Say::new("unknown node `{}` in a lock entry", "not part of the schema",
        "a lock entry holds `library`, `path`, `url`, `component-version`, `content` and `track`"));

/// Declares the lock file. Its top-level nodes are the entries.
#[rustfmt::skip]
pub(crate) const LOCK_FILE: Node = Node::new("lock file",
    "The record of every imported module, which `tect import module` writes.")
    .about("When `tect import module` copies a module into this repository, it records the module \
        here. The entry keeps where the module came from and what its content hashed to, so \
        `tect check` can show whether the copy has changed since.")
    .children(&[LOCKED], Say::new("unknown node `{}` in the lock", "not part of the schema",
        "the lock holds one `module` entry for each imported module"))
    .notes(&[
        "`tect import module` writes the whole file. The user does not edit it, and `tect check` \
         reports an entry that does not read.",
        "The entry is keyed by the local path, so one library module imported twice under two \
         names has two entries.",
        "`plan.json` carries the same hash for each module, so `tect verify` fails on a module \
         edited without a new `tect generate`.",
        "If a module's content no longer matches its entry, then `tect check` names the module. \
         The mismatch is an error only under `audit { enforce #true }`.",
    ]);

/// Holds one module's lock entry.
pub struct Record {
    /// Keys the entry by the module's path under `modules/`.
    pub path: String,
    pub library: String,
    /// Names the module's path inside the library.
    pub upstream: String,
    /// Holds the library's locator and the revision taken. A library that is
    /// a directory on this machine has neither.
    pub pin: Evidence,
    /// Holds the directory hash as it stood at import.
    pub content: String,
    pub tracked: bool,
}

/// Reads every entry in the lock. A lock that does not parse reads as empty,
/// and `check` reports it through `validate`.
pub fn read(root: &Path) -> Vec<Record> {
    let Ok(raw) = std::fs::read_to_string(layout::lock(root)) else {
        return Vec::new();
    };
    let Ok(doc) = raw.parse::<KdlDocument>() else {
        return Vec::new();
    };
    doc.nodes()
        .iter()
        .filter(|node| node.name().value() == LOCKED.name)
        .filter_map(|node| {
            let mut pin = Evidence::new(node.name().span().into());
            pin.url = child(node, "url").and_then(string_arg).map(str::to_string);
            pin.version = child(node, "component-version")
                .and_then(string_arg)
                .map(str::to_string);
            Some(Record {
                path: string_arg(node)?.to_string(),
                library: text(node, "library"),
                upstream: text(node, "path"),
                pin,
                content: text(node, "content"),
                tracked: kids(node).iter().any(|n| n.name().value() == "track"),
            })
        })
        .collect()
}

/// Finds the entry for the module at `dir`, its path under `modules/`.
pub fn entry(root: &Path, dir: &str) -> Option<Record> {
    read(root).into_iter().find(|record| record.path == dir)
}

/// Holds the lock to its grammar. `check` calls this once for the repository,
/// because every module reads its own entry without a diagnostic.
pub fn validate(root: &Path, issues: &mut Issues) {
    let path = layout::lock(root);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    let name = path.display().to_string();
    let src = Source::new(&name, raw.clone());
    match raw.parse::<KdlDocument>() {
        Ok(doc) => check_doc(&doc, &LOCK_FILE, &src, issues),
        Err(err) => issues.push(syntax_issue(&err, &name, &src)),
    }
}

/// Renders the whole lock with `record` added, or replacing the entry at its
/// path. The tool writes whole files, so a lock that does not parse is refused
/// rather than rewritten from nothing, which would drop every other entry.
pub fn with(root: &Path, record: Record) -> Result<String, String> {
    let path = layout::lock(root);
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Err(err) = raw.parse::<KdlDocument>() {
            return Err(format!(
                "{} does not parse, so importing would drop its other entries: {err}; \
                 resolve it, then import again",
                path.display()
            ));
        }
    }
    let mut records: Vec<Record> = read(root)
        .into_iter()
        .filter(|held| held.path != record.path)
        .collect();
    records.push(record);
    records.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(records.iter().map(render).collect::<Vec<_>>().join("\n"))
}

fn render(record: &Record) -> String {
    let mut body = format!(
        "    library {}\n    path {}\n",
        quoted(&record.library),
        quoted(&record.upstream)
    );
    for (name, value) in [
        ("url", record.pin.url.as_deref()),
        ("component-version", record.pin.version.as_deref()),
    ] {
        if let Some(value) = value {
            body.push_str(&format!("    {name} {}\n", quoted(value)));
        }
    }
    body.push_str(&format!("    content {}\n", quoted(&record.content)));
    if record.tracked {
        body.push_str("    track\n");
    }
    format!("module {} {{\n{body}}}\n", quoted(&record.path))
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Hashes every file under a module directory by its path and its content, so
/// renaming a file changes the hash. A module that the directory holds of its
/// own has a hash of its own, so its files stay out of this one.
pub fn hash(dir: &Path) -> Option<String> {
    let held = layout::held_modules(dir);
    let mut listed: Vec<String> = Vec::new();
    for path in crate::tracked(dir) {
        let rel = path.strip_prefix(dir).ok()?;
        if held.iter().any(|module| rel.starts_with(module)) {
            continue;
        }
        listed.push(rel.to_string_lossy().into_owned());
    }
    if listed.is_empty() {
        return None;
    }
    crate::runtime::sha256_tree(dir, &listed).ok()
}

/// Names every imported module whose content no longer matches its entry.
/// Forking a module is legitimate, so this is a read-out that makes the fork
/// visible.
pub fn modified(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = read(root)
        .into_iter()
        .filter(|record| {
            hash(&layout::module(root, &record.path)).is_some_and(|found| found != record.content)
        })
        .map(|record| format!("{}/{}", layout::MODULES, record.path))
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(path: &str, tracked: bool) -> Record {
        let mut pin = Evidence::new(crate::diag::Span::default());
        pin.url = Some("https://host/owner/library.git".into());
        pin.version = Some("v1.0.0".into());
        Record {
            path: path.into(),
            library: "tectonic-os".into(),
            upstream: "browser".into(),
            pin,
            content: "abc".into(),
            tracked,
        }
    }

    #[test]
    fn a_written_entry_reads_back_as_what_was_written() {
        let root = std::env::temp_dir().join(format!("tect-lock-{}", std::process::id()));
        crate::init::put(
            &layout::lock(&root),
            &with(&root, record("browser@1", true)).unwrap(),
        )
        .unwrap();

        let mut issues = Issues::default();
        validate(&root, &mut issues);
        assert!(issues.is_empty(), "{}", issues.plain());
        let read = entry(&root, "browser@1").expect("the entry is there");
        assert_eq!(read.library, "tectonic-os");
        assert_eq!(read.upstream, "browser");
        assert_eq!(read.content, "abc");
        assert_eq!(
            read.pin.url.as_deref(),
            Some("https://host/owner/library.git")
        );
        assert_eq!(read.pin.version.as_deref(), Some("v1.0.0"));
        assert!(read.tracked);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_second_import_replaces_only_its_own_entry() {
        let root = std::env::temp_dir().join(format!("tect-lock-two-{}", std::process::id()));
        crate::init::put(
            &layout::lock(&root),
            &with(&root, record("browser", true)).unwrap(),
        )
        .unwrap();
        crate::init::put(
            &layout::lock(&root),
            &with(&root, record("editor", true)).unwrap(),
        )
        .unwrap();
        crate::init::put(
            &layout::lock(&root),
            &with(&root, record("browser", false)).unwrap(),
        )
        .unwrap();

        let held = read(&root);
        let paths: Vec<&str> = held.iter().map(|record| record.path.as_str()).collect();
        assert_eq!(paths, ["browser", "editor"]);
        assert!(!held[0].tracked);
        assert!(held[1].tracked);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_edited_module_is_named_and_an_unedited_one_is_not() {
        let root = std::env::temp_dir().join(format!("tect-lock-edit-{}", std::process::id()));
        let dir = layout::module(&root, "browser");
        crate::init::put(&dir.join(layout::MODULE_FILE), "description \"x\"\n").unwrap();
        let mut held = record("browser", true);
        held.content = hash(&dir).unwrap();
        crate::init::put(&layout::lock(&root), &with(&root, held).unwrap()).unwrap();
        assert!(modified(&root).is_empty());

        crate::init::put(&dir.join(layout::MODULE_FILE), "description \"y\"\n").unwrap();
        assert_eq!(modified(&root), ["modules/browser"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The overlay file is named `module.kdl` to prove that the overlay never
    /// reads as a held module.
    #[test]
    fn a_held_module_stays_out_of_the_hash_and_the_overlay_does_not() {
        let root = std::env::temp_dir().join(format!("tect-lock-held-{}", std::process::id()));
        let dir = layout::module(&root, "nvidia");
        crate::init::put(&dir.join(layout::MODULE_FILE), "description \"x\"\n").unwrap();
        let before = hash(&dir).unwrap();

        crate::init::put(&dir.join("cuda").join(layout::MODULE_FILE), "").unwrap();
        crate::init::put(&dir.join("cuda/module.sh"), "true\n").unwrap();
        assert_eq!(hash(&dir).unwrap(), before);

        let overlay = dir
            .join(layout::OVERLAY)
            .join("usr/share/x")
            .join(layout::MODULE_FILE);
        crate::init::put(&overlay, "").unwrap();
        assert_ne!(hash(&dir).unwrap(), before);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_lock_that_does_not_parse_is_refused_and_kept() {
        let root = std::env::temp_dir().join(format!("tect-lock-broken-{}", std::process::id()));
        let broken = "<<<<<<< HEAD\nmodule \"browser\" {\n";
        crate::init::put(&layout::lock(&root), broken).unwrap();

        let refused = with(&root, record("editor", true)).unwrap_err();
        assert!(refused.contains("does not parse"), "{refused}");
        let mut issues = Issues::default();
        validate(&root, &mut issues);
        assert!(!issues.is_empty());
        assert_eq!(
            std::fs::read_to_string(layout::lock(&root)).unwrap(),
            broken
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_entry_outside_the_grammar_is_reported() {
        let root = std::env::temp_dir().join(format!("tect-lock-grammar-{}", std::process::id()));
        crate::init::put(
            &layout::lock(&root),
            "module \"browser\" {\n    library \"one\"\n}\n",
        )
        .unwrap();
        let mut issues = Issues::default();
        validate(&root, &mut issues);
        let said = issues.plain();
        assert!(said.contains("records no `path`"), "{said}");
        std::fs::remove_dir_all(root).unwrap();
    }
}

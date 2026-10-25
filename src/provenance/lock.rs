//! `import component` writes the lock at the root of `components/`. For each imported
//! component, the lock records the library it came from, the library's revision,
//! whether upstream is tracked, and what the component directory hashed to.
//!
//! The lock is one tool-owned file. A file inside a component directory would sit
//! outside its own hash.

use crate::diag::{Issue, Issues, Source};
use crate::layout;
use crate::parse::schema::{check_doc, Arg, Node, Say, NEEDS_VALUE};
use crate::parse::{child, kids, string_arg, syntax_issue, text};
use crate::provenance::Evidence;
use kdl::KdlDocument;
use std::path::Path;

/// Declares one imported component, keyed by its path under `components/`.
#[rustfmt::skip]
pub const LOCKED: Node = Node::new("component",
    "One imported component, by its path under `components/`.").example("\"browser\"").minimal()
    .arg(Arg::Str, Say::new("`component` needs the path of the component", "no path given",
        "`component \"browser\"`, the directory under `components/`"))
    .unique(Say::new("`{}` is locked twice", "a second entry would disagree",
        "the lock holds one entry for each component; run `tect import component` again to rewrite it"))
    .children(&[
        Node::new("library", "The library the component was imported from, as `sources` in `repo.kdl` names it.").example("\"tectonic-os\"")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .missing(Say::new("a lock entry records no `{}`", "origin unknown",
                "the name the library is declared under in `sources`")),
        Node::new("path", "The path of the component inside the library. It differs from the entry's own path when the component was imported under another name.").example("\"browser\"")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .missing(Say::new("a lock entry records no `{}`", "origin unknown",
                "the path of the component inside its library")),
        Node::new("url", "The Git URL of the library, for a library that `tect` fetched.").example("\"https://github.com/tectonic-os/library.git\"")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("component-version", "The revision of the library that the component was taken at, for a library that `tect` fetched.").example("\"v0.6.51\"")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("content", "The hash that the component directory had at import.").example("\"b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3\"")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .missing(Say::new("a lock entry records no `{}` hash", "nothing to compare against",
                "the sha256 of the imported directory, which is what makes a later edit visible")),
        Node::new("track", "Marks a copy that tracks its library. An untracked copy is the repository's own.")
            .arg(Arg::None, Say::new("`track` takes no value", "unexpected value",
                "write `track` alone, or leave it out for a component the repository owns"))
            .once(""),
    ], Say::new("unknown node `{}` in a lock entry", "not part of the schema",
        "a lock entry holds `library`, `path`, `url`, `component-version`, `content` and `track`"));

/// Declares the lock file. Its top-level nodes are the entries.
#[rustfmt::skip]
pub(crate) const LOCK_FILE: Node = Node::new("lock file",
    "The record of every imported component, which `tect import component` writes.")
    .about("When `tect import component` copies a component into this repository, it records the component \
        here. The entry keeps where the component came from and what its content hashed to, so \
        `tect check`, `tect generate` and `tect build` can tell whether the copy has changed since.")
    .children(&[LOCKED], Say::new("unknown node `{}` in the lock", "not part of the schema",
        "the lock holds one `component` entry for each imported component"))
    .notes(&[
        "`tect import component` writes the whole file. The user does not edit it, and every command \
         that reads the images refuses an entry that does not read.",
        "`tect fetch components` copies each tracked component again from its library, and records the \
         new `content` and `component-version`. `tect set component <path> --untracked` drops \
         `track` and keeps the rest of the entry.",
        "The entry is keyed by the local path, so one library component imported twice under two \
         names has two entries.",
        "`plan.json` carries the same hash for each component, so `tect verify` fails on a component \
         edited without a new `tect generate`.",
        "If a tracked component's content no longer matches its entry, or its directory is missing, \
         then every command that reads the images refuses it, `tect build` included. `audit` does \
         not change this.",
        "If an untracked component's content no longer matches its entry, then `tect why` reads the \
         edit out. The copy is the repository's own, so no command refuses it.",
    ]);

/// Holds one component's lock entry.
pub struct Record {
    /// Keys the entry by the component's path under `components/`.
    pub path: String,
    pub library: String,
    /// Names the component's path inside the library.
    pub upstream: String,
    /// Holds the library's locator and the revision taken. A library that is
    /// a directory on this machine has neither.
    pub pin: Evidence,
    /// Holds the directory hash as it stood at import.
    pub content: String,
    pub tracked: bool,
}

/// Reads every entry in the lock. A lock that does not parse reads as empty.
/// `tect check` reports it.
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

/// Finds the entry for the component at `dir`, its path under `components/`.
pub fn entry(root: &Path, dir: &str) -> Option<Record> {
    read(root).into_iter().find(|record| record.path == dir)
}

/// Renders the whole lock with `record` added, or replacing the entry at its
/// path. The tool writes whole files, so a lock that does not parse is refused
/// rather than rewritten from nothing, which would drop every other entry.
pub fn with(root: &Path, record: Record) -> Result<String, String> {
    parses(root).map_err(|err| {
        format!("{err}; importing would drop its other entries, so resolve it, then import again")
    })?;
    let mut records: Vec<Record> = read(root)
        .into_iter()
        .filter(|held| held.path != record.path)
        .collect();
    records.push(record);
    records.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(records.iter().map(render).collect::<Vec<_>>().join("\n"))
}

/// Refuses a lock that does not parse. `read` answers such a lock as empty.
pub fn parses(root: &Path) -> Result<(), String> {
    let path = layout::lock(root);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    raw.parse::<KdlDocument>()
        .map(drop)
        .map_err(|err| format!("{} does not parse: {err}", path.display()))
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
    format!("component {} {{\n{body}}}\n", quoted(&record.path))
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Hashes every file under a component directory by its path and its content, so
/// renaming a file changes the hash. A component that the directory holds of its
/// own has a hash of its own, so its files stay out of this one.
pub fn hash(dir: &Path) -> Option<String> {
    let held = layout::held_components(dir);
    let mut listed: Vec<String> = Vec::new();
    for path in crate::tracked(dir) {
        let rel = path.strip_prefix(dir).ok()?;
        if held.iter().any(|component| rel.starts_with(component)) {
            continue;
        }
        listed.push(rel.to_string_lossy().into_owned());
    }
    if listed.is_empty() {
        return None;
    }
    crate::runtime::sha256_tree(dir, &listed).ok()
}

/// Holds the lock to its grammar and refuses each tracked component that no
/// longer matches its entry. A tracked copy is immutable, so `audit` does not
/// change the refusal. An untracked copy is the repository's own.
/// `tect why` reads out its mismatch.
pub fn check(root: &Path, issues: &mut Issues) {
    let path = layout::lock(root);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    let name = path.display().to_string();
    let src = Source::new(&name, raw.clone());
    let doc = match raw.parse::<KdlDocument>() {
        Ok(doc) => doc,
        Err(err) => {
            issues.push(syntax_issue(&err, &name, &src));
            return;
        }
    };
    check_doc(&doc, &LOCK_FILE, &src, issues);
    for record in read(root) {
        // The key is joined onto `components/` and hashed, so a key that climbs out
        // of `components/` would hash files outside the repository.
        if !inside(&record.path) {
            issues.push(
                Issue::new(
                    format!("`{}` is not a path under `components/`", record.path),
                    &src,
                )
                .at(record.pin.span, "this entry")
                .help(
                    "a lock entry is keyed by a path under `components/`; run `tect import \
                         component` again to rewrite it",
                ),
            );
            continue;
        }
        // The grammar already reports an entry with no `content`.
        if !record.tracked || record.content.is_empty() {
            continue;
        }
        let component = format!("{}/{}", layout::COMPONENTS, record.path);
        let dir = layout::component(root, &record.path);
        let issue = match hash(&dir) {
            Some(found) if found == record.content => continue,
            Some(_) => Issue::new(
                format!("`{component}` tracks its library, and its content no longer matches its lock entry"),
                &src,
            )
            .help(format!(
                "undo the edit to `{component}`, or run `tect set component {} --untracked` to make the \
                 copy the repository's own",
                record.path
            )),
            None if !dir.exists() => Issue::new(
                format!("`{component}` tracks its library, and the directory is missing"),
                &src,
            )
            .help(restore(&component, &record)),
            None => Issue::new(
                format!("`{component}` tracks its library, and its content cannot be hashed"),
                &src,
            )
            .help(format!(
                "`{component}` holds no file of its own, or a file in it cannot be read; restore its \
                 files from version control"
            )),
        };
        issues.push(issue.at(record.pin.span, "this entry"));
    }
}

/// Answers the lock key that a path the user typed names. The user may type
/// the key, or the path from the repository root.
pub fn key(path: &str) -> &str {
    let mut key = path;
    while let Some(rest) = key.strip_prefix("./") {
        key = rest;
    }
    key.strip_prefix(&format!("{}/", layout::COMPONENTS))
        .unwrap_or(key)
        .trim_end_matches('/')
}

/// Answers whether a lock key stays under `components/` when joined onto it.
pub(crate) fn inside(key: &str) -> bool {
    !key.is_empty()
        && Path::new(key)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

/// `tect fetch components` copies the entry's library path to the entry's own
/// key, so it restores a renamed copy too.
fn restore(component: &str, record: &Record) -> String {
    format!(
        "restore `{component}` from version control, or run `tect fetch components {}` to copy it \
         again",
        record.path
    )
}

/// Makes the tracked component at `key` the repository's own. The entry keeps
/// the origin and the content hash as evidence.
pub fn untrack(root: &Path, key: &str) -> Result<bool, String> {
    parses(root)?;
    let Some(record) = entry(root, key) else {
        return Err(format!(
            "`{}/{key}` has no entry in the lock, so it is already the repository's own",
            layout::COMPONENTS
        ));
    };
    if !record.tracked {
        return Ok(false);
    }
    let locked = with(
        root,
        Record {
            tracked: false,
            ..record
        },
    )?;
    crate::init::put(&layout::lock(root), &locked)?;
    Ok(true)
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
        let dir = layout::component(&root, "browser@1");
        crate::init::put(&dir.join(layout::MANIFEST_FILE), "description \"x\"\n").unwrap();
        let mut held = record("browser@1", true);
        held.content = hash(&dir).unwrap();
        crate::init::put(&layout::lock(&root), &with(&root, held).unwrap()).unwrap();

        let mut issues = Issues::default();
        check(&root, &mut issues);
        assert!(issues.is_empty(), "{}", issues.plain());
        let read = entry(&root, "browser@1").expect("the entry is there");
        assert_eq!(read.library, "tectonic-os");
        assert_eq!(read.upstream, "browser");
        assert_eq!(Some(read.content), hash(&dir));
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
    fn an_edited_tracked_component_is_refused_and_an_edited_untracked_one_is_not() {
        let root = std::env::temp_dir().join(format!("tect-lock-edit-{}", std::process::id()));
        for (name, tracked) in [("browser", true), ("editor", false)] {
            let dir = layout::component(&root, name);
            crate::init::put(&dir.join(layout::MANIFEST_FILE), "description \"x\"\n").unwrap();
            let mut held = record(name, tracked);
            held.content = hash(&dir).unwrap();
            crate::init::put(&layout::lock(&root), &with(&root, held).unwrap()).unwrap();
        }
        let mut issues = Issues::default();
        check(&root, &mut issues);
        assert!(issues.is_empty(), "{}", issues.plain());

        for name in ["browser", "editor"] {
            let dir = layout::component(&root, name);
            crate::init::put(&dir.join(layout::MANIFEST_FILE), "description \"y\"\n").unwrap();
        }
        let mut issues = Issues::default();
        check(&root, &mut issues);
        let said = issues.plain();
        assert!(
            said.contains("`components/browser` tracks its library"),
            "{said}"
        );
        assert!(!said.contains("components/editor"), "{said}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_key_outside_components_is_refused() {
        let root = std::env::temp_dir().join(format!("tect-lock-climb-{}", std::process::id()));
        crate::init::put(
            &layout::lock(&root),
            "component \"../..\" {\n    library \"x\"\n    path \"x\"\n    content \"0\"\n    track\n}\n",
        )
        .unwrap();
        let mut issues = Issues::default();
        check(&root, &mut issues);
        let said = issues.plain();
        assert!(
            said.contains("`../..` is not a path under `components/`"),
            "{said}"
        );
        assert!(!said.contains("tracks its library"), "{said}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_empty_tracked_directory_is_not_called_missing() {
        let root = std::env::temp_dir().join(format!("tect-lock-empty-{}", std::process::id()));
        std::fs::create_dir_all(layout::component(&root, "browser")).unwrap();
        let held = record("browser", true);
        crate::init::put(&layout::lock(&root), &with(&root, held).unwrap()).unwrap();
        let mut issues = Issues::default();
        check(&root, &mut issues);
        let said = issues.plain();
        assert!(said.contains("cannot be hashed"), "{said}");
        assert!(!said.contains("missing"), "{said}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_library_path_with_an_underscore_is_a_key() {
        assert!(inside("tools/node_exporter"));
        assert!(inside("browser@1"));
        assert!(!inside("../browser"));
        assert!(!inside("/etc"));
        assert!(!inside(""));
    }

    #[test]
    fn a_missing_renamed_component_is_restored_under_its_own_key() {
        let root = std::env::temp_dir().join(format!("tect-lock-gone-{}", std::process::id()));
        crate::init::put(
            &layout::lock(&root),
            &with(&root, record("browser@1", true)).unwrap(),
        )
        .unwrap();
        let mut issues = Issues::default();
        check(&root, &mut issues);
        let said = issues.plain();
        assert!(said.contains("the directory is missing"), "{said}");
        // The plain rendering wraps the help line.
        let said = said.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(said.contains("tect fetch components browser@1"), "{said}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_typed_path_names_its_key() {
        assert_eq!(key("browser"), "browser");
        assert_eq!(key("components/core/browser/"), "core/browser");
        assert_eq!(key("./components/browser@1"), "browser@1");
    }

    #[test]
    fn untracking_a_lock_that_does_not_parse_names_the_lock() {
        let root =
            std::env::temp_dir().join(format!("tect-lock-untrack-broken-{}", std::process::id()));
        crate::init::put(&layout::lock(&root), "component \"browser\" {\n").unwrap();
        let refused = untrack(&root, "browser").unwrap_err();
        assert!(refused.contains("does not parse"), "{refused}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn untracking_keeps_the_origin_and_the_hash() {
        let root = std::env::temp_dir().join(format!("tect-lock-untrack-{}", std::process::id()));
        crate::init::put(
            &layout::lock(&root),
            &with(&root, record("browser", true)).unwrap(),
        )
        .unwrap();
        assert_eq!(untrack(&root, "browser"), Ok(true));
        let held = entry(&root, "browser").expect("the entry stays");
        assert!(!held.tracked);
        assert_eq!(held.library, "tectonic-os");
        assert_eq!(held.upstream, "browser");
        assert_eq!(held.content, "abc");
        assert_eq!(held.pin.version.as_deref(), Some("v1.0.0"));
        assert_eq!(untrack(&root, "browser"), Ok(false));
        assert!(untrack(&root, "editor").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The overlay file is named `manifest.kdl` to prove that the overlay never
    /// reads as a held component.
    #[test]
    fn a_held_component_stays_out_of_the_hash_and_the_overlay_does_not() {
        let root = std::env::temp_dir().join(format!("tect-lock-held-{}", std::process::id()));
        let dir = layout::component(&root, "nvidia");
        crate::init::put(&dir.join(layout::MANIFEST_FILE), "description \"x\"\n").unwrap();
        let before = hash(&dir).unwrap();

        crate::init::put(&dir.join("cuda").join(layout::MANIFEST_FILE), "").unwrap();
        crate::init::put(&dir.join("cuda/component.sh"), "true\n").unwrap();
        assert_eq!(hash(&dir).unwrap(), before);

        let overlay = dir
            .join(layout::OVERLAY)
            .join("usr/share/x")
            .join(layout::MANIFEST_FILE);
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
        check(&root, &mut issues);
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
            "component \"browser\" {\n    library \"one\"\n}\n",
        )
        .unwrap();
        let mut issues = Issues::default();
        check(&root, &mut issues);
        let said = issues.plain();
        assert!(said.contains("records no `path`"), "{said}");
        std::fs::remove_dir_all(root).unwrap();
    }
}

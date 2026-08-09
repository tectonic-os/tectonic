//! Every command, over this repository and over the fixture repositories,
//! compared byte for byte against a committed golden.
//!
//! Regenerate with `UPDATE_GOLDEN=1 cargo test`, then read the diff.

use std::path::{Path, PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn compare(name: &str, file: &str, actual: &str) {
    let path = crate_dir().join("tests/golden").join(name).join(file);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{}: {err}\nrun UPDATE_GOLDEN=1 cargo test", path.display()));
    assert!(
        expected == actual,
        "{} changed. Rerun with UPDATE_GOLDEN=1 and read the diff",
        path.display()
    );
}

/// The plan, the generated section and every diagnostic, for one repository.
///
/// Runs from inside the repository so every path a diagnostic prints is
/// relative to it, which is what makes a golden the same on any machine.
fn capture(name: &str, root: &Path) {
    std::env::set_current_dir(root).expect("fixture root exists");
    let here = Path::new(".");
    for (command, file) in [("plan", "plan.json"), ("section", "section.txt")] {
        compare(name, file, &tect::run(command, None, here).stdout);
    }
    compare(
        name,
        "issues.txt",
        &tect::run("check", None, here).issues.plain(),
    );
}

/// A repository `tect init` wrote, captured like any other fixture: what it
/// scaffolds has to resolve, generate and report nothing.
fn init_repo() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("init");
    let _ = std::fs::remove_dir_all(&root);
    tect::init::write(&root, "Example", "someone", &crate_dir().join("assets")).unwrap();
    root
}

/// The reference in docs/schema.md, re-rendered from the tables. The renderer
/// is what checks that every marker names a schema and every schema is marked.
#[test]
fn schema_doc() {
    let path = crate_dir().join("docs/schema.md");
    let doc = std::fs::read_to_string(&path).expect("docs/schema.md exists");
    let rendered = tect::emit::schema_md::render(&doc).unwrap_or_else(|err| panic!("{err}"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, rendered).unwrap();
        return;
    }
    assert!(
        doc == rendered,
        "docs/schema.md is stale. Rerun with UPDATE_GOLDEN=1 and read the diff"
    );
}

/// One process, one working directory: every capture runs in turn.
#[test]
fn golden() {
    let dir = crate_dir().join("tests/repos");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("tests/repos exists")
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert!(!names.is_empty());

    let init = init_repo();
    for name in names {
        capture(&name, &dir.join(&name));
    }
    capture("init", &init);
}

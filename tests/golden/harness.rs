//! The harness every case shares: paths, snapshot assertions and file helpers.

use std::path::{Path, PathBuf};

pub(super) fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `root` as a path from the process working directory, which cargo and
/// nextest set to the package root. Passing it relative is what keeps a
/// snapshot from holding this machine's paths.
pub(super) fn given(root: &Path) -> PathBuf {
    let here = std::env::current_dir().expect("the test process has a working directory");
    root.strip_prefix(&here)
        .unwrap_or_else(|_| {
            panic!(
                "run the tests from {} so {} stays relative",
                here.display(),
                root.display()
            )
        })
        .to_path_buf()
}

/// The body of one case's snapshot, which the CLI reference splices into the
/// document it renders.
pub(super) fn snapshot_body(name: &str, file: &str) -> String {
    let path = crate_dir()
        .join("tests/snapshots")
        .join(format!("{name}__{file}.snap"));
    insta::Snapshot::from_file(&path)
        .unwrap_or_else(|err| panic!("{}: {err}", path.display()))
        .as_text()
        .expect("the golden is a text snapshot")
        .to_string()
}

pub(super) fn normalize_snapshot_body(text: &str) -> String {
    text.trim_end().replace("\r\n", "\n")
}

pub(super) fn assert_golden(name: &str, file: &str, actual: &str) {
    let mut settings = insta::Settings::clone_current();
    // A module file lives under tests/golden/, so insta's default resolves
    // beside it; the snapshots belong to the crate, not the module.
    settings.set_snapshot_path(crate_dir().join("tests/snapshots"));
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.add_filter(&env!("CARGO_PKG_VERSION").replace('.', r"\."), "{version}");
    settings.bind(|| insta::assert_snapshot!(format!("{name}__{file}"), actual));
}

/// `CARGO_TARGET_TMPDIR` when it sits under the package, and a root under
/// `target/` otherwise. A build that points `CARGO_TARGET_DIR` outside the
/// package still gets a scratch root `given` can keep relative.
fn scratch_root(at: PathBuf) -> PathBuf {
    match at.strip_prefix(crate_dir()) {
        Ok(_) => at,
        Err(_) => crate_dir().join("target/test-tmp"),
    }
}

pub(super) fn tmp() -> PathBuf {
    static AT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    AT.get_or_init(|| {
        let at = scratch_root(PathBuf::from(env!("CARGO_TARGET_TMPDIR")));
        std::fs::create_dir_all(&at).expect("a scratch directory the tests can write");
        at
    })
    .clone()
}

#[test]
fn an_out_of_package_scratch_root_moves_under_the_package() {
    assert_eq!(
        scratch_root(PathBuf::from("/tmp/elsewhere/tmp")),
        crate_dir().join("target/test-tmp")
    );
    let inside = crate_dir().join("target/tmp");
    assert_eq!(scratch_root(inside.clone()), inside);
}

pub(super) fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dest) = (entry.path(), to.join(entry.file_name()));
        match src.is_dir() {
            true => copy(&src, &dest),
            false => drop(std::fs::copy(&src, &dest).unwrap()),
        }
    }
}

/// Lays the fixture base catalog and capabilities where the fetch of the
/// default libraries would put them, so a sealed run reads a catalog instead
/// of the network.
pub(super) fn seed_bases(root: &Path) {
    let keyed = tect::import::cache_path(root, tect::base::LIBRARY_URL, "HEAD")
        .expect("the default library URL hashes");
    let collections = crate_dir().join("tests/collections/upstream");
    let core = keyed.join("base-images/core");
    std::fs::create_dir_all(&core).unwrap();
    for entry in std::fs::read_dir(&collections).unwrap().flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".base.kdl") {
            std::fs::copy(entry.path(), core.join(name)).unwrap();
        }
    }
    let capabilities = keyed.join("capabilities");
    std::fs::create_dir_all(&capabilities).unwrap();
    std::fs::copy(
        collections.join("capabilities.kdl"),
        capabilities.join("capabilities.kdl"),
    )
    .unwrap();
}

/// The same, with the fixture module tree as well, for a flow whose offer has
/// to read what the default modules library holds.
pub(super) fn seed_library(root: &Path) {
    seed_bases(root);
    let keyed = tect::import::cache_path(root, tect::base::LIBRARY_URL, "HEAD")
        .expect("the default library URL hashes");
    let collections = crate_dir().join("tests/collections/upstream");
    copy(&collections, &keyed.join("modules"));
    for stray in ["capabilities.kdl", "fedora.base.kdl", "collected.base.kdl"] {
        let _ = std::fs::remove_file(keyed.join("modules").join(stray));
    }
}

/// Every file under `dir`, which is what an import wrote.
pub(super) fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        match path.is_dir() {
            true => out.extend(walk(&path)),
            false => out.push(path),
        }
    }
    out
}

pub(super) fn contents(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<(PathBuf, Vec<u8>)> = walk(dir)
        .into_iter()
        .map(|path| {
            let name = path.strip_prefix(dir).unwrap().to_path_buf();
            (name, std::fs::read(path).unwrap())
        })
        .collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

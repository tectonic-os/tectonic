use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tect::base::BASES_FILE;
use tect::diag::{Issues, Span};
use tect::model::remote::{At, Collection};

static ENV: Mutex<()> = Mutex::new(());

/// Diagnostics wrap at 80 columns, so a phrase breaks wherever the absolute
/// temp path pushes it. Flatten the gutter away before matching one.
fn flat(text: &str) -> String {
    text.split_whitespace()
        .filter(|word| !matches!(*word, "x" | "|"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn flattening_survives_a_wrap_mid_phrase() {
    // Given: the rendering a longer path produced, broken between "be" and "read".
    let wrapped = "  x /home/runner/work/tectonic/tectonic/target/tmp/io/bases.kdl could not be\n  | read: Is a directory (os error 21)\n";

    // Then: the phrase and the path both match again.
    let flat = flat(wrapped);
    assert!(flat.contains("could not be read"), "{flat}");
    assert!(
        flat.contains("/home/runner/work/tectonic/tectonic/target/tmp/io/bases.kdl"),
        "{flat}"
    );
}

struct Assets {
    _lock: MutexGuard<'static, ()>,
    old: Option<std::ffi::OsString>,
    path: PathBuf,
}

impl Assets {
    fn new(name: &str) -> Self {
        let lock = ENV
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let old = std::env::var_os("TECT_ASSETS");
        std::env::set_var("TECT_ASSETS", &path);
        Self {
            _lock: lock,
            old,
            path,
        }
    }

    fn write(&self, text: &str) {
        std::fs::write(self.path.join(BASES_FILE), text).unwrap();
    }
}

impl Drop for Assets {
    fn drop(&mut self) {
        match self.old.take() {
            Some(value) => std::env::set_var("TECT_ASSETS", value),
            None => std::env::remove_var("TECT_ASSETS"),
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn public_catalog_lists_every_shipped_base_in_order() {
    // Given: no runtime file or collection sources.
    let _assets = Assets::new("task-2-characterized-bases");
    let mut issues = Issues::default();

    // When: the public catalog is loaded.
    let (bases, shadows) = tect::base::catalog(Path::new("."), &[], &mut issues);

    // Then: every shipped row is present, in the order assets/bases.kdl writes them.
    assert!(issues.is_empty());
    assert!(shadows.is_empty());
    assert_eq!(
        bases
            .iter()
            .map(|base| base.image.as_str())
            .collect::<Vec<_>>(),
        [
            "quay.io/fedora/fedora-bootc:44",
            "ghcr.io/ublue-os/bazzite:stable",
            "ghcr.io/ublue-os/aurora:stable",
            "ghcr.io/ublue-os/bluefin:stable",
            "ghcr.io/ublue-os/kinoite-main:44",
            "ghcr.io/bootcrew/debian-bootc:latest",
            "ghcr.io/bootcrew/ubuntu-bootc:latest",
        ]
    );
}

#[test]
fn runtime_file_replaces_embedded_catalog() {
    // Given: one valid row in the runtime catalog.
    let assets = Assets::new("task-2-runtime-replaces");
    assets.write(
        r#"base "example.invalid/runtime:1" {
    about "runtime only"
    family "runtime"
}
"#,
    );

    // When: the public catalog is loaded without collections.
    let mut issues = Issues::default();
    let (bases, shadows) = tect::base::catalog(Path::new("."), &[], &mut issues);

    // Then: only the runtime row is selected.
    assert!(issues.is_empty(), "{}", issues.plain());
    assert!(shadows.is_empty());
    assert_eq!(bases.len(), 1);
    assert_eq!(bases[0].image, "example.invalid/runtime:1");
}

#[test]
fn missing_runtime_file_falls_back_to_embedded_catalog() {
    // Given: an asset directory with no runtime bases file.
    let _assets = Assets::new("task-2-runtime-missing");

    // When: the public catalog is loaded.
    let mut issues = Issues::default();
    let (bases, shadows) = tect::base::catalog(Path::new("."), &[], &mut issues);

    // Then: the embedded seven rows are selected.
    assert!(issues.is_empty(), "{}", issues.plain());
    assert!(shadows.is_empty());
    assert_eq!(bases.len(), 7);
    assert_eq!(bases[5].image, "ghcr.io/bootcrew/debian-bootc:latest");
    assert_eq!(bases[5].family, "debian");
    assert_eq!(bases[5].provides, ["initramfs-generation"]);
    assert!(bases[5].signed);
    assert_eq!(bases[6].image, "ghcr.io/bootcrew/ubuntu-bootc:latest");
    assert_eq!(bases[6].family, "ubuntu");
    assert!(bases[6].signed);
}

#[test]
fn malformed_runtime_file_does_not_substitute_embedded_catalog() {
    // Given: a present malformed runtime bases file.
    let assets = Assets::new("task-2-runtime-malformed");
    assets.write("base {");

    // When: the public catalog is loaded.
    let mut issues = Issues::default();
    let (bases, shadows) = tect::base::catalog(Path::new("."), &[], &mut issues);

    // Then: its diagnostic is retained and no embedded row is substituted.
    assert!(bases.is_empty());
    assert!(shadows.is_empty());
    assert!(flat(&issues.plain()).contains(&assets.path.join(BASES_FILE).display().to_string()));
}

#[test]
fn present_unreadable_runtime_file_is_diagnosed_without_fallback() {
    // Given: the selected runtime bases path is a directory, not a readable file.
    let assets = Assets::new("io");
    let path = assets.path.join(BASES_FILE);
    std::fs::create_dir(&path).unwrap();

    // When: the public catalog is loaded.
    let mut issues = Issues::default();
    let (bases, shadows) = tect::base::catalog(Path::new("."), &[], &mut issues);

    // Then: the read failure names the exact path and embedded rows stay absent.
    assert!(bases.is_empty());
    assert!(shadows.is_empty());
    let diagnostic = flat(&issues.plain());
    assert!(
        diagnostic.contains(&path.display().to_string()),
        "{diagnostic}"
    );
    assert!(diagnostic.contains("could not be read"), "{diagnostic}");
}

#[test]
fn create_image_surfaces_an_unreadable_runtime_catalog() {
    // Given: a repository and a runtime bases path that is a directory.
    let assets = Assets::new("create-io");
    let path = assets.path.join(BASES_FILE);
    std::fs::create_dir(&path).unwrap();
    let root = assets.path.join("repo");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("repo.kdl"), "schema-version 1\n").unwrap();

    // When: image collection reads the selected catalog.
    let error = tect::create::Image::collect(
        &root,
        Some("Example".to_string()),
        Some("example.invalid/base:1".to_string()),
        "Example",
        None,
        "a name argument",
        &tect::prompt::Prompt::silent(),
    )
    .err()
    .expect("the unreadable runtime catalog must stop image creation");

    // Then: the create error retains the exact source and read failure.
    let error = flat(&error);
    assert!(error.contains(&path.display().to_string()), "{error}");
    assert!(error.contains("could not be read"), "{error}");
}

#[test]
fn collection_still_overrides_and_shadows_selected_catalog() {
    // Given: the embedded catalog and a cached collection overriding its first row.
    let assets = Assets::new("task-2-collection-shadow-assets");
    let root = assets.path.join("root");
    let collection_dir = root.join("collection");
    std::fs::create_dir_all(&collection_dir).unwrap();
    std::fs::write(
        collection_dir.join(BASES_FILE),
        r#"base "quay.io/fedora/fedora-bootc:44" {
    about "collection replacement"
    family "fedora"
    signed #true
}
"#,
    )
    .unwrap();
    let sources = [Collection {
        name: "one".to_string(),
        at: At::Dir("collection".to_string()),
        span: Span::default(),
    }];

    // When: the public catalog applies the collection.
    let mut issues = Issues::default();
    let (bases, shadows) = tect::base::catalog(&root, &sources, &mut issues);

    // Then: order is retained, the row is replaced, and the shadow is reported.
    assert!(issues.is_empty(), "{}", issues.plain());
    assert_eq!(bases.len(), 7);
    assert_eq!(bases[0].about, "collection replacement");
    assert!(bases[0].signed);
    assert_eq!(shadows.len(), 1);
    assert_eq!(shadows[0].image, "quay.io/fedora/fedora-bootc:44");
    assert_eq!(shadows[0].collection, "one");
}

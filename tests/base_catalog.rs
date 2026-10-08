use std::path::{Path, PathBuf};
use tect::diag::{Issues, Span};
use tect::model::remote::{At, Collection, Kind as SourceKind};

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
    let wrapped = "  x /home/runner/work/tectonic/tectonic/target/tmp/io/fedora.base.kdl could not be\n  | read: Is a directory (os error 21)\n";

    // Then: the phrase and the path both match again.
    let flat = flat(wrapped);
    assert!(flat.contains("could not be read"), "{flat}");
    assert!(
        flat.contains("/home/runner/work/tectonic/tectonic/target/tmp/io/fedora.base.kdl"),
        "{flat}"
    );
}

struct Library {
    root: PathBuf,
}

impl Library {
    /// A repository root holding one source directory, emptied first.
    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("library")).unwrap();
        Self { root }
    }

    fn at(&self, name: &str) -> PathBuf {
        let path = self.root.join("library").join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write(&self, name: &str, file: &str, text: &str) {
        std::fs::write(self.at(name).join(file), text).unwrap();
    }

    fn source(&self, name: &str) -> Collection {
        Collection {
            kind: SourceKind::BaseImages,
            name: name.to_string(),
            at: At::Dir(format!("library/{name}")),
            span: Span::default(),
        }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const FEDORA: &str = r#"base {
    image "quay.io/fedora/fedora-bootc:44"
    about "Fedora 44"
    family "fedora"
    provides "rechunking" "bootc"
    signed #true
    bootloader "grub2"
}
"#;

#[test]
fn one_file_describes_one_base_named_by_its_stem() {
    // Given: a library holding one base file.
    let library = Library::new("one-base");
    library.write("core", "fedora-bootc-44.base.kdl", FEDORA);

    // When: the catalog reads the declared source.
    let mut issues = Issues::default();
    let bases = tect::base::catalog(&library.root, &[library.source("core")], &mut issues);

    // Then: the file stem names the base and the node describes it.
    assert!(issues.is_empty(), "{}", issues.plain());
    assert_eq!(bases.len(), 1);
    assert_eq!(bases[0].name, "fedora-bootc-44");
    assert_eq!(bases[0].image, "quay.io/fedora/fedora-bootc:44");
    assert_eq!(bases[0].family, "fedora");
    assert!(bases[0].signed);
}

/// A base is found by its image reference or by the catalog name its file
/// stem gives it.
#[test]
fn a_base_is_found_by_its_image_reference_or_catalog_name() {
    let library = Library::new("find-base");
    library.write("core", "fedora-bootc-44.base.kdl", FEDORA);
    let mut issues = Issues::default();
    let bases = tect::base::catalog(&library.root, &[library.source("core")], &mut issues);
    assert!(issues.is_empty(), "{}", issues.plain());
    assert_eq!(
        tect::base::find(&bases, "fedora-bootc-44")
            .expect("the catalog name finds it")
            .image,
        "quay.io/fedora/fedora-bootc:44"
    );
}

/// One file describes one base: a second `base` node and a file with none are
/// both refused.
#[test]
fn a_base_file_holds_exactly_one_base_node() {
    let library = Library::new("two-nodes");
    let second = FEDORA.replace("fedora-bootc:44", "fedora-bootc:43");
    library.write("core", "two.base.kdl", &format!("{FEDORA}\n{second}"));
    library.write("core", "none.base.kdl", "");
    let mut issues = Issues::default();
    let bases = tect::base::catalog(&library.root, &[library.source("core")], &mut issues);
    assert!(bases.is_empty());
    let text = issues.plain();
    assert!(text.contains("`base` is declared twice"), "{text}");
    assert!(text.contains("holds no `base` node"), "{text}");
}

/// A declared base library is not a module collection, so a scan with no
/// modules source reports none and names none.
#[test]
fn a_base_library_is_not_an_unread_module_collection() {
    let library = Library::new("unread-kinds");
    library.write("core", "fedora-bootc-44.base.kdl", FEDORA);
    let disk = tect::parse::disk::Disk::scan(&library.root);
    let index = tect::provider::Index::scan(&library.root, &[library.source("core")], &disk, false);
    assert!(!index.sourced());
    assert!(index.unread().is_empty(), "{:?}", index.unread());
}

#[test]
fn a_source_that_is_not_on_this_machine_describes_nothing() {
    // Given: a repository with no library directory.
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("no-library");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = Collection {
        kind: SourceKind::BaseImages,
        name: "core".to_string(),
        at: At::Dir("missing".to_string()),
        span: Span::default(),
    };

    // When: the catalog reads it.
    let mut issues = Issues::default();
    let bases = tect::base::catalog(&root, &[source], &mut issues);

    // Then: nothing is described and nothing is wrong.
    assert!(bases.is_empty());
    assert!(issues.is_empty(), "{}", issues.plain());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn two_files_describing_one_image_are_refused_naming_both() {
    // Given: two libraries whose files name one image reference.
    let library = Library::new("two-bases");
    library.write("core", "fedora.base.kdl", FEDORA);
    library.write(
        "ublue",
        "another.base.kdl",
        &FEDORA.replace("about \"Fedora 44\"", "about \"another row\""),
    );
    let sources = [library.source("core"), {
        let mut source = library.source("ublue");
        source.name = "ublue".to_string();
        source
    }];

    // When: the catalog reads both.
    let mut issues = Issues::default();
    let bases = tect::base::catalog(&library.root, &sources, &mut issues);

    // Then: the second is refused and the first stands.
    assert_eq!(bases.len(), 1);
    let said = flat(&issues.plain());
    assert!(
        said.contains("`quay.io/fedora/fedora-bootc:44` is described by two base files"),
        "{said}"
    );
    assert!(said.contains("fedora.base.kdl"), "{said}");
}

#[test]
fn a_capability_family_block_overrides_the_default_path() {
    // Given: a capabilities library whose row reads one path on debian.
    let library = Library::new("family-capability");
    let capabilities = library.at("core");
    std::fs::write(
        capabilities.join("capabilities.kdl"),
        r#"capability "ssh" {
    path "/usr/sbin/sshd"
    family "debian" "ubuntu" {
        path "/usr/bin/ssh"
    }
}
"#,
    )
    .unwrap();
    let source = Collection {
        kind: SourceKind::Capabilities,
        name: "core".to_string(),
        at: At::Dir("library/core".to_string()),
        span: Span::default(),
    };

    // When: the rows are read and one family resolves the name.
    let mut issues = Issues::default();
    let rows = tect::base::capabilities(&library.root, &[source], &mut issues);
    assert!(issues.is_empty(), "{}", issues.plain());
    assert_eq!(rows.len(), 1);

    // Then: the named families read the override and the rest the default.
    assert_eq!(
        tect::base::witness("ssh", "debian", None, &rows),
        Some(vec!["/usr/bin/ssh".to_string()])
    );
    assert_eq!(
        tect::base::witness("ssh", "fedora", None, &rows),
        Some(vec!["/usr/sbin/sshd".to_string()])
    );
    // A claim's own path wins over every row.
    assert_eq!(
        tect::base::witness("ssh", "fedora", Some("/opt/ssh"), &rows),
        Some(vec!["/opt/ssh".to_string()])
    );
}

#[test]
fn an_abstract_row_suppresses_the_conventional_probe() {
    // Given: an abstract row and a name no row carries.
    let library = Library::new("abstract-capability");
    let core = library.at("core");
    std::fs::write(core.join("capabilities.kdl"), "capability \"rechunking\"\n").unwrap();
    let source = Collection {
        kind: SourceKind::Capabilities,
        name: "core".to_string(),
        at: At::Dir("library/core".to_string()),
        span: Span::default(),
    };
    let mut issues = Issues::default();
    let rows = tect::base::capabilities(&library.root, &[source], &mut issues);

    // Then: the chain finds nothing for the abstract name, and the probe reads
    // the conventional directories for a name no row names at all.
    assert_eq!(
        tect::base::witness("rechunking", "fedora", None, &rows),
        None
    );
    assert_eq!(tect::base::probe("rechunking", "fedora", &rows), None);
    assert_eq!(
        tect::base::probe("crun", "fedora", &rows),
        Some(vec![
            "/usr/bin/crun".to_string(),
            "/usr/sbin/crun".to_string()
        ])
    );
}

#[test]
fn a_digest_pinned_reference_still_finds_its_catalog_row() {
    // Given: one declared base.
    let library = Library::new("digest-pinned");
    library.write("core", "fedora.base.kdl", FEDORA);
    let mut issues = Issues::default();
    let bases = tect::base::catalog(&library.root, &[library.source("core")], &mut issues);
    assert!(issues.is_empty(), "{}", issues.plain());

    // When: a reference is looked up with a digest appended, as a repository
    // pinning its own base writes it.
    let pinned = format!("quay.io/fedora/fedora-bootc:44@sha256:{}", "0".repeat(64));
    let found = tect::base::find(&bases, &pinned).expect("the pinned base is the catalogued base");

    // Then: it is the same row the bare tag finds, and an uncatalogued base
    // stays uncatalogued.
    assert_eq!(found.image, "quay.io/fedora/fedora-bootc:44");
    assert!(tect::base::find(
        &bases,
        &format!("example.invalid/nosuch:1@sha256:{}", "0".repeat(64))
    )
    .is_none());
}

#[test]
fn an_unreadable_base_file_is_diagnosed_and_reads_no_row() {
    // Given: a declared library whose `*.base.kdl` entry is not UTF-8 text.
    let library = Library::new("io");
    let path = library.at("core").join("fedora.base.kdl");
    std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();

    // When: the catalog reads it.
    let mut issues = Issues::default();
    let bases = tect::base::catalog(&library.root, &[library.source("core")], &mut issues);

    // Then: the read failure names the exact path.
    assert!(bases.is_empty());
    let diagnostic = flat(&issues.plain());
    assert!(diagnostic.contains("could not be read"), "{diagnostic}");
    let squashed = diagnostic.replace(' ', "");
    assert!(
        squashed.contains(&path.display().to_string().replace(' ', "")),
        "{diagnostic}"
    );
}

#[test]
fn create_image_surfaces_an_unreadable_base_file() {
    // Given: a repository whose declared library file cannot be read.
    let library = Library::new("create-io");
    let root = library.root.join("repo");
    let path = root.join("library/core/fedora.base.kdl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
    std::fs::write(
        root.join("repo.kdl"),
        "schema-version 1\n\nsources {\n    base-images \"core\" {\n        dir \"library/core\"\n    }\n}\n",
    )
    .unwrap();

    // When: image collection reads the declared catalog.
    let error = tect::create::Image::collect(
        &root,
        Some("Example".to_string()),
        Some("example.invalid/base:1".to_string()),
        "Example",
        None,
        "a name argument",
        "",
        tect::create::Field::Image,
        None,
        &common::prompt::Prompt::silent(),
    )
    .err()
    .expect("the unreadable library must stop image creation");

    // Then: the create error retains the exact source and read failure.
    let error = flat(&error);
    assert!(error.contains("could not be read"), "{error}");
    let squashed = error.replace(' ', "");
    assert!(
        squashed.contains(&path.display().to_string().replace(' ', "")),
        "{error}"
    );
}

#[test]
fn one_name_leaves_the_family_it_coerces() {
    // Given: a declared base whose image differs from its family name.
    let library = Library::new("family");
    library.write(
        "core",
        "ubuntu.base.kdl",
        r#"base {
    image "docker.io/library/ubuntu:26.04"
    about "Ubuntu 26.04"
    family "ubuntu"
    requires "bootc-base"
    bootloader "grub2" "systemd"
}
"#,
    );

    // When: the catalog reads it.
    let mut issues = Issues::default();
    let bases = tect::base::catalog(&library.root, &[library.source("core")], &mut issues);
    assert!(issues.is_empty(), "{}", issues.plain());

    // Then: every field the image scaffold copies is the file's.
    assert_eq!(bases.len(), 1);
    let base = &bases[0];
    assert_eq!(base.name, "ubuntu");
    assert_eq!(base.family, "ubuntu");
    assert_eq!(base.requires, ["bootc-base"]);
    assert_eq!(base.bootloaders, ["grub2", "systemd"]);
    assert!(!base.signed);
}

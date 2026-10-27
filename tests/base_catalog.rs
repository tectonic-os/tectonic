#![expect(
    clippy::unwrap_used,
    reason = "a test helper panics on a failed step, which fails the calling test"
)]

use std::path::PathBuf;
use tect::diag::Span;
use tect::model::remote::{At, Collection, Kind as SourceKind};

/// A repository root holding one library directory, emptied first.
struct Library {
    root: PathBuf,
}

impl Library {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("library")).unwrap();
        Self { root }
    }

    /// Writes one component's manifest under the library, or under the
    /// repository's own `components/` where `library` is false.
    fn component(&self, library: bool, path: &str, text: &str) {
        let tree = match library {
            true => self.root.join("library"),
            false => self.root.join("components"),
        };
        std::fs::create_dir_all(tree.join(path)).unwrap();
        std::fs::write(tree.join(path).join("manifest.kdl"), text).unwrap();
    }

    fn source(&self) -> Collection {
        Collection {
            kind: SourceKind::Components,
            name: "core".to_string(),
            at: At::Dir("library".to_string()),
            span: Span::default(),
        }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const FEDORA: &str = r#"schema-version 1

description "Fedora bootc"
provides "rechunking" "bootc"
base "quay.io/fedora/fedora-bootc:$version" {
    family "fedora"
    bootloader "grub2" "systemd"
}

option "edition" type="string" {
    default "server"
}

option "version" type="string" {
    default "44"
}
"#;

#[test]
fn a_base_component_is_offered_with_its_default_version() {
    let library = Library::new("catalog-base");
    library.component(true, "base-image/fedora", FEDORA);
    library.component(true, "tools", "schema-version 1\nprovides \"tools\"\n");

    let bases = tect::base::catalog(&library.root, &[library.source()]);

    let [base] = bases.as_slice() else {
        panic!("one base is offered, and the component is not");
    };
    assert_eq!(base.name, "base-image/fedora");
    assert_eq!(base.owner.as_deref(), Some("core"));
    assert_eq!(base.image, "quay.io/fedora/fedora-bootc:44");
    assert_eq!(base.version.as_deref(), Some("44"));
    assert_eq!(base.family, "fedora");
    assert_eq!(base.provides, ["rechunking", "bootc"]);
    assert_eq!(base.bootloaders, ["grub2", "systemd"]);
}

#[test]
fn the_repository_s_own_base_shadows_the_library_s() {
    let library = Library::new("catalog-shadow");
    library.component(true, "base-image/fedora", FEDORA);
    library.component(
        false,
        "base-image/fedora",
        "schema-version 1\nbase \"example.invalid/own:1\"\n",
    );

    let bases = tect::base::catalog(&library.root, &[library.source()]);

    let [base] = bases.as_slice() else {
        panic!("one base is offered");
    };
    assert_eq!(base.owner, None);
    assert_eq!(base.image, "example.invalid/own:1");
}

#[test]
fn a_base_is_found_by_its_path_or_its_image_reference() {
    let library = Library::new("catalog-find");
    library.component(true, "base-image/fedora", FEDORA);
    let bases = tect::base::catalog(&library.root, &[library.source()]);

    for name in [
        "base-image/fedora",
        "quay.io/fedora/fedora-bootc:44",
        "quay.io/fedora/fedora-bootc:44@sha256:abc",
    ] {
        assert!(tect::base::find(&bases, name).is_some(), "{name}");
    }
    assert!(tect::base::find(&bases, "quay.io/fedora/fedora-bootc:43").is_none());
}

#[test]
fn a_source_that_is_not_on_this_machine_offers_nothing() {
    let library = Library::new("catalog-absent");
    let absent = Collection {
        at: At::Dir("nowhere".to_string()),
        ..library.source()
    };
    assert!(tect::base::catalog(&library.root, &[absent]).is_empty());
}

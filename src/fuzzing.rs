use crate::diag::Issues;
use crate::model::image::List;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn root(name: &str) -> tempfile::TempDir {
    let prefix = format!("tect-fuzz-{name}-");
    let mut builder = tempfile::Builder::new();
    builder
        .prefix(&prefix)
        .permissions(Permissions::from_mode(0o700));
    builder
        .tempdir()
        .unwrap_or_else(|error| panic!("cannot create fuzz input directory: {error}"))
}

fn write(path: &Path, text: &str) {
    let parent = path.parent().expect("fuzz input path has a parent");
    std::fs::create_dir_all(parent).unwrap_or_else(|error| {
        panic!(
            "cannot create fuzz input directory {}: {error}",
            parent.display()
        )
    });
    std::fs::write(path, text)
        .unwrap_or_else(|error| panic!("cannot write fuzz input {}: {error}", path.display()));
}

/// The image every repository here builds, on the base component at `BASE`.
const IMAGE: &str = "schema-version 1\nimage {\n    name \"Fuzz\"\n    base-image {\n        fuzz\n    }\n    fuzz\n}\n";
const BASE: &str = "components/base-image/fuzz/manifest.kdl";
const BASE_MANIFEST: &str =
    "schema-version 1\nbase \"example.invalid/fuzz:1\" { family \"fedora\"; }\n";

pub fn repository_kdl(kind: u8, text: &str) {
    let root = root(&kind.to_string());
    match kind % 4 {
        0 => {
            write(&root.path().join("repo.kdl"), text);
            let _ = List::load(root.path());
        }
        1 => {
            write(
                &root.path().join("repo.kdl"),
                "schema-version 1\nname \"Fuzz\"\n",
            );
            write(&root.path().join("image.kdl"), text);
            let _ = List::load(root.path());
        }
        2 => {
            write(
                &root.path().join("repo.kdl"),
                "schema-version 1\nname \"Fuzz\"\n",
            );
            write(&root.path().join("image.kdl"), IMAGE);
            write(&root.path().join(BASE), BASE_MANIFEST);
            write(&root.path().join("components/fuzz/manifest.kdl"), text);
            let _ = crate::load(root.path());
        }
        _ => {
            write(
                &root.path().join("repo.kdl"),
                "schema-version 1\nname \"Fuzz\"\n",
            );
            write(&root.path().join("image.kdl"), IMAGE);
            write(&root.path().join(BASE), text);
            let _ = crate::bases::catalog(root.path(), &[]);
            let _ = crate::load(root.path());
        }
    }
}

pub fn kdl_rewrites(text: &str) {
    fuzz_rewrites(text);
    let root = root("rewrite");
    let file = root.path().join("image.kdl");
    write(&file, text);
    crate::create::fuzz_rewrite(&file);
}

fn fuzz_rewrites(text: &str) {
    let spans = [
        crate::parse::repo::workflows_span(text),
        crate::parse::image::conforms_span(text, "fuzz"),
        crate::parse::component::satisfies_span(text),
    ];
    for span in spans {
        let _ = crate::declaration::splice(text, span, "fuzz \"value\"");
        let _ = crate::declaration::splice(text, span, "");
    }
}

pub fn provenance_record(text: &str) {
    let root = root("record");
    write(&crate::layout::lock(root.path()), text);
    let _ = crate::provenance::lock::read(root.path());
    crate::provenance::lock::check(root.path(), &mut Issues::default());
}

pub fn component_scripts(text: &str) {
    let root = root("scripts");
    let component = "schema-version 1\ndescription \"Fuzz\"\ntargets \"fedora\"\n";
    let repo = "schema-version 1\nname \"Fuzz\"\nsecurity-policy {\n    network {\n        scripts \"deny\"\n    }\n}\n";
    write(&root.path().join("repo.kdl"), repo);
    write(&root.path().join("image.kdl"), IMAGE);
    write(&root.path().join(BASE), BASE_MANIFEST);
    write(&root.path().join("components/fuzz/manifest.kdl"), component);
    write(&root.path().join("components/fuzz/component.sh"), text);
    let _ = crate::load(root.path());
}

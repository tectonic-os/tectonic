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
            let image = "schema-version 1\nimage {\n    name \"Fuzz\"\n    base \"example.invalid/fuzz:1\" { family \"fedora\" }\n    modules { module \"fuzz\" }\n}\n";
            write(
                &root.path().join("repo.kdl"),
                "schema-version 1\nname \"Fuzz\"\n",
            );
            write(&root.path().join("image.kdl"), image);
            write(&root.path().join("modules/fuzz/module.kdl"), text);
            let _ = crate::load(root.path());
        }
        _ => {
            let path = root.path().join("fuzz.base.kdl");
            write(&path, text);
            let _ = crate::parse::bases::read_base(&path, &mut Issues::default());
        }
    }
}

pub fn kdl_rewrites(text: &str) {
    crate::set::fuzz_rewrites(text);
    let root = root("rewrite");
    let file = root.path().join("image.kdl");
    write(&file, text);
    crate::create::fuzz_rewrite(&file);
}

pub fn provenance_record(text: &str) {
    let root = root("record");
    write(&crate::layout::lock(root.path()), text);
    let _ = crate::provenance::lock::read(root.path());
    crate::provenance::lock::check(root.path(), &mut Issues::default());
}

pub fn module_scripts(text: &str) {
    let root = root("scripts");
    let image = "schema-version 1\nimage {\n    name \"Fuzz\"\n    base \"example.invalid/fuzz:1\" { family \"fedora\" }\n    modules { module \"fuzz\" }\n}\n";
    let module = "schema-version 1\ndescription \"Fuzz\"\nsupports \"fedora\"\n";
    let repo = "schema-version 1\nname \"Fuzz\"\nsecurity-policy {\n    network {\n        scripts \"deny\"\n    }\n}\n";
    write(&root.path().join("repo.kdl"), repo);
    write(&root.path().join("image.kdl"), image);
    write(&root.path().join("modules/fuzz/module.kdl"), module);
    write(&root.path().join("modules/fuzz/module.sh"), text);
    let _ = crate::load(root.path());
}

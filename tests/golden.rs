//! Every command, over this repository and over the fixture repositories,
//! compared byte for byte against a committed golden.
//!
//! Regenerate with `UPDATE_GOLDEN=1 cargo test`, then read the diff.

use std::path::{Path, PathBuf};
use tect::Command;

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
    for (command, file) in [
        (Command::Plan, "plan.json"),
        (Command::Section, "section.txt"),
        (Command::Summary, "summary.md"),
        (Command::Sbom, "sbom.json"),
    ] {
        compare(name, file, &tect::run(command, None, here).stdout);
    }
    compare(
        name,
        "issues.txt",
        &tect::run(Command::Check, None, here).issues.plain(),
    );

    // `generate` writes nothing here: what it produced is on the run.
    let mut generated = String::new();
    for (path, body) in &tect::run(Command::Generate, None, here).files {
        generated.push_str(&format!("==== {}\n{body}", path.display()));
    }
    compare(name, "generated.txt", &generated);
}

/// A repository `create repo` wrote, from flags alone, captured like any other
/// fixture: what it scaffolds has to resolve, generate and report nothing.
fn init_repo(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::env::set_var("TECT_ASSETS", crate_dir().join("assets"));
    // `git init` runs here, and reads nothing this machine configured.
    std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
    std::env::set_var("GIT_CONFIG_SYSTEM", "/dev/null");
    tect::create::Repo::collect(
        Some("Example".into()),
        None,
        Some("someone".into()),
        Some("Example".into()),
        None,
        Some(root.clone()),
        &tect::prompt::Prompt::silent(),
    )
    .unwrap()
    .apply()
    .unwrap();
    root
}

/// `verify` both ways: silent once the generated tree is what the tool emits,
/// and naming what moved once it is not.
fn verify(name: &str, root: &Path) {
    std::env::set_current_dir(root).expect("fixture root exists");
    let here = Path::new(".");
    let run = tect::run(Command::Generate, None, here);
    for (path, body) in &run.files {
        std::fs::create_dir_all(path.parent().expect("a file under generated/")).unwrap();
        std::fs::write(path, body).unwrap();
    }
    let issues = || tect::run(Command::Verify, None, here).issues.plain();

    let mut out = format!("==== as generated\n{}", issues());

    let containerfile = &run.files[0].0;
    let edited = std::fs::read_to_string(containerfile)
        .unwrap()
        .replace("ARG IMAGE_VERSION=dev", "ARG IMAGE_VERSION=by-hand");
    std::fs::write(containerfile, edited).unwrap();
    out.push_str(&format!("==== edited by hand\n{}", issues()));

    std::fs::remove_file(containerfile).unwrap();
    std::fs::write("generated/leftover", "").unwrap();
    out.push_str(&format!("==== gone, and one nobody claims\n{}", issues()));

    compare(name, "verify.txt", &out);
}

/// `create image` and `create module`, from flags alone: the URL a second image
/// takes from the repository, what the splice does to both image files a module
/// is listed in, and that the result checks.
fn create(name: &str, root: &Path) {
    std::env::set_current_dir(root).expect("the created-into repository exists");
    let here = Path::new(".");
    let silent = tect::prompt::Prompt::silent();

    tect::create::Image::collect(
        here,
        Some("Server".into()),
        None,
        "example",
        None,
        "a name argument",
        &silent,
    )
    .unwrap()
    .apply(here)
    .unwrap();
    tect::create::Module::collect(
        here,
        Some("My Editor".into()),
        vec!["nano".into()],
        vec![("provides".into(), "editor".into())],
        vec!["example".into(), "server".into()],
        &silent,
    )
    .unwrap()
    .apply()
    .unwrap();
    tect::create::Module::collect(
        here,
        Some("plain".into()),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &silent,
    )
    .unwrap()
    .apply()
    .unwrap();

    let mut out = String::new();
    for file in [
        "modules/my-editor/module.kdl",
        "modules/plain/module.kdl",
        "example.kdl",
        "server.kdl",
    ] {
        out.push_str(&format!(
            "==== {file}\n{}",
            std::fs::read_to_string(file).unwrap()
        ));
    }
    out.push_str(&format!(
        "==== check\n{}",
        tect::run(Command::Check, None, here).issues.plain()
    ));
    compare(name, "create.txt", &out);
}

/// `module import`, against two collections on this machine: what one name
/// resolves to, what a name both of them carry does, and that the tree it wrote
/// checks like any other module.
fn import(name: &str, root: &Path) {
    let mut out = String::new();
    let collections = crate_dir().join("tests/collections");
    std::fs::write(
        root.join("repo.kdl"),
        format!(
            "schema-version 1\n\nsources {{\n    one {:?}\n    two {:?}\n}}\n",
            collections.join("one").display(),
            collections.join("two").display()
        ),
    )
    .unwrap();
    std::env::set_current_dir(root).expect("the imported-into repository exists");
    let here = Path::new(".");

    let (list, issues, _) = tect::declarations(here);
    let sources = list.sources;
    out.push_str(&format!("==== the registry\n{}", issues.plain()));
    for collection in &sources {
        out.push_str(&format!("{}\n", collection.name));
    }

    out.push_str("==== the catalog\n");
    for module in tect::import::catalog(here, &sources).unwrap() {
        out.push_str(&format!("{}  {}\n", module.qualified, module.about()));
    }

    for wanted in [
        "flatpak",
        "browser",
        "one/browser",
        "nosuch",
        "one/nosuch",
        "flatpak",
    ] {
        out.push_str(&format!("==== import {wanted}\n"));
        let module = tect::import::split(wanted).1;
        match tect::import::find(here, &sources, wanted) {
            Err(message) => out.push_str(&format!("{message}\n")),
            Ok(found) if found.len() > 1 => {
                let owners: Vec<&str> = found.iter().map(|f| f.owner.as_str()).collect();
                out.push_str(&format!("ambiguous: {}\n", owners.join(", ")));
            }
            Ok(found) => match tect::import::destination(here, &found[0], module)
                .and_then(|dest| tect::import::vendor(here, &found[0], &dest).map(|()| dest))
            {
                Ok(dest) => out.push_str(&format!("imported {}\n", dest.display())),
                Err(message) => out.push_str(&format!("{message}\n")),
            },
        }
    }

    out.push_str("==== the tree\n");
    let mut written: Vec<String> = walk(Path::new("modules"))
        .iter()
        .map(|p| format!("{}\n", p.display()))
        .collect();
    written.sort();
    out.extend(written);

    out.push_str(&format!(
        "==== check\n{}",
        tect::run(Command::Check, None, here).issues.plain()
    ));
    compare(name, "import.txt", &out);
}

/// Every file under `dir`, which is what an import wrote.
fn walk(dir: &Path) -> Vec<PathBuf> {
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

/// Which commands the missing default belongs to: the ones that had to pick an
/// image with nothing naming one, and no others.
fn no_default(root: &Path) {
    std::env::set_current_dir(root).expect("fixture root exists");
    let here = Path::new(".");
    for quiet in [Command::Check, Command::Generate] {
        let issues = tect::run(quiet, None, here).issues.plain();
        assert!(issues.is_empty(), "{quiet:?} reported {issues}");
    }
    for quiet in [Command::Section, Command::Graph] {
        let issues = tect::run(quiet, Some("server"), here).issues.plain();
        assert!(issues.is_empty(), "{quiet:?} reported {issues}");
    }
    for loud in [
        Command::Plan,
        Command::Section,
        Command::Graph,
        Command::Summary,
        Command::Sbom,
    ] {
        let issues = tect::run(loud, None, here).issues.plain();
        assert!(
            issues.contains("2 images are declared and none is the default"),
            "{loud:?} reported {issues}"
        );
    }
}

/// What a flow is allowed to find: `git`, which `create repo` runs, and
/// whichever `gh` the branch under test wants. Nothing else on this machine is
/// on it, so nothing a flow offers to exec reaches the network.
fn bin(name: &str, gh: Option<&str>) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = tmp().join(format!("{name}-bin"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let git = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|at| Path::new(at).join("git"))
        .find(|at| at.is_file())
        .expect("git on PATH");
    std::os::unix::fs::symlink(git, dir.join("git")).unwrap();
    if let Some(script) = gh {
        let at = dir.join("gh");
        std::fs::write(&at, script).unwrap();
        std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

/// `gh` answering as one of the branches the flow asks about, in place of the
/// one this machine may or may not have.
const SIGNED_OUT: &str = "#!/bin/sh\ntest \"$1\" = auth && exit 1\nexit 0\n";
const SIGNED_IN: &str = "#!/bin/sh\nexit 0\n";

/// Everything a run is allowed to reach: the stub `PATH`, the answers, the
/// assets, and a git that reads none of this machine's configuration.
fn sealed<'a>(
    command: &'a mut std::process::Command,
    path: &Path,
) -> &'a mut std::process::Command {
    command
        .env("PATH", path)
        .env("HOME", tmp())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("TECT_ASSETS", crate_dir().join("assets"))
}

/// One scripted flow: the command run in a temporary repository, both streams
/// merged into the order a person reads them, byte-compared against the
/// transcript the fixture holds beside its answers.
fn flow(name: &str, dir: &Path, gh: Option<&str>, args: &[&str]) {
    let fixture = crate_dir().join("tests/golden").join(name);
    let log = tmp().join(format!("{name}.log"));
    let file = std::fs::File::create(&log).unwrap();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_tect"));
    let status = sealed(&mut command, &bin(name, gh))
        .args(args)
        .current_dir(dir)
        .env("TECT_ANSWERS", fixture.join("answers.txt"))
        .stdout(std::process::Stdio::from(file.try_clone().unwrap()))
        .stderr(std::process::Stdio::from(file))
        .status()
        .unwrap();
    let transcript = format!(
        "{}==== exit {}\n",
        std::fs::read_to_string(&log).unwrap(),
        status.code().unwrap_or_default()
    );
    compare(name, "transcript.txt", &transcript);
}

fn tmp() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
}

/// An empty directory, and the repository a flow runs in, both written by the
/// tool itself from flags alone.
fn tect(dir: &Path, args: &[&str]) {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_tect"));
    let out = sealed(&mut command, &bin("tect", None))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn empty(name: &str) -> PathBuf {
    let dir = tmp().join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A repository written into a directory named after itself, which is what a
/// `create repo` with no `--root` produces and what the name a flow defaults to
/// is read off.
fn flow_repo(name: &str) -> PathBuf {
    let dir = empty(name);
    tect(
        &dir,
        &[
            "--no-tui", "create", "repo", "Example", "--owner", "someone", "--image", "Example",
        ],
    );
    dir.join("example")
}

/// The same, with a second image to list a module in.
fn flow_repo_two(name: &str) -> PathBuf {
    let root = flow_repo(name);
    tect(
        &root,
        &["--no-tui", "--root", ".", "create", "image", "Server"],
    );
    root
}

/// The same, with the two fixture collections declared, which is what a search
/// for the module declaring something reads.
fn flow_repo_sourced(name: &str) -> PathBuf {
    let root = flow_repo(name);
    let collections = crate_dir().join("tests/collections");
    // In place of the scaffolded collection: one registry, and these are the
    // two whose contents the flows are written against.
    let mut repo = std::fs::read_to_string(root.join("repo.kdl"))
        .unwrap()
        .replace(tect::init::SOURCES, "");
    repo.push_str(&format!(
        "sources {{\n    one {:?}\n    two {:?}\n}}\n",
        collections.join("one").display(),
        collections.join("two").display()
    ));
    std::fs::write(root.join("repo.kdl"), repo).unwrap();
    root
}

/// A module declaring a key, which is what `create key` reads everything but
/// the kind out of.
const KEYHOLDER: &str = "description \"Signs the modules it builds\"\n\n\
     supports \"fedora\"\n\n\
     key \"secureboot\" {\n\
     \x20   generator \"openssl\" profile=\"module-signing\" bits=4096\n\
     \x20   public \"/usr/share/secureboot/sb_cert.der\" format=\"der\"\n\
     \x20   private \"MOK.priv\"\n\
     }\n";

/// Every flow that prompts, answered from a script: what a person sees, and
/// that a question the script does not answer fails rather than waits.
#[test]
fn flows() {
    let repo = ["create", "repo"];
    for (name, gh) in [
        ("flow-create-repo", None),
        ("flow-create-repo-no-gh", None),
        ("flow-create-repo-signed-out", Some(SIGNED_OUT)),
        ("flow-create-repo-signed-in", Some(SIGNED_IN)),
        ("flow-create-repo-forgejo", None),
        ("flow-image-default", None),
    ] {
        flow(name, &empty(&format!("{name}-in")), gh, &repo);
    }

    // Sourced, so the picker offers what the collection describes as well as
    // what the tool ships with.
    flow(
        "flow-create-image",
        &flow_repo_sourced("flow-image"),
        None,
        &["--root", ".", "create", "image"],
    );
    flow(
        "flow-check-shadow",
        &flow_repo_sourced("flow-shadow"),
        None,
        &["--root", ".", "check"],
    );

    let root = flow_repo("flow-module");
    let module = ["--root", ".", "create", "module"];
    flow("flow-create-module", &root, None, &module);
    flow(
        "flow-module-taken",
        &root,
        None,
        &[&module[..], &["My Editor"]].concat(),
    );
    flow("flow-unanswered", &root, None, &module);
    flow(
        "flow-module-two-images",
        &flow_repo_two("flow-module-both"),
        None,
        &module,
    );

    flow(
        "flow-import-module",
        &flow_repo_sourced("flow-import"),
        None,
        &["--root", ".", "import", "module"],
    );

    // Neither branch reaches a generator, so neither needs one installed.
    flow(
        "flow-key-absent",
        &flow_repo_sourced("flow-key-none"),
        None,
        &["--root", ".", "create", "key", "cosign"],
    );

    let root = flow_repo("flow-key");
    std::fs::create_dir_all(root.join("modules/signed-kernel")).unwrap();
    std::fs::write(root.join("modules/signed-kernel/module.kdl"), KEYHOLDER).unwrap();
    flow(
        "flow-key-kinds",
        &root,
        None,
        &["--root", ".", "create", "key"],
    );
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

    let init = init_repo("init");
    for name in names {
        capture(&name, &dir.join(&name));
    }
    no_default(&dir.join("no-default"));
    capture("init", &init);
    verify("init", &init);
    create("create", &init_repo("create"));
    import("import", &init_repo("import"));
}

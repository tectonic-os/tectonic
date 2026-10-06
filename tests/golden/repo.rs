//! The repository fixtures: what each one renders and what it refuses.

use crate::harness::{assert_golden, copy, crate_dir, given, walk};

use std::path::{Path, PathBuf};

use tect::Command;

/// The plan, the generated section and every diagnostic, for one repository.
fn capture(name: &str, root: &Path) {
    let here = given(root);
    for (command, file) in [
        (Command::Plan, "plan.json"),
        (Command::Section, "section.txt"),
        (Command::Summary, "summary.md"),
        (Command::Sbom, "sbom.json"),
    ] {
        assert_golden(name, file, &tect::run(command, None, &here).stdout);
    }
    assert_golden(
        name,
        "issues.txt",
        &tect::run(Command::Check, None, &here).issues.plain(),
    );

    // `generate` writes nothing here: what it produced is on the run.
    let mut generated = String::new();
    for (path, body) in &tect::run(Command::Generate, None, &here).files {
        // plan.json has a golden of its own; compiled assets need one path copy,
        // not their identical body repeated for every fixture.
        let covered = path == std::path::Path::new("generated/plan.json")
            || path.starts_with("generated/lib")
            || path.starts_with("scripts/")
            || path.starts_with(tect::layout::WORKFLOW_DIR);
        match covered {
            true => generated.push_str(&format!("==== {}\n", path.display())),
            false => generated.push_str(&format!("==== {}\n{body}", path.display())),
        }
    }
    assert_golden(name, "generated.txt", &generated);
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
        &common::prompt::Prompt::silent(),
    )
    .unwrap()
    .expect("a silent run draws no review screen")
    .apply()
    .unwrap();
    root
}

/// `verify` both ways: silent once the generated tree is what the tool emits,
/// and naming what moved once it is not.
fn verify(name: &str, root: &Path) {
    let here = given(root);
    let run = tect::run(Command::Generate, None, &here);
    for (path, body) in &run.files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a file under generated/")).unwrap();
        std::fs::write(path, body).unwrap();
    }
    let issues = || tect::run(Command::Verify, None, &here).issues.plain();

    let mut out = format!("==== as generated\n{}", issues());

    let manifest = root.join("generated/plan.json");
    let edited = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("\"schema_version\": 1", "\"schema_version\": 0");
    std::fs::write(&manifest, edited).unwrap();
    out.push_str(&format!("==== edited by hand\n{}", issues()));

    std::fs::remove_file(&manifest).unwrap();
    std::fs::write(root.join("generated/leftover"), "").unwrap();
    out.push_str(&format!("==== gone, and one nobody claims\n{}", issues()));

    // A workflow the declaration stops naming is the other half of nothing
    // generates this: reported first, then taken away by the next generate.
    let dropped = root.join(".github/workflows/smoke-test.yml");
    let repo = root.join("repo.kdl");
    let text = std::fs::read_to_string(&repo).unwrap();
    std::fs::write(&repo, text.replace("    smoke-test\n", "")).unwrap();
    out.push_str(&format!("==== one workflow undeclared\n{}", issues()));

    tect::write_generated(&here, &tect::run(Command::Generate, None, &here).files).unwrap();
    out.push_str(&format!(
        "==== and generated again\n{}smoke-test.yml is {}\n",
        issues(),
        match dropped.exists() {
            true => "still there",
            false => "gone",
        }
    ));

    // What a repository that has never generated looks like: everything at
    // once. One diagnostic naming `tect generate`, not one per file.
    std::fs::remove_dir_all(root.join("generated")).unwrap();
    out.push_str(&format!("==== never generated\n{}", issues()));

    assert_golden(name, "verify.txt", &out);
}

/// `create image` and `create module`, from flags alone: the URL a second image
/// takes from the repository, what the splice does to both image files a module
/// is listed in, and that the result checks.
fn create_into(name: &str, root: &Path) {
    let here = given(root);
    let silent = common::prompt::Prompt::silent();

    tect::create::Image::collect(
        &here,
        Some("Server".into()),
        None,
        "example",
        None,
        "a name argument",
        tect::create::Field::Image,
        None,
        &silent,
    )
    .unwrap()
    .apply(&here)
    .unwrap();
    tect::create::Module::collect(
        &here,
        Some("My Editor".into()),
        vec!["nano".into()],
        vec![("provides".into(), "editor".into())],
        vec!["example".into(), "server".into()],
        &silent,
    )
    .unwrap()
    .apply(&here)
    .unwrap();
    tect::create::Module::collect(
        &here,
        Some("plain".into()),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &silent,
    )
    .unwrap()
    .apply(&here)
    .unwrap();

    let repo = root.join("repo.kdl");
    let collections = crate_dir().join("tests/collections");
    let text = std::fs::read_to_string(&repo).unwrap().replace(
        "sources {\n",
        &format!(
            "sources {{\n    upstream {:?}\n",
            collections.join("upstream").display()
        ),
    );
    std::fs::write(&repo, text).unwrap();
    let (list, issues, _) = tect::declarations(&here);
    assert!(issues.is_empty(), "{}", issues.plain());
    tect::import::Module::collect(
        Some("upstream/fedora-family".into()),
        &here,
        &list.sources,
        false,
        vec!["example".into(), "server".into()],
        None,
        tect::import::Place::Reference,
        &silent,
    )
    .unwrap_or_else(|err| panic!("{}", err.message()))
    .apply(&here, &list.sources)
    .unwrap();

    let mut out = String::new();
    for file in [
        "modules/my-editor/module.kdl",
        "modules/plain/module.kdl",
        "example.image.kdl",
        "server.image.kdl",
    ] {
        out.push_str(&format!(
            "==== {file}\n{}",
            std::fs::read_to_string(root.join(file)).unwrap()
        ));
    }
    out.push_str(&format!(
        "==== check\n{}",
        tect::run(Command::Check, None, &here).issues.plain()
    ));
    assert_golden(name, "create.txt", &out);
}

/// `copy module`, against two collections on this machine: what one name
/// resolves to, what a name both of them carry does, and that the tree it wrote
/// checks like any other module.
fn copied(name: &str, root: &Path) {
    let mut out = String::new();
    let collections = crate_dir().join("tests/collections");
    std::fs::write(
        root.join("repo.kdl"),
        format!(
            "schema-version 1\nname \"Imported\"\nsources {{\n    upstream {:?}\n    community {:?}\n}}\n",
            collections.join("upstream").display(),
            collections.join("community").display()
        ),
    )
    .unwrap();
    let here = given(root);

    let (list, issues, _) = tect::declarations(&here);
    let sources = list.sources;
    out.push_str(&format!("==== the registry\n{}", issues.plain()));
    for collection in &sources {
        out.push_str(&format!("{}\n", collection.name));
    }

    out.push_str("==== the catalog\n");
    for module in tect::import::catalog(&here, &sources, true).unwrap() {
        out.push_str(&format!("{}  {}\n", module.qualified(), module.about()));
    }

    for wanted in [
        "flatpak",
        "browser",
        "upstream/browser",
        "community/browser",
        "nosuch",
        "upstream/nosuch",
        "flatpak",
    ] {
        out.push_str(&format!("==== copy {wanted}\n"));
        let module = tect::import::split(wanted).1;
        match tect::import::find(&here, &sources, wanted, false) {
            Err(message) => out.push_str(&format!("{message}\n")),
            Ok(found) if found.len() > 1 => {
                let owners: Vec<&str> = found.iter().map(|f| f.owner.as_str()).collect();
                out.push_str(&format!("ambiguous: {}\n", owners.join(", ")));
            }
            Ok(found) => {
                match tect::import::destination(&here, &found[0], module).and_then(|dest| {
                    tect::import::vendor(&here, &sources, &found[0], &dest).map(|_| dest)
                }) {
                    Ok(dest) => out.push_str(&format!("copied {}\n", dest.display())),
                    Err(message) => out.push_str(&format!("{message}\n")),
                }
            }
        }
    }

    out.push_str("==== the tree\n");
    let mut written: Vec<String> = walk(&root.join("modules"))
        .iter()
        .map(|p| format!("{}\n", p.strip_prefix(root).unwrap().display()))
        .collect();
    written.sort();
    out.extend(written);

    out.push_str(&format!(
        "==== the record\n{}",
        std::fs::read_to_string(root.join("modules/flatpak/provenance.kdl")).unwrap()
    ));
    out.push_str(&format!(
        "==== check\n{}",
        tect::run(Command::Check, None, &here).issues.plain()
    ));
    out.push_str(&format!(
        "==== modified\n{:?}\n",
        tect::provenance::record::modified(&here)
    ));

    // Forking an imported module is legitimate, so the edit is reported. A
    // diagnostic would call it a fault.
    std::fs::write(root.join("modules/flatpak/module.sh"), "echo forked\n").unwrap();
    out.push_str(&format!(
        "==== modified after an edit\n{:?}\n{}",
        tect::provenance::record::modified(&here),
        tect::run(Command::Check, None, &here).issues.plain()
    ));
    assert_golden(name, "import.txt", &out);
}

/// A module edited without regenerating is a `verify` failure, the per-module
/// content hash being one of the facts `generated/plan.json` carries.
fn edited_module(root: &Path) {
    let here = given(root);
    for (path, body) in &tect::run(Command::Generate, None, &here).files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a file under generated/")).unwrap();
        std::fs::write(path, body).unwrap();
    }
    let issues = || tect::run(Command::Verify, None, &here).issues.plain();
    let clean = issues();
    assert!(
        clean.is_empty(),
        "verify was not green to begin with: {clean}"
    );

    std::fs::write(root.join("modules/my-editor/module.sh"), "echo edited\n").unwrap();
    let dirty = issues();
    assert!(
        dirty.contains("generated/plan.json"),
        "an edited module left plan.json current: {dirty}"
    );
}

/// `why`, both renderings and both readings. The repository answer comes off
/// the resolved plan; the host answer comes off the two documents a built image
/// carries, with no repo.kdl anywhere. One renderer, so the same module has to
/// come out the same way.
fn why(name: &str, root: &Path, module: &str) {
    let here = given(root);
    let mut out = String::new();

    for (command, heading) in [(Command::Why, "markdown"), (Command::WhyJson, "json")] {
        out.push_str(&format!("==== {heading}\n"));
        out.push_str(&tect::run(command, module.rsplit('/').next(), &here).stdout);
    }

    out.push_str("==== unknown\n");
    out.push_str(
        &tect::run(Command::Why, Some("nosuch"), &here)
            .issues
            .plain(),
    );

    // The same answer with no repository at all, off what a build bakes.
    let manifest = common::json::Json::parse(&tect::run(Command::Plan, None, &here).stdout)
        .expect("the plan is a document");
    let host = tect::emit::why::on_host(&manifest, None, module).expect("the manifest names it");
    out.push_str("==== from the baked manifest, with no repository\n");
    out.push_str(&host.markdown());
    out.push_str(&format!(
        "==== the names it knows\n{}\n",
        tect::emit::why::display(&tect::emit::why::known_on_host(&manifest, None)).join(", ")
    ));

    // The build record is what was observed. Two documents out of one build
    // cannot disagree, so a disagreement is worth saying out loud.
    let record = common::json::Json::parse(&format!(
        "{{\"modules\": [{{\"path\": {module:?}, \"content\": \"not what was declared\"}}]}}"
    ))
    .expect("the record is a document");
    let observed =
        tect::emit::why::on_host(&manifest, Some(&record), module).expect("the manifest names it");
    out.push_str("==== against a build record that disagrees\n");
    out.push_str(
        observed
            .markdown()
            .split("## Where it came from")
            .nth(1)
            .unwrap_or_default()
            .split("## What it pulls in")
            .next()
            .unwrap_or_default(),
    );

    assert_golden(name, "why.txt", &out);
}

/// `summary`, both readings. The repository's comes off the resolved plan and
/// the host's off the manifest that plan bakes into the image, so for one
/// target the two have to be the same document. Every fixture target is walked.
fn summary_on_host(root: &Path) {
    let here = given(root);
    let manifest = common::json::Json::parse(&tect::run(Command::Plan, None, &here).stdout)
        .expect("the plan is a document");

    let (every, _) = tect::emit::why::built_as(&manifest, None);
    assert!(!every.is_empty(), "{} publishes nothing", root.display());
    for target in every {
        let record = common::json::Json::parse(&format!(
            "{{\"target\": {:?}}}",
            common::json::text(target, "name").expect("a target is named")
        ))
        .expect("the record is a document");
        let (scoped, _) = tect::emit::why::built_as(&manifest, Some(&record));
        let [scoped] = scoped.as_slice() else {
            panic!("the record names exactly one target");
        };
        assert_eq!(
            tect::run(
                Command::Summary,
                common::json::text(target, "name").as_deref(),
                &here
            )
            .stdout,
            tect::emit::summary::on_host(scoped),
            "{} disagrees with itself",
            root.display()
        );
    }
}

/// The two names `why` resolves but cannot read out of the plan: a module the
/// base suppresses, which is listed and never built, and one whose manifest
/// never loaded.
fn why_unbuilt(root: &Path) {
    let temp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("why-unbuilt");
    let _ = std::fs::remove_dir_all(&temp);
    copy(root, &temp);
    let here = given(&temp);
    let mut out = String::new();

    // Suppressed: everything it provides, the base ships, so no layer builds
    // it. `why` still answers, and says which of the two it is.
    out.push_str("==== a module the base suppresses\n");
    let bare = tect::run(Command::Why, Some("flatpak"), &here).stdout;
    out.push_str(&bare);

    // A full path names it too, and names it the same. Worth an assertion; a
    // second copy of the read-out only repeats it.
    let full = tect::run(Command::Why, Some("apps/flatpak"), &here).stdout;
    assert_eq!(bare, full, "the full path is a name like any other");

    // Suppressed by one image is not suppressed by the other. A second image on
    // a base that ships nothing it provides builds it, and the read-out has to
    // say both things. Stopping at the first it finds loses the other.
    std::fs::write(
        temp.join("also.image.kdl"),
        "image {\n    name \"Also\"\n\n    base \"ghcr.io/ublue-os/bazzite:stable\" {\n        \
         family \"fedora\"\n    }\n\n    modules {\n        module \"apps/flatpak\"\n    }\n}\n",
    )
    .unwrap();
    out.push_str("==== and the same module in an image that does build it\n");
    out.push_str(
        &tect::run(Command::Why, Some("flatpak"), &here)
            .stdout
            .split("## What it exchanges")
            .next()
            .unwrap_or_default(),
    );
    std::fs::remove_file(temp.join("also.image.kdl")).unwrap();

    // Listed, but its manifest was deleted out from under the image: there is
    // nothing to read out, and that is a diagnostic. A crash names nothing.
    std::fs::remove_dir_all(temp.join("modules/apps/flatpak")).unwrap();
    out.push_str("==== and one whose manifest never loaded\n");
    let run = tect::run(Command::Why, Some("flatpak"), &here);
    out.push_str(
        run.issues
            .plain()
            .split("`apps/flatpak` is listed")
            .nth(1)
            .map(|rest| format!("`apps/flatpak` is listed{rest}"))
            .expect("the unread module is reported")
            .as_str(),
    );

    assert_golden("suppressed", "why.txt", &out);
}

/// The same repository, both ways. `audit { enforce }` is a lever over a
/// record that always exists, so what it changes is which facts are fatal and
/// nothing about which facts are kept.
fn unenforced(root: &Path) {
    let temp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("unenforced");
    let _ = std::fs::remove_dir_all(&temp);
    copy(root, &temp);
    let repo = temp.join("repo.kdl");
    let text = std::fs::read_to_string(&repo).unwrap();
    let (before, rest) = text.split_once("audit {").expect("the fixture enforces");
    let after = rest.split_once('}').expect("a closed block").1;
    std::fs::write(&repo, format!("{before}{after}")).unwrap();

    let here = given(&temp);
    let issues = tect::run(Command::Check, None, &here).issues.plain();
    assert!(
        issues.is_empty(),
        "the same repository has to check clean unenforced: {issues}"
    );
}

/// An unpinned collection is verified against nothing, so enforcement refuses
/// the import. Refusing at the build is too late: hashing afterwards pins
/// whatever arrived. The refusal lands before anything is fetched.
fn unpinned_import(root: &Path) {
    let here = given(root);
    let (list, _, _) = tect::declarations(&here);
    assert!(
        list.sources.iter().any(|c| c.unpinned()),
        "the fixture has to declare an unpinned collection"
    );
    let refused = tect::import::find(&here, &list.sources, "anything", true)
        .err()
        .expect("enforcement refuses an unpinned collection");
    assert!(refused.contains("follows a moving ref"), "{refused}");
}

/// Which commands the missing default belongs to: the ones that had to pick an
/// image with nothing naming one, and no others.
fn assert_no_default(root: &Path) {
    let here = given(root);
    for quiet in [Command::Check, Command::Generate] {
        let issues = tect::run(quiet, None, &here).issues.plain();
        assert!(issues.is_empty(), "{quiet:?} reported {issues}");
    }
    for quiet in [Command::Section, Command::Graph] {
        let issues = tect::run(quiet, Some("server"), &here).issues.plain();
        assert!(issues.is_empty(), "{quiet:?} reported {issues}");
    }
    for loud in [
        Command::Plan,
        Command::Section,
        Command::Graph,
        Command::Summary,
        Command::Sbom,
    ] {
        let issues = tect::run(loud, None, &here).issues.plain();
        assert!(
            issues.contains("2 images are declared and none is the default"),
            "{loud:?} reported {issues}"
        );
    }
}

fn repo_case(name: &str) {
    let dir = crate_dir().join("tests/repos");
    let root = dir.join(name);
    capture(name, &root);
    match name {
        "collecting" | "multi-image" | "pinned" => summary_on_host(&root),
        "enforced" => {
            unenforced(&root);
            why(name, &root, "one/hello");
        }
        "minimal" => {
            why(name, &root, "core/hello");
            summary_on_host(&root);
        }
        "no-default" => assert_no_default(&root),
        // The repository owns a referenced module's pin, so no provenance record sits beside it.
        "seeding" => why(name, &root, "tectonic-os/flatpak"),
        "suppressed" => {
            why_unbuilt(&root);
            summary_on_host(&root);
        }
        "unpinned-source" => unpinned_import(&root),
        _ => {}
    }
}

macro_rules! repo_cases {
    ($($test:ident => $fixture:literal),+ $(,)?) => {
        const REPO_CASES: &[&str] = &[$($fixture),+];

        $(
            #[test]
            fn $test() {
                repo_case($fixture);
            }
        )+
    };
}

repo_cases! {
    bare_module => "bare-module",
    broken_asset => "broken-asset",
    broken_bases => "broken-bases",
    broken_collect => "broken-collect",
    broken_cycle => "broken-cycle",
    broken_fetch => "broken-fetch",
    broken_flavours => "broken-flavours",
    broken_fragment => "broken-fragment",
    broken_graph => "broken-graph",
    broken_image => "broken-image",
    broken_key => "broken-key",
    broken_layout => "broken-layout",
    broken_module => "broken-module",
    broken_network => "broken-network",
    broken_options => "broken-options",
    broken_overlay => "broken-overlay",
    broken_pin => "broken-pin",
    broken_root => "broken-root",
    broken_seed => "broken-seed",
    broken_sources => "broken-sources",
    broken_syntax => "broken-syntax",
    broken_unpinned => "broken-unpinned",
    collecting => "collecting",
    deb_families => "deb-families",
    enforced => "enforced",
    future_schema => "future-schema",
    media_recipe => "media-recipe",
    minimal => "minimal",
    multi_image => "multi-image",
    network => "network",
    no_default => "no-default",
    no_image => "no-image",
    one_file => "one-file",
    optional_metadata => "optional-metadata",
    past_schema => "past-schema",
    pinned => "pinned",
    refused => "refused",
    seeding => "seeding",
    signing => "signing",
    suppressed => "suppressed",
    uki => "uki",
    unmeasured_family => "unmeasured-family",
    unpinned_source => "unpinned-source",
    wrong_pin => "wrong-pin",
}

#[test]
fn every_repo_fixture_has_a_case() {
    let mut found: Vec<String> = std::fs::read_dir(crate_dir().join("tests/repos"))
        .expect("tests/repos exists")
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    let mut expected = REPO_CASES.to_vec();
    expected.sort();
    assert_eq!(
        found, expected,
        "each repository fixture needs a named test"
    );
}

#[test]
fn init() {
    let init = init_repo("init");
    assert!(
        !init.join("bases.kdl").exists(),
        "bases.kdl is a control asset; it must not land at a scaffolded repository root"
    );
    assert!(
        !init.join("lib").exists(),
        "lib is generated; it must not land in the editable scaffold"
    );
    for script in ["tect.sh", "lint.sh", "vm.sh"] {
        assert!(
            !init.join("scripts").join(script).exists(),
            "{script} is generated; it must not land in the editable scaffold"
        );
    }
    capture("init", &init);
    verify("init", &init);
}

#[test]
fn create() {
    let created = init_repo("create");
    create_into("create", &created);
    edited_module(&created);
}

#[test]
fn copy_module() {
    copied("copy", &init_repo("copy"));
}

/// A fragment's own `RUN` lines are outside the script rule, so `check` names
/// each module that ships one rather than letting `strict` read wider than it
/// is. `core/spliced` has a standard layer *and* a fragment, which is the case
/// the rule closes one half of.
#[test]
fn a_strict_script_rule_names_the_fragments_it_does_not_reach() {
    let root = crate_dir().join("tests/repos/network");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tect"))
        .arg("check")
        .current_dir(&root)
        .env("TECT_ASSETS", crate_dir().join("assets"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("`core/spliced` ships a Containerfile fragment"),
        "{stderr}"
    );
}

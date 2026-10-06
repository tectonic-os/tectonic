//! The interactive flows: each case arranges its own repository with scripted
//! answers, and only its own transcript is compared against a snapshot.

use crate::harness::{assert_golden, contents, crate_dir, tmp};

use std::cell::Cell;

use std::path::{Path, PathBuf};

use tect::Command;

struct Flow {
    target: &'static str,
    /// Holds the number of snapshots this case must assert, so a case that
    /// never reaches its target's step fails instead of asserting nothing.
    expected: usize,
    asserted: Cell<usize>,
}

impl Flow {
    fn new(target: &'static str) -> Self {
        let expected = match target {
            "flow-create-flavour" => 2,
            name if name.starts_with("flow-") => 1,
            _ => 0,
        };
        Self {
            target,
            expected,
            asserted: Cell::new(0),
        }
    }

    /// Fails when the case asserted fewer than the snapshots it owns, which a
    /// case that never reaches its target's step would otherwise hide.
    fn done(&self) {
        assert_eq!(
            self.asserted.get(),
            self.expected,
            "{} must assert each of its snapshots",
            self.target
        );
    }

    /// Returns a scratch path for this test's build-up steps, so two tests
    /// never share one temporary directory.
    fn at(&self, name: &str) -> PathBuf {
        tmp().join(format!("{}--{name}", self.target))
    }

    fn note_assertion(&self) {
        self.asserted.set(self.asserted.get() + 1);
    }

    /// Returns what a flow is allowed to find: `git`, which `create repo` runs,
    /// `sha256sum`, which `copy module` hashes with, and whichever `gh` the case
    /// under test wants. No other tool reaches this path, so nothing a flow
    /// offers to exec reaches the network.
    fn bin(&self, name: &str, gh: Option<&str>) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir = self.at(&format!("{name}-bin"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = std::env::var("PATH").unwrap_or_default();
        for tool in ["git", "sha256sum"] {
            let at = paths
                .split(':')
                .map(|at| Path::new(at).join(tool))
                .find(|at| at.is_file())
                .unwrap_or_else(|| panic!("{tool} on PATH"));
            std::os::unix::fs::symlink(at, dir.join(tool)).unwrap();
        }
        if let Some(script) = gh {
            let at = dir.join("gh");
            std::fs::write(&at, script).unwrap();
            std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    /// Runs one scripted flow in a temporary repository, both streams merged
    /// into the order a person reads them. A question the scripted answers do
    /// not answer leaves the run waiting. The case compares its own run against
    /// its committed transcript; a step that only builds the state asserts
    /// nothing.
    fn run(&self, name: &str, dir: &Path, gh: Option<&str>, args: &[&str]) {
        self.run_with_assets(name, dir, gh, args, &crate_dir().join("assets"));
    }

    fn run_with_assets(
        &self,
        name: &str,
        dir: &Path,
        gh: Option<&str>,
        args: &[&str],
        assets: &Path,
    ) {
        let fixture = crate_dir().join("tests/answers").join(name);
        let log = self.at(&format!("{name}.log"));
        let file = std::fs::File::create(&log).unwrap();
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_tect"));
        let status = sealed(&mut command, &self.bin(name, gh))
            .env("TECT_ASSETS", assets)
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
        if name == self.target {
            self.note_assertion();
            assert_golden(name, "transcript.txt", &transcript);
        }
    }
    /// Runs `create repo` where the scaffolded collection is a pinned archive on
    /// this machine, the one flow that reaches the fetching branch: a fresh
    /// repository has read nothing, so the offer cannot name a module without
    /// downloading the collection first. Every URL the run can reach is a
    /// `file://` pin, so the tools on the path touch nothing off this machine.
    fn offering(&self, name: &str, dir: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let work = self.at(&format!("{name}-src"));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).unwrap();

        // Packs the collection as the archive a pin fetches, hashed the way a pin is.
        let tarball = work.join("upstream.tar.gz");
        let status = std::process::Command::new("tar")
            .args(["czf", &tarball.display().to_string(), "upstream"])
            .current_dir(crate_dir().join("tests/collections"))
            .status()
            .unwrap();
        assert!(status.success(), "tar the fixture collection");
        let out = std::process::Command::new("sha256sum")
            .arg(&tarball)
            .output()
            .unwrap();
        let sha256 = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .unwrap()
            .to_string();

        // Builds an assets tree whose scaffolded `sources` is that archive.
        // Every other file a repository is scaffolded from is the shipped copy.
        let assets = work.join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        let status = std::process::Command::new("cp")
            .arg("-r")
            .arg(crate_dir().join("assets").join("."))
            .arg(&assets)
            .status()
            .unwrap();
        assert!(status.success(), "copy the shipped assets");
        std::fs::write(
            assets.join(tect::init::SOURCES_FILE),
            format!(
                "sources {{\n    upstream {{\n        pin {{\n            version \"1\"\n\
                 \x20           url \"file://{}\"\n            sha256 \"{sha256}\"\n\
                 \x20       }}\n    }}\n}}\n",
                tarball.display()
            ),
        )
        .unwrap();

        // `curl`, `tar` and `gzip` are what unpack a pin. They reach a
        // `file://` URL, and no other URL here names a remote one, so the seal
        // holds.
        let path = self.bin(name, None);
        for tool in ["curl", "tar", "gzip"] {
            let at = std::env::var("PATH")
                .unwrap_or_default()
                .split(':')
                .map(|at| Path::new(at).join(tool))
                .find(|at| at.is_file())
                .unwrap_or_else(|| panic!("{tool} on PATH"));
            std::os::unix::fs::symlink(at, path.join(tool)).unwrap();
        }

        let fixture = crate_dir().join("tests/answers").join(name);
        let log = self.at(&format!("{name}.log"));
        let scratch = work.join("tmp");
        std::fs::create_dir_all(&scratch).unwrap();
        let file = std::fs::File::create(&log).unwrap();
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_tect"));
        let status = command
            .env("PATH", &path)
            .env("HOME", tmp())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("TMPDIR", &scratch)
            .env("TECT_ASSETS", &assets)
            .env("TECT_ANSWERS", fixture.join("answers.txt"))
            .args(["create", "repo"])
            .current_dir(dir)
            .stdout(std::process::Stdio::from(file.try_clone().unwrap()))
            .stderr(std::process::Stdio::from(file))
            .status()
            .unwrap();
        let transcript = format!(
            "{}==== exit {}\n",
            std::fs::read_to_string(&log).unwrap(),
            status.code().unwrap_or_default()
        );
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        if name == self.target {
            self.note_assertion();
            assert_golden(name, "transcript.txt", &transcript);
        }

        // The fetch that answered the offer went to scratch, not into a repository
        // that did not exist yet: a run left at the review screen writes nothing,
        // and this one leaves no cache behind either.
        let root = dir.join("example");
        assert!(!root.join("out").exists(), "the fetch cached into the repo");
        let strays: Vec<PathBuf> = std::fs::read_dir(&scratch)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("tect-bases."))
            })
            .collect();
        assert!(strays.is_empty(), "scratch left behind: {strays:?}");

        let image = std::fs::read_to_string(root.join("example.image.kdl")).unwrap();
        assert!(
            image.contains("source \"upstream\" {\n            module \"fedora-family\"\n"),
            "{image}"
        );
    }
    /// Compares only the tail, because redraw bytes vary with terminal size.
    fn drawn(&self, name: &str, dir: &Path, command: &str, after: &str, steps: &[&[u8]]) {
        let (status, errors, raw) = drawn_run(dir, command, steps);
        assert!(
            status.success(),
            "{errors}{}",
            String::from_utf8_lossy(&raw)
        );

        let text = String::from_utf8_lossy(&raw);
        let stable = text.rsplit_once(after).unwrap().1;
        if name == self.target {
            self.note_assertion();
            assert_golden(
                name,
                "transcript.txt",
                &format!(
                    "{after}{stable}==== exit 0
    "
                ),
            );
        }
    }
    fn tect(&self, dir: &Path, args: &[&str]) {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_tect"));
        let out = sealed(&mut command, &self.bin("tect", None))
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

    fn empty(&self, name: &str) -> PathBuf {
        let dir = self.at(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Returns a repository written into a directory named after itself, which
    /// is what a `create repo` with no `--root` produces and what the name a
    /// flow defaults to is read off.
    fn repo(&self, name: &str) -> PathBuf {
        let dir = self.empty(name);
        self.tect(
            &dir,
            &[
                "--no-tui", "create", "repo", "Example", "--owner", "someone", "--image", "Example",
            ],
        );
        dir.join("example")
    }

    /// Returns the same repository with a second image to list a module in.
    fn repo_two(&self, name: &str) -> PathBuf {
        let root = self.repo(name);
        self.tect(
            &root,
            &["--no-tui", "--root", ".", "create", "image", "Server"],
        );
        root
    }

    /// Returns the same repository with the two fixture collections declared,
    /// which is what a search for the module declaring something reads.
    fn repo_sourced(&self, name: &str) -> PathBuf {
        let root = self.repo(name);
        let collections = crate_dir().join("tests/collections");
        // These two collections replace the scaffolded registry, because the
        // flows are written against their contents.
        let mut repo = std::fs::read_to_string(root.join("repo.kdl"))
            .unwrap()
            .replace(&tect::init::sources(&crate_dir().join("assets")), "");
        repo.push_str(&format!(
            "sources {{\n    upstream {:?}\n    community {:?}\n}}\n",
            collections.join("upstream").display(),
            collections.join("community").display()
        ));
        std::fs::write(root.join("repo.kdl"), repo).unwrap();
        root
    }

    fn repo_with(&self, name: &str, collection: &str) -> PathBuf {
        let root = self.repo(name);
        let mut repo = std::fs::read_to_string(root.join("repo.kdl"))
            .unwrap()
            .replace(&tect::init::sources(&crate_dir().join("assets")), "");
        repo.push_str(&format!(
            "sources {{\n    {collection} {:?}\n}}\n",
            crate_dir()
                .join("tests/collections")
                .join(collection)
                .display()
        ));
        std::fs::write(root.join("repo.kdl"), repo).unwrap();
        root
    }

    /// Returns the same repository with the collection whose one module claims
    /// a benchmark rule, which is what every offer about conformance reads.
    fn repo_claiming(&self, name: &str) -> PathBuf {
        self.repo_with(name, "claims")
    }

    /// Returns the same repository enforcing, which is the other half of what a
    /// `conforms` costs: in an enforcing repository a rule the image fails fails
    /// the build, and both places that say so read the same declaration.
    fn repo_enforcing(&self, name: &str) -> PathBuf {
        let root = self.repo_claiming(name);
        let file = root.join("repo.kdl");
        let mut repo = std::fs::read_to_string(&file).unwrap();
        repo.push_str("\naudit {\n    enforce #true\n}\n");
        std::fs::write(&file, repo).unwrap();
        root
    }

    /// Returns the same repository with a second image, since an import lists a
    /// module in as many images as the answer names and writes a `conforms`
    /// into every one of them.
    fn repo_claiming_two(&self, name: &str) -> PathBuf {
        let root = self.repo_claiming(name);
        self.tect(
            &root,
            &["--no-tui", "--root", ".", "create", "image", "Server"],
        );
        root
    }
}

const SIGNED_OUT: &str = "#!/bin/sh\ntest \"$1\" = auth && exit 1\nexit 0\n";

const SIGNED_IN: &str = "#!/bin/sh\nexit 0\n";

/// Seals a run to everything it may reach: the stub `PATH`, the answers, the
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

/// Drives a real terminal, because scripted answers cannot exercise Escape.
fn drawn_run(
    dir: &Path,
    command: &str,
    steps: &[&[u8]],
) -> (std::process::ExitStatus, String, Vec<u8>) {
    use std::io::{Read, Write};
    use std::process::Stdio;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    let mut child = std::process::Command::new("script")
        .args(["-qfec", command, "/dev/null"])
        .current_dir(dir)
        .env("TECT_ASSETS", crate_dir().join("assets"))
        // A host exporting COLUMNS would leak into the pty and redraw at that
        // width, so the drawn width is pinned the way the golden captured it.
        .env("COLUMNS", "80")
        // The transcript snapshots retain terminal styling, so a shell's
        // NO_COLOR must not reach the pty.
        .env_remove("NO_COLOR")
        // Whether this machine has a TPM decides which encryption rows are
        // present, so it is pinned the same way.
        .env("TECT_TPM", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script from util-linux");
    let input = Arc::new(Mutex::new(child.stdin.take().unwrap()));
    let mut output = child.stdout.take().unwrap();
    let raw = Arc::new(Mutex::new(Vec::new()));
    let reader = {
        let (input, raw) = (input.clone(), raw.clone());
        std::thread::spawn(move || {
            let mut byte = [0];
            while output.read_exact(&mut byte).is_ok() {
                let mut held = raw.lock().unwrap();
                held.push(byte[0]);
                if held.ends_with(b"\x1b[6n") {
                    let mut input = input.lock().unwrap();
                    let _ = input.write_all(b"\x1b[1;1R");
                    let _ = input.flush();
                }
            }
        })
    };
    for keys in steps {
        std::thread::sleep(Duration::from_millis(400));
        let mut input = input.lock().unwrap();
        input.write_all(keys).unwrap();
        input.flush().unwrap();
    }
    let status = child.wait().unwrap();
    reader.join().unwrap();
    let mut errors = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut errors)
        .unwrap();
    let raw = raw.lock().unwrap();
    (status, errors, raw.clone())
}

/// Lists the module a claim is written into, so `check` holds the manifest the
/// picker wrote to the schema.
fn lists_sshd(root: &Path) {
    let file = root.join("example.image.kdl");
    let listed = std::fs::read_to_string(&file).unwrap().replace(
        "    modules {\n    }",
        "    modules {\n        module \"sshd\"\n    }",
    );
    assert!(listed.contains("module \"sshd\""), "{listed}");
    std::fs::write(file, listed).unwrap();
}

const CLAIMANT: &str = "description \"SSH daemon hardening\"\n\nsupports \"fedora\"\n";

const KEYHOLDER: &str = "description \"Signs the modules it builds\"\n\n\
     supports \"fedora\"\n\n\
     key \"secureboot\" {\n\
     \x20   generator \"openssl\" profile=\"module-signing\" bits=4096\n\
     \x20   public \"/usr/share/secureboot/sb_cert.der\" format=\"der\"\n\
     \x20   private \"MOK.priv\"\n\
     }\n";

/// Checks the node paths in a file `tect create` wrote, with each name the
/// author chose, against the nodes the grammar marks as scaffolded. The
/// schema reference opens each file on those nodes, so the two sets have to
/// match or the reference shows a scaffold the tool does not write.
fn scaffolded(area: tect::emit::schema_md::Area, file: &Path) {
    fn walk(nodes: &[kdl::KdlNode], prefix: &str, out: &mut Vec<String>) {
        for node in nodes {
            let path = match prefix.is_empty() {
                true => node.name().value().to_string(),
                false => format!("{prefix}/{}", node.name().value()),
            };
            if let Some(children) = node.children() {
                walk(children.nodes(), &path, out);
            }
            out.push(path);
        }
    }
    fn matches(pattern: &str, path: &str) -> bool {
        let path: Vec<&str> = path.split('/').collect();
        for (at, part) in pattern.split('/').enumerate() {
            if part == "**" {
                return path.len() >= at;
            }
            if path.get(at).is_none_or(|seg| part != "*" && part != *seg) {
                return false;
            }
        }
        pattern.split('/').count() == path.len()
    }
    let text =
        std::fs::read_to_string(file).unwrap_or_else(|err| panic!("{}: {err}", file.display()));
    let doc: kdl::KdlDocument = text.parse().expect("the scaffold is KDL");
    let mut written = Vec::new();
    walk(doc.nodes(), "", &mut written);
    let marked = tect::emit::schema_md::scaffold(area);
    for path in &written {
        assert!(
            marked.iter().any(|pattern| matches(pattern, path)),
            "{}: `tect create` writes `{path}`, which the grammar does not mark as scaffolded",
            file.display()
        );
    }
    for pattern in &marked {
        assert!(
            written.iter().any(|path| matches(pattern, path)),
            "{}: the grammar marks `{pattern}` as scaffolded, and `tect create` does not write it",
            file.display()
        );
    }
}

/// Maps each case to the setup that arranges it. Each case names one function,
/// so no case reaches its repository by falling through another case's steps.
fn flow_case(flow: &Flow) {
    match flow.target {
        "flow-create-repo"
        | "flow-create-repo-no-gh"
        | "flow-create-repo-signed-out"
        | "flow-create-repo-signed-in"
        | "flow-create-repo-forgejo"
        | "flow-image-default" => setup_create_repo(flow),
        "flow-create-repo-offer" => setup_create_repo_offer(flow),
        "flow-create-image" => setup_create_image(flow),
        "flow-check-shadow" => setup_check_shadow(flow),
        "flow-check-unpinned" => setup_check_unpinned(flow),
        "flow-check-conforms" | "flow-check-claims" | "flow-coverage" => setup_conforms(flow),
        "flow-check-conforms-stranger" => setup_check_conforms_stranger(flow),
        "flow-create-module" | "flow-module-taken" | "flow-unanswered" => setup_module(flow),
        "flow-module-two-images" => setup_module_two_images(flow),
        "flow-create-flavour" => setup_create_flavour(flow),
        "flow-module-in-flavour" => setup_module_in_flavour(flow),
        "flow-set-conforms" | "flow-set-conforms-again" => setup_set_conforms(flow),
        "flow-import-conforms" => setup_import_conforms(flow),
        "flow-import-conforms-declined" => setup_import_conforms_declined(flow),
        "flow-set-conforms-enforced" => setup_set_conforms_enforced(flow),
        "flow-import-conforms-enforced" => setup_import_conforms_enforced(flow),
        "flow-import-conforms-two" => setup_import_conforms_two(flow),
        "flow-import-datastream" => setup_import_datastream(flow),
        "flow-import-family" => setup_import_family(flow),
        "flow-copy-conforms" => setup_copy_conforms(flow),
        "flow-set-claims" | "flow-set-claims-again" => setup_set_claims(flow),
        "flow-set-claims-two" => setup_set_claims_two(flow),
        "flow-set-claims-fetched" => setup_set_claims_fetched(flow),
        "flow-set-claims-drawn" => setup_set_claims_drawn(flow),
        "flow-set-workflows" | "flow-set-workflows-drawn" => setup_workflows(flow),
        "flow-import-requires" => setup_import_requires(flow),
        "flow-check-unmet" => setup_check_unmet(flow),
        "flow-import-skip" => setup_import_skip(flow),
        "flow-check-unfetched" => setup_check_unfetched(flow),
        "flow-why-picker" => setup_why_picker(flow),
        "flow-import-kernel" => setup_import_kernel(flow),
        "flow-import-module" => setup_import_module(flow),
        "flow-import-default"
        | "flow-copy-with-schema-issues"
        | "flow-create-module-with-schema-issues" => setup_import_default(flow),
        "import-edit-guards" => setup_import_edit_guards(flow),
        "flow-import-several" => setup_import_several(flow),
        "flow-import-nested" => setup_import_nested(flow),
        "flow-import-suffix" => setup_import_suffix(flow),
        "flow-import-suffix-ambiguous" => setup_import_suffix_ambiguous(flow),
        "flow-import-collides" | "flow-check-collides" => setup_collides(flow),
        "flow-copy-nested" => setup_copy_nested(flow),
        "flow-copy-collides" => setup_copy_collides(flow),
        "flow-copy-module" => setup_copy_module(flow),
        "flow-key-absent" => setup_key_absent(flow),
        "flow-key-kinds" => setup_key_kinds(flow),
        "flow-key-undeclared" => setup_key_undeclared(flow),
        "flow-key-no-kind" => setup_key_no_kind(flow),
        target => panic!("no flow case for {target}"),
    }
}

fn datastream() -> String {
    crate_dir()
        .join("tests/scap/datastream.xml")
        .display()
        .to_string()
}

fn import_sshd(stream: &str) -> [&str; 7] {
    [
        "--root",
        ".",
        "import",
        "module",
        "claims/sshd",
        "--datastream",
        stream,
    ]
}

fn claims(stream: &str) -> [&str; 7] {
    [
        "--root",
        ".",
        "set",
        "claims",
        "sshd",
        "--datastream",
        stream,
    ]
}

fn setup_create_repo(flow: &Flow) {
    let target = flow.target;
    let gh = match target {
        "flow-create-repo-signed-out" => Some(SIGNED_OUT),
        "flow-create-repo-signed-in" => Some(SIGNED_IN),
        _ => None,
    };
    flow.run(
        target,
        &flow.empty(&format!("{target}-in")),
        gh,
        &["create", "repo"],
    );
    if target == "flow-create-repo" {
        let created = flow.at("flow-create-repo-in").join("example");
        scaffolded(tect::emit::schema_md::Area::Repo, &created.join("repo.kdl"));
        scaffolded(
            tect::emit::schema_md::Area::Image,
            &created.join("desktop.image.kdl"),
        );
    }
}

/// Runs the offer `create repo` makes, against a collection it has to fetch
/// before it can name anything in it.
fn setup_create_repo_offer(flow: &Flow) {
    let target = flow.target;
    flow.offering(target, &flow.empty("flow-create-repo-offer-in"));
}

/// Runs against a sourced repository, so the picker offers what the collection
/// describes as well as what the tool ships with. The scaffolded image opens
/// with whatever fills the family-adapter role, which is what a fresh
/// repository could otherwise not resolve without.
fn setup_create_image(flow: &Flow) {
    let target = flow.target;
    let root = flow.repo_sourced("flow-image");
    flow.run(target, &root, None, &["--root", ".", "create", "image"]);
    let image = std::fs::read_to_string(root.join("beta.image.kdl")).unwrap();
    assert!(
            image.contains(
                "    modules {\n        source \"upstream\" {\n            module \"fedora-family\"\n        }\n    }"
            ),
            "{image}"
        );
    // The next fetch resolves the reference the run wrote, and nothing else is wanted.
    flow.tect(&root, &["--no-tui", "--root", ".", "fetch", "modules"]);
    flow.tect(&root, &["--no-tui", "--root", ".", "check"]);
}

fn setup_check_shadow(flow: &Flow) {
    let target = flow.target;
    flow.run(
        target,
        &flow.repo_sourced("flow-shadow"),
        None,
        &["--root", ".", "check"],
    );
}

/// Runs unsourced, so the collection is the one `create repo` scaffolds.
fn setup_check_unpinned(flow: &Flow) {
    let target = flow.target;
    flow.run(
        target,
        &flow.repo("flow-unpinned"),
        None,
        &["--root", ".", "check"],
    );
}

/// Measures an image against a profile, with a module on disk claiming a rule
/// of it that the image does not list. Without a datastream `check` can only
/// count declarations; with one it says which rules are open and what would
/// close them, and under-claims for the collection nothing read.
fn setup_conforms(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let conforms = flow.repo("flow-conforms");
    std::fs::create_dir_all(conforms.join("modules/hardening")).unwrap();
    std::fs::write(
        conforms.join("modules/hardening/module.kdl"),
        "description \"Claims a rule the profile selects\"\n\nsupports \"fedora\"\n\n\
             satisfies {\n    cis-fedora \"5.2.20\"\n}\n",
    )
    .unwrap();
    let image = conforms.join("example.image.kdl");
    let declared = std::fs::read_to_string(&image).unwrap().replace(
        "    modules {",
        "    conforms \"standard\"\n\n    modules {",
    );
    assert!(declared.contains("conforms \"standard\""), "{declared}");
    std::fs::write(&image, declared).unwrap();
    flow.run(
        "flow-check-conforms",
        &conforms,
        None,
        &["--root", ".", "check"],
    );
    if target == "flow-check-conforms" {
        return;
    }
    flow.run(
        "flow-check-claims",
        &conforms,
        None,
        &["--root", ".", "check", "--datastream", &stream],
    );
    if target == "flow-check-claims" {
        return;
    }
    // Reads the same repository out rule by rule. The run is scripted, so the
    // markdown is what a redirect gets and no terminal rendering is in the way.
    flow.run(
        "flow-coverage",
        &conforms,
        None,
        &["--root", ".", "coverage", "--datastream", &stream],
    );
}

/// Runs `check` where a `conforms` names a profile the datastream does not
/// carry. A typo reaches this, and the notice has to say what the content does
/// hold; reporting nothing found leaves the typo invisible.
fn setup_check_conforms_stranger(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let stranger = flow.repo("flow-conforms-stranger");
    let image = stranger.join("example.image.kdl");
    let declared = std::fs::read_to_string(&image).unwrap().replace(
        "    modules {",
        "    conforms \"cusp_fedora\"\n\n    modules {",
    );
    std::fs::write(&image, declared).unwrap();
    flow.run(
        target,
        &stranger,
        None,
        &["--root", ".", "check", "--datastream", &stream],
    );
}

fn setup_module(flow: &Flow) {
    let target = flow.target;
    let root = flow.repo("flow-module");
    let module = ["--root", ".", "create", "module"];
    flow.run("flow-create-module", &root, None, &module);
    scaffolded(
        tect::emit::schema_md::Area::Module,
        &root.join("modules/my-editor/module.kdl"),
    );
    if target == "flow-create-module" {
        return;
    }
    if target == "flow-module-taken" {
        flow.run(target, &root, None, &[&module[..], &["My Editor"]].concat());
        return;
    }
    flow.run(target, &root, None, &module);
}

fn setup_module_two_images(flow: &Flow) {
    let target = flow.target;
    flow.run(
        target,
        &flow.repo_two("flow-module-both"),
        None,
        &["--root", ".", "create", "module"],
    );
}

fn setup_create_flavour(flow: &Flow) {
    let target = flow.target;
    let flavoured = flow.repo("flow-flavour");
    flow.run(
        target,
        &flavoured,
        None,
        &["--root", ".", "create", "flavour"],
    );
    // The CLI reference shows the block the run wrote.
    let image = std::fs::read_to_string(flavoured.join("example.image.kdl")).unwrap();
    flow.note_assertion();
    assert_golden(target, "example.image.kdl", &image);
}

/// The listing question is the image and its flavours, and a gated answer
/// writes the two blocks the image has neither of.
fn setup_module_in_flavour(flow: &Flow) {
    let target = flow.target;
    let root = flow.repo("flow-gated");
    flow.tect(
        &root,
        &[
            "--no-tui", "--root", ".", "create", "flavour", "dx", "--image", "example",
        ],
    );
    flow.run(target, &root, None, &["--root", ".", "create", "module"]);
    let image = std::fs::read_to_string(root.join("example.image.kdl")).unwrap();
    assert!(
            image.contains(
                "    modules {\n        flavour \"dx\" {\n            module \"dev-tools\"\n        }\n    }"
            ),
            "{image}"
        );

    // The ungated entry is in every flavour, so it and a gated one duplicate
    // each other. Two flavours of one image do not duplicate.
    flow.tect(
        &root,
        &[
            "--no-tui", "--root", ".", "create", "flavour", "gaming", "--image", "example",
        ],
    );
    let listed = |at: &str| {
        let (list, _, _) = tect::declarations(&root);
        tect::create::Listing::collect(&root, vec![at.into()], &common::prompt::Prompt::silent())
            .and_then(|listing| listing.refuse_duplicate(&list, "dev-tools", None))
            .err()
            .unwrap_or_default()
    };
    assert_eq!(
        listed("example/dx"),
        "`example/dx` already lists `dev-tools`"
    );
    assert_eq!(
        listed("example"),
        "`example/dx` already lists `dev-tools`, so `example` lists it twice"
    );
    assert_eq!(listed("example/gaming"), "");
}

/// Declares what the image is measured against: the profile is chosen out of
/// the content a scan of it would read, and the collection member claiming its
/// rules is offered with it. A second run replaces the declaration, and by
/// then there is nothing left to offer.
fn setup_set_conforms(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let measured = flow.repo_claiming("flow-set-conforms-in");
    for name in ["flow-set-conforms", "flow-set-conforms-again"] {
        flow.run(
            name,
            &measured,
            None,
            &["--root", ".", "set", "conforms", "--datastream", &stream],
        );
        if target == "flow-set-conforms" {
            return;
        }
    }
    let declared = std::fs::read_to_string(measured.join("example.image.kdl")).unwrap();
    assert_eq!(declared.matches("conforms ").count(), 1, "{declared}");
    assert!(
        declared.contains("    conforms \"ospp\"\n")
            && declared.contains("source \"claims\" {\n            module \"sshd\""),
        "{declared}"
    );
}

/// Runs the reverse offer the third `import module` makes: the set claims rules
/// a profile selects and the image listing it declares no `conforms`, so the
/// import offers one and both edits land in the one file. Declining writes
/// only the import, and `copy module` is never asked at all.
fn setup_import_conforms(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let claiming = flow.repo_claiming("flow-import-conforms-in");
    flow.run(target, &claiming, None, &import_sshd(&stream));
    let taken = std::fs::read_to_string(claiming.join("example.image.kdl")).unwrap();
    assert!(
        taken.contains("    conforms \"standard\"\n")
            && taken.contains("source \"claims\" {\n            module \"sshd\""),
        "{taken}"
    );
}

fn setup_import_conforms_declined(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let unmeasured = flow.repo_claiming("flow-import-conforms-none");
    flow.run(target, &unmeasured, None, &import_sshd(&stream));
    let left = std::fs::read_to_string(unmeasured.join("example.image.kdl")).unwrap();
    assert!(
        !left.contains("conforms") && left.contains("module \"sshd\""),
        "{left}"
    );
}

/// Runs the same two commands in an enforcing repository, which is the arm of
/// the cost line neither caller reached: there the scan does not only publish
/// a score, it fails the build.
fn setup_set_conforms_enforced(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let enforcing = flow.repo_enforcing("flow-set-conforms-enforced-in");
    flow.run(
        target,
        &enforcing,
        None,
        &["--root", ".", "set", "conforms", "--datastream", &stream],
    );
}

fn setup_import_conforms_enforced(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let importing = flow.repo_enforcing("flow-import-conforms-enforced-in");
    flow.run(target, &importing, None, &import_sshd(&stream));
}

/// Runs against two images in one listing, each getting the `conforms`
/// written: the sentence is plural and both files are edited, where every
/// golden above has one image and reads the same either way.
fn setup_import_conforms_two(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let both = flow.repo_claiming_two("flow-import-conforms-two-in");
    flow.run(target, &both, None, &import_sshd(&stream));
    for named in ["example", "server"] {
        let written = std::fs::read_to_string(both.join(format!("{named}.image.kdl"))).unwrap();
        assert!(
            written.contains("    conforms \"standard\"\n") && written.contains("module \"sshd\""),
            "{named}: {written}"
        );
    }
}

/// Runs with a named datastream that does not read, which is a typo the tool
/// refuses. Importing in silence hides it. One this machine merely happens to
/// lack is the other arm, and it is the one every flow above takes.
fn setup_import_datastream(flow: &Flow) {
    let target = flow.target;
    let typo = flow.repo_claiming("flow-import-datastream-in");
    flow.run(
        target,
        &typo,
        None,
        &[
            "--root",
            ".",
            "import",
            "module",
            "claims/sshd",
            "--datastream",
            "no-such-datastream.xml",
        ],
    );
    let untouched = std::fs::read_to_string(typo.join("example.image.kdl")).unwrap();
    // The declaration is what a refusal must not leave behind.
    assert!(
        !untouched.contains("module \"sshd\"") && !typo.join("modules/sshd").exists(),
        "the refusal leaves the repository as it was: {untouched}"
    );
}

/// Fills a `requires` from a collection holding an adapter for more than one
/// family. The image's base is fedora, so the fedora adapter is the one to
/// bring; picking the first provider of the capability brings the deb one,
/// which supports a family this image is not.
fn setup_import_family(flow: &Flow) {
    let target = flow.target;
    let family = flow.repo_with("flow-import-family-in", "upstream");
    flow.run(
        target,
        &family,
        None,
        &["--root", ".", "import", "module", "upstream/needs-family"],
    );
    let listed = std::fs::read_to_string(family.join("example.image.kdl")).unwrap();
    assert!(
        listed.contains("module \"fedora-family\"") && !listed.contains("debian-family"),
        "the adapter brought has to support the base's family: {listed}"
    );
}

/// Runs the same offer down the copy path, which is the same path: a vendored
/// module claiming rules a profile selects is exactly as worth measuring as a
/// referenced one.
fn setup_copy_conforms(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let vendored = flow.repo_claiming("flow-copy-conforms-in");
    flow.run(
        target,
        &vendored,
        None,
        &[
            "--root",
            ".",
            "copy",
            "module",
            "claims/sshd",
            "--datastream",
            stream.as_str(),
        ],
    );
    let copied = std::fs::read_to_string(vendored.join("example.image.kdl")).unwrap();
    assert!(
        copied.contains("conforms \"standard\"") && copied.contains("module \"sshd\""),
        "{copied}"
    );
}

/// Sets the claim the module author makes, chosen out of the rules a profile
/// selects. The second run opens on what the first wrote and replaces the
/// block, leaving one.
fn setup_set_claims(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let claimed = flow.repo("flow-set-claims-in");
    lists_sshd(&claimed);
    std::fs::create_dir_all(claimed.join("modules/sshd")).unwrap();
    std::fs::write(
        claimed.join("modules/sshd/module.kdl"),
        format!("{CLAIMANT}\nsatisfies {{\n    cis-fedora \"5.5.2\"\n}}\n"),
    )
    .unwrap();
    for name in ["flow-set-claims", "flow-set-claims-again"] {
        flow.run(name, &claimed, None, &claims(&stream));
        if target == "flow-set-claims" {
            return;
        }
    }
}

/// Runs against a module already declaring two benchmarks, which the writer
/// collapses to one node under the name the chosen profile derives. Every
/// claim above opened on a single node, so the merge across two was never
/// written.
fn setup_set_claims_two(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let two = flow.repo("flow-set-claims-two-in");
    lists_sshd(&two);
    std::fs::create_dir_all(two.join("modules/sshd")).unwrap();
    std::fs::write(
        two.join("modules/sshd/module.kdl"),
        format!(
            "{CLAIMANT}\nsatisfies {{\n    cis-fedora \"5.5.2\"\n    \
                 stig-fedora \"RHEL-09-232010\"\n}}\n"
        ),
    )
    .unwrap();
    flow.run(
        target,
        &two,
        None,
        &[
            "--root",
            ".",
            "set",
            "claims",
            "sshd",
            "--datastream",
            stream.as_str(),
        ],
    );
    let merged = std::fs::read_to_string(two.join("modules/sshd/module.kdl")).unwrap();
    assert_eq!(merged.matches("satisfies ").count(), 1, "{merged}");
    assert!(
        !merged.contains("cis-fedora") && !merged.contains("stig-fedora"),
        "both benchmark names collapse into the derived one: {merged}"
    );
    flow.tect(&two, &["--no-tui", "--root", ".", "check"]);
}

fn setup_set_claims_fetched(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let claimed = flow.repo("flow-set-claims-in");
    lists_sshd(&claimed);
    std::fs::create_dir_all(claimed.join("modules/sshd")).unwrap();
    std::fs::write(
        claimed.join("modules/sshd/module.kdl"),
        format!("{CLAIMANT}\nsatisfies {{\n    cis-fedora \"5.5.2\"\n}}\n"),
    )
    .unwrap();
    for name in ["flow-set-claims", "flow-set-claims-again"] {
        flow.run(name, &claimed, None, &claims(&stream));
    }
    std::fs::create_dir_all(claimed.join("modules/.remote/upstream/sshd")).unwrap();
    std::fs::write(
        claimed.join("modules/.remote/upstream/sshd/module.kdl"),
        CLAIMANT,
    )
    .unwrap();
    flow.run(
        target,
        &claimed,
        None,
        &[
            "--root",
            ".",
            "set",
            "claims",
            ".remote/upstream/sshd",
            "--datastream",
            stream.as_str(),
        ],
    );
    let declared = std::fs::read_to_string(claimed.join("modules/sshd/module.kdl")).unwrap();
    assert_eq!(declared.matches("satisfies ").count(), 1, "{declared}");
    // Asserts the two chosen rules and the claim about a rule this profile
    // never selects, which a rewrite that only wrote the answer would drop.
    assert!(
        declared
            .contains("    standard \"1.1.1.1\" \\\n        \"5.2.20\" \\\n        \"5.5.2\"\n"),
        "{declared}"
    );
    // Asserts what was written is a manifest the schema still takes, which one
    // benchmark node per number would not have been.
    flow.tect(&claimed, &["--no-tui", "--root", ".", "check"]);
}

/// Runs the same on a real terminal: two widgets, the second the collapsed
/// tree, answered through a filter so what the answer names is the option and
/// not the row the filter left it on.
fn setup_set_claims_drawn(flow: &Flow) {
    let target = flow.target;
    let stream = datastream();
    let drawn = flow.repo("flow-set-claims-drawn-in");
    lists_sshd(&drawn);
    std::fs::create_dir_all(drawn.join("modules/sshd")).unwrap();
    std::fs::write(drawn.join("modules/sshd/module.kdl"), CLAIMANT).unwrap();
    flow.drawn(
        target,
        &drawn,
        &format!(
            "'{}' --root . set claims sshd --datastream '{stream}'",
            env!("CARGO_BIN_EXE_tect")
        ),
        "Which rules does `sshd` claim?:",
        &[b"\r", b"aide \x1b[B\r"],
    );
    let picked = std::fs::read_to_string(drawn.join("modules/sshd/module.kdl")).unwrap();
    assert!(picked.contains("    standard \"1.1.1.1\"\n"), "{picked}");
}

fn setup_workflows(flow: &Flow) {
    let prompted = flow.repo("flow-set");
    flow.run(
        "flow-set-workflows",
        &prompted,
        None,
        &["--root", ".", "set", "workflows"],
    );
    let declaration =
        "workflows at=\"05:45\" scan=\"scheduled\" {\n    build\n    base-sig-probe\n}";
    let prompted_repo = std::fs::read_to_string(prompted.join("repo.kdl")).unwrap();
    assert!(prompted_repo.contains(declaration), "{prompted_repo}");

    let drawn = flow.repo("flow-set-workflows-drawn");
    let repo_path = drawn.join("repo.kdl");
    let repo = std::fs::read_to_string(&repo_path).unwrap().replace(
        "workflows {",
        "workflows publish=\"scheduled\" scan=\"scheduled\" {",
    );
    std::fs::write(&repo_path, repo).unwrap();
    flow.drawn(
        "flow-set-workflows-drawn",
        &drawn,
        &format!("'{}' --root . set workflows", env!("CARGO_BIN_EXE_tect")),
        "Which workflows?:",
        &[b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\r", b"\r", b"\r"],
    );
    let repo = std::fs::read_to_string(&repo_path).unwrap();
    assert!(
        repo.contains("workflows publish=\"scheduled\" scan=\"scheduled\" {"),
        "{repo}"
    );

    let direct = flow.repo("flow-cadence-direct");
    let repo_path = direct.join("repo.kdl");
    let mut direct_repo = std::fs::read_to_string(&repo_path).unwrap();
    let span = tect::parse::repo::workflows_span(&direct_repo).unwrap();
    direct_repo.replace_range(span.offset..span.offset + span.len, declaration);
    std::fs::write(&repo_path, direct_repo).unwrap();

    let generated_build = |root: &Path| {
        let run = tect::run(Command::Generate, None, root);
        assert!(run.issues.is_empty(), "{}", run.issues.plain());
        tect::write_generated(root, &run.files).unwrap();
        assert!(
            tect::run(Command::Verify, None, root).issues.is_empty(),
            "verify rejected its generated workflow"
        );
        run.files
            .into_iter()
            .find(|(path, _)| path == Path::new(".github/workflows/build.yml"))
            .unwrap()
            .1
    };
    let prompted_build = generated_build(&prompted);
    let direct_build = generated_build(&direct);
    let drawn_build = generated_build(&drawn);
    assert_eq!(prompted_build, direct_build);
    assert!(prompted_build.contains(
        "    if: needs.build_push.outputs.publish == 'true' && (github.event_name == 'schedule' || github.event_name == 'workflow_dispatch') && needs.compute-matrix.outputs.scanned != '[]'\n"
    ));
    assert!(!prompted_build.contains(
        "    if: needs.build_push.outputs.publish == 'true' && github.event_name != 'pull_request' && needs.compute-matrix.outputs.scanned != '[]'\n"
    ));

    let cadence = flow.repo("flow-publish-cadence");
    let repo_path = cadence.join("repo.kdl");
    let push_build = generated_build(&cadence);
    assert!(push_build.contains(
        "    if: needs.build_push.outputs.publish == 'true' && github.event_name != 'pull_request' && needs.compute-matrix.outputs.scanned != '[]'\n"
    ));
    let repo = std::fs::read_to_string(&repo_path).unwrap();
    std::fs::write(
        &repo_path,
        repo.replace("workflows {", "workflows publish=\"scheduled\" {"),
    )
    .unwrap();
    let scheduled_build = generated_build(&cadence);
    let publish_gate = r#"          if [ "${{ github.event_name }}" != "schedule" ] \
             && [ "${{ github.event_name }}" != "workflow_dispatch" ]; then
            publish=false
          fi
"#;
    assert!(scheduled_build.contains(publish_gate), "{scheduled_build}");
    assert_eq!(drawn_build, scheduled_build);
    assert_eq!(
        scheduled_build.replace(publish_gate, "").replace(
            "    if: needs.build_push.outputs.publish == 'true' && (github.event_name == 'schedule' || github.event_name == 'workflow_dispatch') && needs.compute-matrix.outputs.scanned != '[]'\n",
            "    if: needs.build_push.outputs.publish == 'true' && github.event_name != 'pull_request' && needs.compute-matrix.outputs.scanned != '[]'\n",
        ),
        push_build,
    );
}

/// Imports what a module requires and nothing in the image provides, and the
/// CI it makes runnable is offered. If the flow leaves it to be found, it is
/// not run.
fn setup_import_requires(flow: &Flow) {
    let target = flow.target;
    let requires = flow.repo_sourced("flow-requires");
    flow.run(
        target,
        &requires,
        None,
        &["--root", ".", "import", "module", "community/browser"],
    );
    let image = std::fs::read_to_string(requires.join("example.image.kdl")).unwrap();
    assert!(
        image.contains("source \"upstream\" {\n            module \"flatpak\"")
            && image.contains("source \"community\" {\n            module \"browser\""),
        "{image}"
    );
    flow.tect(
        &requires,
        &[
            "--no-tui",
            "--root",
            ".",
            "import",
            "module",
            "upstream/fedora-family",
            "--image",
            "example",
        ],
    );
    // The offer is the whole point: what it left behind has to resolve.
    flow.tect(&requires, &["--no-tui", "--root", ".", "check"]);
}

/// Declining leaves a file that is still valid, and a `check` that says which
/// import would satisfy what is missing.
fn setup_check_unmet(flow: &Flow) {
    let target = flow.target;
    let declined = flow.repo_sourced("flow-declined");
    flow.tect(
        &declined,
        &[
            "--no-tui",
            "--root",
            ".",
            "import",
            "module",
            "community/browser",
            "--image",
            "example",
        ],
    );
    flow.tect(
        &declined,
        &[
            "--no-tui",
            "--root",
            ".",
            "import",
            "module",
            "upstream/fedora-family",
            "--image",
            "example",
        ],
    );
    flow.run(target, &declined, None, &["--root", ".", "check"]);
}

/// Answers one listing question, and a member the offer brought is written
/// only where it is not already listed: the first image already lists
/// `flatpak`, so the offer is for the second alone and the write skips the
/// first for that member alone.
fn setup_import_skip(flow: &Flow) {
    let skip = flow.repo_sourced("flow-skip");
    flow.tect(
        &skip,
        &["--no-tui", "--root", ".", "create", "image", "Server"],
    );
    flow.tect(
        &skip,
        &[
            "--no-tui",
            "--root",
            ".",
            "import",
            "module",
            "upstream/flatpak",
            "--image",
            "example",
        ],
    );
    flow.run(
        "flow-import-skip",
        &skip,
        None,
        &[
            "--root",
            ".",
            "import",
            "module",
            "community/browser",
            "--image",
            "example",
            "--image",
            "server",
        ],
    );
    let image = std::fs::read_to_string(skip.join("example.image.kdl")).unwrap();
    assert_eq!(image.matches("module \"flatpak\"").count(), 1, "{image}");
    assert!(image.contains("module \"browser\""), "{image}");
    let server = std::fs::read_to_string(skip.join("server.image.kdl")).unwrap();
    assert!(
        server.contains("module \"flatpak\"") && server.contains("module \"browser\""),
        "{server}"
    );
    // Imports the adapter flatpak's package group needs, which the seeded
    // server image already lists and the unsourced one does not.
    flow.tect(
        &skip,
        &[
            "--no-tui",
            "--root",
            ".",
            "import",
            "module",
            "upstream/fedora-family",
            "--image",
            "example",
        ],
    );
    flow.tect(&skip, &["--no-tui", "--root", ".", "fetch", "modules"]);
    flow.tect(&skip, &["--no-tui", "--root", ".", "check"]);
}

/// Runs on a fresh clone: the collection `create repo` scaffolds is declared
/// and is not on this machine, and resolution never fetches. The help has to
/// name the fetch. Concluding that nothing anywhere provides it sends a person
/// looking for a module that exists.
fn setup_check_unfetched(flow: &Flow) {
    let target = flow.target;
    let unfetched = flow.repo("flow-unfetched");
    std::fs::create_dir_all(unfetched.join("modules/core/one")).unwrap();
    std::fs::write(
        unfetched.join("modules/core/one/module.kdl"),
        "description \"Builds things\"\n\nsupports \"fedora\"\n\nrequires \"build-environment\"\n",
    )
    .unwrap();
    let image = unfetched.join("example.image.kdl");
    let listed = std::fs::read_to_string(&image).unwrap().replace(
        "    modules {\n    }",
        "    modules {\n        module \"core/one\"\n    }",
    );
    assert!(listed.contains("module \"core/one\""), "{listed}");
    std::fs::write(&image, listed).unwrap();
    flow.run(target, &unfetched, None, &["--root", ".", "check"]);
}

fn setup_why_picker(flow: &Flow) {
    let target = flow.target;
    let why = flow.repo("flow-why-picker");
    std::fs::create_dir_all(why.join("modules/core/one")).unwrap();
    std::fs::write(
        why.join("modules/core/one/module.kdl"),
        "description \"Builds things\"\n\nsupports \"fedora\"\n",
    )
    .unwrap();
    let image = why.join("example.image.kdl");
    let listed = std::fs::read_to_string(&image).unwrap().replace(
        "    modules {\n    }",
        "    modules {\n        module \"core/one\"\n    }",
    );
    assert!(listed.contains("module \"core/one\""), "{listed}");
    std::fs::write(image, listed).unwrap();
    flow.drawn(
        target,
        &why,
        &format!(
            "stty cols 80; '{}' --root . why",
            env!("CARGO_BIN_EXE_tect")
        ),
        "Which module?:",
        &[b"\r"],
    );
}

fn setup_import_kernel(flow: &Flow) {
    let target = flow.target;
    let kernel = flow.repo_sourced("flow-kernel");
    flow.run(
        target,
        &kernel,
        None,
        &["--root", ".", "import", "module", "upstream/custom-kernel"],
    );
    assert!(std::fs::read_to_string(kernel.join("repo.kdl"))
        .unwrap()
        .contains("    kernel-freshness\n"));
}

fn setup_import_module(flow: &Flow) {
    let target = flow.target;
    let root = flow.repo_sourced("flow-import");
    flow.run(target, &root, None, &["--root", ".", "import", "module"]);
    let image = std::fs::read_to_string(root.join("example.image.kdl")).unwrap();
    assert!(image.contains("source \"upstream\" {\n            module \"browser\"\n        }"));
    assert!(root
        .join("modules/.remote/upstream/browser/module.kdl")
        .is_file());
    assert!(!root.join("modules/browser").exists());
}

fn setup_import_default(flow: &Flow) {
    let target = flow.target;
    let defaulted = flow.repo("flow-import-default-in");
    let repo = defaulted.join("repo.kdl");
    let text = std::fs::read_to_string(&repo)
        .unwrap()
        .replace(&tect::init::sources(&crate_dir().join("assets")), "")
        + "unfinished-repo-property \"kept for tect check\"\n";
    std::fs::write(&repo, text).unwrap();
    let image_file = defaulted.join("example.image.kdl");
    let image = std::fs::read_to_string(&image_file).unwrap().replace(
        "    modules {",
        "    unfinished-image-property \"kept for tect check\"\n    modules {",
    );
    std::fs::write(&image_file, image).unwrap();
    let (_, issues, _) = tect::declarations(&defaulted);
    assert!(
        !issues.is_empty(),
        "the fixture must prove edits do not depend on a clean check"
    );
    let assets = flow.at("flow-import-default-assets");
    let _ = std::fs::remove_dir_all(&assets);
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(
        assets.join(tect::init::SOURCES_FILE),
        format!(
            "sources {{\n    upstream {:?}\n}}\n",
            crate_dir().join("tests/collections/upstream").display()
        ),
    )
    .unwrap();
    flow.run_with_assets(
        "flow-import-default",
        &defaulted,
        None,
        &[
            "--no-tui",
            "--root",
            ".",
            "import",
            "module",
            "upstream/browser",
            "--image",
            "example",
        ],
        &assets,
    );
    let declared = std::fs::read_to_string(&repo).unwrap();
    let image = std::fs::read_to_string(defaulted.join("example.image.kdl")).unwrap();
    assert!(
        declared.contains("sources {\n    upstream ")
            && image.contains("source \"upstream\" {\n            module \"browser\""),
        "{declared}\n{image}"
    );
    if target == "flow-import-default" {
        return;
    }
    flow.run(
        "flow-copy-with-schema-issues",
        &defaulted,
        None,
        &[
            "--no-tui",
            "--root",
            ".",
            "copy",
            "module",
            "upstream/flatpak",
            "--image",
            "example",
        ],
    );
    let image = std::fs::read_to_string(&image_file).unwrap();
    assert!(image.contains("module \"flatpak\""), "{image}");
    if target == "flow-copy-with-schema-issues" {
        return;
    }
    flow.run(
        "flow-create-module-with-schema-issues",
        &defaulted,
        None,
        &[
            "--no-tui",
            "--root",
            ".",
            "create",
            "module",
            "local-tool",
            "--pkg",
            "hello",
            "--with",
            "description=",
            "--with",
            "supports=",
            "--image",
            "example",
        ],
    );
    let image = std::fs::read_to_string(&image_file).unwrap();
    assert!(
        image.contains("module \"flatpak\"") && image.contains("module \"local-tool\""),
        "{image}"
    );
}

fn setup_import_edit_guards(flow: &Flow) {
    let malformed = flow.repo_sourced("flow-malformed-edit-in");
    let image_file = malformed.join("example.image.kdl");
    let mut image = std::fs::read_to_string(&image_file).unwrap();
    image.push_str("image {\n");
    std::fs::write(&image_file, image).unwrap();
    let before = contents(&malformed);
    for args in [
        vec![
            "--no-tui",
            "--root",
            ".",
            "import",
            "module",
            "upstream/browser",
            "--image",
            "example",
        ],
        vec![
            "--no-tui",
            "--root",
            ".",
            "copy",
            "module",
            "upstream/flatpak",
            "--image",
            "example",
        ],
        vec![
            "--no-tui",
            "--root",
            ".",
            "create",
            "module",
            "local-tool",
            "--with",
            "description=",
            "--with",
            "supports=",
            "--image",
            "example",
        ],
    ] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_tect"));
        let out = sealed(&mut command, &flow.bin("malformed-edit", None))
            .args(args)
            .current_dir(&malformed)
            .output()
            .unwrap();
        assert!(!out.status.success(), "malformed KDL was accepted");
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(error.contains("is not valid KDL"), "{error}");
        assert_eq!(contents(&malformed), before, "a refused edit wrote files");
    }

    for (name, args, declaration) in [
        (
            "flow-import-without-modules",
            vec![
                "--no-tui",
                "--root",
                ".",
                "import",
                "module",
                "upstream/browser",
                "--image",
                "example",
            ],
            "source \"upstream\" {\n            module \"browser\"",
        ),
        (
            "flow-copy-without-modules",
            vec![
                "--no-tui",
                "--root",
                ".",
                "copy",
                "module",
                "upstream/flatpak",
                "--image",
                "example",
            ],
            "modules {\n        module \"flatpak\"",
        ),
        (
            "flow-create-without-modules",
            vec![
                "--no-tui",
                "--root",
                ".",
                "create",
                "module",
                "local-tool",
                "--with",
                "description=",
                "--with",
                "supports=",
                "--image",
                "example",
            ],
            "modules {\n        module \"local-tool\"",
        ),
    ] {
        let root = flow.repo_sourced(name);
        let image_file = root.join("example.image.kdl");
        let image = std::fs::read_to_string(&image_file)
            .unwrap()
            .replace("    modules {\n    }\n", "");
        assert!(!image.contains("modules {"), "fixture still has modules");
        std::fs::write(&image_file, image).unwrap();
        flow.tect(&root, &args);
        let image = std::fs::read_to_string(&image_file).unwrap();
        assert!(image.contains(declaration), "{image}");
    }

    let root = flow.repo_sourced("flow-import-guards");
    flow.tect(
        &root,
        &[
            "--no-tui",
            "--root",
            ".",
            "import",
            "module",
            "upstream/browser",
            "--image",
            "example",
        ],
    );
    let (list, issues, _) = tect::declarations(&root);
    assert!(issues.is_empty(), "{}", issues.plain());
    let declined = tect::import::Module::collect(
        Some("upstream/flatpak".into()),
        &root,
        &list.sources,
        false,
        Vec::new(),
        None,
        tect::import::Place::Reference,
        &common::prompt::Prompt::silent(),
    )
    .unwrap_or_else(|err| panic!("{}", err.message()))
    .apply(&root, &list.sources)
    .unwrap_err();
    assert!(declined.contains("--image"), "{declined}");

    tect::import::Module::collect(
        Some("upstream/flatpak".into()),
        &root,
        &list.sources,
        false,
        vec!["example".into()],
        None,
        tect::import::Place::Reference,
        &common::prompt::Prompt::silent(),
    )
    .unwrap_or_else(|err| panic!("{}", err.message()))
    .apply(&root, &list.sources)
    .unwrap();
    let image = std::fs::read_to_string(root.join("example.image.kdl")).unwrap();
    assert_eq!(image.matches("source \"upstream\"").count(), 1);
    assert!(image.contains("module \"browser\"\n            module \"flatpak\""));

    // A duplicate is refused at the edit, before any later command reads the
    // file. A module gated to two flavours is listed under each, so only an
    // overlap is one.
    let twice = tect::import::Module::collect(
        Some("upstream/flatpak".into()),
        &root,
        &list.sources,
        false,
        vec!["example".into()],
        None,
        tect::import::Place::Reference,
        &common::prompt::Prompt::silent(),
    )
    .err()
    .map(|err| err.message().to_string())
    .unwrap_or_default();
    assert_eq!(twice, "`example` already lists `flatpak`");
}

/// Several modules are imported at once, with one listing answer and one of
/// each offer for the set.
fn setup_import_several(flow: &Flow) {
    let several = flow.repo_sourced("flow-several");
    flow.run(
        "flow-import-several",
        &several,
        None,
        &["--root", ".", "import", "module"],
    );
    let image = std::fs::read_to_string(several.join("example.image.kdl")).unwrap();
    for module in ["flatpak", "browser", "custom-kernel"] {
        assert!(image.contains(&format!("module \"{module}\"")), "{image}");
    }
    assert!(std::fs::read_to_string(several.join("repo.kdl"))
        .unwrap()
        .contains("    kernel-freshness\n"));
    flow.tect(
        &several,
        &[
            "--no-tui",
            "--root",
            ".",
            "import",
            "module",
            "upstream/fedora-family",
            "--image",
            "example",
        ],
    );
    flow.tect(&several, &["--no-tui", "--root", ".", "check"]);
}

/// A collection that groups what it holds in a directory: the walk names the
/// member by its path under the collection, and the picker, the line an image
/// takes, the fetch and the resolver all read it as one name.
fn setup_import_nested(flow: &Flow) {
    let nested = flow.repo_with("flow-nested", "grouped");
    flow.run(
        "flow-import-nested",
        &nested,
        None,
        &["--root", ".", "import", "module"],
    );
    let image = std::fs::read_to_string(nested.join("example.image.kdl")).unwrap();
    assert!(
        image.contains(
            "source \"grouped\" {\n            module \"hardening/coredumps\"\n        }"
        ),
        "{image}"
    );
    assert!(nested
        .join("modules/.remote/grouped/hardening/coredumps/module.kdl")
        .is_file());
    flow.tect(&nested, &["--no-tui", "--root", ".", "fetch", "modules"]);
    flow.tect(&nested, &["--no-tui", "--root", ".", "check"]);
    flow.tect(&nested, &["--no-tui", "--root", ".", "generate"]);
    assert!(nested
        .join("generated/example/modules/grouped/hardening/coredumps.sh")
        .is_file());
    let (list, _, _) = tect::declarations(&nested);
    let refused = tect::import::find(
        &nested,
        &list.sources,
        "grouped/hardening//coredumps",
        false,
    )
    .err()
    .expect("an empty part of a path is refused");
    assert_eq!(
        refused,
        "`grouped/hardening//coredumps` is not a module: a module is named by a path of names, \
             as `<path>`, or `<owner>/<path>` to name one collection, and no part of it may be \
             empty or start with a dot"
    );
}

/// A typed name is a suffix of a member path at a `/` boundary, as `why` reads
/// it: `coredumps` resolves `hardening/coredumps`, and the canonical name is
/// what the image lists and the build runs.
fn setup_import_suffix(flow: &Flow) {
    let suffix = flow.repo_with("flow-suffix", "grouped");
    flow.run(
        "flow-import-suffix",
        &suffix,
        None,
        &[
            "--root",
            ".",
            "import",
            "module",
            "coredumps",
            "--image",
            "example",
        ],
    );
    let image = std::fs::read_to_string(suffix.join("example.image.kdl")).unwrap();
    assert!(
        image.contains(
            "source \"grouped\" {\n            module \"hardening/coredumps\"\n        }"
        ),
        "{image}"
    );
    assert!(!image.contains("module \"coredumps\""), "{image}");
    flow.tect(&suffix, &["--no-tui", "--root", ".", "fetch", "modules"]);
    flow.tect(&suffix, &["--no-tui", "--root", ".", "check"]);
    flow.tect(&suffix, &["--no-tui", "--root", ".", "generate"]);
    assert!(suffix
        .join("generated/example/modules/grouped/hardening/coredumps.sh")
        .is_file());
}

/// Two collections hold a member ending in the typed name: the ask lists
/// qualified names, and choosing one lists that one and not the other.
fn setup_import_suffix_ambiguous(flow: &Flow) {
    let both = flow.repo("flow-suffix-ambiguous");
    let collections = crate_dir().join("tests/collections");
    let mut repo = std::fs::read_to_string(both.join("repo.kdl"))
        .unwrap()
        .replace(&tect::init::sources(&crate_dir().join("assets")), "");
    repo.push_str(&format!(
        "sources {{\n    grouped {:?}\n    namesake {:?}\n}}\n",
        collections.join("grouped").display(),
        collections.join("namesake").display()
    ));
    std::fs::write(both.join("repo.kdl"), repo).unwrap();
    flow.run(
        "flow-import-suffix-ambiguous",
        &both,
        None,
        &[
            "--root",
            ".",
            "import",
            "module",
            "coredumps",
            "--image",
            "example",
        ],
    );
    let image = std::fs::read_to_string(both.join("example.image.kdl")).unwrap();
    assert!(
        image.contains(
            "source \"grouped\" {\n            module \"hardening/coredumps\"\n        }"
        ),
        "{image}"
    );
    assert!(!image.contains("namesake"), "{image}");
    assert!(!image.contains("sandbox"), "{image}");
}

/// A member that ships a path another listed module ships: the import says so
/// the moment it writes, in `check`'s own sentence, and the next `check`
/// reports the same pair.
fn setup_collides(flow: &Flow) {
    let target = flow.target;
    let collide = flow.repo_sourced("flow-collides");
    let remotes = "modules/editor/files/usr/share/example";
    std::fs::create_dir_all(collide.join(remotes)).unwrap();
    std::fs::write(
        collide.join("modules/editor/module.kdl"),
        "description \"Editor shipping its own flatpak remotes\"\n\nsupports \"fedora\"\n",
    )
    .unwrap();
    std::fs::write(collide.join(remotes).join("remotes.list"), "editor\n").unwrap();
    let image = collide.join("example.image.kdl");
    let listed = std::fs::read_to_string(&image).unwrap().replace(
        "    modules {\n    }",
        "    modules {\n        module \"editor\"\n    }",
    );
    assert!(listed.contains("module \"editor\""), "{listed}");
    std::fs::write(&image, listed).unwrap();
    flow.tect(
        &collide,
        &[
            "--no-tui",
            "--root",
            ".",
            "import",
            "module",
            "upstream/fedora-family",
            "--image",
            "example",
        ],
    );
    flow.run(
        "flow-import-collides",
        &collide,
        None,
        &[
            "--root",
            ".",
            "import",
            "module",
            "upstream/flatpak",
            "--image",
            "example",
        ],
    );
    assert!(
        tect::run(Command::Check, None, &collide)
            .issues
            .plain()
            .contains("`upstream/flatpak` overwrites `/usr/share/example/remotes.list`"),
        "check reports the collision the import said"
    );
    if target == "flow-import-collides" {
        return;
    }
    flow.run(target, &collide, None, &["--root", ".", "check"]);
}

/// Copies the same nested member into the tree: it vendors to the same depth
/// it is named at, which the scanner and the checks walk.
fn setup_copy_nested(flow: &Flow) {
    let copied_nested = flow.repo_with("flow-copy-nested", "grouped");
    flow.run(
        "flow-copy-nested",
        &copied_nested,
        None,
        &["--root", ".", "copy", "module"],
    );
    assert!(copied_nested
        .join("modules/hardening/coredumps/provenance.kdl")
        .is_file());
    let image = std::fs::read_to_string(copied_nested.join("example.image.kdl")).unwrap();
    assert!(
        image.contains("    modules {\n        module \"hardening/coredumps\"\n    }"),
        "{image}"
    );
    flow.tect(&copied_nested, &["--no-tui", "--root", ".", "check"]);
    flow.tect(&copied_nested, &["--no-tui", "--root", ".", "generate"]);
    assert!(copied_nested
        .join("generated/example/modules/hardening/coredumps.sh")
        .is_file());
}

/// The vendoring verb says the same collision: the copy is the repository's
/// own module now, but the sentence is `check`'s and the next one agrees.
fn setup_copy_collides(flow: &Flow) {
    let remotes = "modules/editor/files/usr/share/example";
    let copied = flow.repo_sourced("flow-copy-collides");
    std::fs::create_dir_all(copied.join(remotes)).unwrap();
    std::fs::write(
        copied.join("modules/editor/module.kdl"),
        "description \"Editor shipping its own flatpak remotes\"\n\nsupports \"fedora\"\n",
    )
    .unwrap();
    std::fs::write(copied.join(remotes).join("remotes.list"), "editor\n").unwrap();
    let image = copied.join("example.image.kdl");
    let listed = std::fs::read_to_string(&image).unwrap().replace(
        "    modules {\n    }",
        "    modules {\n        module \"editor\"\n    }",
    );
    assert!(listed.contains("module \"editor\""), "{listed}");
    std::fs::write(&image, listed).unwrap();
    flow.run(
        "flow-copy-collides",
        &copied,
        None,
        &[
            "--root",
            ".",
            "copy",
            "module",
            "upstream/flatpak",
            "--image",
            "example",
        ],
    );
    assert!(
        tect::run(Command::Check, None, &copied)
            .issues
            .plain()
            .contains("`flatpak` overwrites `/usr/share/example/remotes.list`"),
        "check reports the collision the copy said"
    );
}

fn setup_copy_module(flow: &Flow) {
    let target = flow.target;
    let root = flow.repo_sourced("flow-copy");
    flow.run(target, &root, None, &["--root", ".", "copy", "module"]);
    assert!(root.join("modules/browser/provenance.kdl").is_file());
    assert!(!root.join("modules/upstream").exists());
}

/// Neither branch reaches a generator, so neither needs one installed.
fn setup_key_absent(flow: &Flow) {
    let target = flow.target;
    flow.run(
        target,
        &flow.repo_sourced("flow-key-none"),
        None,
        &["--root", ".", "create", "key", "cosign"],
    );
}

fn setup_key_kinds(flow: &Flow) {
    let target = flow.target;
    let root = flow.repo("flow-key");
    std::fs::create_dir_all(root.join("modules/signed-kernel")).unwrap();
    std::fs::write(root.join("modules/signed-kernel/module.kdl"), KEYHOLDER).unwrap();
    flow.run(target, &root, None, &["--root", ".", "create", "key"]);
}

/// Runs for a kind nothing declares anywhere: the two fixture collections are
/// on this machine and are searched, and neither they nor the repository
/// carries one.
fn setup_key_undeclared(flow: &Flow) {
    let target = flow.target;
    flow.run(
        target,
        &flow.repo_sourced("flow-key-undeclared"),
        None,
        &["--root", ".", "create", "key", "sbom"],
    );
}

/// Runs with no kind named and nothing to prompt from.
fn setup_key_no_kind(flow: &Flow) {
    let target = flow.target;
    flow.run(
        target,
        &flow.repo("flow-key-no-kind"),
        None,
        &["--root", ".", "create", "key"],
    );
}

macro_rules! flow_cases {
    ($($test:ident => $fixture:literal),+ $(,)?) => {
        const FLOW_CASES: &[&str] = &[$($fixture),+];

        $(
            #[test]
            fn $test() {
                let flow = Flow::new($fixture);
                flow_case(&flow);
                flow.done();
            }
        )+
    };
}

flow_cases! {
    flow_check_claims => "flow-check-claims",
    flow_check_collides => "flow-check-collides",
    flow_check_conforms => "flow-check-conforms",
    flow_check_conforms_stranger => "flow-check-conforms-stranger",
    flow_check_shadow => "flow-check-shadow",
    flow_check_unfetched => "flow-check-unfetched",
    flow_check_unmet => "flow-check-unmet",
    flow_check_unpinned => "flow-check-unpinned",
    flow_copy_collides => "flow-copy-collides",
    flow_copy_conforms => "flow-copy-conforms",
    flow_copy_module => "flow-copy-module",
    flow_copy_nested => "flow-copy-nested",
    flow_copy_with_schema_issues => "flow-copy-with-schema-issues",
    flow_coverage => "flow-coverage",
    flow_create_flavour => "flow-create-flavour",
    flow_create_image => "flow-create-image",
    flow_create_module => "flow-create-module",
    flow_create_module_with_schema_issues => "flow-create-module-with-schema-issues",
    flow_create_repo => "flow-create-repo",
    flow_create_repo_forgejo => "flow-create-repo-forgejo",
    flow_create_repo_no_gh => "flow-create-repo-no-gh",
    flow_create_repo_offer => "flow-create-repo-offer",
    flow_create_repo_signed_in => "flow-create-repo-signed-in",
    flow_create_repo_signed_out => "flow-create-repo-signed-out",
    flow_image_default => "flow-image-default",
    flow_import_collides => "flow-import-collides",
    flow_import_conforms => "flow-import-conforms",
    flow_import_conforms_declined => "flow-import-conforms-declined",
    flow_import_conforms_enforced => "flow-import-conforms-enforced",
    flow_import_conforms_two => "flow-import-conforms-two",
    flow_import_datastream => "flow-import-datastream",
    flow_import_default => "flow-import-default",
    flow_import_family => "flow-import-family",
    flow_import_kernel => "flow-import-kernel",
    flow_import_module => "flow-import-module",
    flow_import_nested => "flow-import-nested",
    flow_import_requires => "flow-import-requires",
    flow_import_several => "flow-import-several",
    flow_import_skip => "flow-import-skip",
    flow_import_suffix => "flow-import-suffix",
    flow_import_suffix_ambiguous => "flow-import-suffix-ambiguous",
    flow_key_absent => "flow-key-absent",
    flow_key_kinds => "flow-key-kinds",
    flow_key_no_kind => "flow-key-no-kind",
    flow_key_undeclared => "flow-key-undeclared",
    flow_module_in_flavour => "flow-module-in-flavour",
    flow_module_taken => "flow-module-taken",
    flow_module_two_images => "flow-module-two-images",
    flow_set_claims => "flow-set-claims",
    flow_set_claims_again => "flow-set-claims-again",
    flow_set_claims_drawn => "flow-set-claims-drawn",
    flow_set_claims_fetched => "flow-set-claims-fetched",
    flow_set_claims_two => "flow-set-claims-two",
    flow_set_conforms => "flow-set-conforms",
    flow_set_conforms_again => "flow-set-conforms-again",
    flow_set_conforms_enforced => "flow-set-conforms-enforced",
    flow_set_workflows => "flow-set-workflows",
    flow_set_workflows_drawn => "flow-set-workflows-drawn",
    flow_unanswered => "flow-unanswered",
    flow_why_picker => "flow-why-picker",
}

#[test]
fn import_edit_guards() {
    let flow = Flow::new("import-edit-guards");
    flow_case(&flow);
    flow.done();
}

#[test]
fn every_answers_directory_has_a_case() {
    let mut found: Vec<String> = std::fs::read_dir(crate_dir().join("tests/answers"))
        .expect("tests/answers exists")
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    for name in &found {
        assert!(
            FLOW_CASES.contains(&name.as_str()),
            "tests/answers/{name} has no named test"
        );
    }
}

/// Below its floor a read-out falls back to the markdown a pipe gets, however
/// the terminal says its width: the pty's own answer, or `COLUMNS` overriding
/// it. `tect why` is the case that matters, its hash column being the widest
/// row in the tool.
#[test]
fn narrow_readouts_fall_back() {
    let root = crate_dir().join("tests/repos/enforced");
    let tect = env!("CARGO_BIN_EXE_tect");
    let run = |command: &str, cols: Option<&str>, clear: bool| {
        let mut child = std::process::Command::new("script");
        child
            .args(["-qfec", command, "/dev/null"])
            .current_dir(&root)
            .env("TECT_ASSETS", crate_dir().join("assets"))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if clear {
            child.env_remove("COLUMNS");
        }
        if let Some(cols) = cols {
            child.env("COLUMNS", cols);
        }
        let out = child
            .spawn()
            .expect("script from util-linux")
            .wait_with_output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    // Asks the terminal's own width, made narrow with stty.
    let graph = run(
        &format!("stty cols 40 rows 24; '{tect}' --root . graph"),
        None,
        true,
    );
    assert!(graph.contains("# Enforced capability graph"), "{graph}");
    assert!(!graph.contains('\u{250c}'), "{graph}");
    let why = run(
        &format!("stty cols 40 rows 24; '{tect}' --root . why one/hello"),
        None,
        true,
    );
    assert!(why.contains("## Where it is built"), "{why}");
    assert!(!why.contains('\u{250c}'), "{why}");

    // `COLUMNS` names a width the terminal will not say.
    let graph = run(&format!("'{tect}' --root . graph"), Some("40"), false);
    assert!(
        graph.contains("# Enforced capability graph") && !graph.contains('\u{250c}'),
        "{graph}"
    );
    let why = run(
        &format!("'{tect}' --root . why one/hello"),
        Some("40"),
        false,
    );
    assert!(
        why.contains("## Where it is built") && !why.contains('\u{250c}'),
        "{why}"
    );

    // At 200 columns both draw a table.
    for (name, command) in [
        ("graph", format!("'{tect}' --root . graph")),
        ("why", format!("'{tect}' --root . why one/hello")),
    ] {
        let drawn = run(&command, Some("200"), false);
        assert!(
            drawn.contains('\u{250c}'),
            "{name} drew no table at 200 columns"
        );
    }
}

/// Runs `create repo` on a real terminal, sealed like `flow`: the owner
/// question is the one drawn line with a prefix, and its echo is what carries
/// it.
#[test]
fn flow_create_repo_drawn() {
    let flow = Flow::new("flow-create-repo-drawn");
    let dir = flow.empty("flow-create-repo-drawn-in");
    flow.drawn(
        "flow-create-repo-drawn",
        &dir,
        &format!(
            "env PATH='{}' HOME='{}' GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null \
             '{}' create repo",
            flow.bin("flow-create-repo-drawn", None).display(),
            tmp().display(),
            env!("CARGO_BIN_EXE_tect")
        ),
        "Creating example...",
        &[
            b"Example\r",
            // Chooses synced, to github.com, as `someone`.
            b"\r",
            b"\r",
            b"someone\r",
            // Skips creating it on the forge, and defines no image.
            b"\x1b[B\r",
            b"\x1b[B\r",
            // Accepts the workflows, both cadences, the time and the kept
            // files, as offered.
            b"\r",
            b"\r",
            b"\r",
            b"\r",
            b"\r",
            // Moves down the review past its nine fields to `Create`.
            b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\r",
        ],
    );
    flow.done();
    assert!(dir.join("example/repo.kdl").is_file());
}

#[test]
fn escape_cancels_create_image_at_each_prompt() {
    let flow = Flow::new("escape-create-image");
    let made = flow.repo_sourced("flow-create-image-cancel-in");
    let root = made.parent().unwrap().join("cancel-default");
    std::fs::rename(&made, &root).unwrap();
    let binary = env!("CARGO_BIN_EXE_tect");
    for (command, steps, absent) in [
        (
            format!("'{binary}' --root . create image"),
            vec![b"\x1b".as_slice()],
            "cancel-default.image.kdl",
        ),
        (
            format!("'{binary}' --root . create image"),
            vec![b"Beta\r".as_slice(), b"\x1b".as_slice()],
            "beta.image.kdl",
        ),
        (
            format!("'{binary}' --root . create image Gamma --base example.invalid/collected:1"),
            vec![b"\x1b".as_slice()],
            "gamma.image.kdl",
        ),
    ] {
        let before = std::fs::read_to_string(root.join("repo.kdl")).unwrap();
        let (status, errors, raw) = drawn_run(&root, &command, &steps);
        assert!(
            status.success(),
            "{errors}{}",
            String::from_utf8_lossy(&raw)
        );
        assert!(!root.join(absent).exists(), "Escape wrote {absent}");
        assert_eq!(
            std::fs::read_to_string(root.join("repo.kdl")).unwrap(),
            before,
            "Escape edited repo.kdl"
        );
    }
}

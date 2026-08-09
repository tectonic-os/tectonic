//! The only reader of the image files and the per-module module.kdl files.

pub mod create;
pub mod diag;
pub mod emit;
pub mod fetch;
pub mod import;
pub mod init;
pub mod model;
pub mod parse;
pub mod prompt;
pub mod registry;
pub mod resolve;
pub mod runtime;
pub mod ui;

use diag::Issue;
use diag::Issues;
use diag::Source;
use model::image::{List, Target, REPO_FILE};
use model::module::Module;
use model::remote::Collection;
pub use parse::repo::compatible;
use resolve::Resolved;
use std::path::{Path, PathBuf};

/// The nearest directory at or above `from` holding a repo.kdl.
pub fn find_root(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .find(|dir| dir.join(REPO_FILE).is_file())
        .map(Path::to_path_buf)
}

/// What the repository declares, without loading a module manifest: enough for
/// anything that only needs a name, and readable before a pinned module has
/// been fetched.
pub fn declarations(root: &Path) -> (List, Issues, String) {
    let (list, issues) = List::load(root);
    let context = context(&list, root);
    (list, issues, context)
}

/// The collections repo.kdl names, and anything wrong with the repository an
/// import has to see before it writes into it.
pub fn sources(root: &Path) -> (Vec<Collection>, Issues, String) {
    let (list, issues, context) = declarations(root);
    (list.sources, issues, context)
}

/// The files read, for the line a failure ends with.
fn context(list: &List, root: &Path) -> String {
    match list.files.is_empty() {
        true => root.display().to_string(),
        false => list.files.join(", "),
    }
}

/// What one command produced: its output, everything wrong with the repository,
/// and the counts `check` reports.
pub struct Run {
    pub stdout: String,
    /// What `generate` produced, as the path each file is written at relative
    /// to the repository root, and its contents.
    pub files: Vec<(PathBuf, String)>,
    pub issues: Issues,
    /// The files read, for the line a failure ends with.
    pub context: String,
    pub images: usize,
    pub modules: usize,
    /// Listed modules the base covers, which nothing builds.
    pub suppressed: usize,
    pub flavours: usize,
}

/// The Containerfile skeleton, when the repository has one to splice into. A
/// repository with no `scripts/` generates its module scripts and no
/// Containerfile.
fn skeleton(root: &Path, issues: &mut Issues) -> Option<String> {
    use emit::containerfile::{BEGIN, END, SKELETON};

    let text = std::fs::read_to_string(root.join(SKELETON)).ok()?;
    let src = Source::new(SKELETON, text.clone());
    let mut found = true;
    for marker in [BEGIN, END] {
        if !text.lines().any(|line| line == marker) {
            issues.push(
                Issue::new(format!("`{SKELETON}` has no `{marker}` line"), &src)
                    .help("the generated phases and module layers go between the two markers"),
            );
            found = false;
        }
    }
    found.then_some(text)
}

/// Loads the repository, resolves every image, then runs one command over the
/// result. `arg` names the image `section` renders and the target `summary` and
/// `sbom` answer about; the defaults otherwise.
pub fn run(command: &str, arg: Option<&str>, root: &Path) -> Run {
    let (target_arg, image_arg) = match command {
        "summary" | "sbom" => (arg, None),
        _ => (None, arg),
    };
    let (mut list, mut issues) = List::load(root);
    let context = context(&list, root);

    let workflows = resolve::workflow::resolve(&list, root, &mut issues);
    let disk = parse::disk::Disk::scan(root, &mut issues);
    parse::module::check_unlisted(&list, root, &disk, &mut issues);

    let mut resolved: Vec<Resolved> = Vec::new();
    for image in &mut list.images {
        // Taken out so a diagnostic can still read the image it was declared in.
        let mut entries = std::mem::take(&mut image.entries);
        for entry in &mut entries {
            entry.module = Module::load(entry, image, root, &mut issues);
        }
        image.entries = entries;

        resolve::graph::suppress(image);
        let order = resolve::order::sort(image, &mut issues);
        resolve::order::apply(image, &order);
        resolve::graph::check_graph(image, root, &disk, &mut issues);
        resolve::graph::check_fragments(image, &mut issues);
        let shipped = resolve::overlay::index(image, &disk);
        resolve::overlay::check(image, &shipped, &mut issues);
        let collected = resolve::collect::resolve_collects(image, root, &disk, &mut issues);

        resolved.push(Resolved { shipped, collected });
    }

    if let Some(unknown) = image_arg.filter(|id| !list.images.iter().any(|i| i.id == *id)) {
        let known: Vec<&str> = list.images.iter().map(|i| i.id.as_str()).collect();
        issues.push(
            Issue::new(
                format!("`{unknown}` is not a declared image"),
                &list.repo_src,
            )
            .help(format!("images: {}", known.join(", "))),
        );
    }

    let targets: Vec<String> = list.targets().iter().map(Target::to_string).collect();
    if let Some(unknown) = target_arg.filter(|name| !targets.iter().any(|have| have == name)) {
        issues.push(
            Issue::new(format!("`{unknown}` is not a build target"), &list.repo_src)
                .help(format!("targets: {}", targets.join(", "))),
        );
    }
    let target = target_arg
        .map(str::to_string)
        .or_else(|| list.default_target().map(|t| t.to_string()));

    let one = match image_arg {
        Some(id) => list.images.iter().position(|i| i.id == id),
        None => list
            .default_image()
            .and_then(|d| list.images.iter().position(|i| i.id == d.id)),
    };

    let skeleton = skeleton(root, &mut issues);

    let mut files: Vec<(PathBuf, String)> = Vec::new();
    if matches!(command, "generate" | "verify") {
        for (image, resolved) in list.images.iter().zip(&resolved) {
            if let Some(skeleton) = &skeleton {
                let section =
                    emit::containerfile::section(image, &resolved.collected, &disk.phases, root);
                files.push((
                    emit::containerfile::path(image),
                    emit::containerfile::file(skeleton, image, &section),
                ));
            }
            files.extend(emit::module_build::scripts(
                image,
                &resolved.collected,
                root,
            ));
            files.extend(emit::graph::files(image));
        }
    }

    if command == "verify" {
        verify(root, &files, &mut issues);
    }

    let stdout = match command {
        "plan" => emit::plan::build(&list, &resolved, &workflows).render(),
        "summary" => target
            .and_then(|name| emit::summary::render(&list, &name))
            .unwrap_or_default(),
        "sbom" => target
            .and_then(|name| emit::sbom::build(&list, &name))
            .map(|json| json.render())
            .unwrap_or_default(),
        "graph" | "graph-json" => match one {
            Some(i) => {
                let graph = emit::graph::of(&list.images[i]);
                match command {
                    "graph" => graph.markdown(),
                    _ => graph.json().render(),
                }
            }
            None => String::new(),
        },
        "section" => match one {
            Some(i) => emit::containerfile::section(
                &list.images[i],
                &resolved[i].collected,
                &disk.phases,
                root,
            ),
            None => String::new(),
        },
        "generate" => files
            .iter()
            .map(|(path, _)| format!("{}\n", path.display()))
            .collect(),
        _ => String::new(),
    };

    Run {
        stdout,
        files,
        issues,
        context,
        images: list.images.len(),
        modules: list.images.iter().map(|i| i.modules().count()).sum(),
        suppressed: list.images.iter().map(|i| i.suppressed.len()).sum(),
        flavours: list.images.iter().map(|i| i.flavours.len()).sum(),
    }
}

/// What `generate` produced, against what is committed under `generated/`: an
/// artifact that differs, one that is not there, and one nothing generates.
fn verify(root: &Path, files: &[(PathBuf, String)], issues: &mut Issues) {
    const HELP: &str = "run `tect generate` and commit what it writes";

    for (path, generated) in files {
        let name = path.display().to_string();
        let Ok(found) = std::fs::read_to_string(root.join(path)) else {
            issues.push(
                Issue::new(format!("`{name}` is not there"), &Source::new(&name, "")).help(HELP),
            );
            continue;
        };
        if found == *generated {
            continue;
        }
        let (span, line) = difference(&found, generated);
        issues.push(
            Issue::new(
                format!("`{name}` is not what this repository generates"),
                &Source::new(&name, found),
            )
            .at(span, line)
            .help(HELP),
        );
    }

    for path in tracked(&root.join("generated")) {
        let Ok(path) = path.strip_prefix(root).map(Path::to_path_buf) else {
            continue;
        };
        if files.iter().any(|(emitted, _)| *emitted == path) {
            continue;
        }
        let name = path.display().to_string();
        issues.push(
            Issue::new(
                format!("nothing generates `{name}`"),
                &Source::new(&name, ""),
            )
            .help("delete it, or declare what it belongs to"),
        );
    }
}

/// The first line two texts differ on, as its span in `found` and what was
/// generated in its place.
fn difference(found: &str, generated: &str) -> (diag::Span, String) {
    let mut offset = 0;
    let mut theirs = found.lines();
    let mut ours = generated.lines();
    loop {
        match (theirs.next(), ours.next()) {
            (Some(a), Some(b)) if a == b => offset += a.len() + 1,
            (a, b) => {
                let span = diag::Span {
                    offset,
                    len: a.map_or(0, str::len),
                };
                let label = match b {
                    Some(line) => format!("generated: {line}"),
                    None => "generated: nothing, the file ends above".to_string(),
                };
                return (span, label);
            }
        }
    }
}

/// Every file under `dir`, deepest last and sorted, so a run reports them in
/// the same order twice.
fn tracked(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            out.extend(tracked(&path));
        } else {
            out.push(path);
        }
    }
    out
}

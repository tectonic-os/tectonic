//! The only reader of the image files and the per-module module.kdl files.

pub mod base;
pub mod build;
pub mod command;
pub mod create;
pub mod diag;
pub mod dispatch;
pub mod emit;
pub mod fetch;
pub mod import;
pub mod init;
pub mod key;
pub mod layout;
pub mod model;
pub mod parse;
pub mod prompt;
pub mod provenance;
pub mod provider;
pub mod registry;
pub mod resolve;
pub mod runtime;
pub mod scap;
pub mod set;
pub mod ui;
pub mod upgrade;

pub use command::{Arg, Command};
use diag::Issue;
use diag::Issues;
use diag::Source;
use model::image::{List, Target};
use model::module::Module;
pub use parse::repo::compatible;
use resolve::Resolved;
use std::path::{Path, PathBuf};

/// The nearest directory at or above `from` holding a repo.kdl.
pub fn find_root(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .find(|dir| dir.join(layout::REPO_FILE).is_file())
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
    /// What a read-out says, as data, for a terminal to draw instead of the
    /// markdown in `stdout`.
    pub parts: Vec<emit::Part>,
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
    /// Seeded bases a collection describes instead. Only `check` looks, since
    /// it is the only command a collection's catalog is any of the business of.
    pub shadowed: Vec<base::Shadow>,
    /// Collections following a moving ref, which is what makes the repository
    /// build a different tree tomorrow. `check`'s alone, like `shadowed`.
    pub unpinned: Vec<String>,
    /// Imported modules whose content no longer matches the record beside them.
    /// Forking one is legitimate, so this is a read-out rather than a
    /// diagnostic; `check`'s alone.
    pub modified: Vec<String>,
    /// Who provides what, as the run saw it: `check`'s conformance notice
    /// asks it what would claim a rule the image is missing.
    pub(crate) index: provider::Index,
    /// The reading this ran against, so a caller that needs one after a command
    /// acts on what the command saw rather than reading the tree again.
    pub(crate) list: List,
    /// Beside `list`, and in the same order as its images.
    pub(crate) resolved: Vec<Resolved>,
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
                    .help("the generated module layers go between the two markers"),
            );
            found = false;
        }
    }
    found.then_some(text)
}

/// Every manifest read and every image resolved, which is what any command
/// beyond a name needs.
pub(crate) struct Loaded {
    list: List,
    resolved: Vec<Resolved>,
    workflows: Vec<resolve::workflow::Declared>,
    /// What a workflow body's guarded regions are kept by.
    facts: Vec<&'static str>,
    index: provider::Index,
    issues: Issues,
    context: String,
}

pub(crate) fn load(root: &Path) -> Loaded {
    let (mut list, mut issues) = List::load(root);
    let context = context(&list, root);

    let disk = parse::disk::Disk::scan(root);
    // Costs no network: a collection not on this machine is passed over.
    let index = provider::Index::scan(root, &list.sources, &disk, false);
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
        resolve::graph::check_graph(image, root, &index, &mut issues);
        resolve::graph::check_fragments(image, &mut issues);
        let shipped = resolve::overlay::index(image, &disk);
        resolve::overlay::check(image, &shipped, &mut issues);
        let collected = resolve::collect::resolve_collects(image, root, &disk, &mut issues);

        resolved.push(Resolved { shipped, collected });
    }

    // After the modules, since what a workflow needs is a fact about them.
    let basis = resolve::workflow::Basis::of(&list);
    let workflows = resolve::workflow::resolve(&list, &basis, &mut issues);
    let facts = resolve::workflow::facts(&basis, list.scans_scheduled, list.publishes_scheduled);

    Loaded {
        list,
        resolved,
        workflows,
        facts,
        index,
        issues,
        context,
    }
}

/// Loads the repository, resolves every image, then runs one command over the
/// result. `arg` names the image `section` renders and the target `summary` and
/// `sbom` answer about; the defaults otherwise.
pub fn run(command: Command, arg: Option<&str>, root: &Path) -> Run {
    run_loaded(command, arg, root, load(root))
}

pub(crate) fn run_loaded(command: Command, arg: Option<&str>, root: &Path, loaded: Loaded) -> Run {
    let (target_arg, image_arg, module_arg) = match command.arg() {
        Some(Arg::Target) => (arg, None, None),
        Some(Arg::Module) => (None, None, arg),
        _ => (None, arg, None),
    };
    let Loaded {
        list,
        resolved,
        workflows,
        facts,
        index,
        mut issues,
        context,
    } = loaded;

    let (shadowed, unpinned, modified) = match command {
        Command::Check => (
            base::catalog(root, &list.sources, &mut issues).1,
            list.sources
                .iter()
                .filter(|c| c.unpinned())
                .map(|c| c.name.clone())
                .collect(),
            provenance::record::modified(root),
        ),
        _ => (Vec::new(), Vec::new(), Vec::new()),
    };
    if list.audit_enforce {
        for name in &modified {
            issues.push(
                Issue::new(
                    format!("`{name}` no longer matches the record it was imported with"),
                    &list.repo_src,
                )
                .help(
                    "`audit { enforce #true }` makes a fork an error; re-import it, or drop the \
                     `provenance.kdl` beside its manifest to own the module outright",
                ),
            );
        }
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
    let needs_default = match command {
        Command::Summary | Command::Sbom => target_arg.is_none(),
        Command::Graph | Command::GraphJson | Command::Section => image_arg.is_none(),
        Command::Why | Command::WhyJson => false,
        Command::Coverage | Command::CoverageJson => image_arg.is_none(),
        Command::Plan => true,
        Command::Check | Command::Generate | Command::Verify => false,
    };
    if needs_default {
        if let Some(issue) = list.no_default() {
            issues.push(issue);
        }
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
    if matches!(command, Command::Generate | Command::Verify) {
        for (image, resolved) in list.images.iter().zip(&resolved) {
            if let Some(skeleton) = &skeleton {
                let section = emit::containerfile::section(image, root);
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
            files.push(emit::finalize::script(image, &resolved.collected, root));
            files.extend(emit::graph::files(image));
        }
        files.extend(emit::seed::file(&list));
        files.push((
            PathBuf::from(layout::GENERATED).join("plan.json"),
            emit::plan::build(&list, &resolved, &workflows).render(),
        ));
        files.extend(workflows.iter().map(|declared| {
            (
                PathBuf::from(layout::WORKFLOW_DIR).join(&declared.file),
                emit::workflows::render(declared.body, declared.schedule.as_deref(), &facts),
            )
        }));
    }

    if command == Command::Verify {
        verify(root, &files, &mut issues);
    }

    let mut parts = Vec::new();
    let stdout = match command {
        Command::Plan => emit::plan::build(&list, &resolved, &workflows).render(),
        Command::Summary => target
            .and_then(|name| emit::summary::render(&list, &name))
            .unwrap_or_default(),
        Command::Sbom => target
            .and_then(|name| emit::sbom::build(&list, &name))
            .map(|json| json.render())
            .unwrap_or_default(),
        Command::Graph | Command::GraphJson => match one {
            Some(i) => {
                let graph = emit::graph::of(&list.images[i]);
                match command {
                    Command::Graph => {
                        parts = graph.parts();
                        graph.markdown()
                    }
                    _ => graph.json().render(),
                }
            }
            None => String::new(),
        },
        Command::Section => match one {
            Some(i) => emit::containerfile::section(&list.images[i], root),
            None => String::new(),
        },
        Command::Generate => files
            .iter()
            .map(|(path, _)| format!("{}\n", path.display()))
            .collect(),
        Command::Why | Command::WhyJson => match module_arg {
            Some(given) => match emit::why::matching(&emit::why::known(&list), given).as_slice() {
                [path] => match emit::why::of(&list, path, root) {
                    Some(why) => match command {
                        Command::Why => {
                            parts = why.parts(true);
                            why.markdown()
                        }
                        _ => why.json().render(),
                    },
                    // Listed, but its manifest never loaded — which is already
                    // an issue above. A module that was not read has nothing
                    // to say about where its bytes came from.
                    None => {
                        issues.push(
                            Issue::new(
                                format!("`{path}` is listed but was not read"),
                                &list.repo_src,
                            )
                            .help("the problem reading it is above"),
                        );
                        String::new()
                    }
                },
                [] => {
                    let known = emit::why::display(&emit::why::known(&list));
                    issues.push(
                        Issue::new(
                            format!("`{given}` is not a module this repository lists"),
                            &list.repo_src,
                        )
                        .help(match known.is_empty() {
                            true => "no image lists a module yet".to_string(),
                            false => format!("modules: {}", known.join(", ")),
                        }),
                    );
                    String::new()
                }
                paths => {
                    issues.push(
                        Issue::new(
                            format!("`{given}` names more than one module"),
                            &list.repo_src,
                        )
                        .help(format!("modules: {}", paths.join(", "))),
                    );
                    String::new()
                }
            },
            None => {
                issues.push(
                    Issue::new("`why` needs a module", &list.repo_src)
                        .help("`tect why <module>`, the path an image lists it under"),
                );
                String::new()
            }
        },
        // `coverage` is filled by the caller: it reads a datastream a flag
        // names, and nothing here reads a flag.
        Command::Check | Command::Verify | Command::Coverage | Command::CoverageJson => {
            String::new()
        }
    };

    Run {
        stdout,
        parts,
        files,
        issues,
        context,
        images: list.images.len(),
        modules: list.images.iter().map(|i| i.modules().count()).sum(),
        suppressed: list.images.iter().map(|i| i.suppressed.len()).sum(),
        flavours: list.images.iter().map(|i| i.flavours.len()).sum(),
        shadowed,
        unpinned,
        modified,
        index,
        list,
        resolved,
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

    let stale = tracked(&layout::generated(root))
        .into_iter()
        .filter_map(|path| path.strip_prefix(root).map(Path::to_path_buf).ok())
        .filter(|path| !files.iter().any(|(emitted, _)| emitted == path))
        .chain(resolve::workflow::orphans(root, files));
    for path in stale {
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

/// Writes what `generate` produced, after clearing the directory so an image or
/// a module that is gone leaves with its files. Everything under `generated/`
/// is written from here, and so is every workflow the repository declares,
/// which is what `verify` holds both to. A workflow it no longer declares is
/// removed by name: one the tool does not ship is the repository's own.
pub fn write_generated(root: &Path, files: &[(PathBuf, String)]) -> Result<(), String> {
    let dir = layout::generated(root);
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    for path in resolve::workflow::orphans(root, files) {
        let path = root.join(path);
        std::fs::remove_file(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    }
    for (path, text) in files {
        let path = root.join(path);
        let dir = path.parent().unwrap_or(&path);
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
        std::fs::write(&path, text).map_err(|err| format!("{}: {err}", path.display()))?;
    }
    Ok(())
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
pub(crate) fn tracked(dir: &Path) -> Vec<PathBuf> {
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

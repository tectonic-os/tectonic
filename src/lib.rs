//! The only reader of the image files and the per-component manifest.kdl files.

pub mod base;
pub mod build;
pub mod command;
pub mod copy;
pub mod create;
pub mod diag;
pub mod dispatch;
pub mod emit;
pub mod fetch;
#[cfg(feature = "fuzzing")]
pub mod fuzzing;
pub mod import;
pub mod init;
pub mod key;
pub mod layout;
pub mod model;
pub mod parse;
pub mod provenance;
pub mod provider;
pub mod registry;
pub mod resolve;
pub mod runtime;
pub mod scap;
pub mod set;
pub mod upgrade;
pub mod vm;

#[cfg(test)]
mod screen_tests;

pub use command::{Arg, Command};
use diag::Issue;
use diag::Issues;
use diag::Source;
use model::component::Component;
use model::image::{Entry, Image, List, Target};
pub use parse::repo::compatible;
use resolve::Resolved;
use std::path::{Path, PathBuf};

/// The nearest directory at or above `from` holding a repo.kdl.
pub fn find_root(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .find(|dir| dir.join(layout::REPO_FILE).is_file())
        .map(Path::to_path_buf)
}

/// What the repository declares, with each image's base component loaded and
/// no other component manifest: enough for anything that only needs a name.
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
    /// What a read-out says, as data, for a terminal to draw. `stdout` carries
    /// the markdown.
    pub parts: Vec<emit::Part>,
    /// What `generate` produced, as the path each file is written at relative
    /// to the repository root, and its contents.
    pub files: Vec<(PathBuf, String)>,
    pub issues: Issues,
    /// The files read, for the line a failure ends with.
    pub context: String,
    pub images: usize,
    pub components: usize,
    /// Listed components the base covers, which nothing builds.
    pub suppressed: usize,
    pub flavours: usize,
    /// Collections following a moving ref, which is what makes the repository
    /// build a different tree tomorrow. `check`'s alone.
    pub unpinned: Vec<String>,
    /// Components whose Containerfile fragment a closed script rule does not
    /// reach. The rule is narrower than its name, so `check` says where.
    pub ungoverned: Vec<String>,
    /// Who provides what, as the run saw it: `check`'s conformance notice
    /// asks it what would claim a rule the image is missing.
    pub(crate) index: provider::Index,
    /// The reading this ran against, so a caller that needs one after a command
    /// acts on what the command saw.
    pub(crate) list: List,
    /// Beside `list`, and in the same order as its images.
    pub(crate) resolved: Vec<Resolved>,
}

/// The Containerfile skeleton to splice into. If the repository has none, then
/// this release's skeleton is used.
fn skeleton(
    root: &Path,
    needs_tail: bool,
    needs_luks: bool,
    issues: &mut Issues,
) -> Option<String> {
    use emit::containerfile::{BEGIN, END, SKELETON, TAIL_BEGIN, TAIL_END};

    let text = match std::fs::read_to_string(root.join(SKELETON)) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => init::SKELETON.to_string(),
        Err(err) => {
            issues.push(Issue::new(
                format!("`{SKELETON}` cannot be read: {err}"),
                &Source::new(SKELETON, String::new()),
            ));
            return None;
        }
    };
    let src = Source::new(SKELETON, text.clone());
    let mut found = true;
    let markers = if needs_tail {
        &[BEGIN, END, TAIL_BEGIN, TAIL_END][..]
    } else {
        &[BEGIN, END][..]
    };
    for marker in markers {
        if !text.lines().any(|line| line == *marker) {
            issues.push(
                Issue::new(format!("`{SKELETON}` has no `{marker}` line"), &src)
                    .help("the generated component layers go between the two markers"),
            );
            found = false;
        }
    }
    if needs_tail && !text.contains("--mount=type=tmpfs,target=/run") {
        issues.push(
            Issue::new(
                format!("`{SKELETON}` does not isolate `/run` from the final image"),
                &src,
            )
            .help("mount `/run` as tmpfs in the final `validate-image` RUN before adding a UKI"),
        );
        found = false;
    }
    if needs_luks
        && (!text.contains("ARG LUKS_INITRAMFS=")
            || !text.contains("LUKS_INITRAMFS=\"${LUKS_INITRAMFS}\""))
    {
        issues.push(
            Issue::new(
                format!("`{SKELETON}` does not pass the LUKS initramfs witness"),
                &src,
            )
            .help(
                "add `ARG LUKS_INITRAMFS=\"false\"` and pass `LUKS_INITRAMFS=\"${LUKS_INITRAMFS}\"` to the final `validate-image` RUN",
            ),
        );
        found = false;
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
    /// Components a closed script rule does not reach, gathered as each image
    /// resolved.
    ungoverned: Vec<String>,
}

pub(crate) fn load(root: &Path) -> Loaded {
    let (mut list, mut issues) = List::load(root);
    let mut ungoverned: Vec<String> = Vec::new();
    let context = context(&list, root);

    let disk = parse::disk::Disk::scan(root);
    let index = provider::Index::local(root, &list.sources, &disk);
    provenance::lock::check(root, &mut issues);

    let network = list.network;
    let schema_version = list.schema_version;
    let repo_src = list.repo_src.clone();
    let mut resolved: Vec<Resolved> = Vec::new();
    for image in &mut list.images {
        let mut load = |entry: &Entry, image: &Image| {
            let mut component = Component::load(
                entry,
                image,
                root,
                &disk,
                schema_version,
                &repo_src,
                &mut issues,
            )?;
            component.access = network.access(&component);
            Some(component)
        };
        // Taken out so a diagnostic can still read the image it was declared in.
        let mut entries = std::mem::take(&mut image.entries);
        for entry in &mut entries {
            entry.component = load(entry, image);
        }
        image.entries = entries;
        for issue in resolve::imports::expand(image, &disk, load) {
            issues.push(issue);
        }

        resolve::graph::suppress(image);
        let order = resolve::order::sort(image, &mut issues);
        resolve::order::apply(image, &order);
        resolve::graph::check_graph(image, &index, &mut issues);
        resolve::graph::check_fragments(image, &mut issues);
        provenance::check_network(image, network.scripts, root, &mut issues);
        ungoverned.extend(provenance::ungoverned(image, network.scripts));
        let shipped = resolve::overlay::index(image, &disk);
        resolve::overlay::check(image, &shipped, &mut issues);
        let collected = resolve::collect::resolve_collects(image, root, &disk, &mut issues);

        resolved.push(Resolved { shipped, collected });
    }

    // This runs after the imports, so an imported component counts as listed.
    parse::component::check_unlisted(&list, root, &disk, &mut issues);

    // A component listed by two images is one component to say this about.
    ungoverned.sort();
    ungoverned.dedup();

    // After the components, since what a workflow needs is a fact about them.
    let basis = resolve::workflow::Basis::of(&list);
    let workflows = resolve::workflow::resolve(&list, &basis, &mut issues);
    let facts = resolve::workflow::facts(
        &basis,
        list.scans_scheduled,
        list.publishes_scheduled,
        list.backend,
    );

    Loaded {
        list,
        resolved,
        workflows,
        facts,
        index,
        issues,
        context,
        ungoverned,
    }
}

/// Loads the repository, resolves every image, then runs one command over the
/// result. `arg` names the image `section` renders and the target `summary` and
/// `sbom` answer about; the defaults otherwise.
pub fn run(command: Command, arg: Option<&str>, root: &Path) -> Run {
    run_loaded(command, arg, root, load(root))
}

#[expect(
    clippy::too_many_lines,
    reason = "every command reads the loaded declarations through one sequence"
)]
pub(crate) fn run_loaded(command: Command, arg: Option<&str>, root: &Path, loaded: Loaded) -> Run {
    let (target_arg, image_arg, component_arg) = match command.arg() {
        Some(Arg::Target) => (arg, None, None),
        Some(Arg::Component) => (None, None, arg),
        Some(Arg::Image) | None => (None, arg, None),
    };
    let Loaded {
        list,
        resolved,
        workflows,
        facts,
        index,
        mut issues,
        context,
        ungoverned,
    } = loaded;

    let unpinned = match command {
        Command::Check => {
            scap::check_content(&list, &mut issues);
            list.sources
                .iter()
                .filter(|c| c.unpinned())
                .map(|c| c.name.clone())
                .collect()
        }
        Command::Plan
        | Command::Generate
        | Command::Verify
        | Command::Section
        | Command::Graph
        | Command::GraphJson
        | Command::Summary
        | Command::Sbom
        | Command::Why
        | Command::WhyJson
        | Command::Coverage
        | Command::CoverageJson => Vec::new(),
    };

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
        Command::Graph
        | Command::GraphJson
        | Command::Section
        | Command::Coverage
        | Command::CoverageJson => image_arg.is_none(),
        Command::Plan => true,
        Command::Why | Command::WhyJson | Command::Check | Command::Generate | Command::Verify => {
            false
        }
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

    let needs_tail = list
        .images
        .iter()
        .flat_map(|image| image.components())
        .any(|component| component.fragment_position == model::component::Position::Tail);
    let needs_luks = list.images.iter().any(|image| {
        let entries: Vec<_> = image.entries.iter().collect();
        emit::plan::provides(image, &entries, emit::recipe::LUKS_INITRAMFS)
    });
    let skeleton = skeleton(root, needs_tail, needs_luks, &mut issues);

    let mut files: Vec<(PathBuf, String)> = Vec::new();
    if matches!(command, Command::Generate | Command::Verify) {
        files.extend(emit::LIBRARIES.iter().map(|(name, body)| {
            (
                PathBuf::from(layout::GENERATED)
                    .join(layout::LIB)
                    .join(name),
                (*body).to_string(),
            )
        }));
        files.extend(
            emit::SCRIPTS
                .iter()
                .filter(|script| !root.join(layout::SCRIPTS).join(script.name).is_file())
                .map(|script| {
                    (
                        PathBuf::from(layout::GENERATED)
                            .join(layout::SCRIPTS)
                            .join(script.name),
                        emit::located(root, script.body),
                    )
                }),
        );
        for (image, resolved) in list.images.iter().zip(&resolved) {
            if let Some(skeleton) = &skeleton {
                let (section, tails) = emit::containerfile::sections(image, root);
                files.push((
                    emit::containerfile::path(image),
                    emit::containerfile::file(skeleton, image, &section, &tails),
                ));
            }
            files.extend(emit::component_build::scripts(
                image,
                &resolved.collected,
                root,
            ));
            files.push(emit::finalize::script(image, &resolved.collected, root));
            files.extend(emit::graph::files(image));
        }
        files.extend(emit::seed::file(&list));
        files.extend(emit::renovate::file(&list).map(|text| (emit::renovate::path(), text)));
        files.push((
            PathBuf::from(layout::GENERATED).join("plan.json"),
            emit::plan::build(&list, &resolved, &workflows).render(),
        ));
        files.extend(workflows.iter().map(|declared| {
            (
                PathBuf::from(layout::WORKFLOW_DIR).join(&declared.file),
                emit::located(
                    root,
                    &emit::workflows::render(declared.body, declared.schedule.as_deref(), &facts),
                ),
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
                    Command::Plan
                    | Command::Check
                    | Command::Generate
                    | Command::Verify
                    | Command::Section
                    | Command::GraphJson
                    | Command::Summary
                    | Command::Sbom
                    | Command::Why
                    | Command::WhyJson
                    | Command::Coverage
                    | Command::CoverageJson => graph.json().render(),
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
        Command::Why | Command::WhyJson => match component_arg {
            Some(given) => match emit::why::matching(&emit::why::known(&list), given).as_slice() {
                [path] => match emit::why::of(&list, path, root) {
                    Some(why) => match command {
                        Command::Why => {
                            parts = why.parts(true);
                            why.markdown()
                        }
                        Command::Plan
                        | Command::Check
                        | Command::Generate
                        | Command::Verify
                        | Command::Section
                        | Command::Graph
                        | Command::GraphJson
                        | Command::Summary
                        | Command::Sbom
                        | Command::WhyJson
                        | Command::Coverage
                        | Command::CoverageJson => why.json().render(),
                    },
                    // Listed, but its manifest never loaded — which is already
                    // an issue above. A component that was not read has nothing
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
                            format!("`{given}` is not a component this repository lists"),
                            &list.repo_src,
                        )
                        .help(match known.is_empty() {
                            true => "no image lists a component yet".to_string(),
                            false => format!("components: {}", known.join(", ")),
                        }),
                    );
                    String::new()
                }
                paths => {
                    issues.push(
                        Issue::new(
                            format!("`{given}` names more than one component"),
                            &list.repo_src,
                        )
                        .help(format!("components: {}", paths.join(", "))),
                    );
                    String::new()
                }
            },
            None => {
                issues.push(
                    Issue::new("`why` needs a component", &list.repo_src)
                        .help("`tect why <component>`, the path an image lists it under"),
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
        components: list.images.iter().map(|i| i.components().count()).sum(),
        suppressed: list.images.iter().map(|i| i.suppressed.len()).sum(),
        flavours: list.images.iter().map(|i| i.flavours.len()).sum(),
        unpinned,
        ungoverned,
        index,
        list,
        resolved,
    }
}

/// What `generate` produced, against what is committed under `generated/`: an
/// artifact that differs, one that is not there, and one nothing generates.
fn verify(root: &Path, files: &[(PathBuf, String)], issues: &mut Issues) {
    const HELP: &str = "run `tect generate` and commit what it writes";

    // A repository that has never generated is missing every one of them, and
    // twenty-one copies of one sentence is not twenty-one findings. One says
    // it, and `tect generate` is the answer to all of them.
    let mut absent: Vec<String> = Vec::new();
    for (path, generated) in files {
        let name = path.display().to_string();
        let Ok(found) = std::fs::read_to_string(root.join(path)) else {
            absent.push(name);
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

    match absent.as_slice() {
        [] => {}
        [name] => issues
            .push(Issue::new(format!("`{name}` is not there"), &Source::new(name, "")).help(HELP)),
        // Named for the tree: the finding is about all of them, and pointing
        // the summary line at one arbitrary file reads as that file being the
        // problem.
        many => issues.push(
            Issue::new(
                format!("{} generated files are not there", many.len()),
                &Source::new(layout::GENERATED, ""),
            )
            .help(format!("{HELP}; it writes all {} of them", many.len())),
        ),
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
/// a component that is gone leaves with its files. Everything under `generated/`
/// is written from here, and so is every workflow the repository declares. A
/// workflow it no longer declares is removed by name: one the tool does not
/// ship is the repository's own.
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
    // CI calls the scripts directly, whatever mode the repository committed.
    let scripts = layout::generated(root).join(layout::SCRIPTS);
    for script in emit::SCRIPTS {
        use std::os::unix::fs::PermissionsExt;
        let path = scripts.join(script.name);
        if !path.is_file() {
            continue;
        }
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .map_err(|err| format!("{}: {err}", path.display()))?;
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
/// Joins a list the way a sentence reads it, as `a, b and c`.
pub(crate) fn said(items: &[String]) -> String {
    match items.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tail_and_luks_require_their_skeleton_guards() {
        let root = std::env::temp_dir().join(format!("tect-skeleton-{}", std::process::id()));
        let path = root.join(emit::containerfile::SKELETON);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let stale = init::SKELETON.replace(
            "--mount=type=tmpfs,target=/run",
            "--mount=type=cache,target=/run",
        );
        std::fs::write(&path, stale).unwrap();

        assert!(skeleton(&root, false, false, &mut Issues::default()).is_some());
        let mut issues = Issues::default();
        assert!(skeleton(&root, true, false, &mut issues).is_none());
        assert!(issues
            .findings()
            .iter()
            .any(|message| message.contains("does not isolate `/run`")));

        let stale = init::SKELETON.replace("LUKS_INITRAMFS=\"${LUKS_INITRAMFS}\"", "");
        std::fs::write(&path, stale).unwrap();
        let mut issues = Issues::default();
        assert!(skeleton(&root, false, true, &mut issues).is_none());
        assert!(issues
            .findings()
            .iter()
            .any(|message| message.contains("does not pass the LUKS initramfs witness")));

        std::fs::remove_dir_all(root).unwrap();
    }
}

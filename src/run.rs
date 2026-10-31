//! This module holds what one command produced.
//! It composes the parts so each has one responsibility.

use crate::command::{Arg, Command};
use crate::diag::Issues;
use crate::emit;
use crate::layout;
use crate::model::image::{List, Target};
use crate::provider;
use crate::scap;
use std::path::{Path, PathBuf};

/// This holds what one command produced.
/// The report, the generated files, the problems, the counts and the reading
/// each have one responsibility.
pub struct Run {
    pub report: Report,
    /// What `generate` produced, as the path each file is written at relative
    /// to the repository root, and its contents.
    pub files: Vec<(PathBuf, String)>,
    pub problems: Problems,
    pub counts: Counts,
    pub reading: Reading,
}

/// This holds the text the command writes out.
/// The parts are the terminal form of that text, where a terminal can draw it.
pub struct Report {
    pub stdout: String,
    pub parts: Vec<emit::Part>,
}

/// This holds everything the command found wrong or warns about.
/// The context names the files the command read.
pub struct Problems {
    pub issues: Issues,
    /// The files read, for the line a failure ends with.
    pub context: String,
    /// Collections following a moving ref, which is what makes the repository
    /// build a different tree tomorrow. `check`'s alone.
    pub unpinned: Vec<String>,
    /// Components whose Containerfile fragment a closed script rule does not
    /// reach. The rule is narrower than its name, so `check` says where.
    pub ungoverned: Vec<String>,
}

impl Problems {
    /// Reports the issues against the files they were read from, and answers
    /// whether there were any. The warnings stay for the caller that prints
    /// them.
    pub fn report(&mut self) -> bool {
        std::mem::take(&mut self.issues).report(&self.context)
    }
}

/// This holds the counts `check` reports.
pub struct Counts {
    pub images: usize,
    pub components: usize,
    /// Listed components the base covers, which nothing builds.
    pub suppressed: usize,
    pub flavours: usize,
}

/// This holds the reading the command ran against.
/// A caller that needs one acts on what the command saw.
pub struct Reading {
    pub list: List,
    pub index: provider::Index,
    /// Each image's overlay and collected files, in the order the list holds
    /// its images. `build` derives its plan from this reading.
    pub resolved: Vec<crate::resolve::Resolved>,
}

/// Loads the repository, resolves every image, then runs one command over the
/// result. `arg` names the image `section` renders and the target `summary` and
/// `sbom` answer about; the defaults otherwise.
pub fn run(command: Command, arg: Option<&str>, root: &Path) -> Run {
    run_loaded(command, arg, root, crate::load(root))
}

#[expect(
    clippy::too_many_lines,
    reason = "every command reads the loaded declarations through one sequence"
)]
pub(crate) fn run_loaded(
    command: Command,
    arg: Option<&str>,
    root: &Path,
    loaded: crate::Loaded,
) -> Run {
    let (target_arg, image_arg, component_arg) = match command.arg() {
        Some(Arg::Target) => (arg, None, None),
        Some(Arg::Component) => (None, None, arg),
        Some(Arg::Image) | None => (None, arg, None),
    };
    let crate::Loaded {
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
            crate::diag::Issue::new(
                format!("`{unknown}` is not a declared image"),
                &list.repo_src,
            )
            .help(format!("images: {}", known.join(", "))),
        );
    }

    let targets: Vec<String> = list.targets().iter().map(Target::to_string).collect();
    if let Some(unknown) = target_arg.filter(|name| !targets.iter().any(|have| have == name)) {
        issues.push(
            crate::diag::Issue::new(format!("`{unknown}` is not a build target"), &list.repo_src)
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

    let skeleton = match command {
        Command::Check | Command::Generate | Command::Verify => crate::skeleton(root, &mut issues),
        _ => None,
    };

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
                let regions = emit::containerfile::sections(
                    image,
                    root,
                    list.audit_enforce,
                    &resolved.shipped,
                );
                files.push((
                    emit::containerfile::path(image),
                    emit::containerfile::file(skeleton, image, &regions),
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
        crate::verify(root, &files, &mut issues);
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
            Some(i) => emit::containerfile::section(
                &list.images[i],
                root,
                list.audit_enforce,
                &resolved[i].shipped,
            ),
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
                    // Listed, but its manifest never loaded, which is already
                    // an issue above. A component that was not read has nothing
                    // to say about where its bytes came from.
                    None => {
                        issues.push(
                            crate::diag::Issue::new(
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
                        crate::diag::Issue::new(
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
                        crate::diag::Issue::new(
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
                    crate::diag::Issue::new("`why` needs a component", &list.repo_src)
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

    let counts = Counts {
        images: list.images.len(),
        components: list.images.iter().map(|i| i.components().count()).sum(),
        suppressed: list.images.iter().map(|i| i.suppressed.len()).sum(),
        flavours: list.images.iter().map(|i| i.flavours.len()).sum(),
    };
    Run {
        report: Report { stdout, parts },
        files,
        problems: Problems {
            issues,
            context,
            unpinned,
            ungoverned,
        },
        counts,
        reading: Reading {
            list,
            index,
            resolved,
        },
    }
}

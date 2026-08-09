//! The only reader of the image files and the per-module module.kdl files.

pub mod diag;
pub mod emit;
pub mod init;
pub mod model;
pub mod parse;
pub mod resolve;
pub mod runtime;

use diag::Issue;
use diag::Issues;
use diag::Source;
use model::image::{List, REPO_FILE};
use model::module::Module;
use resolve::Resolved;
use std::path::{Path, PathBuf};

/// The nearest directory at or above `from` holding a repo.kdl.
pub fn find_root(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .find(|dir| dir.join(REPO_FILE).is_file())
        .map(Path::to_path_buf)
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
/// result. `image_arg` names the image `section` renders; the default image
/// otherwise.
pub fn run(command: &str, image_arg: Option<&str>, root: &Path) -> Run {
    let (mut list, mut issues) = List::load(root);
    let context = if list.files.is_empty() {
        root.display().to_string()
    } else {
        list.files.join(", ")
    };

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

    let one = match image_arg {
        Some(id) => list.images.iter().position(|i| i.id == id),
        None => list
            .default_image()
            .and_then(|d| list.images.iter().position(|i| i.id == d.id)),
    };

    let skeleton = skeleton(root, &mut issues);

    let mut files: Vec<(PathBuf, String)> = Vec::new();
    if command == "generate" {
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
        }
    }

    let stdout = match command {
        "plan" => emit::plan::build(&list, &resolved, &workflows).render(),
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
        flavours: list.images.iter().map(|i| i.flavours.len()).sum(),
    }
}

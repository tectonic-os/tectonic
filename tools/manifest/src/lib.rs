//! The only reader of the image files and the per-module module.kdl files.

pub mod asset;
pub mod diag;
pub mod json;
pub mod list;
pub mod module;
pub mod options;
pub mod order;
pub mod overlay;
pub mod plan;
pub mod remote;
pub mod render;
pub mod workflow;

use diag::Issue;
use diag::Issues;
use list::List;
use plan::Resolved;
use std::path::Path;

/// What one command produced: its output, everything wrong with the repository,
/// and the counts `check` reports.
pub struct Run {
    pub stdout: String,
    pub issues: Issues,
    /// The files read, for the line a failure ends with.
    pub context: String,
    pub images: usize,
    pub modules: usize,
    pub flavours: usize,
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

    let workflows = workflow::resolve(&list, root, &mut issues);

    let mut resolved: Vec<Resolved> = Vec::new();
    for image in &mut list.images {
        let mut modules: Vec<module::Module> = image
            .entries
            .iter()
            .filter_map(|entry| module::Module::load(entry, image, root, &mut issues))
            .collect();

        let order = order::sort(image, &modules, &mut issues);
        order::apply(image, &mut modules, &order);
        module::check_graph(&modules, image, root, &mut issues);
        let shipped = overlay::index(&modules, root);
        overlay::check(&modules, &shipped, &mut issues);
        let collected = module::resolve_collects(&modules, root, &mut issues);

        resolved.push(Resolved {
            modules,
            shipped,
            collected,
        });
    }

    if let Some(unknown) = image_arg.filter(|id| !list.images.iter().any(|i| i.id == *id)) {
        let known: Vec<&str> = list.images.iter().map(|i| i.id.as_str()).collect();
        issues.push(
            Issue::new(format!("`{unknown}` is not a declared image"), &list.repo_file, "")
                .help(format!("images: {}", known.join(", "))),
        );
    }

    let one = match image_arg {
        Some(id) => list.images.iter().position(|i| i.id == id),
        None => list
            .default_image()
            .and_then(|d| list.images.iter().position(|i| i.id == d.id)),
    };

    let stdout = match command {
        "plan" => plan::build(&list, &resolved, &workflows).render(),
        "section" => match one {
            Some(i) => render::section(
                &list.images[i],
                &resolved[i].modules,
                &resolved[i].collected,
                root,
                &mut issues,
            ),
            None => String::new(),
        },
        _ => {
            for (i, image) in list.images.iter().enumerate() {
                let _ = render::section(
                    image,
                    &resolved[i].modules,
                    &resolved[i].collected,
                    root,
                    &mut issues,
                );
            }
            String::new()
        }
    };

    Run {
        stdout,
        issues,
        context,
        images: list.images.len(),
        modules: resolved.iter().map(|r| r.modules.len()).sum(),
        flavours: list.images.iter().map(|i| i.flavours.len()).sum(),
    }
}

//! The only reader of the image files and the per-module module.kdl files.

mod asset;
mod diag;
mod json;
mod list;
mod module;
mod options;
mod order;
mod overlay;
mod plan;
mod remote;
mod render;
mod workflow;

use list::List;
use plan::Resolved;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: manifest <command>

  plan [--json]     every fact this repository derives, as one JSON
                    document: the images, each image's targets, and what
                    each target is made of. Read a field out of it rather
                    than deriving anything from a name
  section [image]   the generated Containerfile module section for an
                    image; the default image when none is given
  check             validate every manifest, printing what is wrong

Run from the repository root, or set MANIFEST_ROOT.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match args.first().map(String::as_str) {
        Some("-h") | Some("--help") => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some(c) => c,
        None => {
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    let rest: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    let image_arg = match (command, rest.as_slice()) {
        ("plan", []) | ("plan", ["--json"]) => None,
        ("check", []) => None,
        ("section", []) => None,
        ("section", [image]) => Some(*image),
        ("plan" | "check" | "section", _) => {
            eprintln!("manifest: `{command}` does not take {}", rest.join(" "));
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
        }
        (other, _) => {
            eprintln!("manifest: unknown command `{other}`");
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    let root = PathBuf::from(std::env::var("MANIFEST_ROOT").unwrap_or_else(|_| ".".into()));

    let (mut list, mut issues) = List::load(&root);
    let list_display = if list.files.is_empty() {
        root.display().to_string()
    } else {
        list.files.join(", ")
    };

    let workflows = workflow::resolve(&list, &root, &mut issues);

    let mut resolved: Vec<Resolved> = Vec::new();
    for image in &mut list.images {
        let mut modules: Vec<module::Module> = image
            .entries
            .iter()
            .filter_map(|entry| module::Module::load(entry, image, &root, &mut issues))
            .collect();

        let order = order::sort(image, &modules, &mut issues);
        order::apply(image, &mut modules, &order);
        module::check_graph(&modules, image, &root, &mut issues);
        let shipped = overlay::index(&modules, &root);
        overlay::check(&modules, &shipped, &mut issues);
        let collected = module::resolve_collects(&modules, &root, &mut issues);

        resolved.push(Resolved {
            modules,
            shipped,
            collected,
        });
    }

    if let Some(unknown) = image_arg.filter(|id| !list.images.iter().any(|i| i.id == *id)) {
        let known: Vec<&str> = list.images.iter().map(|i| i.id.as_str()).collect();
        issues.push(
            diag::Issue::new(format!("`{unknown}` is not a declared image"), &list.repo_file, "")
                .help(format!("images: {}", known.join(", "))),
        );
    }

    let one = match image_arg {
        Some(id) => list.images.iter().position(|i| i.id == id),
        None => list
            .default_image()
            .and_then(|d| list.images.iter().position(|i| i.id == d.id)),
    };

    let output = match command {
        "plan" => plan::build(&list, &resolved, &workflows).render(),
        "section" => match one {
            Some(i) => render::section(
                &list.images[i],
                &resolved[i].modules,
                &resolved[i].collected,
                &root,
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
                    &root,
                    &mut issues,
                );
            }
            String::new()
        }
    };

    if issues.report(&list_display) {
        return ExitCode::FAILURE;
    }
    print!("{output}");
    if command == "check" {
        eprintln!(
            "manifest: {} images, {} modules, {} flavours",
            list.images.len(),
            resolved.iter().map(|r| r.modules.len()).sum::<usize>(),
            list.images.iter().map(|i| i.flavours.len()).sum::<usize>()
        );
    }
    ExitCode::SUCCESS
}

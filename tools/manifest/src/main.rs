//! The only reader of the image files and the per-module module.kdl files.

mod asset;
mod diag;
mod list;
mod module;
mod options;
mod order;
mod overlay;
mod remote;
mod render;
mod workflow;

use list::{List, Target};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: manifest <command>

Output is one item per line, in declaration order, except where a command
says otherwise.

A target is `<image>/<flavour>`, with `<image>/none` for the ungated build
that publishes unsuffixed.

  images            every image the repository declares, by machine name,
                    which is what the generator writes one Containerfile
                    per
  default-image     the image a build builds when none is named
  image-name        the image's human name, as os-release NAME
  image-file        the file the image is declared in, which is what an
                    edit to that image has to open
  base-image        the base image reference, which the generated FROM uses
  base-family       the base family every module's `supports` is checked
                    against
  base-provides     every capability the base image itself provides
  base-signatures   every image's base and whether it publishes a cosign
                    signature, pipe separated: image, base, signed. Every
                    image, so a repository building on two bases reports
                    both
  flavours           every declared flavour
  targets           every build target
  default-target    what a build with no target named builds: the default
                    image at its default flavour, or its ungated set when it
                    declares no flavours
  pr-target         the one target a pull request builds
  section [image]   the generated Containerfile module section for an
                    image; the default image when none is given
  summary [target]  what a target is made of, as markdown; every entry
                    when no target is given
  assets [target]   every pinned asset, pipe separated: module, name,
                    manifest, version, sha256, hash source, resolved URL
  remotes           every out-of-tree module pin, pipe separated: name,
                    directory, ref, sha256, resolved URL, subtree path, and
                    the file declaring it
  workflows         every file in .github/workflows/ and whether the
                    declaration says it runs, pipe separated: file,
                    enabled. Undeclared is enabled
  find-provider <abs-path> [target]
                    the module that provides a contract file path; nothing
                    when none does. Per target when one is given, because
                    a path provided only by a gated module is not provided
                    on every target
  owns <abs-path> [target]
                    the module whose files/ overlay puts a path in the
                    image; nothing when none does. Per target for the same
                    reason find-provider is. Overlay-shipped paths only: a
                    package-installed one is not in the index and could
                    not be without rpm -qf on a built image
  secrets [target]  every secret ID an enabled module declares, unique;
                    per target when one is given
  contract-files [target]
                    every contract file path an enabled module provides and
                    the finished image still carries, unique; per target
                    when one is given. Excludes `build-only` paths
  verify-exceptions [target]
                    every systemd-analyze verify diagnostic an enabled
                    module accepts on one of its own units, pipe
                    separated: class, unit; per target when one is given
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
    const PER_TARGET: [&str; 5] = [
        "summary",
        "assets",
        "secrets",
        "contract-files",
        "verify-exceptions",
    ];
    const PER_IMAGE: [&str; 7] = [
        "section",
        "image-name",
        "image-file",
        "base-image",
        "base-family",
        "base-provides",
        "flavours",
    ];
    let path_first = matches!(command, "find-provider" | "owns");
    let takes_name = path_first || PER_TARGET.contains(&command) || PER_IMAGE.contains(&command);
    let max_args = usize::from(path_first) + usize::from(takes_name);
    if args.len() - 1 > max_args {
        eprintln!(
            "manifest: `{command}` takes {}",
            match max_args {
                0 => "no arguments".to_string(),
                1 => "at most one argument".to_string(),
                n => format!("at most {n} arguments"),
            }
        );
        return ExitCode::FAILURE;
    }
    let named = args.get(1 + usize::from(path_first)).map(String::as_str);
    let per_image = PER_IMAGE.contains(&command);
    let target = if per_image { None } else { named };
    let image_arg = if per_image { named } else { None };

    let root = PathBuf::from(std::env::var("MANIFEST_ROOT").unwrap_or_else(|_| ".".into()));

    let (mut list, mut issues) = List::load(&root);
    let list_display = if list.files.is_empty() {
        root.display().to_string()
    } else {
        list.files.join(", ")
    };

    if command == "remotes" {
        let output = render::remotes(&list);
        if issues.report(&list_display) {
            return ExitCode::FAILURE;
        }
        print!("{output}");
        return ExitCode::SUCCESS;
    }

    let workflows = workflow::resolve(&list, &root, &mut issues);
    if command == "workflows" {
        let output = workflow::render(&workflows);
        if issues.report(&list_display) {
            return ExitCode::FAILURE;
        }
        print!("{output}");
        return ExitCode::SUCCESS;
    }

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

    let known: Vec<String> = list.targets().iter().map(Target::to_string).collect();
    let target = target.and_then(|name| {
        let parsed = Target::parse(name).filter(|t| known.iter().any(|have| have == &t.to_string()));
        if parsed.is_none() {
            issues.push(
                diag::Issue::new(format!("`{name}` is not a build target"), &list.repo_file, "")
                .help(format!("targets: {}", known.join(", "))),
            );
        }
        parsed
    });

    let flavour = target.as_ref().map(|t| t.flavour.as_str());

    if let Some(unknown) = image_arg.filter(|id| !list.images().iter().any(|i| i.id == *id)) {
        let known: Vec<&str> = list.images().iter().map(|i| i.id.as_str()).collect();
        issues.push(
            diag::Issue::new(format!("`{unknown}` is not a declared image"), &list.repo_file, "")
            .help(format!("images: {}", known.join(", "))),
        );
    }

    let selected: Vec<usize> = if let Some(t) = &target {
        list.images.iter().position(|i| i.id == t.image).into_iter().collect()
    } else if per_image {
        match image_arg {
            Some(id) => list.images.iter().position(|i| i.id == id).into_iter().collect(),
            None => list
                .default_image()
                .and_then(|d| list.images.iter().position(|i| i.id == d.id))
                .into_iter()
                .collect(),
        }
    } else {
        (0..list.images.len()).collect()
    };
    let one = selected.first().copied();

    let output = match command {
        "images" => lines(list.images.iter().map(|i| i.id.clone())),
        "default-image" => lines(list.default_image().map(|i| i.id.clone())),
        "image-name" => lines(one.map(|i| list.images[i].name.clone())),
        "image-file" => lines(one.map(|i| list.images[i].file.clone())),
        "base-image" => lines(
            one.and_then(|i| list.images[i].base.as_ref())
                .map(|b| b.image.clone()),
        ),
        "base-signatures" => lines(selected.iter().filter_map(|&i| {
            let image = &list.images[i];
            image
                .base
                .as_ref()
                .map(|b| format!("{}|{}|{}", image.id, b.image, b.signed))
        })),
        "base-family" => lines(
            one.and_then(|i| list.images[i].base.as_ref())
                .map(|b| b.family.clone()),
        ),
        "base-provides" => lines(
            one.and_then(|i| list.images[i].base.as_ref())
                .into_iter()
                .flat_map(|b| b.provides.iter())
                .map(|d| d.name.clone()),
        ),
        "flavours" => lines(
            one.into_iter()
                .flat_map(|i| list.images[i].flavours.iter())
                .map(|f| f.name.clone()),
        ),
        "targets" => lines(list.targets().iter().map(Target::to_string)),
        "default-target" => lines(list.default_target().map(|t| t.to_string())),
        "pr-target" => lines(list.pr_target().map(|t| t.to_string())),
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
        "check" => {
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
        "find-provider" => {
            let Some(path) = args.get(1) else {
                eprintln!("manifest: find-provider needs an absolute path");
                return ExitCode::FAILURE;
            };
            over(&selected, |i| {
                render::find_provider(&list.images[i], &resolved[i].modules, path, flavour)
            })
        }
        "owns" => {
            let Some(path) = args.get(1) else {
                eprintln!("manifest: owns needs an absolute path");
                return ExitCode::FAILURE;
            };
            over(&selected, |i| {
                overlay::owns(&resolved[i].modules, &resolved[i].shipped, path, flavour)
            })
        }
        "secrets" => over(&selected, |i| {
            render::secrets(&list.images[i], &resolved[i].modules, flavour)
        }),
        "contract-files" => over(&selected, |i| {
            render::contract_files(&list.images[i], &resolved[i].modules, flavour)
        }),
        "verify-exceptions" => over(&selected, |i| {
            render::verify_exceptions(&list.images[i], &resolved[i].modules, flavour)
        }),
        "summary" => selected
            .iter()
            .map(|&i| {
                let body = render::summary(&list.images[i], &resolved[i].modules, flavour);
                if selected.len() > 1 {
                    format!("## {}\n\n{body}", list.images[i].id)
                } else {
                    body
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        "assets" => over(&selected, |i| {
            render::assets(&list.images[i], &resolved[i].modules, flavour)
        }),
        other => {
            eprintln!("manifest: unknown command `{other}`");
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
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

/// One image, resolved: the manifests its entries name, loaded and checked
/// together, and the two indexes built while doing it.
struct Resolved {
    modules: Vec<module::Module>,
    shipped: overlay::Index,
    collected: std::collections::BTreeMap<String, Vec<(String, String)>>,
}

/// A per-image answer, over however many images the command selected.
fn over(selected: &[usize], mut answer: impl FnMut(usize) -> String) -> String {
    let mut seen: Vec<String> = Vec::new();
    for &index in selected {
        for line in answer(index).lines() {
            if !seen.iter().any(|had| had == line) {
                seen.push(line.to_string());
            }
        }
    }
    lines(seen)
}

fn lines(items: impl IntoIterator<Item = String>) -> String {
    items
        .into_iter()
        .map(|s| s + "\n")
        .collect::<Vec<_>>()
        .concat()
}

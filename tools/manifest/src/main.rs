//! The only reader of modules.kdl and the per-module module.kdl files.

mod asset;
mod diag;
mod list;
mod module;
mod options;
mod order;
mod overlay;
mod remote;
mod render;

use list::List;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: manifest <command>

Output is one item per line, in declaration order, except where a command
says otherwise.

  base-image        the base image reference, which the generated FROM uses
  base-family       the base family every module's `supports` is checked
                    against
  base-provides     every capability the base image itself provides
  flavours           every declared flavour
  default-flavour    the flavour marked default, which builds use when none
                    is given; nothing when no flavours are declared
  pr-flavour         the flavour a pull request builds
  targets           every build target: the ungated `none`, then flavours
  section           the generated Containerfile module section
  summary [target]  what a target is made of, as markdown; every entry
                    when no target is given
  assets [target]   every pinned asset, pipe separated: module, name,
                    manifest, version, sha256, hash source, resolved URL
  remotes           every out-of-tree module pin, pipe separated: name,
                    directory, ref, sha256, resolved URL, subtree path
  find-provider <abs-path> [target]
                    the module that provides a contract file path; nothing
                    when none does. Per target when one is given, because
                    a path provided only by a gated module is not provided
                    on every target
  secrets [target]  every secret ID an enabled module declares, unique;
                    per target when one is given
  contract-files [target]
                    every contract file path an enabled module provides and
                    the finished image still carries, unique; per target
                    when one is given. Excludes `build-only` paths
  check             validate every manifest, printing what is wrong

Run from the repository root, or set TECTONIC_ROOT.
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
    const PER_TARGET: [&str; 4] = ["summary", "assets", "secrets", "contract-files"];
    let path_first = command == "find-provider";
    let max_args = usize::from(path_first) + usize::from(path_first || PER_TARGET.contains(&command));
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
    let target = args.get(1 + usize::from(path_first)).map(String::as_str);

    let root = PathBuf::from(std::env::var("TECTONIC_ROOT").unwrap_or_else(|_| ".".into()));
    let list_path = root.join("modules.kdl");
    let list_display = list_path.display().to_string();

    let (mut list, mut issues) = match List::load(&list_display) {
        Ok(v) => v,
        Err(issue) => {
            let mut issues = diag::Issues::default();
            issues.push(*issue);
            issues.report("modules.kdl");
            return ExitCode::FAILURE;
        }
    };

    if command == "remotes" {
        let output = render::remotes(&list);
        if issues.report(&list_display) {
            return ExitCode::FAILURE;
        }
        print!("{output}");
        return ExitCode::SUCCESS;
    }

    let mut modules: Vec<module::Module> = list
        .entries
        .iter()
        .filter_map(|entry| module::Module::load(entry, &list, &root, &mut issues))
        .collect();

    let order = order::sort(&list, &modules, &mut issues);
    order::apply(&mut list, &mut modules, &order);
    module::check_graph(&modules, &list, &root, &mut issues);
    overlay::check(&modules, &root, &mut issues);
    let collected = module::resolve_collects(&modules, &root, &mut issues);

    if let Some(unknown) = target.filter(|t| !list.targets().iter().any(|have| have == t)) {
        issues.push(
            diag::Issue::new(
                format!("`{unknown}` is not a build target"),
                &list_display,
                &list.text,
            )
            .help(format!("targets: {}", list.targets().join(", "))),
        );
    }

    let output = match command {
        "base-image" => lines(list.base.as_ref().map(|b| b.image.clone())),
        "base-family" => lines(list.base.as_ref().map(|b| b.family.clone())),
        "base-provides" => lines(
            list.base
                .iter()
                .flat_map(|b| b.provides.iter())
                .map(|d| d.name.clone()),
        ),
        "flavours" => lines(list.flavours.iter().map(|f| f.name.clone())),
        "default-flavour" => lines(list.default_flavour().map(str::to_string)),
        "pr-flavour" => lines(list.pr_flavour().map(str::to_string)),
        "targets" => lines(list.targets()),
        "section" | "check" => {
            let section = render::section(&list, &modules, &collected, &root, &mut issues);
            if command == "check" {
                String::new()
            } else {
                section
            }
        }
        "find-provider" => {
            let Some(path) = args.get(1) else {
                eprintln!("manifest: find-provider needs an absolute path");
                return ExitCode::FAILURE;
            };
            render::find_provider(&list, &modules, path, target)
        }
        "secrets" => render::secrets(&list, &modules, target),
        "contract-files" => render::contract_files(&list, &modules, target),
        "summary" => render::summary(&list, &modules, target),
        "assets" => render::assets(&list, &modules, target),
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
            "manifest: {} modules, {} flavours",
            modules.len(),
            list.flavours.len()
        );
    }
    ExitCode::SUCCESS
}

fn lines(items: impl IntoIterator<Item = String>) -> String {
    items
        .into_iter()
        .map(|s| s + "\n")
        .collect::<Vec<_>>()
        .concat()
}

//! The only reader of modules.kdl and the per-module module.kdl files.

mod asset;
mod diag;
mod list;
mod module;
mod options;
mod order;
mod overlay;
mod render;

use list::List;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: manifest <command>

Output is one item per line, in declaration order, except where a command
says otherwise.

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
    const PER_TARGET: [&str; 2] = ["summary", "assets"];
    let target = args.get(1).map(String::as_str);
    if target.is_some() && !PER_TARGET.contains(&command) {
        eprintln!("manifest: `{command}` takes no arguments");
        return ExitCode::FAILURE;
    }
    if args.len() > 2 {
        eprintln!("manifest: `{command}` takes at most one target");
        return ExitCode::FAILURE;
    }

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

    let mut modules: Vec<module::Module> = list
        .entries
        .iter()
        .filter_map(|entry| module::Module::load(entry, &list, &root, &mut issues))
        .collect();

    let order = order::sort(&list, &modules, &mut issues);
    order::apply(&mut list, &mut modules, &order);
    module::check_graph(&modules, &root, &mut issues);
    overlay::check(&modules, &root, &mut issues);
    let collected = module::resolve_collects(&modules, &root, &mut issues);

    let output = match command {
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
        "summary" | "assets" => {
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
            if command == "summary" {
                render::summary(&list, &modules, target)
            } else {
                render::assets(&list, &modules, target)
            }
        }
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

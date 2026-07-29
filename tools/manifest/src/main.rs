//! The only reader of modules.kdl and the per-module module.kdl files.

mod diag;
mod list;
mod module;
mod render;

use list::List;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: manifest <command>

All output is one item per line, in declaration order.

  flavours           every declared flavour
  default-flavour    the flavour marked default, which builds use when none
                    is given; nothing when no flavours are declared
  pr-flavour         the flavour a pull request builds
  targets           every build target: the ungated `none`, then flavours
  section           the generated Containerfile module section
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
    if args.len() > 1 {
        eprintln!("manifest: `{command}` takes no arguments");
        return ExitCode::FAILURE;
    }

    let root = PathBuf::from(std::env::var("TECTONIC_ROOT").unwrap_or_else(|_| ".".into()));
    let list_path = root.join("modules.kdl");
    let list_display = list_path.display().to_string();

    let (list, mut issues) = match List::load(&list_display) {
        Ok(v) => v,
        Err(issue) => {
            let mut issues = diag::Issues::default();
            issues.push(*issue);
            issues.report("modules.kdl");
            return ExitCode::FAILURE;
        }
    };

    let modules: Vec<module::Module> = list
        .entries
        .iter()
        .filter_map(|entry| module::Module::load(entry, &list, &root, &mut issues))
        .collect();
    module::check_graph(&modules, &root, &mut issues);

    let output = match command {
        "flavours" => lines(list.flavours.iter().map(|f| f.name.clone())),
        "default-flavour" => lines(list.default_flavour().map(str::to_string)),
        "pr-flavour" => lines(list.pr_flavour().map(str::to_string)),
        "targets" => lines(list.targets()),
        "section" | "check" => {
            let section = render::section(&list, &root, &mut issues);
            if command == "check" {
                String::new()
            } else {
                section
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

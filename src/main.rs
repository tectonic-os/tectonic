//! Reads the arguments, runs the command, prints what it produced.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: tect [--root <dir>] <command>

  plan [--json]     every fact this repository derives, as one JSON
                    document: the images, each image's targets, and what
                    each target is made of. Read a field out of it rather
                    than deriving anything from a name
  section [image]   the generated Containerfile module section for an
                    image; the default image when none is given
  check             validate every manifest, printing what is wrong

The repository is the nearest directory at or above the working directory
holding a repo.kdl, or `--root`. Data goes to stdout and diagnostics to
stderr; exit 1 is the invocation, exit 2 the repository.
";

/// The invocation is wrong: an unknown command, a bad argument, no repository.
const USAGE_ERROR: u8 = 1;
/// The repository is wrong, and every problem was printed to stderr.
const REPO_ERROR: u8 = 2;

fn usage_error(message: String) -> ExitCode {
    eprintln!("tect: {message}");
    eprint!("{USAGE}");
    ExitCode::from(USAGE_ERROR)
}

/// Removes `--root <dir>` or `--root=<dir>` from the arguments.
fn take_root(args: &mut Vec<String>) -> Result<Option<PathBuf>, String> {
    let mut root = None;
    let mut i = 0;
    while i < args.len() {
        let taken = if let Some(dir) = args[i].strip_prefix("--root=") {
            root = Some(PathBuf::from(dir));
            1
        } else if args[i] == "--root" {
            let dir = args.get(i + 1).ok_or("`--root` takes a directory")?;
            root = Some(PathBuf::from(dir));
            2
        } else {
            i += 1;
            continue;
        };
        args.drain(i..i + taken);
    }
    Ok(root)
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let root_arg = match take_root(&mut args) {
        Ok(root) => root,
        Err(message) => return usage_error(message),
    };

    let command = match args.first().map(String::as_str) {
        Some("-h") | Some("--help") => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some(c) => c,
        None => {
            eprint!("{USAGE}");
            return ExitCode::from(USAGE_ERROR);
        }
    };

    let rest: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    let image_arg = match (command, rest.as_slice()) {
        ("plan", []) | ("plan", ["--json"]) => None,
        ("check", []) => None,
        ("section", []) => None,
        ("section", [image]) => Some(*image),
        ("plan" | "check" | "section", _) => {
            return usage_error(format!("`{command}` does not take {}", rest.join(" ")))
        }
        (other, _) => return usage_error(format!("unknown command `{other}`")),
    };

    let root = match root_arg {
        Some(root) => root,
        None => {
            let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match tect::find_root(&here) {
                Some(root) => root,
                None => {
                    return usage_error(format!(
                        "no repo.kdl in {} or any parent directory",
                        here.display()
                    ))
                }
            }
        }
    };

    let run = tect::run(command, image_arg, &root);

    if run.issues.report(&run.context) {
        return ExitCode::from(REPO_ERROR);
    }
    print!("{}", run.stdout);
    if command == "check" {
        eprintln!(
            "tect: {} images, {} modules, {} flavours",
            run.images, run.modules, run.flavours
        );
    }
    ExitCode::SUCCESS
}

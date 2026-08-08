//! Reads the arguments, runs the command, prints what it produced.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: tect <command>

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
            eprintln!("tect: `{command}` does not take {}", rest.join(" "));
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
        }
        (other, _) => {
            eprintln!("tect: unknown command `{other}`");
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    let root = PathBuf::from(std::env::var("MANIFEST_ROOT").unwrap_or_else(|_| ".".into()));
    let run = tect::run(command, image_arg, &root);

    if run.issues.report(&run.context) {
        return ExitCode::FAILURE;
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

//! Reads the arguments, runs the command, prints what it produced.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: tect [--root <dir>] <command>

  init [name]       write a new repository: the manifests, the module
                    directory and the scaffolding, into `--root`, else a
                    directory named for the image, else here. `--owner`
                    names who it belongs to on github
  plan [--json]     every fact this repository derives, as one JSON
                    document: the images, each image's targets, and what
                    each target is made of. Read a field out of it rather
                    than deriving anything from a name
  section [image]   the generated Containerfile module section for an
                    image; the default image when none is given
  graph [--format md|json]
                    the default image's capability graph: what provides
                    what, what requires it, what only orders against it,
                    and what the base already carries. Markdown, holding a
                    mermaid diagram, unless `json` is asked for
  module import <name>
                    copy one module out of a source collection into
                    modules/<owner>/<name>, where <owner> names the
                    collection repo.kdl declares it in. `<owner>/<name>`
                    picks between two collections that both have it.
                    Listing it in an image is a separate operation
  generate          write the Containerfile per image, the per-module build
                    scripts and both renderings of the graph, under
                    generated/, and list what was written
  verify            re-emit all of that and compare it against what is
                    committed under generated/, naming what differs
  check             validate every manifest, printing what is wrong

Inside a build layer, where the binary is mounted and there is no
repository to read:

  os-release        write the image identity the ARGs carry into
                    /usr/lib/os-release
  fetch <what> <url> <sha256> [target] [extra...]
                    download, verify against the hash, and place it: `file`
                    keeps it, `tree` unpacks it, `bin` installs one
                    executable, `rpm` installs the package
  validate-image    every check a built image has to pass

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

/// Removes `--<flag> <value>` or `--<flag>=<value>` from the arguments.
fn take_flag(args: &mut Vec<String>, flag: &str) -> Result<Option<String>, String> {
    let mut value = None;
    let mut i = 0;
    while i < args.len() {
        let taken = if let Some(v) = args[i].strip_prefix(&format!("--{flag}=")) {
            value = Some(v.to_string());
            1
        } else if args[i] == format!("--{flag}") {
            value = Some(
                args.get(i + 1)
                    .ok_or(format!("`--{flag}` takes a value"))?
                    .clone(),
            );
            2
        } else {
            i += 1;
            continue;
        };
        args.drain(i..i + taken);
    }
    Ok(value)
}

const OWNERSHIP: &str = "your account or org on github (not tectonic-os)";

/// Asks for what no flag gave, when there is someone to ask.
fn ask(question: &str) -> Result<String, String> {
    if !std::io::stdin().is_terminal() {
        return Err(format!("nothing to read an answer from: {question}"));
    }
    print!("{question}: ");
    std::io::stdout().flush().map_err(|err| err.to_string())?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|err| err.to_string())?;
    Ok(answer.trim().to_string())
}

/// Writes the tree, then prints what the tool deliberately does not do: the
/// repository, the remote and the first commit are the user's.
fn init(args: &[&str], root_arg: Option<PathBuf>, owner: Option<String>) -> Result<(), String> {
    let name = match args {
        [] => None,
        [name] => Some((*name).to_string()),
        _ => return Err(format!("`init` takes one name, not {}", args.join(" "))),
    };

    let root = match (&root_arg, &name) {
        (Some(root), _) => root.clone(),
        (None, Some(name)) => PathBuf::from(tect::init::id(name)?),
        (None, None) => PathBuf::from("."),
    };

    let name = match name {
        Some(name) => name,
        None => std::fs::canonicalize(&root)
            .ok()
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| format!("cannot name an image after {}", root.display()))?,
    };

    let owner = match owner {
        Some(owner) => owner,
        None => ask(&format!("owner, {OWNERSHIP}"))?,
    };
    if owner.is_empty() {
        return Err(format!("`--owner` is {OWNERSHIP}"));
    }

    let assets = tect::init::assets()?;
    tect::init::write(&root, &name, &owner, &assets)?;

    let id = tect::init::id(&name)?;
    println!(
        "wrote {} into {}\n\n\
         next, in that directory:\n\
         \x20 git init && git add -A && git commit\n\
         \x20 gh repo create {owner}/{id} --source=. --push\n",
        name,
        root.display()
    );
    Ok(())
}

/// Copies one module in, asking which collection only when a name is in more
/// than one and there is someone to ask. Nothing here touches an image file.
fn import(name: &str, root: &Path) -> Result<ExitCode, String> {
    let (sources, issues, context) = tect::sources(root);
    if issues.report(&context) {
        return Ok(ExitCode::from(REPO_ERROR));
    }

    let found = tect::import::find(root, &sources, name)?;
    let module = tect::import::split(name).1;
    let one = match found.as_slice() {
        [one] => one,
        many => {
            let owners: Vec<&str> = many.iter().map(|f| f.owner.as_str()).collect();
            let listed = owners.join(", ");
            if !std::io::stdin().is_terminal() {
                return Err(format!(
                    "`{module}` is in {listed}; name which one, as `{}/{module}`",
                    owners[0]
                ));
            }
            let answer = ask(&format!("`{module}` is in {listed}; which one"))?;
            many.iter()
                .find(|f| f.owner == answer)
                .ok_or_else(|| format!("`{answer}` is not one of {listed}"))?
        }
    };

    let dest = tect::import::vendor(root, one, module)?;
    println!(
        "imported {}\n\n\
         next, to build it, list it in an image:\n\
         \x20 module \"{}/{module}\"\n",
        dest.display(),
        one.owner
    );
    Ok(ExitCode::SUCCESS)
}

/// Writes what `generate` produced, after clearing the directory so an image or
/// a module that is gone leaves with its files. Everything under `generated/`
/// is written from here, which is what `verify` holds it to.
fn write_generated(root: &Path, files: &[(PathBuf, String)]) -> Result<(), String> {
    let dir = root.join("generated");
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    for (path, text) in files {
        let path = root.join(path);
        let dir = path.parent().unwrap_or(&path);
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
        std::fs::write(&path, text).map_err(|err| format!("{}: {err}", path.display()))?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let root_arg = match take_flag(&mut args, "root") {
        Ok(root) => root.map(PathBuf::from),
        Err(message) => return usage_error(message),
    };
    let owner_arg = match take_flag(&mut args, "owner") {
        Ok(owner) => owner,
        Err(message) => return usage_error(message),
    };
    let format_arg = match take_flag(&mut args, "format") {
        Ok(format) => format,
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

    let command = match (command, format_arg.as_deref()) {
        ("graph", None | Some("md")) => "graph",
        ("graph", Some("json")) => "graph-json",
        ("graph", Some(other)) => {
            return usage_error(format!("`--format` is md or json, not `{other}`"))
        }
        (command, Some(_)) => return usage_error(format!("`{command}` does not take `--format`")),
        (command, None) => command,
    };

    let rest: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    if command == "init" {
        return match init(&rest, root_arg, owner_arg) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => usage_error(message),
        };
    }
    if owner_arg.is_some() {
        return usage_error(format!("`{command}` does not take `--owner`"));
    }

    // The build-layer commands read the image around them, not a repository.
    let in_layer = match command {
        "os-release" => Some(tect::runtime::os_release()),
        "validate-image" => Some(tect::runtime::validate_image()),
        "fetch" => Some(tect::runtime::fetch(&rest)),
        _ => None,
    };
    if let Some(result) = in_layer {
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("tect: {message}");
                ExitCode::from(USAGE_ERROR)
            }
        };
    }

    let image_arg = match (command, rest.as_slice()) {
        ("plan", []) | ("plan", ["--json"]) => None,
        ("check", []) | ("generate", []) | ("verify", []) => None,
        ("section", []) | ("graph" | "graph-json", []) => None,
        ("section", [image]) => Some(*image),
        ("module", ["import", _]) => None,
        ("module", _) => {
            return usage_error(format!(
                "`module` takes `import <name>`, not {}",
                rest.join(" ")
            ))
        }
        ("plan" | "check" | "section" | "graph" | "graph-json" | "generate" | "verify", _) => {
            return usage_error(format!("`{}` does not take {}", args[0], rest.join(" ")))
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

    if command == "module" {
        return match import(rest[1], &root) {
            Ok(code) => code,
            Err(message) => usage_error(message),
        };
    }

    let run = tect::run(command, image_arg, &root);

    if run.issues.report(&run.context) {
        return ExitCode::from(REPO_ERROR);
    }
    if command == "generate" {
        if let Err(message) = write_generated(&root, &run.files) {
            eprintln!("tect: {message}");
            return ExitCode::from(USAGE_ERROR);
        }
    }
    print!("{}", run.stdout);
    if command == "check" {
        eprintln!(
            "tect: {} images, {} modules, {} flavours{}",
            run.images,
            run.modules,
            run.flavours,
            match run.suppressed {
                0 => String::new(),
                n => format!(", {n} the base already provides"),
            }
        );
    }
    if command == "verify" {
        let count = run.files.len();
        eprintln!(
            "tect: {count} generated file{} match the manifests",
            if count == 1 { "" } else { "s" }
        );
    }
    ExitCode::SUCCESS
}

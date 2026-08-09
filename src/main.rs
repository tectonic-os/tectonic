//! Reads the arguments, runs the command, prints what it produced.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tect::prompt::Prompt;
use tect::ui::Choice;

const HEAD: &str = "usage: tect [--root <dir>] <command>\n";

const CREATE_REPO: &str = "\
\x20 create repo [name]  start a repository for your own images, here or in
                      `--root`. `--owner` is your account or org on github
";

const IN_REPO: &str = "\
\x20 create image [name] add an image: what it is called, and what it builds on
  create module [name]
                      write a module, with the packages it installs, and offer
                      to list it in an image
  import module [name]
                      copy a module in from a collection repo.kdl declares,
                      choosing from what they hold, and offer to list it in an
                      image
  check               read every manifest and say what is wrong with it
  generate            write the build files, and list what was written
  section [image]     print the Containerfile section an image generates
  graph [--format md|json]
                      print what provides what, what requires it, and what the
                      base already carries
";

const RULE: &str = "\
Every command takes a flag for everything it needs. What no flag gave is asked
for, and `--no-tui` asks nothing, failing and naming the flag instead.

docs/commands.md is the reference. Data goes to stdout and diagnostics to
stderr; exit 1 is the invocation, exit 2 the repository.
";

/// The invocation is wrong: an unknown command, a bad argument, no repository.
const USAGE_ERROR: u8 = 1;
/// The repository is wrong, and every problem was printed to stderr.
const REPO_ERROR: u8 = 2;

/// Only what can run here: outside a repository that is `create repo`, and the
/// rest is listed as needing one.
fn usage(in_repo: bool) -> String {
    match in_repo {
        true => format!("{HEAD}\n{CREATE_REPO}{IN_REPO}\n{RULE}"),
        false => format!(
            "{HEAD}\n{CREATE_REPO}\nthese need a repository, and there is none here or above:\n\n\
             {IN_REPO}\n{RULE}"
        ),
    }
}

fn in_repo() -> bool {
    std::env::current_dir()
        .ok()
        .and_then(|here| tect::find_root(&here))
        .is_some()
}

/// Removes every `--<flag> <value>` and `--<flag>=<value>` from the arguments.
fn take_flags(args: &mut Vec<String>, flag: &str) -> Result<Vec<String>, String> {
    let mut values = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let taken = if let Some(v) = args[i].strip_prefix(&format!("--{flag}=")) {
            values.push(v.to_string());
            1
        } else if args[i] == format!("--{flag}") {
            values.push(
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
    Ok(values)
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> Result<Option<String>, String> {
    Ok(take_flags(args, flag)?.pop())
}

/// Removes `--<flag>`.
fn take_switch(args: &mut Vec<String>, flag: &str) -> bool {
    let before = args.len();
    args.retain(|arg| arg != &format!("--{flag}"));
    args.len() != before
}

/// A flag the command does not read is a failure rather than a silent no-op.
fn only(given: &[&str], takes: &[&str], command: &str) -> Result<(), String> {
    match given.iter().find(|flag| !takes.contains(flag)) {
        Some(flag) => Err(format!("`{command}` does not take `--{flag}`")),
        None => Ok(()),
    }
}

/// The optional name a `create` takes, and nothing else.
fn one_name(rest: &[&str], command: &str) -> Result<Option<String>, String> {
    match rest {
        [] => Ok(None),
        [name] => Ok(Some((*name).to_string())),
        _ => Err(format!(
            "`{command}` takes one name, not {}",
            rest.join(" ")
        )),
    }
}

/// `--root`, else the nearest directory at or above here holding a repo.kdl.
fn repo_root(given: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(root) = given {
        return Ok(root);
    }
    let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    tect::find_root(&here)
        .ok_or_else(|| format!("no repo.kdl in {} or any parent directory", here.display()))
}

/// A repository this release may not work in, refused before it writes into it.
/// Everything that reads one is refused where it loads it.
fn refused(root: &Path) -> bool {
    tect::compatible(root).report(&root.join("repo.kdl").display().to_string())
}

/// Copies one module in, asking which one when no name was given and which
/// collection when a name is in more than one, then offers it to an image.
fn import(
    name: Option<String>,
    root: &Path,
    image_arg: Option<String>,
    prompt: &Prompt,
) -> Result<ExitCode, String> {
    let (sources, issues, context) = tect::sources(root);
    if issues.report(&context) {
        return Ok(ExitCode::from(REPO_ERROR));
    }

    let name = match name {
        Some(name) => name,
        None => tect::import::choose(root, &sources, prompt)?,
    };
    let found = tect::import::find(root, &sources, &name)?;
    let module = tect::import::split(&name).1;
    let one = match found.as_slice() {
        [one] => one,
        many => {
            let owners: Vec<String> = many.iter().map(|f| f.owner.clone()).collect();
            let listed = owners.join(", ");
            let options: Vec<Choice> = owners.iter().map(|owner| Choice::new(owner, "")).collect();
            let chosen = prompt
                .choose(&format!("`{module}` is in {listed}; which one"), &options)?
                .ok_or_else(|| {
                    format!(
                        "`{module}` is in {listed}; name which one, as `{}/{module}`",
                        owners[0]
                    )
                })?;
            &many[chosen]
        }
    };

    let dest = tect::import::vendor(root, one, module)?;
    println!("imported {}", dest.display());
    tect::create::add_to_image(root, &format!("{}/{module}", one.owner), image_arg, prompt)?;
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
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("tect: {message}");
            eprint!("{}", usage(in_repo()));
            ExitCode::from(USAGE_ERROR)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let prompt = Prompt::new(take_switch(&mut args, "no-tui"));
    let root_arg = take_flag(&mut args, "root")?.map(PathBuf::from);
    let owner = take_flag(&mut args, "owner")?;
    let image_arg = take_flag(&mut args, "image")?;
    let base = take_flag(&mut args, "base")?;
    let format = take_flag(&mut args, "format")?;
    let target = take_flag(&mut args, "target")?;
    let tags = take_flags(&mut args, "tag")?;
    let pkgs = take_flags(&mut args, "pkg")?;
    let with = take_flags(&mut args, "with")?
        .iter()
        .map(|pair| match pair.split_once('=') {
            Some((verb, value)) => Ok((verb.to_string(), value.to_string())),
            None => Err(format!("`--with` is `verb=value`, not `{pair}`")),
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut given: Vec<&str> = Vec::new();
    for (flag, present) in [
        ("root", root_arg.is_some()),
        ("owner", owner.is_some()),
        ("image", image_arg.is_some()),
        ("base", base.is_some()),
        ("format", format.is_some()),
        ("target", target.is_some()),
        ("tag", !tags.is_empty()),
        ("pkg", !pkgs.is_empty()),
        ("with", !with.is_empty()),
    ] {
        if present {
            given.push(flag);
        }
    }

    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.first() {
        None => {
            eprint!("{}", usage(in_repo()));
            return Ok(ExitCode::from(USAGE_ERROR));
        }
        Some(&"-h") | Some(&"--help") => {
            print!("{}", usage(in_repo()));
            return Ok(ExitCode::SUCCESS);
        }
        Some(_) => {}
    }

    if let ["create", "repo", rest @ ..] = words.as_slice() {
        only(&given, &["root", "owner", "image", "base"], "create repo")?;
        let name = one_name(rest, "create repo")?;
        tect::create::repo(name, owner, image_arg, base, root_arg, &prompt)?;
        return Ok(ExitCode::SUCCESS);
    }

    if let ["fetch", "modules"] = words.as_slice() {
        only(&given, &["root"], "fetch modules")?;
        let root = repo_root(root_arg)?;
        let (list, issues, context) = tect::declarations(&root);
        if issues.report(&context) {
            return Ok(ExitCode::from(REPO_ERROR));
        }
        for line in tect::fetch::modules(&root, &list)? {
            eprintln!("tect: {line}");
        }
        return Ok(ExitCode::SUCCESS);
    }

    // The build-layer commands read the image around them, not a repository.
    let in_layer = match words.as_slice() {
        ["os-release"] => Some(tect::runtime::os_release()),
        ["validate-image"] => Some(tect::runtime::validate_image()),
        ["fetch", rest @ ..] => Some(tect::runtime::fetch(rest)),
        _ => None,
    };
    if let Some(result) = in_layer {
        result?;
        return Ok(ExitCode::SUCCESS);
    }

    match words.as_slice() {
        ["create", "image", rest @ ..] => {
            only(&given, &["root", "owner", "base"], "create image")?;
            let name = one_name(rest, "create image")?;
            let root = repo_root(root_arg)?;
            if refused(&root) {
                return Ok(ExitCode::from(REPO_ERROR));
            }
            tect::create::image(
                &root,
                name,
                base,
                owner.as_deref(),
                "a name argument",
                &prompt,
            )?;
            return Ok(ExitCode::SUCCESS);
        }
        ["create", "module", rest @ ..] => {
            only(&given, &["root", "image", "pkg", "with"], "create module")?;
            let name = one_name(rest, "create module")?;
            let root = repo_root(root_arg)?;
            if refused(&root) {
                return Ok(ExitCode::from(REPO_ERROR));
            }
            tect::create::module(&root, name, pkgs, with, image_arg, &prompt)?;
            return Ok(ExitCode::SUCCESS);
        }
        ["import", "module", rest @ ..] => {
            only(&given, &["root", "image"], "import module")?;
            let name = one_name(rest, "import module")?;
            return import(name, &repo_root(root_arg)?, image_arg, &prompt);
        }
        ["registry", "namespace"] => {
            only(&given, &["root"], "registry namespace")?;
            println!("{}", tect::registry::namespace(&repo_root(root_arg)?)?);
            return Ok(ExitCode::SUCCESS);
        }
        ["registry", "ref"] => {
            only(&given, &["root", "target", "tag"], "registry ref")?;
            let root = repo_root(root_arg)?;
            let (list, issues, context) = tect::declarations(&root);
            if issues.report(&context) {
                return Ok(ExitCode::from(REPO_ERROR));
            }
            println!(
                "{}",
                tect::registry::reference(&list, &root, target.as_deref(), tags.last())?
            );
            return Ok(ExitCode::SUCCESS);
        }
        ["registry", ..] => return Err("`registry` takes `namespace` or `ref`".into()),
        ["create", ..] => {
            return Err("`create` takes `repo <name>`, `image <name>` or `module <name>`".into())
        }
        ["import", ..] => return Err("`import` takes `module <name>`".into()),
        _ => {}
    }

    only(
        &given,
        match words[0] {
            "graph" => &["root", "format"],
            _ => &["root"],
        },
        words[0],
    )?;
    let command = match (words[0], format.as_deref()) {
        ("graph", None | Some("md")) => "graph",
        ("graph", Some("json")) => "graph-json",
        ("graph", Some(other)) => {
            return Err(format!("`--format` is md or json, not `{other}`"));
        }
        (command, _) => command,
    };

    let rest = &words[1..];
    let image_arg = match (command, rest) {
        ("plan", []) | ("plan", ["--json"]) => None,
        ("check", []) | ("generate", []) | ("verify", []) => None,
        ("section", []) | ("graph" | "graph-json", []) => None,
        ("section", [image]) => Some(*image),
        ("summary" | "sbom", []) => None,
        ("summary" | "sbom", [target]) => Some(*target),
        (
            "plan" | "check" | "section" | "graph" | "graph-json" | "generate" | "verify"
            | "summary" | "sbom",
            _,
        ) => {
            return Err(format!("`{}` does not take {}", words[0], rest.join(" ")));
        }
        (other, _) => return Err(format!("unknown command `{other}`")),
    };

    let root = repo_root(root_arg)?;
    let run = tect::run(command, image_arg, &root);

    if run.issues.report(&run.context) {
        return Ok(ExitCode::from(REPO_ERROR));
    }
    if command == "generate" {
        write_generated(&root, &run.files)?;
    }
    print!("{}", run.stdout);
    if command == "check" {
        match run.images {
            0 => eprintln!("tect: no image yet; `tect create image <name>` writes one"),
            _ => eprintln!(
                "tect: {} images, {} modules, {} flavours{}",
                run.images,
                run.modules,
                run.flavours,
                match run.suppressed {
                    0 => String::new(),
                    n => format!(", {n} the base already provides"),
                }
            ),
        }
    }
    if command == "generate" && run.files.is_empty() {
        eprintln!("tect: nothing to generate; `tect create image <name>` writes an image");
    }
    if command == "verify" {
        let count = run.files.len();
        eprintln!(
            "tect: {count} generated file{} match the manifests",
            if count == 1 { "" } else { "s" }
        );
    }
    Ok(ExitCode::SUCCESS)
}

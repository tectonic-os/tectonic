//! What each row of the command table does, which is everything the binary
//! is not allowed to decide for itself.

use crate::command::{Spec, Verb};
use crate::prompt::Prompt;
use crate::Command;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The invocation is wrong: an unknown command, a bad argument, no repository.
pub const USAGE_ERROR: u8 = 1;
/// The repository is wrong, and every problem was printed to stderr.
pub const REPO_ERROR: u8 = 2;

/// Why a command did not run. Usage answers an invocation and says nothing
/// about an operation that failed part way, so only the first prints it.
pub enum Error {
    Invocation(String),
    Operation(String),
}

impl Error {
    pub fn message(&self) -> &str {
        match self {
            Self::Invocation(message) | Self::Operation(message) => message,
        }
    }
}

/// Anything the library reports is an operation that failed.
impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Operation(message)
    }
}

/// Every flag the surface takes, as the words gave them. A flag a command does
/// not read is refused before it runs, so a field it never looks at is empty.
pub struct Flags {
    pub root: Option<PathBuf>,
    pub owner: Option<String>,
    pub host: Option<String>,
    pub images: Vec<String>,
    pub module: Option<String>,
    pub cn: Option<String>,
    pub base: Option<String>,
    pub format: Option<String>,
    pub target: Option<String>,
    pub datastream: Option<PathBuf>,
    pub baseline: Option<PathBuf>,
    pub tags: Vec<String>,
    pub kernel: Option<String>,
    pub backend: Option<String>,
    pub oci_output: Option<String>,
    pub secrets: Vec<String>,
    pub pkgs: Vec<String>,
    pub with: Vec<(String, String)>,
    pub cache_to: bool,
    pub no_cache_from: bool,
}

/// The optional name a `create` takes, and nothing else.
fn one_name(rest: &[&str], spec: &Spec) -> Result<Option<String>, Error> {
    match rest {
        [] => Ok(None),
        [name] => Ok(Some((*name).to_string())),
        _ => Err(Error::Invocation(format!(
            "`{}` takes one name, not {}",
            spec.name(),
            rest.join(" ")
        ))),
    }
}

/// `--root`, else the nearest directory at or above here holding a repo.kdl,
/// named the way `--root .` names one: every path a command prints hangs off
/// this, and a person reads `modules/x` rather than where their home is.
fn repo_root(given: Option<PathBuf>) -> Result<PathBuf, Error> {
    if let Some(root) = given {
        return Ok(root);
    }
    let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let found = crate::find_root(&here).ok_or_else(|| {
        Error::Invocation(format!(
            "no repo.kdl in {} or any parent directory",
            here.display()
        ))
    })?;
    Ok(
        match here.strip_prefix(&found).map(|d| d.components().count()) {
            Ok(0) => PathBuf::from("."),
            Ok(up) => (0..up).map(|_| "..").collect(),
            Err(_) => found,
        },
    )
}

/// `why` off the baked documents: the manifest is what the image declares it is
/// made of, the build record what the build resolved. Neither needs a checkout.
fn why_on_host(command: Command, arg: Option<&str>) -> Result<ExitCode, Error> {
    use crate::provenance::build::{MANIFEST, RECORD};

    let Some(path) = arg else {
        return Err(Error::Invocation(
            "`why` needs a module: `tect why <module>`".into(),
        ));
    };
    let (manifest, record) = crate::emit::why::baked(Path::new(MANIFEST), Path::new(RECORD))
        .map_err(|err| {
            Error::Invocation(format!(
                "{err}\n\nno repo.kdl here and no baked manifest either, so there is nothing to \
                 answer from"
            ))
        })?;

    let Some(why) = crate::emit::why::on_host(&manifest, record.as_ref(), path) else {
        let known = crate::emit::why::known_on_host(&manifest);
        return Err(Error::Invocation(format!(
            "`{path}` is not a module this image carries\n\nmodules: {}",
            known.join(", ")
        )));
    };
    print!(
        "{}",
        match command {
            Command::Why => why.markdown(),
            _ => why.json().render(),
        }
    );
    Ok(ExitCode::SUCCESS)
}

/// The repository, or `None` when this release may not work in it and said so.
/// A command that writes is refused here; everything that reads one is refused
/// where it loads it.
fn open(given: Option<PathBuf>) -> Result<Option<PathBuf>, Error> {
    let root = repo_root(given)?;
    let refused =
        crate::compatible(&root).report(&root.join(crate::layout::REPO_FILE).display().to_string());
    Ok((!refused).then_some(root))
}

/// One command, from the row that named it.
pub fn dispatch(
    spec: &Spec,
    rest: &[&str],
    flags: Flags,
    prompt: &Prompt,
) -> Result<ExitCode, Error> {
    let Flags {
        root: root_arg,
        owner,
        host,
        images,
        module: module_arg,
        cn,
        base,
        format,
        target,
        datastream,
        baseline,
        tags,
        kernel,
        backend,
        oci_output,
        secrets,
        pkgs,
        with,
        cache_to,
        no_cache_from,
    } = flags;
    match spec.verb {
        Verb::CreateRepo => {
            let name = one_name(rest, spec)?;
            // The image `create repo` writes is one, so its `--image` is a name.
            let image = images.last().cloned();
            crate::create::Repo::collect(name, host, owner, image, base, root_arg, prompt)?
                .apply()?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::CreateImage => {
            let name = one_name(rest, spec)?;
            let Some(root) = open(root_arg)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            // Every image in a repository shares its URL, so the id in one is
            // the repository's, which is what the tree it sits in is called.
            let repo = crate::create::named_after_root(&root).unwrap_or_default();
            let url = owner.map(|owner| {
                format!(
                    "{}/{repo}",
                    crate::create::origin(crate::create::HOST, &owner)
                )
            });
            let wrote = crate::create::Image::collect(
                &root,
                name,
                base,
                &repo,
                url,
                "a name argument",
                prompt,
            )?
            .apply(&root)?;
            crate::create::report(&root, &wrote);
            Ok(ExitCode::SUCCESS)
        }
        Verb::CreateModule => {
            let name = one_name(rest, spec)?;
            let Some(root) = open(root_arg)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            let wrote = crate::create::Module::collect(&root, name, pkgs, with, images, prompt)?
                .apply(&root)?;
            crate::create::report(&root, &wrote);
            Ok(ExitCode::SUCCESS)
        }
        Verb::CreateKey => {
            let kind = one_name(rest, spec)?;
            let Some(root) = open(root_arg)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            crate::key::Key::collect(&root, kind, module_arg, cn, prompt)?.apply(&root)?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::ImportModule => {
            let name = one_name(rest, spec)?;
            let root = repo_root(root_arg)?;
            let (list, issues, context) = crate::declarations(&root);
            if issues.report(&context) {
                return Ok(ExitCode::from(REPO_ERROR));
            }
            crate::import::Module::collect(
                name,
                &root,
                &list.sources,
                list.audit_enforce,
                images,
                prompt,
            )?
            .apply(&root, &list.sources)?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::FetchModules => {
            let root = repo_root(root_arg)?;
            let (list, issues, context) = crate::declarations(&root);
            if issues.report(&context) {
                return Ok(ExitCode::from(REPO_ERROR));
            }
            for line in crate::fetch::modules(&root, &list)? {
                eprintln!("tect: {line}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Verb::Build => {
            let opts = crate::build::Options {
                target: one_name(rest, spec)?.or(target),
                kernel,
                tags,
                secrets,
                backend,
                oci_output,
                no_cache_from,
                cache_to,
            };
            Ok(match crate::build::run(&repo_root(root_arg)?, &opts)? {
                crate::build::Stopped::Repository => ExitCode::from(REPO_ERROR),
            })
        }
        Verb::RegistryNamespace => {
            println!("{}", crate::registry::namespace(&repo_root(root_arg)?)?);
            Ok(ExitCode::SUCCESS)
        }
        Verb::RegistryRef => {
            let root = repo_root(root_arg)?;
            let (list, issues, context) = crate::declarations(&root);
            if issues.report(&context) {
                return Ok(ExitCode::from(REPO_ERROR));
            }
            println!(
                "{}",
                crate::registry::reference(&list, &root, target.as_deref(), tags.last())?
            );
            Ok(ExitCode::SUCCESS)
        }
        Verb::ScapContent => Ok(
            match crate::scap::content(&repo_root(root_arg)?, target.as_deref())? {
                crate::scap::Verdict::Clean => ExitCode::SUCCESS,
                crate::scap::Verdict::Wrong => ExitCode::from(REPO_ERROR),
            },
        ),
        Verb::Scap => {
            let [arf] = rest else {
                return Err(Error::Invocation(
                    "`scap` takes one report: `tect scap <arf.xml>`, or `scap content`".into(),
                ));
            };
            let opts = crate::scap::Options {
                target,
                datastream,
                baseline,
            };
            Ok(
                match crate::scap::run(&repo_root(root_arg)?, Path::new(arf), &opts)? {
                    crate::scap::Verdict::Clean => ExitCode::SUCCESS,
                    crate::scap::Verdict::Wrong => ExitCode::from(REPO_ERROR),
                },
            )
        }
        // The build-layer commands read the image around them, not a repository.
        Verb::OsRelease => {
            crate::runtime::os_release()?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::BuildRecord => {
            crate::provenance::build::write()?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::ValidateImage => {
            crate::runtime::validate_image()?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::Fetch => {
            crate::runtime::fetch(rest)?;
            Ok(ExitCode::SUCCESS)
        }
        _ => reading(spec, rest, format.as_deref(), root_arg),
    }
}

/// The commands the repository is read for, which is one call into the library
/// and then the counts and read-outs that hang off it.
fn reading(
    spec: &Spec,
    rest: &[&str],
    format: Option<&str>,
    root_arg: Option<PathBuf>,
) -> Result<ExitCode, Error> {
    let command = spec.verb.reads().expect("a command run reads");
    let command = match (command, format) {
        (Command::Graph, None | Some("md")) => Command::Graph,
        (Command::Graph, Some("json")) => Command::GraphJson,
        (Command::Why, None | Some("md")) => Command::Why,
        (Command::Why, Some("json")) => Command::WhyJson,
        (Command::Graph | Command::Why, Some(other)) => {
            return Err(Error::Invocation(format!(
                "`--format` is md or json, not `{other}`"
            )));
        }
        (command, _) => command,
    };

    let arg = match rest {
        [] => None,
        ["--json"] if command == Command::Plan => None,
        [one] if command.arg().is_some() => Some(*one),
        _ => {
            return Err(Error::Invocation(format!(
                "`{}` does not take {}",
                spec.name(),
                rest.join(" ")
            )));
        }
    };

    // `why` is the one command a live host runs, where the image carries the
    // two documents and there is no repository at all.
    if matches!(command, Command::Why | Command::WhyJson) && repo_root(root_arg.clone()).is_err() {
        return why_on_host(command, arg);
    }

    let root = repo_root(root_arg)?;
    let run = crate::run(command, arg, &root);

    if run.issues.report(&run.context) {
        return Ok(ExitCode::from(REPO_ERROR));
    }
    if command == Command::Generate {
        crate::write_generated(&root, &run.files)?;
    }
    print!("{}", run.stdout);
    if command == Command::Check {
        for shadow in &run.shadowed {
            eprintln!(
                "tect: {}/{} replaces the tool's own entry for {}",
                shadow.collection,
                crate::base::BASES_FILE,
                shadow.image
            );
        }
        for name in &run.unpinned {
            eprintln!(
                "tect: `{name}` is unpinned, so an import of it takes whatever its ref holds \
                 then, unverified"
            );
        }
        for name in &run.modified {
            eprintln!("tect: `{name}` has been edited since it was imported");
        }
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
    if command == Command::Generate && run.files.is_empty() {
        eprintln!("tect: nothing to generate; `tect create image <name>` writes an image");
    }
    if command == Command::Verify {
        let count = run.files.len();
        eprintln!(
            "tect: {count} generated file{} match the manifests",
            if count == 1 { "" } else { "s" }
        );
    }
    Ok(ExitCode::SUCCESS)
}

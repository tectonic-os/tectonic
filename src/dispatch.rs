//! What each row of the command table does, which is everything the binary
//! is not allowed to decide for itself.

use crate::command::{self, Context, Spec, Verb};
use crate::copy;
use crate::prompt::Prompt;
use crate::Command;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

/// The invocation is wrong: an unknown command, a bad argument, no repository.
pub const USAGE_ERROR: u8 = 1;
/// The repository is wrong, and every problem was printed to stderr.
pub const REPO_ERROR: u8 = 2;

/// Why a command did not run. Usage answers an invocation and says nothing
/// about an operation that failed part way, so only the first prints it.
pub enum Error {
    /// The words are not a command, which is the one failure the whole command
    /// list answers.
    Usage(String),
    Invocation(String),
    Operation(String),
}

impl Error {
    pub fn message(&self) -> &str {
        match self {
            Self::Usage(message) | Self::Invocation(message) | Self::Operation(message) => message,
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
    pub base_scan: Option<PathBuf>,
    pub tags: Vec<String>,
    pub kernel: Option<String>,
    pub ram: Option<String>,
    pub backend: Option<String>,
    pub oci_output: Option<String>,
    pub secrets: Vec<String>,
    pub pkgs: Vec<String>,
    pub with: Vec<(String, String)>,
    pub cache_to: bool,
    pub no_cache_from: bool,
    pub rebuild: bool,
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

/// The repository the run is in. `Context` already asked where this is; this
/// is the one place that turns *not a repository* into a refusal, so it is
/// also the one place that can name what does run here instead.
fn repo_root(here: &Context) -> Result<PathBuf, Error> {
    let root = match here {
        Context::Repo(root) => root.clone(),
        Context::Host => {
            return Err(Error::Invocation(format!(
                "this is a tectonic image and not a repository, so there is no source tree to \
                 read\n\nhelp: {} off the documents the build baked",
                answers()
            )))
        }
        Context::Loose => {
            return Err(Error::Invocation(format!(
                "no repo.kdl in {} or any parent directory",
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .display()
            )))
        }
    };
    note_pin(&root);
    Ok(root)
}

/// What a booted image can answer, for the refusal above.
fn answers() -> String {
    let rows: Vec<String> = command::on_host()
        .iter()
        .map(|spec| format!("`tect {}`", spec.label()))
        .collect();
    match rows.split_last() {
        Some((last, [])) => format!("{last} answers"),
        Some((last, rest)) => format!("{} and {last} answer", rest.join(", ")),
        None => "nothing answers".to_string(),
    }
}

/// Whether the pin notice is already on stderr, since `repo_root` answers more
/// than once in an invocation and the notice is about the run, not the call.
static PINNED: AtomicBool = AtomicBool::new(false);

/// A repository pinned to another release still reads: `schema-version` is what
/// decides that. What differs is what this release would generate, so the
/// notice names the two commands that settle it and nothing refuses.
fn note_pin(root: &Path) {
    if PINNED.swap(true, Ordering::Relaxed) {
        return;
    }
    if let Some(version) = crate::parse::repo::pinned_elsewhere(root) {
        eprintln!(
            "tect: this repository pins tect {version} and this is {}, so what it generates may \
             differ; `tect generate` writes this release's output, `tect-version` moves the pin, \
             and `scripts/tect.sh` fetches the pinned release",
            crate::model::image::TECT_VERSION
        );
    }
}

/// A host answers for the image that is running and for nothing else, so a
/// target it is given is taken only when it is that one.
fn this_target(named: Option<&str>, scope: &crate::emit::why::Scope) -> Result<(), Error> {
    use crate::emit::why::Scope;
    let Some(named) = named else {
        return Ok(());
    };
    match scope {
        Scope::Built(name) if name == named => Ok(()),
        Scope::Built(name) => Err(Error::Invocation(format!(
            "this image was built as `{name}`, so it cannot answer for `{named}`"
        ))),
        _ => Err(Error::Invocation(format!(
            "the build record does not name a target, so this image cannot answer for `{named}` \
             rather than for itself"
        ))),
    }
}

/// The refusal when the record named no target and the manifest holds more
/// than one, so there is no honest answer to give.
fn unscoped(spec: &Spec) -> String {
    format!(
        "the build record does not name a target and the baked manifest holds more than one, so \
         `{}` cannot say which of them is running\n\nhelp: `tect why <module>` still answers, \
         and says that it read across all of them",
        spec.name()
    )
}

/// What a booted image answers, off the two documents the build baked: the
/// manifest is what it declares it is made of, the record what the build
/// resolved. Neither needs a checkout, and both describe the whole repository,
/// so everything here is scoped to the target the record names — a host
/// read-out that answers across targets describes an image that is not this
/// one.
fn on_host(
    spec: &Spec,
    rest: &[&str],
    format: Option<&str>,
    target: Option<&str>,
    prompt: &Prompt,
) -> Result<ExitCode, Error> {
    use crate::emit::why::{built_as, image_of};
    use crate::provenance::build::{MANIFEST, RECORD};

    let unwanted = || {
        Error::Invocation(format!(
            "`{}` does not take {}",
            spec.name(),
            rest.join(" ")
        ))
    };
    // The manifest as it stands, before anything reads a field out of it.
    if spec.verb == Verb::Plan {
        if !matches!(rest, [] | ["--json"]) {
            return Err(unwanted());
        }
        let raw = std::fs::read_to_string(MANIFEST)
            .map_err(|err| Error::Invocation(format!("{MANIFEST}: {err}")))?;
        print!("{raw}");
        return Ok(ExitCode::SUCCESS);
    }

    let (manifest, record) = crate::emit::why::baked(Path::new(MANIFEST), Path::new(RECORD))
        .map_err(|err| {
            Error::Invocation(format!(
                "{err}\n\nno repo.kdl here and no baked manifest either, so there is nothing to \
                 answer from"
            ))
        })?;
    let (targets, scope) = built_as(&manifest, record.as_ref());

    if spec.verb == Verb::Why {
        return why_on_host(&manifest, record.as_ref(), rest, format, prompt);
    }

    // `summary` names its target as an argument and `scap content` as a flag;
    // either way a host takes it only when it is the one that is running.
    this_target(
        match (spec.verb, rest) {
            (Verb::ScapContent, []) => target,
            (Verb::Summary, []) => None,
            (Verb::Summary, [named]) => Some(*named),
            _ => return Err(unwanted()),
        },
        &scope,
    )?;
    let [target] = targets.as_slice() else {
        return Err(Error::Invocation(unscoped(spec)));
    };
    match spec.verb {
        Verb::Summary => {
            print!("{}", crate::emit::summary::on_host(target));
            Ok(ExitCode::SUCCESS)
        }
        Verb::ScapContent => {
            let image = image_of(&manifest, target).ok_or_else(|| {
                Error::Invocation("the manifest names no image for this target".to_string())
            })?;
            Ok(match crate::scap::content_on_host(image, target)? {
                crate::scap::Verdict::Clean => ExitCode::SUCCESS,
                crate::scap::Verdict::Wrong => ExitCode::from(REPO_ERROR),
            })
        }
        verb => unreachable!("{verb:?} is not answered on a host"),
    }
}

/// The per-module trust read-out off the baked documents. `known_on_host` is
/// scoped the same way `on_host` is, so a name it resolves is one the image
/// carries and the read-out cannot come back empty.
fn why_on_host(
    manifest: &crate::emit::json::Json,
    record: Option<&crate::emit::json::Json>,
    rest: &[&str],
    format: Option<&str>,
    prompt: &Prompt,
) -> Result<ExitCode, Error> {
    let json = match format {
        Some("json") => true,
        None | Some("md") => false,
        Some(other) => {
            return Err(Error::Invocation(format!(
                "`--format` is md or json, not `{other}`"
            )))
        }
    };
    let [given] = rest else {
        return Err(Error::Invocation(
            "`why` needs a module: `tect why <module>`".into(),
        ));
    };

    let known = crate::emit::why::known_on_host(manifest, record);
    let path = match crate::emit::why::matching(&known, given).as_slice() {
        [path] => path.clone(),
        [] => {
            return Err(Error::Invocation(format!(
                "`{given}` is not a module this image carries\n\nmodules: {}",
                crate::emit::why::display(&known).join(", ")
            )))
        }
        paths => {
            return Err(Error::Invocation(format!(
                "`{given}` names more than one module\n\nmodules: {}",
                paths.join(", ")
            )))
        }
    };
    let why = crate::emit::why::on_host(manifest, record, &path)
        .expect("the resolved module is one this image carries");
    let parts = why.parts(true);
    print!(
        "{}",
        match json {
            true => why.json().render(),
            false if prompt.draws() && crate::ui::fits(&parts) => crate::ui::parts(&parts),
            false => why.markdown(),
        }
    );
    Ok(ExitCode::SUCCESS)
}

/// The repository, or `None` when this release may not work in it and said so.
/// A command that writes is refused here; everything that reads one is refused
/// where it loads it.
fn open(here: &Context) -> Result<Option<PathBuf>, Error> {
    let root = repo_root(here)?;
    let refused =
        crate::compatible(&root).report(&root.join(crate::layout::REPO_FILE).display().to_string());
    Ok((!refused).then_some(root))
}

fn collection_repo(here: &Context) -> Result<Option<(PathBuf, crate::model::image::List)>, Error> {
    let root = repo_root(here)?;
    let (list, issues, context) = crate::declarations(&root);
    Ok((!issues.report(&context)).then_some((root, list)))
}

/// One command, from the row that named it.
pub fn dispatch(
    spec: &Spec,
    rest: &[&str],
    flags: Flags,
    prompt: &Prompt,
    here: &Context,
) -> Result<ExitCode, Error> {
    let Flags {
        // `--root` is already folded into `here`; `create repo` may write
        // where there is no repository, so it takes the raw flag.
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
        base_scan,
        tags,
        kernel,
        ram,
        backend,
        oci_output,
        secrets,
        pkgs,
        with,
        cache_to,
        no_cache_from,
        rebuild,
    } = flags;
    // On a host the two baked documents are the whole of what there is to
    // read, and the table's `host` column says which commands answer off them.
    if *here == Context::Host && spec.host {
        return on_host(spec, rest, format.as_deref(), target.as_deref(), prompt);
    }
    match spec.verb {
        Verb::Upgrade => {
            if let [word, ..] = rest {
                return Err(Error::Invocation(format!(
                    "`{}` does not take {word}",
                    spec.name()
                )));
            }
            crate::upgrade::run()?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::CreateRepo => {
            let name = one_name(rest, spec)?;
            // The image `create repo` writes is one, so its `--image` is a name.
            let image = images.last().cloned();
            if let Some(repo) =
                crate::create::Repo::collect(name, host, owner, image, base, root_arg, prompt)?
            {
                repo.apply()?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Verb::CreateImage => {
            let name = one_name(rest, spec)?;
            let Some(root) = open(here)? else {
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
                crate::create::Field::Image,
                None,
                prompt,
            )?
            .apply(&root)?;
            crate::create::report(&root, &wrote);
            Ok(ExitCode::SUCCESS)
        }
        Verb::CreateFlavour => {
            let name = one_name(rest, spec)?;
            let Some(root) = open(here)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            let wrote =
                crate::create::Flavour::collect(&root, name, images, prompt)?.apply(&root)?;
            crate::create::report(&root, &wrote);
            Ok(ExitCode::SUCCESS)
        }
        Verb::CreateModule => {
            let name = one_name(rest, spec)?;
            let Some(root) = open(here)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            let wrote = crate::create::Module::collect(&root, name, pkgs, with, images, prompt)?
                .apply(&root)?;
            crate::create::report(&root, &wrote);
            Ok(ExitCode::SUCCESS)
        }
        Verb::CreateKey => {
            let kind = one_name(rest, spec)?;
            let Some(root) = open(here)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            crate::key::Key::collect(&root, kind, module_arg, cn, prompt)?.apply(&root)?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::ImportModule => {
            let name = one_name(rest, spec)?;
            let Some((root, list)) = collection_repo(here)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            crate::import::Module::collect(
                name,
                &root,
                &list.sources,
                list.audit_enforce,
                images,
                datastream.as_deref(),
                prompt,
            )?
            .apply(&root)?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::CopyModule => {
            let name = one_name(rest, spec)?;
            let Some((root, list)) = collection_repo(here)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            crate::import::Copy::collect(
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
        Verb::SetWorkflows => {
            if let [word, ..] = rest {
                return Err(Error::Invocation(format!(
                    "`{}` does not take {word}",
                    spec.name()
                )));
            }
            let Some(root) = open(here)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            if !prompt.asks() {
                return Err(Error::Invocation(crate::set::BY_HAND.to_string()));
            }
            // The declaration this edits is readable whatever else is wrong
            // with the repository, so the issues are `check`'s rather than
            // this command's.
            let list = crate::load(&root).list;
            let on: Vec<&str> = list.workflows.iter().map(|w| w.name.as_str()).collect();
            let Some(set) = crate::set::Workflows::collect(
                &crate::resolve::workflow::Basis::of(&list),
                &on,
                list.workflows_at,
                list.publishes_scheduled,
                list.scans_scheduled,
                crate::create::Field::Workflows,
                prompt,
            )?
            else {
                return Ok(ExitCode::SUCCESS);
            };
            let wrote = set.apply(&root)?;
            crate::create::report(&root, &wrote);
            println!("\nnext, in {}:\n\x20 tect generate\n", root.display());
            Ok(ExitCode::SUCCESS)
        }
        Verb::SetConforms => {
            let name = one_name(rest, spec)?;
            let Some(root) = open(here)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            if !prompt.asks() {
                return Err(Error::Invocation(crate::set::CONFORMS_BY_HAND.to_string()));
            }
            let Some(set) = crate::set::Conforms::collect(&root, name, datastream, prompt)? else {
                return Ok(ExitCode::SUCCESS);
            };
            let wrote = set.apply(&root)?;
            crate::create::report(&root, &wrote);
            Ok(ExitCode::SUCCESS)
        }
        Verb::SetClaims => {
            let named = one_name(rest, spec)?.ok_or_else(|| {
                Error::Invocation(format!("`{}` takes the module to claim for", spec.name()))
            })?;
            let Some(root) = open(here)? else {
                return Ok(ExitCode::from(REPO_ERROR));
            };
            if !prompt.asks() {
                return Err(Error::Invocation(crate::set::CLAIMS_BY_HAND.to_string()));
            }
            let Some(set) = crate::set::Claims::collect(&root, &named, datastream, prompt)? else {
                return Ok(ExitCode::SUCCESS);
            };
            let wrote = set.apply(&root)?;
            crate::create::report(&root, &wrote);
            Ok(ExitCode::SUCCESS)
        }
        Verb::FetchModules => {
            let root = repo_root(here)?;
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
            Ok(match crate::build::run(&repo_root(here)?, &opts)? {
                crate::build::Stopped::Repository => ExitCode::from(REPO_ERROR),
            })
        }
        // The disk path is the repository's own script, run as it stands.
        Verb::VmBuild | Verb::VmRun | Verb::VmSpawn => {
            let root = repo_root(here)?;
            let opts = crate::vm::Options {
                target,
                image: images.last().cloned(),
                tag: tags.last().cloned(),
                ram,
                rebuild,
            };
            crate::vm::run(&root, spec, one_name(rest, spec)?.as_deref(), &opts, prompt)?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::RegistryNamespace => {
            println!("{}", crate::registry::namespace(&repo_root(here)?)?);
            Ok(ExitCode::SUCCESS)
        }
        Verb::RegistryRef => {
            let root = repo_root(here)?;
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
            match crate::scap::content(&repo_root(here)?, target.as_deref())? {
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
                base: base_scan,
            };
            Ok(
                match crate::scap::run(&repo_root(here)?, Path::new(arf), &opts)? {
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
        _ => reading(spec, rest, format.as_deref(), datastream, prompt, here),
    }
}

/// `coverage`'s read-out onto the run that produced it. The datastream is a
/// flag and `run_loaded` reads none, so this is resolved here — and only from
/// what was passed, never by probing the host, or what the command prints
/// depends on whether SSG is installed.
fn coverage(
    run: &mut crate::Run,
    json: bool,
    arg: Option<&str>,
    datastream: &Path,
) -> Result<(), Error> {
    let content = crate::scap::content_of(datastream)?;

    let image = match arg {
        // An argument naming no image is already an issue on the run.
        Some(id) => match run.list.images.iter().find(|image| image.id == id) {
            Some(image) => image,
            None => return Ok(()),
        },
        None => match run.list.default_image() {
            Some(image) => image,
            None => return Ok(()),
        },
    };
    if image.conforms.is_empty() {
        run.issues.push(
            crate::diag::Issue::new(format!("`{}` declares no `conforms`", image.id), &image.src)
                .at(image.span, "this image is measured against nothing")
                .help(
                    "`conforms \"<profile>\"` is the profile a scan measures it against, and what \
                 this read-out is coverage of",
                ),
        );
        return Ok(());
    }
    let Some(read_out) = crate::emit::coverage::of(image, &content, &run.index) else {
        run.issues.push(
            crate::diag::Issue::new(
                format!(
                    "`{}` conforms to `{}`, which is none of the profiles this datastream carries",
                    image.id, image.conforms
                ),
                &image.src,
            )
            .at(image.span, "this is what it is measured against")
            .help(format!(
                "the datastream carries: {}",
                crate::scap::profile_names(&content)
            )),
        );
        return Ok(());
    };
    let (stdout, parts) = match json {
        true => (read_out.json().render(), read_out.parts()),
        false => (read_out.markdown(), read_out.parts()),
    };
    run.stdout = stdout;
    run.parts = parts;
    Ok(())
}

/// The commands the repository is read for, which is one call into the library
/// and then the counts and read-outs that hang off it.
fn reading(
    spec: &Spec,
    rest: &[&str],
    format: Option<&str>,
    datastream: Option<PathBuf>,
    prompt: &Prompt,
    here: &Context,
) -> Result<ExitCode, Error> {
    let command = spec.verb.reads().expect("a command run reads");
    let command = match (command, format) {
        (Command::Graph, None | Some("md")) => Command::Graph,
        (Command::Graph, Some("json")) => Command::GraphJson,
        (Command::Why, None | Some("md")) => Command::Why,
        (Command::Why, Some("json")) => Command::WhyJson,
        (Command::Coverage, None | Some("md")) => Command::Coverage,
        (Command::Coverage, Some("json")) => Command::CoverageJson,
        (Command::Graph | Command::Why | Command::Coverage, Some(other)) => {
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

    if matches!(command, Command::Coverage | Command::CoverageJson) && datastream.is_none() {
        return Err(Error::Invocation(
            "`coverage` reads the profile out of a SCAP datastream, and never probes this machine \
             for one: `tect coverage --datastream <file>`\n\nhelp: `tect scap content` prints the \
             path a scan of this repository uses"
                .into(),
        ));
    }

    // On a host the two baked documents are the whole of what there is to
    // read, and the table's `host` column says which commands answer off them.

    let root = repo_root(here)?;
    // The two read-outs about one thing: what is named is picked from what
    // there is, where there is a terminal to pick on.
    let picks = matches!(
        command,
        Command::Why | Command::WhyJson | Command::Coverage | Command::CoverageJson
    ) && arg.is_none()
        && prompt.draws();
    let mut chosen = None;
    let mut run = if picks {
        let loaded = crate::load(&root);
        let (question, known, shown) = match command {
            Command::Coverage | Command::CoverageJson => {
                let known: Vec<String> = loaded.list.images.iter().map(|i| i.id.clone()).collect();
                (copy::WHICH_IMAGE, known.clone(), known)
            }
            _ => {
                let known = crate::emit::why::known(&loaded.list);
                let shown = crate::emit::why::display(&known);
                (copy::WHICH_MODULE, known, shown)
            }
        };
        if known.is_empty() {
            crate::run_loaded(command, None, &root, loaded)
        } else {
            let options = shown
                .iter()
                .map(|name| crate::ui::Choice::new(name, ""))
                .collect::<Vec<_>>();
            let Some(at) = prompt.choose(question, &options)? else {
                return Ok(ExitCode::SUCCESS);
            };
            chosen = Some(known[at].clone());
            crate::run_loaded(command, chosen.as_deref(), &root, loaded)
        }
    } else {
        crate::run(command, arg, &root)
    };
    let arg = chosen.as_deref().or(arg);

    if matches!(command, Command::Coverage | Command::CoverageJson) {
        coverage(
            &mut run,
            command == Command::CoverageJson,
            arg,
            datastream
                .as_deref()
                .expect("coverage checked its datastream"),
        )?;
    }
    let problems = run.issues.report(&run.context);
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
                "tect: `{name}` is unpinned, so a fetch from it takes whatever its ref holds \
                 then, unverified"
            );
        }
        if let Some(version) = crate::parse::repo::pinned_unverified(&root) {
            eprintln!(
                "tect: `tect-version` names {version} with no sha256, so the binary it fetches \
                 is whatever the release holds, unverified"
            );
        }
        for name in &run.modified {
            eprintln!("tect: `{name}` has been edited since it was imported");
        }
        for line in crate::scap::conformance(&run.list, &run.index, datastream.as_deref())? {
            eprintln!("tect: {line}");
        }
    }
    if problems {
        return Ok(ExitCode::from(REPO_ERROR));
    }
    if command == Command::Generate {
        crate::write_generated(&root, &run.files)?;
    }
    // A terminal gets the read-out and nothing else, and only while it is wide
    // enough that no table folds a word mid-way; a pipe, a redirect, `--no-tui`
    // and a too-narrow terminal get the markdown a forge would render.
    print!(
        "{}",
        match matches!(command, Command::Graph | Command::Why | Command::Coverage)
            && prompt.draws()
            && crate::ui::fits(&run.parts)
        {
            false => run.stdout,
            true => crate::ui::parts(&run.parts),
        }
    );
    if command == Command::Check {
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
    if matches!(command, Command::Coverage | Command::CoverageJson) {
        if let Some(table) = run.parts.iter().find_map(|part| match part {
            crate::emit::Part::Table(table) => Some(table),
            _ => None,
        }) {
            eprintln!(
                "tect: {} of {} rules are unclaimed",
                table.rows.iter().filter(|(_, defect)| *defect).count(),
                table.rows.len()
            );
        }
        // The `Would claim` column concludes from what was read, so it says
        // what was not.
        match run.index.unsearched() {
            clause if clause.is_empty() => {}
            clause => eprintln!("tect: {clause}"),
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

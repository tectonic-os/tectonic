//! Create commands collect every answer before writing, so no `apply` takes a
//! `Prompt`.

use crate::copy;
use crate::diag::Issues;
use crate::layout;
use crate::model::remote::Kind as SourceKind;
use common::prompt::Prompt;
pub use common::ui::tree::Change;
use common::ui::{Answer, Choice};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// One host feeds the origin and image URLs because the tool has no second
/// forge model.
pub const HOST: &str = "github.com";

const GH_INSTALL: &str = "install gh from https://github.com/cli/cli";

/// The family-adapter role makes a family's package manager usable from
/// a build layer. Every family needs it filled by a different module.
const BUILD_ENVIRONMENT: &str = "build-environment";

pub fn origin(host: &str, owner: &str) -> String {
    format!("https://{host}/{owner}")
}

pub fn named_after_root(root: &Path) -> Option<String> {
    std::fs::canonicalize(root)
        .ok()
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
}

/// Review rows double as re-entry points. Re-entry asks that field and every
/// later field. A gated row must re-ask whether its value exists so the user
/// can change `none` into a value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Name,
    Provider,
    Remote,
    Libraries,
    Image,
    Base,
    Workflows,
    Publish,
    Scans,
    Daily,
    Scripts,
}

/// Flags seed only the first pass. A repeated field opens on its current
/// answer. The root flag persists because it alone locates the tree.
#[derive(Default)]
struct Given {
    name: Option<String>,
    host: Option<String>,
    owner: Option<String>,
    image: Option<String>,
    base: Option<String>,
    root: Option<PathBuf>,
}

pub struct Repo {
    name: String,
    id: String,
    root: PathBuf,
    host: String,
    /// Ownership is absent when the user declines scheduled builds and no
    /// origin is composed.
    owner: Option<String>,
    assets: PathBuf,
    libraries: Libraries,
    image: Option<Image>,
    /// The shared `set workflows` type keeps both commands on the same CI
    /// generator. A repository without an origin cannot run that CI.
    workflows: Option<crate::set::Workflows>,
    scripts: Scripts,
    remote: bool,
    install_gh: bool,
}

impl Repo {
    pub fn collect(
        name: Option<String>,
        host: Option<String>,
        owner: Option<String>,
        image_name: Option<String>,
        base: Option<String>,
        root_arg: Option<PathBuf>,
        prompt: &Prompt,
    ) -> Result<Option<Self>, String> {
        let mut given = Given {
            name,
            host,
            owner,
            image: image_name,
            base,
            root: root_arg,
        };
        let mut repo = Self::ask(Field::Name, &given, None, prompt)?;
        given = Given {
            root: given.root.take(),
            ..Given::default()
        };
        while prompt.draws() {
            let rows = repo.rows();
            let drawn: Vec<(String, String)> = rows
                .iter()
                .map(|(_, label, value)| (label.to_string(), value.clone()))
                .collect();
            // The action row is where the cursor opens, so a short review can
            // be accepted without a walk down every summary row.
            match common::ui::review(
                copy::REVIEW,
                &drawn,
                copy::CREATE,
                copy::REVIEW_KEYS,
                None,
                rows.len(),
            )? {
                None => return Ok(None),
                Some(at) if at == rows.len() => break,
                Some(at) => repo = Self::ask(rows[at].0, &given, Some(&repo), prompt)?,
            }
        }
        Ok(Some(repo))
    }

    /// Re-entry keeps earlier answers and revalidates later answers against the
    /// edited choice.
    fn ask(
        from: Field,
        given: &Given,
        prev: Option<&Self>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
        let name = match prev {
            Some(prev) if from > Field::Name => prev.name.clone(),
            _ => prompt.line(
                given
                    .name
                    .clone()
                    .or_else(|| given.root.as_deref().and_then(named_after_root)),
                copy::REPO_NAME,
                "a name argument",
                "",
                prev.map(|prev| prev.name.as_str()),
            )?,
        };
        let id = crate::init::id(&name)?;
        let root = given.root.clone().unwrap_or_else(|| PathBuf::from(&id));
        refuse_nesting(&root)?;
        let assets = crate::init::assets()?;
        if prev.is_none() {
            println!("Creating {id}...\n");
        }

        let (host, owner) = match prev {
            Some(prev) if from > Field::Provider => (prev.host.clone(), prev.owner.clone()),
            _ => {
                // One decision has one row and one entry point. The gate is
                // asked first, so `provider` can become `none` and back again.
                let configure = given.host.is_some()
                    || given.owner.is_some()
                    || match prev {
                        None => prompt.confirm(copy::SCHEDULED, copy::YES, copy::NO)?,
                        Some(prev) => prompt.confirm_current(
                            copy::SCHEDULED,
                            copy::YES,
                            copy::NO,
                            prev.owner.is_some(),
                        )?,
                    };
                let host = match (configure, given.host.clone()) {
                    (true, None) => choose_host(prev.map(|prev| prev.host.as_str()), prompt)?,
                    (_, given) => given.unwrap_or_else(|| HOST.to_string()),
                };
                let owner = match configure {
                    true => Some(prompt.line(
                        given.owner.clone(),
                        &copy::username(&host),
                        "`--owner`",
                        &format!("{host}/"),
                        prev.and_then(|prev| prev.owner.as_deref()),
                    )?),
                    false => None,
                };
                (host, owner)
            }
        };
        let mut remote = false;
        let mut install_gh = false;
        if let Some(named) = &owner {
            match prev {
                Some(prev) if from > Field::Remote => {
                    remote = prev.remote;
                    install_gh = prev.install_gh;
                }
                _ => {
                    // The `gh` command supports GitHub, so only a GitHub origin
                    // can offer remote repository creation.
                    let offering = host == HOST && prompt.asks();
                    // The origin line belongs to the question above it, which a
                    // re-entry at this row did not ask.
                    if from <= Field::Provider {
                        println!("Added {host}/{named}/{id} as the origin repo");
                        if !offering {
                            println!();
                        }
                    }
                    let asked = match (offering, prev) {
                        (false, _) => false,
                        (true, None) => {
                            prompt.confirm(copy::CREATE_REMOTE, copy::YES, copy::SKIP)?
                        }
                        (true, Some(prev)) => prompt.confirm_current(
                            copy::CREATE_REMOTE,
                            copy::YES,
                            copy::SKIP,
                            prev.remote,
                        )?,
                    };
                    if asked {
                        match (gh_installed(), gh_logged_in()) {
                            (false, _) => {
                                install_gh =
                                    prompt.confirm(copy::NO_GH, copy::YES, copy::SKIP_REMOTE)?
                            }
                            (true, false) => println!(
                                "You will need to login with user '{named}' to create the repo on Github.\n\
                                 You can log in with the following command:\n\
                                 gh auth login\n"
                            ),
                            (true, true) => remote = true,
                        }
                    }
                }
            }
        }
        let url = owner
            .as_deref()
            .map(|owner| format!("{}/{id}", origin(&host, owner)));
        let libraries = match prev {
            Some(prev) if from > Field::Libraries => prev.libraries.clone(),
            _ => Libraries::collect(prev.map(|prev| &prev.libraries), prompt)?,
        };
        let held = prev.and_then(|prev| prev.image.as_ref());
        let image = match prev {
            Some(prev) if from > Field::Base => prev.image.clone(),
            _ => {
                // The `base` row is inside the image, so re-entering at it does
                // not re-ask whether there is one.
                let wanted = given.image.is_some()
                    || from == Field::Base
                    || match prev {
                        None => prompt.confirm(copy::IMAGES, copy::YES, copy::NO)?,
                        Some(prev) => prompt.confirm_current(
                            copy::IMAGES,
                            copy::YES,
                            copy::NO,
                            prev.image.is_some(),
                        )?,
                    };
                match wanted {
                    true => Some(Image::collect(
                        &root,
                        given.image.clone(),
                        given.base.clone(),
                        &name,
                        url,
                        "`--image`",
                        &crate::init::sources_block(&libraries.chosen),
                        from,
                        held,
                        prompt,
                    )?),
                    false => None,
                }
            }
        };
        let workflows = match owner.is_some() {
            false => None,
            true => {
                // The family the base belongs to is what makes a workflow row
                // reachable, so an edited base re-words the rows below it.
                let family = image.as_ref().map_or("", |image| image.family.as_str());
                let basis = crate::resolve::workflow::Basis::scaffolding(family);
                match prev.and_then(|prev| prev.workflows.as_ref()) {
                    Some(held) => held.again(&basis, from, prompt)?,
                    None => crate::set::Workflows::collect(
                        &basis,
                        &crate::set::Workflows::every(&basis),
                        crate::resolve::workflow::DEFAULT_AT,
                        false,
                        false,
                        from,
                        prompt,
                    )?,
                }
            }
        };
        let scripts = match prev {
            Some(prev) if from > Field::Scripts => prev.scripts.clone(),
            _ => {
                let held = prev.map_or(&[SKELETON_FILE][..], |prev| &prev.scripts.chosen[..]);
                Scripts::collect(&root, &[], held, prompt)?.unwrap_or_default()
            }
        };
        Ok(Self {
            name,
            id,
            root,
            host,
            owner,
            assets,
            libraries,
            image,
            workflows,
            scripts,
            remote,
            install_gh,
        })
    }

    /// A gate answered Yes adds no review row. A gate answered No adds a
    /// `none` row that can re-enter the gate.
    fn rows(&self) -> Vec<(Field, &'static str, String)> {
        let mut rows = vec![
            (Field::Name, copy::ROW_NAME, self.name.clone()),
            (
                Field::Provider,
                copy::ROW_PROVIDER,
                match &self.owner {
                    Some(owner) => format!("{}/{owner}", self.host),
                    None => copy::NONE.to_string(),
                },
            ),
        ];
        // The action says what will happen because no other screen text says
        // a remote will be made.
        if self.owner.is_some() && self.host == HOST {
            rows.push((
                Field::Remote,
                copy::ROW_REMOTE,
                match self.remote {
                    true => copy::REMOTE_MADE,
                    false => copy::REMOTE_NOT,
                }
                .to_string(),
            ));
        }
        rows.push(self.libraries.row());
        match &self.image {
            Some(image) => {
                rows.push((Field::Image, copy::ROW_IMAGE, image.name.clone()));
                rows.push((Field::Base, copy::ROW_BASE, image.base.clone()));
            }
            None => rows.push((Field::Image, copy::ROW_IMAGE, copy::NONE.to_string())),
        }
        match (&self.workflows, self.owner.is_some()) {
            (Some(workflows), _) => rows.extend(workflows.rows()),
            (None, true) => rows.push((
                Field::Workflows,
                copy::ROW_WORKFLOWS,
                copy::NONE.to_string(),
            )),
            (None, false) => {}
        }
        rows.push(self.scripts.row());
        rows
    }

    pub fn apply(&self) -> Result<(), String> {
        let mut wrote: Vec<(PathBuf, Change)> = crate::init::write(
            &self.root,
            &self.name,
            &self.assets,
            &crate::init::sources_block(&self.libraries.chosen),
        )?
        .into_iter()
        .map(|path| (path, Change::Created))
        .collect();
        git_init(&self.root)?;
        println!("initialised a git repository in {}", self.root.display());
        if let Some(image) = &self.image {
            wrote.extend(image.apply(&self.root)?);
        }
        if let Some(workflows) = &self.workflows {
            workflows.apply(&self.root)?;
        }
        wrote.extend(self.scripts.apply(&self.root)?);
        if let (true, Some(owner)) = (self.remote, &self.owner) {
            create_remote(owner, &self.id)?;
            println!("created {}/{} on github", owner, self.id);
        }
        report(&self.root, &wrote);

        let Self { host, id, .. } = self;
        let mut next = Vec::new();
        // A process cannot move its parent, so the step it left you one above
        // is the first thing offered.
        if std::fs::canonicalize(&self.root).ok() != std::env::current_dir().ok() {
            next.push(format!("cd {}", self.root.display()));
        }
        next.push("tect generate".to_string());
        next.push("git add -A && git commit".to_string());
        if self.install_gh {
            next.push(GH_INSTALL.to_string());
        }
        if let Some(owner) = &self.owner {
            next.push(match self.remote || host != HOST {
                true => format!(
                    "git remote add origin {}/{id} && git push -u origin main",
                    origin(host, owner)
                ),
                false => format!("gh repo create {owner}/{id} --source=. --push"),
            });
        }
        let next: Vec<String> = next.iter().map(|line| format!("\x20 {line}")).collect();
        println!("\nnext:\n{}\n", next.join("\n"));
        Ok(())
    }
}

const SKELETON_FILE: &str = "Containerfile.skeleton";

/// The libraries a fresh repository reads, which is the one choice every
/// default library is offered under.
#[derive(Clone, Default)]
pub struct Libraries {
    pub(crate) chosen: Vec<&'static crate::base::DefaultLibrary>,
}

impl Libraries {
    /// Every default library, in the order the scaffolded block lists them.
    pub fn every() -> Vec<&'static crate::base::DefaultLibrary> {
        crate::base::DEFAULT_LIBRARIES.iter().collect()
    }

    /// A first run opens on every default. A second opens on the answer held,
    /// so a re-entry at this row does not silently take the defaults back.
    pub fn collect(held: Option<&Libraries>, prompt: &Prompt) -> Result<Self, String> {
        let every = Self::every();
        let options: Vec<Choice> = every
            .iter()
            .map(|library| {
                Choice::new(
                    format!("{} \"{}\"", library.kind.as_str(), library.alias),
                    library.about,
                )
            })
            .collect();
        let on: Vec<usize> = match held {
            Some(held) => every
                .iter()
                .enumerate()
                .filter(|(_, library)| {
                    held.chosen
                        .iter()
                        .any(|held| held.alias == library.alias && held.kind == library.kind)
                })
                .map(|(at, _)| at)
                .collect(),
            None => (0..every.len()).collect(),
        };
        let Answer::Chosen(chosen) = prompt.choose_many(copy::LIBRARIES, &options, &on)? else {
            unreachable!("choose_many answers with a choice or fails")
        };
        Ok(Self {
            chosen: chosen.into_iter().map(|at| every[at]).collect(),
        })
    }

    fn row(&self) -> (Field, &'static str, String) {
        let mut aliases: Vec<&str> = Vec::new();
        for library in &self.chosen {
            if !aliases.contains(&library.alias) {
                aliases.push(library.alias);
            }
        }
        (
            Field::Libraries,
            copy::ROW_LIBRARIES,
            match aliases.is_empty() {
                true => copy::NONE.to_string(),
                false => aliases.join(", "),
            },
        )
    }
}

/// A repository-owned script replaces the matching script supplied by `tect`.
fn offered() -> Vec<(&'static str, &'static str, &'static str)> {
    std::iter::once((
        SKELETON_FILE,
        "the Containerfile around the generated module layers",
        crate::init::SKELETON,
    ))
    .chain(
        crate::emit::SCRIPTS
            .iter()
            .map(|script| (script.name, script.about, script.body)),
    )
    .collect()
}

#[derive(Clone, Default)]
pub struct Scripts {
    chosen: Vec<&'static str>,
}

impl Scripts {
    /// `names` skips the picker. The picker offers only the files that
    /// `scripts/` does not hold yet, and opens on `held`.
    pub fn collect(
        root: &Path,
        names: &[&str],
        held: &[&str],
        prompt: &Prompt,
    ) -> Result<Option<Self>, String> {
        let open: Vec<_> = offered()
            .into_iter()
            .filter(|(name, _, _)| !root.join(layout::SCRIPTS).join(name).exists())
            .collect();
        if !names.is_empty() {
            let chosen = names
                .iter()
                .map(|name| match open.iter().find(|(open, _, _)| open == name) {
                    Some((open, _, _)) => Ok(*open),
                    None if root.join(layout::SCRIPTS).join(name).exists() => {
                        Err(format!("scripts/{name} is already the repository's own"))
                    }
                    None => Err(format!(
                        "`{name}` is not a file tect supplies; it supplies {}",
                        offered()
                            .iter()
                            .map(|(name, _, _)| *name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                })
                .collect::<Result<Vec<_>, String>>()?;
            return Ok(Some(Self { chosen }));
        }
        let options: Vec<Choice> = open
            .iter()
            .map(|(name, about, _)| Choice::new(*name, *about))
            .collect();
        let on: Vec<usize> = open
            .iter()
            .enumerate()
            .filter(|(_, (name, _, _))| held.contains(name))
            .map(|(at, _)| at)
            .collect();
        let Answer::Chosen(chosen) = prompt.choose_many(copy::SCRIPTS, &options, &on)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            chosen: chosen.iter().map(|at| open[*at].0).collect(),
        }))
    }

    fn row(&self) -> (Field, &'static str, String) {
        (
            Field::Scripts,
            copy::ROW_SCRIPTS,
            match self.chosen.is_empty() {
                true => copy::NONE.to_string(),
                false => self.chosen.join(", "),
            },
        )
    }

    /// Copied scripts must agree on the paths they use to call sibling scripts.
    /// Otherwise an unchanged kept script could call a stale sibling path.
    /// A script that the repository already keeps moves with them only if it
    /// still matches what tect supplied. That equality proves the user has not
    /// changed the script.
    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        let kept = |name: &str| root.join(layout::SCRIPTS).join(name);
        let unchanged: Vec<_> = crate::emit::SCRIPTS
            .iter()
            .filter(|script| {
                !self.chosen.contains(&script.name)
                    && std::fs::read_to_string(kept(script.name))
                        .is_ok_and(|held| held == crate::emit::located(root, script.body))
            })
            .map(|script| (script.name, script.body, Change::Updated(String::new())))
            .collect();
        let chosen: Vec<_> = offered()
            .into_iter()
            .filter(|(name, _, _)| self.chosen.contains(name))
            .map(|(name, _, body)| (name, body, Change::Created))
            .collect();
        for (name, body, _) in &chosen {
            crate::init::put(&kept(name), body)?;
        }
        let mut wrote = Vec::new();
        for (name, body, change) in chosen.into_iter().chain(unchanged) {
            let at = kept(name);
            crate::init::put(&at, &crate::emit::located(root, body))?;
            if name.ends_with(".sh") {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o755))
                    .map_err(|err| format!("{}: {err}", at.display()))?;
            }
            wrote.push((PathBuf::from(layout::SCRIPTS).join(name), change));
        }
        Ok(wrote)
    }
}

pub fn report(root: &Path, wrote: &[(PathBuf, Change)]) {
    let declared = crate::model::image::List::load(root).0.id;
    let id = match declared.is_empty() {
        true => named_after_root(root).unwrap_or_default(),
        false => declared,
    };
    common::ui::tree::print(&id, wrote, describe);
}

/// A tree line says what a later step added to an existing file. File-purpose
/// descriptions stay in documentation so every path does not gain a column.
fn describe(_path: &Path, change: Option<&Change>) -> String {
    match change {
        Some(Change::Updated(edit)) => edit.clone(),
        _ => String::new(),
    }
}

fn under(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

/// The host catalog is limited to the forges supported by generated workflows.
fn choose_host(current: Option<&str>, prompt: &Prompt) -> Result<String, String> {
    let options = host_options();
    let at = current.map(|held| usize::from(held != HOST));
    match ask_one(prompt, copy::REPO_HOST, &options, at)? {
        Some(0) | None => Ok(HOST.to_string()),
        _ => prompt.line(
            None,
            copy::FORGEJO_ADDRESS,
            "`--host`",
            "",
            current.filter(|held| *held != HOST),
        ),
    }
}

fn host_options() -> [Choice; 2] {
    [
        Choice::new(HOST, copy::host_github()),
        Choice::new("forgejo", copy::HOST_FORGEJO),
    ]
}

/// A repeated question opens on its current answer. A first question uses its
/// normal default.
fn ask_one(
    prompt: &Prompt,
    question: &str,
    options: &[Choice],
    at: Option<usize>,
) -> Result<Option<usize>, String> {
    match at {
        Some(at) => prompt.choose_current(question, options, at),
        None => prompt.choose(question, options),
    }
}

#[derive(Clone)]
pub struct Image {
    file: PathBuf,
    text: String,
    pub name: String,
    pub base: String,
    /// The base family decides which CI workflows can run.
    pub family: String,
    /// The recorded choice keeps the base-provider offer stable on re-entry.
    took: bool,
    /// An uncatalogued base has no installer bootloader default.
    bootloader: Option<String>,
    /// A second image records the former implicit default so a bare build keeps
    /// its target.
    names_default: Option<String>,
}

impl Image {
    /// A missing image name falls back to the repository name. Existing images
    /// already carry the repository URL. `sources_text` is the `sources` block
    /// a repository not written yet is about to get, which is the only thing a
    /// scaffolded image can offer modules against.
    pub fn collect(
        root: &Path,
        name: Option<String>,
        base: Option<String>,
        repo: &str,
        url: Option<String>,
        flag: &str,
        sources_text: &str,
        from: Field,
        prev: Option<&Self>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
        let name = match prev {
            Some(prev) if from > Field::Image => prev.name.clone(),
            _ => prompt.line(
                name,
                copy::IMAGE_NAME,
                flag,
                "",
                prev.map(|prev| prev.name.as_str())
                    .or_else(|| crate::init::id(repo).is_ok().then_some(repo)),
            )?,
        };
        let took = prev.is_none_or(|prev| prev.took);
        let id = crate::init::id(&name)?;
        let file = root.join(format!("{id}{}", layout::IMAGE_SUFFIX));
        if file.exists() {
            return Err(format!("{} is already there", file.display()));
        }
        let (mut list, _) = crate::model::image::List::load(root);
        let names_default = implicit_default(&list).filter(|was| *was != id);
        let url = url.or_else(|| {
            list.images
                .iter()
                .find(|image| !image.url.is_empty())
                .map(|image| image.url.clone())
        });

        // A repository with no base-image library has no catalog to pick from,
        // and the offer is the one flow that writes one. A repository not
        // written yet has its answer from the libraries question instead.
        if root.join(layout::REPO_FILE).is_file()
            && !list
                .sources
                .iter()
                .any(|source| source.kind == SourceKind::BaseImages)
            && prompt.asks()
            && prompt.confirm(copy::NO_BASE_LIBRARY, copy::YES, copy::NO)?
        {
            if let Some(set) = crate::set::Library::adding(SourceKind::BaseImages, prompt)? {
                report(root, &set.apply(root)?);
                list = crate::model::image::List::load(root).0;
            }
        }
        // A repository that is not written yet gets the block its libraries
        // answer is about to write, which is the only thing `create repo` has
        // to offer modules against. Both halves read the library through its
        // source cache, which a declared library that is not on this machine is
        // fetched into first.
        let scaffolded;
        let sources = match list.sources.is_empty() && !root.join(layout::REPO_FILE).is_file() {
            true => {
                scaffolded = crate::parse::repo::sources_in(sources_text);
                scaffolded.as_slice()
            }
            false => list.sources.as_slice(),
        };
        if let Err(why) = crate::base::fetch(root, sources) {
            eprintln!("tect: {why}; the catalog holds only what is already on this machine");
        }
        let mut catalog_issues = Issues::default();
        let bases = crate::base::catalog(root, sources, &mut catalog_issues);
        if !catalog_issues.is_empty() {
            return Err(catalog_issues.plain());
        }
        let base = match base {
            Some(given) => given,
            None => choose_base(&bases, prev.map(|prev| prev.base.as_str()), prompt)?,
        };
        let base = catalogued(&bases, base);
        let family = match crate::base::find(&bases, &base) {
            Some(known) => known.family.clone(),
            None => prompt.text(
                None,
                copy::BASE_FAMILY,
                "`--base`, naming a base the catalog knows",
                bases.first().map(|base| base.family.as_str()),
            )?,
        };
        let disk = crate::parse::disk::Disk::scan(root);
        let known = crate::base::find(&bases, &base);
        let roles = roles(known);
        let index = crate::provider::Index::scan(root, sources, &disk, false);
        let mut wanted = wanted(&index, &family, &roles);
        // Network is used only while the user can answer the provider question
        // and the local collections leave a required role unresolved. The
        // fetch lands in the repository's own source cache, because a library
        // the answer names is one the repository reads from then on.
        let fetched;
        if wanted.len() < roles.len() && prompt.asks() && !index.unread().is_empty() {
            fetched = crate::provider::Index::scan(root, sources, &disk, true);
            // One line in this tool's voice precedes the offer it narrows. The
            // fetcher's own `curl: (6) Could not resolve host` does not explain
            // the incomplete provider question.
            if let Some(why) = fetched.unreached() {
                eprintln!(
                    "tect: {why}; what is offered below is only what is already on this machine"
                );
            }
            wanted = self::wanted(&fetched, &family, &roles);
        }
        let take_wanted = match wanted.is_empty() {
            true => true,
            false => offer(&base, &wanted, prompt, took)?,
        };
        let seed = match take_wanted {
            true => seeded(&wanted),
            false => String::new(),
        };
        let held = prev
            .filter(|prev| prev.base == base)
            .and_then(|prev| prev.bootloader.as_deref());
        let bootloader = choose_bootloader(known, held, prompt)?;
        let text = image_kdl(
            &name,
            url.as_deref(),
            &base,
            &family,
            known,
            bootloader.as_deref(),
            &seed,
        );
        Ok(Self {
            text,
            file,
            name,
            base,
            family,
            took: wanted.is_empty() || !seed.is_empty(),
            bootloader,
            names_default,
        })
    }

    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        crate::init::put(&self.file, &self.text)?;
        let mut wrote = vec![(under(root, &self.file), Change::Created)];
        if let Some(was) = &self.names_default {
            append_default_image(root, was)?;
            wrote.push((
                under(root, Path::new(layout::REPO_FILE)),
                Change::Updated(format!("{was} set as the default image")),
            ));
        }
        Ok(wrote)
    }
}

/// A repository with one image can omit `default-image`, but adding a second
/// image must preserve the former implicit default.
fn implicit_default(list: &crate::model::image::List) -> Option<String> {
    match (&list.default_image_id, list.images.as_slice()) {
        (None, [only]) => Some(only.id.clone()),
        _ => None,
    }
}

/// The writer appends `default-image` because rewriting `repo.kdl` would lose
/// the user's layout and comments.
fn append_default_image(root: &Path, id: &str) -> Result<(), String> {
    let file = root.join(layout::REPO_FILE);
    let mut text =
        std::fs::read_to_string(&file).map_err(|err| format!("{}: {err}", file.display()))?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("\ndefault-image \"{id}\"\n"));
    std::fs::write(&file, text).map_err(|err| format!("{}: {err}", file.display()))
}

/// Flavours are appended because `default` and `pr-build` must remain explicit
/// edits. A default flavour silently changes what a bare target builds.
pub struct Flavour {
    name: String,
    image: String,
    file: PathBuf,
}

impl Flavour {
    pub fn collect(
        root: &Path,
        name: Option<String>,
        images: Vec<String>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
        let name = prompt.text(name, copy::FLAVOUR_NAME, "a name argument", None)?;

        if !crate::model::image::is_name(&name) {
            return Err(format!(
                "`{name}` must be lowercase letters, digits and dashes, starting with a letter"
            ));
        }
        if name == crate::model::image::NO_FLAVOUR {
            return Err(format!(
                "`{}` is what the ungated build is called, so it is not a flavour name",
                crate::model::image::NO_FLAVOUR
            ));
        }

        let (list, _) = crate::model::image::List::load(root);
        if list.images.is_empty() {
            return Err(
                "no image to add a flavour to; `tect create image <name>` writes one".to_string(),
            );
        }

        let at = match images.len() {
            0 => {
                let options: Vec<Choice> = list
                    .images
                    .iter()
                    .map(|image| match image.name == image.id {
                        true => Choice::new(&image.id, ""),
                        false => Choice::new(&image.id, &image.name),
                    })
                    .collect();
                match prompt.choose(copy::FLAVOUR_IMAGE, &options)? {
                    Some(at) => at,
                    None => return Err(
                        "give `--image`, since nothing can be asked here: which image publishes it"
                            .to_string(),
                    ),
                }
            }
            1 => match list.images.iter().position(|image| image.id == images[0]) {
                Some(at) => at,
                None => {
                    let ids: Vec<&str> =
                        list.images.iter().map(|image| image.id.as_str()).collect();
                    return Err(format!(
                        "`{}` is not a declared image; there is {}",
                        images[0],
                        ids.join(", ")
                    ));
                }
            },
            _ => {
                return Err(
                    "`create flavour` writes into one image; name one with `--image`".to_string(),
                )
            }
        };

        let image = &list.images[at];

        if image.flavours.iter().any(|held| held.name == name) {
            return Err(format!(
                "`{}` already declares a flavour `{name}`",
                image.id
            ));
        }

        Ok(Self {
            name,
            image: image.name.clone(),
            file: PathBuf::from(image.src.name()),
        })
    }

    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        append(&self.file, &self.image, &[("flavours", None)], &self.name)?;
        Ok(vec![(
            under(root, &self.file),
            Change::Updated(format!("{} added to flavours", self.name)),
        )])
    }
}

/// The image reference a typed `--base` stands for: a catalogued base's own
/// reference, whether the name was that reference or the catalog name, and the
/// typed name itself where the catalog does not know it.
fn catalogued(bases: &[crate::base::Base], base: String) -> String {
    crate::base::find(bases, &base).map_or(base, |known| known.image.clone())
}

/// An unknown base stays available because the catalog cannot constrain a base
/// it does not describe.
fn choose_base(
    bases: &[crate::base::Base],
    current: Option<&str>,
    prompt: &Prompt,
) -> Result<String, String> {
    let mut options = base_options(bases);
    let at = current.and_then(|held| bases.iter().position(|base| base.image == held));
    if prompt.draws() && !options.is_empty() {
        options.push(Choice::new(copy::OTHER_BASE, copy::OTHER_BASE_ABOUT));
        return match ask_one(prompt, copy::IMAGE_BASE, &options, at)? {
            Some(chosen) if chosen < bases.len() => Ok(bases[chosen].image.clone()),
            Some(_) => prompt.text(
                None,
                copy::BASE_IMAGE,
                "`--base`",
                bases.first().map(|base| base.image.as_str()),
            ),
            None => unreachable!("a drawn picker either chooses or cancels"),
        };
    }
    match ask_one(prompt, copy::IMAGE_BASE, &options, at)? {
        Some(chosen) => Ok(bases[chosen].image.clone()),
        None => prompt.text(
            None,
            copy::BASE_IMAGE,
            "`--base`",
            bases.first().map(|base| base.image.as_str()),
        ),
    }
}

fn base_options(bases: &[crate::base::Base]) -> Vec<Choice> {
    bases
        .iter()
        .map(|base| Choice::new(&base.image, &base.about))
        .collect()
}

/// A single bootloader uses the base default. Multiple bootloaders require the
/// user's choice and open on that default.
fn choose_bootloader(
    known: Option<&crate::base::Base>,
    held: Option<&str>,
    prompt: &Prompt,
) -> Result<Option<String>, String> {
    let offered = known
        .map(|base| base.bootloaders.as_slice())
        .unwrap_or_default();
    if offered.len() < 2 {
        return Ok(offered.first().cloned());
    }
    let options = bootloader_options(offered);
    let at = held.and_then(|held| offered.iter().position(|name| name == held));
    Ok(prompt
        .choose_current(copy::IMAGE_BOOTLOADER, &options, at.unwrap_or(0))?
        .map(|chosen| offered[chosen].clone()))
}

fn bootloader_options(offered: &[String]) -> Vec<Choice> {
    offered
        .iter()
        .map(|name| match name.as_str() {
            "grub2" => Choice::new(name, copy::BOOTLOADER_GRUB2),
            _ => Choice::new(name, copy::BOOTLOADER_SYSTEMD),
        })
        .collect()
}

/// Module creation keeps repository ownership separate from image listing.
pub struct Module {
    path: String,
    file: PathBuf,
    text: String,
    listing: Listing,
}

impl Module {
    pub fn collect(
        root: &Path,
        name: Option<String>,
        pkgs: Vec<String>,
        with: Vec<(String, String)>,
        images: Vec<String>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
        let list = crate::model::image::List::editable(root).map_err(|issues| issues.plain())?;
        let name = prompt.text(name, copy::MODULE_NAME, "a name argument", None)?;
        let path = name
            .split('/')
            .map(crate::init::id)
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        let file = layout::manifest(root, &path);
        if file.exists() {
            return Err(format!("modules/{path} is already there"));
        }

        let description = match with.iter().any(|(verb, _)| verb == "description") {
            true => String::new(),
            false => prompt.text(
                None,
                copy::MODULE_DESCRIPTION,
                "`--with description=...`",
                Some(""),
            )?,
        };
        let supports: Vec<String> = match with.iter().any(|(verb, _)| verb == "supports") {
            true => Vec::new(),
            false => prompt
                .text(
                    None,
                    copy::MODULE_SUPPORTS,
                    "`--with supports=...`",
                    Some(""),
                )?
                .split_whitespace()
                .map(str::to_string)
                .collect(),
        };
        let pkgs =
            match pkgs.is_empty() && prompt.confirm(copy::MODULE_PACKAGES, copy::YES, copy::NO)? {
                true => prompt
                    .text(None, copy::PACKAGE_NAMES, "`--pkg`", Some(""))?
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
                false => pkgs,
            };

        let text = module_kdl(&description, &supports, &pkgs, &with)?;
        let listing = Listing::collect_from(&list, images, prompt)?;
        listing.refuse_duplicate(&list, &path, None)?;
        Ok(Self {
            path,
            file,
            text,
            listing,
        })
    }

    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        if self.listing.cancelled() {
            return Ok(Vec::new());
        }
        self.listing.validate()?;
        let list = crate::model::image::List::editable(root).map_err(|issues| issues.plain())?;
        crate::init::put(&self.file, &self.text)?;
        let mut wrote = vec![(under(root, &self.file), Change::Created)];
        wrote.extend(self.listing.apply(&list, &self.path)?);
        Ok(wrote)
    }
}

fn module_kdl(
    description: &str,
    supports: &[String],
    pkgs: &[String],
    with: &[(String, String)],
) -> Result<String, String> {
    let mut sections: Vec<String> = vec![format!(
        "schema-version {}",
        crate::model::image::SCHEMA_VERSION
    )];
    if !description.is_empty() {
        sections.push(format!("description \"{}\"", quotable(description)?));
    }
    let mut declarations: Vec<String> = Vec::new();
    if !supports.is_empty() {
        let families = supports
            .iter()
            .map(|family| Ok(format!("\"{}\"", quotable(family)?)))
            .collect::<Result<Vec<_>, String>>()?
            .join(" ");
        declarations.push(format!("supports {families}"));
    }
    for (verb, value) in with {
        declarations.push(format!("{} \"{}\"", quotable(verb)?, quotable(value)?));
    }
    if !declarations.is_empty() {
        sections.push(declarations.join("\n"));
    }
    if !pkgs.is_empty() {
        let listed = pkgs
            .iter()
            .map(|pkg| Ok(format!("\"{}\"", quotable(pkg)?)))
            .collect::<Result<Vec<_>, String>>()?
            .join(" ");
        sections.push(format!("packages {listed}"));
    }
    Ok(format!("{}\n", sections.join("\n\n")))
}

fn quotable(value: &str) -> Result<&str, String> {
    match value.contains(['"', '\\', '\n']) {
        true => Err(format!(
            "`{value}` is not writable into a manifest as it is"
        )),
        false => Ok(value),
    }
}

/// Image listing remains a user decision even when the repository has one
/// image.
pub enum Listing {
    Cancelled,
    NoImage,
    /// Declining every image is a valid answer. `asked` is false when no image
    /// exists and the response must name the flag.
    Declined {
        asked: bool,
    },
    In(Vec<Listed>),
}

pub struct Listed {
    file: PathBuf,
    image: String,
    flavour: Option<String>,
}

impl Listing {
    /// Duplicate checks run per module because one answer can select several
    /// images.
    pub fn collect(root: &Path, given: Vec<String>, prompt: &Prompt) -> Result<Self, String> {
        let list = crate::model::image::List::editable(root).map_err(|issues| issues.plain())?;
        Self::collect_from(&list, given, prompt)
    }

    fn collect_from(
        list: &crate::model::image::List,
        given: Vec<String>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
        if list.images.is_empty() {
            return Ok(Self::NoImage);
        }
        let targets = list.targets();
        let named: Vec<String> = targets.iter().map(ToString::to_string).collect();

        let chosen: Vec<usize> = match given.is_empty() {
            true => match ask(&list, &targets, prompt)? {
                common::ui::Answer::Cancelled => return Ok(Self::Cancelled),
                common::ui::Answer::Chosen(chosen) => chosen,
            },
            false => given
                .iter()
                .map(|name| {
                    named.iter().position(|known| known == name).ok_or_else(|| {
                        format!(
                            "`{name}` is not a declared image; there is {}",
                            named.join(", ")
                        )
                    })
                })
                .collect::<Result<_, _>>()?,
        };
        let mut unique = Vec::new();
        for at in chosen {
            if !unique.contains(&at) {
                unique.push(at);
            }
        }
        let chosen = unique;
        // The ungated entry is already in every flavour. The widget makes the
        // pair unreachable. A flag and the numbered list do not.
        let ungated = |target: &crate::model::image::Target| {
            chosen.iter().any(|at| {
                targets[*at].image == target.image
                    && targets[*at].flavour == crate::model::image::NO_FLAVOUR
            })
        };
        if let Some(gated) = chosen
            .iter()
            .map(|at| &targets[*at])
            .find(|target| target.flavour != crate::model::image::NO_FLAVOUR && ungated(target))
        {
            return Err(format!(
                "`{gated}` is inside `{}`, so listing it in both lists it twice",
                gated.image
            ));
        }

        Ok(match chosen.is_empty() {
            true => Self::Declined {
                asked: prompt.asks(),
            },
            false => Self::In(
                chosen
                    .iter()
                    .map(|at| {
                        let target = &targets[*at];
                        let image = list
                            .images
                            .iter()
                            .find(|image| image.id == target.image)
                            .expect("a target names an image the list holds");
                        Listed {
                            file: PathBuf::from(image.src.name()),
                            image: image.name.clone(),
                            flavour: match target.flavour == crate::model::image::NO_FLAVOUR {
                                true => None,
                                false => Some(target.flavour.clone()),
                            },
                        }
                    })
                    .collect(),
            ),
        })
    }

    /// Duplicate checks compare `name` and `source` within each selected image.
    /// A module gated to two flavours conflicts only where the flavours overlap.
    pub fn refuse_duplicate(
        &self,
        list: &crate::model::image::List,
        name: &str,
        source: Option<&str>,
    ) -> Result<(), String> {
        let Self::In(listed) = self else {
            return Ok(());
        };
        let dir = dir_of(name, source);
        let target = |image: &crate::model::image::Image, flavour: &Option<String>| {
            crate::model::image::Target {
                image: image.id.clone(),
                flavour: flavour
                    .clone()
                    .unwrap_or_else(|| crate::model::image::NO_FLAVOUR.to_string()),
            }
        };
        for into in listed {
            let Some((image, held)) = holds(list, into, name, source) else {
                continue;
            };
            let (at, into) = (target(image, &held.flavour), target(image, &into.flavour));
            // When the spellings differ, the path distinguishes an owned
            // `dev-tools` module from an existing `.remote` entry.
            let elsewhere = match held.dir() == dir {
                true => String::new(),
                false => format!(", as {}/{}", crate::layout::MODULES, held.dir()),
            };
            return Err(match at.to_string() == into.to_string() {
                true => format!("`{at}` already lists `{name}`{elsewhere}"),
                false => {
                    format!("`{at}` already lists `{name}`{elsewhere}, so `{into}` lists it twice")
                }
            });
        }
        Ok(())
    }

    /// Existing declarations are skipped per image, so every returned image
    /// file was updated by this application. A declaration in one image does
    /// not suppress insertion into another image.
    pub fn apply(
        &self,
        list: &crate::model::image::List,
        path: &str,
    ) -> Result<Vec<(PathBuf, Change)>, String> {
        self.apply_declaration(list, &[(path, None)])
    }

    pub fn cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    fn validate(&self) -> Result<(), String> {
        let Self::In(listed) = self else {
            return Ok(());
        };
        for target in listed {
            let text = std::fs::read_to_string(&target.file)
                .map_err(|err| format!("{}: {err}", target.file.display()))?;
            if crate::parse::image::block_close(&text, &target.image, &[]).is_none() {
                return Err(format!(
                    "{} declares no image `{}`",
                    target.file.display(),
                    target.image
                ));
            }
        }
        Ok(())
    }

    pub fn images(&self) -> Vec<&str> {
        let Self::In(listed) = self else {
            return Vec::new();
        };
        let mut out: Vec<&str> = Vec::new();
        for target in listed {
            if !out.contains(&target.image.as_str()) {
                out.push(&target.image);
            }
        }
        out
    }

    pub(crate) fn targets(&self) -> Vec<(&str, Option<&str>)> {
        let Self::In(listed) = self else {
            return Vec::new();
        };
        listed
            .iter()
            .map(|target| (target.image.as_str(), target.flavour.as_deref()))
            .collect()
    }

    /// Batch application preserves member order and collection ownership.
    /// An existing member is skipped only in the image that already declares
    /// it.
    pub(crate) fn apply_declaration(
        &self,
        list: &crate::model::image::List,
        declarations: &[(&str, Option<&str>)],
    ) -> Result<Vec<(PathBuf, Change)>, String> {
        match self {
            Self::Cancelled => Ok(Vec::new()),
            Self::NoImage => {
                println!("no image lists it yet; `tect create image <name>` writes one");
                Ok(Vec::new())
            }
            Self::Declined { .. } => {
                for (name, source) in declarations {
                    let shown = wrap(&listed_in(None, *source)[1..], &leaf(name));
                    println!(
                        "next, to build it, list it in an image:\n {}",
                        shown.replace('\n', "\n ")
                    );
                }
                Ok(Vec::new())
            }
            Self::In(listed) => {
                let mut wrote: Vec<(PathBuf, Vec<String>)> = Vec::new();
                for target in listed {
                    let mut taken: Vec<String> = Vec::new();
                    for (name, source) in declarations {
                        if holds(list, target, name, *source).is_some() {
                            continue;
                        }
                        append(
                            &target.file,
                            &target.image,
                            &listed_in(target.flavour.as_deref(), *source),
                            &leaf(name),
                        )?;
                        taken.push((*name).to_string());
                    }
                    if taken.is_empty() {
                        continue;
                    }
                    if let Some((_, added)) =
                        wrote.iter_mut().find(|(file, _)| *file == target.file)
                    {
                        for name in taken {
                            if !added.contains(&name) {
                                added.push(name);
                            }
                        }
                    } else {
                        wrote.push((target.file.clone(), taken));
                    }
                }
                Ok(wrote
                    .into_iter()
                    .map(|(file, added)| {
                        (
                            file,
                            Change::Updated(format!(
                                "{} added to modules",
                                crate::import::said(&added)
                            )),
                        )
                    })
                    .collect())
            }
        }
    }
}

/// The relative path distinguishes referenced members under `.remote` from
/// repository-owned modules.
fn dir_of(name: &str, source: Option<&str>) -> String {
    match source {
        Some(owner) => format!("{}/{owner}/{name}", crate::model::remote::REMOTE_DIR),
        None => name.to_string(),
    }
}

/// An ungated entry spans every flavour, so an existing entry conflicts only
/// where its target overlaps `into`.
///
/// Matched by the module's name, so `import` under `.remote/<collection>/` and
/// `copy` under `modules/` see each other. A namesake from a different
/// collection is a different module and is not one of these.
fn holds<'a>(
    list: &'a crate::model::image::List,
    into: &Listed,
    name: &str,
    source: Option<&str>,
) -> Option<(
    &'a crate::model::image::Image,
    &'a crate::model::image::Entry,
)> {
    let image = list.images.iter().find(|image| image.name == into.image)?;
    let entry = image.entries.iter().find(|entry| {
        let same_module = match (entry.source.as_deref(), source) {
            (Some(held), Some(want)) => held == want,
            _ => true,
        };
        entry.name() == name
            && same_module
            && (into.flavour.is_none() || entry.flavour.is_none() || entry.flavour == into.flavour)
    })?;
    Some((image, entry))
}

fn leaf(name: &str) -> String {
    format!("module \"{name}\"")
}

/// Listing and flavour gating are one question at two depths. A single image
/// without flavours therefore uses a yes-or-no question.
fn ask(
    list: &crate::model::image::List,
    targets: &[crate::model::image::Target],
    prompt: &Prompt,
) -> Result<common::ui::Answer, String> {
    if let [only] = targets {
        let listed = prompt.confirm(&copy::list_in(&only.to_string()), copy::YES, copy::NO)?;
        return Ok(common::ui::Answer::Chosen(match listed {
            true => vec![0],
            false => Vec::new(),
        }));
    }
    let mut ungated = 0;
    let mut rows: Vec<Choice> = Vec::new();
    for (at, target) in targets.iter().enumerate() {
        let label = target.to_string();
        if target.flavour != crate::model::image::NO_FLAVOUR {
            rows.push(Choice::new(label, "").under(ungated));
            continue;
        }
        ungated = at;
        let named = list
            .images
            .iter()
            .find(|image| image.id == target.image)
            .map_or("", |image| match image.name == image.id {
                true => "",
                false => image.name.as_str(),
            });
        rows.push(Choice::new(label, named));
    }
    prompt.choose_many(copy::LIST_IN_IMAGES, &rows, &[])
}

fn listed_in<'a>(
    flavour: Option<&'a str>,
    source: Option<&'a str>,
) -> Vec<(&'a str, Option<&'a str>)> {
    let mut chain = vec![("modules", None)];
    if let Some(flavour) = flavour {
        chain.push(("flavour", Some(flavour)));
    }
    if let Some(source) = source {
        chain.push(("source", Some(source)));
    }
    chain
}

fn wrap(blocks: &[(&str, Option<&str>)], leaf: &str) -> String {
    blocks
        .iter()
        .rev()
        .fold(leaf.to_string(), |inner, (node, arg)| {
            let arg = arg.map_or(String::new(), |arg| format!(" \"{arg}\""));
            let inner: String = inner.lines().map(|line| format!("    {line}\n")).collect();
            format!("{node}{arg} {{\n{inner}}}")
        })
}

/// Append inserts one declaration at the deepest existing block and wraps the
/// missing blocks. Every other byte stays in place.
fn append(
    file: &Path,
    image: &str,
    chain: &[(&str, Option<&str>)],
    leaf: &str,
) -> Result<(), String> {
    let mut text =
        std::fs::read_to_string(file).map_err(|err| format!("{}: {err}", file.display()))?;
    let (kept, close) = (0..=chain.len())
        .rev()
        .find_map(|kept| {
            crate::parse::image::block_close(&text, image, &chain[..kept]).map(|at| (kept, at))
        })
        .ok_or_else(|| format!("{} declares no image `{image}`", file.display()))?;
    let declaration = wrap(&chain[kept..], leaf);

    let start = text[..close].rfind('\n').map_or(0, |at| at + 1);
    let indent = &text[start..close];
    let (at, line) = match indent.trim().is_empty() {
        true => (
            start,
            declaration
                .lines()
                .map(|line| format!("{indent}    {line}\n"))
                .collect(),
        ),
        false => (close, format!("{} ", declaration.replace('\n', " "))),
    };
    text.insert_str(at, &line);
    std::fs::write(file, text).map_err(|err| format!("{}: {err}", file.display()))
}

/// A fresh image needs the family adapter and every provider required by its
/// base, so one question offers both groups.
fn wanted<'a>(
    index: &'a crate::provider::Index,
    family: &str,
    roles: &[&str],
) -> Vec<&'a crate::provider::Provider> {
    let mut out: Vec<&crate::provider::Provider> = Vec::new();
    for capability in roles {
        // The role is filled per family, so the provider that fits this image
        // is the one wanted.
        let Some(provider) = index.adapter(capability, family) else {
            continue;
        };
        if !out.iter().any(|held| held.dir() == provider.dir()) {
            out.push(provider);
        }
    }
    out
}

/// A provider list shorter than these roles means an unread collection may
/// still supply an answer.
fn roles(base: Option<&crate::base::Base>) -> Vec<&str> {
    std::iter::once(BUILD_ENVIRONMENT)
        .chain(
            base.into_iter()
                .flat_map(|base| base.requires.iter().map(String::as_str)),
        )
        .collect()
}

/// The question says *needs* because the family adapter is not a declared
/// `requires` on the base row.
fn offer(
    base: &str,
    wanted: &[&crate::provider::Provider],
    prompt: &Prompt,
    current: bool,
) -> Result<bool, String> {
    if prompt.asks() {
        println!("{base} needs the following modules:");
        for provider in wanted {
            println!("\x20   {}", provider.qualified());
        }
        println!();
    }
    let [yes, no] = yes_no_options();
    prompt.confirm_current(copy::BRING_FOR_BASE, &yes.label, &no.label, current)
}

fn yes_no_options() -> [Choice; 2] {
    [Choice::new(copy::YES, ""), Choice::new(copy::NO, "")]
}

fn seeded(wanted: &[&crate::provider::Provider]) -> String {
    let mut owners: Vec<Option<&str>> = Vec::new();
    for provider in wanted {
        let owner = provider.owner.as_deref();
        if !owners.contains(&owner) {
            owners.push(owner);
        }
    }
    let mut out = String::new();
    for owner in owners {
        let leaf: String = wanted
            .iter()
            .filter(|provider| provider.owner.as_deref() == owner)
            .map(|provider| format!("module \"{}\"", provider.name))
            .collect::<Vec<String>>()
            .join("\n");
        let chain: Vec<(&str, Option<&str>)> =
            owner.iter().map(|owner| ("source", Some(*owner))).collect();
        out.push_str(&wrap(&chain, &leaf));
        out.push('\n');
    }
    out.lines()
        .map(|line| format!("\x20       {line}\n"))
        .collect()
}

fn image_kdl(
    name: &str,
    url: Option<&str>,
    base: &str,
    family: &str,
    known: Option<&crate::base::Base>,
    bootloader: Option<&str>,
    modules: &str,
) -> String {
    let layout = match bootloader {
        Some(bootloader) => format!(
            "\x20   layout {{\n\
             \x20       bootloader \"{bootloader}\"\n\
             \x20   }}\n\
             \n"
        ),
        None => String::new(),
    };
    let urls = match url {
        Some(url) => format!(
            "\x20   url \"{url}\"\n\
             \x20   issues-url \"{url}/issues\"\n"
        ),
        None => String::new(),
    };
    let mut ships = String::new();
    if let Some(known) = known {
        for (node, names) in [("provides", &known.provides), ("requires", &known.requires)] {
            if !names.is_empty() {
                let listed: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
                ships.push_str(&format!("\x20       {node} {}\n", listed.join(" ")));
            }
        }
        // The signature probe can correct only a written line. An omission
        // would leave it no declaration to repair.
        ships.push_str(&format!("\x20       signed #{}\n", known.signed));
    }
    format!(
        "schema-version {}\n\
         \n\
         image {{\n\
         \x20   name \"{name}\"\n\
         {urls}\n\
         \x20   base \"{base}\" {{\n\
         \x20       family \"{family}\"\n\
         {ships}\
         \x20   }}\n\
         \n\
         {layout}\
         \x20   modules {{\n\
         {modules}\
         \x20   }}\n\
         }}\n",
        crate::model::image::SCHEMA_VERSION
    )
}

/// The outer repository would read a nested repository as
/// its own modules and images.
fn refuse_nesting(root: &Path) -> Result<(), String> {
    let full = match root.is_absolute() {
        true => root.to_path_buf(),
        false => std::env::current_dir().unwrap_or_default().join(root),
    };
    let from = full.ancestors().find(|dir| dir.exists()).unwrap_or(&full);
    match crate::find_root(from) {
        Some(outer) => Err(format!(
            "{} is inside the repository at {}; a repository does not nest",
            root.display(),
            outer.display()
        )),
        None => Ok(()),
    }
}

/// Puts a directory under git control for a repository that was hand-written
/// or copied. An existing ignore file is the repository's, so it is never
/// replaced.
pub fn git_control(root: &Path) -> Result<(), String> {
    let full = match root.is_absolute() {
        true => root.to_path_buf(),
        false => std::env::current_dir().unwrap_or_default().join(root),
    };
    if let Some(outer) = enclosing_git(&full) {
        return Err(match outer == full {
            true => format!("{} is already a git repository", full.display()),
            false => format!(
                "{} is inside the git repository at {}; a repository does not nest",
                full.display(),
                outer.display()
            ),
        });
    }
    let mut wrote: Vec<(PathBuf, Change)> = Vec::new();
    let ignore = full.join(".gitignore");
    if !ignore.exists() {
        crate::init::put(&ignore, crate::init::GITIGNORE)?;
        wrote.push((PathBuf::from(".gitignore"), Change::Created));
    }
    git_init(&full)?;
    report(&full, &wrote);
    println!("initialised a git repository in {}", full.display());
    Ok(())
}

/// The nearest directory holding a `.git` entry, the directory itself
/// included. A `.git` file names a worktree, which is a repository too.
fn enclosing_git(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Git output is discarded so every create step emits one consistent status
/// line.
fn git_init(root: &Path) -> Result<(), String> {
    // The push line, remote and workflows all require the `main` branch.
    match quietly(Command::new("git").args(["init", "-b", "main"]).arg(root)).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!(
            "`git init` exited {}",
            status.code().unwrap_or_default()
        )),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(
            "`git` is not installed, and it is what initialises the repository: \
             install it from your platform's git package"
                .to_string(),
        ),
        Err(err) => Err(format!("git: {err}")),
    }
}

fn quietly(command: &mut Command) -> &mut Command {
    command.stdout(Stdio::null()).stderr(Stdio::null())
}

/// Both `gh` checks run during collection because their results decide which
/// offers the flow can make.
fn gh_installed() -> bool {
    quietly(Command::new("gh").arg("--version"))
        .status()
        .is_ok_and(|status| status.success())
}

fn gh_logged_in() -> bool {
    quietly(Command::new("gh").args(["auth", "status"]))
        .status()
        .is_ok_and(|status| status.success())
}

fn create_remote(owner: &str, id: &str) -> Result<(), String> {
    match Command::new("gh")
        .args(["repo", "create", &format!("{owner}/{id}"), "--public"])
        .status()
    {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!(
            "`gh repo create` exited {}",
            status.code().unwrap_or_default()
        )),
        Err(err) => Err(format!("gh: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kept script calls the other scripts by path, so a second copy moves
    /// the calls in a kept script the user left as it was, and only in that one.
    #[test]
    fn a_second_kept_script_moves_the_calls_of_an_unchanged_one() {
        let root = std::env::temp_dir().join(format!("tect-kept-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let keep = |names: &[&'static str]| {
            Scripts {
                chosen: names.to_vec(),
            }
            .apply(&root)
            .unwrap()
        };
        keep(&["lint.sh", "smoke.sh"]);
        let smoke = root.join("scripts/smoke.sh");
        let edited = std::fs::read_to_string(&smoke).unwrap() + "# reviewed\n";
        std::fs::write(&smoke, &edited).unwrap();

        keep(&["tect.sh"]);
        let lint = std::fs::read_to_string(root.join("scripts/lint.sh")).unwrap();
        assert!(lint.contains("\n./scripts/tect.sh check\n"), "{lint}");
        assert!(!lint.contains("generated/scripts/tect.sh"), "{lint}");
        assert_eq!(std::fs::read_to_string(&smoke).unwrap(), edited);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Interleaved members from two collections prove that seeding writes one
    /// `source` block per collection.
    #[test]
    fn the_seeded_block_groups_each_collections_modules_under_one_source() {
        let provider = |owner: Option<&str>, name: &str| crate::provider::Provider {
            owner: owner.map(str::to_string),
            name: name.to_string(),
            here: false,
            declares: crate::parse::module::Summary::default(),
        };
        // Grouping by owner keeps one block per collection when a base
        // interleaves their requirements.
        let held = [
            provider(Some("tectonic-os"), "debian-family"),
            provider(None, "mine"),
            provider(Some("tectonic-os"), "deb-bootc-base/bootc"),
        ];
        let wanted: Vec<&crate::provider::Provider> = held.iter().collect();
        assert_eq!(
            seeded(&wanted),
            "        source \"tectonic-os\" {\n\
             \x20           module \"debian-family\"\n\
             \x20           module \"deb-bootc-base/bootc\"\n\
             \x20       }\n\
             \x20       module \"mine\"\n"
        );
        assert_eq!(seeded(&[]), "");
    }

    #[test]
    fn repeated_targets_write_once_and_one_file_names_every_addition() {
        let root = std::env::temp_dir().join(format!("tect-listing-{}", std::process::id()));
        crate::init::put(
            &root.join("image.kdl"),
            r#"schema-version 1

image {
    name "Example"
    base "example" { family "fedora" }
    flavours {
        dev
        server
    }
    modules {
        flavour "dev" { source "one" { module "one" } }
    }
}
"#,
        )
        .unwrap();
        let listing = Listing::collect(
            &root,
            vec![
                "example/dev".into(),
                "example/dev".into(),
                "example/server".into(),
            ],
            &Prompt::silent(),
        )
        .unwrap();
        assert_eq!(listing.targets().len(), 2);

        let (list, _) = crate::model::image::List::load(&root);
        let wrote = listing
            .apply_declaration(&list, &[("one", Some("one")), ("two", Some("one"))])
            .unwrap();
        assert_eq!(wrote.len(), 1);
        let Change::Updated(description) = &wrote[0].1 else {
            panic!("an existing image file is updated")
        };
        assert!(
            description.contains("one") && description.contains("two"),
            "{description}"
        );

        let image = std::fs::read_to_string(root.join("image.kdl")).unwrap();
        assert_eq!(image.matches("module \"one\"").count(), 2, "{image}");
        assert_eq!(image.matches("module \"two\"").count(), 2, "{image}");
        let _ = std::fs::remove_dir_all(root);
    }

    /// One catalog row, for a test that needs a known base rather than a
    /// library on disk.
    fn base(
        name: &str,
        image: &str,
        family: &str,
        provides: &[&str],
        bootloaders: &[&str],
    ) -> crate::base::Base {
        crate::base::Base {
            name: name.to_string(),
            image: image.to_string(),
            family: family.to_string(),
            provides: provides.iter().map(|name| name.to_string()).collect(),
            requires: Vec::new(),
            about: format!("what {name} ships"),
            signed: false,
            scap_content: String::new(),
            bootloaders: bootloaders.iter().map(|b| b.to_string()).collect(),
            span: crate::diag::Span::default(),
        }
    }

    /// A base and an unknown one write different images: a catalogued base
    /// writes every property the library holds for it.
    #[test]
    fn a_catalogued_base_writes_what_it_ships_and_an_unknown_one_writes_nothing() {
        let bazzite = "ghcr.io/ublue-os/bazzite:stable";
        let seeded = base(
            "bazzite",
            bazzite,
            "fedora",
            &["rechunking", "flatpak"],
            &["grub2"],
        );
        let known = image_kdl(
            "Bazzite",
            None,
            bazzite,
            "fedora",
            crate::base::find(std::slice::from_ref(&seeded), bazzite),
            Some("grub2"),
            "",
        );
        assert!(
            known.starts_with("schema-version 1\n\nimage {\n"),
            "{known}"
        );
        assert!(
            known.contains("        provides \"rechunking\" \"flatpak\"\n"),
            "{known}"
        );
        assert!(
            known.contains("    layout {\n        bootloader \"grub2\"\n    }\n"),
            "{known}"
        );

        let shared = image_kdl(
            "Server",
            Some("https://github.com/someone/example"),
            bazzite,
            "fedora",
            None,
            None,
            "",
        );
        assert!(
            shared.contains("    url \"https://github.com/someone/example\"\n")
                && shared
                    .contains("    issues-url \"https://github.com/someone/example/issues\"\n"),
            "{shared}"
        );

        let row = |bootloaders: &[&str]| crate::base::Base {
            name: "own".to_string(),
            image: "example.invalid/own:1".to_string(),
            family: "fedora".to_string(),
            provides: Vec::new(),
            requires: Vec::new(),
            about: "what a collection describes".to_string(),
            signed: true,
            scap_content: String::new(),
            bootloaders: bootloaders.iter().map(|b| b.to_string()).collect(),
            span: crate::diag::Span::default(),
        };
        let described = row(&["grub2", "systemd"]);
        let extended = image_kdl(
            "Own",
            None,
            &described.image,
            "fedora",
            Some(&described),
            None,
            "",
        );
        assert!(extended.contains("        signed #true\n"), "{extended}");

        let unknown = image_kdl(
            "Own",
            None,
            "example.invalid/own:1",
            "fedora",
            None,
            None,
            "",
        );
        assert!(!unknown.contains("provides"), "{unknown}");
        assert!(!unknown.contains("layout"), "{unknown}");

        assert_eq!(module_kdl("", &[], &[], &[]).unwrap(), "schema-version 1\n");

        // One bootloader is written without a question. Two are asked, opened
        // on the row's default, and an unattended run takes it.
        let one = row(&["grub2"]);
        assert_eq!(
            choose_bootloader(Some(&one), None, &Prompt::scripted(Vec::new())).unwrap(),
            Some("grub2".to_string())
        );
        assert_eq!(
            choose_bootloader(Some(&described), None, &Prompt::silent()).unwrap(),
            Some("grub2".to_string())
        );
        assert_eq!(
            choose_bootloader(Some(&described), None, &Prompt::scripted(vec!["2".into()])).unwrap(),
            Some("systemd".to_string())
        );
        assert_eq!(
            choose_bootloader(Some(&described), Some("systemd"), &Prompt::silent()).unwrap(),
            Some("systemd".to_string()),
            "asking again opens on the answer held"
        );
        assert_eq!(
            choose_bootloader(None, None, &Prompt::silent()).unwrap(),
            None
        );
        assert!(
            unknown.contains("base \"example.invalid/own:1\" {\n        family \"fedora\"\n"),
            "{unknown}"
        );
    }

    /// A typed `--base` naming the catalog name writes the image reference the
    /// catalog holds for it, and an unknown name passes through.
    #[test]
    fn a_base_is_canonicalised_from_its_catalog_name() {
        let seeded = base(
            "bazzite",
            "ghcr.io/ublue-os/bazzite:stable",
            "fedora",
            &[],
            &["grub2"],
        );
        let bases = [seeded];
        assert_eq!(
            catalogued(&bases, "bazzite".to_string()),
            "ghcr.io/ublue-os/bazzite:stable"
        );
        assert_eq!(
            catalogued(&bases, "ghcr.io/other/unknown:1".to_string()),
            "ghcr.io/other/unknown:1"
        );
    }

    /// A capability the library misspells is a name a generated graph cannot
    /// carry, and every witness path is checked on a machine with no working
    /// directory.
    #[test]
    fn every_library_name_is_a_name() {
        use crate::model::image::is_name;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/collections/upstream");
        let mut issues = crate::diag::Issues::default();
        let mut bases = Vec::new();
        for path in crate::base::base_files(&dir) {
            let (base, _) = crate::parse::bases::read_base(&path, &mut issues)
                .unwrap_or_else(|| panic!("{} describes no base", path.display()));
            bases.push(base);
        }
        let (rows, _) =
            crate::parse::bases::read_capabilities(&dir.join("capabilities.kdl"), &mut issues)
                .expect("the fixture library carries capabilities.kdl");
        assert!(issues.is_empty(), "{}", issues.plain());
        for base in bases {
            assert!(is_name(&base.family), "{}", base.image);
            for name in &base.provides {
                assert!(is_name(name), "{} provides {name}", base.image);
            }
        }
        for row in rows {
            assert!(is_name(&row.name), "capability {}", row.name);
            assert!(
                row.path.as_deref().is_none_or(|p| p.starts_with('/')),
                "{}",
                row.name
            );
            for (_, path) in &row.families {
                assert!(path.starts_with('/'), "{} reads {path}", row.name);
            }
        }
    }

    /// The gated reference lives two blocks below `modules` and neither is
    /// there, so both are written on the way down.
    #[test]
    fn a_missing_flavour_and_source_block_are_written_around_the_declaration() {
        let root = std::env::temp_dir().join(format!("tect-nested-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("example.image.kdl");
        std::fs::write(
            &file,
            image_kdl(
                "Example",
                None,
                "quay.io/fedora/fedora-bootc:44",
                "fedora",
                None,
                None,
                "",
            ),
        )
        .unwrap();

        let chain = listed_in(Some("dx"), Some("one"));
        append(&file, "Example", &chain, "module \"dev-tools\"").unwrap();
        let written = std::fs::read_to_string(&file).unwrap();
        let nested = [
            "    modules {",
            "        flavour \"dx\" {",
            "            source \"one\" {",
            "                module \"dev-tools\"",
            "            }",
            "        }",
            "    }",
        ]
        .join("\n");
        assert!(written.contains(&nested), "{written}");

        append(&file, "Example", &chain, "module \"editor\"").unwrap();
        let written = std::fs::read_to_string(&file).unwrap();
        assert_eq!(written.matches("flavour \"dx\"").count(), 1);
        assert_eq!(written.matches("source \"one\"").count(), 1);
        assert!(
            written.contains("module \"dev-tools\"\n                module \"editor\"\n"),
            "{written}"
        );

        append(&file, "Example", &[("flavours", None)], "dx").unwrap();
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(
            written.contains("    flavours {\n        dx\n    }\n"),
            "{written}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_cancelled_listing_does_not_create_a_module() {
        let root = std::env::temp_dir().join(format!("tect-cancel-module-{}", std::process::id()));
        let module = Module {
            path: "hello".into(),
            file: root.join("modules/hello/module.kdl"),
            text: "description \"hello\"\n".into(),
            listing: Listing::Cancelled,
        };
        assert!(module.apply(&root).unwrap().is_empty());
        assert!(!module.file.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn create_repo_and_image_screens_snapshot_production_copy() {
        use crate::screen_tests::{assert_screen, screen};

        assert_screen(
            "create-repo-name",
            screen().line(copy::REPO_NAME, "", "Example", Some("example")),
        );
        assert_screen(
            "create-repo-owner",
            screen().line(&copy::username(HOST), "github.com/", "someone", None),
        );
        let yes_no = yes_no_options();
        assert_screen(
            "create-repo-provider",
            screen().picker(copy::SCHEDULED, &yes_no, None, "enter confirms", 0),
        );
        assert_screen(
            "create-repo-host",
            screen().picker(copy::REPO_HOST, &host_options(), None, "enter confirms", 0),
        );
        assert_screen(
            "create-repo-image",
            screen().picker(copy::IMAGES, &yes_no, None, "enter confirms", 1),
        );
        assert_screen(
            "create-repo-remote",
            screen().picker(copy::CREATE_REMOTE, &yes_no, None, "enter confirms", 1),
        );
        let scripts: Vec<Choice> = offered()
            .into_iter()
            .map(|(name, about, _)| Choice::new(name, about))
            .collect();
        assert_screen(
            "create-repo-scripts",
            screen().picker(copy::SCRIPTS, &scripts, Some(&[0]), "space toggles", 0),
        );

        assert_screen(
            "create-image-name",
            screen().line(copy::IMAGE_NAME, "", "Workstation", Some("Example")),
        );
        let libraries: Vec<Choice> = Libraries::every()
            .iter()
            .map(|library| {
                Choice::new(
                    format!("{} \"{}\"", library.kind.as_str(), library.alias),
                    library.about,
                )
            })
            .collect();
        assert_screen(
            "create-repo-libraries",
            screen().picker(
                copy::LIBRARIES,
                &libraries,
                Some(&[0, 1, 2]),
                "space toggles",
                0,
            ),
        );
        let bases = vec![
            base(
                "fedora-bootc-44",
                "quay.io/fedora/fedora-bootc:44",
                "fedora",
                &["rechunking", "bootc"],
                &["grub2"],
            ),
            base(
                "ubuntu-26-04",
                "docker.io/library/ubuntu:26.04",
                "ubuntu",
                &[],
                &["grub2", "systemd"],
            ),
        ];
        let mut options = base_options(&bases);
        options.push(Choice::new(copy::OTHER_BASE, copy::OTHER_BASE_ABOUT));
        assert_screen(
            "create-image-base",
            screen().picker(copy::IMAGE_BASE, &options, None, "enter confirms", 0),
        );
        assert_screen(
            "create-image-add-modules",
            screen().picker(
                copy::BRING_FOR_BASE,
                &yes_no_options(),
                None,
                "enter confirms",
                0,
            ),
        );
        let dual = bases
            .iter()
            .find(|base| base.bootloaders.len() == 2)
            .expect("the production catalog carries a two-bootloader base");
        assert_screen(
            "create-image-bootloader",
            screen().picker(
                copy::IMAGE_BOOTLOADER,
                &bootloader_options(&dual.bootloaders),
                None,
                "enter confirms",
                0,
            ),
        );
        assert_screen(
            "create-image-family",
            screen().line(copy::BASE_FAMILY, "", "fedora", None),
        );
    }

    #[test]
    fn repository_review_screen_snapshots_collected_rows() {
        use crate::screen_tests::{assert_screen, assert_style_at, screen};

        let repo = Repo {
            name: "Example".to_string(),
            id: "example".to_string(),
            root: PathBuf::from("example"),
            host: HOST.to_string(),
            owner: Some("someone".to_string()),
            assets: PathBuf::from("assets"),
            libraries: Libraries {
                chosen: Libraries::every(),
            },
            image: None,
            workflows: None,
            scripts: Scripts {
                chosen: vec![SKELETON_FILE],
            },
            remote: false,
            install_gh: false,
        };
        let mut rows: Vec<Choice> = repo
            .rows()
            .into_iter()
            .map(|(_, label, value)| Choice::new(format!("{label}  {value}"), ""))
            .collect();
        rows.push(Choice::new(copy::CREATE, ""));
        let rendered = assert_screen(
            "create-repo-review",
            screen().picker(copy::REVIEW, &rows, None, copy::REVIEW_KEYS, rows.len() - 1),
        );
        assert_style_at(
            &rendered,
            0,
            8,
            "fg: Rgb(238, 111, 248), bg: Reset, underline: Reset, modifier: BOLD",
            "the selected Create action",
        );
    }

    /// The hand-written path's first step. A second run refuses, and an
    /// ignore file the repository already keeps survives.
    #[test]
    fn git_control_initialises_once_and_keeps_a_kept_ignore_file() {
        let fresh = std::env::temp_dir().join(format!("tect-git-{}", std::process::id()));
        let kept = std::env::temp_dir().join(format!("tect-git-kept-{}", std::process::id()));
        for dir in [&fresh, &kept] {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(kept.join(".gitignore"), "mine\n").unwrap();

        git_control(&fresh).unwrap();
        assert!(fresh.join(".git").is_dir());
        assert_eq!(
            std::fs::read_to_string(fresh.join(".gitignore")).unwrap(),
            crate::init::GITIGNORE
        );
        let refused = git_control(&fresh).unwrap_err();
        assert!(refused.contains("already a git repository"), "{refused}");

        git_control(&kept).unwrap();
        assert_eq!(
            std::fs::read_to_string(kept.join(".gitignore")).unwrap(),
            "mine\n"
        );

        let _ = std::fs::remove_dir_all(&fresh);
        let _ = std::fs::remove_dir_all(&kept);
    }
}

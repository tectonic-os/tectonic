//! Create commands collect every answer before writing, so no `apply` takes a
//! `Prompt`.

use crate::copy;
use crate::declaration::append;
use crate::flow::{named_after_root, report, under, Field, Libraries, Listing, Workflows};
use crate::layout;
use common::prompt::Prompt;
pub use common::ui::tree::Change;
use common::ui::{Answer, Choice};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const GH_INSTALL: &str = "install gh from https://github.com/cli/cli";

/// The family-adapter role makes a family's package manager usable from
/// a build layer. Every family needs it filled by a different component.
const BUILD_ENVIRONMENT: &str = "build-environment";

/// The identity role writes the image's own name into os-release at build
/// time. Without it, the booted system keeps the name of the upstream image.
const OS_RELEASE: &str = "os-release";

pub fn origin(host: &str, owner: &str) -> String {
    format!("https://{host}/{owner}")
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
    workflows: Option<Workflows>,
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
                    (_, given) => given.unwrap_or_else(|| copy::HOST.to_string()),
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
                    let offering = host == copy::HOST && prompt.asks();
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
                // The base row is inside the image, so re-entering at it does
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
                        Home {
                            root: &root,
                            repo: &name,
                            url,
                            sources_text: &crate::init::sources_block(&libraries.chosen),
                        },
                        given.image.clone(),
                        given.base.clone(),
                        "`--image`",
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
                    None => Workflows::collect(
                        &basis,
                        &Workflows::every(&basis),
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
                Scripts::collect(&root, &[], held, prompt)?
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
        if self.owner.is_some() && self.host == copy::HOST {
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
            next.push(match self.remote || host != copy::HOST {
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

/// A repository-owned script replaces the matching script supplied by `tect`.
fn offered() -> Vec<(&'static str, &'static str, &'static str)> {
    std::iter::once((
        SKELETON_FILE,
        "the Containerfile around the generated component layers",
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
    ) -> Result<Self, String> {
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
            return Ok(Self { chosen });
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
            unreachable!("choose_many answers with a choice or fails")
        };
        Ok(Self {
            chosen: chosen.iter().map(|at| open[*at].0).collect(),
        })
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

/// The host catalog is limited to the forges supported by generated workflows.
fn choose_host(current: Option<&str>, prompt: &Prompt) -> Result<String, String> {
    let options = host_options();
    let at = current.map(|held| usize::from(held != copy::HOST));
    match ask_one(prompt, copy::REPO_HOST, &options, at)? {
        Some(0) | None => Ok(copy::HOST.to_string()),
        _ => prompt.line(
            None,
            copy::FORGEJO_ADDRESS,
            "`--host`",
            "",
            current.filter(|held| *held != copy::HOST),
        ),
    }
}

fn host_options() -> [Choice; 2] {
    [
        Choice::new(copy::HOST, copy::host_github()),
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
    /// Holds the library providers the base offer took. `apply` vendors them
    /// once `repo.kdl` declares their library. The lock entry then carries the
    /// library's pin.
    vendored: Vec<crate::import::Found>,
    /// Names the library providers the offer took that the repository
    /// already holds. The image lists the local component, and the tree says so.
    kept: Vec<String>,
    /// Holds the path and the manifest of a base component that `apply`
    /// writes, for a typed reference that no base component names.
    own: Option<(String, String)>,
}

/// The repository that a scaffolded image is written into.
pub struct Home<'a> {
    pub root: &'a Path,
    pub repo: &'a str,
    pub url: Option<String>,
    pub sources_text: &'a str,
}

impl Image {
    /// A missing image name falls back to the repository name. Existing images
    /// already carry the repository URL.
    pub fn collect(
        home: Home,
        name: Option<String>,
        base: Option<String>,
        flag: &str,
        from: Field,
        prev: Option<&Self>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
        let Home {
            root,
            repo,
            url,
            sources_text,
        } = home;
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
        let (list, _) = crate::model::image::List::load(root);
        let names_default = implicit_default(&list).filter(|was| *was != id);
        let url = url.or_else(|| {
            list.images
                .iter()
                .find(|image| !image.url.is_empty())
                .map(|image| image.url.clone())
        });

        // A repository that is not written yet gets the block its libraries
        // answer is about to write, which is the only thing `create repo` has
        // to offer components against. Both halves read the library through its
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
            eprintln!("tect: {why}; the bases offered are only what is already on this machine");
        }
        let bases = crate::base::catalog(root, sources);
        let base = match base {
            Some(given) => given,
            None => choose_base(&bases, prev.map(|prev| prev.base.as_str()), prompt)?,
        };
        let base = catalogued(&bases, base);
        // The image names a base component by its path.
        // A path that names no component is a typo.
        // Written as a reference, it would fail only when the build pulls it.
        if crate::base::find(&bases, &base).is_none() && crate::model::image::is_local_path(&base) {
            return Err(format!(
                "`{base}` names no base component in the repository or a library; give an image \
                 reference with a tag, such as `quay.io/fedora/fedora-bootc:44`"
            ));
        }
        let family = match crate::base::find(&bases, &base) {
            Some(known) => known.family.clone(),
            None => prompt.text(
                None,
                copy::BASE_FAMILY,
                "`--base`, naming a base component the repository or a library holds",
                bases.first().map(|base| base.family.as_str()),
            )?,
        };
        let disk = crate::parse::disk::Disk::scan(root);
        let known = crate::base::find(&bases, &base);
        let index = crate::provider::Index::scan(root, sources, &disk, false);
        let roles = roles(known, &index);
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
            false => Vec::new(),
        };
        // A provider the repository already holds is listed as it is, so a
        // local fork is never copied over.
        let holds = |provider: &crate::provider::Provider| {
            layout::is_component(&layout::component(root, &provider.name))
        };
        let kept: Vec<String> = match take_wanted {
            true => wanted
                .iter()
                .filter(|provider| provider.owner.is_some() && holds(provider))
                .map(|provider| provider.name.clone())
                .collect(),
            false => Vec::new(),
        };
        let vendored = match take_wanted {
            true => wanted
                .iter()
                .filter_map(|provider| Some((provider, provider.owner.as_deref()?)))
                .filter(|(provider, _)| !holds(provider))
                .map(|(provider, owner)| {
                    crate::import::cached_member(
                        root,
                        sources,
                        list.audit_enforce,
                        owner,
                        &provider.name,
                    )?
                    .ok_or_else(|| {
                        format!("`{}` is no longer in its library", provider.qualified())
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            false => Vec::new(),
        };
        let mut vendored = vendored;
        let (component, own, version, digest) = match known {
            Some(known) => {
                let held = layout::is_component(&layout::component(root, &known.name));
                let from = match known.owner.as_deref().filter(|_| !held) {
                    Some(owner) => crate::import::cached_member(
                        root,
                        sources,
                        list.audit_enforce,
                        owner,
                        &known.name,
                    )?
                    .ok_or_else(|| {
                        format!("`{owner}/{}` is no longer in its library", known.name)
                    })?,
                    None => crate::import::Found {
                        owner: known.owner.clone().unwrap_or_default(),
                        name: known.name.clone(),
                        dir: layout::component(root, &known.name),
                    },
                };
                // The base's imports are copied in beside it and listed nowhere.
                let copying: Vec<String> = vendored
                    .iter()
                    .map(|found| found.name.clone())
                    .chain(std::iter::once(known.name.clone()))
                    .collect();
                let imports = crate::import::imports_of(
                    root,
                    sources,
                    list.audit_enforce,
                    from.clone(),
                    &copying,
                )?;
                if !held {
                    vendored.push(from);
                }
                vendored.extend(imports);
                (known.name.clone(), None, known.version.clone(), None)
            }
            None => {
                let OwnBase {
                    path,
                    manifest,
                    version,
                    digest,
                } = own_base(root, &base, &family)?;
                (path.clone(), Some((path, manifest)), version, digest)
            }
        };
        let held = prev
            .filter(|prev| prev.base == base)
            .and_then(|prev| prev.bootloader.as_deref());
        let bootloader = choose_bootloader(known, held, prompt)?;
        let text = image_kdl(
            &name,
            url.as_deref(),
            &BaseLine {
                path: &component,
                version: version.as_deref(),
                digest: digest.as_deref(),
            },
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
            vendored,
            kept,
            own,
        })
    }

    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        let (list, _) = crate::model::image::List::load(root);
        let copies: Vec<(&crate::import::Found, &str)> = self
            .vendored
            .iter()
            .map(|from| (from, from.name.as_str()))
            .collect();
        // The copies track their library. `import component` opens on the same
        // answer.
        let mut wrote = crate::import::vendor_all(root, &list.sources, &copies, true)?;
        wrote.extend(self.kept.iter().map(|name| {
            (
                PathBuf::from(layout::COMPONENTS).join(name),
                Change::Updated("already here, so the image lists it as it is".to_string()),
            )
        }));
        if let Some((path, text)) = &self.own {
            let file = layout::manifest(root, path);
            crate::init::put(&file, text)?;
            wrote.push((under(root, &file), Change::Created));
        }
        crate::init::put(&self.file, &self.text)?;
        wrote.push((under(root, &self.file), Change::Created));
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

/// What a typed `--base` stands for: the path of the base component it names
/// by its path or its image reference, and the typed reference itself where
/// no base component names it.
fn catalogued(bases: &[crate::base::Base], base: String) -> String {
    crate::base::find(bases, &base).map_or(base, |known| known.name.clone())
}

/// Holds a base component that `create image` writes for a typed reference
/// that no base component names, so a custom base stays one command away.
struct OwnBase {
    path: String,
    manifest: String,
    /// Holds the reference's tag. The image sets it as the base's `version`.
    /// Renovate then keeps the value current.
    version: Option<String>,
    /// Holds the reference's digest. The image sets it on its base entry.
    digest: Option<String>,
}

/// Writes the base component for a typed reference. Its path comes from the
/// reference's last part. A tag becomes the component's `version` option.
fn own_base(root: &Path, image: &str, family: &str) -> Result<OwnBase, String> {
    let (undigested, digest) = match image.split_once('@') {
        Some((before, digest)) => (before, Some(digest.to_string())),
        None => (image, None),
    };
    let (repository, version) = match undigested.rsplit_once(':') {
        Some((repository, tag)) if !tag.contains('/') => (repository, Some(tag.to_string())),
        _ => (undigested, None),
    };
    let last = repository.rsplit('/').next().unwrap_or(repository);
    let stem: String = last
        .to_lowercase()
        .chars()
        .map(|c| match c.is_ascii_alphanumeric() {
            true => c,
            false => '-',
        })
        .collect();
    let stem = stem.trim_matches('-');
    let stem = match stem.starts_with(|c: char| c.is_ascii_lowercase()) {
        true => stem.to_string(),
        false => format!("base-{stem}"),
    };
    let path = format!("base-image/{stem}");
    if layout::component(root, &path).exists() {
        return Err(format!(
            "components/{path} is already there and names another base; `--base {path}` builds on it"
        ));
    }
    let block = match family.is_empty() {
        true => String::new(),
        false => format!(" {{\n    family \"{family}\"\n}}"),
    };
    let reference = match &version {
        Some(tag) => format!(
            "base \"{repository}:$version\"{block}\n\noption \"version\" type=\"string\" {{\n    default \"{tag}\"\n}}\n"
        ),
        None => format!("base \"{repository}\"{block}\n"),
    };
    let manifest = format!(
        "schema-version {}\n\n{reference}",
        crate::model::image::SCHEMA_VERSION
    );
    Ok(OwnBase {
        path,
        manifest,
        version,
        digest,
    })
}

/// An unknown base stays available because the catalog cannot constrain a base
/// it does not describe.
fn choose_base(
    bases: &[crate::base::Base],
    current: Option<&str>,
    prompt: &Prompt,
) -> Result<String, String> {
    let mut options = base_options(bases);
    let at = current.and_then(|held| bases.iter().position(|base| base.name == held));
    if prompt.draws() && !options.is_empty() {
        options.push(Choice::new(copy::OTHER_BASE, copy::OTHER_BASE_ABOUT));
        return match ask_one(prompt, copy::IMAGE_BASE, &options, at)? {
            Some(chosen) if chosen < bases.len() => Ok(bases[chosen].name.clone()),
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
        Some(chosen) => Ok(bases[chosen].name.clone()),
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
        .map(|base| Choice::new(base.label(), &base.about))
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

/// Component creation keeps repository ownership separate from image listing.
pub struct Component {
    path: String,
    file: PathBuf,
    text: String,
    listing: Listing,
}

impl Component {
    pub fn collect(
        root: &Path,
        name: Option<String>,
        pkgs: Vec<String>,
        with: Vec<(String, String)>,
        images: Vec<String>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
        let list = crate::model::image::List::editable(root).map_err(|issues| issues.plain())?;
        let name = prompt.text(name, copy::COMPONENT_NAME, "a name argument", None)?;
        let path = name
            .split('/')
            .map(crate::init::id)
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        let file = layout::manifest(root, &path);
        if file.exists() {
            return Err(format!("components/{path} is already there"));
        }

        let description = match with.iter().any(|(verb, _)| verb == "description") {
            true => String::new(),
            false => prompt.text(
                None,
                copy::COMPONENT_DESCRIPTION,
                "`--with description=...`",
                Some(""),
            )?,
        };
        let targets: Vec<String> = match with.iter().any(|(verb, _)| verb == "targets") {
            true => Vec::new(),
            false => prompt
                .text(
                    None,
                    copy::COMPONENT_SUPPORTS,
                    "`--with targets=...`",
                    Some(""),
                )?
                .split_whitespace()
                .map(str::to_string)
                .collect(),
        };
        let pkgs = match pkgs.is_empty()
            && prompt.confirm(copy::COMPONENT_PACKAGES, copy::YES, copy::NO)?
        {
            true => prompt
                .text(None, copy::PACKAGE_NAMES, "`--pkg`", Some(""))?
                .split_whitespace()
                .map(str::to_string)
                .collect(),
            false => pkgs,
        };

        let text = component_kdl(&description, &targets, &pkgs, &with)?;
        let listing = Listing::collect_from(&list, images, prompt)?;
        listing.refuse_duplicate(&list, &path)?;
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

fn component_kdl(
    description: &str,
    targets: &[String],
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
    if !targets.is_empty() {
        let families = targets
            .iter()
            .map(|family| Ok(format!("\"{}\"", quotable(family)?)))
            .collect::<Result<Vec<_>, String>>()?
            .join(" ");
        declarations.push(format!("targets {families}"));
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

#[cfg(feature = "fuzzing")]
pub fn fuzz_rewrite(file: &Path) {
    let _ = append(file, "fuzz", &[], "fuzz");
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
        if !out
            .iter()
            .any(|held| held.qualified() == provider.qualified())
        {
            out.push(provider);
        }
    }
    out
}

/// A provider list shorter than these roles means an unread collection may
/// still supply an answer.
fn roles<'a>(base: Option<&'a crate::base::Base>, index: &crate::provider::Index) -> Vec<&'a str> {
    // The image lists a provider of each role that the base's imports leave
    // unfilled. A base written from an image reference imports nothing.
    let lacks = |role: &str| {
        base.is_none_or(|base| {
            !index
                .fitting(role, &base.family)
                .iter()
                .any(|provider| base.imports.contains(&provider.name))
        })
    };
    [BUILD_ENVIRONMENT, OS_RELEASE]
        .into_iter()
        .filter(|role| lacks(role))
        .chain(
            base.into_iter()
                .flat_map(|base| base.requires.iter().map(String::as_str)),
        )
        .collect()
}

/// The list fills the roles that the base's imports leave unfilled.
/// The identity role is optional, so the list never says the base needs it.
fn offer(
    base: &str,
    wanted: &[&crate::provider::Provider],
    prompt: &Prompt,
    current: bool,
) -> Result<bool, String> {
    if prompt.asks() {
        println!("{base} brings none of these components:");
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

fn seeded(wanted: &[&crate::provider::Provider]) -> Vec<String> {
    wanted
        .iter()
        .map(|provider| provider.name.clone())
        .collect()
}

/// Writes component paths as nested segments, each line indented four
/// spaces a level. A run of paths that share a first segment shares its
/// block. Each path's body lines go inside its own segment, ahead of the
/// segments below it.
fn tree(entries: &[(&str, Vec<String>)]) -> String {
    let mut out = String::new();
    let mut at = 0;
    while at < entries.len() {
        let head = entries[at].0.split('/').next().unwrap_or_default();
        let run = entries[at..]
            .iter()
            .take_while(|(path, _)| path.split('/').next() == Some(head))
            .count();
        let mut body: Vec<String> = Vec::new();
        let mut below: Vec<(&str, Vec<String>)> = Vec::new();
        for (path, lines) in &entries[at..at + run] {
            match path.split_once('/') {
                Some((_, rest)) => below.push((rest, lines.clone())),
                None => body.extend(lines.iter().cloned()),
            }
        }
        let inner: String = body
            .iter()
            .map(|line| format!("{line}\n"))
            .chain(std::iter::once(tree(&below)))
            .collect::<String>()
            .lines()
            .map(|line| format!("    {line}\n"))
            .collect();
        out.push_str(&match inner.is_empty() {
            true => format!("{head}\n"),
            false => format!("{head} {{\n{inner}}}\n"),
        });
        at += run;
    }
    out
}

/// Holds what the base's segment writes.
struct BaseLine<'a> {
    path: &'a str,
    version: Option<&'a str>,
    digest: Option<&'a str>,
}

fn image_kdl(
    name: &str,
    url: Option<&str>,
    base: &BaseLine,
    bootloader: Option<&str>,
    components: &[String],
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
    // Renovate keeps a written version current. The signature probe can
    // correct only a written line, and it starts from the claim that no
    // signature is published.
    let pins: Vec<String> = [("version", base.version), ("digest", base.digest)]
        .into_iter()
        .filter_map(|(node, value)| Some(format!("{node} \"{}\"", value?)))
        .chain(std::iter::once("signed #false".to_string()))
        .collect();
    let entries: Vec<(&str, Vec<String>)> = std::iter::once((base.path, pins))
        .chain(components.iter().map(|path| (path.as_str(), Vec::new())))
        .collect();
    let members: String = tree(&entries)
        .lines()
        .map(|line| format!("\x20   {line}\n"))
        .collect();
    format!(
        "schema-version {}\n\
         \n\
         image {{\n\
         \x20   name \"{name}\"\n\
         {urls}\n\
         {layout}\
         {members}\
         }}\n",
        crate::model::image::SCHEMA_VERSION
    )
}

/// The outer repository would read a nested repository as
/// its own components and images.
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
        Ok(status) => Err(format!("`git init` failed: {status}")),
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
        Ok(status) => Err(format!("`gh repo create` failed: {status}")),
        Err(err) => Err(format!("gh: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::{placed, unlisting};

    /// One component fills both roles, so the offer names it once.
    #[test]
    fn a_component_filling_two_roles_is_offered_once() {
        let root = crate::provider::tests::two_providers("wanted");
        let index =
            crate::provider::Index::local(&root, &[], &crate::parse::disk::Disk::scan(&root));
        let offered: Vec<String> = wanted(&index, "fedora", &["x", "y"])
            .iter()
            .map(|provider| provider.name.clone())
            .collect();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(offered, ["both"]);
    }

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

    #[test]
    fn repeated_targets_write_once_and_one_file_names_every_addition() {
        let root = std::env::temp_dir().join(format!("tect-listing-{}", std::process::id()));
        crate::init::put(
            &root.join("image.kdl"),
            r#"schema-version 1

image {
    name "Example"
    example { base "example" { family "fedora"; }; }
    flavours {
        dev
        server
    }
    flavour "dev" { one }
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
            .apply_declaration(&list, &[("one", ""), ("two", "")])
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
        for name in ["one", "two"] {
            assert_eq!(
                crate::parse::image::listings(&image, name),
                [Some("dev".to_string()), Some("server".to_string())],
                "{image}"
            );
        }
        let _ = std::fs::remove_dir_all(root);
    }

    /// One offered base, for a test that needs a known base rather than a
    /// library on disk.
    fn base(
        name: &str,
        image: &str,
        version: Option<&str>,
        bootloaders: &[&str],
    ) -> crate::base::Base {
        crate::base::Base {
            name: name.to_string(),
            owner: Some("ublue".to_string()),
            image: image.to_string(),
            family: "fedora".to_string(),
            provides: vec!["rechunking".to_string()],
            requires: Vec::new(),
            imports: Vec::new(),
            about: format!("what {name} ships"),
            version: version.map(str::to_string),
            scap_content: String::new(),
            bootloaders: bootloaders.iter().map(|b| b.to_string()).collect(),
        }
    }

    /// A base the repository wrote from an image reference imports nothing.
    /// A hand-written base may import only its identity.
    #[test]
    fn a_base_that_imports_no_role_provider_offers_one() {
        let root = std::env::temp_dir().join(format!("tect-roles-{}", std::process::id()));
        for (path, provides) in [
            ("fedora/build-environment", "build-environment"),
            ("identity", "os-release"),
        ] {
            crate::init::put(
                &root.join("components").join(path).join("manifest.kdl"),
                &format!("schema-version 1\n\nprovides \"{provides}\"\n"),
            )
            .unwrap();
        }
        let index =
            crate::provider::Index::local(&root, &[], &crate::parse::disk::Disk::scan(&root));
        let own = base("base-image/own", "quay.io/own/own:1", None, &[]);
        let mut identity_only = base("base-image/named", "quay.io/named/named:1", None, &[]);
        identity_only.imports = vec!["identity".to_string()];
        let mut library = base("base-image/library", "quay.io/lib/lib:1", None, &[]);
        library.imports = vec!["fedora/build-environment".to_string()];
        let offered = [
            roles(Some(&own), &index),
            roles(Some(&identity_only), &index),
            roles(Some(&library), &index),
            roles(None, &index),
        ];
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(offered[0], [BUILD_ENVIRONMENT, OS_RELEASE]);
        assert_eq!(offered[1], [BUILD_ENVIRONMENT]);
        assert_eq!(offered[2], [OS_RELEASE]);
        assert_eq!(offered[3], [BUILD_ENVIRONMENT, OS_RELEASE]);
    }

    /// The image lists the base component by its segments and sets its
    /// version. The signature probe corrects only a written `signed` line.
    #[test]
    fn an_image_names_its_base_component_and_sets_its_version() {
        let known = image_kdl(
            "Bazzite",
            None,
            &BaseLine {
                path: "base-image/bazzite",
                version: Some("stable"),
                digest: None,
            },
            Some("grub2"),
            &[],
        );
        assert!(
            known.starts_with("schema-version 1\n\nimage {\n"),
            "{known}"
        );
        assert!(
            known.contains(
                "    base-image {\n        bazzite {\n            version \"stable\"\n            signed #false\n        }\n    }\n"
            ),
            "{known}"
        );
        assert!(
            known.contains("    layout {\n        bootloader \"grub2\"\n    }\n"),
            "{known}"
        );

        let shared = image_kdl(
            "Server",
            Some("https://github.com/someone/example"),
            &BaseLine {
                path: "base-image/own",
                version: None,
                digest: None,
            },
            None,
            &[],
        );
        assert!(
            shared.contains("    url \"https://github.com/someone/example\"\n")
                && shared
                    .contains("    issues-url \"https://github.com/someone/example/issues\"\n"),
            "{shared}"
        );
        assert!(!shared.contains("        version"), "{shared}");
        assert!(!shared.contains("layout"), "{shared}");

        assert_eq!(
            component_kdl("", &[], &[], &[]).unwrap(),
            "schema-version 1\n"
        );

        // One bootloader is written without a question. Two are asked, opened
        // on the base's default, and an unattended run takes it.
        let one = base("base-image/one", "example.invalid/one:1", None, &["grub2"]);
        let described = base(
            "base-image/two",
            "example.invalid/two:1",
            None,
            &["grub2", "systemd"],
        );
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
    }

    /// A typed `--base` naming a base component's path or reference stands for
    /// that component, and an unknown reference passes through.
    #[test]
    fn a_base_is_canonicalised_to_its_component() {
        let bases = [base(
            "base-image/bazzite",
            "ghcr.io/ublue-os/bazzite:stable",
            None,
            &["grub2"],
        )];
        for typed in ["base-image/bazzite", "ghcr.io/ublue-os/bazzite:stable"] {
            assert_eq!(catalogued(&bases, typed.to_string()), "base-image/bazzite");
        }
        assert_eq!(
            catalogued(&bases, "ghcr.io/other/unknown:1".to_string()),
            "ghcr.io/other/unknown:1"
        );
    }

    /// Asking again opens on the base already chosen.
    #[test]
    fn the_base_picker_opens_on_the_base_held() {
        let bases = [
            base("base-image/one", "example.invalid/one:1", None, &["grub2"]),
            base("base-image/two", "example.invalid/two:1", None, &["grub2"]),
        ];
        let chosen = choose_base(
            &bases,
            Some("base-image/two"),
            &Prompt::scripted(vec![String::new()]),
        )
        .unwrap();
        assert_eq!(chosen, "base-image/two");
    }

    /// A typed reference no component names gets a base component of its own,
    /// which keeps a custom base one command away.
    #[test]
    fn an_unknown_reference_gets_a_base_component() {
        let root = std::env::temp_dir().join(format!("tect-own-base-{}", std::process::id()));
        let own = own_base(&root, "ghcr.io/other/Unknown_Base:1.2@sha256:abc", "fedora")
            .expect("nothing is in the way");
        assert_eq!(own.path, "base-image/unknown-base");
        assert_eq!(own.version.as_deref(), Some("1.2"));
        assert_eq!(own.digest.as_deref(), Some("sha256:abc"));
        assert_eq!(
            own.manifest,
            "schema-version 1\n\nbase \"ghcr.io/other/Unknown_Base:$version\" {\n    family \"fedora\"\n}\n\noption \"version\" type=\"string\" {\n    default \"1.2\"\n}\n"
        );
        let untagged = own_base(&root, "localhost:5000/own", "").expect("nothing is in the way");
        assert_eq!(untagged.version, None);
        assert_eq!(
            untagged.manifest,
            "schema-version 1\n\nbase \"localhost:5000/own\"\n"
        );
    }

    /// A capability the library misspells is a name a generated graph cannot
    /// carry, and every witness path is checked on a machine with no working
    /// directory.
    #[test]
    fn every_library_name_is_a_name() {
        use crate::model::image::is_name;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/collections");
        let sources: Vec<crate::model::remote::Collection> = ["upstream", "ublue"]
            .into_iter()
            .map(|name| crate::model::remote::Collection {
                kind: crate::model::remote::Kind::Components,
                name: name.to_string(),
                at: crate::model::remote::At::Dir(dir.join(name).display().to_string()),
                span: crate::diag::Span::default(),
            })
            .collect();
        let bases = crate::base::catalog(Path::new("/nonexistent"), &sources);
        assert_eq!(bases.len(), 3);
        for base in bases {
            assert!(is_name(&base.family), "{}", base.image);
            for name in &base.provides {
                assert!(is_name(name), "{} provides {name}", base.image);
            }
        }
    }

    /// The gated entry lives in a flavour block that is not there yet, so the
    /// block is written on the way down.
    #[test]
    fn a_missing_flavour_block_is_written_around_the_declaration() {
        let root = std::env::temp_dir().join(format!("tect-nested-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("example.image.kdl");
        std::fs::write(
            &file,
            image_kdl(
                "Example",
                None,
                &BaseLine {
                    path: "base-image/fedora",
                    version: None,
                    digest: None,
                },
                None,
                &[],
            ),
        )
        .unwrap();

        let (chain, line) = placed(Some("dx"), "tools/dev-tools", "");
        append(&file, "Example", &chain, &line).unwrap();
        let written = std::fs::read_to_string(&file).unwrap();
        let nested = [
            "    flavour \"dx\" {",
            "        tools {",
            "            dev-tools",
            "        }",
            "    }",
        ]
        .join("\n");
        assert!(written.contains(&nested), "{written}");

        let (chain, line) = placed(Some("dx"), "tools/editor", " { theme \"dark\"; }");
        append(&file, "Example", &chain, &line).unwrap();
        let written = std::fs::read_to_string(&file).unwrap();
        assert_eq!(written.matches("flavour \"dx\"").count(), 1);
        assert_eq!(written.matches("tools {").count(), 1);
        assert!(
            written.contains("dev-tools\n            editor { theme \"dark\"; }\n"),
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

    /// `apps/gone` is the only segment in `apps`, so `apps` goes with it.
    /// An empty segment would otherwise list the component at its own path.
    #[test]
    fn unlisting_removes_only_the_named_entry() {
        let root = std::env::temp_dir().join(format!("tect-unlist-{}", std::process::id()));
        let file = root.join("image.kdl");
        crate::init::put(
            &file,
            "image {\n    name \"one\"\n    kept; trailing\n    apps {\n        gone\n    }\n    flavour \"dev\" { inline; also }\n}\n",
        )
        .unwrap();

        let text = unlisting(
            &file,
            &std::fs::read_to_string(&file).unwrap(),
            "one",
            None,
            "apps/gone",
        )
        .unwrap();
        crate::init::put(&file, &text).unwrap();
        let text = unlisting(
            &file,
            &std::fs::read_to_string(&file).unwrap(),
            "one",
            Some("dev"),
            "inline",
        )
        .unwrap();
        crate::init::put(&file, &text).unwrap();
        let text = unlisting(
            &file,
            &std::fs::read_to_string(&file).unwrap(),
            "one",
            None,
            "trailing",
        )
        .unwrap();
        crate::init::put(&file, &text).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "image {\n    name \"one\"\n    kept;\n    flavour \"dev\" { also }\n}\n"
        );
        assert!(unlisting(
            &file,
            &std::fs::read_to_string(&file).unwrap(),
            "one",
            None,
            "absent"
        )
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Unlists `name` from `text`, as the image file of a repository whose
    /// `hardware/nvidia` is a component.
    fn unlisted(
        case: &str,
        text: &str,
        flavour: Option<&str>,
        name: &str,
    ) -> Result<String, String> {
        let root = std::env::temp_dir().join(format!("tect-unlist-{case}-{}", std::process::id()));
        let file = root.join("image.kdl");
        crate::init::put(&file, text).unwrap();
        crate::init::put(&root.join("components/hardware/nvidia/manifest.kdl"), "").unwrap();
        let out = unlisting(&file, text, "one", flavour, name);
        std::fs::remove_dir_all(root).unwrap();
        out
    }

    #[test]
    fn unlisting_keeps_a_parent_that_holds_another_segment() {
        let image = "image {\n    name \"one\"\n    apps {\n        gone\n        kept\n    }\n}\n";
        assert_eq!(
            unlisted("kept", image, None, "apps/gone").unwrap(),
            "image {\n    name \"one\"\n    apps {\n        kept\n    }\n}\n"
        );
    }

    #[test]
    fn unlisting_removes_every_parent_it_empties() {
        let image = "image {\n    name \"one\"\n    a {\n        b {\n            c\n        }\n    }\n    kept\n}\n";
        assert_eq!(
            unlisted("deep", image, None, "a/b/c").unwrap(),
            "image {\n    name \"one\"\n    kept\n}\n"
        );
    }

    /// The flavour block stays, because it declares no segment of its own.
    #[test]
    fn unlisting_a_flavour_s_last_entry_keeps_the_flavour_block() {
        let image = "image {\n    name \"one\"\n    flavour \"dev\" {\n        tools {\n            gone\n        }\n    }\n}\n";
        assert_eq!(
            unlisted("flavour", image, Some("dev"), "tools/gone").unwrap(),
            "image {\n    name \"one\"\n    flavour \"dev\" {\n    }\n}\n"
        );
    }

    #[test]
    fn unlisting_finds_a_path_through_a_repeated_parent() {
        let image =
            "image {\n    name \"one\"\n    hardware { audio-a }\n    hardware { audio-b }\n}\n";
        assert_eq!(
            unlisted("repeated", image, None, "hardware/audio-b").unwrap(),
            "image {\n    name \"one\"\n    hardware { audio-a }\n}\n"
        );
    }

    /// `hardware/nvidia` is a component on disk, so emptying its block keeps
    /// it listed.
    #[test]
    fn unlisting_keeps_a_parent_that_is_a_component() {
        let image = "image {\n    name \"one\"\n    hardware {\n        nvidia {\n            cuda\n        }\n    }\n}\n";
        assert_eq!(
            unlisted("component", image, None, "hardware/nvidia/cuda").unwrap(),
            "image {\n    name \"one\"\n    hardware {\n        nvidia\n    }\n}\n"
        );
    }

    /// `variant="lean"` chooses what `hardware/nvidia` builds, so it outlives
    /// the emptied block.
    #[test]
    fn unlisting_keeps_the_properties_of_a_component_parent() {
        let image = "image {\n    name \"one\"\n    hardware {\n        nvidia variant=\"lean\" {\n            cuda\n        }\n    }\n}\n";
        assert_eq!(
            unlisted("variant", image, None, "hardware/nvidia/cuda").unwrap(),
            "image {\n    name \"one\"\n    hardware {\n        nvidia variant=\"lean\"\n    }\n}\n"
        );
    }

    /// `fragment position="after"` belongs to the definition, so it is no
    /// segment below `tools`.
    #[test]
    fn unlisting_an_inline_definition_removes_it() {
        let image = "image {\n    name \"one\"\n    tools {\n        fragment position=\"after\"\n    }\n    kept\n}\n";
        assert_eq!(
            unlisted("inline", image, None, "tools").unwrap(),
            "image {\n    name \"one\"\n    kept\n}\n"
        );
    }

    #[test]
    fn unlisting_a_segment_that_holds_segments_is_refused() {
        let image = "image {\n    name \"one\"\n    hardware {\n        nvidia {\n            cuda\n        }\n    }\n}\n";
        let refused = unlisted("holds", image, None, "hardware/nvidia").unwrap_err();
        assert!(refused.contains("with cuda below it"), "{refused}");
    }

    #[test]
    fn a_cancelled_listing_does_not_create_a_component() {
        let root =
            std::env::temp_dir().join(format!("tect-cancel-component-{}", std::process::id()));
        let component = Component {
            path: "hello".into(),
            file: root.join("components/hello/manifest.kdl"),
            text: "description \"hello\"\n".into(),
            listing: Listing::Cancelled,
        };
        assert!(component.apply(&root).unwrap().is_empty());
        assert!(!component.file.exists());
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
            screen().line(&copy::username(copy::HOST), "github.com/", "someone", None),
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
                "base-image/fedora-bootc",
                "quay.io/fedora/fedora-bootc:44",
                Some("44"),
                &["grub2"],
            ),
            base(
                "base-image/ubuntu",
                "docker.io/library/ubuntu:26.04",
                Some("26.04"),
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
            "create-image-add-components",
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
            host: copy::HOST.to_string(),
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

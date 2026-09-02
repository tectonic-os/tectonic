//! `create repo`, `create image` and `create module`. Every step of a chain is
//! also a command: `create repo` calls `create image` in place rather than
//! writing an image of its own.
//!
//! Each of them collects every answer first and writes afterwards, which is why
//! no `apply` takes a `Prompt`.

use crate::copy;
use crate::diag::Issues;
use crate::layout;
use crate::prompt::Prompt;
pub use crate::ui::tree::Change;
use crate::ui::Choice;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Where a repository is hosted unless something says otherwise. The host is
/// read into the origin and the image URLs and nowhere else: nothing in the
/// tool learns about a second forge.
pub const HOST: &str = "github.com";

const GH_INSTALL: &str = "install gh from https://github.com/cli/cli";

/// The family-adapter role: what makes a family's package manager usable from
/// a build layer. Every family needs the same role filled by a different
/// module, which is why this is the name of a role and not a row of a
/// family-to-capability table.
const BUILD_ENVIRONMENT: &str = "build-environment";

/// The prefix every URL a repository writes is built from.
pub fn origin(host: &str, owner: &str) -> String {
    format!("https://{host}/{owner}")
}

/// What the directory a repository sits in calls it, which is the only name a
/// tree that is already written carries.
pub fn named_after_root(root: &Path) -> Option<String> {
    std::fs::canonicalize(root)
        .ok()
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
}

/// A row of the review screen, and so a point `Repo::collect` can be re-entered
/// at. The order is the order the questions are asked in: re-entering at a
/// field asks it and everything after it.
///
/// A row re-enters at its gate, not at its first field, which is what makes a
/// collapsed gate reversible in both directions — `provider` re-asks whether
/// there is one at all, so it can become `none`, and `none` can become one.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Name,
    Provider,
    Remote,
    Image,
    Base,
    Workflows,
    Publish,
    Scans,
    Daily,
}

/// What the flags gave, which only the first pass reads. A field asked again
/// opens on the answer it has rather than on the flag that seeded it; the root
/// is the exception, since nothing else says where the tree goes.
#[derive(Default)]
struct Given {
    name: Option<String>,
    host: Option<String>,
    owner: Option<String>,
    image: Option<String>,
    base: Option<String>,
    root: Option<PathBuf>,
}

/// The tree, the git repository, an image in it, then the remote, which is
/// optional and last: each step adds to what the one before it wrote.
pub struct Repo {
    name: String,
    id: String,
    root: PathBuf,
    host: String,
    /// Who owns it on the host, which is absent where scheduled builds were
    /// declined and no origin was composed.
    owner: Option<String>,
    assets: PathBuf,
    image: Option<Image>,
    /// The CI to generate, asked through the struct `set workflows` uses so the
    /// two cannot drift. Absent where there is no origin to run it on.
    workflows: Option<crate::set::Workflows>,
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
        // The flags were the first pass's. A field asked again opens on the
        // answer it has, not on the flag that seeded it; only the root a flag
        // named survives, since nothing else says where the tree goes.
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
            match crate::ui::review(copy::REVIEW, &drawn)? {
                // Nothing was written, so leaving is a leaving rather than a
                // failure, the way every other widget's is.
                None => return Ok(None),
                Some(at) if at == rows.len() => break,
                Some(at) => repo = Self::ask(rows[at].0, &given, Some(&repo), prompt)?,
            }
        }
        Ok(Some(repo))
    }

    /// Ask from `from` onward, keeping every answer before it. The procedure is
    /// the order and the gating, so re-entering it at a point is the only
    /// re-collection that cannot disagree with the first pass: what comes after
    /// an edited answer opens on its previous answer where that still exists,
    /// and is dropped where it no longer does.
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
                // One decision, one row, one entry point: the gate is asked
                // first, so `provider` can become `none` and back again.
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
                    // `gh` is github's, so the offer to create the repository is
                    // too, and it is what closes the block the origin line opens.
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
        Ok(Self {
            name,
            id,
            root,
            host,
            owner,
            assets,
            image,
            workflows,
            remote,
            install_gh,
        })
    }

    /// One row per piece of configuration, and nothing per question.
    ///
    /// A gate answered Yes is not a row — `provider github.com/someone` is the
    /// decision and the Yes is only how it was reached — and a gate answered No
    /// is one row saying `none`, which is what the repository will have and
    /// what re-enters the gate. Nothing collected disappears from the screen.
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
        // An action rather than a setting, said as what will happen: nothing
        // else on the screen says a remote will be made.
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
        rows
    }

    pub fn apply(&self) -> Result<(), String> {
        let mut wrote: Vec<(PathBuf, Change)> =
            crate::init::write(&self.root, &self.name, &self.assets)?
                .into_iter()
                .map(|path| (path, Change::Created))
                .collect();
        git_init(&self.root)?;
        println!("initialised a git repository in {}", self.root.display());
        if let Some(image) = &self.image {
            wrote.extend(image.apply(&self.root)?);
        }
        if let Some(workflows) = &self.workflows {
            // repo.kdl is already in `wrote`, as the file this run created.
            workflows.apply(&self.root)?;
        }
        if let (true, Some(owner)) = (self.remote, &self.owner) {
            create_remote(owner, &self.id)?;
            println!("created {}/{} on github", owner, self.id);
        }
        report(&self.root, &wrote);

        let Self { host, id, .. } = self;
        let mut next = Vec::new();
        // A process cannot move its parent, so the step it left you one above
        // is the first thing offered rather than the last thing implied.
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

/// The tree a create, import or copy wrote, hung off what the repository calls
/// itself rather than off the directory it happens to sit in.
pub fn report(root: &Path, wrote: &[(PathBuf, Change)]) {
    let id = crate::model::image::List::load(root).0.id;
    crate::ui::tree::print(&id, wrote, describe);
}

/// What a line in the tree says: what a later step added to a file that was
/// already there, and nothing for one this run wrote whole. What a kind of
/// file is for is a documentation job, not a column beside every name.
fn describe(_path: &Path, change: Option<&Change>) -> String {
    match change {
        Some(Change::Updated(edit)) => edit.clone(),
        _ => String::new(),
    }
}

/// A path a command wrote, said the way the tree draws it.
fn under(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

/// Where the repository is hosted. The catalog is the two forges the workflows
/// the tool ships know how to run under.
fn choose_host(current: Option<&str>, prompt: &Prompt) -> Result<String, String> {
    let options = [
        Choice::new(HOST, copy::HOST_GITHUB),
        Choice::new("forgejo", copy::HOST_FORGEJO),
    ];
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

/// A question with an answer already held opens on it; one asked for the first
/// time opens where it always did.
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

/// One image, in `<image-id>.image.kdl` at the repository root.
#[derive(Clone)]
pub struct Image {
    file: PathBuf,
    text: String,
    /// What it is called and what it is built on, which are the two rows the
    /// review screen draws for it and the two answers asking it again opens on.
    pub name: String,
    pub base: String,
    /// What the chosen base belongs to, which decides what CI can run here.
    pub family: String,
    /// Whether the offer of what the base cannot build without was taken, so
    /// that asking again opens on the answer rather than back on yes.
    took: bool,
    /// The image a second one takes the fallback away from, named in repo.kdl
    /// so that a bare build still builds what it built before.
    names_default: Option<String>,
}

impl Image {
    /// `repo` is what the repository is called, which the name falls back to,
    /// and `url` is the repository's own, which the images an existing one
    /// holds already carry.
    pub fn collect(
        root: &Path,
        name: Option<String>,
        base: Option<String>,
        repo: &str,
        url: Option<String>,
        flag: &str,
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
        let took = prev.map_or(true, |prev| prev.took);
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

        let mut catalog_issues = Issues::default();
        let (bases, _) = crate::base::catalog(root, &list.sources, &mut catalog_issues);
        if !catalog_issues.is_empty() {
            return Err(catalog_issues.plain());
        }
        let base = match base {
            Some(given) => given,
            None => choose_base(&bases, prev.map(|prev| prev.base.as_str()), prompt)?,
        };
        let family = match crate::base::find(&bases, &base) {
            Some(known) => known.family.clone(),
            None => prompt.text(
                None,
                copy::BASE_FAMILY,
                "`--base`, naming a base the catalog knows",
                bases.first().map(|base| base.family.as_str()),
            )?,
        };
        // A repository that is not written yet declares what the scaffold is
        // about to give it, which is the only thing `create repo` has to offer
        // modules against.
        let scaffolded;
        let sources = match list.sources.is_empty() && !root.join(layout::REPO_FILE).is_file() {
            true => {
                scaffolded =
                    crate::parse::repo::sources_in(&crate::init::sources(&crate::init::assets()?));
                scaffolded.as_slice()
            }
            false => list.sources.as_slice(),
        };
        let disk = crate::parse::disk::Disk::scan(root);
        let known = crate::base::find(&bases, &base);
        let roles = roles(known);
        let index = crate::provider::Index::scan(root, sources, &disk, false);
        let mut wanted = wanted(&index, &family, &roles);
        // The one moment this costs network: only where the question it is for
        // can be asked, and only where the collections already here did not
        // answer it. A fresh repository has fetched nothing, so what the base
        // says it needs is in a collection nothing has read, and an offer that
        // named none of it would be no offer at all.
        let fetched;
        if wanted.len() < roles.len() && prompt.asks() && !index.unread().is_empty() {
            // Nothing is written yet, and this run may end at the review screen
            // without anything ever being: the fetch goes to scratch rather
            // than laying a cache into a directory nobody asked for. The
            // repository's own fetch is `tect fetch modules`, later.
            let scratch = std::env::temp_dir().join(format!("tect-bases.{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&scratch);
            let _ = std::fs::create_dir_all(&scratch);
            let _ = std::fs::set_permissions(
                &scratch,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            );
            let cache = match root.join(layout::REPO_FILE).is_file() {
                true => root,
                false => scratch.as_path(),
            };
            fetched = crate::provider::Index::scan(cache, sources, &disk, true);
            wanted = self::wanted(&fetched, &family, &roles);
            let _ = std::fs::remove_dir_all(&scratch);
        }
        let seed = match wanted.is_empty() || offer(&base, &wanted, prompt, took)? {
            true => seeded(&wanted),
            false => String::new(),
        };
        let text = image_kdl(&name, url.as_deref(), &base, &family, known, &seed);
        Ok(Self {
            text,
            file,
            name,
            base,
            family,
            took: wanted.is_empty() || !seed.is_empty(),
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

/// The image a repository with one of them and no `default-image` falls back
/// to, which a second image takes away unless it is written down.
fn implicit_default(list: &crate::model::image::List) -> Option<String> {
    match (&list.default_image_id, list.images.as_slice()) {
        (None, [only]) => Some(only.id.clone()),
        _ => None,
    }
}

/// One `default-image` line at the end of repo.kdl, which does not carry the
/// node: an append, never a rewrite.
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

/// One flavour in an image's `flavours` block, creating the block when the
/// image has none. Neither `default` nor `pr-build` is scaffolded: each is
/// an edit, and `default` silently changes what a bare target builds.
pub struct Flavour {
    name: String,
    /// The image's `name`, which is what the writer walks to.
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

/// One of the bases the catalog holds, or one typed in: an unknown base is not
/// an error, it is a base nothing can say anything about.
fn choose_base(
    bases: &[crate::base::Base],
    current: Option<&str>,
    prompt: &Prompt,
) -> Result<String, String> {
    let options: Vec<Choice> = bases
        .iter()
        .map(|base| Choice::new(&base.image, &base.about))
        .collect();
    let at = current.and_then(|held| bases.iter().position(|base| base.image == held));
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

/// One module in the repository, and the offer to list it in an image, which
/// is a separate operation.
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

        let pkgs =
            match pkgs.is_empty() && prompt.confirm(copy::MODULE_PACKAGES, copy::YES, copy::NO)? {
                true => prompt
                    .text(None, copy::PACKAGE_NAMES, "`--pkg`", Some(""))?
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
                false => pkgs,
            };

        let text = module_kdl(&name, &family(root)?, &pkgs, &with)?;
        let listing = Listing::collect(root, images, prompt)?;
        listing.refuse_duplicate(&crate::model::image::List::load(root).0, &path, None)?;
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
        crate::init::put(&self.file, &self.text)?;
        let mut wrote = vec![(under(root, &self.file), Change::Created)];
        let (list, _) = crate::model::image::List::load(root);
        wrote.extend(self.listing.apply(&list, &self.path)?);
        Ok(wrote)
    }
}

/// The family the repository already builds on.
fn family(root: &Path) -> Result<String, String> {
    let (list, _) = crate::model::image::List::load(root);
    if let Some(family) = list
        .images
        .iter()
        .find_map(|image| image.base.as_ref().map(|base| base.family.clone()))
    {
        return Ok(family);
    }
    let mut issues = Issues::default();
    let bases = crate::base::catalog(root, &list.sources, &mut issues).0;
    if !issues.is_empty() {
        return Err(issues.plain());
    }
    bases
        .first()
        .map(|base| base.family.clone())
        .ok_or_else(|| "no base in the catalog to derive a module family from".to_string())
}

fn module_kdl(
    name: &str,
    family: &str,
    pkgs: &[String],
    with: &[(String, String)],
) -> Result<String, String> {
    let mut text = format!(
        "description \"{}\"\n\nsupports \"{family}\"\n",
        quotable(name)?
    );
    for (verb, value) in with {
        text.push_str(&format!("{} \"{}\"\n", quotable(verb)?, quotable(value)?));
    }
    if !pkgs.is_empty() {
        let mut listed = String::new();
        for pkg in pkgs {
            listed.push_str(&format!(" \"{}\"", quotable(pkg)?));
        }
        text.push_str(&format!("\npackages {{\n    {family}{listed}\n}}\n"));
    }
    Ok(text)
}

/// A value that would have to be escaped to survive being written into KDL.
fn quotable(value: &str) -> Result<&str, String> {
    match value.contains(['"', '\\', '\n']) {
        true => Err(format!(
            "`{value}` is not writable into a manifest as it is"
        )),
        false => Ok(value),
    }
}

/// Which images a module is listed in, or why none is. It asks even when there
/// is one image, because having a module in the repository and listing it in an
/// image are different decisions.
pub enum Listing {
    /// The picker was left, so the command writes nothing.
    Cancelled,
    /// Nothing to list it in yet.
    NoImage,
    /// None of them, which is an answer. `asked` is false where there was
    /// nobody to ask, and naming the flag is the only useful thing to say.
    Declined {
        asked: bool,
    },
    In(Vec<Listed>),
}

/// One place a module gets a line: which image file, which image in it, and the
/// flavour gating it, if any.
pub struct Listed {
    file: PathBuf,
    image: String,
    flavour: Option<String>,
}

impl Listing {
    /// The question alone: which images, and nothing about what goes in them.
    /// What one answer writes into each is checked per module by
    /// `refuse_duplicate`, since one answer covers a set.
    pub fn collect(root: &Path, given: Vec<String>, prompt: &Prompt) -> Result<Self, String> {
        let (list, _) = crate::model::image::List::load(root);
        if list.images.is_empty() {
            return Ok(Self::NoImage);
        }
        let targets = list.targets();
        let named: Vec<String> = targets.iter().map(ToString::to_string).collect();

        let chosen: Vec<usize> = match given.is_empty() {
            true => match ask(&list, &targets, prompt)? {
                crate::ui::Answer::Cancelled => return Ok(Self::Cancelled),
                crate::ui::Answer::Chosen(chosen) => chosen,
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
        // pair unreachable; a flag and the numbered list do not.
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

    /// The declaration `name` and `source` write, against what the images the
    /// answer names already say. A module gated to two flavours is listed under
    /// each, so only an overlap is a duplicate.
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
            // Where the two spellings differ the path is the whole of what
            // the reader is missing: `dev-tools` says nothing about there
            // already being a `.remote` node for it.
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

    /// The image files the module got a line in, which is nothing where no
    /// image took it. Appending is the only thing this does, and an image that
    /// already lists it is skipped, so every file named is an update.
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

    /// The images the answer writes into, by `name`, which is what a check
    /// made at the point of the edit is made against.
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

    /// The exact image targets the answer writes into.
    pub(crate) fn targets(&self) -> Vec<(&str, Option<&str>)> {
        let Self::In(listed) = self else {
            return Vec::new();
        };
        listed
            .iter()
            .map(|target| (target.image.as_str(), target.flavour.as_deref()))
            .collect()
    }

    /// One answer applied to a set: every member gets its line in every image
    /// the answer named, in the order they are given, under the collection it
    /// came from or none where the repository now owns it. A member an image
    /// already lists is skipped there alone, since the offer that brought it
    /// may have been for the other images only.
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

/// Where a declaration lives relative to `modules/`, which is what an image
/// entry records: a referenced member under `.remote`, an owned module at its
/// path.
fn dir_of(name: &str, source: Option<&str>) -> String {
    match source {
        Some(owner) => format!("{}/{owner}/{name}", crate::model::remote::REMOTE_DIR),
        None => name.to_string(),
    }
}

/// What the image already lists as `name`, where the way `into` names it
/// counts it: an ungated entry is in every flavour, so only an overlap is a
/// duplicate.
///
/// Matched by the module's name rather than the directory it lands in, so the
/// two ways one module reaches an image see each other: `import` puts it under
/// `.remote/<collection>/` and `copy` puts it directly under `modules/`, and
/// comparing directories made each spelling invisible to the other. A namesake
/// from a *different* collection is a different module and is not one of these.
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

/// The declaration a module gets, which is one line whatever wraps it.
fn leaf(name: &str) -> String {
    format!("module \"{name}\"")
}

/// The listing question: every image with its flavours under it, since listing
/// a module in an image and gating it to a flavour are the same question at two
/// depths. One image with no flavours is not a list, it is a yes or a no.
fn ask(
    list: &crate::model::image::List,
    targets: &[crate::model::image::Target],
    prompt: &Prompt,
) -> Result<crate::ui::Answer, String> {
    if let [only] = targets {
        let listed = prompt.confirm(&copy::list_in(&only.to_string()), copy::YES, copy::NO)?;
        return Ok(crate::ui::Answer::Chosen(match listed {
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

/// The blocks a module declaration sits inside, outermost first.
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

/// A declaration inside the blocks that have to be written around it.
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

/// One declaration before the closing brace of the deepest block on `chain`
/// that is already there, wrapped in the ones below it that are not. Every
/// other byte is left where it was: the tool creates whole files and appends
/// declarations, and never rewrites a value.
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

/// The modules a fresh image cannot build without: whatever fills the
/// family-adapter role, and whatever satisfies what the base row says it
/// requires. Two kinds of missing module, and they fail together on a fresh
/// repository — a base that is not a bootc image needs both, and neither is
/// there — so they are gathered as one list and asked as one question.
fn wanted<'a>(
    index: &'a crate::provider::Index,
    family: &str,
    roles: &[&str],
) -> Vec<&'a crate::provider::Provider> {
    let mut out: Vec<&crate::provider::Provider> = Vec::new();
    for capability in roles {
        // The role is filled per family, so the provider that fits this image
        // is the one wanted rather than the one that sorts first.
        let Some(provider) = index.adapter(capability, family) else {
            continue;
        };
        if !out.iter().any(|held| held.dir() == provider.dir()) {
            out.push(provider);
        }
    }
    out
}

/// The capabilities those modules are looked up by, which is also how many
/// answers a complete offer has: one short of that is a collection nothing has
/// read yet.
fn roles(base: Option<&crate::base::Base>) -> Vec<&str> {
    std::iter::once(BUILD_ENVIRONMENT)
        .chain(
            base.into_iter()
                .flat_map(|base| base.requires.iter().map(String::as_str)),
        )
        .collect()
}

/// The question, which names them: a person meeting a base for the first time
/// is told what it cannot build without. *Needs* rather than *requires*, since
/// only one of the two kinds is a `requires` on the base row and the other is
/// the family adapter, which no row declares. Nobody to ask takes them, which
/// is what the seed always did.
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
    prompt.confirm_current(copy::BRING_FOR_BASE, copy::YES, copy::NO, current)
}

/// Those modules as an image's `modules` block, each collection's grouped under
/// one `source`. An image with nothing to seed opens with an empty block, which
/// is what it always did.
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
    modules: &str,
) -> String {
    let urls = match url {
        Some(url) => format!(
            "\x20   url \"{url}\"\n\
             \x20   issues-url \"{url}/issues\"\n"
        ),
        None => String::new(),
    };
    let mut ships = String::new();
    if let Some(known) = known {
        for (node, names) in [
            ("provides", &known.provides),
            ("provides-file", &known.provides_files),
            ("requires", &known.requires),
        ] {
            if !names.is_empty() {
                let listed: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
                ships.push_str(&format!("\x20       {node} {}\n", listed.join(" ")));
            }
        }
        // Always written: the signature probe corrects the line it finds, and
        // an omitted one is a correction it cannot make.
        ships.push_str(&format!("\x20       signed #{}\n", known.signed));
    }
    format!(
        "image {{\n\
         \x20   name \"{name}\"\n\
         {urls}\n\
         \x20   base \"{base}\" {{\n\
         \x20       family \"{family}\"\n\
         {ships}\
         \x20   }}\n\
         \n\
         \x20   modules {{\n\
         {modules}\
         \x20   }}\n\
         }}\n"
    )
}

/// A repository inside a repository: the outer one would read the inner tree as
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

/// The repository the rest of the tree is committed into. Its own output is
/// dropped: what a run prints is the one line every other step prints.
fn git_init(root: &Path) -> Result<(), String> {
    // `main`, because the push line, the remote and every workflow name it.
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

/// Whether `gh` is there and signed in, both collect-time reads: what they
/// answer is which of the offers the flow makes.
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

    /// Two collections' worth and two of one collection's, so the block a
    /// fresh image opens with is one `source` per collection rather than one
    /// per module.
    #[test]
    fn the_seeded_block_groups_each_collections_modules_under_one_source() {
        let provider = |owner: Option<&str>, name: &str| crate::provider::Provider {
            owner: owner.map(str::to_string),
            name: name.to_string(),
            here: false,
            declares: crate::parse::module::Summary::default(),
        };
        // The second `tectonic-os` is not beside the first: grouping is by
        // owner, not by run, so a base whose `requires` interleaves two
        // collections still writes one block apiece.
        let held = [
            provider(Some("tectonic-os"), "debian-family"),
            provider(None, "mine"),
            provider(Some("tectonic-os"), "debian-bootc-base/bootc"),
        ];
        let wanted: Vec<&crate::provider::Provider> = held.iter().collect();
        assert_eq!(
            seeded(&wanted),
            "        source \"tectonic-os\" {\n\
             \x20           module \"debian-family\"\n\
             \x20           module \"debian-bootc-base/bootc\"\n\
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
            r#"image {
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

    #[test]
    fn a_catalogued_base_writes_what_it_ships_and_an_unknown_one_writes_nothing() {
        let bazzite = "ghcr.io/ublue-os/bazzite:stable";
        let seeded: Vec<crate::base::Base> =
            crate::base::catalog(Path::new("."), &[], &mut crate::diag::Issues::default()).0;
        let known = image_kdl(
            "Bazzite",
            None,
            bazzite,
            "fedora",
            crate::base::find(&seeded, bazzite),
            "",
        );
        assert!(
            known.contains("        provides \"rechunking\" \"flatpak\"\n"),
            "{known}"
        );
        assert!(
            known.contains("        provides-file \"/usr/bin/flatpak\"\n"),
            "{known}"
        );

        let shared = image_kdl(
            "Server",
            Some("https://github.com/someone/example"),
            bazzite,
            "fedora",
            None,
            "",
        );
        assert!(
            shared.contains("    url \"https://github.com/someone/example\"\n")
                && shared
                    .contains("    issues-url \"https://github.com/someone/example/issues\"\n"),
            "{shared}"
        );

        let described = crate::base::Base {
            image: "example.invalid/own:1".to_string(),
            family: "fedora".to_string(),
            provides: Vec::new(),
            provides_files: Vec::new(),
            requires: Vec::new(),
            about: "what a collection describes".to_string(),
            signed: true,
            span: crate::diag::Span::default(),
        };
        let extended = image_kdl(
            "Own",
            None,
            &described.image,
            "fedora",
            Some(&described),
            "",
        );
        assert!(extended.contains("        signed #true\n"), "{extended}");

        let unknown = image_kdl("Own", None, "example.invalid/own:1", "fedora", None, "");
        assert!(!unknown.contains("provides"), "{unknown}");
        assert!(
            unknown.contains("base \"example.invalid/own:1\" {\n        family \"fedora\"\n"),
            "{unknown}"
        );
    }

    /// A capability the catalog misspells is written into every image scaffolded
    /// on that base, where it suppresses nothing and satisfies nothing.
    #[test]
    fn every_catalogued_name_is_a_name() {
        use crate::model::image::is_name;
        let bases =
            crate::base::catalog(Path::new("."), &[], &mut crate::diag::Issues::default()).0;
        for base in bases {
            assert!(is_name(&base.family), "{}", base.image);
            for name in &base.provides {
                assert!(is_name(name), "{} provides {name}", base.image);
            }
            for path in &base.provides_files {
                assert!(path.starts_with('/'), "{} provides {path}", base.image);
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

        // A second member of the same collection joins the blocks now there.
        append(&file, "Example", &chain, "module \"editor\"").unwrap();
        let written = std::fs::read_to_string(&file).unwrap();
        assert_eq!(written.matches("flavour \"dx\"").count(), 1);
        assert_eq!(written.matches("source \"one\"").count(), 1);
        assert!(
            written.contains("module \"dev-tools\"\n                module \"editor\"\n"),
            "{written}"
        );

        // The block `create flavour` writes, which the image had none of.
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
}

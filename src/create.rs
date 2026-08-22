//! `create repo`, `create image` and `create module`. Every step of a chain is
//! also a command: `create repo` calls `create image` in place rather than
//! writing an image of its own.
//!
//! Each of them collects every answer first and writes afterwards, which is why
//! no `apply` takes a `Prompt`.

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

const SCHEDULED: &str = "This repo is designed to run scheduled builds of the repo's images via\n\
     Github or Forgejo actions.\n\
     Would you like to configure this now?";

const NO_GH: &str = "To create a repo from Tectonic requires the Github CLI tool 'gh' installed.\n\
     Would you like to install it now?";

const IMAGES: &str =
    "Tectonic defines images through kdl files. These image files define the base\n\
     image and modules to be included in the built image.\n\
     Would you like to create an image file now?";

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
    ) -> Result<Self, String> {
        let name = prompt.line(
            name.or_else(|| root_arg.as_deref().and_then(named_after_root)),
            "What will the repo be called?",
            "a name argument",
            "",
            None,
        )?;
        let id = crate::init::id(&name)?;
        let root = root_arg.unwrap_or_else(|| PathBuf::from(&id));
        refuse_nesting(&root)?;
        let assets = crate::init::assets()?;
        println!("Creating {id}...\n");

        let configure =
            host.is_some() || owner.is_some() || prompt.confirm(SCHEDULED, "Yes", "No")?;
        let host = match (configure, host) {
            (true, None) => choose_host(prompt)?,
            (_, given) => given.unwrap_or_else(|| HOST.to_string()),
        };
        let owner = match configure {
            true => Some(prompt.line(
                owner,
                &username(&host),
                "`--owner`",
                &format!("{host}/"),
                None,
            )?),
            false => None,
        };
        let mut remote = false;
        let mut install_gh = false;
        // `gh` is github's, so the offer to create the repository is too, and
        // it is what closes the block the origin line opens.
        let offering = host == HOST && prompt.asks();
        if let Some(named) = &owner {
            println!("Added {host}/{named}/{id} as the origin repo");
            if !offering {
                println!();
            }
            if offering
                && prompt.confirm(
                    "Would you like to create this repo on Github now?",
                    "Yes",
                    "Skip",
                )?
            {
                match (gh_installed(), gh_logged_in()) {
                    (false, _) => {
                        install_gh = prompt.confirm(NO_GH, "Yes", "Skip Github repo creation")?
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
        let image = match image_name.is_some() || prompt.confirm(IMAGES, "Yes", "No")? {
            true => Some(Image::collect(
                &root,
                image_name,
                base,
                &name,
                owner
                    .as_deref()
                    .map(|owner| format!("{}/{id}", origin(&host, owner))),
                "`--image`",
                prompt,
            )?),
            false => None,
        };
        let workflows = match configure {
            false => None,
            true => {
                let family = image.as_ref().map_or("", |image| image.family.as_str());
                let basis = crate::resolve::workflow::Basis::scaffolding(family);
                let every = crate::set::Workflows::every(&basis);
                crate::set::Workflows::collect(
                    &basis,
                    &every,
                    crate::resolve::workflow::DEFAULT_AT,
                    prompt,
                )?
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
        let mut next = vec![
            "tect generate".to_string(),
            "git add -A && git commit".to_string(),
        ];
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
        println!("\nnext, in {}:\n{}\n", self.root.display(), next.join("\n"));
        Ok(())
    }
}

/// The tree a create, import or copy wrote, hung off what the repository calls
/// itself rather than off the directory it happens to sit in.
pub fn report(root: &Path, wrote: &[(PathBuf, Change)]) {
    let id = crate::model::image::List::load(root).0.id;
    crate::ui::tree::print(&id, wrote, describe);
}

/// What one short phrase per kind of file says it is for, empty for a file
/// that speaks for itself. UI copy, written once and read forever.
fn describe(path: &Path) -> &'static str {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    if layout::is_image_file(&name) {
        return "an image: its base, and the modules it lists";
    }
    if path.starts_with(layout::MODULES) && path.components().count() > 1 {
        return match name.as_ref() {
            layout::MODULE_FILE => "what the module installs, places and provides",
            layout::RECORD_FILE => "where it was imported from, and its hash",
            layout::OVERLAY => "what it lays into the image",
            "module.sh" => "the shell its build layer runs",
            _ => "",
        };
    }
    match path.to_string_lossy().as_ref() {
        layout::REPO_FILE => "what the repo pins, and where modules come from",
        layout::MODULES => "every module this repo owns or imported",
        "README.md" => "yours to write",
        "lib" => "shell a module's build layer sources",
        "scripts" => "what CI runs, and what you run by hand",
        "disk_config" => "how a disk or installer image is shaped",
        ".github/renovate.json5" => "keeps the pinned versions moving",
        _ => "",
    }
}

/// A path a command wrote, said the way the tree draws it.
fn under(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

/// Where the repository is hosted. The catalog is the two forges the workflows
/// the tool ships know how to run under.
fn choose_host(prompt: &Prompt) -> Result<String, String> {
    let options = [
        Choice::new(HOST, "Github, and the workflows Tectonic ships"),
        Choice::new("forgejo", "a Forgejo instance, whose address you give"),
    ];
    match prompt.choose("Where will the repo be hosted?", &options)? {
        Some(0) | None => Ok(HOST.to_string()),
        _ => prompt.line(
            None,
            "What is the address of the Forgejo instance?",
            "`--host`",
            "",
            None,
        ),
    }
}

/// Github is asked for by name, and every other host by its address, which is
/// all the tool knows about one.
fn username(host: &str) -> String {
    match host {
        HOST => "What is your github username?".to_string(),
        host => format!("What is your username on {host}?"),
    }
}

/// One image, in `<image-id>.image.kdl` at the repository root.
pub struct Image {
    file: PathBuf,
    text: String,
    /// What the chosen base belongs to, which decides what CI can run here.
    pub family: String,
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
        prompt: &Prompt,
    ) -> Result<Self, String> {
        let name = prompt.line(
            name,
            "What will the image be called?",
            flag,
            "",
            crate::init::id(repo).is_ok().then_some(repo),
        )?;
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
            None => choose_base(&bases, prompt)?,
        };
        let family = match crate::base::find(&bases, &base) {
            Some(known) => known.family.clone(),
            None => prompt.text(
                None,
                "base family",
                "`--base`, naming a base the catalog knows",
                bases.first().map(|base| base.family.as_str()),
            )?,
        };
        // Costs no network, the way the base catalog beside it does: a
        // collection this machine has not fetched seeds nothing.
        let disk = crate::parse::disk::Disk::scan(root);
        let index = crate::provider::Index::scan(root, &list.sources, &disk, false);
        let text = image_kdl(
            &name,
            url.as_deref(),
            &base,
            &family,
            crate::base::find(&bases, &base),
            &seeded(index.adapter(BUILD_ENVIRONMENT, &family)),
        );
        Ok(Self {
            text,
            file,
            family,
            names_default,
        })
    }

    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        crate::init::put(&self.file, &self.text)?;
        let mut wrote = vec![(under(root, &self.file), Change::Created)];
        if let Some(was) = &self.names_default {
            append_default_image(root, was)?;
            wrote.push((under(root, Path::new(layout::REPO_FILE)), Change::Updated));
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
        let name = prompt.text(name, "flavour name", "a name argument", None)?;

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
                match prompt.choose("which image publishes it", &options)? {
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
        Ok(vec![(under(root, &self.file), Change::Updated)])
    }
}

/// One of the bases the catalog holds, or one typed in: an unknown base is not
/// an error, it is a base nothing can say anything about.
fn choose_base(bases: &[crate::base::Base], prompt: &Prompt) -> Result<String, String> {
    let options: Vec<Choice> = bases
        .iter()
        .map(|base| Choice::new(&base.image, &base.about))
        .collect();
    match prompt.choose("What is the base image for this image?", &options)? {
        Some(chosen) => Ok(bases[chosen].image.clone()),
        None => prompt.text(
            None,
            "base image",
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
        let name = prompt.text(name, "module name", "a name argument", None)?;
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
            match pkgs.is_empty() && prompt.confirm("does it install packages", "Yes", "No")? {
                true => prompt
                    .text(
                        None,
                        "package names, separated by spaces",
                        "`--pkg`",
                        Some(""),
                    )?
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
                false => pkgs,
            };

        let text = module_kdl(&name, &family(root)?, &pkgs, &with)?;
        let listing = Listing::collect(root, images, &path, None, prompt)?;
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
        wrote.extend(self.listing.apply(&self.path)?);
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
    /// `name` and `source` are the declaration the answer writes, which is what
    /// an image already listing it is recognised by.
    pub fn collect(
        root: &Path,
        given: Vec<String>,
        name: &str,
        source: Option<&str>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
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

        // The same thing against what the file already says: a module gated to
        // two flavours is listed under each, so only an overlap is a duplicate.
        let dir = match source {
            Some(owner) => format!("{}/{owner}/{name}", crate::model::remote::REMOTE_DIR),
            None => name.to_string(),
        };
        for target in chosen.iter().map(|at| &targets[*at]) {
            let Some(image) = list.images.iter().find(|image| image.id == target.image) else {
                continue;
            };
            let gated = target.flavour != crate::model::image::NO_FLAVOUR;
            let Some(held) = image.entries.iter().find(|entry| {
                entry.dir() == dir
                    && (!gated
                        || entry.flavour.is_none()
                        || entry.flavour.as_deref() == Some(target.flavour.as_str()))
            }) else {
                continue;
            };
            let at = crate::model::image::Target {
                image: image.id.clone(),
                flavour: held
                    .flavour
                    .clone()
                    .unwrap_or_else(|| crate::model::image::NO_FLAVOUR.to_string()),
            };
            return Err(match at.to_string() == target.to_string() {
                true => format!("`{at}` already lists `{name}`"),
                false => format!("`{at}` already lists `{name}`, so `{target}` lists it twice"),
            });
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

    /// The image files the module got a line in, which is nothing where no
    /// image took it. Appending is the only thing this does, so every one of
    /// them is an update.
    pub fn apply(&self, path: &str) -> Result<Vec<(PathBuf, Change)>, String> {
        self.apply_declaration(path, None)
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

    pub fn apply_source(&self, source: &str, name: &str) -> Result<Vec<(PathBuf, Change)>, String> {
        self.apply_declaration(name, Some(source))
    }

    fn apply_declaration(
        &self,
        name: &str,
        source: Option<&str>,
    ) -> Result<Vec<(PathBuf, Change)>, String> {
        let leaf = format!("module \"{name}\"");
        match self {
            Self::Cancelled => Ok(Vec::new()),
            Self::NoImage => {
                println!("no image lists it yet; `tect create image <name>` writes one");
                Ok(Vec::new())
            }
            Self::Declined { .. } => {
                let shown = wrap(&listed_in(None, source)[1..], &leaf);
                println!(
                    "next, to build it, list it in an image:\n {}",
                    shown.replace('\n', "\n ")
                );
                Ok(Vec::new())
            }
            Self::In(listed) => {
                let mut wrote: Vec<(PathBuf, Change)> = Vec::new();
                for target in listed {
                    append(
                        &target.file,
                        &target.image,
                        &listed_in(target.flavour.as_deref(), source),
                        &leaf,
                    )?;
                    // Two flavours of one image are two lines in one file.
                    if !wrote.iter().any(|(file, _)| *file == target.file) {
                        wrote.push((target.file.clone(), Change::Updated));
                    }
                }
                Ok(wrote)
            }
        }
    }
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
        let listed = prompt.confirm(&format!("list it in {only}"), "Yes", "No")?;
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
    prompt.choose_many("list it in images", &rows, &[])
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

/// The one module a fresh image opens with: whatever fills the family-adapter
/// role, wrapped in its source block where it is a collection's. An image with
/// nothing to fill it opens with an empty block, which is what it always did.
fn seeded(adapter: Option<&crate::provider::Provider>) -> String {
    let Some(adapter) = adapter else {
        return String::new();
    };
    let chain: Vec<(&str, Option<&str>)> = adapter
        .owner
        .iter()
        .map(|owner| ("source", Some(owner.as_str())))
        .collect();
    wrap(&chain, &format!("module \"{}\"", adapter.name))
        .lines()
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
             `dnf install git`"
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

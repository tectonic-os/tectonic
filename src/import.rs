//! `import module` references one collection member from an image; `copy
//! module` vendors one into the repository.

use crate::create::{report, Change, Listing};
use crate::dispatch::Error;
use crate::layout;
use crate::model::remote::{At, Collection, REMOTE_DIR};
use crate::prompt::Prompt;
use crate::provenance::record;
use crate::provider::Provider;
use crate::ui::Choice;
use std::path::{Path, PathBuf};

/// One collection that has the module, and where its directory is on disk.
pub struct Found {
    pub owner: String,
    /// The member's path below the collection root: the canonical name, which
    /// the typed one may only be a suffix of.
    pub name: String,
    pub dir: PathBuf,
}

/// The possible owner prefix and what follows it.
pub fn split(name: &str) -> (Option<&str>, &str) {
    match name.split_once('/') {
        Some((owner, module)) => (Some(owner), module),
        None => (None, name),
    }
}

fn names(sources: &[Collection]) -> String {
    sources
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every collection that has `name`. Never the first of them: which one an
/// ambiguous name comes from is the caller's to settle, and `<owner>/<name>` is
/// what settles it without asking.
///
/// `enforce` refuses an unpinned collection before it is used.
pub fn find(
    root: &Path,
    sources: &[Collection],
    name: &str,
    enforce: bool,
) -> Result<Vec<Found>, String> {
    // A collection may group its members in directories, so the name is a path
    // under it and every part of it is held to what one part always was.
    if name
        .split('/')
        .any(|part| part.is_empty() || part.starts_with('.'))
    {
        return Err(format!(
            "`{name}` is not a module: a module is named by a path of names, as `<path>`, or \
             `<owner>/<path>` to name one collection, and no part of it may be empty or start \
             with a dot"
        ));
    }
    if sources.is_empty() {
        const NONE: &str = "repo.kdl declares no `sources`, so there is no collection to import \
                            from";
        // The scaffold is data, so it says what to write only where it is there.
        let block = crate::init::assets().map(|dir| crate::init::sources(&dir));
        return Err(match block.as_deref().unwrap_or("") {
            "" => format!("{NONE}, and no collection is scaffolded to name one"),
            block => format!("{NONE}.\n\nThis is the block `tect create repo` writes:\n\n{block}"),
        });
    }

    let (possible_owner, rest) = split(name);
    let owner = possible_owner.filter(|owner| sources.iter().any(|source| source.name == *owner));
    let module = owner.map_or(name, |_| rest);
    let mut searched: Vec<&str> = Vec::new();
    let mut found: Vec<Found> = Vec::new();
    for collection in sources {
        if owner.is_some_and(|owner| owner != collection.name) {
            continue;
        }
        if enforce && collection.unpinned() {
            return Err(format!(
                "`{}` follows a moving ref with no `sha256`, so using it is verified \
                 against nothing; `audit {{ enforce #true }}` makes that an error. Pin the \
                 collection to a tag and its hash, or drop the enforcement",
                collection.name
            ));
        }
        searched.push(&collection.name);
        let tree = tree(root, collection)?.join(collection.subtree().unwrap_or(""));
        if tree.join(module).join(layout::MODULE_FILE).is_file() {
            found.push(Found {
                owner: collection.name.clone(),
                name: module.to_string(),
                dir: tree.join(module),
            });
        }
    }

    if found.is_empty() {
        // No member has the exact path. A typed name is also a suffix of a
        // member path at a `/` boundary — `why`'s rule, one predicate for both
        // — so every member of every eligible collection is a candidate. The
        // owner filter and the unpinned refusal above already ran over every
        // one of them, so `searched` stands.
        for collection in sources {
            if owner.is_some_and(|owner| owner != collection.name) {
                continue;
            }
            let tree = tree(root, collection)?.join(collection.subtree().unwrap_or(""));
            for path in crate::resolve::name::matching(&members(&tree), module) {
                found.push(Found {
                    owner: collection.name.clone(),
                    dir: tree.join(&path),
                    name: path,
                });
            }
        }
    }

    found.sort_by(|a, b| (&a.name, &a.owner).cmp(&(&b.name, &b.owner)));

    if found.is_empty() {
        return Err(format!(
            "no module called `{module}` in {}",
            searched.join(", ")
        ));
    }
    Ok(found)
}

/// Every member path below `tree`, as `catalog` walks it: directories only,
/// dot entries passed over, and the descent stops at a directory holding
/// `module.kdl`. The names are the paths below `tree`.
fn members(tree: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut dirs: Vec<PathBuf> = match std::fs::read_dir(tree) {
        Ok(dirs) => dirs.flatten().map(|entry| entry.path()).collect(),
        Err(_) => return out,
    };
    while let Some(dir) = dirs.pop() {
        let hidden = |name: &std::ffi::OsStr| name.to_string_lossy().starts_with('.');
        if !dir.is_dir() || dir.file_name().is_some_and(hidden) {
            continue;
        }
        let manifest = dir.join(layout::MODULE_FILE);
        if !manifest.is_file() {
            dirs.extend(
                std::fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.path()),
            );
            continue;
        }
        out.push(dir.strip_prefix(tree).unwrap_or(&dir).display().to_string());
    }
    out
}

/// Every module every declared collection holds, by name and then by
/// collection. `fetch` decides whether a collection that is not on this
/// machine is downloaded to answer or passed over.
///
/// The walk goes as deep as `Disk::scan`'s, so a member the collection groups
/// under a directory is named by its path and is otherwise a member like any
/// other. A dot directory is passed over: a collection read out of a working
/// tree carries `.git`.
pub fn catalog(root: &Path, sources: &[Collection], fetch: bool) -> Result<Vec<Provider>, String> {
    let mut listed: Vec<Provider> = Vec::new();
    for collection in sources {
        let tree = match fetch {
            true => tree(root, collection)?,
            false => match cached(root, collection) {
                Some(dir) => dir,
                None => continue,
            },
        };
        let tree = tree.join(collection.subtree().unwrap_or(""));
        if let Err(err) = std::fs::read_dir(&tree) {
            if !fetch {
                continue;
            }
            return Err(format!("`{}`: {}: {err}", collection.name, tree.display()));
        }
        for name in members(&tree) {
            listed.push(Provider {
                owner: Some(collection.name.clone()),
                declares: crate::parse::module::summary(
                    &tree.join(&name).join(layout::MODULE_FILE),
                ),
                name,
                here: false,
            });
        }
    }
    listed.sort_by(|a, b| (&a.name, &a.owner).cmp(&(&b.name, &b.owner)));
    Ok(listed)
}

/// The catalog as a question, with what it holds and how it is named.
fn offered(root: &Path, sources: &[Collection]) -> Result<(Vec<Provider>, Vec<Choice>), String> {
    let listed = catalog(root, sources, true)?;
    if listed.is_empty() {
        return Err(format!("no module in {}", names(sources)));
    }
    let options = listed
        .iter()
        .map(|module| Choice::new(module.qualified(), module.about()))
        .collect();
    Ok((listed, options))
}

fn unchosen(command: &str) -> String {
    format!("no module chosen; `tect {command} module <name>` names one")
}

/// Which module, out of everything the collections hold.
pub fn choose(
    root: &Path,
    sources: &[Collection],
    command: &str,
    prompt: &Prompt,
) -> Result<String, String> {
    let (listed, options) = offered(root, sources)?;
    match prompt.choose("which module", &options)? {
        Some(chosen) => Ok(listed[chosen].qualified()),
        None => Err(unchosen(command)),
    }
}

/// The same question taking a set, which is what one import brings: they share
/// one listing answer and one of each offer. A name on the command line is a
/// set of one, so the picker is the only way to name several.
fn choose_several(
    root: &Path,
    sources: &[Collection],
    command: &str,
    prompt: &Prompt,
) -> Result<Vec<String>, String> {
    let (listed, options) = offered(root, sources)?;
    let chosen = match prompt.choose_many("which modules", &options, &[])? {
        crate::ui::Answer::Chosen(chosen) => chosen,
        crate::ui::Answer::Cancelled => Vec::new(),
    };
    match chosen.is_empty() {
        true => Err(unchosen(command)),
        false => Ok(chosen.iter().map(|at| listed[*at].qualified()).collect()),
    }
}

/// The collection's tree where it is already on this machine: the directory it
/// is, an archive fetched at the hash it is still pinned to, or whatever the
/// last import of an unpinned one left. Nothing is fetched, so a reader that
/// only wants what is there costs no network.
pub fn cached(root: &Path, collection: &Collection) -> Option<PathBuf> {
    let dir = match &collection.at {
        At::Dir(dir) => root.join(dir),
        At::Archive(pin) if pin.unpinned() => {
            root.join(layout::SOURCES_CACHE).join(&collection.name)
        }
        At::Archive(pin) => {
            let stamp = root
                .join(layout::SOURCES_CACHE)
                .join(format!("{}.pin", collection.name));
            match std::fs::read_to_string(&stamp).ok().as_deref() == pin.sha256.as_deref() {
                true => root.join(layout::SOURCES_CACHE).join(&collection.name),
                false => return None,
            }
        }
    };
    dir.is_dir().then_some(dir)
}

/// The collection's tree on this machine: the directory it already is, or the
/// pinned archive, fetched and verified once and kept for the next lookup. An
/// unpinned one is fetched again every time, since the ref it follows has
/// moved by now for all anything here knows.
pub(crate) fn tree(root: &Path, collection: &Collection) -> Result<PathBuf, String> {
    if !collection.unpinned() {
        if let Some(dir) = cached(root, collection) {
            return Ok(dir);
        }
    }
    let remote = match &collection.at {
        At::Dir(dir) => {
            return Err(format!(
                "`{}` is {}, which is not a directory on this machine",
                collection.name,
                root.join(dir).display()
            ))
        }
        At::Archive(remote) => remote,
    };

    let dir = root.join(layout::SOURCES_CACHE).join(&collection.name);
    let pin = root
        .join(layout::SOURCES_CACHE)
        .join(format!("{}.pin", collection.name));
    let _ = std::fs::remove_dir_all(&dir);
    let url = remote.url_resolved().unwrap_or_default();
    let sha256 = (!remote.unpinned())
        .then(|| remote.sha256.as_deref())
        .flatten();
    crate::runtime::extract(&url, sha256, &dir, &["--strip-components=1"])
        .map_err(|err| format!("`{}`: {err}", collection.name))?;
    match sha256 {
        Some(sha256) => {
            std::fs::write(&pin, sha256).map_err(|err| format!("{}: {err}", pin.display()))?
        }
        None => drop(std::fs::remove_file(&pin)),
    }
    Ok(dir)
}

/// Where a copied module goes, refused when something is already there.
pub fn destination(root: &Path, found: &Found, module: &str) -> Result<PathBuf, String> {
    let dest = PathBuf::from(layout::MODULES).join(module);
    let dir = root.join(&dest);
    if !dir.exists() {
        return Ok(dest);
    }
    let mut issues = crate::diag::Issues::default();
    match record::read(&dir, &mut issues) {
        Some(record) if record.collection == found.owner => Err(format!(
            "`{module}` was already copied from `{}`",
            found.owner
        )),
        Some(record) => Err(format!(
            "`{module}` was copied from `{}`; refusing the namesake from `{}`",
            record.collection, found.owner
        )),
        None => Err(format!(
            "{} is already there; delete it to copy over it",
            dest.display()
        )),
    }
}

/// Copies the module in and leaves the record of where it came from beside its
/// manifest. The repository is a working tree, so committing what this wrote is
/// the user's.
pub fn vendor(
    root: &Path,
    sources: &[Collection],
    found: &Found,
    dest: &Path,
) -> Result<Vec<PathBuf>, String> {
    // The copy is the one moment a verifiable record is guaranteed, so a tree
    // that cannot be hashed refuses before anything is written. The source and
    // the copy are the same files, so the two hashes are the same value.
    let content = record::hash(&found.dir).ok_or_else(|| {
        format!(
            "{} cannot be hashed, so nothing can record what was copied",
            found.dir.display()
        )
    })?;
    let dir = root.join(dest);
    let mut wrote: Vec<PathBuf> = crate::init::copy_tree(&found.dir, &dir)?
        .into_iter()
        .map(|under| dest.join(under))
        .collect();
    let pin = sources
        .iter()
        .find(|c| c.name == found.owner)
        .and_then(Collection::pin);
    crate::init::put(
        &dir.join(record::RECORD),
        &record::write(&found.owner, pin, &content),
    )?;
    wrote.push(dest.join(record::RECORD));
    Ok(wrote)
}

fn select(
    name: Option<String>,
    root: &Path,
    sources: &[Collection],
    enforce: bool,
    command: &str,
    prompt: &Prompt,
) -> Result<(Found, String), Error> {
    let name = match name {
        Some(name) => name,
        None => choose(root, sources, command, prompt)?,
    };
    resolve(&name, root, sources, enforce, prompt)
}

/// One name to the collection it comes from, asked where more than one has it.
/// The ask is per name, so a set of them asks once each.
fn resolve(
    name: &str,
    root: &Path,
    sources: &[Collection],
    enforce: bool,
    prompt: &Prompt,
) -> Result<(Found, String), Error> {
    let mut found = find(root, sources, name, enforce)?;
    let module = split(name).1.to_string();
    let at = match found.as_slice() {
        [_] => 0,
        many => {
            let options: Vec<Choice> = many
                .iter()
                .map(|f| Choice::new(format!("{}/{}", f.owner, f.name), ""))
                .collect();
            prompt
                .choose(
                    &format!("`{module}` names more than one module; which one"),
                    &options,
                )?
                .ok_or_else(|| {
                    format!(
                        "`{module}` names more than one module; name which one, as `{}/{}`",
                        many[0].owner, many[0].name
                    )
                })?
        }
    };
    let found = found.swap_remove(at);
    let name = found.name.clone();
    Ok((found, name))
}

/// One collection member a reference places under `.remote`, and the name an
/// image lists it by.
struct Member {
    from: Found,
    name: String,
}

/// Which collection members an image references. One import brings a set: the
/// ones chosen, and whatever the offer brought along in front of them.
pub struct Module {
    members: Vec<Member>,
    listing: Listing,
    /// The CI the import makes runnable, where that offer was taken.
    workflows: Option<crate::set::Workflows>,
    /// The profile the images it is listed in are measured against, where the
    /// set claims rules one selects and that offer was taken.
    conforms: Vec<crate::set::Conforms>,
}

impl Module {
    /// Asks which ones when no name was given, and which collection when a name
    /// is in more than one. Then the three offers, once for the set: what it
    /// requires and nothing provides, the CI it makes runnable, and the profile
    /// its claims would have the images measured against.
    pub fn collect(
        name: Option<String>,
        root: &Path,
        sources: &[Collection],
        enforce: bool,
        images: Vec<String>,
        datastream: Option<&Path>,
        prompt: &Prompt,
    ) -> Result<Self, Error> {
        let (list, _) = crate::model::image::List::load(root);
        if list.images.is_empty() {
            return Err(
                "`import module` needs an image; run `tect create image <name>` first"
                    .to_string()
                    .into(),
            );
        }
        let asked = match name {
            Some(name) => vec![name],
            None => choose_several(root, sources, "import", prompt)?,
        };
        let picked = asked
            .iter()
            .map(|name| resolve(name, root, sources, enforce, prompt))
            .collect::<Result<Vec<(Found, String)>, Error>>()?;
        let declares: Vec<crate::parse::module::Summary> = picked
            .iter()
            .map(|(from, _)| crate::parse::module::summary(&from.dir.join(layout::MODULE_FILE)))
            .collect();

        let listing = Listing::collect(root, images, prompt)?;
        for (from, name) in &picked {
            listing.refuse_duplicate(&list, name, Some(&from.owner))?;
        }
        let named: Vec<String> = picked.iter().map(|(_, name)| name.clone()).collect();

        let also = short(root, sources, &list, &listing, &named, &declares, prompt)?
            .into_iter()
            .filter(|qualified| {
                !picked
                    .iter()
                    .any(|(from, name)| format!("{}/{name}", from.owner) == *qualified)
            })
            .map(|qualified| {
                let mut found = find(root, sources, &qualified, enforce)?;
                let from = found.swap_remove(0);
                let name = from.name.clone();
                Ok(Member { from, name })
            })
            .collect::<Result<Vec<Member>, String>>()?;

        // Both offers are about what an image ends up holding, so neither is
        // worth asking where the answer listed it in nothing.
        let unlocked = match listing.images().is_empty() {
            true => Vec::new(),
            false => {
                let args: Vec<String> =
                    declares.iter().flat_map(|held| held.args.clone()).collect();
                crate::resolve::workflow::unlocked(&list, &args)
            }
        };
        let workflows = match unlocked.as_slice() {
            [] => None,
            unlocked => {
                let named: Vec<&'static str> = unlocked.iter().map(|s| s.stem).collect();
                let rows: Vec<String> = unlocked
                    .iter()
                    .map(|s| format!("`{}` {}", s.stem, s.about))
                    .collect();
                let question = format!(
                    "{} {} CI runnable that this repo does not generate:\n{}\n\
                     Generate it now?",
                    said(&quoted(&picked)),
                    match picked.len() {
                        1 => "makes",
                        _ => "make",
                    },
                    rows.join("\n")
                );
                prompt
                    .confirm(&question, "Yes", "No")?
                    .then(|| crate::set::Workflows::adding(&list, &named))
            }
        };

        // What the set requires goes in first, so the list reads in build order.
        let mut members = also;
        members.extend(picked.into_iter().map(|(from, name)| Member { from, name }));
        let brought: Vec<String> = members.iter().map(|member| member.name.clone()).collect();
        let conforms = measured(
            &list, &listing, &named, &declares, datastream, &brought, prompt,
        )?;
        Ok(Self {
            members,
            listing,
            workflows,
            conforms,
        })
    }

    pub fn apply(&self, root: &Path) -> Result<(), String> {
        let wrote = self.write(root)?;
        if !wrote.is_empty() {
            report(root, &wrote);
        }
        Ok(())
    }

    /// Everything `apply` does but say so, for a caller drawing a tree of its
    /// own around it.
    pub(crate) fn write(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        match self.listing {
            Listing::Cancelled => return Ok(Vec::new()),
            Listing::NoImage => {
                return Err(
                    "`import module` needs an image; run `tect create image <name>` first".into(),
                )
            }
            Listing::Declined { asked: false } => {
                return Err(
                    "`import module` needs an image to list it in; name one with `--image`".into(),
                )
            }
            Listing::Declined { asked: true } => {
                return Err(
                    "an import is a reference an image lists, so listing it in none imports \
                     nothing"
                        .into(),
                )
            }
            Listing::In(_) => {}
        }
        for member in &self.members {
            let dir = layout::module(root, REMOTE_DIR)
                .join(&member.from.owner)
                .join(&member.name);
            let _ = std::fs::remove_dir_all(&dir);
            crate::init::copy_tree(&member.from.dir, &dir)?;
        }
        let listed: Vec<(&str, &str)> = self
            .members
            .iter()
            .map(|member| (member.from.owner.as_str(), member.name.as_str()))
            .collect();
        let (list, _) = crate::model::image::List::load(root);
        let mut wrote: Vec<(PathBuf, Change)> = self.listing.apply_source(&list, &listed)?;
        if let Some(workflows) = &self.workflows {
            wrote.extend(workflows.apply(root)?);
        }
        // Last, since it splices into the image files this just wrote the
        // module lines into, and the tree keeps the line that says both.
        for conforms in &self.conforms {
            wrote.extend(conforms.apply(root)?);
        }
        let brought: Vec<String> = self
            .members
            .iter()
            .map(|member| format!("{REMOTE_DIR}/{}/{}", member.from.owner, member.name))
            .collect();
        collisions(root, &brought);
        Ok(wrote)
    }

    /// The names the set brings, as the tree a caller of `write` draws says
    /// what it added.
    pub(crate) fn brought(&self) -> Vec<String> {
        self.members
            .iter()
            .map(|member| member.name.clone())
            .collect()
    }
}

/// The same import, made out of an offer somewhere else: named members listed
/// in one image, with none of `import module`'s own offers on top.
pub fn bring(
    root: &Path,
    sources: &[Collection],
    enforce: bool,
    named: &[String],
    image: &str,
) -> Result<Module, String> {
    let members = named
        .iter()
        .map(|qualified| {
            let mut found = find(root, sources, qualified, enforce)?;
            let from = found.swap_remove(0);
            let name = from.name.clone();
            Ok(Member { from, name })
        })
        .collect::<Result<Vec<Member>, String>>()?;
    Ok(Module {
        members,
        listing: Listing::collect(root, vec![image.to_string()], &Prompt::silent())?,
        workflows: None,
        conforms: Vec::new(),
    })
}

/// What the set requires that the images it is being listed in do not have, as
/// the collection members that would satisfy it. The offer is one question for
/// the set: declining it leaves a file that is still valid and a `check` that
/// says so.
fn short(
    root: &Path,
    sources: &[Collection],
    list: &crate::model::image::List,
    listing: &Listing,
    named: &[String],
    declares: &[crate::parse::module::Summary],
    prompt: &Prompt,
) -> Result<Vec<String>, String> {
    let targets: Vec<(&crate::model::image::Image, Option<&str>)> = listing
        .targets()
        .iter()
        .filter_map(|(named, flavour)| {
            list.images
                .iter()
                .find(|image| image.name == *named)
                .map(|image| (image, *flavour))
        })
        .collect();
    let requires: Vec<&String> = declares
        .iter()
        .flat_map(|held| held.requires.iter())
        // One of the set may be what another one of them needs.
        .filter(|want| {
            !declares
                .iter()
                .any(|held| held.provides.iter().any(|has| &has == want))
        })
        .collect();
    if targets.is_empty() || requires.is_empty() {
        return Ok(Vec::new());
    }
    let disk = crate::parse::disk::Disk::scan(root);
    let index = crate::provider::Index::scan(root, sources, &disk, false);

    let mut unmet: Vec<&String> = Vec::new();
    let mut bring: Vec<String> = Vec::new();
    for want in requires {
        if unmet.contains(&want) {
            continue;
        }
        let any = targets
            .iter()
            .any(|(image, flavour)| !image_has(image, *flavour, want, &index));
        // A provider the repository owns needs a line rather than an import,
        // which is what the unsatisfied-`requires` help already says.
        let Some(provider) = (match any {
            false => None,
            true => index.of(want).into_iter().find(|held| held.owner.is_some()),
        }) else {
            continue;
        };
        unmet.push(want);
        let qualified = provider.qualified();
        if !bring.contains(&qualified) {
            bring.push(qualified);
        }
    }
    if bring.is_empty() {
        return Ok(Vec::new());
    }

    // The images at least one of the unmet wants has no provider in.
    let lacking: Vec<(&crate::model::image::Image, Option<&str>)> = targets
        .iter()
        .copied()
        .filter(|(image, flavour)| {
            unmet
                .iter()
                .any(|want| !image_has(image, *flavour, want, &index))
        })
        .collect();

    let question = format!(
        "{} {} {}, which nothing in {} provides.\nImport {} as well?",
        said(
            &named
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
        ),
        match named.len() {
            1 => "requires",
            _ => "require",
        },
        said(
            &unmet
                .iter()
                .map(|want| format!("`{want}`"))
                .collect::<Vec<_>>()
        ),
        said(
            &lacking
                .iter()
                .map(|(image, flavour)| match flavour {
                    Some(flavour) => format!("`{}/{flavour}`", image.id),
                    None => format!("`{}`", image.id),
                })
                .collect::<Vec<_>>()
        ),
        said(&bring.iter().map(|at| format!("`{at}`")).collect::<Vec<_>>()),
    );
    match prompt.confirm(&question, "Yes", "No")? {
        true => Ok(bring),
        false => Ok(Vec::new()),
    }
}

/// Whether an image already has a provider for `want`: the base declares it,
/// or an entry the image lists provides it.
fn image_has(
    image: &crate::model::image::Image,
    flavour: Option<&str>,
    want: &str,
    index: &crate::provider::Index,
) -> bool {
    image
        .base
        .iter()
        .flat_map(|base| base.provides.iter().chain(base.provides_files.iter()))
        .any(|decl| decl.name == want)
        || image.entries.iter().any(|entry| {
            (entry.flavour.is_none() || entry.flavour.as_deref() == flavour)
                && index
                    .at(&entry.dir())
                    .is_some_and(|held| held.declares.provides.iter().any(|has| has == want))
        })
}

/// The collisions the import just wrote, said now so a red `check` does not
/// have to deliver them. Read off a fresh resolve of the written tree — the
/// same one the next `check` makes — so the two cannot disagree.
fn collisions(root: &Path, brought: &[String]) {
    let brought: std::collections::BTreeSet<String> = brought.iter().cloned().collect();
    let loaded = crate::load(root);
    for (image, resolved) in loaded.list.images.iter().zip(&loaded.resolved) {
        for line in crate::resolve::overlay::collisions(image, &resolved.shipped, &brought) {
            println!("{line}");
        }
    }
}

/// Which profile the set's claims would have the images it is listed in
/// measured against, as the question whether to declare one. `conforms` means
/// measure me against this rather than I pass this, so an image that has just
/// taken a claiming module is exactly the one worth measuring.
///
/// The content is probed the way `set conforms` probes it, since this writes
/// the same declaration and a profile has to be one the scan carries. No
/// content on this machine is no offer rather than a refusal: an import is not
/// a conformance command, and `tect set conforms` is the one that says so.
fn measured(
    list: &crate::model::image::List,
    listing: &Listing,
    named: &[String],
    declares: &[crate::parse::module::Summary],
    datastream: Option<&Path>,
    brought: &[String],
    prompt: &Prompt,
) -> Result<Vec<crate::set::Conforms>, String> {
    let claims: Vec<&String> = declares
        .iter()
        .flat_map(|held| held.satisfies.iter())
        .collect();
    if !prompt.asks() || claims.is_empty() {
        return Ok(Vec::new());
    }
    let images: Vec<&crate::model::image::Image> = listing
        .images()
        .iter()
        .filter_map(|named| list.images.iter().find(|image| image.name == *named))
        .filter(|image| image.conforms.is_empty())
        .collect();
    let Some(first) = images.first() else {
        return Ok(Vec::new());
    };
    let family = first.base.as_ref().map_or("", |base| base.family.as_str());
    let Ok(path) = crate::scap::content_path(family, datastream) else {
        return Ok(Vec::new());
    };
    let content = match crate::scap::content_of(&path) {
        Ok(content) => content,
        // A named datastream that does not read is a typo, and refuses the way
        // `check` and `coverage` do. One only this machine happens to have is
        // silence.
        Err(err) if datastream.is_some() => return Err(err),
        Err(_) => return Ok(Vec::new()),
    };
    let claimed = crate::scap::reached(&content, claims.into_iter());
    let profiles: Vec<&crate::scap::Profile> = content
        .profiles
        .iter()
        .filter(|profile| !content.selected(&profile.id).is_disjoint(&claimed))
        .collect();
    if profiles.is_empty() {
        return Ok(Vec::new());
    }

    let measured = said(
        &images
            .iter()
            .map(|i| format!("`{}`", i.id))
            .collect::<Vec<_>>(),
    );
    println!(
        "{} {} rules {} {}, and {measured} {} no `conforms`.\n{}\n",
        said(
            &named
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
        ),
        match named.len() {
            1 => "claims",
            _ => "claim",
        },
        said(
            &profiles
                .iter()
                .map(|p| format!("`{}`", p.name()))
                .collect::<Vec<_>>()
        ),
        match profiles.len() {
            1 => "selects",
            _ => "select",
        },
        match images.len() {
            1 => "declares",
            _ => "declare",
        },
        crate::set::cost(&measured, list.audit_enforce),
    );
    let options: Vec<Choice> = profiles
        .iter()
        .map(|profile| Choice::new(profile.name(), &profile.title))
        .collect();
    let Some(chosen) = prompt.choose("which profile", &options)? else {
        return Ok(Vec::new());
    };
    Ok(images
        .iter()
        .map(|image| {
            crate::set::Conforms::declaring(image, profiles[chosen].name(), brought.to_vec())
        })
        .collect())
}

/// A set of members as a sentence names them.
fn quoted(picked: &[(Found, String)]) -> Vec<String> {
    picked.iter().map(|(_, name)| format!("`{name}`")).collect()
}

/// A list as a sentence reads it.
pub(crate) fn said(items: &[String]) -> String {
    match items.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection(name: &str, path: &str) -> Collection {
        Collection {
            name: name.to_string(),
            at: At::Dir(path.to_string()),
            span: crate::diag::Span::default(),
        }
    }

    #[test]
    fn a_complete_nested_path_wins_before_an_owner_prefix() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sources = [collection("four", "tests/collections/four")];
        let found = find(root, &sources, "hardening/coredumps", false).unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].owner, "four");
        assert_eq!(found[0].name, "hardening/coredumps");
    }

    #[test]
    fn exact_matches_are_sorted_like_suffix_matches() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sources = [
            collection("two", "tests/collections/two"),
            collection("one", "tests/collections/one"),
        ];
        let found = find(root, &sources, "browser", false).unwrap();
        let owners: Vec<&str> = found.iter().map(|found| found.owner.as_str()).collect();

        assert_eq!(owners, ["one", "two"]);
    }

    #[test]
    fn a_provider_in_one_flavour_does_not_cover_its_sibling() {
        let root =
            std::env::temp_dir().join(format!("tect-flavour-provider-{}", std::process::id()));
        crate::init::put(
            &root.join("image.kdl"),
            r#"image {
    name "Example"
    base "example" { family "fedora" }
    flavours {
        dev
        server
    }
    modules { flavour "dev" { module "provider" } }
}
"#,
        )
        .unwrap();
        crate::init::put(
            &root.join("modules/provider/module.kdl"),
            "description \"Provider\"\nsupports \"fedora\"\nprovides \"tool\"\n",
        )
        .unwrap();
        let (list, _) = crate::model::image::List::load(&root);
        let disk = crate::parse::disk::Disk::scan(&root);
        let index = crate::provider::Index::scan(&root, &[], &disk, false);
        let image = &list.images[0];

        assert!(image_has(image, Some("dev"), "tool", &index));
        assert!(!image_has(image, Some("server"), "tool", &index));
        assert!(!image_has(image, None, "tool", &index));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_unhashable_copy_writes_nothing() {
        let root =
            std::env::temp_dir().join(format!("tect-unhashable-copy-{}", std::process::id()));
        let found = Found {
            owner: "one".into(),
            name: "missing".into(),
            dir: root.join("missing"),
        };
        let dest = Path::new("modules/missing");

        assert!(vendor(&root, &[], &found, dest).is_err());
        assert!(!root.join(dest).exists());
    }

    #[test]
    fn a_cancelled_listing_writes_nothing() {
        let root = std::env::temp_dir().join(format!("tect-cancel-import-{}", std::process::id()));
        let from = root.join("collection/module");
        crate::init::put(&from.join(layout::MODULE_FILE), "description \"x\"\n").unwrap();

        Module {
            members: vec![Member {
                from: Found {
                    owner: "one".into(),
                    name: "module".into(),
                    dir: from.clone(),
                },
                name: "module".into(),
            }],
            listing: Listing::Cancelled,
            workflows: None,
            conforms: Vec::new(),
        }
        .apply(&root)
        .unwrap();
        assert!(!root.join("modules/.remote/one/module").exists());

        Copy {
            from: Found {
                owner: "one".into(),
                name: "module".into(),
                dir: from,
            },
            dest: PathBuf::from("modules/module"),
            name: "module".into(),
            listing: Listing::Cancelled,
        }
        .apply(&root, &[])
        .unwrap();
        assert!(!root.join("modules/module").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn no_image_is_refused_before_a_module_is_resolved() {
        let root =
            std::env::temp_dir().join(format!("tect-no-import-image-{}", std::process::id()));
        crate::init::put(
            &root.join(layout::REPO_FILE),
            "schema-version 1\nname \"Example\"\n",
        )
        .unwrap();
        let result = Module::collect(
            Some("nosuch".into()),
            &root,
            &[],
            false,
            Vec::new(),
            None,
            &Prompt::silent(),
        );
        let message = match result {
            Err(err) => err.message().to_string(),
            Ok(_) => panic!("an import with no image was accepted"),
        };
        assert!(message.contains("needs an image"), "{message}");
        let _ = std::fs::remove_dir_all(root);
    }
}

/// Which module is copied in, where it goes, and which image lists it.
pub struct Copy {
    from: Found,
    dest: PathBuf,
    name: String,
    listing: Listing,
}

impl Copy {
    pub fn collect(
        name: Option<String>,
        root: &Path,
        sources: &[Collection],
        enforce: bool,
        images: Vec<String>,
        prompt: &Prompt,
    ) -> Result<Self, Error> {
        let (from, name) = select(name, root, sources, enforce, "copy", prompt)?;
        let dest = destination(root, &from, &name)?;
        let listing = Listing::collect(root, images, prompt)?;
        listing.refuse_duplicate(&crate::model::image::List::load(root).0, &name, None)?;
        Ok(Self {
            from,
            dest,
            name,
            listing,
        })
    }

    pub fn apply(&self, root: &Path, sources: &[Collection]) -> Result<(), String> {
        if self.listing.cancelled() {
            return Ok(());
        }
        let mut wrote: Vec<(PathBuf, Change)> = vendor(root, sources, &self.from, &self.dest)?
            .into_iter()
            .map(|path| (path, Change::Created))
            .collect();
        let (list, _) = crate::model::image::List::load(root);
        wrote.extend(self.listing.apply(&list, &self.name)?);
        collisions(
            root,
            &[self
                .dest
                .strip_prefix(layout::MODULES)
                .map(|path| path.display().to_string())
                .unwrap_or_default()],
        );
        report(root, &wrote);
        Ok(())
    }
}

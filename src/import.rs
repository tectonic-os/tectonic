//! `import component` copies collection members into the repository, records each
//! one in the lock, and lists them in the image the user named.

use crate::copy;
use crate::error::Error;
use crate::layout;
use crate::model::remote::{Collection, Kind as SourceKind};
use crate::provenance::lock;
use crate::provider::Provider;
use crate::report::report;
use crate::said;
use crate::setup::Listing;
use common::prompt::Prompt;
use common::ui::tree::Change;
use common::ui::Choice;
use std::path::{Path, PathBuf};

/// One collection that has the component, and where its directory is on disk.
#[derive(Clone)]
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
        Some((owner, component)) => (Some(owner), component),
        None => (None, name),
    }
}

fn names(sources: &[Collection]) -> String {
    sources
        .iter()
        .filter(|source| source.kind == SourceKind::Components)
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every collection that has `name`. Never the first of them: which one an
/// ambiguous name comes from is the caller's to settle. `enforce` refuses an
/// unpinned collection before it is used.
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
            "`{name}` is not a component: a component is named by a path of names, as `<path>`, or \
             `<owner>/<path>` to name one collection, and no part of it may be empty or start \
             with a dot"
        ));
    }
    no_library(sources)?;

    let (possible_owner, rest) = split(name);
    let owner = possible_owner.filter(|owner| {
        sources
            .iter()
            .any(|source| source.kind == SourceKind::Components && source.name == *owner)
    });
    let component = owner.map_or(name, |_| rest);
    let mut searched: Vec<&str> = Vec::new();
    let mut found: Vec<Found> = Vec::new();
    for collection in sources {
        if collection.kind != SourceKind::Components {
            continue;
        }
        if owner.is_some_and(|owner| owner != collection.name) {
            continue;
        }
        verified(collection, enforce)?;
        searched.push(&collection.name);
        let tree =
            crate::collection::tree(root, collection)?.join(collection.subtree().unwrap_or(""));
        if tree.join(component).join(layout::MANIFEST_FILE).is_file() {
            found.push(Found {
                owner: collection.name.clone(),
                name: component.to_string(),
                dir: tree.join(component),
            });
        }
    }

    if found.is_empty() {
        // No member has the exact path. A typed name is also a suffix of a
        // member path at a `/` boundary — `why`'s rule — so every member of
        // every eligible collection is a candidate. The owner filter and the
        // unpinned refusal above already ran over all of them.
        for collection in sources {
            if collection.kind != SourceKind::Components {
                continue;
            }
            if owner.is_some_and(|owner| owner != collection.name) {
                continue;
            }
            let tree =
                crate::collection::tree(root, collection)?.join(collection.subtree().unwrap_or(""));
            for path in
                crate::resolve::name::matching(&crate::collection::members(&tree), component)
            {
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
            "no component called `{component}` in {}",
            searched.join(", ")
        ));
    }
    Ok(found)
}

/// Refuses an unpinned collection under `audit { enforce #true }`, because
/// nothing verifies what it holds. Every path that copies a member in asks
/// this first.
pub(crate) fn verified(collection: &Collection, enforce: bool) -> Result<(), String> {
    if enforce && collection.unpinned() {
        return Err(format!(
            "`{}` follows a moving ref with no `sha256`, so using it is verified against \
             nothing; `audit {{ enforce #true }}` makes that an error. Pin the collection to a \
             tag and its hash, or drop the enforcement",
            collection.name
        ));
    }
    Ok(())
}

/// Refuses a repository that declares no component library, and shows the block
/// that would declare one.
fn no_library(sources: &[Collection]) -> Result<(), String> {
    if sources
        .iter()
        .any(|source| source.kind == SourceKind::Components)
    {
        return Ok(());
    }
    const NONE: &str = "repo.kdl declares no `sources`, so there is no collection to import from";
    let block = crate::init::sources_block(&crate::setup::Libraries::every());
    Err(format!(
        "{NONE}.\n\nThis is the block `tect create repo` writes:\n\n{block}"
    ))
}

/// The catalog as a question, with what it holds and how it is named. A
/// member that cannot join the target stays on the list as unavailable, with
/// the reason beside it, so the user sees what the library holds.
fn offered(
    root: &Path,
    sources: &[Collection],
    fit: Option<&Fit>,
) -> Result<(Vec<Provider>, Vec<Choice>), String> {
    no_library(sources)?;
    let listed = crate::provider::catalog(root, sources, true)?;
    if listed.is_empty() {
        return Err(format!("no component in {}", names(sources)));
    }
    let options = listed
        .iter()
        .map(|component| {
            let group = component
                .name
                .split_once('/')
                .map_or("", |(group, _)| group);
            match fit.and_then(|fit| fit.unavailable(&component.declares)) {
                Some(why) => Choice::new(component.qualified(), why).unavailable(),
                None => Choice::new(component.qualified(), component.about()),
            }
            .within(group)
        })
        .collect();
    Ok((listed, options))
}

/// Holds the image and flavour an import is for, and the provider index that
/// the clash rule reads. The picker and a named import both ask this, so they
/// refuse alike.
struct Fit<'a> {
    image: &'a crate::model::image::Image,
    flavour: Option<&'a str>,
    index: crate::provider::Index,
}

impl<'a> Fit<'a> {
    /// Finds the target of an import, which an import for no image has none of.
    fn of(
        root: &Path,
        sources: &[Collection],
        list: &'a crate::model::image::List,
        listing: &'a Listing,
    ) -> Option<Self> {
        let (named, flavour) = listing.targets().into_iter().next()?;
        let image = list.images.iter().find(|image| image.name == named)?;
        let disk = crate::parse::disk::Disk::scan(root);
        Some(Self {
            image,
            flavour,
            index: crate::provider::Index::scan(root, sources, &disk, false),
        })
    }

    /// Says why a component cannot join the target, which is the first of the
    /// family it does not support and a capability it would make a conflict.
    fn unavailable(&self, declares: &crate::parse::component::Summary) -> Option<String> {
        self.unsupported(declares).or_else(|| {
            self.clash(declares)
                .map(|(capability, held)| copy::exclusive_to(&capability, &held[0].path))
        })
    }

    /// Says why a component cannot be listed in the target. A base component
    /// never is, because an image lists its one base itself. A component
    /// that cannot build on the target's family is not either, through the
    /// rule `check` applies. A base that names no family leaves every component
    /// available.
    fn unsupported(&self, declares: &crate::parse::component::Summary) -> Option<String> {
        if declares
            .provides
            .iter()
            .any(|name| name == crate::parse::component::BASE_IMAGE)
        {
            return Some(copy::IS_BASE.to_string());
        }
        let family = self
            .image
            .base
            .as_ref()
            .map(|base| base.family.as_str())
            .filter(|family| !family.is_empty())?;
        (!crate::resolve::targets(&declares.targets, family))
            .then(|| copy::unsupported(&declares.targets, family))
    }

    /// Names the first capability that the component would make a conflict in the
    /// image, and every entry that already provides it. A component the base
    /// covers entirely is suppressed, so it never clashes. An entry clashes
    /// only if it builds into a target with the import, as `check` counts it.
    fn clash(
        &self,
        declares: &crate::parse::component::Summary,
    ) -> Option<(String, Vec<&'a crate::model::image::Entry>)> {
        let covered = self
            .image
            .base
            .as_ref()
            .is_some_and(|base| base.covers(declares.provides.iter().map(String::as_str)));
        if covered {
            return None;
        }
        for capability in &declares.provides {
            let import = (self.flavour, declares.exclusive.contains(capability));
            let held: Vec<&'a crate::model::image::Entry> = self
                .image
                .entries
                .iter()
                .filter(|entry| {
                    self.index.at(&entry.path).is_some_and(|provider| {
                        provider.declares.provides.contains(capability)
                            && crate::resolve::conflict(
                                import,
                                (
                                    entry.flavour.as_deref(),
                                    provider.declares.exclusive.contains(capability),
                                ),
                            )
                    })
                })
                .collect();
            if !held.is_empty() {
                return Some((capability.clone(), held));
            }
        }
        None
    }

    /// Asks whether the import replaces the entries that already provide an
    /// exclusive capability. The answer removes their list lines, and their
    /// component directories stay where they are.
    fn settle(
        &self,
        capability: &str,
        held: &[&crate::model::image::Entry],
        prompt: &Prompt,
    ) -> Result<Vec<Unlisted>, String> {
        let named: Vec<String> = held.iter().map(|entry| entry.path.clone()).collect();
        let holder = said(&named);
        let options = [
            Choice::new(copy::replace(&holder), ""),
            Choice::new(copy::CANCEL, ""),
        ];
        let answer = match prompt.asks() {
            true => prompt.choose(
                &copy::exclusive_clash(capability, &holder, &self.image.name),
                &options,
            )?,
            false => None,
        };
        match answer {
            Some(0) => Ok(held
                .iter()
                .map(|entry| Unlisted {
                    file: PathBuf::from(self.image.src.name()),
                    image: self.image.name.clone(),
                    flavour: entry.flavour.clone(),
                    name: entry.path.clone(),
                })
                .collect()),
            Some(_) => Err(format!(
                "nothing imported: `{capability}` is exclusive to `{holder}` in `{}`",
                self.image.name
            )),
            None => Err(format!(
                "`{capability}` is exclusive, and `{holder}` already provides it in `{}`; run \
                 `tect import component` in a terminal to replace it",
                self.image.name
            )),
        }
    }
}

/// Holds one image entry that a replacing import removes.
struct Unlisted {
    file: PathBuf,
    image: String,
    flavour: Option<String>,
    name: String,
}

fn unchosen() -> String {
    "no component chosen; `tect import component <name>` names one".to_string()
}

/// Which components, out of everything the collections hold. One run brings a
/// set: they share one target and one of each offer. A name on the command
/// line is a set of one, so the picker is the only way to name several.
fn choose_several(
    root: &Path,
    sources: &[Collection],
    fit: Option<&Fit>,
    prompt: &Prompt,
) -> Result<Vec<String>, String> {
    let (listed, options) = offered(root, sources, fit)?;
    let chosen = match prompt.choose_many(copy::WHICH_COMPONENTS, &options, &[])? {
        common::ui::Answer::Chosen(chosen) => chosen,
        common::ui::Answer::Cancelled => Vec::new(),
    };
    match chosen.is_empty() {
        true => Err(unchosen()),
        false => Ok(chosen.iter().map(|at| listed[*at].qualified()).collect()),
    }
}

/// Answers the member `owner/name` from the tree that the provider index read.
/// Nothing is fetched, so an offer vendors the same files it showed the user.
pub(crate) fn cached_member(
    root: &Path,
    sources: &[Collection],
    enforce: bool,
    owner: &str,
    name: &str,
) -> Result<Option<Found>, String> {
    let Some(collection) = sources
        .iter()
        .find(|source| source.kind == SourceKind::Components && source.name == owner)
    else {
        return Ok(None);
    };
    verified(collection, enforce)?;
    let Some(tree) = crate::collection::cached(root, collection) else {
        return Ok(None);
    };
    let dir = tree.join(collection.subtree().unwrap_or("")).join(name);
    if !dir.join(layout::MANIFEST_FILE).is_file() {
        return Ok(None);
    }
    // A symbolic link in the library can point the member outside the cache.
    // The copy and the lock hash both follow it.
    let real = |path: &Path| {
        path.canonicalize()
            .map_err(|err| format!("{}: {err}", path.display()))
    };
    if !real(&dir)?.starts_with(real(&tree)?) {
        return Err(format!(
            "{}: library `{owner}` links `{name}` outside its own tree",
            dir.display()
        ));
    }
    Ok(Some(Found {
        owner: owner.to_string(),
        name: name.to_string(),
        dir,
    }))
}

/// Copies each member in under its local name and records it in the lock. The
/// returned tree lines name each copy and the lock.
pub(crate) fn vendor_all(
    root: &Path,
    sources: &[Collection],
    members: &[(&Found, &str)],
    tracked: bool,
) -> Result<Vec<(PathBuf, Change)>, String> {
    let existed = layout::lock(root).exists();
    let mut wrote: Vec<(PathBuf, Change)> = Vec::new();
    for (from, name) in members {
        wrote.extend(
            vendor(root, sources, from, name, tracked)?
                .into_iter()
                .map(|path| (path, Change::Created)),
        );
    }
    if !members.is_empty() {
        let names: Vec<String> = members
            .iter()
            .map(|(_, name)| (*name).to_string())
            .collect();
        wrote.push((
            PathBuf::from(layout::COMPONENTS).join(layout::LOCK_FILE),
            match existed {
                true => Change::Updated(format!("{} recorded", said(&names))),
                false => Change::Created,
            },
        ));
    }
    Ok(wrote)
}

/// Copies the component in under its local name and records it in the lock. The
/// repository is a working tree, so committing what this wrote is the user's.
fn vendor(
    root: &Path,
    sources: &[Collection],
    from: &Found,
    name: &str,
    tracked: bool,
) -> Result<Vec<PathBuf>, String> {
    // The copy is the one moment a verifiable record is guaranteed, so a tree
    // that cannot be hashed refuses before anything is written. The source and
    // the copy are the same files, so the two hashes are the same value.
    let content = lock::hash(&from.dir).ok_or_else(|| {
        format!(
            "{} cannot be hashed, so nothing can record what was copied",
            from.dir.display()
        )
    })?;
    let mut pin = crate::provenance::Evidence::new(crate::diag::Span::default());
    if let Some(library) = sources
        .iter()
        .find(|c| c.kind == SourceKind::Components && c.name == from.owner)
        .and_then(Collection::pin)
    {
        pin.url.clone_from(&library.url);
        pin.version.clone_from(&library.version);
    }
    let record = lock::Record {
        path: name.to_string(),
        library: from.owner.clone(),
        upstream: from.name.clone(),
        pin,
        content,
        tracked,
    };
    // The lock is rendered before the copy, so a lock that cannot take the
    // entry refuses before anything is written.
    let locked = lock::with(root, record)?;
    let dest = PathBuf::from(layout::COMPONENTS).join(name);
    let wrote = crate::init::copy_component(&from.dir, &root.join(&dest))?
        .into_iter()
        .map(|under| dest.join(under))
        .collect();
    crate::init::put(&layout::lock(root), &locked)?;
    Ok(wrote)
}

/// One name to the collection it comes from, asked where more than one has it.
/// The ask is per name, so a set of them asks once each.
fn resolve(
    name: &str,
    root: &Path,
    sources: &[Collection],
    enforce: bool,
    prompt: &Prompt,
) -> Result<Found, Error> {
    let mut found = find(root, sources, name, enforce)?;
    let component = split(name).1.to_string();
    let at = match found.as_slice() {
        [_] => 0,
        many => {
            let options: Vec<Choice> = many
                .iter()
                .map(|f| Choice::new(format!("{}/{}", f.owner, f.name), ""))
                .collect();
            prompt
                .choose(
                    &format!("`{component}` names more than one component; which one"),
                    &options,
                )?
                .ok_or_else(|| {
                    format!(
                        "`{component}` names more than one component; name which one, as `{}/{}`",
                        many[0].owner, many[0].name
                    )
                })?
        }
    };
    Ok(found.swap_remove(at))
}

/// Holds one collection member and the local name it is listed by. `vendor` is false
/// where the answer listed a component the repository already holds.
struct Member {
    from: Found,
    name: String,
    vendor: bool,
}

impl Member {
    /// Reads the manifest the image will list. A component the repository already
    /// holds is judged by its own manifest, never by the library's.
    fn declares(&self, root: &Path) -> crate::parse::component::Summary {
        let owned;
        let dir = match self.vendor {
            true => self.from.dir.as_path(),
            false => {
                owned = layout::component(root, &self.name);
                owned.as_path()
            }
        };
        crate::parse::component::summary(&dir.join(layout::MANIFEST_FILE))
    }
}

/// Carries what the user asked `import component` for.
pub struct Request<'a> {
    pub name: Option<String>,
    pub image: Option<String>,
    pub datastream: Option<&'a Path>,
    pub dependencies: bool,
}

/// Holds the component names that one import adds, their summaries and the image
/// listing that receives them.
struct Listed<'a> {
    listing: &'a Listing,
    names: &'a [String],
    declares: &'a [crate::parse::component::Summary],
}

/// Which collection members an image takes. One run brings a set: the ones
/// chosen, and the providers the offer brought along in front of them.
pub struct Component {
    members: Vec<Member>,
    listing: Listing,
    /// Says whether the copies track upstream. The user answers once for the
    /// set.
    tracked: bool,
    /// Lists the entries that a replacing answer removes from the target.
    unlisted: Vec<Unlisted>,
    /// The CI it makes runnable, where that offer was taken.
    workflows: Option<crate::setup::Workflows>,
    /// The profile the images it is listed in are measured against, where the
    /// set claims rules one selects and that offer was taken.
    conforms: Vec<crate::conformance::Conforms>,
    /// Holds the components that the set imports. Each is copied in where the
    /// repository lacks it. None is listed.
    imported: Vec<Member>,
}

impl Component {
    /// Asks for the target image first, because its family decides which
    /// members the picker marks unavailable. Then which members, which
    /// collection where a name is in more than one, and a local name where the
    /// repository already holds one. Then the three offers, once for the set,
    /// and whether the set tracks upstream.
    pub fn collect(
        request: Request,
        root: &Path,
        sources: &[Collection],
        enforce: bool,
        prompt: &Prompt,
        say: crate::report::Say<'_>,
    ) -> Result<Self, Error> {
        let Request {
            name,
            image,
            datastream,
            dependencies,
        } = request;
        let (list, _) = crate::model::image::List::load(root);
        let listing = Listing::target(root, image, prompt)?;
        let fit = Fit::of(root, sources, &list, &listing);
        let asked = match name {
            Some(name) => vec![name],
            None => choose_several(root, sources, fit.as_ref(), prompt)?,
        };
        let picked = asked
            .iter()
            .map(|name| resolve(name, root, sources, enforce, prompt))
            .collect::<Result<Vec<Found>, Error>>()?;
        for from in &picked {
            let issues = crate::parse::component::validate_manifest(
                &from.dir.join(layout::MANIFEST_FILE),
                list.schema_version,
                &list.repo_src,
            );
            if !issues.is_empty() {
                return Err(issues.plain().into());
            }
        }
        // Before any offer is made, so a repository that cannot take the set
        // says so before it asks four questions about it.
        let chosen = picked
            .iter()
            .map(|from| member(root, from, &listing, prompt))
            .collect::<Result<Vec<Member>, String>>()?;
        let declares: Vec<crate::parse::component::Summary> =
            chosen.iter().map(|member| member.declares(root)).collect();
        if let Some(fit) = &fit {
            for (member, declares) in chosen.iter().zip(&declares) {
                if let Some(why) = fit.unsupported(declares) {
                    return Err(format!("`{}` {why}", member.name).into());
                }
            }
        }
        for member in &chosen {
            listing.refuse_duplicate(&list, &member.name)?;
        }
        let named: Vec<String> = chosen.iter().map(|member| member.name.clone()).collect();
        let mut imports = imported(root, sources, enforce, &chosen, &named)?;
        // What the set imports meets a requirement as a listed member does.
        let set_names: Vec<String> = named
            .iter()
            .cloned()
            .chain(imports.iter().map(|member| member.name.clone()))
            .collect();
        let set_declares: Vec<crate::parse::component::Summary> = chosen
            .iter()
            .chain(&imports)
            .map(|member| member.declares(root))
            .collect();

        let owed_now = short(
            root,
            sources,
            &list,
            &Listed {
                listing: &listing,
                names: &set_names,
                declares: &set_declares,
            },
            dependencies,
            prompt,
            &mut *say,
        )?;
        let also = owed_now
            .bring
            .into_iter()
            .filter(|qualified| {
                !picked
                    .iter()
                    .any(|from| format!("{}/{}", from.owner, from.name) == *qualified)
            })
            .map(|qualified| brought(root, sources, enforce, &qualified))
            .collect::<Result<Vec<Member>, String>>()?;
        let held: Vec<String> = set_names
            .iter()
            .cloned()
            .chain(also.iter().map(|member| member.name.clone()))
            .collect();
        imports.extend(imported(root, sources, enforce, &also, &held)?);
        let mut unlisted: Vec<Unlisted> = Vec::new();
        if let Some(fit) = &fit {
            for member in also.iter().chain(&chosen).chain(&imports) {
                let Some((capability, held)) = fit.clash(&member.declares(root)) else {
                    continue;
                };
                // Two members may clash with one entry, which is asked about
                // and removed once.
                let held: Vec<&crate::model::image::Entry> = held
                    .into_iter()
                    .filter(|entry| !unlisted.iter().any(|gone| gone.name == entry.path))
                    .collect();
                if !held.is_empty() {
                    unlisted.extend(fit.settle(&capability, &held, prompt)?);
                }
            }
        }

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
                say(&format!("{}\n\n", rows.join("\n")));
                prompt
                    .confirm(copy::GENERATE_WORKFLOWS, copy::YES, copy::NO)?
                    .then(|| crate::setup::Workflows::adding(&list, &named))
            }
        };

        // What the set requires goes in first, so the list reads in build order.
        let members: Vec<Member> = also.into_iter().chain(chosen).collect();
        let brought: Vec<String> = members.iter().map(|member| member.name.clone()).collect();
        let conforms = measured(
            &list, &listing, &named, &declares, datastream, &brought, prompt, say,
        )?;
        let copied: Vec<String> = members
            .iter()
            .chain(&imports)
            .filter(|member| member.vendor)
            .map(|member| format!("`{}`", member.name))
            .collect();
        // Without a terminal the copies track their library, which is the
        // answer a terminal opens on.
        let tracked = match copied.is_empty() {
            true => true,
            false => {
                prompt.confirm_current(&copy::track(&said(&copied)), copy::YES, copy::NO, true)?
            }
        };
        Ok(Self {
            members,
            listing,
            tracked,
            unlisted,
            workflows,
            conforms,
            imported: imports,
        })
    }

    /// Writes the set and draws the tree of what changed.
    pub fn apply(&self, root: &Path, sources: &[Collection]) -> Result<(), String> {
        let wrote = self.write(root, sources)?;
        if !wrote.is_empty() {
            report(root, &wrote);
        }
        Ok(())
    }

    /// Everything `apply` does but say so, for a caller drawing a tree of its
    /// own around it. The sources are the repository's declaration, which the
    /// caller has already read.
    pub(crate) fn write(
        &self,
        root: &Path,
        sources: &[Collection],
    ) -> Result<Vec<(PathBuf, Change)>, String> {
        let copies: Vec<(&Found, &str)> = self
            .members
            .iter()
            .chain(&self.imported)
            .filter(|member| member.vendor)
            .map(|member| (&member.from, member.name.as_str()))
            .collect();
        // Every removal is worked out before anything is copied, so a refused
        // one leaves the repository as it was.
        let mut edits: Vec<(PathBuf, String)> = Vec::new();
        for gone in &self.unlisted {
            let file = root.join(&gone.file);
            let at = match edits.iter().position(|(held, _)| *held == file) {
                Some(at) => at,
                None => {
                    let text = std::fs::read_to_string(&file)
                        .map_err(|err| format!("{}: {err}", file.display()))?;
                    edits.push((file, text));
                    edits.len() - 1
                }
            };
            let text = crate::declaration::unlisting(
                &edits[at].0,
                &edits[at].1,
                &gone.image,
                gone.flavour.as_deref(),
                &gone.name,
            )?;
            edits[at].1 = text;
        }
        let mut wrote = vendor_all(root, sources, &copies, self.tracked)?;
        let listed: Vec<(&str, &str)> = self
            .members
            .iter()
            .map(|member| (member.name.as_str(), ""))
            .collect();
        for (file, text) in &edits {
            std::fs::write(file, text).map_err(|err| format!("{}: {err}", file.display()))?;
        }
        let (list, _) = crate::model::image::List::load(root);
        wrote.extend(self.listing.apply_declaration(&list, &listed)?);
        for gone in &self.unlisted {
            let removed = format!("{} removed", gone.name);
            match wrote.iter_mut().find(|(path, _)| *path == gone.file) {
                Some((_, Change::Updated(said))) => {
                    said.push_str(", and ");
                    said.push_str(&removed);
                }
                _ => wrote.push((gone.file.clone(), Change::Updated(removed))),
            }
        }
        if let Some(workflows) = &self.workflows {
            wrote.extend(workflows.apply(root)?);
        }
        // Last, since it splices into the image files this just wrote the
        // component lines into, and the tree keeps the line that says both.
        for conforms in &self.conforms {
            wrote.extend(conforms.apply(root)?);
        }
        collisions(root, &self.brought());
        Ok(wrote)
    }

    /// The local names the set brings, as the tree a caller of `write` draws
    /// says what it added.
    pub(crate) fn brought(&self) -> Vec<String> {
        self.members
            .iter()
            .chain(&self.imported)
            .map(|member| member.name.clone())
            .collect()
    }
}

/// The same import, made out of an offer somewhere else: named members listed
/// in one image, with none of `import component`'s own questions on top.
pub fn bring(
    root: &Path,
    sources: &[Collection],
    enforce: bool,
    named: &[String],
    image: &str,
) -> Result<Component, String> {
    let members = named
        .iter()
        .map(|qualified| brought(root, sources, enforce, qualified))
        .collect::<Result<Vec<Member>, String>>()?;
    let held: Vec<String> = members.iter().map(|member| member.name.clone()).collect();
    let imported = imported(root, sources, enforce, &members, &held)?;
    Ok(Component {
        members,
        imported,
        listing: Listing::collect(root, vec![image.to_string()], &Prompt::silent())?,
        tracked: true,
        unlisted: Vec::new(),
        workflows: None,
        conforms: Vec::new(),
    })
}

/// Settles a provider that an offer brings. A component the repository already
/// holds under the same name is listed as it is, because the offer asks no
/// question about names.
fn brought(
    root: &Path,
    sources: &[Collection],
    enforce: bool,
    qualified: &str,
) -> Result<Member, String> {
    // The offer names a provider the repository holds by its bare path.
    let local = layout::component(root, qualified);
    if layout::is_component(&local) {
        return Ok(Member {
            from: Found {
                owner: String::new(),
                name: qualified.to_string(),
                dir: local,
            },
            name: qualified.to_string(),
            vendor: false,
        });
    }
    let mut found = find(root, sources, qualified, enforce)?;
    let from = found.swap_remove(0);
    let name = from.name.clone();
    Ok(Member {
        vendor: !layout::is_component(&layout::component(root, &name)),
        from,
        name,
    })
}

/// Settles the local name of one chosen member. If the repository already
/// holds a component at the member's path, then the user lists the local one or
/// names the copy. A path the user names is refused while something is there.
fn member(root: &Path, from: &Found, listing: &Listing, prompt: &Prompt) -> Result<Member, String> {
    let name = from.name.clone();
    if !layout::is_component(&layout::component(root, &name)) {
        return Ok(Member {
            from: from.clone(),
            name,
            vendor: true,
        });
    }
    let image = listing.images().first().map(ToString::to_string);
    let mut options = Vec::new();
    if let Some(image) = &image {
        options.push(Choice::new(copy::add_local(&name, image), ""));
    }
    options.push(Choice::new(copy::ANOTHER_NAME, ""));
    let local = lock::hash(&layout::component(root, &name));
    let same = local.is_some() && local == lock::hash(&from.dir);
    let answer = match prompt.asks() {
        true => prompt.choose(&copy::present(&name), &options)?,
        // Without a terminal, the local copy is listed only if it holds what the
        // library holds. If the local copy is a fork, then the import is refused.
        // Only the user can choose which copy the image lists.
        false => (same && image.is_some()).then_some(0),
    };
    match (answer, &image) {
        (Some(0), Some(_)) => Ok(Member {
            from: from.clone(),
            name,
            vendor: false,
        }),
        (Some(_), _) => {
            let suggested = free_name(root, &name);
            let chosen =
                prompt.line(None, copy::LOCAL_NAME, "a local name", "", Some(&suggested))?;
            if !crate::model::image::is_local_path(&chosen) {
                return Err(format!(
                    "`{chosen}` is not a component path: each part is lowercase letters, digits and \
                     dashes, starting with a letter, and the last part may end in one `@<tag>`"
                ));
            }
            if layout::component(root, &chosen).exists() {
                return Err(format!("`{chosen}` is already in this repository"));
            }
            Ok(Member {
                from: from.clone(),
                name: chosen,
                vendor: true,
            })
        }
        (None, _) if !same => Err(format!(
            "`{name}` is already in this repository and differs from the library's; run `tect \
             import component` in a terminal to list the local `{name}` or import the library's \
             under another name"
        )),
        (None, _) => Err(format!(
            "`{name}` is already in this repository; run `tect import component` in a terminal to \
             list the local `{name}` or import the library's under another name"
        )),
    }
}

/// Finds the first `<name>@<n>` the repository does not hold yet. `@` reads as a
/// local alias of the library's component, never as another upstream name.
fn free_name(root: &Path, name: &str) -> String {
    (1..)
        .map(|at| format!("{name}@{at}"))
        .find(|candidate| !layout::component(root, candidate).exists())
        .expect("an unbounded range holds a free name")
}

/// Holds what the provider offer brings. Each provider is listed in the image.
/// A provider from a library is copied in, and a provider the repository holds
/// is listed as it is.
#[derive(Default)]
struct Owed {
    bring: Vec<String>,
}

/// What the set requires that the images it is being listed in do not have, as
/// the collection members that would satisfy it. One question for the set;
/// declining it leaves the requirement for `check` to diagnose. A run with no
/// terminal declines the offer unless `--dependencies` answers it. Where
/// several members would satisfy one requirement, the user chooses one.
/// `--dependencies` cannot choose, so it is refused naming each.
fn short(
    root: &Path,
    sources: &[Collection],
    list: &crate::model::image::List,
    listed: &Listed,
    dependencies: bool,
    prompt: &Prompt,
    say: crate::report::Say<'_>,
) -> Result<Owed, String> {
    let &Listed {
        listing,
        names,
        declares,
    } = listed;
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
    let requires = outside(names, declares);
    if targets.is_empty() || requires.is_empty() || !(dependencies || prompt.asks()) {
        return Ok(Owed::default());
    }
    let disk = crate::parse::disk::Disk::scan(root);
    let index = crate::provider::Index::scan(root, sources, &disk, false);

    let mut mappings: Vec<(String, String, String, String)> = Vec::new();
    let mut owed_now = Owed::default();
    for (component, want) in requires {
        // Per target, because the adapter filling a role is a different component
        // on every family: two images on two families owe two providers, and
        // one image owes the one that targets it.
        for (image, flavour) in &targets {
            let candidates = owed(image, *flavour, want, &index);
            let provider = match candidates.as_slice() {
                [] => continue,
                [one] => *one,
                many => match pick(component, want, many, dependencies, prompt)? {
                    Some(picked) => picked,
                    None => continue,
                },
            };
            let base = image
                .base
                .as_ref()
                .map(|base| base.image.rsplit('/').next().unwrap_or(&base.image))
                .unwrap_or_default();
            let mapping = (
                component.to_string(),
                base.to_string(),
                want.to_string(),
                provider.name.clone(),
            );
            if !mappings.contains(&mapping) {
                mappings.push(mapping);
            }
            let qualified = provider.qualified();
            if !owed_now.bring.contains(&qualified) {
                owed_now.bring.push(qualified);
            }
        }
    }
    if owed_now.bring.is_empty() || dependencies {
        return Ok(owed_now);
    }

    say(&format!("{}\n", copy::REQUIRED));
    for (component, base, want, provider) in mappings {
        say(&format!(
            "\n  {component} > {base}\n    requires: {want}\n    provider component: {provider}\n"
        ));
    }
    say("\n");

    match prompt.confirm(copy::BRING_REQUIRED, copy::YES, copy::NO)? {
        true => Ok(owed_now),
        false => Ok(Owed::default()),
    }
}

/// Asks which of several library members meets one requirement.
/// `--dependencies` answers the offer without the user, so it is refused
/// naming each member.
fn pick<'a>(
    component: &str,
    want: &str,
    many: &[&'a crate::provider::Provider],
    dependencies: bool,
    prompt: &Prompt,
) -> Result<Option<&'a crate::provider::Provider>, String> {
    if dependencies {
        let named: Vec<String> = many
            .iter()
            .map(|held| format!("`{}`", held.qualified()))
            .collect();
        return Err(format!(
            "`{component}` requires `{want}`, and {} each provide it; run `tect import component` \
             in a terminal to choose one",
            said(&named)
        ));
    }
    let options: Vec<Choice> = many
        .iter()
        .map(|held| Choice::new(held.qualified(), held.about()))
        .collect();
    Ok(prompt
        .choose(&copy::which_provider(component, want), &options)?
        .map(|at| many[at]))
}

/// Pairs each member of the imported set with each requirement it declares
/// that no member of the set meets. A member meets a requirement for its own
/// path or for a capability it provides.
fn outside<'a>(
    names: &'a [String],
    declares: &'a [crate::parse::component::Summary],
) -> Vec<(&'a String, &'a String)> {
    names
        .iter()
        .zip(declares)
        .flat_map(|(name, held)| held.requires.iter().map(move |want| (name, want)))
        .filter(|(_, want)| {
            !names.iter().zip(declares).any(|(name, held)| {
                crate::resolve::answers(name, held.provides.iter().map(String::as_str), want)
            })
        })
        .collect()
}

/// Answers which components one target of an image could take for `want`. A
/// target that already has a provider owes none. If the repository holds a
/// fitting provider, then the offer lists it in place of a library copy.
fn owed<'a>(
    image: &crate::model::image::Image,
    flavour: Option<&str>,
    want: &str,
    index: &'a crate::provider::Index,
) -> Vec<&'a crate::provider::Provider> {
    if image_has(image, flavour, want, index) {
        return Vec::new();
    }
    let family = image.base.as_ref().map_or("", |base| base.family.as_str());
    let fits = index.fitting(want, family);
    let here: Vec<&crate::provider::Provider> =
        fits.iter().copied().filter(|held| held.here).collect();
    match here.is_empty() {
        true => fits
            .into_iter()
            .filter(|held| held.owner.is_some())
            .collect(),
        false => here,
    }
}

/// Whether an image already has a provider for `want`. If the base declares
/// it, or an entry the image lists or a component that the base or an entry
/// imports is at that path or provides it, then the image has one.
fn image_has(
    image: &crate::model::image::Image,
    flavour: Option<&str>,
    want: &str,
    index: &crate::provider::Index,
) -> bool {
    // The base's own imports come from the loaded image, so an inline base and
    // an option import read what the image resolved.
    let mut paths: Vec<String> = image
        .base
        .iter()
        .flat_map(|base| {
            std::iter::once(base.path.clone())
                .chain(base.imports.iter().map(|import| import.path.clone()))
        })
        .chain(
            image
                .entries
                .iter()
                .filter(|entry| entry.flavour.is_none() || entry.flavour.as_deref() == flavour)
                .map(|entry| entry.path.clone()),
        )
        .collect();
    let mut at = 0;
    while at < paths.len() {
        let imports = index
            .at(&paths[at])
            .map(|held| held.declares.imports.clone())
            .unwrap_or_default();
        for path in imports {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        at += 1;
    }
    image.base_answers(want)
        || paths.iter().any(|path| {
            crate::resolve::answers(
                path,
                index
                    .at(path)
                    .into_iter()
                    .flat_map(|held| held.declares.provides.iter().map(String::as_str)),
                want,
            )
        })
}

/// Answers the library members that one component imports and the repository
/// lacks. Imports are followed transitively. `held` names the paths that the
/// calling command already copies in.
pub(crate) fn imports_of(
    root: &Path,
    sources: &[Collection],
    enforce: bool,
    from: Found,
    held: &[String],
) -> Result<Vec<Found>, String> {
    let name = from.name.clone();
    let vendor = !layout::is_component(&layout::component(root, &name));
    let member = Member { from, name, vendor };
    Ok(imported(root, sources, enforce, &[member], held)?
        .into_iter()
        .filter(|member| member.vendor)
        .map(|member| member.from)
        .collect())
}

/// Settles every component that the members import. Imports are followed
/// transitively. A path in `held` or already settled is skipped. A component
/// the repository holds is read from its own manifest. Otherwise it comes from
/// the importing member's library. If that member has no library, then every
/// library is searched.
fn imported(
    root: &Path,
    sources: &[Collection],
    enforce: bool,
    members: &[Member],
    held: &[String],
) -> Result<Vec<Member>, String> {
    let mut out: Vec<Member> = Vec::new();
    let mut todo: Vec<(String, String, Vec<String>)> = members
        .iter()
        .map(|member| {
            let imports = member.declares(root).imports;
            (member.name.clone(), member.from.owner.clone(), imports)
        })
        .collect();
    while let Some((by, owner, paths)) = todo.pop() {
        for path in paths {
            if path.is_empty()
                || held.contains(&path)
                || out.iter().any(|member| member.name == path)
            {
                continue;
            }
            // A library's manifest is not trusted to stay under components/.
            // A path with `..` would read and write outside it.
            if !crate::model::image::is_local_path(&path) {
                return Err(format!(
                    "`{by}` imports `{path}`, which is not a component path"
                ));
            }
            let local = layout::component(root, &path);
            let member = match layout::is_component(&local) {
                true => Member {
                    from: Found {
                        owner: owner.clone(),
                        name: path.clone(),
                        dir: local,
                    },
                    name: path.clone(),
                    vendor: false,
                },
                false => {
                    // The importer's own library answers first. Nothing is
                    // fetched, so the copy is what the cache already holds.
                    let owners = sources
                        .iter()
                        .filter(|source| source.kind == SourceKind::Components)
                        .map(|source| source.name.as_str())
                        .filter(|name| owner.is_empty() || *name == owner);
                    let mut from = None;
                    for library in owners {
                        from = cached_member(root, sources, enforce, library, &path)?;
                        if from.is_some() {
                            break;
                        }
                    }
                    let from = from.ok_or_else(|| {
                        format!("`{by}` imports `{path}`, which no library on this machine holds")
                    })?;
                    Member {
                        from,
                        name: path.clone(),
                        vendor: true,
                    }
                }
            };
            todo.push((
                member.name.clone(),
                member.from.owner.clone(),
                member.declares(root).imports,
            ));
            out.push(member);
        }
    }
    Ok(out)
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
/// measured against, as the question whether to declare one. The content is
/// probed the way `set conforms` probes it, since a profile has to be one the
/// scan carries. No content on this machine is no offer.
#[expect(
    clippy::too_many_arguments,
    reason = "each argument is one value the offer reads from the imported set"
)]
fn measured(
    list: &crate::model::image::List,
    listing: &Listing,
    named: &[String],
    declares: &[crate::parse::component::Summary],
    datastream: Option<&Path>,
    brought: &[String],
    prompt: &Prompt,
    say: crate::report::Say<'_>,
) -> Result<Vec<crate::conformance::Conforms>, String> {
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
    let (scap_content, family) = crate::scap::named_by(first.base.as_ref());
    let Ok(path) = crate::scap::content_path(scap_content, family, datastream) else {
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
    say(&format!(
        "{} {} rules {} {}, and {measured} {} no `conforms`.\n{}\n\n",
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
        crate::conformance::cost(&measured, list.audit_enforce),
    ));
    let options: Vec<Choice> = profiles
        .iter()
        .map(|profile| Choice::new(profile.name(), &profile.title))
        .collect();
    let Some(chosen) = prompt.choose(copy::WHICH_PROFILE, &options)? else {
        return Ok(Vec::new());
    };
    Ok(images
        .iter()
        .map(|image| {
            crate::conformance::Conforms::declaring(
                image,
                profiles[chosen].name(),
                brought.to_vec(),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::remote::At;

    fn collection(name: &str, path: &str) -> Collection {
        Collection {
            kind: SourceKind::Components,
            name: name.to_string(),
            at: At::Dir(path.to_string()),
            span: crate::diag::Span::default(),
        }
    }

    #[test]
    fn a_complete_nested_path_wins_before_an_owner_prefix() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sources = [collection("grouped", "tests/collections/grouped")];
        let found = find(root, &sources, "hardening/coredumps", false).unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].owner, "grouped");
        assert_eq!(found[0].name, "hardening/coredumps");
    }

    #[test]
    fn exact_matches_are_sorted_like_suffix_matches() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sources = [
            collection("community", "tests/collections/community"),
            collection("upstream", "tests/collections/upstream"),
        ];
        let found = find(root, &sources, "browser", false).unwrap();
        let owners: Vec<&str> = found.iter().map(|found| found.owner.as_str()).collect();

        assert_eq!(owners, ["community", "upstream"]);
    }

    #[test]
    fn the_component_picker_groups_nested_paths_by_their_first_directory() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sources = [
            collection("community", "tests/collections/community"),
            collection("grouped", "tests/collections/grouped"),
        ];
        let (_, options) = offered(root, &sources, None).unwrap();
        let nested = options
            .iter()
            .find(|option| option.label == "grouped/hardening/coredumps")
            .unwrap();
        let root_level = options
            .iter()
            .find(|option| option.label == "community/browser")
            .unwrap();

        assert_eq!(nested.group, "hardening");
        assert!(root_level.group.is_empty());
    }

    /// Writes the fedora base component `base-image/example`, with `extra`
    /// manifest lines.
    fn base_at(root: &Path, extra: &str) {
        crate::init::put(
            &root.join("components/base-image/example/manifest.kdl"),
            &format!("schema-version 1\n\nbase \"example\" {{ family \"fedora\"; }}\n{extra}\n"),
        )
        .unwrap();
    }

    /// Writes a repository whose one fedora image lists a local component that
    /// declares `flatpak` exclusive.
    fn targeted(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tect-{name}-{}", std::process::id()));
        crate::init::put(
            &root.join(layout::REPO_FILE),
            "schema-version 1\nname \"Example\"\n",
        )
        .unwrap();
        crate::init::put(
            &root.join(layout::IMAGE_FILE),
            "schema-version 1\n\nimage {\n    name \"example\"\n    base-image {\n        example\n    }\n    flathub\n}\n",
        )
        .unwrap();
        crate::init::put(
            &root.join("components/flathub/manifest.kdl"),
            "schema-version 1\n\nprovides \"flatpak\" exclusive=#true\n",
        )
        .unwrap();
        base_at(&root, "");
        root
    }

    #[test]
    fn a_member_that_would_clash_is_unavailable_with_its_reason() {
        let root = targeted("clash-row");
        let sources = [collection(
            "upstream",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/collections/upstream"),
        )];
        let (list, _) = crate::model::image::List::load(&root);
        let listing = Listing::target(&root, Some("example".into()), &Prompt::silent()).unwrap();
        let fit = Fit::of(&root, &sources, &list, &listing).expect("the image is the target");
        let (_, options) = offered(&root, &sources, Some(&fit)).unwrap();
        let row = |label: &str| {
            options
                .iter()
                .find(|option| option.label == label)
                .unwrap_or_else(|| panic!("no row {label}"))
        };

        assert!(!row("upstream/flatpak").available);
        assert_eq!(
            row("upstream/flatpak").detail,
            "`flatpak` is exclusive to `flathub`"
        );
        assert!(!row("upstream/debian-family").available);
        assert!(row("upstream/browser").available);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Writes an image with two flavours whose `dev` lists the local
    /// `flathub`, which declares `flatpak` exclusive. `extra` adds lines to the
    /// base component's manifest.
    fn flavoured(name: &str, extra: &str) -> PathBuf {
        let root = targeted(name);
        base_at(&root, extra);
        let base = "base-image { example }";
        crate::init::put(
            &root.join(layout::IMAGE_FILE),
            &format!("schema-version 1\n\nimage {{\n    name \"example\"\n    {base}\n    flavours {{\n        dev\n        prod\n    }}\n    flavour \"dev\" {{ flathub }}\n}}\n"),
        )
        .unwrap();
        root
    }

    fn upstream_flatpak() -> crate::parse::component::Summary {
        crate::parse::component::summary(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/collections/upstream/flatpak/manifest.kdl"
        )))
    }

    #[test]
    fn a_holder_in_a_sibling_flavour_never_clashes() {
        let root = flavoured("clash-sibling", "");
        let (list, _) = crate::model::image::List::load(&root);
        let listing =
            Listing::target(&root, Some("example/prod".into()), &Prompt::silent()).unwrap();
        let fit = Fit::of(&root, &[], &list, &listing).expect("the flavour is the target");

        assert!(fit.clash(&upstream_flatpak()).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_ungated_import_clashes_with_a_holder_in_any_flavour() {
        let root = flavoured("clash-ungated", "");
        let (list, _) = crate::model::image::List::load(&root);
        let listing = Listing::target(&root, Some("example".into()), &Prompt::silent()).unwrap();
        let fit = Fit::of(&root, &[], &list, &listing).expect("the image is the target");
        let (capability, held) = fit.clash(&upstream_flatpak()).expect("a clash");

        assert_eq!(capability, "flatpak");
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].path, "flathub");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// `plain` is listed first and coexists with the import. Only the
    /// exclusive `flathub` clashes with it.
    #[test]
    fn an_ungated_import_clashes_only_with_an_exclusive_holder() {
        let root = targeted("clash-plain");
        crate::init::put(
            &root.join(layout::IMAGE_FILE),
            "schema-version 1\n\nimage {\n    name \"example\"\n    base-image {\n        example\n    }\n    flavours {\n        dev\n        prod\n    }\n    flavour \"prod\" { plain }\n    flavour \"dev\" { flathub }\n}\n",
        )
        .unwrap();
        crate::init::put(
            &root.join("components/plain/manifest.kdl"),
            "schema-version 1\n\nprovides \"flatpak\"\n",
        )
        .unwrap();
        let (list, _) = crate::model::image::List::load(&root);
        let listing = Listing::target(&root, Some("example".into()), &Prompt::silent()).unwrap();
        let fit = Fit::of(&root, &[], &list, &listing).expect("the image is the target");
        let (_, held) = fit.clash(&upstream_flatpak()).expect("a clash");

        let paths: Vec<&str> = held.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["flathub"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_member_the_base_covers_never_clashes() {
        let root = flavoured("clash-base", "provides \"flatpak\"");
        let (list, _) = crate::model::image::List::load(&root);
        let listing = Listing::target(&root, Some("example".into()), &Prompt::silent()).unwrap();
        let fit = Fit::of(&root, &[], &list, &listing).expect("the image is the target");

        assert!(fit.clash(&upstream_flatpak()).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_library_is_refused_with_the_block_that_declares_one() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let named = find(root, &[], "browser", false)
            .err()
            .expect("no library is refused");
        let picked = offered(root, &[], None)
            .err()
            .expect("no library is refused");

        assert!(named.contains("declares no `sources`"), "{named}");
        assert_eq!(picked, named);
    }

    #[test]
    fn choosing_no_component_is_refused() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sources = [collection("upstream", "tests/collections/upstream")];
        let refused = choose_several(root, &sources, None, &Prompt::silent()).unwrap_err();

        assert!(refused.starts_with("no component chosen"), "{refused}");
    }

    /// The base offer in `create image` vendors through this lookup, so it has
    /// to refuse what `find` refuses.
    #[test]
    fn an_enforced_lookup_refuses_an_unpinned_library() {
        let mut pin = crate::provenance::Evidence::new(crate::diag::Span::default());
        pin.url = Some("https://host/owner/library.git".into());
        pin.version = Some("main".into());
        // A pinned-by-directory library sorts first, so a lookup that took the
        // first library would answer it instead of the unpinned one.
        let sources = [
            collection("other", "elsewhere"),
            Collection {
                kind: SourceKind::Components,
                name: "one".into(),
                at: At::Git(pin),
                span: crate::diag::Span::default(),
            },
        ];
        let root = Path::new("/nonexistent");
        let refused = cached_member(root, &sources, true, "one", "hello")
            .err()
            .expect("enforcement refuses an unpinned library");
        assert!(refused.contains("follows a moving ref"), "{refused}");
        assert!(cached_member(root, &sources, false, "one", "hello")
            .unwrap()
            .is_none());
    }

    /// The linked directory holds a manifest, so only the containment check
    /// stops the lookup from answering it.
    #[test]
    fn a_library_member_linked_outside_the_library_is_refused() {
        let root = std::env::temp_dir().join(format!("tect-import-link-{}", std::process::id()));
        crate::init::put(&root.join("outside/manifest.kdl"), "schema-version 1\n").unwrap();
        std::fs::create_dir_all(root.join("library")).unwrap();
        std::os::unix::fs::symlink(root.join("outside"), root.join("library/evil")).unwrap();
        let sources = [collection("lib", "library")];
        let refused = cached_member(&root, &sources, false, "lib", "evil");
        std::fs::remove_dir_all(&root).unwrap();
        let refused = refused
            .err()
            .expect("a member linked outside the library is refused");
        assert!(
            refused.contains("links `evil` outside its own tree"),
            "{refused}"
        );
    }

    /// The escaped directory exists and holds a manifest, so only the path
    /// check stops the import from taking it.
    #[test]
    fn an_import_that_climbs_out_of_components_is_refused() {
        let root = std::env::temp_dir().join(format!("tect-import-escape-{}", std::process::id()));
        crate::init::put(&root.join("escape/manifest.kdl"), "schema-version 1\n").unwrap();
        crate::init::put(
            &layout::component(&root, "one").join(layout::MANIFEST_FILE),
            "schema-version 1\n\nimports \"../escape\"\n",
        )
        .unwrap();
        let member = Member {
            from: Found {
                owner: String::new(),
                name: "one".into(),
                dir: layout::component(&root, "one"),
            },
            name: "one".into(),
            vendor: false,
        };
        let refused = imported(&root, &[], false, &[member], &[]);
        std::fs::remove_dir_all(&root).unwrap();
        let refused = refused.err().expect("a path with `..` is refused");
        assert!(
            refused.contains("`../escape`, which is not a component path"),
            "{refused}"
        );
    }

    #[test]
    fn a_copy_from_a_fetched_library_records_its_url_and_revision() {
        let root = std::env::temp_dir().join(format!("tect-vendor-pin-{}", std::process::id()));
        crate::init::put(&root.join("library/hello/component.sh"), "true\n").unwrap();
        let mut pin = crate::provenance::Evidence::new(crate::diag::Span::default());
        pin.url = Some("https://host/owner/library.git".into());
        pin.version = Some("v2.0.0".into());
        let sources = [
            collection("other", "elsewhere"),
            Collection {
                kind: SourceKind::Components,
                name: "one".into(),
                at: At::Git(pin),
                span: crate::diag::Span::default(),
            },
        ];
        let from = Found {
            owner: "one".into(),
            name: "hello".into(),
            dir: root.join("library/hello"),
        };
        vendor(&root, &sources, &from, "hello", false).unwrap();
        let entry = lock::entry(&root, "hello").expect("the copy is recorded");

        assert_eq!(
            entry.pin.url.as_deref(),
            Some("https://host/owner/library.git")
        );
        assert_eq!(entry.pin.version.as_deref(), Some("v2.0.0"));
        assert!(!entry.tracked);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_target_leaves_every_member_available() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sources = [collection("upstream", "tests/collections/upstream")];
        let (_, options) = offered(root, &sources, None).unwrap();

        assert!(options.iter().all(|option| option.available));
    }

    #[test]
    fn a_provider_in_one_flavour_does_not_cover_its_sibling() {
        let root =
            std::env::temp_dir().join(format!("tect-flavour-provider-{}", std::process::id()));
        crate::init::put(
            &root.join("image.kdl"),
            r#"schema-version 1

image {
    name "Example"
    base-image {
        example
    }
    flavours {
        dev
        server
    }
    flavour "dev" { provider }
}
"#,
        )
        .unwrap();
        base_at(&root, "");
        crate::init::put(
            &root.join("components/provider/manifest.kdl"),
            "schema-version 1\n\ndescription \"Provider\"\ntargets \"fedora\"\nprovides \"tool\"\n",
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

    /// Before the repository owns a provider, the upstream member is owed.
    /// That case proves the index reads the collection.
    #[test]
    fn a_provider_the_repository_owns_is_offered_in_place_of_the_library_copy() {
        let root = std::env::temp_dir().join(format!("tect-owed-{}", std::process::id()));
        crate::init::put(
            &root.join("image.kdl"),
            "schema-version 1\n\nimage {\n    name \"Example\"\n    base-image {\n        example\n    }\n}\n",
        )
        .unwrap();
        base_at(&root, "");
        let sources = [collection(
            "upstream",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/collections/upstream"),
        )];
        let owed_now = |root: &Path| {
            let (list, _) = crate::model::image::List::load(root);
            let disk = crate::parse::disk::Disk::scan(root);
            let index = crate::provider::Index::scan(root, &sources, &disk, false);
            owed(&list.images[0], None, "flatpak", &index)
                .first()
                .map(|held| held.qualified())
        };

        assert_eq!(owed_now(&root).as_deref(), Some("upstream/flatpak"));
        crate::init::put(
            &root.join("components/apps/flatpak/manifest.kdl"),
            "schema-version 1\n\ndescription \"Flatpak\"\nprovides \"flatpak\"\n",
        )
        .unwrap();
        assert_eq!(owed_now(&root).as_deref(), Some("apps/flatpak"));
        let _ = std::fs::remove_dir_all(root);
    }

    /// The base imports `flatpak`, so the image already has its provider.
    #[test]
    fn a_provider_the_base_imports_leaves_nothing_owed() {
        let root = std::env::temp_dir().join(format!("tect-owed-base-{}", std::process::id()));
        crate::init::put(
            &root.join("image.kdl"),
            "schema-version 1\n\nimage {\n    name \"Example\"\n    base-image {\n        example\n    }\n}\n",
        )
        .unwrap();
        base_at(&root, "imports \"apps/flatpak\"\n");
        crate::init::put(
            &root.join("components/apps/flatpak/manifest.kdl"),
            "schema-version 1\n\nprovides \"flatpak\"\n",
        )
        .unwrap();
        let (list, _) = crate::model::image::List::load(&root);
        let disk = crate::parse::disk::Disk::scan(&root);
        let index = crate::provider::Index::scan(&root, &[], &disk, false);
        let owed_now = owed(&list.images[0], None, "flatpak", &index).len();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(owed_now, 0);
    }

    /// The base is defined inline, so no manifest on disk names its imports.
    #[test]
    fn a_provider_an_inline_base_imports_leaves_nothing_owed() {
        let root = std::env::temp_dir().join(format!("tect-owed-inline-{}", std::process::id()));
        crate::init::put(
            &root.join("image.kdl"),
            "schema-version 1\n\nimage {\n    name \"Example\"\n    base-image {\n        base \"example\" { family \"fedora\"; }\n        imports \"apps/flatpak\"\n    }\n}\n",
        )
        .unwrap();
        crate::init::put(
            &root.join("components/apps/flatpak/manifest.kdl"),
            "schema-version 1\n\nprovides \"flatpak\"\n",
        )
        .unwrap();
        let (list, _) = crate::model::image::List::load(&root);
        let disk = crate::parse::disk::Disk::scan(&root);
        let index = crate::provider::Index::scan(&root, &[], &disk, false);
        let owed_now = owed(&list.images[0], None, "flatpak", &index).len();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(owed_now, 0);
    }

    /// The summary fills an option import with the option's default, because
    /// import copies in the component before any image entry sets a value.
    #[test]
    fn a_summary_fills_an_option_import_with_its_default() {
        let root = std::env::temp_dir().join(format!("tect-summary-import-{}", std::process::id()));
        let file = root.join("manifest.kdl");
        crate::init::put(
            &file,
            "schema-version 1\n\nimports \"$runtime\" \"fixed\"\noption \"other\" type=\"string\" {\n    default \"docker\"\n}\noption \"runtime\" type=\"string\" {\n    default \"podman\"\n}\n",
        )
        .unwrap();
        let imports = crate::parse::component::summary(&file).imports;
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(imports, ["podman", "fixed"]);
    }

    #[test]
    fn a_member_of_the_set_meets_what_another_member_requires() {
        let set = |members: &[(&str, &[&str], &[&str])]| {
            let names: Vec<String> = members.iter().map(|(name, ..)| name.to_string()).collect();
            let declares: Vec<crate::parse::component::Summary> = members
                .iter()
                .map(|(_, requires, provides)| crate::parse::component::Summary {
                    requires: requires.iter().map(|want| want.to_string()).collect(),
                    provides: provides.iter().map(|has| has.to_string()).collect(),
                    ..Default::default()
                })
                .collect();
            outside(&names, &declares)
                .into_iter()
                .map(|(name, want)| format!("{name} {want}"))
                .collect::<Vec<_>>()
        };

        assert!(set(&[("consumer", &["helper"], &[]), ("helper", &[], &[])]).is_empty());
        assert!(set(&[("consumer", &["rpm"], &[]), ("manager", &[], &["rpm/dnf5"])]).is_empty());
        assert_eq!(
            set(&[("consumer", &["rpm"], &[]), ("rpm/dnf5", &[], &[])]),
            ["consumer rpm"]
        );
    }

    #[test]
    fn an_unhashable_copy_writes_nothing() {
        let root =
            std::env::temp_dir().join(format!("tect-unhashable-copy-{}", std::process::id()));
        let from = Found {
            owner: "one".into(),
            name: "missing".into(),
            dir: root.join("missing"),
        };

        assert!(vendor(&root, &[], &from, "missing", true).is_err());
        assert!(!root.join("components/missing").exists());
        assert!(!layout::lock(&root).exists());
    }

    /// The parent directory exists only because the held component was imported
    /// first.
    #[test]
    fn a_parent_imported_after_its_held_component_is_copied() {
        let root = std::env::temp_dir().join(format!("tect-held-parent-{}", std::process::id()));
        crate::init::put(&root.join("library/nvidia/manifest.kdl"), "").unwrap();
        crate::init::put(&root.join("library/nvidia/cuda/manifest.kdl"), "").unwrap();
        crate::init::put(&root.join("components/nvidia/cuda/manifest.kdl"), "").unwrap();
        let sources = [collection("one", "library")];
        let found = find(&root, &sources, "nvidia", false)
            .unwrap()
            .swap_remove(0);
        let listing = Listing::collect(&root, Vec::new(), &Prompt::silent()).unwrap();

        let chosen = member(&root, &found, &listing, &Prompt::silent());
        let offered = brought(&root, &sources, false, "one/nvidia");
        std::fs::remove_dir_all(root).unwrap();
        assert!(chosen.unwrap().vendor);
        assert!(offered.unwrap().vendor);
    }

    #[test]
    fn a_renamed_copy_takes_the_first_free_tag() {
        let root = std::env::temp_dir().join(format!("tect-free-name-{}", std::process::id()));
        crate::init::put(&root.join("components/browser/component.sh"), "true\n").unwrap();
        crate::init::put(&root.join("components/browser@1/component.sh"), "true\n").unwrap();

        assert_eq!(free_name(&root, "browser"), "browser@2");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_import_refuses_a_manifest_newer_than_the_repository() {
        let root = std::env::temp_dir().join(format!("tect-import-schema-{}", std::process::id()));
        crate::init::put(
            &root.join(layout::REPO_FILE),
            "schema-version 1\nname \"Example\"\n",
        )
        .unwrap();
        crate::init::put(
            &root.join(layout::IMAGE_FILE),
            "schema-version 1\n\nimage { name \"Example\"; example;  }\n",
        )
        .unwrap();
        crate::init::put(
            &root.join("collection/new/manifest.kdl"),
            "schema-version 2\n\npackages \"new\"\n",
        )
        .unwrap();
        let sources = [collection("one", "collection")];

        let result = Component::collect(
            Request {
                name: Some("new".into()),
                image: Some("example".into()),
                datastream: None,
                dependencies: false,
            },
            &root,
            &sources,
            false,
            &Prompt::silent(),
            &mut |_| {},
        );
        let message = match result {
            Err(err) => err.message().to_string(),
            Ok(_) => panic!("an incompatible import was accepted"),
        };
        assert!(message.contains("manifest.kdl declares"), "{message}");
        assert!(message.contains("version 2, but"), "{message}");
        assert!(message.contains("repo.kdl declares 1"), "{message}");
        assert!(!root.join("components/new").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Writes a repository on the fedora base and a library `lib` that holds
    /// `consumer`, which requires `runtime`, and two providers of it.
    /// `consumer` is the manifest `requires` written for it.
    fn two_providers(name: &str, consumer: &str) -> (PathBuf, [Collection; 1]) {
        let root = std::env::temp_dir().join(format!("tect-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in [
            ("repo.kdl", "schema-version 1\nname \"Example\"\n".to_string()),
            (
                "image.kdl",
                "schema-version 1\n\nimage {\n    name \"example\"\n    base-image {\n        example\n    }\n}\n".to_string(),
            ),
            ("library/consumer/manifest.kdl", format!("schema-version 1\n\n{consumer}\n")),
            ("library/docker/manifest.kdl", "schema-version 1\n\nprovides \"runtime\"\n".to_string()),
            ("library/podman/manifest.kdl", "schema-version 1\n\nprovides \"runtime\"\n".to_string()),
        ] {
            crate::init::put(&root.join(path), &text).unwrap();
        }
        base_at(&root, "");
        (root, [collection("lib", "library")])
    }

    fn consumer_request(dependencies: bool) -> Request<'static> {
        Request {
            name: Some("lib/consumer".into()),
            image: Some("example".into()),
            datastream: None,
            dependencies,
        }
    }

    #[test]
    fn two_library_providers_under_dependencies_are_refused_naming_both() {
        let (root, sources) = two_providers("closure-refused", "requires \"runtime\"");
        let result = Component::collect(
            consumer_request(true),
            &root,
            &sources,
            false,
            &Prompt::silent(),
            &mut |_| {},
        );
        let _ = std::fs::remove_dir_all(&root);
        let message = match result {
            Err(err) => err.message().to_string(),
            Ok(_) => panic!("two candidates were settled without asking"),
        };
        assert!(
            message.contains(
                "`consumer` requires `runtime`, and `lib/docker` and `lib/podman` each provide it"
            ),
            "{message}"
        );
    }

    /// `--dependencies` answers the offer for the user, so it refuses to choose
    /// even where a terminal could ask.
    #[test]
    fn dependencies_refuse_two_providers_even_with_a_terminal() {
        let (root, sources) = two_providers("closure-refused-tty", "requires \"runtime\"");
        let answers = ["2", "y", "y"].map(str::to_string).to_vec();
        let result = Component::collect(
            consumer_request(true),
            &root,
            &sources,
            false,
            &Prompt::scripted(answers),
            &mut |_| {},
        );
        let _ = std::fs::remove_dir_all(&root);
        assert!(result.is_err(), "two candidates were settled by a flag");
    }

    #[test]
    fn the_user_chooses_which_library_provider_is_brought() {
        let (root, sources) = two_providers("closure-chosen", "requires \"runtime\"");
        let answers = ["2", "y", "y"].map(str::to_string).to_vec();
        Component::collect(
            consumer_request(false),
            &root,
            &sources,
            false,
            &Prompt::scripted(answers),
            &mut |_| {},
        )
        .unwrap_or_else(|err| panic!("{}", err.message()))
        .write(&root, &sources)
        .unwrap();
        let image = std::fs::read_to_string(root.join("image.kdl")).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            crate::parse::image::listings(&image, "podman") == [None],
            "{image}"
        );
        assert!(
            crate::parse::image::listings(&image, "docker").is_empty(),
            "{image}"
        );
    }

    /// The import copies in what the chosen component imports and lists only
    /// the chosen component.
    #[test]
    fn an_import_copies_what_the_component_imports_and_lists_it_alone() {
        let (root, sources) = two_providers("imports-copied", "imports \"podman\"");
        Component::collect(
            consumer_request(false),
            &root,
            &sources,
            false,
            &Prompt::scripted(vec!["y".to_string()]),
            &mut |_| {},
        )
        .unwrap_or_else(|err| panic!("{}", err.message()))
        .write(&root, &sources)
        .unwrap();
        let image = std::fs::read_to_string(root.join("image.kdl")).unwrap();
        let held = root.join("components/podman/manifest.kdl").is_file();
        let lock = std::fs::read_to_string(layout::lock(&root)).unwrap();
        let issues = crate::run(crate::Command::Check, None, &root)
            .problems
            .issues
            .plain();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            crate::parse::image::listings(&image, "consumer") == [None]
                && crate::parse::image::listings(&image, "podman").is_empty(),
            "{image}"
        );
        assert!(held);
        assert!(lock.contains("component \"podman\""), "{lock}");
        assert!(issues.is_empty(), "{issues}");
    }

    /// The first removal would succeed and the second is refused, because
    /// `hardware/nvidia` holds `cuda`. Nothing may be written before the
    /// refusal, or the image would lose `flathub` with the import undone.
    #[test]
    fn a_refused_unlist_leaves_the_image_file_as_it_was() {
        let root = targeted("refused-unlist");
        base_at(&root, "");
        let file = root.join(layout::IMAGE_FILE);
        let image = "schema-version 1\n\nimage {\n    name \"example\"\n    base-image {\n        example\n    }\n    flathub\n    hardware {\n        nvidia {\n            cuda\n        }\n    }\n}\n";
        crate::init::put(&file, image).unwrap();
        let gone = |name: &str| Unlisted {
            file: PathBuf::from(layout::IMAGE_FILE),
            image: "example".to_string(),
            flavour: None,
            name: name.to_string(),
        };
        let import = Component {
            members: Vec::new(),
            listing: Listing::collect(&root, vec!["example".to_string()], &Prompt::silent())
                .unwrap(),
            tracked: true,
            unlisted: vec![gone("flathub"), gone("hardware/nvidia")],
            workflows: None,
            conforms: Vec::new(),
            imported: Vec::new(),
        };
        let refused = import.write(&root, &[]).map(|_| ()).unwrap_err();
        let after = std::fs::read_to_string(&file).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(refused.contains("with cuda below it"), "{refused}");
        assert_eq!(after, image);
    }

    /// Both removals edit one file, so the second must start from the first.
    #[test]
    fn two_unlists_in_one_file_both_land() {
        let root = targeted("two-unlists");
        base_at(&root, "");
        let file = root.join(layout::IMAGE_FILE);
        crate::init::put(
            &file,
            "schema-version 1\n\nimage {\n    name \"example\"\n    base-image {\n        example\n    }\n    flathub\n    other\n    kept\n}\n",
        )
        .unwrap();
        let gone = |name: &str| Unlisted {
            file: PathBuf::from(layout::IMAGE_FILE),
            image: "example".to_string(),
            flavour: None,
            name: name.to_string(),
        };
        let import = Component {
            members: Vec::new(),
            listing: Listing::collect(&root, vec!["example".to_string()], &Prompt::silent())
                .unwrap(),
            tracked: true,
            unlisted: vec![gone("flathub"), gone("other")],
            workflows: None,
            conforms: Vec::new(),
            imported: Vec::new(),
        };
        import.write(&root, &[]).unwrap();
        let after = std::fs::read_to_string(&file).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            after,
            "schema-version 1\n\nimage {\n    name \"example\"\n    base-image {\n        example\n    }\n    kept\n}\n"
        );
    }
}

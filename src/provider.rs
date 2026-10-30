//! Given a capability, which component provides it: over the repository, over
//! what it has already imported, and over the collections it declares.
//!
//! One index: five callers ask the same question.

use crate::model::remote::{Collection, Kind as SourceKind};
use crate::parse::component::Summary;
use crate::parse::disk::Disk;
use crate::{layout, parse};
use std::collections::BTreeSet;
use std::path::Path;

/// One component that could satisfy a requirement, and what it takes to get it.
pub struct Provider {
    /// The collection it comes from, absent for a component the repository owns.
    pub owner: Option<String>,
    /// Its directory under `components/`, or its path inside the collection.
    pub name: String,
    /// Already on this machine, so an image can list it without an import.
    pub here: bool,
    pub declares: Summary,
}

impl Provider {
    /// How a person names it: `<owner>/<name>` for a collection member,
    /// whatever else has the same name.
    pub fn qualified(&self) -> String {
        match &self.owner {
            Some(owner) => format!("{owner}/{}", self.name),
            None => self.name.clone(),
        }
    }

    /// What a picker shows beside the name.
    pub fn about(&self) -> String {
        let requires = format!("(requires {})", self.declares.requires.join(", "));
        match (
            self.declares.description.is_empty(),
            self.declares.requires.is_empty(),
        ) {
            (_, true) => self.declares.description.clone(),
            (true, false) => requires,
            (false, false) => format!("{} {requires}", self.declares.description),
        }
    }
}

/// Every component a repository could reach, the ones already on it first —
/// and the declared collections this scan never looked in, so that nothing
/// reading the index states as fact what it did not check.
pub struct Index {
    held: Vec<Provider>,
    /// Declared component collections not on this machine, which a scan that does
    /// not fetch skips: empty when every one was read, and empty when none were
    /// declared, which `sourced` tells apart.
    unread: Vec<String>,
    /// Whether the repository declares a component collection at all.
    sourced: bool,
    /// Why a fetching scan fell back to what was already here. `None` where it
    /// did not fetch, or where the fetch worked.
    unreached: Option<String>,
}

impl Index {
    /// The repository's own components and nothing else, for resolution.
    pub fn local(root: &Path, sources: &[Collection], disk: &Disk) -> Self {
        Self {
            held: own(root, disk),
            unread: Vec::new(),
            sourced: sources
                .iter()
                .any(|collection| collection.kind == SourceKind::Components),
            unreached: None,
        }
    }

    /// `fetch` decides whether a collection that is not on this machine is
    /// downloaded to answer, which resolution never does and a flow may.
    pub fn scan(root: &Path, sources: &[Collection], disk: &Disk, fetch: bool) -> Self {
        let mut out = own(root, disk);
        // One collection that cannot be reached takes the whole catalog with
        // it, so a scan that meant to fetch falls back to what is already here.
        // `unread` is what still says the search was incomplete.
        let fetched = catalog(root, sources, fetch);
        let unreached = fetched.as_ref().err().cloned();
        let reached = fetched.is_ok();
        let found = fetched
            .or_else(|_| catalog(root, sources, false))
            .unwrap_or_default();
        out.extend(found);
        Self {
            held: out,
            unreached,
            // The component collections `catalog` just skipped: a source counts
            // as read only where the tree it names is. Two sources naming one
            // library share a cache directory, so the other kind's subtree
            // being there does not make this one read. A base-image source is
            // not a collection this index speaks for, so it is never named
            // here.
            unread: match fetch && reached {
                true => Vec::new(),
                false => sources
                    .iter()
                    .filter(|collection| collection.kind == SourceKind::Components)
                    .filter(|collection| {
                        !crate::collection::cached(root, collection).is_some_and(|dir| {
                            dir.join(collection.subtree().unwrap_or("")).is_dir()
                        })
                    })
                    .map(|collection| collection.name.clone())
                    .collect(),
            },
            sourced: sources
                .iter()
                .any(|collection| collection.kind == SourceKind::Components),
        }
    }

    /// The declared collections this index cannot speak for, because nothing
    /// read them. A diagnostic saying nothing provides a capability has to say
    /// this too, or it claims to have searched them.
    pub fn unread(&self) -> &[String] {
        &self.unread
    }

    /// Why a fetch this scan asked for did not happen, said once and in this
    /// tool's voice: the fetcher itself prints nothing, so a flow that goes on
    /// to ask a question off an incomplete index says this before it asks.
    pub fn unreached(&self) -> Option<&str> {
        self.unreached.as_deref()
    }

    /// Whether the repository declares collections at all, which is how an
    /// empty `unread` is told from having nowhere else to look.
    pub fn sourced(&self) -> bool {
        self.sourced
    }

    /// The same thing as a sentence, so every diagnostic concluding from
    /// silence carries one voice. Empty where the scan was complete.
    pub fn unsearched(&self) -> String {
        // A scan that tried and failed is not a scan that skipped: saying
        // `tect import component` fetches it, to someone whose fetch has just
        // been refused, is the wrong sentence.
        if let Some(why) = &self.unreached {
            return format!(
                "A declared collection could not be read, so nothing searched it: {why}"
            );
        }
        let named: Vec<String> = self.unread.iter().map(|name| format!("`{name}`")).collect();
        match named.len() {
            0 => String::new(),
            1 => format!(
                "The {} collection is declared but not on this machine, so nothing read it: `tect import component` fetches it",
                named.join(", ")
            ),
            _ => format!(
                "The {} collections are declared but not on this machine, so nothing read them: `tect import component` fetches them",
                named.join(", ")
            ),
        }
    }

    /// Every component that meets `capability`, the repository's own first. A
    /// component meets it by its own name or by a declared name under it, the
    /// rule `Component::answers` keeps for an image.
    pub fn of(&self, capability: &str) -> Vec<&Provider> {
        self.held
            .iter()
            .filter(|held| {
                crate::resolve::answers(
                    &held.name,
                    held.declares.provides.iter().map(String::as_str),
                    capability,
                )
            })
            .collect()
    }

    /// Every component declaring `capability` that targets `family`, the
    /// repository's own first. An adapter role is filled per family, so `of`
    /// alone would hand a fedora image the deb adapter whenever the deb one
    /// sorts first. A base fits no requirement, because an image lists its
    /// one base itself.
    pub fn fitting(&self, capability: &str, family: &str) -> Vec<&Provider> {
        self.of(capability)
            .into_iter()
            .filter(|held| crate::resolve::targets(&held.declares.targets, family))
            .filter(|held| {
                !held
                    .declares
                    .provides
                    .iter()
                    .any(|name| name == crate::parse::component::BASE_IMAGE)
            })
            .collect()
    }

    /// The component filling a role on one family: it provides the capability the
    /// role is named by, and it targets the family. Every family needs the
    /// same role filled by a different component.
    pub fn adapter(&self, capability: &str, family: &str) -> Option<&Provider> {
        self.fitting(capability, family).into_iter().next()
    }

    /// Every component declaring a key of this kind.
    pub fn declaring_key(&self, kind: &str) -> Vec<&Provider> {
        self.held
            .iter()
            .filter(|held| held.declares.keys.iter().any(|has| has == kind))
            .collect()
    }

    /// Every component claiming one of these benchmark numbers, which answers
    /// which components help an image conform once the numbers are the ones the
    /// declared profile's rules are reached by.
    pub fn claiming(&self, numbers: &BTreeSet<String>) -> Vec<&Provider> {
        self.held
            .iter()
            .filter(|held| {
                held.declares
                    .satisfies
                    .iter()
                    .any(|number| numbers.contains(number))
            })
            .collect()
    }

    /// The one an image entry names, which is how what an image already has is
    /// read back off its declaration. An entry names a component under
    /// `components/`, never a library member.
    pub fn at(&self, dir: &str) -> Option<&Provider> {
        self.held
            .iter()
            .find(|held| held.owner.is_none() && held.name == dir)
    }
}

/// Every component every declared collection holds, by name and then by
/// collection.
/// `fetch` decides whether a collection that is not on this machine is
/// downloaded to answer or passed over. A grouped member is named by its path.
pub fn catalog(root: &Path, sources: &[Collection], fetch: bool) -> Result<Vec<Provider>, String> {
    let mut listed: Vec<Provider> = Vec::new();
    for collection in sources {
        if collection.kind != SourceKind::Components {
            continue;
        }
        let tree = match fetch {
            true => crate::collection::tree(root, collection)?,
            false => match crate::collection::cached(root, collection) {
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
        for name in crate::collection::members(&tree) {
            listed.push(Provider {
                owner: Some(collection.name.clone()),
                declares: crate::parse::component::summary(
                    &tree.join(&name).join(layout::MANIFEST_FILE),
                ),
                name,
                here: false,
            });
        }
    }
    listed.sort_by(|a, b| (&a.name, &a.owner).cmp(&(&b.name, &b.owner)));
    Ok(listed)
}

/// Every component directory the repository holds.
fn own(root: &Path, disk: &Disk) -> Vec<Provider> {
    disk.components()
        .map(|dir| Provider {
            owner: None,
            name: dir.clone(),
            here: true,
            declares: parse::component::summary(&layout::manifest(root, dir)),
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Writes a repository where `both` provides `x` and `y`, and `other`
    /// provides `y` alone.
    pub(crate) fn two_providers(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("tect-providers-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        crate::init::put(
            &root.join("components/both/manifest.kdl"),
            "schema-version 1\n\ntargets \"fedora\"\n\nprovides \"x\" \"y\"\n",
        )
        .unwrap();
        crate::init::put(
            &root.join("components/other/manifest.kdl"),
            "schema-version 1\n\ntargets \"fedora\"\n\nprovides \"y\"\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn an_entry_finds_the_component_it_names() {
        let root = two_providers("at");
        let index = Index::local(&root, &[], &Disk::scan(&root));
        let found = index.at("other").map(|held| held.name.as_str());
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(found, Some("other"));
    }
}

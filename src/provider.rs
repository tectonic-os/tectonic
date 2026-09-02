//! Given a capability, which module provides it: over the repository, over
//! what it has already imported, and over the collections it declares.
//!
//! One index, because three things ask the same question — the
//! unsatisfied-`requires` help, the offer `import module` makes, and the family
//! module `create image` seeds — a fourth asks it of a key kind, and a fifth
//! asks which modules claim the rules a profile selects.

use crate::model::remote::{Collection, REMOTE_DIR};
use crate::parse::disk::Disk;
use crate::parse::module::Summary;
use crate::{layout, parse};
use std::collections::BTreeSet;
use std::path::Path;

/// One module that could satisfy a requirement, and what it takes to get it.
pub struct Provider {
    /// The collection it comes from, absent for a module the repository owns.
    pub owner: Option<String>,
    /// Its directory under `modules/`, or its name inside the collection.
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

    /// Where its manifest sits under `modules/`, which is what an image entry
    /// naming it resolves to.
    pub fn dir(&self) -> String {
        match &self.owner {
            Some(owner) => format!("{REMOTE_DIR}/{owner}/{}", self.name),
            None => self.name.clone(),
        }
    }

    /// What a picker shows beside the name.
    pub fn about(&self) -> String {
        match self.declares.requires.is_empty() {
            true => self.declares.description.clone(),
            false => format!(
                "{} (requires {})",
                self.declares.description,
                self.declares.requires.join(", ")
            ),
        }
    }
}

/// Every module a repository could reach, the ones already on it first —
/// and the declared collections this scan never looked in, so that nothing
/// reading the index states as fact what it did not check.
pub struct Index {
    held: Vec<Provider>,
    /// Declared collections not on this machine, which a scan that does not
    /// fetch skips: empty when every one was read, and empty when none were
    /// declared, which `sourced` tells apart.
    unread: Vec<String>,
    /// Whether the repository declares any collections at all.
    sourced: bool,
    /// A member nested inside another member, which the walk stops short of and
    /// nothing else would ever mention.
    hidden: Vec<String>,
    /// Why a fetching scan fell back to what was already here. `None` where it
    /// did not fetch, or where the fetch worked.
    unreached: Option<String>,
}

impl Index {
    /// `fetch` decides whether a collection that is not on this machine is
    /// downloaded to answer, which resolution never does and a flow may.
    pub fn scan(root: &Path, sources: &[Collection], disk: &Disk, fetch: bool) -> Self {
        let mut out: Vec<Provider> = Vec::new();
        for dir in disk.modules() {
            let (owner, name) = split(dir);
            out.push(Provider {
                owner,
                name,
                here: true,
                declares: parse::module::summary(&layout::manifest(root, dir)),
            });
        }
        // One collection that cannot be reached takes the whole catalog with
        // it, so a scan that meant to fetch falls back to what is already
        // here: a reader asking what provides something is answered worse by
        // nothing at all than by what one unreachable collection is missing
        // from. `unread` is what still says the search was incomplete.
        let fetched = crate::import::catalog(root, sources, fetch);
        let unreached = fetched.as_ref().err().cloned();
        let reached = fetched.is_ok();
        let (found, hidden) = fetched
            .or_else(|_| crate::import::catalog(root, sources, false))
            .unwrap_or_default();
        for module in found {
            if !out.iter().any(|held| held.dir() == module.dir()) {
                out.push(module);
            }
        }
        Self {
            held: out,
            hidden,
            unreached,
            // The collections `catalog` just skipped: `cached` is the same
            // question it asked, and on `fetch` there is nothing to skip.
            unread: match fetch && reached {
                true => Vec::new(),
                false => sources
                    .iter()
                    .filter(|collection| crate::import::cached(root, collection).is_none())
                    .map(|collection| collection.name.clone())
                    .collect(),
            },
            sourced: !sources.is_empty(),
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

    /// The members no walk of a declared collection can reach. Nothing else
    /// names them: they are absent from the catalog, from `find` and from the
    /// picker alike, and absence is exactly what a silent one looks like.
    pub fn hidden(&self) -> &[String] {
        &self.hidden
    }

    /// The same thing as a sentence, so every diagnostic concluding from
    /// silence carries one voice. Empty where the scan was complete.
    pub fn unsearched(&self) -> String {
        // A scan that tried and failed is not a scan that skipped: saying
        // `tect fetch modules` downloads it, to someone whose fetch has just
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
                "The {} collection is declared but not on this machine, so nothing read it: `tect fetch modules` downloads it",
                named.join(", ")
            ),
            _ => format!(
                "The {} collections are declared but not on this machine, so nothing read them: `tect fetch modules` downloads them",
                named.join(", ")
            ),
        }
    }

    /// Every module declaring `capability`, the repository's own first.
    pub fn of(&self, capability: &str) -> Vec<&Provider> {
        self.held
            .iter()
            .filter(|held| held.declares.provides.iter().any(|has| has == capability))
            .collect()
    }

    /// Every module declaring `capability` that supports `family`, the
    /// repository's own first. An adapter role is filled per family, so a
    /// provider for another family is not a candidate for this image at all —
    /// `of` alone would hand a fedora image the deb adapter whenever the deb
    /// one sorts first.
    pub fn fitting(&self, capability: &str, family: &str) -> Vec<&Provider> {
        self.of(capability)
            .into_iter()
            .filter(|held| held.declares.supports.iter().any(|has| has == family))
            .collect()
    }

    /// The module filling a role on one family: it provides the capability the
    /// role is named by, and it supports the family. Every family needs the
    /// same role filled by a different module.
    pub fn adapter(&self, capability: &str, family: &str) -> Option<&Provider> {
        self.fitting(capability, family).into_iter().next()
    }

    /// Every module declaring a key of this kind.
    pub fn declaring_key(&self, kind: &str) -> Vec<&Provider> {
        self.held
            .iter()
            .filter(|held| held.declares.keys.iter().any(|has| has == kind))
            .collect()
    }

    /// Every module claiming one of these benchmark numbers, which answers
    /// which modules help an image conform once the numbers are the ones the
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
    /// read back off its declaration.
    pub fn at(&self, dir: &str) -> Option<&Provider> {
        self.held.iter().find(|held| held.dir() == dir)
    }
}

/// A module directory as the collection and name it stands for: everything
/// under `.remote` belongs to the collection it was fetched from.
fn split(dir: &str) -> (Option<String>, String) {
    match dir
        .strip_prefix(REMOTE_DIR)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(|rest| rest.split_once('/'))
    {
        Some((owner, name)) => (Some(owner.to_string()), name.to_string()),
        None => (None, dir.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fetched_module_is_the_collection_member_it_came_from() {
        let (owner, name) = split(".remote/one/browser");
        assert_eq!((owner.as_deref(), name.as_str()), (Some("one"), "browser"));
        let (owner, name) = split("my/editor");
        assert_eq!((owner, name.as_str()), (None, "my/editor"));
    }
}

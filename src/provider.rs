//! Given a capability, which module provides it: over the repository, over
//! what it has already imported, and over the collections it declares.
//!
//! One index, because three things ask the same question — the
//! unsatisfied-`requires` help, the offer `import module` makes, and the family
//! module `create image` seeds — and a fourth asks it of a key kind.

use crate::model::remote::{Collection, REMOTE_DIR};
use crate::parse::disk::Disk;
use crate::parse::module::Summary;
use crate::{layout, parse};
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

/// Every module a repository could reach, the ones already on it first.
pub struct Index(Vec<Provider>);

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
        for module in crate::import::catalog(root, sources, fetch).unwrap_or_default() {
            if !out.iter().any(|held| held.dir() == module.dir()) {
                out.push(module);
            }
        }
        Self(out)
    }

    /// Every module declaring `capability`, the repository's own first.
    pub fn of(&self, capability: &str) -> Vec<&Provider> {
        self.0
            .iter()
            .filter(|held| held.declares.provides.iter().any(|has| has == capability))
            .collect()
    }

    /// The module filling a role on one family: it provides the capability the
    /// role is named by, and it supports the family. Every family needs the
    /// same role filled by a different module.
    pub fn adapter(&self, capability: &str, family: &str) -> Option<&Provider> {
        self.of(capability)
            .into_iter()
            .find(|held| held.declares.supports.iter().any(|has| has == family))
    }

    /// Every module declaring a key of this kind.
    pub fn declaring_key(&self, kind: &str) -> Vec<&Provider> {
        self.0
            .iter()
            .filter(|held| held.declares.keys.iter().any(|has| has == kind))
            .collect()
    }

    /// The one an image entry names, which is how what an image already has is
    /// read back off its declaration.
    pub fn at(&self, dir: &str) -> Option<&Provider> {
        self.0.iter().find(|held| held.dir() == dir)
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

//! `import module`: one module out of a source collection, copied into the
//! tree. Nothing here writes an image file: what a repository holds and what an
//! image is made of are different questions.

use crate::model::remote::{At, Collection};
use crate::prompt::Prompt;
use crate::ui::Choice;
use std::path::{Path, PathBuf};

/// Where a fetched collection is unpacked. Under `out/`, which is ignored: the
/// copy that gets committed is the one under `modules/`.
const CACHE: &str = "out/sources";

/// One collection that has the module, and where its directory is on disk.
pub struct Found {
    pub owner: String,
    pub dir: PathBuf,
}

/// `<owner>/<name>`, or a bare name every collection is searched for.
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
pub fn find(root: &Path, sources: &[Collection], name: &str) -> Result<Vec<Found>, String> {
    let (owner, module) = split(name);
    if module.is_empty() || module.contains('/') || module.starts_with('.') {
        return Err(format!(
            "`{name}` is not a module: `<name>`, or `<owner>/<name>` when two collections have it"
        ));
    }
    if sources.is_empty() {
        return Err(format!(
            "repo.kdl declares no `sources`, so there is no collection to import from.\n\n\
             This is the block `tect create repo` writes:\n\n{}",
            crate::init::SOURCES
        ));
    }

    let mut searched: Vec<&str> = Vec::new();
    let mut found: Vec<Found> = Vec::new();
    for collection in sources {
        if owner.is_some_and(|owner| owner != collection.name) {
            continue;
        }
        searched.push(&collection.name);
        let dir = tree(root, collection)?
            .join(collection.subtree().unwrap_or(""))
            .join(module);
        if dir.join("module.kdl").is_file() {
            found.push(Found {
                owner: collection.name.clone(),
                dir,
            });
        }
    }

    if searched.is_empty() {
        return Err(format!(
            "no collection called `{}`; repo.kdl declares {}",
            owner.unwrap_or_default(),
            names(sources)
        ));
    }
    if found.is_empty() {
        return Err(format!(
            "no module called `{module}` in {}",
            searched.join(", ")
        ));
    }
    Ok(found)
}

/// One module a collection has, and what its manifest says about it.
pub struct Listed {
    pub owner: String,
    pub name: String,
    /// `<owner>/<name>`, which names it whatever else has the same name.
    pub qualified: String,
    pub description: String,
    pub requires: Vec<String>,
    /// The key kinds it declares, which is what an absent one is traced back
    /// to this module by.
    pub keys: Vec<String>,
}

impl Listed {
    /// What a picker shows beside the name.
    pub fn about(&self) -> String {
        match self.requires.is_empty() {
            true => self.description.clone(),
            false => format!(
                "{} (requires {})",
                self.description,
                self.requires.join(", ")
            ),
        }
    }
}

/// Every module every declared collection holds, by name and then by collection.
pub fn catalog(root: &Path, sources: &[Collection]) -> Result<Vec<Listed>, String> {
    let mut listed: Vec<Listed> = Vec::new();
    for collection in sources {
        let tree = tree(root, collection)?.join(collection.subtree().unwrap_or(""));
        let dirs = std::fs::read_dir(&tree)
            .map_err(|err| format!("`{}`: {}: {err}", collection.name, tree.display()))?;
        for dir in dirs.flatten().map(|entry| entry.path()) {
            if !dir.join("module.kdl").is_file() {
                continue;
            }
            let name = dir.file_name().unwrap_or_default().to_string_lossy();
            let (description, requires, keys) =
                crate::parse::module::summary(&dir.join("module.kdl"));
            listed.push(Listed {
                qualified: format!("{}/{name}", collection.name),
                name: name.into_owned(),
                owner: collection.name.clone(),
                description,
                requires,
                keys,
            });
        }
    }
    listed.sort_by(|a, b| (&a.name, &a.owner).cmp(&(&b.name, &b.owner)));
    Ok(listed)
}

/// Which module, out of everything the collections hold.
pub fn choose(root: &Path, sources: &[Collection], prompt: &Prompt) -> Result<String, String> {
    let listed = catalog(root, sources)?;
    if listed.is_empty() {
        return Err(format!("no module in {}", names(sources)));
    }
    let options: Vec<Choice> = listed
        .iter()
        .map(|module| Choice::new(&module.qualified, module.about()))
        .collect();
    match prompt.choose("which module", &options)? {
        Some(chosen) => Ok(listed[chosen].qualified.clone()),
        None => Err("no module chosen; `tect import module <name>` names one".to_string()),
    }
}

/// The collection's tree where it is already on this machine: the directory it
/// is, or an archive fetched at the hash it is still pinned to. Nothing is
/// fetched, so a reader that only wants what is there costs no network.
pub fn cached(root: &Path, collection: &Collection) -> Option<PathBuf> {
    let dir = match &collection.at {
        At::Dir(dir) => root.join(dir),
        At::Archive(remote) => {
            let pin = root.join(CACHE).join(format!("{}.pin", collection.name));
            match std::fs::read_to_string(&pin).ok().as_deref() == Some(remote.sha256.as_str()) {
                true => root.join(CACHE).join(&collection.name),
                false => return None,
            }
        }
    };
    dir.is_dir().then_some(dir)
}

/// The collection's tree on this machine: the directory it already is, or the
/// pinned archive, fetched and verified once and kept for the next import.
fn tree(root: &Path, collection: &Collection) -> Result<PathBuf, String> {
    if let Some(dir) = cached(root, collection) {
        return Ok(dir);
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

    let dir = root.join(CACHE).join(&collection.name);
    let pin = root.join(CACHE).join(format!("{}.pin", collection.name));
    let _ = std::fs::remove_dir_all(&dir);
    let url = remote.url_resolved();
    crate::runtime::extract(&url, &remote.sha256, &dir, &["--strip-components=1"])
        .map_err(|err| format!("`{}`: {err}", collection.name))?;
    std::fs::write(&pin, &remote.sha256).map_err(|err| format!("{}: {err}", pin.display()))?;
    Ok(dir)
}

/// Where the module goes, at the path its owner qualifies, refused when
/// something is already there.
pub fn destination(root: &Path, found: &Found, module: &str) -> Result<PathBuf, String> {
    let dest = PathBuf::from("modules").join(&found.owner).join(module);
    match root.join(&dest).exists() {
        true => Err(format!(
            "{} is already there; delete it to import over it",
            dest.display()
        )),
        false => Ok(dest),
    }
}

/// Copies the module in. The repository is a working tree, so committing what
/// this wrote is the user's.
pub fn vendor(root: &Path, found: &Found, dest: &Path) -> Result<(), String> {
    crate::init::copy_tree(&found.dir, &root.join(dest))
}

//! `module import`: one module out of a source collection, copied into the
//! tree. Nothing here writes an image file: what a repository holds and what an
//! image is made of are different questions.

use crate::model::remote::{At, Collection};
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
        return Err(
            "repo.kdl declares no `sources`, so there is no collection to import from".to_string(),
        );
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

/// The collection's tree on this machine: the directory it already is, or the
/// pinned archive, fetched and verified once and kept for the next import.
fn tree(root: &Path, collection: &Collection) -> Result<PathBuf, String> {
    let remote = match &collection.at {
        At::Dir(dir) => {
            let path = root.join(dir);
            return match path.is_dir() {
                true => Ok(path),
                false => Err(format!(
                    "`{}` is {}, which is not a directory on this machine",
                    collection.name,
                    path.display()
                )),
            };
        }
        At::Archive(remote) => remote,
    };

    let dir = root.join(CACHE).join(&collection.name);
    let pin = root.join(CACHE).join(format!("{}.pin", collection.name));
    if std::fs::read_to_string(&pin).ok().as_deref() == Some(remote.sha256.as_str()) {
        return Ok(dir);
    }

    let _ = std::fs::remove_dir_all(&dir);
    let url = remote.url_resolved();
    let target = dir.to_string_lossy().into_owned();
    crate::runtime::fetch(&[
        "tree",
        &url,
        &remote.sha256,
        &target,
        "--strip-components=1",
    ])
    .map_err(|err| format!("`{}`: {err}", collection.name))?;
    std::fs::write(&pin, &remote.sha256).map_err(|err| format!("{}: {err}", pin.display()))?;
    Ok(dir)
}

/// Copies the module in, at the path its owner qualifies. The repository is a
/// working tree, so committing what this wrote is the user's.
pub fn vendor(root: &Path, found: &Found, module: &str) -> Result<PathBuf, String> {
    let dest = PathBuf::from("modules").join(&found.owner).join(module);
    let full = root.join(&dest);
    if full.exists() {
        return Err(format!(
            "{} is already there; delete it to import over it",
            dest.display()
        ));
    }
    crate::init::copy_tree(&found.dir, &full)?;
    Ok(dest)
}

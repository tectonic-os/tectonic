//! The out-of-tree modules an image pins, brought to where the resolver looks
//! for them. One fetch directory for the repository: a module two images pin is
//! one tree on disk.

use crate::model::image::List;
use crate::model::remote::REMOTE_DIR;
use std::fs;
use std::path::{Path, PathBuf};

/// What a fetched tree hashed to, kept out of the tree it describes so nothing
/// under `modules/` is tool-written state.
const STAMPS: &str = "out/remote-modules";

struct Pin {
    name: String,
    git_ref: String,
    url: String,
    sha256: String,
    /// The module's directory inside the archive.
    path: String,
}

impl Pin {
    /// What the stamp has to say for the tree on disk to be the pinned one.
    fn stamped(&self) -> String {
        format!("{} {} {}", self.sha256, self.url, self.path)
    }
}

/// Fetches what is not already current and removes what is no longer pinned,
/// reporting what it did.
pub fn modules(root: &Path, list: &List) -> Result<Vec<String>, String> {
    let pins = pins(list);
    let mut said = prune(root, &pins)?;

    for pin in &pins {
        let dir = root.join("modules").join(REMOTE_DIR).join(&pin.name);
        let stamp = root.join(STAMPS).join(format!("{}.pin", pin.name));
        let current = fs::read_to_string(&stamp)
            .is_ok_and(|found| found.trim_end() == pin.stamped())
            && dir.join("module.kdl").is_file();
        if current {
            said.push(format!("{} {} is current", pin.name, pin.git_ref));
            continue;
        }

        let tmp = root.join(format!("out/fetch-module.{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        crate::runtime::extract(&pin.url, Some(&pin.sha256), &tmp, &["--strip-components=1"])?;

        let source = match pin.path.is_empty() {
            true => tmp.clone(),
            false => tmp.join(&pin.path),
        };
        let placed = place(&source, &dir, pin);
        let _ = fs::remove_dir_all(&tmp);
        placed?;

        crate::init::put(&stamp, &format!("{}\n", pin.stamped()))?;
        said.push(format!("{} {} fetched and verified", pin.name, pin.git_ref));
    }
    Ok(said)
}

fn place(source: &Path, dir: &Path, pin: &Pin) -> Result<(), String> {
    if !source.join("module.kdl").is_file() {
        return Err(format!(
            "{}: {} ships no module.kdl {}",
            pin.name,
            pin.url,
            match pin.path.is_empty() {
                true => "at its root".to_string(),
                false => format!("under {}", pin.path),
            }
        ));
    }
    let _ = fs::remove_dir_all(dir);
    crate::init::copy_tree(source, dir)
}

/// Every pin, first declaration wins, so two images pinning one module agree by
/// construction rather than by fetch order.
fn pins(list: &List) -> Vec<Pin> {
    let mut out: Vec<Pin> = Vec::new();
    for image in &list.images {
        for entry in &image.entries {
            let Some(remote) = &entry.remote else {
                continue;
            };
            if out.iter().any(|pin| pin.name == entry.path) {
                continue;
            }
            out.push(Pin {
                name: entry.path.clone(),
                git_ref: remote.version.clone().unwrap_or_default(),
                url: remote.url_resolved().unwrap_or_default(),
                sha256: remote.sha256.clone().unwrap_or_default(),
                path: remote.path.clone().unwrap_or_default(),
            });
        }
    }
    out
}

/// Fetched trees no image pins any more, and the empty directories they leave.
fn prune(root: &Path, pins: &[Pin]) -> Result<Vec<String>, String> {
    let fetched = root.join("modules").join(REMOTE_DIR);
    let mut said = Vec::new();
    for dir in trees(&fetched, &PathBuf::new()) {
        let name = dir.display().to_string();
        if pins.iter().any(|pin| pin.name == name) {
            continue;
        }
        fs::remove_dir_all(fetched.join(&dir)).map_err(|err| format!("{name}: {err}"))?;
        let _ = fs::remove_file(root.join(STAMPS).join(format!("{name}.pin")));
        said.push(format!("{name} is no longer pinned, removing"));
    }
    empties(&fetched);
    Ok(said)
}

/// Every fetched module tree under `dir`, by its path relative to the fetch
/// directory, which is the name the image pinned it under.
fn trees(dir: &Path, rel: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let rel = rel.join(entry.file_name());
        match path.join("module.kdl").is_file() {
            true => out.push(rel),
            false => out.extend(trees(&path, &rel)),
        }
    }
    out
}

/// Removes `dir` and everything under it that holds nothing.
fn empties(dir: &Path) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.path().is_dir() {
            empties(&entry.path());
        }
    }
    let _ = fs::remove_dir(dir);
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// A pin named `owner/module` is one tree at that depth, not two.
    #[test]
    fn trees_are_named_by_their_pin() {
        let root = std::env::temp_dir().join(format!("tect-fetch.{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for name in ["flat", "owner/nested"] {
            crate::init::put(&root.join(name).join("module.kdl"), "").unwrap();
        }
        std::fs::create_dir_all(root.join("owner/half-removed")).unwrap();

        let mut found = super::trees(&root, Path::new(""));
        found.sort();
        assert_eq!(found, vec![PathBuf::from("flat"), "owner/nested".into()]);

        super::empties(&root.join("owner"));
        assert!(root.join("owner/nested").is_dir());
        assert!(!root.join("owner/half-removed").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}

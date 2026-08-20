//! The out-of-tree modules an image references, brought to where the resolver looks
//! for them. One fetch directory for the repository: a module two images pin is
//! one tree on disk.

use crate::layout;
use crate::model::image::List;
use crate::model::remote::{At, REMOTE_DIR};
use std::fs;
use std::path::{Path, PathBuf};

struct Pin {
    name: String,
    git_ref: String,
    from: From,
}

enum From {
    Dir(PathBuf),
    Archive {
        url: String,
        sha256: Option<String>,
        /// The module's directory inside the archive.
        path: String,
    },
}

impl Pin {
    /// What the stamp has to say for the tree on disk to be the pinned one.
    fn stamped(&self) -> Option<String> {
        match &self.from {
            From::Dir(_) => None,
            From::Archive { url, sha256, path } => sha256
                .as_ref()
                .map(|sha256| format!("{sha256} {url} {path}")),
        }
    }
}

/// Fetches what is not already current and removes what is no longer pinned,
/// reporting what it did.
pub fn modules(root: &Path, list: &List) -> Result<Vec<String>, String> {
    let pins = pins(root, list);
    let mut said = prune(root, &pins)?;

    for pin in &pins {
        let dir = layout::module(root, REMOTE_DIR).join(&pin.name);
        let stamp = root.join(layout::STAMPS).join(format!("{}.pin", pin.name));
        let current = pin.stamped().is_some_and(|want| {
            fs::read_to_string(&stamp).is_ok_and(|found| found.trim_end() == want)
        }) && dir.join(layout::MODULE_FILE).is_file();
        if current {
            said.push(format!("{} {} is current", pin.name, pin.git_ref));
            continue;
        }

        let mut tmp = None;
        let source = match &pin.from {
            From::Dir(source) => source.clone(),
            From::Archive { url, sha256, path } => {
                let work = layout::out(root).join(format!("fetch-module.{}", std::process::id()));
                let _ = fs::remove_dir_all(&work);
                crate::runtime::extract(url, sha256.as_deref(), &work, &["--strip-components=1"])?;
                let source = match path.is_empty() {
                    true => work.clone(),
                    false => work.join(path),
                };
                tmp = Some(work);
                source
            }
        };
        let placed = place(&source, &dir, pin);
        if let Some(tmp) = tmp {
            let _ = fs::remove_dir_all(tmp);
        }
        placed?;

        if let Some(stamped) = pin.stamped() {
            crate::init::put(&stamp, &format!("{stamped}\n"))?;
        } else {
            let _ = fs::remove_file(&stamp);
        }
        said.push(match &pin.from {
            From::Dir(_) => format!("{} copied from its local collection", pin.name),
            From::Archive {
                sha256: Some(_), ..
            } => format!("{} {} fetched and verified", pin.name, pin.git_ref),
            From::Archive { sha256: None, .. } => {
                format!("{} {} fetched unverified", pin.name, pin.git_ref)
            }
        });
    }
    Ok(said)
}

fn place(source: &Path, dir: &Path, pin: &Pin) -> Result<(), String> {
    if !source.join(layout::MODULE_FILE).is_file() {
        return Err(format!(
            "{}: {} ships no module.kdl {}",
            pin.name,
            match &pin.from {
                From::Dir(path) => path.display().to_string(),
                From::Archive { url, .. } => url.clone(),
            },
            match &pin.from {
                From::Dir(_) => "at that path".to_string(),
                From::Archive { path, .. } if path.is_empty() => "at its root".to_string(),
                From::Archive { path, .. } => format!("under {path}"),
            }
        ));
    }
    let _ = fs::remove_dir_all(dir);
    crate::init::copy_tree(source, dir).map(drop)
}

/// Every pin, first declaration wins, so two images pinning one module agree by
/// construction rather than by fetch order.
fn pins(root: &Path, list: &List) -> Vec<Pin> {
    let mut out: Vec<Pin> = Vec::new();
    for image in &list.images {
        for entry in &image.entries {
            if out.iter().any(|pin| pin.name == entry.path) {
                continue;
            }
            let pin = match &entry.source {
                Some(name) => {
                    let Some(collection) = list.sources.iter().find(|source| &source.name == name)
                    else {
                        continue;
                    };
                    match &collection.at {
                        At::Dir(path) => Pin {
                            name: entry.path.clone(),
                            git_ref: "local".into(),
                            from: From::Dir(rooted(root, path).join(entry.name())),
                        },
                        At::Archive(remote) => Pin {
                            name: entry.path.clone(),
                            git_ref: remote.version.clone().unwrap_or_default(),
                            from: From::Archive {
                                url: remote.url_resolved().unwrap_or_default(),
                                sha256: remote.sha256.clone(),
                                path: PathBuf::from(remote.path.as_deref().unwrap_or(""))
                                    .join(entry.name())
                                    .display()
                                    .to_string(),
                            },
                        },
                    }
                }
                None => {
                    let Some(remote) = &entry.remote else {
                        continue;
                    };
                    Pin {
                        name: entry.path.clone(),
                        git_ref: remote.version.clone().unwrap_or_default(),
                        from: From::Archive {
                            url: remote.url_resolved().unwrap_or_default(),
                            sha256: remote.sha256.clone(),
                            path: remote.path.clone().unwrap_or_default(),
                        },
                    }
                }
            };
            out.push(pin);
        }
    }
    out
}

fn rooted(root: &Path, path: &str) -> PathBuf {
    match Path::new(path).is_absolute() {
        true => PathBuf::from(path),
        false => root.join(path),
    }
}

/// Fetched trees no image references any more, and the empty directories they leave.
fn prune(root: &Path, pins: &[Pin]) -> Result<Vec<String>, String> {
    let fetched = layout::module(root, REMOTE_DIR);
    let mut said = Vec::new();
    for dir in trees(&fetched, &PathBuf::new()) {
        let name = dir.display().to_string();
        if pins.iter().any(|pin| pin.name == name) {
            continue;
        }
        fs::remove_dir_all(fetched.join(&dir)).map_err(|err| format!("{name}: {err}"))?;
        let _ = fs::remove_file(root.join(layout::STAMPS).join(format!("{name}.pin")));
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
        match path.join(layout::MODULE_FILE).is_file() {
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

    #[test]
    fn a_collection_member_is_copied_to_the_remote_tree() {
        let root = std::env::temp_dir().join(format!("tect-fetch-source.{}", std::process::id()));
        let collection = root.join("collection");
        let _ = std::fs::remove_dir_all(&root);
        crate::init::put(
            &collection.join("hello/module.kdl"),
            "description \"Says hello\"\n\nsupports \"fedora\"\n",
        )
        .unwrap();
        crate::init::put(
            &root.join("repo.kdl"),
            &format!(
                "schema-version 1\nname \"Example\"\nsources {{\n    one {:?}\n}}\n",
                collection.display()
            ),
        )
        .unwrap();
        crate::init::put(
            &root.join("image.kdl"),
            "image {\n    name \"Example\"\n    base \"example.invalid/image\" { family \"fedora\" }\n    modules {\n        source \"one\" { module \"hello\" }\n    }\n}\n",
        )
        .unwrap();

        let (list, issues, _) = crate::declarations(&root);
        assert!(issues.is_empty(), "{}", issues.plain());
        super::modules(&root, &list).unwrap();
        assert!(root.join("modules/.remote/one/hello/module.kdl").is_file());

        crate::init::put(
            &root.join("repo.kdl"),
            "schema-version 1\nname \"Example\"\nsources {\n    one {\n        pin {\n            unpinned \"test\"\n            version \"main\"\n            url \"https://example.invalid/{version}\"\n        }\n    }\n}\naudit { enforce #true }\n",
        )
        .unwrap();
        let (_, issues, _) = crate::declarations(&root);
        assert!(issues.plain().contains("follows an unverified ref"));
        let _ = std::fs::remove_dir_all(root);
    }

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

//! The out-of-tree modules an image references, brought to where the resolver looks
//! for them. One fetch directory for the repository: a module two images pin is
//! one tree on disk.

use crate::layout;
use crate::model::image::List;
use crate::model::remote::{At, Kind as SourceKind, REMOTE_DIR};
use std::fs;
use std::path::{Path, PathBuf};

struct Pin {
    name: String,
    git_ref: String,
    from: From,
}

enum From {
    Collection {
        dir: PathBuf,
        /// None for a local directory, otherwise whether the archive had a hash.
        verified: Option<bool>,
        stamp: Option<String>,
    },
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
            From::Collection { stamp, .. } => stamp.clone(),
            From::Archive { url, sha256, path } => sha256
                .as_ref()
                .map(|sha256| format!("{sha256} {url} {path}")),
        }
    }
}

/// Fetches what is not already current and removes what is no longer pinned,
/// reporting what it did.
pub fn modules(root: &Path, list: &List) -> Result<Vec<String>, String> {
    let wanted = names(list);
    let mut said = prune(root, &wanted)?;
    let pins = pins(root, list)?;

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
            From::Collection { dir, .. } => dir.clone(),
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
            From::Collection { verified: None, .. } => {
                format!("{} copied from its local collection", pin.name)
            }
            From::Collection {
                verified: Some(true),
                ..
            } => format!(
                "{} {} copied from its verified collection",
                pin.name, pin.git_ref
            ),
            From::Collection {
                verified: Some(false),
                ..
            } => format!(
                "{} {} copied from its unverified collection",
                pin.name, pin.git_ref
            ),
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
                From::Collection { dir, .. } => dir.display().to_string(),
                From::Archive { url, .. } => url.clone(),
            },
            match &pin.from {
                From::Collection { .. } => "at that path".to_string(),
                From::Archive { path, .. } if path.is_empty() => "at its root".to_string(),
                From::Archive { path, .. } => format!("under {path}"),
            }
        ));
    }
    let _ = fs::remove_dir_all(dir);
    crate::init::copy_tree(source, dir).map(drop)
}

/// Every pin, first declaration wins, so two images pinning one module agree by
/// construction.
fn pins(root: &Path, list: &List) -> Result<Vec<Pin>, String> {
    let mut out: Vec<Pin> = Vec::new();
    let mut trees: Vec<(String, PathBuf)> = Vec::new();
    for image in &list.images {
        for entry in &image.entries {
            if out.iter().any(|pin| pin.name == entry.path) {
                continue;
            }
            let pin =
                match &entry.source {
                    Some(name) => {
                        let Some(collection) = list.sources.iter().find(|source| {
                            source.kind == SourceKind::Modules && &source.name == name
                        }) else {
                            continue;
                        };
                        let tree = match trees.iter().find(|(found, _)| found == name) {
                            Some((_, tree)) => tree.clone(),
                            None => {
                                let tree = crate::import::tree(root, collection)?;
                                trees.push((name.clone(), tree.clone()));
                                tree
                            }
                        };
                        let (git_ref, verified) = match &collection.at {
                            At::Dir(_) => ("local".into(), None),
                            At::Git(remote) => (
                                remote.version.clone().unwrap_or_default(),
                                Some(!remote.unpinned()),
                            ),
                        };
                        let stamp = collection.pin().and_then(|remote| {
                            remote.sha256.as_ref().map(|sha256| {
                                let member = Path::new(collection.subtree().unwrap_or(""))
                                    .join(entry.name());
                                format!(
                                    "{sha256} {} {}",
                                    remote.url_resolved().unwrap_or_default(),
                                    member.display()
                                )
                            })
                        });
                        Pin {
                            name: entry.path.clone(),
                            git_ref,
                            from: From::Collection {
                                dir: tree
                                    .join(collection.subtree().unwrap_or(""))
                                    .join(entry.name()),
                                verified,
                                stamp,
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
    Ok(out)
}

/// Names the remote trees the declarations still reference, without fetching them.
fn names(list: &List) -> Vec<String> {
    let mut names = Vec::new();
    for entry in list.images.iter().flat_map(|image| &image.entries) {
        let declared = entry.source.as_ref().is_some_and(|name| {
            list.sources
                .iter()
                .any(|collection| &collection.name == name)
        });
        if (declared || entry.remote.is_some()) && !names.contains(&entry.path) {
            names.push(entry.path.clone());
        }
    }
    names
}

/// Fetched trees no image references any more, and the empty directories they leave.
fn prune(root: &Path, names: &[String]) -> Result<Vec<String>, String> {
    let fetched = layout::module(root, REMOTE_DIR);
    let mut said = Vec::new();
    for dir in trees(&fetched, &PathBuf::new()) {
        let name = dir.display().to_string();
        if names.contains(&name) {
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
    use crate::model::remote::{At, Collection, Kind as SourceKind};
    use crate::provenance::{Evidence, ShaFrom, Tracker};
    use std::path::{Path, PathBuf};

    fn git(repo: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit(repo: &Path, message: &str) {
        git(repo, &["add", "."]);
        git(
            repo,
            &[
                "-c",
                "user.name=Tectonic tests",
                "-c",
                "user.email=tests@example.invalid",
                "commit",
                "--quiet",
                "-m",
                message,
            ],
        );
    }

    fn archive_hash(repo: &Path) -> String {
        let archive = repo.join("../source.tar");
        let output = format!("--output={}", archive.display());
        git(repo, &["archive", "--format=tar", &output, "HEAD"]);
        crate::runtime::sha256_file(&archive).unwrap()
    }

    fn git_source(repo: &Path, sha256: Option<String>) -> Collection {
        let mut pin = Evidence::new(crate::diag::Span::default());
        pin.url = Some(format!("file://{}", repo.display()));
        pin.version = Some("main".to_string());
        pin.sha256 = sha256;
        pin.from = ShaFrom::Manual;
        pin.tracker = match pin.sha256.is_some() {
            true => Tracker::Manual("test fixture".to_string()),
            false => Tracker::Unpinned("test fixture".to_string()),
        };
        Collection {
            kind: SourceKind::Modules,
            name: "one".to_string(),
            at: At::Git(pin),
            span: crate::diag::Span::default(),
        }
    }

    fn list_with(root: &Path, source: Collection) -> crate::model::image::List {
        let name = &source.name;
        crate::init::put(
            &root.join("repo.kdl"),
            &format!(
                "schema-version 1\nname \"Example\"\nsources {{ modules {name:?} {{ dir \"source\" }} }}\n"
            ),
        )
        .unwrap();
        crate::init::put(
            &root.join("image.kdl"),
            &format!(
                "schema-version 1\n\nimage {{\n    name \"Example\"\n    base \"example.invalid/image\" {{ family \"fedora\" }}\n    modules {{ source {name:?} {{ module \"hello\" }} }}\n}}\n"
            ),
        )
        .unwrap();
        let (mut list, issues, _) = crate::declarations(root);
        assert!(issues.is_empty(), "{}", issues.plain());
        list.sources = vec![source];
        list
    }

    fn next_fetch(root: &Path) {
        let cache = root.join(crate::layout::SOURCES_CACHE);
        for entry in std::fs::read_dir(cache).into_iter().flatten().flatten() {
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "run")
            {
                std::fs::remove_file(entry.path()).unwrap();
            }
        }
    }

    #[test]
    fn a_collection_member_is_copied_to_the_remote_tree() {
        let root = std::env::temp_dir().join(format!("tect-fetch-source.{}", std::process::id()));
        let collection = root.join("collection");
        let _ = std::fs::remove_dir_all(&root);
        crate::init::put(
            &collection.join("hello/module.kdl"),
            "schema-version 1\n\ndescription \"Says hello\"\n\nsupports \"fedora\"\n",
        )
        .unwrap();
        crate::init::put(
            &collection.join("goodbye/module.kdl"),
            "schema-version 1\n\ndescription \"Says goodbye\"\n\nsupports \"fedora\"\n",
        )
        .unwrap();
        crate::init::put(
            &root.join("repo.kdl"),
            &format!(
                "schema-version 1\nname \"Example\"\nsources {{\n    modules \"one\" {{ dir {:?} }}\n}}\n",
                collection.display()
            ),
        )
        .unwrap();
        crate::init::put(
            &root.join("image.kdl"),
            "schema-version 1\n\nimage {\n    name \"Example\"\n    base \"example.invalid/image\" { family \"fedora\" }\n    modules {\n        source \"one\" { module \"hello\"; module \"goodbye\" }\n    }\n}\n",
        )
        .unwrap();

        let (list, issues, _) = crate::declarations(&root);
        assert!(issues.is_empty(), "{}", issues.plain());
        super::modules(&root, &list).unwrap();
        assert!(root.join("modules/.remote/one/hello/module.kdl").is_file());
        assert!(root
            .join("modules/.remote/one/goodbye/module.kdl")
            .is_file());

        crate::init::put(
            &root.join("image.kdl"),
            "schema-version 1\n\nimage {\n    name \"Example\"\n    base \"example.invalid/image\" { family \"fedora\" }\n    modules { source \"missing\" { module \"hello\" } }\n}\n",
        )
        .unwrap();
        let (list, issues, _) = crate::declarations(&root);
        assert!(!issues.is_empty());
        assert!(super::names(&list).is_empty());

        crate::init::put(
            &root.join("image.kdl"),
            "schema-version 1\n\nimage {\n    name \"Example\"\n    base \"example.invalid/image\" { family \"fedora\" }\n    modules { source \"one\" { module \"hello\" } }\n}\n",
        )
        .unwrap();
        crate::init::put(
            &root.join("repo.kdl"),
            "schema-version 1\nname \"Example\"\nsources { modules \"one\" { dir \"missing\" } }\n",
        )
        .unwrap();
        let (list, issues, _) = crate::declarations(&root);
        assert!(issues.is_empty(), "{}", issues.plain());
        let failed = super::modules(&root, &list).unwrap_err();
        assert!(failed.contains("not a directory"), "{failed}");
        assert!(!root.join("modules/.remote/one/goodbye/module.kdl").exists());

        crate::init::put(
            &root.join("repo.kdl"),
            "schema-version 1\nname \"Example\"\nsources {\n    modules \"one\" {\n        url \"https://example.invalid/repository\"\n        version \"main\"\n        unpinned \"test\"\n    }\n}\naudit { enforce #true }\n",
        )
        .unwrap();
        let (_, issues, _) = crate::declarations(&root);
        assert!(issues.plain().contains("follows an unverified ref"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_pinned_collection_member_is_current_the_second_time() {
        let root = std::env::temp_dir().join(format!("tect-fetch-pinned.{}", std::process::id()));
        let repository = root.join("library");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&repository).unwrap();
        git(&repository, &["init", "--quiet", "-b", "main"]);
        crate::init::put(
            &repository.join("hello/module.kdl"),
            "schema-version 1\n\ndescription \"Says hello\"\n\nsupports \"fedora\"\n",
        )
        .unwrap();
        commit(&repository, "initial");
        let list = list_with(
            &root,
            git_source(&repository, Some(archive_hash(&repository))),
        );
        let first = super::modules(&root, &list).unwrap();
        assert!(first
            .iter()
            .any(|line| line.contains("copied from its verified collection")));
        assert_eq!(
            super::modules(&root, &list).unwrap(),
            ["one/hello main is current"]
        );
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
    /// A file the collection deleted is gone from the tree the next fetch
    /// places. Surviving into a `plan.json` leaves CI disagreeing with it.
    #[test]
    fn a_deleted_file_does_not_survive_the_next_fetch() {
        let root = std::env::temp_dir().join(format!("tect-fetch-stale.{}", std::process::id()));
        let repository = root.join("library");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&repository).unwrap();
        git(&repository, &["init", "--quiet", "-b", "main"]);
        crate::init::put(
            &repository.join("hello/module.kdl"),
            "schema-version 1\n\ndescription \"Says hello\"\n\nsupports \"fedora\"\n",
        )
        .unwrap();
        crate::init::put(&repository.join("hello/files/gone.conf"), "old\n").unwrap();
        commit(&repository, "initial");
        let list = list_with(&root, git_source(&repository, None));
        super::modules(&root, &list).unwrap();
        assert!(root
            .join("modules/.remote/one/hello/files/gone.conf")
            .is_file());

        std::fs::remove_file(repository.join("hello/files/gone.conf")).unwrap();
        commit(&repository, "remove gone file");
        next_fetch(&root);
        let said = super::modules(&root, &list).unwrap();
        assert!(
            !root
                .join("modules/.remote/one/hello/files/gone.conf")
                .exists(),
            "stale file survived: {said:?}"
        );

        // A pinned source instead uses the canonical archive hash as its
        // current stamp, and changing the expected hash replaces the tree.
        crate::init::put(&repository.join("hello/files/back.conf"), "new\n").unwrap();
        commit(&repository, "add back file");
        let pinned = list_with(
            &root,
            git_source(&repository, Some(archive_hash(&repository))),
        );
        super::modules(&root, &pinned).unwrap();
        assert!(root
            .join("modules/.remote/one/hello/files/back.conf")
            .is_file());

        std::fs::remove_file(repository.join("hello/files/back.conf")).unwrap();
        commit(&repository, "remove back file");
        let repinned = list_with(
            &root,
            git_source(&repository, Some(archive_hash(&repository))),
        );
        let said = super::modules(&root, &repinned).unwrap();
        assert!(
            !root
                .join("modules/.remote/one/hello/files/back.conf")
                .exists(),
            "a pinned collection kept a file its new hash does not carry: {said:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// A `sha256` that does not match the canonical archive stops the fetch, so
    /// a commit that changed under a tag lands nowhere.
    #[test]
    fn a_wrong_hash_stops_the_fetch() {
        let root = std::env::temp_dir().join(format!("tect-fetch-hash.{}", std::process::id()));
        let repository = root.join("library");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&repository).unwrap();
        git(&repository, &["init", "--quiet", "-b", "main"]);
        crate::init::put(
            &repository.join("hello/module.kdl"),
            "schema-version 1\n\ndescription \"Says hello\"\n\nsupports \"fedora\"\n",
        )
        .unwrap();
        commit(&repository, "initial");
        let list = list_with(&root, git_source(&repository, Some("0".repeat(64))));
        let failed = super::modules(&root, &list).unwrap_err();
        assert!(
            failed.contains("expected sha256") && failed.contains("got"),
            "{failed}"
        );
        assert!(!root.join("modules/.remote/one/hello/module.kdl").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    /// Two sources that name one repository at one selector share the one
    /// fetched tree. The repository is gone before the second source reads it,
    /// so a fetch keyed by the alias would fail here.
    #[test]
    fn two_sources_naming_one_library_share_the_fetch() {
        let root = std::env::temp_dir().join(format!("tect-fetch-share.{}", std::process::id()));
        let repository = root.join("library");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&repository).unwrap();
        git(&repository, &["init", "--quiet", "-b", "main"]);
        crate::init::put(
            &repository.join("hello/module.kdl"),
            "schema-version 1\n\ndescription \"Says hello\"\n\nsupports \"fedora\"\n",
        )
        .unwrap();
        commit(&repository, "initial");
        let mut second = git_source(&repository, None);
        second.name = "two".to_string();
        let _ = super::modules(&root, &list_with(&root, git_source(&repository, None))).unwrap();
        let _ = std::fs::remove_dir_all(&repository);
        let said = super::modules(&root, &list_with(&root, second)).unwrap();
        assert!(
            said.iter().any(|line| line.contains("two/hello")),
            "{said:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// A repository named by a relative `--root` fetches the same way. The
    /// archive write runs inside the bare repository, so a work path relative
    /// to the calling shell would land outside it.
    #[test]
    fn a_relative_root_fetches() {
        let root = PathBuf::from(format!("target/fetch-relative.{}", std::process::id()));
        let repository =
            std::env::temp_dir().join(format!("tect-fetch-relative.{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&repository);
        std::fs::create_dir_all(&repository).unwrap();
        git(&repository, &["init", "--quiet", "-b", "main"]);
        crate::init::put(
            &repository.join("hello/module.kdl"),
            "schema-version 1\n\ndescription \"Says hello\"\n\nsupports \"fedora\"\n",
        )
        .unwrap();
        commit(&repository, "initial");
        let tree = crate::import::tree(&root, &git_source(&repository, None)).unwrap();
        assert!(tree.join("hello/module.kdl").is_file());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&repository);
    }
}

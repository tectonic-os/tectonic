//! `fetch components` refreshes each tracked component from the library its lock
//! entry names, as `repo.kdl` declares that library now.

use crate::create::Change;
use crate::layout;
use crate::model::remote::{Collection, Kind as SourceKind};
use crate::provenance::lock::{self, Record};
use std::path::{Path, PathBuf};

/// Refreshes the tracked components at `paths`, or every tracked component if
/// `paths` is empty. The returned tree lines name each component that changed and
/// the lock.
pub fn components(
    root: &Path,
    sources: &[Collection],
    enforce: bool,
    paths: &[&str],
) -> Result<Vec<(PathBuf, Change)>, String> {
    let chosen = chosen(root, paths)?;
    // An update replaces the component's files, so an edit is refused before
    // any component is fetched or written.
    for record in &chosen {
        refuse_edited(root, record)?;
    }
    let mut wrote = Vec::new();
    let mut refreshed = Vec::new();
    for record in chosen {
        let library = sources
            .iter()
            .find(|source| source.kind == SourceKind::Components && source.name == record.library)
            .ok_or_else(|| {
                format!(
                    "`{}` was imported from `{}`, and `sources` in repo.kdl no longer declares \
                     it; declare the library again, or run `tect set component {} --untracked`",
                    shown(&record.path),
                    record.library,
                    record.path
                )
            })?;
        crate::import::verified(library, enforce)?;
        let upstream = crate::import::tree(root, library)?
            .join(library.subtree().unwrap_or(""))
            .join(&record.upstream);
        if !upstream.join(layout::MANIFEST_FILE).is_file() {
            return Err(format!(
                "`{}` tracks `{}` in `{}`, and the library no longer has it",
                shown(&record.path),
                record.upstream,
                record.library
            ));
        }
        let content = lock::hash(&upstream).ok_or_else(|| {
            format!(
                "{} cannot be hashed, so nothing can record what was copied",
                upstream.display()
            )
        })?;
        let url = library.pin().and_then(|pin| pin.url.clone());
        let version = library.pin().and_then(|pin| pin.version.clone());
        let dir = layout::component(root, &record.path);
        if content == record.content
            && url == record.pin.url
            && version == record.pin.version
            && dir.is_dir()
        {
            continue;
        }
        let line = (
            PathBuf::from(layout::COMPONENTS).join(&record.path),
            Change::Updated(match &version {
                Some(version) => format!("refreshed from {} at {version}", record.library),
                None => format!("refreshed from {}", record.library),
            }),
        );
        refreshed.push(record.path.clone());
        let mut pin = crate::provenance::Evidence::new(record.pin.span);
        pin.url = url;
        pin.version = version;
        // The lock is rendered before the copy, so a lock that cannot take
        // the entry refuses before the component changes.
        let locked = lock::with(
            root,
            Record {
                pin,
                content,
                ..record
            },
        )?;
        replace(root, &upstream, &dir)?;
        crate::init::put(&layout::lock(root), &locked)?;
        wrote.push(line);
    }
    if !refreshed.is_empty() {
        wrote.push((
            PathBuf::from(layout::COMPONENTS).join(layout::LOCK_FILE),
            Change::Updated(format!("{} recorded", crate::said(&refreshed))),
        ));
    }
    Ok(wrote)
}

/// Answers the lock entries to refresh. A named path must be a tracked entry,
/// because an untracked copy is the repository's own.
fn chosen(root: &Path, paths: &[&str]) -> Result<Vec<Record>, String> {
    lock::parses(root).map_err(|err| format!("{err}; resolve it, then fetch again"))?;
    let records = lock::read(root);
    if paths.is_empty() {
        return Ok(records
            .into_iter()
            .filter(|record| record.tracked)
            .collect());
    }
    let keys: Vec<&str> = paths.iter().map(|path| lock::key(path)).collect();
    for key in &keys {
        match records.iter().find(|record| record.path == *key) {
            None => {
                return Err(format!(
                    "`{}` has no entry in the lock, so it came from no library",
                    shown(key)
                ))
            }
            Some(record) if !record.tracked => {
                return Err(format!(
                    "`{}` is the repository's own copy, so no library updates it",
                    shown(key)
                ))
            }
            Some(_) => {}
        }
    }
    Ok(records
        .into_iter()
        .filter(|record| keys.contains(&record.path.as_str()))
        .collect())
}

/// Refuses a tracked component whose content no longer matches its entry. A
/// missing directory is restored by the refresh, so it passes.
fn refuse_edited(root: &Path, record: &Record) -> Result<(), String> {
    if !lock::inside(&record.path) {
        return Err(format!(
            "`{}` is not a path under `{}/`",
            record.path,
            layout::COMPONENTS
        ));
    }
    let dir = layout::component(root, &record.path);
    if !dir.exists() || lock::hash(&dir).as_deref() == Some(record.content.as_str()) {
        return Ok(());
    }
    Err(format!(
        "`{}` tracks its library, and its content no longer matches its lock entry, so an \
         update would overwrite the edit; undo the edit, or run `tect set component {} \
         --untracked` to make the copy the repository's own",
        shown(&record.path),
        record.path
    ))
}

/// Replaces the component's own files with the library's. A component that the
/// directory holds of its own has its own entry, so it stays. The library's
/// files are staged first, so a copy that is refused leaves the component whole.
fn replace(root: &Path, upstream: &Path, dir: &Path) -> Result<(), String> {
    let staged = crate::init::Scratch(
        root.join(".cache")
            .join(format!("refresh.{}", std::process::id())),
    );
    crate::init::remove(&staged.0)?;
    crate::init::copy_component(upstream, &staged.0)?;
    let held = layout::held_components(dir);
    for file in crate::tracked(dir) {
        let own = file
            .strip_prefix(dir)
            .is_ok_and(|rel| !held.iter().any(|component| rel.starts_with(component)));
        if own {
            crate::init::remove(&file)?;
        }
    }
    crate::init::copy_component(&staged.0, dir)?;
    Ok(())
}

fn shown(key: &str) -> String {
    format!("{}/{key}", layout::COMPONENTS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::Found;
    use crate::model::remote::At;

    fn library() -> [Collection; 1] {
        [Collection {
            kind: SourceKind::Components,
            name: "one".into(),
            at: At::Dir("library".into()),
            span: crate::diag::Span::default(),
        }]
    }

    /// Writes `files` into the library and imports each named member.
    fn imported(
        name: &str,
        files: &[(&str, &str)],
        members: &[(&str, &str)],
        tracked: bool,
    ) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tect-fetch-{name}-{}", std::process::id()));
        for (path, text) in files {
            crate::init::put(&root.join("library").join(path), text).unwrap();
        }
        for (upstream, key) in members {
            let found = Found {
                owner: "one".into(),
                name: (*upstream).into(),
                dir: root.join("library").join(upstream),
            };
            crate::import::vendor_all(&root, &library(), &[(&found, key)], tracked).unwrap();
        }
        root
    }

    fn read(root: &Path, path: &str) -> Option<String> {
        std::fs::read_to_string(root.join(path)).ok()
    }

    #[test]
    fn an_unmoved_library_leaves_the_tree_untouched() {
        let root = imported(
            "unmoved",
            &[("browser/manifest.kdl", "")],
            &[("browser", "browser")],
            true,
        );
        let lock = read(&root, "components/components.lock.kdl");
        let wrote = components(&root, &library(), false, &[]).unwrap();
        assert!(wrote.is_empty());
        assert_eq!(read(&root, "components/components.lock.kdl"), lock);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_moved_library_replaces_the_files_and_the_hash() {
        let root = imported(
            "moved",
            &[("browser/manifest.kdl", ""), ("browser/old.sh", "true\n")],
            &[("browser", "browser")],
            true,
        );
        std::fs::remove_file(root.join("library/browser/old.sh")).unwrap();
        crate::init::put(&root.join("library/browser/new.sh"), "true\n").unwrap();

        let wrote = components(&root, &library(), false, &[]).unwrap();
        assert_eq!(wrote.len(), 2);
        assert_eq!(read(&root, "components/browser/old.sh"), None);
        assert_eq!(
            read(&root, "components/browser/new.sh").as_deref(),
            Some("true\n")
        );
        let entry = lock::entry(&root, "browser").unwrap();
        assert_eq!(
            Some(entry.content),
            lock::hash(&root.join("library/browser"))
        );
        assert!(entry.tracked);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_edited_tracked_component_refuses_the_update() {
        let root = imported(
            "edited",
            &[("browser/manifest.kdl", "")],
            &[("browser", "browser")],
            true,
        );
        crate::init::put(&root.join("components/browser/mine.sh"), "true\n").unwrap();
        crate::init::put(&root.join("library/browser/new.sh"), "true\n").unwrap();

        let Err(refused) = components(&root, &library(), false, &[]) else {
            panic!("the edit is refused");
        };
        assert!(refused.contains("`components/browser` tracks"), "{refused}");
        assert!(
            refused.contains("tect set component browser --untracked"),
            "{refused}"
        );
        assert_eq!(
            read(&root, "components/browser/mine.sh").as_deref(),
            Some("true\n")
        );
        assert_eq!(read(&root, "components/browser/new.sh"), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_untracked_component_is_passed_over_and_refused_by_name() {
        let root = imported(
            "untracked",
            &[("browser/manifest.kdl", "")],
            &[("browser", "browser")],
            false,
        );
        crate::init::put(&root.join("library/browser/new.sh"), "true\n").unwrap();

        assert!(components(&root, &library(), false, &[])
            .unwrap()
            .is_empty());
        let Err(refused) = components(&root, &library(), false, &["components/browser"]) else {
            panic!("an untracked copy is refused by name");
        };
        assert!(refused.contains("the repository's own copy"), "{refused}");
        assert_eq!(read(&root, "components/browser/new.sh"), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A lock that does not parse reads as empty, and an update that read it
    /// so would pass with nothing refreshed.
    #[test]
    fn a_lock_that_does_not_parse_refuses_the_update() {
        let root = imported(
            "broken",
            &[("browser/manifest.kdl", "")],
            &[("browser", "browser")],
            true,
        );
        crate::init::put(&layout::lock(&root), "component \"browser\" {\n").unwrap();
        let Err(refused) = components(&root, &library(), false, &[]) else {
            panic!("the lock is refused");
        };
        assert!(refused.contains("does not parse"), "{refused}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_key_outside_components_refuses_the_update() {
        let root = imported("climb", &[("browser/manifest.kdl", "")], &[], true);
        crate::init::put(
            &layout::lock(&root),
            "component \"../library/browser\" {\n    library \"one\"\n    path \"browser\"\n    content \"0\"\n    track\n}\n",
        )
        .unwrap();
        let Err(refused) = components(&root, &library(), false, &[]) else {
            panic!("the key is refused");
        };
        assert!(refused.contains("is not a path under"), "{refused}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The other library holds a different `browser`, and it is listed first.
    #[test]
    fn the_refresh_reads_the_library_its_entry_names() {
        let root = imported(
            "two",
            &[("browser/manifest.kdl", "")],
            &[("browser", "browser")],
            true,
        );
        crate::init::put(&root.join("other/browser/manifest.kdl"), "").unwrap();
        crate::init::put(&root.join("other/browser/wrong.sh"), "true\n").unwrap();
        crate::init::put(&root.join("library/browser/right.sh"), "true\n").unwrap();
        let mut sources = vec![Collection {
            kind: SourceKind::Components,
            name: "other".into(),
            at: At::Dir("other".into()),
            span: crate::diag::Span::default(),
        }];
        sources.extend(library());

        components(&root, &sources, false, &[]).unwrap();
        assert!(root.join("components/browser/right.sh").is_file());
        assert!(!root.join("components/browser/wrong.sh").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The entry recorded a fetched library. The library is now a directory,
    /// which has no URL or revision to record.
    #[test]
    fn a_refresh_records_the_pin_the_library_has_now() {
        let root = imported("pin", &[("browser/manifest.kdl", "")], &[], true);
        let mut pin = crate::provenance::Evidence::new(crate::diag::Span::default());
        pin.url = Some("https://host/owner/library.git".into());
        pin.version = Some("v1".into());
        let content = lock::hash(&root.join("library/browser")).unwrap();
        crate::init::put(
            &layout::lock(&root),
            &lock::with(
                &root,
                Record {
                    path: "browser".into(),
                    library: "one".into(),
                    upstream: "browser".into(),
                    pin,
                    content,
                    tracked: true,
                },
            )
            .unwrap(),
        )
        .unwrap();
        crate::init::copy_component(
            &root.join("library/browser"),
            &root.join("components/browser"),
        )
        .unwrap();

        assert_eq!(components(&root, &library(), false, &[]).unwrap().len(), 2);
        let entry = lock::entry(&root, "browser").unwrap();
        assert_eq!(entry.pin.url, None);
        assert_eq!(entry.pin.version, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The link names a file outside the library.
    #[test]
    fn a_library_link_refuses_the_update_and_leaves_the_component_whole() {
        let root = imported(
            "link",
            &[("browser/manifest.kdl", "")],
            &[("browser", "browser")],
            true,
        );
        crate::init::put(&root.join("secret"), "token\n").unwrap();
        std::os::unix::fs::symlink(root.join("secret"), root.join("library/browser/notes"))
            .unwrap();

        let Err(refused) = components(&root, &library(), false, &[]) else {
            panic!("the link is refused");
        };
        assert!(refused.contains("is a symbolic link"), "{refused}");
        assert!(root.join("components/browser/manifest.kdl").is_file());
        assert!(!root.join("components/browser/notes").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_held_component_survives_the_refresh_of_its_parent() {
        let root = imported(
            "held",
            &[
                ("nvidia/manifest.kdl", ""),
                ("nvidia/cuda/manifest.kdl", ""),
            ],
            &[("nvidia", "nvidia"), ("nvidia/cuda", "nvidia/cuda")],
            true,
        );
        crate::init::put(&root.join("library/nvidia/component.sh"), "true\n").unwrap();

        components(&root, &library(), false, &["nvidia"]).unwrap();
        assert_eq!(
            read(&root, "components/nvidia/component.sh").as_deref(),
            Some("true\n")
        );
        assert!(root.join("components/nvidia/cuda/manifest.kdl").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_missing_tracked_component_is_copied_again_under_its_own_key() {
        let root = imported(
            "missing",
            &[("browser/manifest.kdl", "")],
            &[("browser", "browser@1")],
            true,
        );
        std::fs::remove_dir_all(root.join("components/browser@1")).unwrap();

        components(&root, &library(), false, &[]).unwrap();
        assert!(root.join("components/browser@1/manifest.kdl").is_file());
        assert!(!root.join("components/browser").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

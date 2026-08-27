//! files/ overlay collisions.

use crate::diag::{Issue, Issues};
use crate::model::image::{Entry, Image};
use crate::parse::disk::Disk;
use std::collections::{BTreeMap, BTreeSet};

/// Every path an overlay puts in the image, to the modules shipping it, as
/// indices into the image's entries in build order.
pub type Index = BTreeMap<String, Vec<usize>>;

/// Built once and handed to both readers.
pub fn index(image: &Image, disk: &Disk) -> Index {
    let mut shipped: Index = BTreeMap::new();
    for (index, entry) in image.entries.iter().enumerate() {
        let Some(module) = &entry.module else {
            continue;
        };
        for path in disk.overlays.get(&module.dir).into_iter().flatten() {
            shipped.entry(path.clone()).or_default().push(index);
        }
        for key in &module.keys {
            shipped.entry(key.public.clone()).or_default().push(index);
        }
    }
    shipped
}

pub fn check(image: &Image, shipped: &Index, issues: &mut Issues) {
    let entries = &image.entries;
    let mut used: Vec<BTreeSet<&str>> = vec![BTreeSet::new(); entries.len()];

    for (path, owners) in shipped {
        for (position, &later) in owners.iter().enumerate() {
            let Some(module) = &entries[later].module else {
                continue;
            };
            let Some(&earlier) = owners[..position]
                .iter()
                .rev()
                .find(|&&earlier| coinstalled(&entries[earlier], &entries[later]))
            else {
                continue;
            };
            if let Some(decl) = module.overrides.iter().find(|d| &d.name == path) {
                used[later].insert(decl.name.as_str());
                continue;
            }
            issues.push(
                Issue::new(
                    format!(
                        "`{}` overwrites `{path}`, which `{}` also ships",
                        entries[later].path, entries[earlier].path
                    ),
                    &module.src,
                )
                .help(format!(
                    "overlays are copied in build order, so this one wins and the other file never reaches the image. \
                     Rename one of the two, or declare `overrides \"{path}\"` here if replacing it is the point"
                )),
            );
        }
    }

    for (index, entry) in entries.iter().enumerate() {
        let Some(module) = &entry.module else {
            continue;
        };
        for decl in &module.overrides {
            if used[index].contains(decl.name.as_str()) {
                continue;
            }
            issues.push(
                Issue::new(
                    format!(
                        "`{}` overrides `{}`, which no earlier module ships",
                        entry.path, decl.name
                    ),
                    &module.src,
                )
                .at(decl.span, "nothing to replace")
                .help("an override is checked, so it cannot outlive the collision it was added for; drop it, or check the path against what the other module actually ships"),
            );
        }
    }
}

/// The collisions `check` would report that involve one of the modules just
/// added, read off the same resolved index and build order `check` used. An
/// import calls this the moment it has written the tree, so the person is
/// told then rather than on the next `check`, in `check`'s own sentence.
pub fn collisions(image: &Image, shipped: &Index, brought: &BTreeSet<String>) -> Vec<String> {
    let entries = &image.entries;
    let mut out: Vec<String> = Vec::new();
    for (path, owners) in shipped {
        for (position, &later) in owners.iter().enumerate() {
            let Some(module) = &entries[later].module else {
                continue;
            };
            let Some(&earlier) = owners[..position]
                .iter()
                .rev()
                .find(|&&earlier| coinstalled(&entries[earlier], &entries[later]))
            else {
                continue;
            };
            if !brought.contains(&entries[later].dir())
                && !brought.contains(&entries[earlier].dir())
            {
                continue;
            }
            if module.overrides.iter().any(|decl| &decl.name == path) {
                continue;
            }
            out.push(format!(
                "`{}` overwrites `{path}`, which `{}` also ships — declare `overrides \"{path}\"` \
                 in `{}` to take the replacement, or rename one of the two",
                entries[later].path, entries[earlier].path, entries[later].path
            ));
        }
    }
    out
}

/// Two modules land in the same image unless they are gated to different
/// flavours.
fn coinstalled(a: &Entry, b: &Entry) -> bool {
    if a.path == b.path {
        return false;
    }
    match (&a.flavour, &b.flavour) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

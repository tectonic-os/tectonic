//! files/ overlay collisions.

use crate::diag::{Issue, Issues};
use crate::model::image::{Entry, Image};
use crate::parse::disk::Disk;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Every path an overlay puts in the image, to the modules shipping it, as
/// indices into the image's entries in build order.
pub type Index = BTreeMap<String, Vec<usize>>;

/// Built once and handed to both readers.
pub fn index(image: &Image, disk: &Disk, root: &Path) -> Index {
    let mut shipped: Index = BTreeMap::new();
    for (index, entry) in image.entries.iter().enumerate() {
        let Some(module) = &entry.module else {
            continue;
        };
        for path in disk.overlays.get(&module.dir).into_iter().flatten() {
            shipped.entry(path.clone()).or_default().push(index);
        }
        for key in &module.keys {
            if crate::layout::public_key(root, &key.public)
                .metadata()
                .is_ok_and(|meta| meta.len() > 0)
            {
                shipped.entry(key.public.clone()).or_default().push(index);
            }
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

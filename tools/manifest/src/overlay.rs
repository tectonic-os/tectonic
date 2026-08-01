//! files/ overlay collisions.

use crate::diag::{Issue, Issues};
use crate::module::Module;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Every path an overlay puts in the image, to the modules shipping it, as
/// indices into `modules` in build order.
pub type Index = BTreeMap<String, Vec<usize>>;

/// Built once and handed to both readers.
pub fn index(modules: &[Module], root: &Path) -> Index {
    let mut shipped: Index = BTreeMap::new();
    for (index, module) in modules.iter().enumerate() {
        let overlay = root.join("modules").join(&module.dir).join("files");
        for path in overlay_paths(&overlay) {
            shipped.entry(path).or_default().push(index);
        }
    }
    shipped
}

/// The module whose overlay actually puts a file at `path` in a target's
/// image.
pub fn owns(modules: &[Module], shipped: &Index, path: &str, target: Option<&str>) -> String {
    let Some(owners) = shipped.get(path) else {
        return String::new();
    };
    owners
        .iter()
        .rev()
        .find(|&&owner| in_target(&modules[owner], target))
        .map(|&owner| format!("{}\n", modules[owner].path))
        .unwrap_or_default()
}

/// Whether a module lands in a target's image.
fn in_target(module: &Module, target: Option<&str>) -> bool {
    match (&module.flavour, target) {
        (None, _) => true,
        (Some(_), None) => true,
        (Some(gate), Some(target)) => gate == target,
    }
}

pub fn check(modules: &[Module], shipped: &Index, issues: &mut Issues) {
    let mut used: Vec<BTreeSet<&str>> = vec![BTreeSet::new(); modules.len()];

    for (path, owners) in shipped {
        for (position, &later) in owners.iter().enumerate() {
            let Some(&earlier) = owners[..position]
                .iter()
                .rev()
                .find(|&&earlier| coinstalled(&modules[earlier], &modules[later]))
            else {
                continue;
            };
            if let Some(decl) = modules[later].overrides.iter().find(|d| &d.name == path) {
                used[later].insert(decl.name.as_str());
                continue;
            }
            issues.push(
                Issue::new(
                    format!(
                        "`{}` overwrites `{path}`, which `{}` also ships",
                        modules[later].path, modules[earlier].path
                    ),
                    &modules[later].file,
                    &modules[later].text,
                )
                .help(format!(
                    "overlays are copied in build order, so this one wins and the other file never reaches the image. \
                     Rename one of the two, or declare `overrides \"{path}\"` here if replacing it is the point"
                )),
            );
        }
    }

    for (index, module) in modules.iter().enumerate() {
        for decl in &module.overrides {
            if used[index].contains(decl.name.as_str()) {
                continue;
            }
            issues.push(
                Issue::new(
                    format!(
                        "`{}` overrides `{}`, which no earlier module ships",
                        module.path, decl.name
                    ),
                    &module.file,
                    &module.text,
                )
                .at(decl.span, "nothing to replace")
                .help("an override is checked, so it cannot outlive the collision it was added for; drop it, or check the path against what the other module actually ships"),
            );
        }
    }
}

/// Two modules land in the same image unless they are gated to different
/// flavours.
fn coinstalled(a: &Module, b: &Module) -> bool {
    if a.path == b.path {
        return false;
    }
    match (&a.flavour, &b.flavour) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

/// Every file in an overlay, as the absolute path it becomes in the image.
fn overlay_paths(overlay: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut dirs = vec![overlay.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = path.symlink_metadata() else {
                continue;
            };
            if meta.is_dir() {
                dirs.push(path);
            } else if let Ok(rel) = path.strip_prefix(overlay) {
                out.push(format!("/{}", rel.display()));
            }
        }
    }
    out
}

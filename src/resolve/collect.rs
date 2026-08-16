//! Where each contribution is staged, and what gets assembled from them.

use crate::diag::{Issue, Issues};
use crate::layout;
use crate::model::image::Image;
use crate::model::module::Module;
use crate::parse::disk::Disk;
use std::collections::BTreeMap;
use std::path::Path;

/// One contribution, as the file in the contributor's directory, the path its
/// layer stages that file at, and the destination it is assembled into.
pub struct Collected {
    pub file: String,
    pub staged: String,
    pub into: String,
}

/// Every contribution, and every destination assembled from them.
#[derive(Default)]
pub struct Collection {
    /// Contributor module path to what its layer stages.
    pub by_module: BTreeMap<String, Vec<Collected>>,
    /// Every declared destination, sorted so two runs on the same tree emit
    /// the same plan.
    pub destinations: Vec<String>,
    /// Each destination and the staged parts it is assembled from, ordered by
    /// the priority each contribution declared rather than by build order.
    pub assembled: BTreeMap<String, Vec<String>>,
}

pub fn resolve_collects(
    image: &Image,
    root: &Path,
    disk: &Disk,
    issues: &mut Issues,
) -> Collection {
    let mut by_file: BTreeMap<&str, &Module> = BTreeMap::new();
    for module in image.modules() {
        for collect in &module.collects {
            if let Some(first) = by_file.get(collect.file.as_str()) {
                issues.push(
                    Issue::new(
                        format!("two enabled modules collect `{}`", collect.file),
                        &module.src,
                    )
                    .at(collect.span, "collected again here")
                    .help(format!("already collected by `{}`", first.path)),
                );
            } else {
                by_file.insert(collect.file.as_str(), module);
            }
        }
    }

    let mut out = Collection {
        destinations: by_file
            .values()
            .flat_map(|collector| collector.collects.iter().map(|c| c.into.clone()))
            .collect(),
        ..Collection::default()
    };
    out.destinations.sort();
    out.destinations.dedup();

    for module in image.modules() {
        let dir = layout::module(root, &module.dir);
        for (file, collector) in &disk.collectors {
            if !dir.join(file).is_file() {
                continue;
            }
            match by_file.get(file.as_str()) {
                Some(enabled) => {
                    let declared = enabled.collects.iter().find(|c| &c.file == file);
                    let into = declared.map(|c| c.into.clone()).unwrap_or_default();
                    let priority = module
                        .contributes
                        .iter()
                        .find(|c| &c.file == file)
                        .map(|c| c.priority)
                        .or_else(|| declared.map(|c| c.priority))
                        .unwrap_or_default();
                    if !module.standard_layer {
                        issues.push(
                            Issue::new(
                                format!(
                                    "`{}` ships a {file} with no standard layer to collect it from",
                                    module.path
                                ),
                                &module.src,
                            )
                            .help(format!(
                                "`standard-layer #false` makes the fragment the whole layer, so it has to append the file to {into} itself"
                            )),
                        );
                        continue;
                    }
                    out.by_module
                        .entry(module.path.clone())
                        .or_default()
                        .push(Collected {
                            file: file.clone(),
                            staged: staged(&into, priority, &module.path),
                            into,
                        });
                }
                None => issues.push(
                    Issue::new(
                        format!(
                            "`{}` ships a {file} but nothing enabled collects it",
                            module.path
                        ),
                        &module.src,
                    )
                    .help(format!(
                        "`{collector}` collects it; add it to this image, or drop the {file}"
                    )),
                ),
            }
        }
    }

    let mut assembled: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for collected in out.by_module.values().flatten() {
        assembled
            .entry(collected.into.clone())
            .or_default()
            .push(collected.staged.clone());
    }
    // The staged name carries the declared priority, so sorting it is the
    // declared order and nothing downstream has to know the build order.
    for parts in assembled.values_mut() {
        parts.sort();
    }
    out.assembled = assembled;
    out
}

/// Where one contribution is staged: `<into>.d/NNNN-<module>.part`.
fn staged(into: &str, priority: u32, module: &str) -> String {
    format!("{into}.d/{priority:04}-{}.part", module.replace('/', "-"))
}

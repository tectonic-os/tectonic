//! Where each contribution is staged, and what gets assembled from them.

use crate::diag::{Issue, Issues};
use crate::layout;
use crate::model::component::Component;
use crate::model::image::Image;
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
    /// Contributor component path to what its layer stages.
    pub by_component: BTreeMap<String, Vec<Collected>>,
    /// Every declared destination, sorted so two runs on the same tree emit
    /// the same plan.
    pub destinations: Vec<String>,
    /// Each destination and the staged parts it is assembled from, ordered by
    /// the priority each contribution declared.
    pub assembled: BTreeMap<String, Vec<String>>,
}

pub fn resolve_collects(
    image: &Image,
    root: &Path,
    disk: &Disk,
    issues: &mut Issues,
) -> Collection {
    let mut by_file: BTreeMap<&str, &Component> = BTreeMap::new();
    for component in image.components() {
        for collect in &component.collects {
            if let Some(first) = by_file.get(collect.file.as_str()) {
                issues.push(
                    Issue::new(
                        format!("two enabled components collect `{}`", collect.file),
                        &component.src,
                    )
                    .at(collect.span, "collected again here")
                    .help(format!("already collected by `{}`", first.path)),
                );
            } else {
                by_file.insert(collect.file.as_str(), component);
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

    // A contribution is a file the component ships, so it gates the way the rest
    // of them do. One file lands per component and the collector claims one name,
    // so this picks the most specific copy.
    let family = image.base.as_ref().map_or("", |base| base.family.as_str());
    for component in image.components() {
        let dir = layout::component(root, &component.dir);
        for (file, collector) in &disk.collectors {
            let Some(shipped) = layout::shipped(&dir, family, file) else {
                continue;
            };
            match by_file.get(file.as_str()) {
                Some(enabled) => {
                    let declared = enabled.collects.iter().find(|c| &c.file == file);
                    let into = declared.map(|c| c.into.clone()).unwrap_or_default();
                    let priority = component
                        .contributes
                        .iter()
                        .find(|c| &c.file == file)
                        .map(|c| c.priority)
                        .or_else(|| declared.map(|c| c.priority))
                        .unwrap_or_default();
                    if !component.standard_layer {
                        issues.push(
                            Issue::new(
                                format!(
                                    "`{}` ships a {file} with no standard layer to collect it from",
                                    component.path
                                ),
                                &component.src,
                            )
                            .help(format!(
                                "`standard-layer #false` makes the fragment the whole layer, so it has to append the file to {into} itself"
                            )),
                        );
                        continue;
                    }
                    out.by_component
                        .entry(component.path.clone())
                        .or_default()
                        .push(Collected {
                            file: shipped,
                            staged: staged(&into, priority, &component.path),
                            into,
                        });
                }
                // An enabled collector whose `collects` was refused has
                // already reported it, and adding it to the image fixes
                // nothing.
                None if image.components().any(|enabled| enabled.dir == *collector) => {}
                None => issues.push(
                    Issue::new(
                        format!(
                            "`{}` ships a {file} but nothing enabled collects it",
                            component.path
                        ),
                        &component.src,
                    )
                    .help(format!(
                        "`{collector}` collects it; add it to this image, or drop the {file}"
                    )),
                ),
            }
        }
    }

    let mut assembled: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for collected in out.by_component.values().flatten() {
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

/// Where one contribution is staged: `<into>.d/NNNN-<component>.part`.
fn staged(into: &str, priority: u32, component: &str) -> String {
    format!(
        "{into}.d/{priority:04}-{}.part",
        component.replace('/', "-")
    )
}

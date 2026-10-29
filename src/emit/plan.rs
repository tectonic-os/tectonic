//! One resolved JSON plan holds every fact that downstream commands derive.

use crate::model::asset::Asset;
use crate::model::component::Component;
use crate::model::image::{Entry, Image, List, Target, NO_FLAVOUR};
use crate::resolve::overlay;
use crate::resolve::workflow::Declared;
use crate::resolve::Resolved;
use common::json::Json;

/// The image one target names, the flavour it gates on, and the entries that
/// land in its build. None when nothing publishes under that name.
pub(crate) fn of_target<'a>(
    list: &'a List,
    name: &str,
) -> Option<(&'a Image, Option<String>, Vec<&'a Entry>)> {
    let target = list.targets().into_iter().find(|t| t.to_string() == name)?;
    let image = list.images.iter().find(|i| i.id == target.image)?;
    let entries = image
        .entries
        .iter()
        .filter(|entry| in_target(entry, &target.flavour))
        .collect();
    let flavour = match target.flavour.as_str() {
        NO_FLAVOUR => None,
        flavour => Some(flavour.to_string()),
    };
    Some((image, flavour, entries))
}

/// The shape of this document. Additive fields stay in version 1 while the
/// software has no external consumers; bump this only for an incompatible
/// shape. `model::image::SCHEMA_VERSION` separately governs `repo.kdl` grammar
/// through `parse::repo::compatible`.
pub const SCHEMA_VERSION: u32 = 1;

pub fn build(list: &List, resolved: &[Resolved], workflows: &[Declared]) -> Json {
    Json::object([
        ("schema_version", Json::number(SCHEMA_VERSION)),
        (
            "default_image",
            Json::optional(list.default_image().map(|i| i.id.clone())),
        ),
        (
            "default_image_name",
            Json::optional(list.default_image().map(|i| i.name.clone())),
        ),
        (
            "pr_image",
            Json::optional(list.pr_image().map(|i| i.id.clone())),
        ),
        (
            "default_target",
            Json::optional(list.default_target().map(|t| t.to_string())),
        ),
        (
            "default_published",
            Json::optional(list.default_target().map(|t| t.published())),
        ),
        (
            "pr_target",
            Json::optional(list.pr_target().map(|t| t.to_string())),
        ),
        (
            "pr_published",
            Json::optional(list.pr_target().map(|t| t.published())),
        ),
        (
            "ungated_target",
            Json::optional(list.ungated_target().map(|t| t.to_string())),
        ),
        (
            "ungated_published",
            Json::optional(list.ungated_target().map(|t| t.published())),
        ),
        ("cache_image", Json::optional(list.cache_image())),
        (
            "workflows",
            Json::array(workflows.iter().map(|declared| {
                Json::object([
                    ("file", Json::string(&declared.file)),
                    ("schedule", Json::optional(declared.schedule.clone())),
                ])
            })),
        ),
        (
            "audit",
            Json::object([("enforce", Json::Bool(list.audit_enforce))]),
        ),
        ("assets", every_asset(list)),
        (
            "images",
            Json::array(
                list.images
                    .iter()
                    .zip(resolved)
                    .map(|(image, resolved)| self::image(list, image, resolved)),
            ),
        ),
    ])
}

/// The image file, named from the repository root. `plan.json` is baked into
/// the image, so the path in it cannot depend on where `generate` was run.
fn declared_in(src: &str) -> String {
    format!(
        "./{}",
        std::path::Path::new(src)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    )
}

fn image(list: &List, image: &Image, resolved: &Resolved) -> Json {
    Json::object([
        ("id", Json::string(&image.id)),
        ("name", Json::string(&image.name)),
        ("pretty_name", Json::string(&image.pretty_name)),
        ("file", Json::string(declared_in(image.src.name()))),
        ("url", Json::string(&image.url)),
        ("issues_url", Json::string(&image.issues_url)),
        ("description", Json::string(&image.description)),
        ("keywords", Json::strings(image.keywords.iter().cloned())),
        ("logo_url", Json::string(&image.logo_url)),
        ("conforms", Json::string(&image.conforms)),
        ("boot", Json::string(&image.boot)),
        ("allows", refusals(&image.allows)),
        (
            "layout",
            match &image.layout {
                None => Json::Null,
                Some(layout) => Json::object([
                    ("filesystem", Json::string(&layout.filesystem)),
                    ("bootloader", Json::string(&layout.bootloader)),
                ]),
            },
        ),
        (
            "base",
            match &image.base {
                None => Json::Null,
                Some(base) => Json::object([
                    ("path", Json::string(&base.path)),
                    (
                        "defined",
                        match image
                            .base_entry
                            .as_ref()
                            .and_then(|held| held.entry.inline.as_ref())
                        {
                            Some(_) => Json::string(declared_in(image.src.name())),
                            None => Json::Null,
                        },
                    ),
                    ("image", Json::string(&base.image)),
                    ("family", Json::string(&base.family)),
                    ("scap_content", Json::string(&base.scap_content)),
                    ("signed", Json::Bool(base.signed)),
                    (
                        "provides",
                        Json::strings(base.shipped().map(|decl| decl.name.clone())),
                    ),
                    ("witnesses", witnesses(image)),
                    (
                        "requires",
                        Json::strings(base.requires.iter().map(|d| d.name.clone())),
                    ),
                    (
                        "satisfies",
                        Json::array(base.satisfies.iter().map(|coverage| {
                            Json::object([
                                ("benchmark", Json::string(&coverage.benchmark)),
                                ("rules", Json::strings(&coverage.rules)),
                            ])
                        })),
                    ),
                ]),
            },
        ),
        (
            "flavours",
            Json::array(image.flavours.iter().map(|flavour| {
                Json::object([
                    ("name", Json::string(&flavour.name)),
                    ("default", Json::Bool(flavour.default)),
                    ("pr_build", Json::Bool(flavour.pr_build)),
                ])
            })),
        ),
        (
            "default_flavour",
            Json::optional(image.default_flavour().map(str::to_string)),
        ),
        (
            "targets",
            Json::array(
                list.targets()
                    .iter()
                    .filter(|t| t.image == image.id)
                    .map(|t| target(list, image, resolved, t)),
            ),
        ),
    ])
}

fn target(list: &List, image: &Image, resolved: &Resolved, target: &Target) -> Json {
    let flavour = target.flavour.as_str();
    let family = image.base.as_ref().map_or("", |base| base.family.as_str());
    let entries: Vec<&Entry> = image
        .entries
        .iter()
        .filter(|entry| in_target(entry, flavour))
        .collect();
    let components: Vec<&Component> = entries
        .iter()
        .filter_map(|e| e.component.as_ref())
        .collect();

    Json::object([
        ("name", Json::string(target.to_string())),
        ("image", Json::string(&target.image)),
        (
            "flavour",
            match target.flavour.as_str() {
                NO_FLAVOUR => Json::Null,
                flavour => Json::string(flavour),
            },
        ),
        ("published", Json::string(target.published())),
        ("conforms", Json::string(image.conforms_of(flavour))),
        ("allows", refusals(&image.allows)),
        (
            "default",
            Json::Bool(
                list.default_target()
                    .is_some_and(|d| d.to_string() == target.to_string()),
            ),
        ),
        (
            "pr",
            Json::Bool(
                list.pr_target()
                    .is_some_and(|p| p.to_string() == target.to_string()),
            ),
        ),
        (
            "siblings",
            Json::array(
                list.targets()
                    .iter()
                    .filter(|other| other.image == target.image)
                    .filter(|other| other.flavour != target.flavour)
                    .map(|other| {
                        Json::object([
                            ("name", Json::string(other.to_string())),
                            ("published", Json::string(other.published())),
                        ])
                    }),
            ),
        ),
        (
            "components",
            Json::array(
                entries
                    .iter()
                    .map(|entry| self::component(entry, family, image, &entries)),
            ),
        ),
        (
            "suppressed",
            Json::array(
                image
                    .suppressed
                    .iter()
                    .filter(|entry| in_target(entry, flavour))
                    .map(|entry| self::component(entry, family, image, &entries)),
            ),
        ),
        (
            "secrets",
            unique(&components, |m| {
                m.secrets.iter().map(|d| d.name.clone()).collect()
            }),
        ),
        (
            "contract_files",
            Json::strings(contract_files(image, &components)),
        ),
        (
            "verify_exceptions",
            Json::array(
                unique_pairs(&components, |m| {
                    m.verify_exceptions
                        .iter()
                        .map(|e| (e.class.clone(), e.unit.clone()))
                        .collect()
                })
                .into_iter()
                .map(|(class, unit)| {
                    Json::object([("class", Json::string(class)), ("unit", Json::string(unit))])
                }),
            ),
        ),
        ("assets", assets(&components)),
        (
            "overlay_files",
            overlay_files(image, &resolved.shipped, flavour),
        ),
        (
            "overlay_overridden",
            overridden(image, &resolved.shipped, flavour),
        ),
        (
            "collected_files",
            Json::map(entries.iter().filter_map(|entry| {
                resolved
                    .collected
                    .by_component
                    .get(&entry.path)
                    .map(|staged| {
                        (
                            entry.path.clone(),
                            Json::array(staged.iter().map(|c| {
                                Json::object([
                                    ("file", Json::string(&c.file)),
                                    ("staged", Json::string(&c.staged)),
                                ])
                            })),
                        )
                    })
            })),
        ),
        (
            "collect_destinations",
            Json::strings(resolved.collected.destinations.clone()),
        ),
    ])
}

/// Says how each requirement of one entry was met on this target. It names
/// the component that met it.
fn met_by(entry: &Entry, image: &Image, entries: &[&Entry]) -> Json {
    let Some(component) = &entry.component else {
        return Json::map(std::iter::empty::<(String, Json)>());
    };
    Json::map(component.requires.iter().filter_map(|decl| {
        if image.base_answers(&decl.name) {
            return Some((
                decl.name.clone(),
                Json::object([("how", Json::string("base"))]),
            ));
        }
        let provider = entries.iter().find(|other| {
            other
                .component
                .as_ref()
                .is_some_and(|component| component.answers(&decl.name))
        })?;
        Some((
            decl.name.clone(),
            match &provider.imported_by {
                None => Json::object([
                    ("how", Json::string("listed")),
                    ("provider", Json::string(&provider.path)),
                ]),
                Some(by) => Json::object([
                    ("how", Json::string("imported")),
                    ("provider", Json::string(&provider.path)),
                    ("by", Json::string(by)),
                ]),
            },
        ))
    }))
}

fn component(entry: &Entry, family: &str, image: &Image, entries: &[&Entry]) -> Json {
    let component = entry.component.as_ref();
    Json::object([
        ("path", Json::string(&entry.path)),
        ("flavour", Json::optional(entry.flavour.clone())),
        ("variant", Json::optional(entry.variant.clone())),
        (
            "description",
            Json::string(
                component
                    .map(|m| m.description.as_str())
                    .unwrap_or_default(),
            ),
        ),
        (
            "options",
            Json::map(
                component
                    .map(|m| m.resolved.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|(name, value)| (name.clone(), Json::string(value))),
            ),
        ),
        ("provides", Json::strings(decls(component, |m| &m.provides))),
        ("requires", Json::strings(decls(component, |m| &m.requires))),
        ("met_by", met_by(entry, image, entries)),
        ("removes", Json::strings(decls(component, |m| &m.removes))),
        (
            "files",
            Json::map(
                component
                    .map(|m| m.files.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|f| (f.name.clone(), Json::string(&f.file))),
            ),
        ),
        (
            "packages",
            batches(component.map(|m| m.packages.as_slice()), family),
        ),
        (
            "groups",
            batches(component.map(|m| m.groups.as_slice()), family),
        ),
        ("coprs", coprs(component, family)),
        ("provenance", provenance(entry)),
        (
            "origin",
            match &entry.imported_by {
                None => Json::object([("how", Json::string("listed"))]),
                Some(by) => {
                    Json::object([("how", Json::string("imported")), ("by", Json::string(by))])
                }
            },
        ),
        (
            "satisfies",
            Json::array(
                component
                    .map(|m| m.satisfies.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|coverage| {
                        Json::object([
                            ("benchmark", Json::string(&coverage.benchmark)),
                            ("rules", Json::strings(&coverage.rules)),
                        ])
                    }),
            ),
        ),
        (
            "refuses",
            refusals(component.map(|m| m.refuses.as_slice()).unwrap_or_default()),
        ),
    ])
}

/// Refusals and allowances share this shape because renderers read them
/// together.
fn refusals(declared: &[crate::model::component::Refusal]) -> Json {
    Json::array(declared.iter().map(|refusal| {
        Json::object([
            ("rule", Json::string(&refusal.rule)),
            ("because", Json::string(&refusal.because)),
        ])
    }))
}

/// Only the target family's package groups can reach its build layer.
fn batches(declared: Option<&[crate::model::component::PackageGroup]>, family: &str) -> Json {
    Json::array(
        declared
            .unwrap_or_default()
            .iter()
            .filter(|batch| batch.family == family)
            .map(|batch| {
                Json::object([
                    ("family", Json::string(&batch.family)),
                    ("names", Json::strings(batch.packages.iter().cloned())),
                    ("enablerepo", Json::optional(batch.enablerepo.clone())),
                ])
            }),
    )
}

/// The COPR repositories one component enables, which only a Fedora base can
/// reach. Every name is derived from the two declared segments.
fn coprs(component: Option<&Component>, family: &str) -> Json {
    if family != "fedora" {
        return Json::array(std::iter::empty());
    }
    Json::array(
        component
            .map(|m| m.coprs.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|copr| {
                Json::object([
                    ("id", Json::string(copr.name())),
                    ("url", Json::string(copr.url())),
                ])
            }),
    )
}

/// An unresolved manifest contributes no capability declarations.
fn decls(
    component: Option<&Component>,
    of: impl Fn(&Component) -> &Vec<crate::model::component::Decl>,
) -> Vec<String> {
    component
        .map(|m| of(m).iter().map(|d| d.name.clone()).collect())
        .unwrap_or_default()
}

/// Keeping content and import evidence together lets a consumer tell an
/// imported copy from a component the repository owns. An inline definition
/// has no directory to hash, so `defined` names the image file that holds it.
fn provenance(entry: &Entry) -> Json {
    let component = entry.component.as_ref();
    Json::object([
        (
            "defined",
            match (&entry.inline, component) {
                (Some(_), Some(m)) => Json::string(declared_in(m.src.name())),
                _ => Json::Null,
            },
        ),
        (
            "content",
            Json::optional(component.and_then(|m| m.content.clone())),
        ),
        ("repo", Json::Bool(component.is_some_and(|m| m.repo))),
        (
            "imported",
            match component.and_then(|m| m.imported.as_ref()) {
                None => Json::Null,
                Some(record) => Json::object([
                    ("library", Json::string(&record.library)),
                    ("path", Json::string(&record.upstream)),
                    ("content", Json::string(&record.content)),
                    ("pin", record.pin.json()),
                    ("tracked", Json::Bool(record.tracked)),
                ]),
            },
        ),
    ])
}

/// A name is witnessed only by a provider's `witness=`. Each witness is one layer
/// probe even when the base and a component each declare one. A removed name's
/// probe carries a leading `!`, and `validate-image` requires each of its
/// paths to be absent.
pub(crate) fn contract_files(image: &Image, components: &[&Component]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |paths: Vec<String>, removed: bool| {
        let token = match removed {
            true => format!("!{}", paths.join("|")),
            false => paths.join("|"),
        };
        if !out.contains(&token) {
            out.push(token);
        }
    };
    let removed = |name: &str| components.iter().any(|component| component.withdraws(name));
    let base = image.base.iter().flat_map(|base| &base.files);
    for file in base.chain(components.iter().flat_map(|component| &component.files)) {
        if !file.build_only {
            add(vec![file.file.clone()], removed(&file.name));
        }
    }
    out
}

pub(crate) fn provides(image: &Image, entries: &[&Entry], name: &str) -> bool {
    let mut components = entries.iter().filter_map(|entry| entry.component.as_ref());
    !components
        .clone()
        .any(|component| component.withdraws(name))
        && (image.base_answers(name) || components.any(|component| component.answers(name)))
}

/// The base probe measures each name that a provider in the image witnesses.
/// A name with no `witness=` has nothing to probe.
/// A component that meets a base requirement stands in for the base on purpose.
/// A component that the base imports is part of the base component.
/// The probe must never ask the base to claim what either component provides.
fn witnesses(image: &Image) -> Json {
    let brought = brought(image);
    let stands_in = |component: &&crate::model::component::Component| {
        brought.contains(component.path.as_str())
            || image.base.iter().any(|base| {
                base.requires.iter().any(|decl| {
                    crate::resolve::answers(
                        &component.path,
                        component.provides.iter().map(|has| has.name.as_str()),
                        &decl.name,
                    )
                })
            })
    };
    let components = image
        .entries
        .iter()
        .chain(&image.suppressed)
        .filter_map(|entry| entry.component.as_ref())
        .filter(|component| !stands_in(component));
    let files = image
        .base
        .iter()
        .flat_map(|base| &base.files)
        .chain(components.flat_map(|component| &component.files));
    let mut paths: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
    for file in files {
        let held = paths.entry(&file.name).or_default();
        if !held.contains(&file.file) {
            held.push(file.file.clone());
        }
    }
    Json::map(
        paths
            .into_iter()
            .map(|(name, paths)| (name.to_string(), Json::strings(paths))),
    )
}

/// Lists the path of each component that the base imports, directly or through
/// another import. An entry's `imported_by` names only its first importer, so
/// the walk follows the manifests' `imports` instead. An import that the base
/// covers is suppressed, and its own imports still belong to the base.
fn brought(image: &Image) -> std::collections::BTreeSet<&str> {
    let mut held = std::collections::BTreeSet::new();
    let mut todo: Vec<&str> = image
        .base
        .iter()
        .flat_map(|base| &base.imports)
        .map(|import| import.path.as_str())
        .collect();
    while let Some(path) = todo.pop() {
        if !held.insert(path) {
            continue;
        }
        let component = image
            .entries
            .iter()
            .chain(&image.suppressed)
            .find(|entry| entry.path == path)
            .and_then(|entry| entry.component.as_ref());
        if let Some(component) = component {
            todo.extend(component.imports.iter().map(|import| import.path.as_str()));
        }
    }
    held
}

fn overlay_files(image: &Image, shipped: &overlay::Index, flavour: &str) -> Json {
    Json::map(shipped.iter().filter_map(|(path, owners)| {
        owners
            .iter()
            .rev()
            .map(|&owner| &image.entries[owner])
            .find(|owner| in_target(owner, flavour))
            .map(|owner| (path.clone(), Json::string(&owner.path)))
    }))
}

/// Every path a component ships that another component's overlay replaced, to the
/// component whose copy the image actually carries. A claim about a file its
/// claimant no longer owns is a composition failure, and this is what tells it
/// from a false claim.
fn overridden(image: &Image, shipped: &overlay::Index, flavour: &str) -> Json {
    Json::array(
        overrides(image, shipped, flavour)
            .into_iter()
            .map(|(path, component, by)| {
                Json::object([
                    ("path", Json::string(path)),
                    ("component", Json::string(component)),
                    ("by", Json::string(by)),
                ])
            }),
    )
}

pub(crate) fn overrides(
    image: &Image,
    shipped: &overlay::Index,
    flavour: &str,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (path, owners) in shipped {
        let here: Vec<usize> = owners
            .iter()
            .copied()
            .filter(|&owner| in_target(&image.entries[owner], flavour))
            .collect();
        let Some((&winner, losers)) = here.split_last() else {
            continue;
        };
        for &loser in losers {
            out.push((
                path.clone(),
                image.entries[loser].path.clone(),
                image.entries[winner].path.clone(),
            ));
        }
    }
    out
}

/// Whether any component in the image ships a preset. A shipped preset is what
/// makes the generated `COMPONENT_PRESETS` argument needed.
pub(crate) fn declares_presets(shipped: &overlay::Index) -> bool {
    shipped.keys().any(|path| is_preset(path))
}

/// The layer verifies only presets delivered by this target's overlays.
pub(crate) fn preset_files(image: &Image, shipped: &overlay::Index, flavour: &str) -> Vec<String> {
    shipped
        .keys()
        .filter(|path| is_preset(path))
        .filter(|path| {
            shipped[*path]
                .iter()
                .any(|&owner| in_target(&image.entries[owner], flavour))
        })
        .cloned()
        .collect()
}

fn is_preset(path: &str) -> bool {
    let Some(name) = path
        .strip_prefix("/usr/lib/systemd/system-preset/")
        .or_else(|| path.strip_prefix("/usr/lib/systemd/user-preset/"))
    else {
        return false;
    };
    name.starts_with(crate::runtime::COMPONENT_PRESET) && name.ends_with(".preset")
}

/// A component-level key prevents the same asset from reaching the build twice.
pub(crate) fn pinned<'a>(components: &[&'a Component]) -> Vec<(&'a Component, &'a Asset)> {
    let mut seen: Vec<(&str, &str)> = Vec::new();
    let mut out = Vec::new();
    for component in components {
        for asset in &component.assets {
            if seen.contains(&(component.path.as_str(), asset.name.as_str())) {
                continue;
            }
            seen.push((component.path.as_str(), asset.name.as_str()));
            out.push((*component, asset));
        }
    }
    out
}

fn assets(components: &[&Component]) -> Json {
    Json::array(pinned(components).into_iter().map(|(component, asset)| {
        Json::object([
            ("component", Json::string(&component.path)),
            ("name", Json::string(&asset.name)),
            (
                "manifest",
                Json::string(format!("components/{}/manifest.kdl", component.dir)),
            ),
            ("pin", asset.pin.json()),
        ])
    }))
}

fn every_asset(list: &List) -> Json {
    let components: Vec<&Component> = list.images.iter().flat_map(Image::components).collect();
    assets(&components)
}

/// `NO_FLAVOUR` must identify the ungated target because an absent gate would
/// silently admit gated entries.
pub(crate) fn in_target(entry: &Entry, target: &str) -> bool {
    match &entry.flavour {
        None => true,
        Some(gate) => gate == target,
    }
}

fn unique(components: &[&Component], of: impl Fn(&Component) -> Vec<String>) -> Json {
    Json::strings(distinct(
        components.iter().flat_map(|component| of(component)),
    ))
}

pub(crate) fn unique_pairs(
    components: &[&Component],
    of: impl Fn(&Component) -> Vec<(String, String)>,
) -> Vec<(String, String)> {
    distinct(components.iter().flat_map(|component| of(component)))
}

/// Preserving resolved build order keeps derived lists aligned with components.
pub(crate) fn distinct<T: PartialEq>(values: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for value in values {
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Span;

    fn entry(flavour: Option<&str>) -> Entry {
        Entry {
            path: "owner/component".into(),
            flavour: flavour.map(str::to_string),
            variant: None,
            options: Vec::new(),
            span: Span::default(),
            component: None,
            imported_by: None,
            inline: None,
            organises: false,
        }
    }

    /// The backend-only build arguments have no golden, so this test owns
    /// ungated filtering.
    #[test]
    fn a_gated_entry_is_out_of_the_ungated_target() {
        assert!(in_target(&entry(None), NO_FLAVOUR));
        assert!(in_target(&entry(Some("dx")), "dx"));
        assert!(!in_target(&entry(Some("dx")), NO_FLAVOUR));
        assert!(!in_target(&entry(Some("dx")), "laptop"));
    }

    /// Every golden runs from the repository root, so nothing there sees a
    /// path `--root` or a subdirectory would have produced.
    #[test]
    fn an_image_is_named_from_the_repository_root() {
        assert_eq!(declared_in("./image.kdl"), "./image.kdl");
        assert_eq!(declared_in("../server.image.kdl"), "./server.image.kdl");
        assert_eq!(
            declared_in("/tmp/repo/server.image.kdl"),
            "./server.image.kdl"
        );
    }
}

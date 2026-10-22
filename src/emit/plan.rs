//! One resolved JSON plan holds every fact that downstream commands derive.

use crate::model::asset::Asset;
use crate::model::image::{Entry, Image, List, Target, NO_FLAVOUR};
use crate::model::module::Module;
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
                    ("image", Json::string(&base.image)),
                    ("family", Json::string(&base.family)),
                    ("scap_content", Json::string(&base.scap_content)),
                    ("signed", Json::Bool(base.signed)),
                    (
                        "provides",
                        Json::strings(base.provides.iter().map(|d| d.name.clone())),
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
    let modules: Vec<&Module> = entries.iter().filter_map(|e| e.module.as_ref()).collect();

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
            "modules",
            Json::array(entries.iter().map(|entry| self::module(entry, family))),
        ),
        (
            "suppressed",
            Json::array(
                image
                    .suppressed
                    .iter()
                    .filter(|entry| in_target(entry, flavour))
                    .map(|entry| self::module(entry, family)),
            ),
        ),
        (
            "secrets",
            unique(&modules, |m| {
                m.secrets.iter().map(|d| d.name.clone()).collect()
            }),
        ),
        (
            "contract_files",
            Json::strings(contract_files(image, &modules)),
        ),
        (
            "verify_exceptions",
            Json::array(
                unique_pairs(&modules, |m| {
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
        ("assets", assets(&modules)),
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
                resolved.collected.by_module.get(&entry.path).map(|staged| {
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

fn module(entry: &Entry, family: &str) -> Json {
    let module = entry.module.as_ref();
    Json::object([
        ("path", Json::string(&entry.path)),
        ("flavour", Json::optional(entry.flavour.clone())),
        ("variant", Json::optional(entry.variant.clone())),
        (
            "description",
            Json::string(module.map(|m| m.description.as_str()).unwrap_or_default()),
        ),
        (
            "options",
            Json::map(
                module
                    .map(|m| m.resolved.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|(name, value)| (name.clone(), Json::string(value))),
            ),
        ),
        ("provides", Json::strings(decls(module, |m| &m.provides))),
        ("requires", Json::strings(decls(module, |m| &m.requires))),
        (
            "files",
            Json::map(
                module
                    .map(|m| m.files.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|f| (f.name.clone(), Json::string(&f.file))),
            ),
        ),
        (
            "packages",
            batches(module.map(|m| m.packages.as_slice()), family),
        ),
        (
            "groups",
            batches(module.map(|m| m.groups.as_slice()), family),
        ),
        ("coprs", coprs(module, family)),
        ("provenance", provenance(entry)),
        (
            "satisfies",
            Json::array(
                module
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
            refusals(module.map(|m| m.refuses.as_slice()).unwrap_or_default()),
        ),
    ])
}

/// Refusals and allowances share this shape because renderers read them
/// together.
fn refusals(declared: &[crate::model::module::Refusal]) -> Json {
    Json::array(declared.iter().map(|refusal| {
        Json::object([
            ("rule", Json::string(&refusal.rule)),
            ("because", Json::string(&refusal.because)),
        ])
    }))
}

/// Only the target family's package groups can reach its build layer.
fn batches(declared: Option<&[crate::model::module::PackageGroup]>, family: &str) -> Json {
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

/// The COPR repositories one module enables, which only a Fedora base can
/// reach. Every name is derived from the two declared segments.
fn coprs(module: Option<&Module>, family: &str) -> Json {
    if family != "fedora" {
        return Json::array(std::iter::empty());
    }
    Json::array(
        module
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
    module: Option<&Module>,
    of: impl Fn(&Module) -> &Vec<crate::model::module::Decl>,
) -> Vec<String> {
    module
        .map(|m| of(m).iter().map(|d| d.name.clone()).collect())
        .unwrap_or_default()
}

/// Keeping content and import evidence together lets a consumer tell an
/// imported copy from a module the repository owns.
fn provenance(entry: &Entry) -> Json {
    let module = entry.module.as_ref();
    Json::object([
        (
            "content",
            Json::optional(module.and_then(|m| m.content.clone())),
        ),
        ("repo", Json::Bool(module.is_some_and(|m| m.repo))),
        (
            "imported",
            match module.and_then(|m| m.imported.as_ref()) {
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

/// Each capability stays one layer probe even when base, module and table
/// paths offer alternatives.
pub(crate) fn contract_files(image: &Image, modules: &[&Module]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |paths: Vec<String>| {
        let token = paths.join("|");
        if !out.contains(&token) {
            out.push(token);
        }
    };
    for decl in image.base.iter().flat_map(|b| b.provides.iter()) {
        if !image.boot.is_empty() && decl.name == "bootupctl" {
            continue;
        }
        if let Some(paths) = crate::base::witness(&decl.name, None) {
            add(paths);
        }
    }
    for module in modules {
        for decl in &module.provides {
            let own = module.files.iter().find(|f| f.name == decl.name);
            if own.is_some_and(|file| file.build_only) {
                continue;
            }
            let own = own.map(|file| file.file.as_str());
            if let Some(paths) = crate::base::witness(&decl.name, own) {
                add(paths);
            }
        }
    }
    out
}

pub(crate) fn provides(image: &Image, entries: &[&Entry], name: &str) -> bool {
    image
        .base
        .iter()
        .flat_map(|base| &base.provides)
        .chain(
            entries
                .iter()
                .filter_map(|entry| entry.module.as_ref())
                .flat_map(|module| &module.provides),
        )
        .any(|decl| decl.name == name)
}

/// Only located capabilities can produce filesystem probes. Abstract rows must
/// leave the probe result unchanged.
fn witnesses(image: &Image) -> Json {
    let mut names: Vec<&str> = crate::base::CAPABILITIES
        .iter()
        .filter(|row| row.path.is_some())
        .map(|row| row.name)
        .chain(
            image
                .base
                .iter()
                .flat_map(|b| b.provides.iter().map(|d| d.name.as_str())),
        )
        .chain(
            image
                .entries
                .iter()
                .chain(&image.suppressed)
                .filter_map(|entry| entry.module.as_ref())
                .flat_map(|m| m.requires.iter().map(|d| d.name.as_str())),
        )
        .collect();
    names.sort_unstable();
    names.dedup();
    Json::map(names.into_iter().filter_map(|name| {
        let paths = crate::base::probe(name)?;
        Some((name.to_string(), Json::strings(paths)))
    }))
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

/// Every path a module ships that another module's overlay replaced, to the
/// module whose copy the image actually carries. A claim about a file its
/// claimant no longer owns is a composition failure, and this is what tells it
/// from a false claim.
fn overridden(image: &Image, shipped: &overlay::Index, flavour: &str) -> Json {
    Json::array(
        overrides(image, shipped, flavour)
            .into_iter()
            .map(|(path, module, by)| {
                Json::object([
                    ("path", Json::string(path)),
                    ("module", Json::string(module)),
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
    name.starts_with(crate::runtime::MODULE_PRESET) && name.ends_with(".preset")
}

/// A module-level key prevents the same asset from reaching the build twice.
pub(crate) fn pinned<'a>(modules: &[&'a Module]) -> Vec<(&'a Module, &'a Asset)> {
    let mut seen: Vec<(&str, &str)> = Vec::new();
    let mut out = Vec::new();
    for module in modules {
        for asset in &module.assets {
            if seen.contains(&(module.path.as_str(), asset.name.as_str())) {
                continue;
            }
            seen.push((module.path.as_str(), asset.name.as_str()));
            out.push((*module, asset));
        }
    }
    out
}

fn assets(modules: &[&Module]) -> Json {
    Json::array(pinned(modules).into_iter().map(|(module, asset)| {
        Json::object([
            ("module", Json::string(&module.path)),
            ("name", Json::string(&asset.name)),
            (
                "manifest",
                Json::string(format!("modules/{}/module.kdl", module.dir)),
            ),
            ("pin", asset.pin.json()),
        ])
    }))
}

fn every_asset(list: &List) -> Json {
    let modules: Vec<&Module> = list.images.iter().flat_map(Image::modules).collect();
    assets(&modules)
}

/// `NO_FLAVOUR` must identify the ungated target because an absent gate would
/// silently admit gated entries.
pub(crate) fn in_target(entry: &Entry, target: &str) -> bool {
    match &entry.flavour {
        None => true,
        Some(gate) => gate == target,
    }
}

fn unique(modules: &[&Module], of: impl Fn(&Module) -> Vec<String>) -> Json {
    Json::strings(distinct(modules.iter().flat_map(|module| of(module))))
}

pub(crate) fn unique_pairs(
    modules: &[&Module],
    of: impl Fn(&Module) -> Vec<(String, String)>,
) -> Vec<(String, String)> {
    distinct(modules.iter().flat_map(|module| of(module)))
}

/// Preserving resolved build order keeps derived lists aligned with modules.
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
            path: "owner/module".into(),
            flavour: flavour.map(str::to_string),
            variant: None,
            options: Vec::new(),
            span: Span::default(),
            module: None,
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

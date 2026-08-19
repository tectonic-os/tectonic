//! One resolved plan, as JSON: every fact anything downstream derives.

use crate::emit::json::Json;
use crate::model::asset::Asset;
use crate::model::image::{Entry, Image, List, Target, NO_FLAVOUR, SCHEMA_VERSION};
use crate::model::module::Module;
use crate::provenance::Evidence;
use crate::resolve::overlay;
use crate::resolve::Resolved;

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

pub fn build(list: &List, resolved: &[Resolved], workflows: &[(String, bool)]) -> Json {
    Json::object([
        (
            "schema_version",
            Json::Number(list.schema_version.unwrap_or(SCHEMA_VERSION)),
        ),
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
            Json::array(workflows.iter().map(|(file, enabled)| {
                Json::object([
                    ("file", Json::string(file)),
                    ("enabled", Json::Bool(*enabled)),
                ])
            })),
        ),
        (
            "audit",
            Json::object([("enforce", Json::Bool(list.audit_enforce))]),
        ),
        ("remotes", remotes(list)),
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
        (
            "base",
            match &image.base {
                None => Json::Null,
                Some(base) => Json::object([
                    ("image", Json::string(&base.image)),
                    ("family", Json::string(&base.family)),
                    ("signed", Json::Bool(base.signed)),
                    (
                        "provides",
                        Json::strings(base.provides.iter().map(|d| d.name.clone())),
                    ),
                    (
                        "provides_files",
                        Json::strings(base.provides_files.iter().map(|d| d.name.clone())),
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

/// What one target is made of.
fn target(list: &List, image: &Image, resolved: &Resolved, target: &Target) -> Json {
    let flavour = target.flavour.as_str();
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
            Json::array(entries.iter().map(|entry| self::module(entry))),
        ),
        (
            "suppressed",
            Json::array(
                image
                    .suppressed
                    .iter()
                    .filter(|entry| in_target(entry, flavour))
                    .map(self::module),
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
        ("provides_files", provides_files(&modules)),
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

fn module(entry: &Entry) -> Json {
    let module = entry.module.as_ref();
    Json::object([
        ("path", Json::string(&entry.path)),
        ("dir", Json::string(entry.dir())),
        ("flavour", Json::optional(entry.flavour.clone())),
        ("variant", Json::optional(entry.variant.clone())),
        (
            "remote",
            Json::optional(entry.remote.as_ref().and_then(|r| r.version.clone())),
        ),
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
    ])
}

/// One list of capability names off a module, empty where the manifest did not
/// load.
fn decls(
    module: Option<&Module>,
    of: impl Fn(&Module) -> &Vec<crate::model::module::Decl>,
) -> Vec<String> {
    module
        .map(|m| of(m).iter().map(|d| d.name.clone()).collect())
        .unwrap_or_default()
}

/// Where this module came from: the content hash `verify` covers, the pin an
/// out-of-tree module is fetched at, and the record an import left inside it.
fn provenance(entry: &Entry) -> Json {
    let module = entry.module.as_ref();
    Json::object([
        (
            "content",
            Json::optional(module.and_then(|m| m.content.clone())),
        ),
        ("repo", Json::Bool(module.is_some_and(|m| m.repo))),
        (
            "pin",
            entry.remote.as_ref().map_or(Json::Null, Evidence::json),
        ),
        (
            "imported",
            match module.and_then(|m| m.imported.as_ref()) {
                None => Json::Null,
                Some(record) => Json::object([
                    ("collection", Json::string(&record.collection)),
                    ("content", Json::string(&record.content)),
                    ("pin", record.pin.json()),
                ]),
            },
        ),
    ])
}

/// Contract file paths the finished image still carries: what the base
/// guarantees, then what the enabled modules declare.
pub(crate) fn contract_files(image: &Image, modules: &[&Module]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for decl in image.base.iter().flat_map(|b| b.provides_files.iter()) {
        if !out.contains(&decl.name) {
            out.push(decl.name.clone());
        }
    }
    for module in modules {
        for decl in &module.provides_files {
            if module.provides_files_build_only.contains(&decl.name) {
                continue;
            }
            if !out.contains(&decl.name) {
                out.push(decl.name.clone());
            }
        }
    }
    out
}

/// Every contract path an enabled module declares, to the module that declares
/// it.
fn provides_files(modules: &[&Module]) -> Json {
    let mut out: Vec<(String, Json)> = Vec::new();
    for module in modules {
        for decl in &module.provides_files {
            if out.iter().any(|(path, _)| path == &decl.name) {
                continue;
            }
            out.push((decl.name.clone(), Json::string(&module.path)));
        }
    }
    Json::map(out)
}

/// Every path a files/ overlay puts in this target's image, to the module it
/// comes from.
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
/// claimant no longer owns is a composition failure rather than a false claim,
/// and this is what tells the two apart.
fn overridden(image: &Image, shipped: &overlay::Index, flavour: &str) -> Json {
    let mut out: Vec<Json> = Vec::new();
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
            out.push(Json::object([
                ("path", Json::string(path)),
                ("module", Json::string(&image.entries[loser].path)),
                ("by", Json::string(&image.entries[winner].path)),
            ]));
        }
    }
    Json::Array(out)
}

/// The preset files the target's overlays put in the image, which is what the
/// layer checks arrived.
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

/// Every pinned asset the given modules declare, deduplicated by module and
/// name.
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

/// Every asset in the repository, whatever image or flavour it is behind.
fn every_asset(list: &List) -> Json {
    let modules: Vec<&Module> = list.images.iter().flat_map(Image::modules).collect();
    assets(&modules)
}

/// Every out-of-tree pin, across every image, because there is one fetch
/// directory: a module two images pin is one tree on disk, fetched once.
fn remotes(list: &List) -> Json {
    let mut seen: Vec<&str> = Vec::new();
    let mut out: Vec<Json> = Vec::new();
    for image in &list.images {
        for entry in &image.entries {
            let Some(remote) = &entry.remote else {
                continue;
            };
            if seen.contains(&entry.path.as_str()) {
                continue;
            }
            seen.push(&entry.path);
            out.push(Json::object([
                ("name", Json::string(&entry.path)),
                ("dir", Json::string(format!("modules/{}", entry.dir()))),
                ("file", Json::string(declared_in(image.src.name()))),
                ("pin", remote.json()),
            ]));
        }
    }
    Json::Array(out)
}

/// Whether an entry lands in a target's image. The ungated target is
/// `NO_FLAVOUR`, never an absent one: a caller with nothing to compare against
/// would silently take the gated entries too.
pub(crate) fn in_target(entry: &Entry, target: &str) -> bool {
    match &entry.flavour {
        None => true,
        Some(gate) => gate == target,
    }
}

fn unique(modules: &[&Module], of: impl Fn(&Module) -> Vec<String>) -> Json {
    let mut out: Vec<String> = Vec::new();
    for module in modules {
        for name in of(module) {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    Json::strings(out)
}

pub(crate) fn unique_pairs(
    modules: &[&Module],
    of: impl Fn(&Module) -> Vec<(String, String)>,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for module in modules {
        for pair in of(module) {
            if !out.contains(&pair) {
                out.push(pair);
            }
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
            remote: None,
            span: Span::default(),
            module: None,
        }
    }

    /// The ungated target is a flavour name like any other, and a gated entry
    /// is not in it. Nothing golden covers this: the build args are the only
    /// consumer, and they are handed to a backend rather than written.
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
        assert_eq!(declared_in("/tmp/repo/server.image.kdl"), "./server.image.kdl");
    }
}

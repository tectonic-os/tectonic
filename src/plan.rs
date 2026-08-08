//! One resolved plan, as JSON: every fact anything downstream derives.

use crate::json::Json;
use crate::list::{Entry, Image, List, Target, NO_FLAVOUR, SCHEMA_VERSION};
use crate::module::{Collection, Module};
use crate::overlay;

/// One image, resolved: the manifests its entries name, loaded and checked
/// together, and the two indexes built while doing it.
pub struct Resolved {
    pub modules: Vec<Module>,
    pub shipped: overlay::Index,
    pub collected: Collection,
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
        ("remotes", remotes(list)),
        ("assets", every_asset(resolved)),
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

fn image(list: &List, image: &Image, resolved: &Resolved) -> Json {
    Json::object([
        ("id", Json::string(&image.id)),
        ("name", Json::string(&image.name)),
        ("pretty_name", Json::string(&image.pretty_name)),
        ("file", Json::string(&image.file)),
        ("url", Json::string(&image.url)),
        ("issues_url", Json::string(&image.issues_url)),
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
    let flavour = Some(target.flavour.as_str());
    let entries: Vec<(&Entry, Option<&Module>)> = image
        .entries
        .iter()
        .filter(|entry| in_target(entry, flavour))
        .map(|entry| {
            let module = resolved
                .modules
                .iter()
                .find(|m| m.path == entry.path && m.flavour == entry.flavour);
            (entry, module)
        })
        .collect();
    let modules: Vec<&Module> = entries.iter().filter_map(|(_, module)| *module).collect();

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
            Json::Bool(list.default_target().is_some_and(|d| d.to_string() == target.to_string())),
        ),
        (
            "pr",
            Json::Bool(list.pr_target().is_some_and(|p| p.to_string() == target.to_string())),
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
            Json::array(entries.iter().map(|(entry, module)| self::module(entry, *module))),
        ),
        ("secrets", unique(&modules, |m| {
            m.secrets.iter().map(|d| d.name.clone()).collect()
        })),
        ("contract_files", contract_files(image, &modules)),
        (
            "verify_exceptions",
            Json::array(unique_pairs(&modules, |m| {
                m.verify_exceptions
                    .iter()
                    .map(|e| (e.class.clone(), e.unit.clone()))
                    .collect()
            })
            .into_iter()
            .map(|(class, unit)| {
                Json::object([("class", Json::string(class)), ("unit", Json::string(unit))])
            })),
        ),
        ("assets", assets(&modules)),
        ("provides_files", provides_files(&modules)),
        ("overlay_files", overlay_files(resolved, flavour)),
        (
            "collected_files",
            Json::map(entries.iter().filter_map(|(entry, _)| {
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

fn module(entry: &Entry, module: Option<&Module>) -> Json {
    Json::object([
        ("path", Json::string(&entry.path)),
        ("dir", Json::string(entry.dir())),
        ("flavour", Json::optional(entry.flavour.clone())),
        ("variant", Json::optional(entry.variant.clone())),
        (
            "remote",
            Json::optional(entry.remote.as_ref().map(|r| r.git_ref.clone())),
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
    ])
}

/// Contract file paths the finished image still carries: what the base
/// guarantees, then what the enabled modules declare.
fn contract_files(image: &Image, modules: &[&Module]) -> Json {
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
    Json::strings(out)
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
fn overlay_files(resolved: &Resolved, flavour: Option<&str>) -> Json {
    Json::map(resolved.shipped.iter().filter_map(|(path, owners)| {
        owners
            .iter()
            .rev()
            .map(|&owner| &resolved.modules[owner])
            .find(|owner| match (&owner.flavour, flavour) {
                (None, _) => true,
                (Some(_), None) => true,
                (Some(gate), Some(flavour)) => gate == flavour,
            })
            .map(|owner| (path.clone(), Json::string(&owner.path)))
    }))
}

/// Every pinned asset the given modules declare, deduplicated by module and
/// name.
fn assets(modules: &[&Module]) -> Json {
    let mut seen: Vec<(&str, &str)> = Vec::new();
    let mut out: Vec<Json> = Vec::new();
    for module in modules {
        for asset in &module.assets {
            if seen.contains(&(module.path.as_str(), asset.name.as_str())) {
                continue;
            }
            seen.push((module.path.as_str(), asset.name.as_str()));
            out.push(Json::object([
                ("module", Json::string(&module.path)),
                ("name", Json::string(&asset.name)),
                (
                    "manifest",
                    Json::string(format!("modules/{}/module.kdl", module.dir)),
                ),
                ("version", Json::optional(asset.version.clone())),
                ("sha256", Json::optional(asset.sha256.clone())),
                ("from", Json::string(asset.from.as_str())),
                ("url", Json::optional(asset.url_resolved())),
            ]));
        }
    }
    Json::Array(out)
}

/// Every asset in the repository, whatever image or flavour it is behind.
fn every_asset(resolved: &[Resolved]) -> Json {
    let modules: Vec<&Module> = resolved.iter().flat_map(|r| r.modules.iter()).collect();
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
                ("ref", Json::string(&remote.git_ref)),
                ("sha256", Json::string(&remote.sha256)),
                ("url", Json::string(remote.url_resolved())),
                ("path", Json::optional(remote.path.clone())),
                ("file", Json::string(&image.file)),
            ]));
        }
    }
    Json::Array(out)
}

/// Whether an entry lands in a target's image.
fn in_target(entry: &Entry, target: Option<&str>) -> bool {
    match (&entry.flavour, target) {
        (None, _) => true,
        (Some(_), None) => true,
        (Some(gate), Some(target)) => gate == target,
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

fn unique_pairs(
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

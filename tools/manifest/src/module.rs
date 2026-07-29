//! module.kdl: the module author's file.

use crate::diag::{Issue, Issues};
use crate::list::{Entry, List};
use crate::options::{self, Opt, Variant};
use kdl::{KdlDocument, KdlNode};
use miette::SourceSpan;
use std::collections::BTreeMap;
use std::path::Path;

/// A capability or contract path, and where it was declared.
pub struct Decl {
    pub name: String,
    pub span: SourceSpan,
}

/// An aggregated file: the consuming module says where it lands and which
/// filename feeds it.
pub struct Sink {
    pub name: String,
    pub file: String,
    pub path: String,
    pub span: SourceSpan,
}

pub struct Module {
    /// The list path, which is the module's identity everywhere.
    #[allow(dead_code)]
    pub path: String,
    #[allow(dead_code)]
    pub file: String,
    #[allow(dead_code)]
    pub text: String,
    pub description: String,
    pub supports: Vec<String>,
    /// Capabilities.
    pub provides: Vec<Decl>,
    pub requires: Vec<Decl>,
    /// Soft: ordering and cache preference, never fails.
    pub after: Vec<Decl>,
    /// Exact paths one module writes and another reads.
    pub provides_files: Vec<Decl>,
    pub requires_files: Vec<Decl>,
    /// The flavour this module is gated to, from the list rather than the
    /// manifest: a module never names a flavour.
    pub flavour: Option<String>,
    pub sinks: Vec<Sink>,
    pub options: Vec<Opt>,
    pub variants: Vec<Variant>,
    /// Resolved option name to value, ready to become env on the layer.
    pub resolved: Vec<(String, String)>,
}

/// The only base family today.
const FAMILIES: [&str; 1] = ["fedora"];

fn prop<'a>(node: &'a KdlNode, key: &str) -> Option<&'a str> {
    node.entries()
        .iter()
        .find(|e| e.name().map(|n| n.value()) == Some(key))
        .and_then(|e| e.value().as_string())
}

/// The first unnamed entry of a node, as a string.
fn string_args(node: &KdlNode) -> Vec<&str> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .filter_map(|e| e.value().as_string())
        .collect()
}

impl Module {
    pub fn load(entry: &Entry, list: &List, root: &Path, issues: &mut Issues) -> Option<Self> {
        let dir = root.join("modules").join(&entry.path);
        let path = dir.join("module.kdl");
        let file = path.display().to_string();

        let Ok(text) = std::fs::read_to_string(&path) else {
            issues.push(
                Issue::new(
                    format!("`{}` has no module.kdl", entry.path),
                    &list.file,
                    &list.text,
                )
                .at(entry.span, "every module needs a manifest")
                .help(format!(
                    "create {file}; modules/_template/module-name/module.kdl is a copy-me reference"
                )),
            );
            return None;
        };

        let doc: KdlDocument = match text.parse() {
            Ok(doc) => doc,
            Err(err) => {
                eprintln!("{:?}", miette::Report::new(err));
                issues.push(Issue::new(format!("{file} is not valid KDL"), &file, &text));
                return None;
            }
        };

        let mut module = Module {
            path: entry.path.clone(),
            description: String::new(),
            supports: Vec::new(),
            provides: Vec::new(),
            requires: Vec::new(),
            after: Vec::new(),
            provides_files: Vec::new(),
            requires_files: Vec::new(),
            flavour: entry.flavour.clone(),
            sinks: Vec::new(),
            options: Vec::new(),
            variants: Vec::new(),
            resolved: Vec::new(),
            file: file.clone(),
            text: text.clone(),
        };

        for node in doc.nodes() {
            match node.name().value() {
                "description" => match string_args(node).first() {
                    Some(d) if !d.is_empty() => module.description = d.to_string(),
                    _ => issues.push(
                        Issue::new("`description` needs a string", &file, &text)
                            .at(node.name().span(), "no description given"),
                    ),
                },
                "supports" => {
                    for family in string_args(node) {
                        if !FAMILIES.contains(&family) {
                            issues.push(
                                Issue::new(format!("unknown base family `{family}`"), &file, &text)
                                    .at(
                                        node.name().span(),
                                        "not a family this repository builds on",
                                    )
                                    .help(format!("known families: {}", FAMILIES.join(", "))),
                            );
                        }
                        module.supports.push(family.to_string());
                    }
                }
                kind @ ("provides" | "requires" | "after") => {
                    let decls = string_args(node)
                        .iter()
                        .map(|c| Decl {
                            name: c.to_string(),
                            span: node.name().span(),
                        })
                        .collect::<Vec<_>>();
                    if decls.is_empty() {
                        issues.push(
                            Issue::new(format!("`{kind}` needs a capability name"), &file, &text)
                                .at(node.name().span(), "nothing named"),
                        );
                    }
                    match kind {
                        "provides" => module.provides.extend(decls),
                        "requires" => module.requires.extend(decls),
                        _ => module.after.extend(decls),
                    }
                }
                kind @ ("provides-file" | "requires-file") => {
                    for path in string_args(node) {
                        if !path.starts_with('/') {
                            issues.push(
                                Issue::new(
                                    format!("`{path}` is not an absolute path"),
                                    &file,
                                    &text,
                                )
                                .at(
                                    node.name().span(),
                                    "a contract file is an exact path in the image",
                                ),
                            );
                        }
                        let decl = Decl {
                            name: path.to_string(),
                            span: node.name().span(),
                        };
                        if kind == "provides-file" {
                            module.provides_files.push(decl);
                        } else {
                            module.requires_files.push(decl);
                        }
                    }
                }
                "sink" => {
                    let sink_name = string_args(node).first().map(|s| s.to_string());
                    let sink_file = prop(node, "file");
                    let sink_path = prop(node, "path");
                    let missing = if sink_name.is_none() {
                        Some("a name")
                    } else if sink_file.is_none() {
                        Some("file=, the filename a contributing module ships")
                    } else if sink_path.is_none() {
                        Some("path=, where it lands in the image")
                    } else if !sink_path.is_some_and(|p| p.starts_with('/')) {
                        Some("an absolute path=")
                    } else {
                        None
                    };
                    match missing {
                        None => module.sinks.push(Sink {
                            name: sink_name.unwrap_or_default(),
                            file: sink_file.unwrap_or_default().to_string(),
                            path: sink_path.unwrap_or_default().to_string(),
                            span: node.name().span(),
                        }),
                        Some(missing) => issues.push(
                            Issue::new(format!("`sink` needs {missing}"), &file, &text)
                                .at(node.name().span(), "incomplete sink")
                                .help("`sink \"justfile\" file=\"justfile.inc\" path=\"/usr/share/goojust/justfile.apps\"`"),
                        ),
                    }
                }
                "option" => {
                    if let Some(opt) = options::parse_option(node, &file, &text, issues) {
                        if module.options.iter().any(|o| o.name == opt.name) {
                            issues.push(
                                Issue::new(
                                    format!("option `{}` is declared twice", opt.name),
                                    &file,
                                    &text,
                                )
                                .at(opt.span, "already declared above"),
                            );
                        } else {
                            module.options.push(opt);
                        }
                    }
                }
                "variant" => {
                    if let Some(variant) = options::parse_variant(node, &file, &text, issues) {
                        if module.variants.iter().any(|v| v.name == variant.name) {
                            issues.push(
                                Issue::new(
                                    format!("variant `{}` is declared twice", variant.name),
                                    &file,
                                    &text,
                                )
                                .at(variant.span, "already declared above"),
                            );
                        } else {
                            module.variants.push(variant);
                        }
                    }
                }
                other => issues.push(
                    Issue::new(format!("unknown node `{other}`"), &file, &text)
                        .at(node.name().span(), "not part of the schema")
                        .help("modules/SCHEMA.md documents every node a manifest may hold"),
                ),
            }
        }

        if module.description.is_empty() {
            issues.push(
                Issue::new(format!("`{}` declares no description", entry.path), &file, &text)
                    .help("one line, present tense, no trailing period; it names the module in the resolved build summary"),
            );
        }
        if module.supports.is_empty() {
            issues.push(
                Issue::new(format!("`{}` declares no `supports`", entry.path), &file, &text)
                    .help("a module has to say which base families it can build on, so a portability gap surfaces at lint rather than mid-build"),
            );
        }

        module.resolved = options::resolve(
            &module.options,
            &module.variants,
            &file,
            &text,
            entry,
            list,
            issues,
        );

        Some(module)
    }
}

/// Every module on disk, whether or not the list enables it.
fn providers_on_disk(root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let modules = root.join("modules");
    let mut dirs = vec![modules.clone()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.file_name().is_some_and(|n| n == "_template") {
                continue;
            }
            let manifest = path.join("module.kdl");
            if manifest.is_file() {
                let Ok(text) = std::fs::read_to_string(&manifest) else {
                    continue;
                };
                let Ok(doc) = text.parse::<KdlDocument>() else {
                    continue;
                };
                let name = path
                    .strip_prefix(&modules)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                for node in doc.nodes() {
                    if matches!(node.name().value(), "provides" | "provides-file") {
                        for cap in string_args(node) {
                            out.entry(cap.to_string()).or_default().push(name.clone());
                        }
                    }
                }
            } else {
                dirs.push(path);
            }
        }
    }
    out
}

/// Single pass over the resolved graph.
pub fn check_graph(modules: &[Module], root: &Path, issues: &mut Issues) {
    let mut offered: BTreeMap<&str, Vec<(usize, &Module)>> = BTreeMap::new();
    for (index, module) in modules.iter().enumerate() {
        for decl in module.provides.iter().chain(module.provides_files.iter()) {
            offered
                .entry(decl.name.as_str())
                .or_default()
                .push((index, module));
        }
    }

    for (capability, providers) in &offered {
        if providers.len() > 1 {
            let names: Vec<&str> = providers.iter().map(|(_, m)| m.path.as_str()).collect();
            let (_, first) = providers[0];
            issues.push(
                Issue::new(
                    format!("`{capability}` is provided by more than one enabled module"),
                    &first.file,
                    &first.text,
                )
                .at(
                    first.provides.iter().chain(first.provides_files.iter())
                        .find(|d| d.name == **capability)
                        .map(|d| d.span)
                        .unwrap_or_else(|| (0usize, 0usize).into()),
                    "also provided elsewhere",
                )
                .help(format!(
                    "provided by: {}. Enable one provider, so that what satisfies a requirement is never ambiguous",
                    names.join(", ")
                )),
            );
        }
    }

    let on_disk = providers_on_disk(root);

    for (index, module) in modules.iter().enumerate() {
        let hard = module
            .requires
            .iter()
            .map(|d| (d, "requires"))
            .chain(module.requires_files.iter().map(|d| (d, "requires-file")));

        for (decl, kind) in hard {
            let Some(providers) = offered.get(decl.name.as_str()) else {
                let help = match on_disk.get(&decl.name) {
                    Some(candidates) => format!(
                        "{} would satisfy it; add it to modules.kdl. Nothing is included automatically, so the list stays the complete statement of what is in the image",
                        candidates.join(" or ")
                    ),
                    None => format!(
                        "no module in the repository declares `provides {:?}`",
                        decl.name
                    ),
                };
                issues.push(
                    Issue::new(
                        format!(
                            "`{}` {kind} `{}`, which nothing enabled provides",
                            module.path, decl.name
                        ),
                        &module.file,
                        &module.text,
                    )
                    .at(decl.span, "unsatisfied")
                    .help(help),
                );
                continue;
            };

            if let Some((provider_index, provider)) = providers.first() {
                if *provider_index > index {
                    issues.push(
                        Issue::new(
                            format!(
                                "`{}` {kind} `{}`, which `{}` provides further down the list",
                                module.path, decl.name, provider.path
                            ),
                            &module.file,
                            &module.text,
                        )
                        .at(decl.span, "provided too late to be usable")
                        .help("a requirement implies ordering: move the provider above the module that needs it"),
                    );
                    continue;
                }

                if let Some(provider_flavour) = &provider.flavour {
                    if module.flavour.as_ref() != Some(provider_flavour) {
                        issues.push(
                            Issue::new(
                                format!(
                                    "`{}` {kind} `{}`, which only `{}` provides and only on the `{provider_flavour}` flavour",
                                    module.path, decl.name, provider.path
                                ),
                                &module.file,
                                &module.text,
                            )
                            .at(decl.span, "unsatisfied on every other target")
                            .help("either gate this module to the same flavour, or move the provider out of the flavour block"),
                        );
                    }
                }
            }
        }
    }
}

/// Every sink declared anywhere on disk, as filename to the module that owns
/// it, so a contribution whose consumer is not in the list can name what to
/// enable rather than just being dropped.
fn sinks_on_disk(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let modules = root.join("modules");
    let mut dirs = vec![modules.clone()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || path.file_name().is_some_and(|n| n == "_template") {
                continue;
            }
            let manifest = path.join("module.kdl");
            if !manifest.is_file() {
                dirs.push(path);
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&manifest) else {
                continue;
            };
            let Ok(doc) = text.parse::<KdlDocument>() else {
                continue;
            };
            let name = path
                .strip_prefix(&modules)
                .unwrap_or(&path)
                .display()
                .to_string();
            for node in doc.nodes().iter().filter(|n| n.name().value() == "sink") {
                if let Some(file) = prop(node, "file") {
                    out.insert(file.to_string(), name.clone());
                }
            }
        }
    }
    out
}

/// Which files each module contributes, and where each lands.
pub fn resolve_sinks(
    modules: &[Module],
    root: &Path,
    issues: &mut Issues,
) -> BTreeMap<String, Vec<(String, String)>> {
    let mut by_name: BTreeMap<&str, &Module> = BTreeMap::new();
    let mut by_file: BTreeMap<&str, &Module> = BTreeMap::new();
    for module in modules {
        for sink in &module.sinks {
            for (key, map, what) in [
                (sink.name.as_str(), &mut by_name, "name"),
                (sink.file.as_str(), &mut by_file, "filename"),
            ] {
                if let Some(first) = map.get(key) {
                    issues.push(
                        Issue::new(
                            format!("two enabled modules declare a sink with the {what} `{key}`"),
                            &module.file,
                            &module.text,
                        )
                        .at(sink.span, "declared again here")
                        .help(format!("already declared by `{}`", first.path)),
                    );
                } else {
                    map.insert(key, module);
                }
            }
        }
    }

    let on_disk = sinks_on_disk(root);
    let mut out: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

    for module in modules {
        let dir = root.join("modules").join(&module.path);
        for (file, owner) in &on_disk {
            if !dir.join(file).is_file() {
                continue;
            }
            match by_file.get(file.as_str()) {
                Some(sink_owner) => {
                    let path = sink_owner
                        .sinks
                        .iter()
                        .find(|s| &s.file == file)
                        .map(|s| s.path.clone())
                        .unwrap_or_default();
                    out.entry(module.path.clone())
                        .or_default()
                        .push((file.clone(), path));
                }
                None => issues.push(
                    Issue::new(
                        format!(
                            "`{}` ships a {file} but nothing enabled aggregates it",
                            module.path
                        ),
                        &module.file,
                        &module.text,
                    )
                    .help(format!(
                        "`{owner}` declares that sink; add it to modules.kdl, or drop the {file}"
                    )),
                ),
            }
        }
    }
    out
}

//! module.kdl: the module author's file.

use crate::asset::{self, Asset};
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

/// A filename this module collects from every other module that ships one, and
/// where the build puts them.
pub struct Collect {
    pub file: String,
    pub into: String,
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
    /// Paths this module's files/ overlay knowingly replaces.
    pub overrides: Vec<Decl>,
    /// The flavour this module is gated to, from the list rather than the
    /// manifest: a module never names a flavour.
    pub flavour: Option<String>,
    pub collects: Vec<Collect>,
    /// Build inputs the field sets cover, so that needing a secret or a build
    /// arg does not force a module to hand-write a whole RUN block.
    pub secrets: Vec<Decl>,
    pub args: Vec<Decl>,
    pub options: Vec<Opt>,
    pub variants: Vec<Variant>,
    /// Pinned upstream payloads, resolved into env on the layer.
    pub assets: Vec<Asset>,
    /// Resolved option name to value, ready to become env on the layer.
    pub resolved: Vec<(String, String)>,
    /// Where a Containerfile.inc goes relative to the generated block, and
    /// whether that block is emitted at all.
    pub fragment_after: bool,
    pub standard_layer: bool,
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
            overrides: Vec::new(),
            flavour: entry.flavour.clone(),
            collects: Vec::new(),
            secrets: Vec::new(),
            args: Vec::new(),
            options: Vec::new(),
            variants: Vec::new(),
            assets: Vec::new(),
            resolved: Vec::new(),
            fragment_after: false,
            standard_layer: true,
            file: file.clone(),
            text: text.clone(),
        };

        let mut fragment_span: Option<SourceSpan> = None;
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
                kind @ ("provides-file" | "requires-file" | "overrides") => {
                    for path in string_args(node) {
                        if !path.starts_with('/') {
                            issues.push(
                                Issue::new(
                                    format!("`{path}` is not an absolute path"),
                                    &file,
                                    &text,
                                )
                                .at(node.name().span(), "an exact path in the image"),
                            );
                        }
                        let decl = Decl {
                            name: path.to_string(),
                            span: node.name().span(),
                        };
                        match kind {
                            "provides-file" => module.provides_files.push(decl),
                            "requires-file" => module.requires_files.push(decl),
                            _ => module.overrides.push(decl),
                        }
                    }
                }
                kind @ ("secret" | "arg") => {
                    let names = string_args(node);
                    if names.is_empty() {
                        issues.push(
                            Issue::new(format!("`{kind}` needs a name"), &file, &text)
                                .at(node.name().span(), "nothing named"),
                        );
                    }
                    for name in names {
                        let decl = Decl {
                            name: name.to_string(),
                            span: node.name().span(),
                        };
                        if kind == "secret" {
                            module.secrets.push(decl);
                        } else {
                            module.args.push(decl);
                        }
                    }
                }
                "collects" => {
                    let collected = string_args(node).first().map(|s| s.to_string());
                    let into = prop(node, "into");
                    match (collected, into) {
                        (Some(collected), Some(into)) if into.starts_with('/') => {
                            module.collects.push(Collect {
                                file: collected,
                                into: into.to_string(),
                                span: node.name().span(),
                            })
                        }
                        (collected, into) => {
                            let missing = if collected.is_none() {
                                "the filename it collects"
                            } else if into.is_none() {
                                "into=, where the build puts them"
                            } else {
                                "an absolute into="
                            };
                            issues.push(
                                Issue::new(format!("`collects` needs {missing}"), &file, &text)
                                    .at(node.name().span(), "incomplete")
                                    .help("`collects \"justfile.inc\" into=\"/usr/share/goojust/justfile.apps\"`"),
                            );
                        }
                    }
                }
                "fragment" => {
                    if let Some(first) = fragment_span {
                        issues.push(
                            Issue::new("`fragment` is declared twice", &file, &text)
                                .at(first, "first here")
                                .at(node.name().span(), "and again here"),
                        );
                        continue;
                    }
                    fragment_span = Some(node.name().span());
                    if !dir.join("Containerfile.inc").is_file() {
                        issues.push(
                            Issue::new(
                                format!("`{}` declares `fragment` but ships no Containerfile.inc", entry.path),
                                &file,
                                &text,
                            )
                            .at(node.name().span(), "nothing to place")
                            .help("shipping the file is what adds a fragment; this node only says where it goes"),
                        );
                    }
                    module.parse_fragment(node, &file, &text, issues);
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
                "asset" => {
                    if let Some(pin) = asset::parse(node, &file, &text, issues) {
                        if module.assets.iter().any(|a| a.name == pin.name) {
                            issues.push(
                                Issue::new(
                                    format!("asset `{}` is declared twice", pin.name),
                                    &file,
                                    &text,
                                )
                                .at(pin.span, "already declared above")
                                .help("two assets under one name would resolve to the same ASSET_* env"),
                            );
                        } else {
                            module.assets.push(pin);
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
        if !module.standard_layer {
            let dropped = module
                .secrets
                .iter()
                .map(|d| ("secret", d.name.as_str(), d.span))
                .chain(module.args.iter().map(|d| ("arg", d.name.as_str(), d.span)))
                .chain(
                    module
                        .options
                        .iter()
                        .map(|o| ("option", o.name.as_str(), o.span)),
                )
                .chain(
                    module
                        .assets
                        .iter()
                        .map(|a| ("asset", a.name.as_str(), a.span)),
                );
            for (kind, name, span) in dropped {
                issues.push(
                    Issue::new(
                        format!(
                            "`{}` declares `{kind} \"{name}\"` with no standard layer to carry it",
                            entry.path
                        ),
                        &file,
                        &text,
                    )
                    .at(span, "nowhere to land")
                    .help("`standard-layer #false` makes the fragment the whole layer, so it has to spell out its own mounts, args and env; drop one or the other"),
                );
            }
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

    /// `fragment position="after" standard-layer=#false` Defaults are the
    /// additive case: the fragment goes above the generated block and the
    /// block is still emitted.
    fn parse_fragment(&mut self, node: &KdlNode, file: &str, text: &str, issues: &mut Issues) {
        let mut position_span = None;
        for prop in node.entries() {
            let Some(key) = prop.name().map(|n| n.value()) else {
                issues.push(
                    Issue::new("`fragment` takes no arguments", file, text)
                        .at(prop.span(), "unexpected value")
                        .help("`fragment position=\"after\"`"),
                );
                continue;
            };
            match key {
                "position" => match prop.value().as_string() {
                    Some(p @ ("before" | "after")) => {
                        self.fragment_after = p == "after";
                        position_span = Some(prop.span());
                    }
                    _ => issues.push(
                        Issue::new("`position` must be \"before\" or \"after\"", file, text)
                            .at(prop.span(), "not a position")
                            .help("before, the default, puts the fragment above the generated block; after puts it below"),
                    ),
                },
                "standard-layer" => match prop.value().as_bool() {
                    Some(v) => self.standard_layer = v,
                    None => issues.push(
                        Issue::new("`standard-layer` must be #true or #false", file, text)
                            .at(prop.span(), "not a boolean"),
                    ),
                },
                other => issues.push(
                    Issue::new(format!("unknown fragment property `{other}`"), file, text)
                        .at(prop.span(), "not part of the schema")
                        .help("a fragment accepts `position` and `standard-layer`"),
                ),
            }
        }

        if !self.standard_layer {
            if let Some(span) = position_span {
                issues.push(
                    Issue::new(
                        "`position` says nothing without a standard layer",
                        file,
                        text,
                    )
                    .at(span, "there is nothing to be before or after")
                    .help("`standard-layer #false` makes the fragment the only thing this module emits"),
                );
            }
        }
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
    let mut offered: BTreeMap<&str, Vec<&Module>> = BTreeMap::new();
    for module in modules {
        for decl in module.provides.iter().chain(module.provides_files.iter()) {
            offered.entry(decl.name.as_str()).or_default().push(module);
        }
    }

    for (capability, providers) in &offered {
        if providers.len() > 1 {
            let names: Vec<&str> = providers.iter().map(|m| m.path.as_str()).collect();
            let first = providers[0];
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

    for module in modules {
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

            if let Some(provider) = providers.first() {
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

/// Every `collects` declared anywhere on disk, as filename to the module that
/// declares it, so a contribution whose consumer is not in the list can name
/// what to enable rather than just being dropped.
fn collectors_on_disk(root: &Path) -> BTreeMap<String, String> {
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
            for node in doc
                .nodes()
                .iter()
                .filter(|n| n.name().value() == "collects")
            {
                if let Some(file) = string_args(node).first() {
                    out.insert(file.to_string(), name.clone());
                }
            }
        }
    }
    out
}

/// Which files each module contributes, and where the build puts each.
pub fn resolve_collects(
    modules: &[Module],
    root: &Path,
    issues: &mut Issues,
) -> BTreeMap<String, Vec<(String, String)>> {
    let mut by_file: BTreeMap<&str, &Module> = BTreeMap::new();
    for module in modules {
        for collect in &module.collects {
            if let Some(first) = by_file.get(collect.file.as_str()) {
                issues.push(
                    Issue::new(
                        format!("two enabled modules collect `{}`", collect.file),
                        &module.file,
                        &module.text,
                    )
                    .at(collect.span, "collected again here")
                    .help(format!("already collected by `{}`", first.path)),
                );
            } else {
                by_file.insert(collect.file.as_str(), module);
            }
        }
    }

    let on_disk = collectors_on_disk(root);
    let mut out: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

    for module in modules {
        let dir = root.join("modules").join(&module.path);
        for (file, collector) in &on_disk {
            if !dir.join(file).is_file() {
                continue;
            }
            match by_file.get(file.as_str()) {
                Some(enabled) => {
                    let into = enabled
                        .collects
                        .iter()
                        .find(|c| &c.file == file)
                        .map(|c| c.into.clone())
                        .unwrap_or_default();
                    if !module.standard_layer {
                        issues.push(
                            Issue::new(
                                format!(
                                    "`{}` ships a {file} with no standard layer to collect it from",
                                    module.path
                                ),
                                &module.file,
                                &module.text,
                            )
                            .help(format!(
                                "`standard-layer #false` makes the fragment the whole layer, so it has to append the file to {into} itself"
                            )),
                        );
                        continue;
                    }
                    out.entry(module.path.clone())
                        .or_default()
                        .push((file.clone(), into));
                }
                None => issues.push(
                    Issue::new(
                        format!(
                            "`{}` ships a {file} but nothing enabled collects it",
                            module.path
                        ),
                        &module.file,
                        &module.text,
                    )
                    .help(format!(
                        "`{collector}` collects it; add it to modules.kdl, or drop the {file}"
                    )),
                ),
            }
        }
    }
    out
}

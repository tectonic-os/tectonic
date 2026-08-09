//! module.kdl: the module author's file.

use crate::diag::{Issue, Issues, Source, Span};
use crate::model::image::{Entry, Image, List};
use crate::model::module::{Collect, Contribution, Decl, Module, PackageGroup, VerifyException};
use crate::model::remote::REMOTE_DIR;
use crate::parse::disk::Disk;
use crate::parse::{asset, options, prop, string_args, syntax_issue};
use crate::resolve::options as resolve_options;
use crate::runtime::{class_names, VERIFY_CLASSES};
use kdl::{KdlDocument, KdlNode};
use std::collections::BTreeSet;
use std::path::Path;

/// The only base family today.
const FAMILIES: [&str; 1] = ["fedora"];

const TOKEN_HELP: &str = "package names and repo IDs are emitted straight into the RUN line, so they are limited to letters, digits and . _ + : -; anything else belongs in module.sh, where it can be quoted deliberately";

/// Why a package name or repo ID is not safe to emit, or None when it is.
fn bad_token(value: &str) -> Option<&'static str> {
    if value.is_empty() {
        return Some("is empty");
    }
    if value.starts_with('-') {
        return Some("starts with a dash");
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._+:-".contains(c))
    {
        return Some("has a character that is not allowed");
    }
    None
}

/// A declared `priority=`, four digits at most because that is what the staged
/// filename carries and the filename is what orders the assembly.
enum Priority {
    Missing,
    Invalid,
    Set(u32),
}

fn priority(node: &KdlNode, src: &Source, issues: &mut Issues) -> Priority {
    let Some(entry) = node
        .entries()
        .iter()
        .find(|e| e.name().map(|n| n.value()) == Some("priority"))
    else {
        return Priority::Missing;
    };
    match entry.value().as_integer() {
        Some(value) if (0..=9999).contains(&value) => Priority::Set(value as u32),
        _ => {
            issues.push(
                Issue::new("`priority` is a number from 0 to 9999", src)
                    .at(entry.span(), "not a priority")
                    .help("it becomes the NNNN in the staged filename, so four digits is the whole range there is"),
            );
            Priority::Invalid
        }
    }
}

impl Module {
    pub fn load(entry: &Entry, image: &Image, root: &Path, issues: &mut Issues) -> Option<Self> {
        if entry.remote.is_some() && root.join("modules").join(&entry.path).is_dir() {
            issues.push(
                Issue::new(
                    format!("`{}` is pinned but also exists in tree", entry.path),
                    &image.src,
                )
                .at(entry.span, "two modules would answer to this name")
                .help(format!(
                    "rename the pinned one, or drop modules/{}",
                    entry.path
                )),
            );
        }

        let dir_rel = entry.dir();
        let file = root
            .join("modules")
            .join(&dir_rel)
            .join("module.kdl")
            .display()
            .to_string();

        let Ok(text) = std::fs::read_to_string(&file) else {
            issues.push(
                Issue::new(
                    format!("`{}` has no module.kdl", entry.path),
                    &image.src,
                )
                .at(entry.span, "every module needs a manifest")
                .help(match entry.remote {
                    Some(_) => "run ./scripts/fetch-modules.sh to fetch what the image pins"
                        .to_string(),
                    None => format!(
                        "create {file}; modules/_template/module-name/module.kdl is a copy-me reference"
                    ),
                }),
            );
            return None;
        };

        let mut module = Self::parse(&entry.path, &dir_rel, root, text, issues)?;
        module.flavour = entry.flavour.clone();
        let src = &module.src.clone();
        module.resolved =
            resolve_options::resolve(&module.options, &module.variants, src, entry, image, issues);
        Some(module)
    }

    /// Everything a manifest says on its own, so a module no image lists is
    /// still held to the schema.
    fn parse(
        path: &str,
        dir_rel: &str,
        root: &Path,
        text: String,
        issues: &mut Issues,
    ) -> Option<Self> {
        let dir = root.join("modules").join(dir_rel);
        let file = dir.join("module.kdl").display().to_string();

        let src = &Source::new(&file, text.clone());
        let doc: KdlDocument = match text.parse() {
            Ok(doc) => doc,
            Err(err) => {
                issues.push(syntax_issue(&err, &file, src));
                return None;
            }
        };

        let mut module = Module {
            path: path.to_string(),
            dir: dir_rel.to_string(),
            description: String::new(),
            supports: Vec::new(),
            provides: Vec::new(),
            requires: Vec::new(),
            after: Vec::new(),
            provides_files: Vec::new(),
            provides_files_build_only: Vec::new(),
            requires_files: Vec::new(),
            overrides: Vec::new(),
            verify_exceptions: Vec::new(),
            flavour: None,
            collects: Vec::new(),
            contributes: Vec::new(),
            secrets: Vec::new(),
            args: Vec::new(),
            options: Vec::new(),
            variants: Vec::new(),
            assets: Vec::new(),
            packages: Vec::new(),
            resolved: Vec::new(),
            fragment: std::fs::read_to_string(dir.join("Containerfile.inc")).ok(),
            fragment_after: false,
            standard_layer: true,
            src: src.clone(),
        };

        let mut fragment_span: Option<Span> = None;
        for node in doc.nodes() {
            match node.name().value() {
                "description" => match string_args(node).first() {
                    Some(d) if !d.is_empty() => module.description = d.to_string(),
                    _ => issues.push(
                        Issue::new("`description` needs a string", src)
                            .at(node.name().span(), "no description given"),
                    ),
                },
                "supports" => {
                    for family in string_args(node) {
                        if !FAMILIES.contains(&family) {
                            issues.push(
                                Issue::new(format!("unknown base family `{family}`"), src)
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
                            span: node.name().span().into(),
                        })
                        .collect::<Vec<_>>();
                    if decls.is_empty() {
                        issues.push(
                            Issue::new(format!("`{kind}` needs a capability name"), src)
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
                    let build_only = match node
                        .entries()
                        .iter()
                        .find(|e| e.name().map(|n| n.value()) == Some("build-only"))
                    {
                        None => false,
                        Some(entry) if kind != "provides-file" => {
                            issues.push(
                                Issue::new(format!("`build-only` is not a `{kind}` property"), src)
                                    .at(entry.span(), "only `provides-file` declares a lifetime"),
                            );
                            false
                        }
                        Some(entry) => match entry.value().as_bool() {
                            Some(value) => value,
                            None => {
                                issues.push(
                                    Issue::new("`build-only` takes #true or #false", src)
                                        .at(entry.span(), "not a boolean"),
                                );
                                false
                            }
                        },
                    };
                    for path in string_args(node) {
                        if !path.starts_with('/') {
                            issues.push(
                                Issue::new(format!("`{path}` is not an absolute path"), src)
                                    .at(node.name().span(), "an exact path in the image"),
                            );
                        }
                        let decl = Decl {
                            name: path.to_string(),
                            span: node.name().span().into(),
                        };
                        match kind {
                            "provides-file" => {
                                if build_only {
                                    module.provides_files_build_only.push(path.to_string());
                                }
                                module.provides_files.push(decl);
                            }
                            "requires-file" => module.requires_files.push(decl),
                            _ => module.overrides.push(decl),
                        }
                    }
                }
                kind @ ("secret" | "arg") => {
                    let names = string_args(node);
                    if names.is_empty() {
                        issues.push(
                            Issue::new(format!("`{kind}` needs a name"), src)
                                .at(node.name().span(), "nothing named"),
                        );
                    }
                    for name in names {
                        let decl = Decl {
                            name: name.to_string(),
                            span: node.name().span().into(),
                        };
                        if kind == "secret" {
                            module.secrets.push(decl);
                        } else {
                            module.args.push(decl);
                        }
                    }
                }
                "allow-verify" => {
                    let span: Span = node.name().span().into();
                    let class = string_args(node).first().map(|s| s.to_string());
                    let mut unit = None;
                    for prop in node.entries() {
                        let Some(key) = prop.name().map(|n| n.value()) else {
                            continue; // the class itself
                        };
                        match key {
                            "unit" => match prop.value().as_string() {
                                Some(v) => unit = Some(v.to_string()),
                                None => issues.push(
                                    Issue::new("`unit` must be a string", src)
                                        .at(prop.span(), "not a string"),
                                ),
                            },
                            other => issues.push(
                                Issue::new(
                                    format!("unknown `allow-verify` property `{other}`"),
                                    src,
                                )
                                .at(prop.span(), "not part of the schema")
                                .help("`allow-verify` accepts `unit`"),
                            ),
                        }
                    }

                    match (class, unit) {
                        (Some(class), Some(unit)) => {
                            if !VERIFY_CLASSES.iter().any(|(name, _)| *name == class) {
                                issues.push(
                                    Issue::new(
                                        format!("`{class}` is not a verify diagnostic class"),
                                        src,
                                    )
                                    .at(span, "not one of the known classes")
                                    .help(format!(
                                        "known classes: {}. They are named rather than written as patterns, and `tect validate-image` holds what each one stands for",
                                        class_names()
                                    )),
                                );
                            } else if let Some(dup) = module
                                .verify_exceptions
                                .iter()
                                .find(|e| e.class == class && e.unit == unit)
                            {
                                issues.push(
                                    Issue::new(
                                        format!("`{class}` is allowed twice on `{unit}`"),
                                        src,
                                    )
                                    .at(dup.span, "first here")
                                    .at(span, "and again here"),
                                );
                            } else {
                                module.verify_exceptions.push(VerifyException {
                                    class,
                                    unit,
                                    span,
                                });
                            }
                        }
                        (class, unit) => {
                            let missing = if class.is_none() {
                                "a diagnostic class"
                            } else if unit.is_none() {
                                "unit=, the unit it applies to"
                            } else {
                                "both a class and a unit"
                            };
                            issues.push(
                                Issue::new(
                                    format!("`allow-verify` needs {missing}"),
                                    src,
                                )
                                .at(span, "incomplete")
                                .help(
                                    "`allow-verify \"man-page-missing\" unit=\"plasmalogin.service\"`, \
                                     which accepts one diagnostic on one unit rather than image-wide",
                                ),
                            );
                        }
                    }
                }
                "collects" => {
                    let collected = string_args(node).first().map(|s| s.to_string());
                    let into = prop(node, "into");
                    let priority = priority(node, src, issues);
                    match (collected, into, priority) {
                        (Some(collected), Some(into), Priority::Set(priority))
                            if into.starts_with('/') =>
                        {
                            module.collects.push(Collect {
                                file: collected,
                                into: into.to_string(),
                                priority,
                                span: node.name().span().into(),
                            })
                        }
                        (_, _, Priority::Invalid) => {}
                        (collected, into, priority) => {
                            let missing = if collected.is_none() {
                                "the filename it collects"
                            } else if into.is_none() {
                                "into=, where the build puts them"
                            } else if matches!(priority, Priority::Missing) {
                                "priority=, where a contribution lands when it names none"
                            } else {
                                "an absolute into="
                            };
                            issues.push(
                                Issue::new(format!("`collects` needs {missing}"), src)
                                    .at(node.name().span(), "incomplete")
                                    .help("`collects \"justfile.inc\" into=\"/usr/share/just/justfile.apps\" priority=500`"),
                            );
                        }
                    }
                }
                "contributes" => {
                    let contributed = string_args(node).first().map(|s| s.to_string());
                    let priority = priority(node, src, issues);
                    match (contributed, priority) {
                        (Some(contributed), Priority::Set(priority)) => {
                            if !dir.join(&contributed).is_file() {
                                issues.push(
                                    Issue::new(
                                        format!("`{}` orders a {contributed} it does not ship", path),
                                        src,
                                    )
                                    .at(node.name().span(), "nothing to order")
                                    .help("shipping the file is what contributes it; this node only says where it lands"),
                                );
                            } else if let Some(dup) =
                                module.contributes.iter().find(|c| c.file == contributed)
                            {
                                issues.push(
                                    Issue::new(format!("`{contributed}` is ordered twice"), src)
                                        .at(dup.span, "first here")
                                        .at(node.name().span(), "and again here"),
                                );
                            } else {
                                module.contributes.push(Contribution {
                                    file: contributed,
                                    priority,
                                    span: node.name().span().into(),
                                });
                            }
                        }
                        (_, Priority::Invalid) => {}
                        (contributed, _) => {
                            let missing = if contributed.is_none() {
                                "the filename it contributes"
                            } else {
                                "priority=, which is the only thing it declares"
                            };
                            issues.push(
                                Issue::new(format!("`contributes` needs {missing}"), src)
                                    .at(node.name().span(), "incomplete")
                                    .help("`contributes \"justfile.inc\" priority=900`, for a module that has to land after the rest"),
                            );
                        }
                    }
                }
                "fragment" => {
                    if let Some(first) = fragment_span {
                        issues.push(
                            Issue::new("`fragment` is declared twice", src)
                                .at(first, "first here")
                                .at(node.name().span(), "and again here"),
                        );
                        continue;
                    }
                    fragment_span = Some(node.name().span().into());
                    if module.fragment.is_none() {
                        issues.push(
                            Issue::new(
                                format!("`{}` declares `fragment` but ships no Containerfile.inc", path),
                                src,
                            )
                            .at(node.name().span(), "nothing to place")
                            .help("shipping the file is what adds a fragment; this node only says where it goes"),
                        );
                    }
                    module.parse_fragment(node, src, issues);
                }
                "option" => {
                    if let Some(opt) = options::parse_option(node, src, issues) {
                        if module.options.iter().any(|o| o.name == opt.name) {
                            issues.push(
                                Issue::new(format!("option `{}` is declared twice", opt.name), src)
                                    .at(opt.span, "already declared above"),
                            );
                        } else {
                            module.options.push(opt);
                        }
                    }
                }
                "asset" => {
                    if let Some(pin) = asset::parse(node, src, issues) {
                        if module.assets.iter().any(|a| a.name == pin.name) {
                            issues.push(
                                Issue::new(
                                    format!("asset `{}` is declared twice", pin.name),
                                    src,
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
                    if let Some(variant) = options::parse_variant(node, src, issues) {
                        if module.variants.iter().any(|v| v.name == variant.name) {
                            issues.push(
                                Issue::new(
                                    format!("variant `{}` is declared twice", variant.name),
                                    src,
                                )
                                .at(variant.span, "already declared above"),
                            );
                        } else {
                            module.variants.push(variant);
                        }
                    }
                }
                "packages" => module.parse_packages(node, src, issues),
                other => issues.push(
                    Issue::new(format!("unknown node `{other}`"), src)
                        .at(node.name().span(), "not part of the schema")
                        .help("SCHEMA.md documents every node a manifest may hold"),
                ),
            }
        }

        if module.description.is_empty() {
            issues.push(
                Issue::new(format!("`{}` declares no description", path), src)
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
                            path
                        ),
                        src,
                    )
                    .at(span, "nowhere to land")
                    .help("`standard-layer #false` makes the fragment the whole layer, so it has to spell out its own mounts, args and env; drop one or the other"),
                );
            }
        }

        if module.supports.is_empty() {
            issues.push(
                Issue::new(format!("`{}` declares no `supports`", path), src)
                    .help("a module has to say which base families it can build on, so a portability gap surfaces at lint rather than mid-build"),
            );
        }

        if dir.join("repo").is_file() {
            for group in &module.packages {
                issues.push(
                    Issue::new(
                        format!("`{}` declares both a `repo` file and `packages`", path),
                        src,
                    )
                    .at(group.span, "installed before the repo file is sourced")
                    .help("run-module.sh sources `repo` after the generated install, so call `dnf5 install -y` in module.sh instead"),
                );
            }
        }

        Some(module)
    }

    /// `fragment position="after" standard-layer=#false` Defaults are the
    /// additive case: the fragment goes above the generated block and the
    /// block is still emitted.
    fn parse_fragment(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let mut position_span: Option<Span> = None;
        for prop in node.entries() {
            let Some(key) = prop.name().map(|n| n.value()) else {
                issues.push(
                    Issue::new("`fragment` takes no arguments", src)
                        .at(prop.span(), "unexpected value")
                        .help("`fragment position=\"after\"`"),
                );
                continue;
            };
            match key {
                "position" => match prop.value().as_string() {
                    Some(p @ ("before" | "after")) => {
                        self.fragment_after = p == "after";
                        position_span = Some(prop.span().into());
                    }
                    _ => issues.push(
                        Issue::new("`position` must be \"before\" or \"after\"", src)
                            .at(prop.span(), "not a position")
                            .help("before, the default, puts the fragment above the generated block; after puts it below"),
                    ),
                },
                "standard-layer" => match prop.value().as_bool() {
                    Some(v) => self.standard_layer = v,
                    None => issues.push(
                        Issue::new("`standard-layer` must be #true or #false", src)
                            .at(prop.span(), "not a boolean"),
                    ),
                },
                other => issues.push(
                    Issue::new(format!("unknown fragment property `{other}`"), src)
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
                        src,
                    )
                    .at(span, "there is nothing to be before or after")
                    .help("`standard-layer #false` makes the fragment the only thing this module emits"),
                );
            }
        }
    }

    /// `packages { fedora "pkg1" "pkg2" }` Each child node names a base family
    /// and carries the package names as positional arguments.
    fn parse_packages(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let Some(children) = node.children() else {
            return;
        };
        for child in children.nodes() {
            let family = child.name().value().to_string();
            if family.is_empty() {
                issues.push(
                    Issue::new("a family name is required inside `packages`", src)
                        .at(child.name().span(), "empty name")
                        .help("`packages { fedora \"pkg1\" \"pkg2\" }`"),
                );
                continue;
            }
            if !FAMILIES.contains(&family.as_str()) {
                issues.push(
                    Issue::new(format!("unknown base family `{family}`"), src)
                        .at(
                            child.name().span(),
                            "not a family this repository builds on",
                        )
                        .help(format!("known families: {}", FAMILIES.join(", "))),
                );
                continue;
            }
            let mut packages: Vec<String> = Vec::new();
            for arg in child.entries().iter().filter(|e| e.name().is_none()) {
                let Some(value) = arg.value().as_string() else {
                    issues.push(
                        Issue::new("a package name has to be a string", src)
                            .at(arg.span(), "not a string")
                            .help("quote it: `fedora \"7zip\"`"),
                    );
                    continue;
                };
                if let Some(problem) = bad_token(value) {
                    issues.push(
                        Issue::new(format!("package name `{value}` {problem}"), src)
                            .at(arg.span(), "would not survive the RUN line")
                            .help(TOKEN_HELP),
                    );
                    continue;
                }
                packages.push(value.to_string());
            }
            if packages.is_empty() {
                issues.push(
                    Issue::new(format!("`{family}` has no packages listed"), src)
                        .at(child.name().span(), "nothing to install"),
                );
                continue;
            }
            let mut enablerepo: Option<String> = None;
            for entry in child.entries() {
                let Some(key) = entry.name().map(|n| n.value()) else {
                    continue;
                };
                match key {
                    "enablerepo" => match entry.value().as_string() {
                        Some(v) if !v.is_empty() => match bad_token(v) {
                            Some(problem) => issues.push(
                                Issue::new(format!("repo ID `{v}` {problem}"), src)
                                    .at(entry.span(), "would not survive the RUN line")
                                    .help(TOKEN_HELP),
                            ),
                            None => enablerepo = Some(v.to_string()),
                        },
                        _ => issues.push(
                            Issue::new("`enablerepo` needs a repo ID string", src)
                                .at(entry.span(), "not a string"),
                        ),
                    },
                    other => issues.push(
                        Issue::new(format!("unknown property `{other}` in packages block"), src)
                            .at(entry.span(), "not part of the schema")
                            .help("a family entry in `packages` accepts `enablerepo`"),
                    ),
                }
            }
            self.packages.push(PackageGroup {
                family,
                packages,
                enablerepo,
                span: child.name().span().into(),
            });
        }
    }
}

/// Every module on disk that no image lists, held to the schema on its own.
pub fn check_unlisted(list: &List, root: &Path, disk: &Disk, issues: &mut Issues) {
    let listed: BTreeSet<String> = list
        .images
        .iter()
        .flat_map(|image| image.entries.iter())
        .map(Entry::dir)
        .collect();

    for dir in disk.modules() {
        if listed.contains(dir) || dir.starts_with(REMOTE_DIR) {
            continue;
        }
        let file = root.join("modules").join(dir).join("module.kdl");
        if let Ok(text) = std::fs::read_to_string(&file) {
            Module::parse(dir, dir, root, text, issues);
        }
    }
}

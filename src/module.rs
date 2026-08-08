//! module.kdl: the module author's file.

use crate::asset::{self, Asset};
use crate::diag::{Issue, Issues, Source, Span};
use crate::disk::Disk;
use crate::list::{Entry, Image};
use crate::options::{self, Opt, Variant};
use crate::runtime::{class_names, VERIFY_CLASSES};
use kdl::{KdlDocument, KdlNode};
use std::collections::BTreeMap;
use std::path::Path;

/// A batch of packages keyed to a base family, with an optional repo to enable
/// for just this install.
#[derive(Debug)]
pub struct PackageGroup {
    pub family: String,
    pub packages: Vec<String>,
    pub enablerepo: Option<String>,
    pub span: Span,
}

/// A capability or contract path, and where it was declared.
pub struct Decl {
    pub name: String,
    pub span: Span,
}

/// A filename this module collects from every other module that ships one,
/// where the build puts them, and where a contribution lands in the result
/// when the contributor says nothing.
pub struct Collect {
    pub file: String,
    pub into: String,
    pub priority: u32,
    pub span: Span,
}

/// Where one contribution lands, for a module that cares.
pub struct Contribution {
    pub file: String,
    pub priority: u32,
    pub span: Span,
}

/// One `systemd-analyze verify` diagnostic a module accepts on one of its
/// units, so that a known-benign complaint does not have to be tolerated
/// image-wide.
pub struct VerifyException {
    pub class: String,
    pub unit: String,
    pub span: Span,
}

pub struct Module {
    /// The list path, which is the module's identity everywhere.
    pub path: String,
    /// Where the directory actually is, relative to `modules/`.
    pub dir: String,
    pub src: Source,
    pub description: String,
    pub supports: Vec<String>,
    /// Capabilities.
    pub provides: Vec<Decl>,
    pub requires: Vec<Decl>,
    /// Soft: ordering and cache preference, never fails.
    pub after: Vec<Decl>,
    /// Exact paths one module writes and another reads.
    pub provides_files: Vec<Decl>,
    /// The subset of `provides_files` declared `build-only=#true`: a real
    /// contract while the image builds, and gone from the shipped one because
    /// the providing module removes it again.
    pub provides_files_build_only: Vec<String>,
    pub requires_files: Vec<Decl>,
    /// Paths this module's files/ overlay knowingly replaces.
    pub overrides: Vec<Decl>,
    /// Verify diagnostics this module's own units are allowed to produce.
    pub verify_exceptions: Vec<VerifyException>,
    /// The flavour this module is gated to, from the list rather than the
    /// manifest: a module never names a flavour.
    pub flavour: Option<String>,
    pub collects: Vec<Collect>,
    pub contributes: Vec<Contribution>,
    /// Build inputs the field sets cover, so that needing a secret or a build
    /// arg does not force a module to hand-write a whole RUN block.
    pub secrets: Vec<Decl>,
    pub args: Vec<Decl>,
    pub options: Vec<Opt>,
    pub variants: Vec<Variant>,
    /// Pinned upstream payloads, resolved into env on the layer.
    pub assets: Vec<Asset>,
    /// Packages keyed to base family, installed by the generator before
    /// module.sh runs.
    pub packages: Vec<PackageGroup>,
    /// Resolved option name to value, ready to become env on the layer.
    pub resolved: Vec<(String, String)>,
    /// A Containerfile.inc, inlined verbatim, for a module whose needs the
    /// field sets cannot express.
    pub fragment: Option<String>,
    /// Where the fragment goes relative to the generated block, and whether
    /// that block is emitted at all.
    pub fragment_after: bool,
    pub standard_layer: bool,
}

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

        let dir = root.join("modules").join(entry.dir());
        let path = dir.join("module.kdl");
        let file = path.display().to_string();

        let Ok(text) = std::fs::read_to_string(&path) else {
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

        let src = &Source::new(&file, text.clone());
        let doc: KdlDocument = match text.parse() {
            Ok(doc) => doc,
            Err(err) => {
                issues.push(crate::list::syntax_issue(&err, &file, src));
                return None;
            }
        };

        let mut module = Module {
            path: entry.path.clone(),
            dir: entry.dir(),
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
            flavour: entry.flavour.clone(),
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
                                    Issue::new(format!("`build-only` takes #true or #false"), src)
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
                                        format!("`{}` orders a {contributed} it does not ship", entry.path),
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
                                format!("`{}` declares `fragment` but ships no Containerfile.inc", entry.path),
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
                Issue::new(format!("`{}` declares no description", entry.path), src)
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
                        src,
                    )
                    .at(span, "nowhere to land")
                    .help("`standard-layer #false` makes the fragment the whole layer, so it has to spell out its own mounts, args and env; drop one or the other"),
                );
            }
        }

        if module.supports.is_empty() {
            issues.push(
                Issue::new(format!("`{}` declares no `supports`", entry.path), src)
                    .help("a module has to say which base families it can build on, so a portability gap surfaces at lint rather than mid-build"),
            );
        }

        if dir.join("repo").is_file() {
            for group in &module.packages {
                issues.push(
                    Issue::new(
                        format!("`{}` declares both a `repo` file and `packages`", entry.path),
                        src,
                    )
                    .at(group.span, "installed before the repo file is sourced")
                    .help("run-module.sh sources `repo` after the generated install, so call `dnf5 install -y` in module.sh instead"),
                );
            }
        }

        module.resolved =
            options::resolve(&module.options, &module.variants, src, entry, image, issues);

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

/// Single pass over the resolved graph.
pub fn check_graph(image: &Image, root: &Path, disk: &Disk, issues: &mut Issues) {
    let mut offered: BTreeMap<&str, Vec<&Module>> = BTreeMap::new();
    for module in image.modules() {
        for decl in module.provides.iter().chain(module.provides_files.iter()) {
            offered.entry(decl.name.as_str()).or_default().push(module);
        }
    }

    let base_caps: BTreeMap<&str, &crate::list::Decl> = image
        .base
        .iter()
        .flat_map(|b| b.provides.iter().chain(b.provides_files.iter()))
        .map(|decl| (decl.name.as_str(), decl))
        .collect();

    for cap in base_caps.keys() {
        let Some(providers) = offered.get(cap) else {
            continue;
        };
        for module in providers {
            issues.push(
                Issue::new(
                    format!(
                        "`{}` provides `{cap}`, which the base image already provides",
                        module.path
                    ),
                    &module.src,
                )
                .at(
                    module
                        .provides
                        .iter()
                        .chain(module.provides_files.iter())
                        .find(|d| &d.name == cap)
                        .map(|d| d.span)
                        .unwrap_or_default(),
                    "already provided by the base",
                )
                .help(format!(
                    "the `base` node in {} declares it. Drop it from the module, or drop it from the base if the base no longer carries it",
                    image.src.name()
                )),
            );
        }
    }

    let base_family = image
        .base
        .as_ref()
        .map(|b| b.family.as_str())
        .filter(|f| !f.is_empty());
    for module in image.modules() {
        let Some(base_family) = base_family else {
            break;
        };
        if !module.supports.iter().any(|f| f == base_family) {
            let supported = module.supports.join(", ");
            issues.push(
                Issue::new(
                    format!(
                        "`{}` does not support the `{base_family}` base family",
                        module.path
                    ),
                    &module.src,
                )
                .help(if supported.is_empty() {
                    "add `supports \"fedora\"` to the manifest".to_string()
                } else {
                    format!("it declares support for: {supported}")
                }),
            );
        }
    }

    for (capability, providers) in &offered {
        if providers.len() > 1 {
            let names: Vec<&str> = providers.iter().map(|m| m.path.as_str()).collect();
            let first = providers[0];
            issues.push(
                Issue::new(
                    format!("`{capability}` is provided by more than one enabled module"),
                    &first.src,
                )
                .at(
                    first.provides.iter().chain(first.provides_files.iter())
                        .find(|d| d.name == **capability)
                        .map(|d| d.span)
                        .unwrap_or_default(),
                    "also provided elsewhere",
                )
                .help(format!(
                    "provided by: {}. Enable one provider, so that what satisfies a requirement is never ambiguous",
                    names.join(", ")
                )),
            );
        }
    }

    const MAC_POLICY: &str = "mac-policy";
    for module in image.modules() {
        let dir = root.join("modules").join(&module.dir);
        let has_policy = std::fs::read_dir(dir.join("selinux"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| e.path().extension().is_some_and(|ext| ext == "te"));
        if !has_policy || module.requires.iter().any(|d| d.name == MAC_POLICY) {
            continue;
        }
        issues.push(
            Issue::new(
                format!(
                    "`{}` ships SELinux policy without requiring `{MAC_POLICY}`",
                    module.path
                ),
                &module.src,
            )
            .help(format!(
                "add `requires \"{MAC_POLICY}\"`; lib/run-module.sh compiles selinux/*.te against the base image's policy store"
            )),
        );
    }

    for module in image.modules() {
        let hard = module
            .requires
            .iter()
            .map(|d| (d, "requires"))
            .chain(module.requires_files.iter().map(|d| (d, "requires-file")));

        for (decl, kind) in hard {
            if base_caps.contains_key(decl.name.as_str()) {
                continue;
            }

            let Some(providers) = offered.get(decl.name.as_str()) else {
                let help = match disk.providers.get(&decl.name) {
                    Some(candidates) => format!(
                        "{} would satisfy it; add it to this image. Nothing is included automatically, so the list stays the complete statement of what is in the image",
                        candidates.join(" or ")
                    ),
                    None => format!(
                        "no module in the repository declares `provides {:?}`, and neither does the `base` node in {}",
                        decl.name, image.src.name()
                    ),
                };
                issues.push(
                    Issue::new(
                        format!(
                            "`{}` {kind} `{}`, which nothing enabled provides",
                            module.path, decl.name
                        ),
                        &module.src,
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
                                &module.src,
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

/// A fragment is inlined verbatim, so nothing the generator does can make it
/// agree with the entry that carries it. Walks the entries in build order,
/// which is where `ARG FLAVOUR` lands: directly above the first gated module.
pub fn check_fragments(image: &Image, issues: &mut Issues) {
    let mut gated = false;
    for entry in &image.entries {
        let Some(module) = &entry.module else {
            continue;
        };
        gated |= entry.flavour.is_some();
        let Some(body) = &module.fragment else {
            continue;
        };
        let path = &entry.path;

        if !gated && (body.contains("${FLAVOUR}") || body.contains("$FLAVOUR")) {
            issues.push(
                Issue::new(
                    format!("`{path}` expands FLAVOUR above the flavour gate"),
                    &image.src,
                )
                .at(entry.span, "listed above the first flavour-gated module")
                .help("ARG FLAVOUR is declared directly above the first gated entry, so a fragment before it would expand to an empty string"),
            );
        }

        let runs = body.lines().any(|l| l.trim_start().starts_with("RUN "));
        let Some(flavour) = entry.flavour.as_ref().filter(|_| runs) else {
            continue;
        };
        let declared = body
            .split("FLAVOUR_GATE=")
            .nth(1)
            .map(|rest| rest.split_whitespace().next().unwrap_or_default());
        match declared {
            Some(d) if d == flavour => {}
            Some(d) => issues.push(
                Issue::new(
                    format!("`{path}` is listed under `{flavour}` but its fragment gates on `{d}`"),
                    &image.src,
                )
                .at(entry.span, "listed here"),
            ),
            None => issues.push(
                Issue::new(
                    format!(
                        "`{path}` is listed under `{flavour}` but its fragment sets no FLAVOUR_GATE"
                    ),
                    &image.src,
                )
                .at(entry.span, "the flavour gate would be silently ignored")
                .help(
                    "a fragment is emitted unconditionally, so anything it runs has to carry the gate itself",
                ),
            ),
        }
    }
}

/// One contribution, as the file in the contributor's directory and the path
/// its layer stages that file at.
pub struct Collected {
    pub file: String,
    pub staged: String,
}

/// Every contribution, and every destination assembled from them.
#[derive(Default)]
pub struct Collection {
    /// Contributor module path to what its layer stages.
    pub by_module: BTreeMap<String, Vec<Collected>>,
    /// Every destination the finalize phase assembles, sorted so two runs on
    /// the same tree emit the same ARG.
    pub destinations: Vec<String>,
}

/// Where each contribution is staged, and what gets assembled from them.
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

    let mut out = Collection::default();
    out.destinations = by_file
        .values()
        .flat_map(|collector| collector.collects.iter().map(|c| c.into.clone()))
        .collect();
    out.destinations.sort();
    out.destinations.dedup();

    for module in image.modules() {
        let dir = root.join("modules").join(&module.dir);
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
    out
}

/// Where one contribution is staged: `<into>.d/NNNN-<module>.part`.
fn staged(into: &str, priority: u32, module: &str) -> String {
    format!("{into}.d/{priority:04}-{}.part", module.replace('/', "-"))
}

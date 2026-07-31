//! modules.kdl: the image author's file.

use crate::diag::{Issue, Issues};
use crate::remote::{self, Remote, REMOTE_DIR};
use kdl::{KdlDocument, KdlNode, KdlValue};
use miette::SourceSpan;

/// The build target that carries no flavour: the ungated set, published
/// unsuffixed.
pub const NO_FLAVOUR: &str = "none";

/// The base image, and what building on it may assume.
pub struct Base {
    /// The full image reference, emitted verbatim as the generated `FROM`.
    pub image: String,
    pub family: String,
    /// Capabilities the base satisfies that no module could implement
    /// portably: rechunking, initramfs generation, MAC policy.
    pub provides: Vec<Decl>,
    /// Binaries the base guarantees.
    pub provides_files: Vec<Decl>,
    pub span: SourceSpan,
}

/// A name the base declares, with the span to point at when something about it
/// is wrong.
pub struct Decl {
    pub name: String,
    pub span: SourceSpan,
}

pub struct Flavour {
    pub name: String,
    pub default: bool,
    pub pr_build: bool,
    pub span: SourceSpan,
}

/// One entry in the list: a module, and the decisions the image author makes
/// about it.
pub struct Entry {
    pub path: String,
    pub flavour: Option<String>,
    pub variant: Option<String>,
    /// Option name to the values set on it.
    pub options: Vec<(String, Vec<KdlValue>, SourceSpan)>,
    /// The pin, for a module that lives outside this repository.
    pub remote: Option<Remote>,
    pub span: SourceSpan,
}

impl Entry {
    /// Where the module's directory is, relative to `modules/`.
    pub fn dir(&self) -> String {
        match self.remote {
            Some(_) => format!("{REMOTE_DIR}/{}", self.path),
            None => self.path.clone(),
        }
    }
}

pub struct List {
    pub file: String,
    pub text: String,
    /// None only when the `base` node is missing or malformed, which is
    /// already an issue: nothing downstream invents a default for it.
    pub base: Option<Base>,
    pub flavours: Vec<Flavour>,
    pub entries: Vec<Entry>,
}

fn is_flavour_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The first unnamed entry of a node, as a string.
fn string_arg(node: &KdlNode) -> Option<&str> {
    node.entries()
        .iter()
        .find(|e| e.name().is_none())
        .and_then(|e| e.value().as_string())
}

/// Every unnamed entry of a node, as strings, so `provides "a" "b"` reads as
/// the list it looks like.
fn string_args(node: &KdlNode) -> Vec<&str> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .filter_map(|e| e.value().as_string())
        .collect()
}

impl List {
    pub fn load(path: &str) -> Result<(Self, Issues), Box<Issue>> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Box::new(Issue::new(format!("cannot read {path}: {e}"), path, "")))?;
        Ok(Self::parse(path, text))
    }

    pub fn parse(file: &str, text: String) -> (Self, Issues) {
        let mut issues = Issues::default();
        let mut list = List {
            file: file.to_string(),
            text: text.clone(),
            base: None,
            flavours: Vec::new(),
            entries: Vec::new(),
        };

        let doc: KdlDocument = match text.parse() {
            Ok(doc) => doc,
            Err(err) => {
                eprintln!("{:?}", miette::Report::new(err));
                issues.push(Issue::new(format!("{file} is not valid KDL"), file, &text));
                return (list, issues);
            }
        };

        for node in doc.nodes() {
            match node.name().value() {
                "base" => list.parse_base(node, &mut issues),
                "flavours" => list.parse_flavours(node, &mut issues),
                "modules" => list.parse_modules(node, &mut issues),
                other => issues.push(
                    Issue::new(format!("unknown top-level node `{other}`"), file, &text)
                        .at(node.name().span(), "not part of the schema")
                        .help(
                            "modules.kdl holds a `base` node, an optional `flavours` block and a `modules` block",
                        ),
                ),
            }
        }

        if !doc.nodes().iter().any(|n| n.name().value() == "modules") {
            issues.push(
                Issue::new(format!("{file} has no `modules` block"), file, &text)
                    .help("an image with no modules is almost certainly a mistake; the block is required even when empty"),
            );
        }

        if list.base.is_none() && !doc.nodes().iter().any(|n| n.name().value() == "base") {
            issues.push(
                Issue::new(format!("{file} has no `base` node"), file, &text).help(
                    "`base \"quay.io/fedora/fedora-bootc:44\" { family \"fedora\" }`, \
                     naming the image every layer builds on",
                ),
            );
        }

        list.check_flavours(&mut issues);
        (list, issues)
    }

    fn parse_base(&mut self, node: &KdlNode, issues: &mut Issues) {
        let (file, text) = (self.file.clone(), self.text.clone());

        if let Some(first) = &self.base {
            issues.push(
                Issue::new("`base` is declared twice", &file, &text)
                    .at(first.span, "first here")
                    .at(node.name().span(), "and again here")
                    .help("an image builds on one base; a second family is a second image"),
            );
            return;
        }

        let Some(image) = string_arg(node) else {
            issues.push(
                Issue::new("`base` needs an image reference", &file, &text)
                    .at(node.name().span(), "no image given")
                    .help("`base \"quay.io/fedora/fedora-bootc:44\"`, emitted verbatim as the generated FROM"),
            );
            return;
        };

        let mut base = Base {
            image: image.to_string(),
            family: String::new(),
            provides: Vec::new(),
            provides_files: Vec::new(),
            span: node.name().span(),
        };

        for child in node.children().map(|c| c.nodes()).unwrap_or_default() {
            let names = || {
                string_args(child)
                    .iter()
                    .map(|name| Decl {
                        name: name.to_string(),
                        span: child.name().span(),
                    })
                    .collect::<Vec<_>>()
            };
            match child.name().value() {
                "family" => match string_arg(child) {
                    Some(f) => base.family = f.to_string(),
                    None => issues.push(
                        Issue::new("`family` needs a name", &file, &text)
                            .at(child.name().span(), "no family given")
                            .help("`family \"fedora\"`, matched against each module's `supports`"),
                    ),
                },
                "provides" => base.provides.extend(names()),
                "provides-file" => base.provides_files.extend(names()),
                other => issues.push(
                    Issue::new(format!("unknown base property `{other}`"), &file, &text)
                        .at(child.name().span(), "not part of the schema")
                        .help("a base accepts `family`, `provides` and `provides-file`"),
                ),
            }
        }

        if base.family.is_empty() {
            issues.push(
                Issue::new("`base` declares no `family`", &file, &text)
                    .at(base.span, "no family")
                    .help("every module declares which families it `supports`, and the two are checked against each other"),
            );
        }

        for decl in &base.provides_files {
            if !decl.name.starts_with('/') {
                issues.push(
                    Issue::new(
                        format!("`{}` is not an absolute path", decl.name),
                        &file,
                        &text,
                    )
                    .at(decl.span, "`provides-file` takes absolute paths")
                    .help("the path is checked on the finished image, where nothing has a working directory"),
                );
            }
        }

        self.base = Some(base);
    }

    fn parse_flavours(&mut self, block: &KdlNode, issues: &mut Issues) {
        let (file, text) = (self.file.clone(), self.text.clone());
        let Some(children) = block.children() else {
            issues.push(
                Issue::new("`flavours` has no flavours in it", &file, &text)
                    .at(block.name().span(), "empty block")
                    .help("omit the block entirely to build one unnamed image"),
            );
            return;
        };

        for node in children.nodes() {
            let name = node.name().value().to_string();
            let mut flavour = Flavour {
                default: false,
                pr_build: false,
                span: node.name().span(),
                name,
            };

            if !is_flavour_name(&flavour.name) {
                issues.push(
                    Issue::new(format!("invalid flavour name `{}`", flavour.name), &file, &text)
                        .at(flavour.span, "must be lowercase letters, digits and dashes, starting with a letter")
                        .help("a flavour name reaches an image name, a cache tag and a build arg, all of which restrict it"),
                );
            } else if flavour.name == NO_FLAVOUR {
                issues.push(
                    Issue::new(format!("`{NO_FLAVOUR}` is reserved"), &file, &text)
                        .at(flavour.span, "not usable as a flavour name")
                        .help("`none` names the ungated build, which is published unsuffixed and needs no declaration"),
                );
            }

            for entry in node.entries() {
                let Some(key) = entry.name().map(|n| n.value()) else {
                    issues.push(
                        Issue::new("a flavour takes no arguments", &file, &text)
                            .at(entry.span(), "unexpected value")
                            .help("the flavour's name is the node name: `desktop default=#true`"),
                    );
                    continue;
                };
                let flag = |issues: &mut Issues| match entry.value().as_bool() {
                    Some(v) => v,
                    None => {
                        issues.push(
                            Issue::new(format!("`{key}` must be #true or #false"), &file, &text)
                                .at(entry.span(), "not a boolean"),
                        );
                        false
                    }
                };
                match key {
                    "default" => flavour.default = flag(issues),
                    "pr-build" => flavour.pr_build = flag(issues),
                    other => issues.push(
                        Issue::new(format!("unknown flavour property `{other}`"), &file, &text)
                            .at(entry.span(), "not part of the schema")
                            .help("a flavour accepts `default` and `pr-build`"),
                    ),
                }
            }

            if let Some(dup) = self.flavours.iter().find(|f| f.name == flavour.name) {
                issues.push(
                    Issue::new(
                        format!("flavour `{}` is declared twice", flavour.name),
                        &file,
                        &text,
                    )
                    .at(dup.span, "first here")
                    .at(flavour.span, "and again here"),
                );
                continue;
            }
            self.flavours.push(flavour);
        }
    }

    fn parse_modules(&mut self, block: &KdlNode, issues: &mut Issues) {
        let (file, text) = (self.file.clone(), self.text.clone());
        let Some(children) = block.children() else {
            return;
        };
        for node in children.nodes() {
            match node.name().value() {
                "module" => {
                    if let Some(entry) = self.parse_entry(node, None, issues) {
                        self.entries.push(entry);
                    }
                }
                "flavour" => {
                    let Some(name) = string_arg(node) else {
                        issues.push(
                            Issue::new("`flavour` needs a flavour name", &file, &text)
                                .at(node.name().span(), "no name given")
                                .help("`flavour \"desktop\" { module \"...\" }`"),
                        );
                        continue;
                    };
                    let name = name.to_string();
                    if !self.flavours.iter().any(|f| f.name == name) {
                        let known: Vec<&str> =
                            self.flavours.iter().map(|f| f.name.as_str()).collect();
                        issues.push(
                            Issue::new(format!("`{name}` is not a declared flavour"), &file, &text)
                                .at(node.name().span(), "no such flavour")
                                .help(if known.is_empty() {
                                    "no flavours are declared; add a `flavours` block above"
                                        .to_string()
                                } else {
                                    format!("declared flavours: {}", known.join(", "))
                                }),
                        );
                    }
                    for inner in node.children().map(|c| c.nodes()).unwrap_or_default() {
                        if inner.name().value() != "module" {
                            issues.push(
                                Issue::new(
                                    format!("`{}` is not allowed inside a flavour block", inner.name().value()),
                                    &file,
                                    &text,
                                )
                                .at(inner.name().span(), "only `module` belongs here")
                                .help("flavour blocks do not nest; a module gated to two flavours is listed under each"),
                            );
                            continue;
                        }
                        if let Some(entry) = self.parse_entry(inner, Some(name.clone()), issues) {
                            self.entries.push(entry);
                        }
                    }
                }
                other => issues.push(
                    Issue::new(format!("unknown node `{other}` in `modules`"), &file, &text)
                        .at(node.name().span(), "not part of the schema")
                        .help("`modules` holds `module` entries and `flavour` blocks"),
                ),
            }
        }
    }

    fn parse_entry(
        &self,
        node: &KdlNode,
        flavour: Option<String>,
        issues: &mut Issues,
    ) -> Option<Entry> {
        let (file, text) = (&self.file, &self.text);
        let Some(path) = string_arg(node) else {
            issues.push(
                Issue::new("`module` needs a path", file, text)
                    .at(node.name().span(), "no path given")
                    .help("`module \"core/flatpak\"`, the path relative to modules/"),
            );
            return None;
        };
        let path = path.to_string();

        if let Some(dup) = self
            .entries
            .iter()
            .find(|e| e.path == path && e.flavour == flavour)
        {
            issues.push(
                Issue::new(format!("`{path}` is listed twice"), file, text)
                    .at(dup.span, "first here")
                    .at(node.name().span(), "and again here")
                    .help("a module builds once per flavour it is listed under"),
            );
            return None;
        }

        let mut variant = None;
        for entry in node.entries() {
            let Some(key) = entry.name().map(|n| n.value()) else {
                continue; // the path itself
            };
            match key {
                "variant" => match entry.value().as_string() {
                    Some(v) => variant = Some(v.to_string()),
                    None => issues.push(
                        Issue::new("`variant` must be a string", file, text)
                            .at(entry.span(), "not a string"),
                    ),
                },
                other => issues.push(
                    Issue::new(format!("unknown module property `{other}`"), file, text)
                        .at(entry.span(), "not part of the schema")
                        .help("a list entry accepts `variant`; options are set as child nodes"),
                ),
            }
        }

        let mut options = Vec::new();
        let mut pin: Option<Remote> = None;
        for child in node.children().map(|c| c.nodes()).unwrap_or_default() {
            if child.name().value() == "source" {
                if let Some(first) = pin.as_ref().map(|p| p.span) {
                    issues.push(
                        Issue::new(format!("`{path}` is pinned twice"), file, text)
                            .at(first, "first here")
                            .at(child.name().span(), "and again here"),
                    );
                    continue;
                }
                pin = remote::parse(child, file, text, issues);
                continue;
            }
            options.push((
                child.name().value().to_string(),
                child
                    .entries()
                    .iter()
                    .filter(|e| e.name().is_none())
                    .map(|e| e.value().clone())
                    .collect(),
                child.name().span(),
            ));
        }

        if pin.is_some() && !is_flavour_name(&path) {
            issues.push(
                Issue::new(format!("invalid module name `{path}`"), file, text)
                    .at(node.name().span(), "must be lowercase letters, digits and dashes, starting with a letter")
                    .help(format!("a pinned module is fetched into modules/{REMOTE_DIR}/<name>, so its name is one path segment rather than a path")),
            );
        }

        Some(Entry {
            path,
            flavour,
            variant,
            options,
            remote: pin,
            span: node.name().span(),
        })
    }

    /// The marks that replaced "first entry in the list".
    fn check_flavours(&self, issues: &mut Issues) {
        let (file, text) = (&self.file, &self.text);
        if self.flavours.is_empty() {
            return;
        }

        let defaults: Vec<&Flavour> = self.flavours.iter().filter(|f| f.default).collect();
        match defaults.len() {
            1 => {}
            0 => issues.push(
                Issue::new("no flavour is marked `default=#true`", file, text)
                    .at(self.flavours[0].span, "one of these must be the default")
                    .help("`just build` with no flavour has to build something; marking it beats inferring it from position"),
            ),
            _ => {
                let mut issue = Issue::new("more than one flavour is marked `default=#true`", file, text);
                for f in &defaults {
                    issue = issue.at(f.span, "marked default");
                }
                issues.push(issue);
            }
        }

        let pr: Vec<&Flavour> = self.flavours.iter().filter(|f| f.pr_build).collect();
        if pr.len() > 1 {
            let mut issue = Issue::new(
                "more than one flavour is marked `pr-build=#true`",
                file,
                text,
            )
            .help("a pull request builds one flavour, for half the runner time");
            for f in &pr {
                issue = issue.at(f.span, "marked pr-build");
            }
            issues.push(issue);
        }
    }

    pub fn default_flavour(&self) -> Option<&str> {
        self.flavours
            .iter()
            .find(|f| f.default)
            .map(|f| f.name.as_str())
    }

    /// Falls back to the default: a repository that has not thought about
    /// which flavour covers the most build surface still gets a PR build.
    pub fn pr_flavour(&self) -> Option<&str> {
        self.flavours
            .iter()
            .find(|f| f.pr_build)
            .map(|f| f.name.as_str())
            .or_else(|| self.default_flavour())
    }

    /// Every flavour, plus the ungated set.
    pub fn targets(&self) -> Vec<String> {
        let mut out = vec![NO_FLAVOUR.to_string()];
        out.extend(self.flavours.iter().map(|f| f.name.clone()));
        out
    }
}

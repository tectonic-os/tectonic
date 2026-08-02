//! image.kdl: the image author's file.

use crate::diag::{Issue, Issues};
use crate::remote::{self, Remote, REMOTE_DIR};
use kdl::{KdlDocument, KdlNode, KdlValue};
use miette::SourceSpan;

/// The build target that carries no flavour: the ungated set, published
/// unsuffixed.
pub const NO_FLAVOUR: &str = "none";

/// A build target: which image, and which flavour of it.
pub struct Target {
    pub image: String,
    /// A declared flavour, or `NO_FLAVOUR` for the ungated build.
    pub flavour: String,
}

impl Target {
    /// `<image>/<flavour>`.
    pub fn parse(text: &str) -> Option<Self> {
        let (image, flavour) = text.split_once('/')?;
        if image.is_empty() || flavour.is_empty() || flavour.contains('/') {
            return None;
        }
        Some(Target {
            image: image.to_string(),
            flavour: flavour.to_string(),
        })
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.image, self.flavour)
    }
}

/// One image: what it calls itself, what it builds on, and everything it is
/// made of.
pub struct Image {
    /// Where this was declared, and that file's source, so a diagnostic about
    /// anything under it points at the right file.
    pub file: String,
    pub text: String,
    /// The machine name: published image, build target, cache tag, os-release
    /// DEFAULT_HOSTNAME, MOK key directory.
    pub id: String,
    pub name: String,
    pub pretty_name: String,
    pub url: String,
    pub issues_url: String,
    /// Repository-relative paths, under brand/, which is the only directory of
    /// theirs the build context carries.
    pub logo: String,
    pub watermark: String,
    /// None only when the `base` node is missing or malformed, which is
    /// already an issue: nothing downstream invents a default for it.
    pub base: Option<Base>,
    pub flavours: Vec<Flavour>,
    pub entries: Vec<Entry>,
    pub span: SourceSpan,
}

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

/// One workflow the image author has decided about, named by its file stem
/// under `.github/workflows/`.
pub struct WorkflowToggle {
    pub name: String,
    pub enabled: bool,
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
    /// Every image declared, in declaration order.
    pub images: Vec<Image>,
    /// Only the workflows named here.
    pub workflows: Vec<WorkflowToggle>,
}

/// Lowercase letters, digits and dashes, starting with a letter.
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
            images: Vec::new(),
            workflows: Vec::new(),
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
                "image" => list.parse_image(node, &mut issues),
                "workflows" => list.parse_workflows(node, &mut issues),
                other => issues.push(
                    Issue::new(format!("unknown top-level node `{other}`"), file, &text)
                        .at(node.name().span(), "not part of the schema")
                        .help(
                            "an image file holds one `image` node; `base`, `flavours` and \
                             `modules` are declared inside it, because they are what the \
                             image is rather than what the repository is",
                        ),
                ),
            }
        }

        if !doc.nodes().iter().any(|n| n.name().value() == "image") {
            issues.push(
                Issue::new(format!("{file} declares no image"), file, &text).help(
                    "`image { name \"Name\" }`, what the image calls itself in os-release \
                     and what it publishes as",
                ),
            );
        }

        (list, issues)
    }

    fn parse_image(&mut self, node: &KdlNode, issues: &mut Issues) {
        let (file, text) = (self.file.clone(), self.text.clone());

        if let Some(stray) = string_arg(node) {
            issues.push(
                Issue::new("`image` takes no argument", &file, &text)
                    .at(node.name().span(), "the name belongs in the block")
                    .help(format!(
                        "`image {{ id \"{stray}\" }}` is the machine name, and `name` is the \
                         human one it derives from when absent"
                    )),
            );
        }

        let mut image = Image {
            file: file.clone(),
            text: text.clone(),
            id: String::new(),
            name: String::new(),
            pretty_name: String::new(),
            url: String::new(),
            issues_url: String::new(),
            logo: String::new(),
            watermark: String::new(),
            base: None,
            flavours: Vec::new(),
            entries: Vec::new(),
            span: node.name().span(),
        };

        let children = node.children().map(|c| c.nodes()).unwrap_or_default();
        for child in children {
            let value = |field: &str, issues: &mut Issues| match string_arg(child) {
                Some(v) => v.to_string(),
                None => {
                    issues.push(
                        Issue::new(format!("`{field}` needs a value"), &file, &text)
                            .at(child.name().span(), "nothing given"),
                    );
                    String::new()
                }
            };
            match child.name().value() {
                "id" => image.id = value("id", issues),
                "name" => image.name = value("name", issues),
                "pretty-name" => image.pretty_name = value("pretty-name", issues),
                "url" => image.url = value("url", issues),
                "issues-url" => image.issues_url = value("issues-url", issues),
                "logo" => image.logo = value("logo", issues),
                "watermark" => image.watermark = value("watermark", issues),
                "base" => image.parse_base(child, issues),
                "flavours" => image.parse_flavours(child, issues),
                "modules" => {}
                other => issues.push(
                    Issue::new(format!("unknown image property `{other}`"), &file, &text)
                        .at(child.name().span(), "not part of the schema")
                        .help(
                            "an image accepts `id`, `name`, `pretty-name`, `url`, \
                             `issues-url`, `logo` and `watermark`, and the `base`, \
                             `flavours` and `modules` blocks",
                        ),
                ),
            }
        }
        for child in children {
            if child.name().value() == "modules" {
                image.parse_modules(child, issues);
            }
        }

        if image.name.is_empty() {
            issues.push(
                Issue::new("`image` declares no `name`", &file, &text)
                    .at(image.span, "no name")
                    .help("`name \"Tectonic\"` is os-release NAME, which the boot menu and the desktop read"),
            );
        }

        if image.id.is_empty() {
            image.id = image.name.to_lowercase().replace(' ', "-");
            if !image.name.is_empty() && !is_flavour_name(&image.id) {
                issues.push(
                    Issue::new(
                        format!("`{}` does not derive a usable image name", image.name),
                        &file,
                        &text,
                    )
                    .at(image.span, "no `id`, and `name` does not lowercase into one")
                    .help("declare `id \"something\"`: lowercase letters, digits and dashes, starting with a letter"),
                );
                image.id = String::new();
            }
        } else if !is_flavour_name(&image.id) {
            issues.push(
                Issue::new(format!("invalid image name `{}`", image.id), &file, &text)
                    .at(image.span, "must be lowercase letters, digits and dashes, starting with a letter")
                    .help("it becomes an image tag, a cache tag and the default hostname, all of which restrict it"),
            );
        }

        for (field, path) in [("logo", &image.logo), ("watermark", &image.watermark)] {
            if !path.is_empty() && !path.starts_with("brand/") {
                issues.push(
                    Issue::new(format!("`{field}` is not under brand/"), &file, &text)
                        .at(image.span, "brand assets live in brand/")
                        .help("the build context carries brand/ for them; a path anywhere else is not in it"),
                );
            }
        }

        if image.base.is_none() && !children.iter().any(|c| c.name().value() == "base") {
            issues.push(
                Issue::new("`image` declares no `base`", &file, &text)
                    .at(image.span, "nothing to build on")
                    .help(
                        "`base \"quay.io/fedora/fedora-bootc:44\" { family \"fedora\" }`, \
                         naming the image every layer builds on",
                    ),
            );
        }

        if !children.iter().any(|c| c.name().value() == "modules") {
            issues.push(
                Issue::new("`image` has no `modules` block", &file, &text)
                    .at(image.span, "nothing in it")
                    .help("an image with no modules is almost certainly a mistake; the block is required even when empty"),
            );
        }

        image.check_flavours(issues);
        self.images.push(image);
    }

}

impl Image {
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

    /// `workflows { smoke-test enabled=#false }` Each child names a workflow
    /// by its file stem.
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

    /// Every image the repository declares, in declaration order.
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

}

impl List {
    fn parse_workflows(&mut self, block: &KdlNode, issues: &mut Issues) {
        let (file, text) = (self.file.clone(), self.text.clone());
        let Some(children) = block.children() else {
            issues.push(
                Issue::new("`workflows` has no workflows in it", &file, &text)
                    .at(block.name().span(), "empty block")
                    .help("omit the block entirely; every workflow in .github/workflows/ runs unless something here says otherwise"),
            );
            return;
        };

        for node in children.nodes() {
            let name = node.name().value().to_string();
            let span = node.name().span();

            if let Some(dup) = self.workflows.iter().find(|w| w.name == name) {
                issues.push(
                    Issue::new(format!("workflow `{name}` is declared twice"), &file, &text)
                        .at(dup.span, "first here")
                        .at(span, "and again here")
                        .help("one workflow is either on or off; two answers means the file below wins silently"),
                );
                continue;
            }

            let mut enabled: Option<bool> = None;
            let mut stated = false;
            for entry in node.entries() {
                let Some(key) = entry.name().map(|n| n.value()) else {
                    issues.push(
                        Issue::new("a workflow takes no arguments", &file, &text)
                            .at(entry.span(), "unexpected value")
                            .help("the file stem is the node name: `smoke-test enabled=#false`"),
                    );
                    continue;
                };
                match key {
                    "enabled" => {
                        stated = true;
                        match entry.value().as_bool() {
                            Some(v) => enabled = Some(v),
                            None => issues.push(
                                Issue::new("`enabled` must be #true or #false", &file, &text)
                                    .at(entry.span(), "not a boolean"),
                            ),
                        }
                    }
                    other => issues.push(
                        Issue::new(format!("unknown workflow property `{other}`"), &file, &text)
                            .at(entry.span(), "not part of the schema")
                            .help("a workflow accepts `enabled`"),
                    ),
                }
            }

            let Some(enabled) = enabled else {
                if !stated {
                    issues.push(
                        Issue::new(
                            format!("`{name}` says nothing about whether it runs"),
                            &file,
                            &text,
                        )
                        .at(span, "no `enabled`")
                        .help(format!(
                            "`{name} enabled=#false` turns it off; a workflow nobody wants to change belongs outside this block"
                        )),
                    );
                }
                continue;
            };

            self.workflows.push(WorkflowToggle {
                name,
                enabled,
                span,
            });
        }
    }

    pub fn images(&self) -> Vec<&Image> {
        self.images.iter().collect()
    }

    /// The image a command answers about when it is given no image, and the
    /// one a bare build builds.
    pub fn default_image(&self) -> Option<&Image> {
        self.images.first()
    }

    /// Every target: for each image, the ungated set and then its flavours.
    pub fn targets(&self) -> Vec<Target> {
        let mut out = Vec::new();
        for image in self.images() {
            out.push(Target {
                image: image.id.clone(),
                flavour: NO_FLAVOUR.to_string(),
            });
            out.extend(image.flavours.iter().map(|f| Target {
                image: image.id.clone(),
                flavour: f.name.clone(),
            }));
        }
        out
    }

    /// What a build with nothing named builds: the default image, at its
    /// default flavour, or its ungated set when it declares no flavours.
    pub fn default_target(&self) -> Option<Target> {
        self.default_image().map(|image| Target {
            image: image.id.clone(),
            flavour: image.default_flavour().unwrap_or(NO_FLAVOUR).to_string(),
        })
    }

    /// The one target a pull request builds, for half the runner time.
    pub fn pr_target(&self) -> Option<Target> {
        self.default_image().map(|image| Target {
            image: image.id.clone(),
            flavour: image.pr_flavour().unwrap_or(NO_FLAVOUR).to_string(),
        })
    }
}

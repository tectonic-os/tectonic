//! The `image` node: what an image calls itself, builds on, and is made of.

use crate::diag::{Issue, Issues, Source};
use crate::model::image::{is_flavour_name, Base, Decl, Entry, Flavour, Image, List, NO_FLAVOUR};
use crate::model::remote::{Remote, REMOTE_DIR};
use crate::parse::{bool_arg, options, remote, string_arg, string_args};
use kdl::KdlNode;

impl List {
    pub(super) fn parse_image(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        if let Some(stray) = string_arg(node) {
            issues.push(
                Issue::new("`image` takes no argument", src)
                    .at(node.name().span(), "the name belongs in the block")
                    .help(format!(
                        "`image {{ id \"{stray}\" }}` is the machine name, and `name` is the \
                         human one it derives from when absent"
                    )),
            );
        }

        let mut image = Image {
            src: src.clone(),
            id: String::new(),
            name: String::new(),
            pretty_name: String::new(),
            url: String::new(),
            issues_url: String::new(),
            base: None,
            flavours: Vec::new(),
            entries: Vec::new(),
            span: node.name().span().into(),
        };

        let children = node.children().map(|c| c.nodes()).unwrap_or_default();
        for child in children {
            let value = |field: &str, issues: &mut Issues| match string_arg(child) {
                Some(v) => v.to_string(),
                None => {
                    issues.push(
                        Issue::new(format!("`{field}` needs a value"), src)
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
                "base" => image.parse_base(child, issues),
                "flavours" => image.parse_flavours(child, issues),
                "modules" => {}
                other => issues.push(
                    Issue::new(format!("unknown image property `{other}`"), src)
                        .at(child.name().span(), "not part of the schema")
                        .help(
                            "an image accepts `id`, `name`, `pretty-name`, `url` \
                             and `issues-url`, and the `base`, `flavours` and \
                             `modules` blocks",
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
                Issue::new("`image` declares no `name`", src)
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
                        src,
                    )
                    .at(image.span, "no `id`, and `name` does not lowercase into one")
                    .help("declare `id \"something\"`: lowercase letters, digits and dashes, starting with a letter"),
                );
                image.id = String::new();
            }
        } else if !is_flavour_name(&image.id) {
            issues.push(
                Issue::new(format!("invalid image name `{}`", image.id), src)
                    .at(image.span, "must be lowercase letters, digits and dashes, starting with a letter")
                    .help("it becomes an image tag, a cache tag and the default hostname, all of which restrict it"),
            );
        }

        if image.base.is_none() && !children.iter().any(|c| c.name().value() == "base") {
            issues.push(
                Issue::new("`image` declares no `base`", src)
                    .at(image.span, "nothing to build on")
                    .help(
                        "`base \"quay.io/fedora/fedora-bootc:44\" { family \"fedora\" }`, \
                         naming the image every layer builds on",
                    ),
            );
        }

        if !children.iter().any(|c| c.name().value() == "modules") {
            issues.push(
                Issue::new("`image` has no `modules` block", src)
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
        let src = &self.src.clone();

        if let Some(first) = &self.base {
            issues.push(
                Issue::new("`base` is declared twice", src)
                    .at(first.span, "first here")
                    .at(node.name().span(), "and again here")
                    .help("an image builds on one base; a second family is a second image"),
            );
            return;
        }

        let Some(image) = string_arg(node) else {
            issues.push(
                Issue::new("`base` needs an image reference", src)
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
            signed: false,
            span: node.name().span().into(),
        };

        for child in node.children().map(|c| c.nodes()).unwrap_or_default() {
            let names = || {
                string_args(child)
                    .iter()
                    .map(|name| Decl {
                        name: name.to_string(),
                        span: child.name().span().into(),
                    })
                    .collect::<Vec<_>>()
            };
            match child.name().value() {
                "family" => match string_arg(child) {
                    Some(f) => base.family = f.to_string(),
                    None => issues.push(
                        Issue::new("`family` needs a name", src)
                            .at(child.name().span(), "no family given")
                            .help("`family \"fedora\"`, matched against each module's `supports`"),
                    ),
                },
                "provides" => base.provides.extend(names()),
                "provides-file" => base.provides_files.extend(names()),
                "signed" => match bool_arg(child) {
                    Some(v) => base.signed = v,
                    None => issues.push(
                        Issue::new("`signed` needs #true or #false", src)
                            .at(child.name().span(), "not a boolean")
                            .help("`signed #false` records that this base publishes no cosign signature; base-sig-probe.yml keeps it current"),
                    ),
                },
                other => issues.push(
                    Issue::new(format!("unknown base property `{other}`"), src)
                        .at(child.name().span(), "not part of the schema")
                        .help("a base accepts `family`, `provides`, `provides-file` and `signed`"),
                ),
            }
        }

        if base.family.is_empty() {
            issues.push(
                Issue::new("`base` declares no `family`", src)
                    .at(base.span, "no family")
                    .help("every module declares which families it `supports`, and the two are checked against each other"),
            );
        }

        for decl in &base.provides_files {
            if !decl.name.starts_with('/') {
                issues.push(
                    Issue::new(
                        format!("`{}` is not an absolute path", decl.name),
                        src,
                    )
                    .at(decl.span, "`provides-file` takes absolute paths")
                    .help("the path is checked on the finished image, where nothing has a working directory"),
                );
            }
        }

        self.base = Some(base);
    }

    fn parse_flavours(&mut self, block: &KdlNode, issues: &mut Issues) {
        let src = &self.src.clone();
        let Some(children) = block.children() else {
            issues.push(
                Issue::new("`flavours` has no flavours in it", src)
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
                span: node.name().span().into(),
                name,
            };

            if !is_flavour_name(&flavour.name) {
                issues.push(
                    Issue::new(format!("invalid flavour name `{}`", flavour.name), src)
                        .at(flavour.span, "must be lowercase letters, digits and dashes, starting with a letter")
                        .help("a flavour name reaches an image name, a cache tag and a build arg, all of which restrict it"),
                );
            } else if flavour.name == NO_FLAVOUR {
                issues.push(
                    Issue::new(format!("`{NO_FLAVOUR}` is reserved"), src)
                        .at(flavour.span, "not usable as a flavour name")
                        .help("`none` names the ungated build, which is published unsuffixed and needs no declaration"),
                );
            }

            for entry in node.entries() {
                let Some(key) = entry.name().map(|n| n.value()) else {
                    issues.push(
                        Issue::new("a flavour takes no arguments", src)
                            .at(entry.span(), "unexpected value")
                            .help("the flavour's name is the node name: `desktop default=#true`"),
                    );
                    continue;
                };
                let flag = |issues: &mut Issues| match entry.value().as_bool() {
                    Some(v) => v,
                    None => {
                        issues.push(
                            Issue::new(format!("`{key}` must be #true or #false"), src)
                                .at(entry.span(), "not a boolean"),
                        );
                        false
                    }
                };
                match key {
                    "default" => flavour.default = flag(issues),
                    "pr-build" => flavour.pr_build = flag(issues),
                    other => issues.push(
                        Issue::new(format!("unknown flavour property `{other}`"), src)
                            .at(entry.span(), "not part of the schema")
                            .help("a flavour accepts `default` and `pr-build`"),
                    ),
                }
            }

            if let Some(dup) = self.flavours.iter().find(|f| f.name == flavour.name) {
                issues.push(
                    Issue::new(format!("flavour `{}` is declared twice", flavour.name), src)
                        .at(dup.span, "first here")
                        .at(flavour.span, "and again here"),
                );
                continue;
            }
            self.flavours.push(flavour);
        }
    }

    fn parse_modules(&mut self, block: &KdlNode, issues: &mut Issues) {
        let src = &self.src.clone();
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
                            Issue::new("`flavour` needs a flavour name", src)
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
                            Issue::new(format!("`{name}` is not a declared flavour"), src)
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
                                    src,
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
                    Issue::new(format!("unknown node `{other}` in `modules`"), src)
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
        let src = &self.src;
        let Some(path) = string_arg(node) else {
            issues.push(
                Issue::new("`module` needs a path", src)
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
                Issue::new(format!("`{path}` is listed twice"), src)
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
                        Issue::new("`variant` must be a string", src)
                            .at(entry.span(), "not a string"),
                    ),
                },
                other => issues.push(
                    Issue::new(format!("unknown module property `{other}`"), src)
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
                        Issue::new(format!("`{path}` is pinned twice"), src)
                            .at(first, "first here")
                            .at(child.name().span(), "and again here"),
                    );
                    continue;
                }
                pin = remote::parse(child, src, issues);
                continue;
            }
            options.push((
                child.name().value().to_string(),
                options::args(child),
                child.name().span().into(),
            ));
        }

        if pin.is_some() && !is_flavour_name(&path) {
            issues.push(
                Issue::new(format!("invalid module name `{path}`"), src)
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
            span: node.name().span().into(),
            module: None,
        })
    }

    fn check_flavours(&self, issues: &mut Issues) {
        let src = &self.src;
        if self.flavours.is_empty() {
            return;
        }

        let defaults: Vec<&Flavour> = self.flavours.iter().filter(|f| f.default).collect();
        match defaults.len() {
            0 | 1 => {}
            _ => {
                let mut issue = Issue::new("more than one flavour is marked `default=#true`", src);
                for f in &defaults {
                    issue = issue.at(f.span, "marked default");
                }
                issues.push(issue);
            }
        }

        let pr: Vec<&Flavour> = self.flavours.iter().filter(|f| f.pr_build).collect();
        if pr.len() > 1 {
            let mut issue = Issue::new("more than one flavour is marked `pr-build=#true`", src)
                .help("a pull request builds one flavour, for half the runner time");
            for f in &pr {
                issue = issue.at(f.span, "marked pr-build");
            }
            issues.push(issue);
        }
    }
}

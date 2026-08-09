//! The `image` node: what an image calls itself, builds on, and is made of.

use crate::diag::{Issue, Issues, Source, Span};
use crate::model::image::{is_flavour_name, Base, Decl, Entry, Flavour, Image, List, NO_FLAVOUR};
use crate::model::remote::{Remote, REMOTE_DIR};
use crate::parse::schema::{Arg, Kind, Node, Prop, Say};
use crate::parse::{bool_arg, child, flag, kids, options, prop, remote};
use crate::parse::{string_arg, string_args, text};
use kdl::KdlNode;

const NEEDS_VALUE: Say = Say::new("`{}` needs a value", "nothing given", "");

/// A list entry, which is the same node ungated and inside a flavour block.
#[rustfmt::skip]
const ENTRY: Node = Node::new("module", "One module the image is made of, named by its path under `modules/`.")
    .arg(Arg::Str, Say::new("`module` needs a path", "no path given",
        "`module \"core/flatpak\"`, the path relative to modules/"))
    .props(&[
        Prop { name: "variant", kind: Kind::Str,
            desc: "Which of the module's declared variants this image builds.",
            say: Say::new("`variant` must be a string", "not a string", ""),
            missing: Say::NONE },
    ], Say::new("unknown module property `{}`", "not part of the schema",
        "a list entry accepts `variant`; options are set as child nodes"))
    .children(&[
        Node::new("source",
            "Where a module that lives outside this repository is fetched from, and what pins it."),
        Node::new("",
            "An option the module declares, set for this image by the node's name."),
    ], Say::NONE);

/// The image file's grammar, and the whole of it.
#[rustfmt::skip]
pub const IMAGE: Node = Node::new("image",
    "One image: what it calls itself, what it builds on, and everything it is made of.")
    .arg(Arg::None, Say::new("`image` takes no argument", "the name belongs in the block",
        "`image { id \"{}\" }` is the machine name, and `name` is the human one it derives from \
         when absent"))
    .children(&[
        Node::new("id",
            "The machine name: published image, build target, cache tag, os-release \
             DEFAULT_HOSTNAME. Derived from `name` when it is not declared.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("name", "os-release NAME, which the boot menu and the desktop read.")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .missing(Say::new("`image` declares no `name`", "no name",
                "`name \"Tectonic\"` is os-release NAME, which the boot menu and the desktop read")),
        Node::new("pretty-name", "os-release PRETTY_NAME, the full name a user is shown.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("url", "The project's home page, in os-release and the image labels.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("issues-url", "Where a user reports a problem with the image.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),

        Node::new("base", "The image every layer builds on, and what building on it may assume.")
            .arg(Arg::Str, Say::new("`base` needs an image reference", "no image given",
                "`base \"quay.io/fedora/fedora-bootc:44\"`, emitted verbatim as the generated FROM"))
            .once("an image builds on one base; a second family is a second image")
            .missing(Say::new("`image` declares no `base`", "nothing to build on",
                "`base \"quay.io/fedora/fedora-bootc:44\" { family \"fedora\" }`, naming the image \
                 every layer builds on"))
            .children(&[
                Node::new("family", "The base's family, matched against every module's `supports`.")
                    .arg(Arg::Str, Say::new("`family` needs a name", "no family given",
                        "`family \"fedora\"`, matched against each module's `supports`"))
                    .once("")
                    .missing(Say::new("`base` declares no `family`", "no family",
                        "every module declares which families it `supports`, and the two are \
                         checked against each other")),
                Node::new("provides",
                    "Capabilities the base satisfies that no module could implement portably.")
                    .arg(Arg::Strs, Say::NONE),
                Node::new("provides-file",
                    "Absolute paths the base guarantees, which a module may require.")
                    .arg(Arg::Strs, Say::NONE),
                Node::new("signed", "Whether the base publishes a cosign signature.")
                    .arg(Arg::Bool, Say::new("`signed` needs #true or #false", "not a boolean",
                        "`signed #false` records that this base publishes no cosign signature; \
                         base-sig-probe.yml keeps it current"))
                    .once(""),
            ], Say::new("unknown base property `{}`", "not part of the schema",
                "a base accepts `family`, `provides`, `provides-file` and `signed`")),

        Node::new("flavours", "The flavours this image publishes beside its ungated build.")
            .once("a second block would split one set of flavours in two")
            .empty(Say::new("`flavours` has no flavours in it", "empty block",
                "omit the block entirely to build one unnamed image"))
            .children(&[
                Node::new("",
                    "One flavour, named by the node: a gated module set published as \
                     `<image>-<flavour>`.")
                    .arg(Arg::None, Say::new("a flavour takes no arguments", "unexpected value",
                        "the flavour's name is the node name: `desktop default=#true`"))
                    .props(&[
                        Prop { name: "default", kind: Kind::Bool,
                            desc: "Whether a build that names no flavour builds this one.",
                            say: Say::new("`{}` must be #true or #false", "not a boolean", ""),
                            missing: Say::NONE },
                        Prop { name: "pr-build", kind: Kind::Bool,
                            desc: "Whether a pull request builds this flavour rather than the \
                                   default.",
                            say: Say::new("`{}` must be #true or #false", "not a boolean", ""),
                            missing: Say::NONE },
                    ], Say::new("unknown flavour property `{}`", "not part of the schema",
                        "a flavour accepts `default` and `pr-build`")),
            ], Say::NONE),

        Node::new("modules",
            "Every module the image is made of: ungated entries, and the flavours that gate \
             the rest.")
            .once("a second block would split one list in two")
            .missing(Say::new("`image` has no `modules` block", "nothing in it",
                "an image with no modules is almost certainly a mistake; the block is required \
                 even when empty"))
            .children(&[
                ENTRY,
                Node::new("flavour", "The modules one flavour adds, which build only for that flavour.")
                    .arg(Arg::Str, Say::new("`flavour` needs a flavour name", "no name given",
                        "`flavour \"desktop\" { module \"...\" }`"))
                    .children(&[ENTRY],
                        Say::new("`{}` is not allowed inside a flavour block",
                            "only `module` belongs here",
                            "flavour blocks do not nest; a module gated to two flavours is listed \
                             under each")),
            ], Say::new("unknown node `{}` in `modules`", "not part of the schema",
                "`modules` holds `module` entries and `flavour` blocks")),
    ], Say::new("unknown image property `{}`", "not part of the schema",
        "an image accepts `id`, `name`, `pretty-name`, `url` and `issues-url`, and the `base`, \
         `flavours` and `modules` blocks"));

/// Every `name "a" "b"` under a node, as declarations pointing at the node.
fn decls(node: &KdlNode, name: &str) -> Vec<Decl> {
    kids(node)
        .iter()
        .filter(|c| c.name().value() == name)
        .flat_map(|c| {
            let span: Span = c.name().span().into();
            string_args(c).into_iter().map(move |value| Decl {
                name: value.to_string(),
                span,
            })
        })
        .collect()
}

impl List {
    pub(super) fn parse_image(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let mut image = Image {
            src: src.clone(),
            id: text(node, "id"),
            name: text(node, "name"),
            pretty_name: text(node, "pretty-name"),
            url: text(node, "url"),
            issues_url: text(node, "issues-url"),
            base: None,
            flavours: Vec::new(),
            entries: Vec::new(),
            span: node.name().span().into(),
        };

        if let Some(base) = child(node, "base") {
            image.parse_base(base, issues);
        }
        if let Some(flavours) = child(node, "flavours") {
            image.parse_flavours(flavours, issues);
        }
        if let Some(modules) = child(node, "modules") {
            image.parse_modules(modules, issues);
        }

        image.check_id(issues);
        image.check_flavours(issues);
        self.images.push(image);
    }
}

impl Image {
    fn parse_base(&mut self, node: &KdlNode, issues: &mut Issues) {
        let src = &self.src.clone();
        let Some(image) = string_arg(node) else {
            return;
        };

        let base = Base {
            image: image.to_string(),
            family: text(node, "family"),
            provides: decls(node, "provides"),
            provides_files: decls(node, "provides-file"),
            signed: child(node, "signed").and_then(bool_arg).unwrap_or(false),
            span: node.name().span().into(),
        };

        for decl in &base.provides_files {
            if !decl.name.starts_with('/') {
                issues.push(
                    Issue::new(format!("`{}` is not an absolute path", decl.name), src)
                        .at(decl.span, "`provides-file` takes absolute paths")
                        .help("the path is checked on the finished image, where nothing has a working directory"),
                );
            }
        }

        self.base = Some(base);
    }

    /// The id derives from `name` when it is not declared, and either way has to
    /// survive being an image tag.
    fn check_id(&mut self, issues: &mut Issues) {
        let src = &self.src.clone();
        if self.id.is_empty() {
            self.id = self.name.to_lowercase().replace(' ', "-");
            if !self.name.is_empty() && !is_flavour_name(&self.id) {
                issues.push(
                    Issue::new(
                        format!("`{}` does not derive a usable image name", self.name),
                        src,
                    )
                    .at(self.span, "no `id`, and `name` does not lowercase into one")
                    .help("declare `id \"something\"`: lowercase letters, digits and dashes, starting with a letter"),
                );
                self.id = String::new();
            }
        } else if !is_flavour_name(&self.id) {
            issues.push(
                Issue::new(format!("invalid image name `{}`", self.id), src)
                    .at(self.span, "must be lowercase letters, digits and dashes, starting with a letter")
                    .help("it becomes an image tag, a cache tag and the default hostname, all of which restrict it"),
            );
        }
    }

    fn parse_flavours(&mut self, block: &KdlNode, issues: &mut Issues) {
        let src = &self.src.clone();
        for node in kids(block) {
            let flavour = Flavour {
                name: node.name().value().to_string(),
                default: flag(node, "default"),
                pr_build: flag(node, "pr-build"),
                span: node.name().span().into(),
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
        for node in kids(block) {
            match node.name().value() {
                "module" => {
                    if let Some(entry) = self.parse_entry(node, None, issues) {
                        self.entries.push(entry);
                    }
                }
                "flavour" => {
                    let Some(name) = string_arg(node).map(str::to_string) else {
                        continue;
                    };
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
                    for inner in kids(node) {
                        if inner.name().value() != "module" {
                            continue;
                        }
                        if let Some(entry) = self.parse_entry(inner, Some(name.clone()), issues) {
                            self.entries.push(entry);
                        }
                    }
                }
                _ => {}
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
        let path = string_arg(node)?.to_string();

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

        let mut options = Vec::new();
        let mut pin: Option<Remote> = None;
        for child in kids(node) {
            if child.name().value() == "source" {
                match pin.as_ref().map(|p| p.span) {
                    Some(first) => issues.push(
                        Issue::new(format!("`{path}` is pinned twice"), src)
                            .at(first, "first here")
                            .at(child.name().span(), "and again here"),
                    ),
                    None => pin = remote::parse(child, src, issues),
                }
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
            variant: prop(node, "variant").map(str::to_string),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::schema::check;
    use kdl::KdlDocument;

    fn messages(text: &str) -> Vec<String> {
        let doc: KdlDocument = text.parse().expect("valid KDL");
        let src = Source::new("image.kdl", text);
        let mut issues = Issues::default();
        check(&doc.nodes()[0], &IMAGE, &src, &mut issues);
        issues
            .plain()
            .lines()
            .filter_map(|line| line.strip_prefix("  x "))
            .map(str::to_string)
            .collect()
    }

    /// Every shape the golden corpus has no broken fixture for.
    #[test]
    fn the_table_catches_what_the_corpus_does_not() {
        let found = messages(
            r#"
image "stray" {
    id
    base "quay.io/fedora/fedora-bootc:44" {
        signed "yes"
        colour "red"
    }
    base "quay.io/fedora/fedora-bootc:43"
    flavours {
        dev sparkle=#true
        dim default="yes"
        bad "arg"
    }
    modules {
        flavour "dev" {
            packages
        }
        module
        module "core/one" flavor="x"
        drives
    }
    palette "red"
}
"#,
        );
        assert_eq!(
            found,
            [
                "`image` takes no argument",
                "`id` needs a value",
                "`signed` needs #true or #false",
                "unknown base property `colour`",
                "`base` declares no `family`",
                "`base` is declared twice",
                "unknown flavour property `sparkle`",
                "`default` must be #true or #false",
                "a flavour takes no arguments",
                "`packages` is not allowed inside a flavour block",
                "`module` needs a path",
                "unknown module property `flavor`",
                "unknown node `drives` in `modules`",
                "unknown image property `palette`",
                "`image` declares no `name`",
            ]
        );
    }

    #[test]
    fn an_empty_flavours_block_is_a_block_with_nothing_in_it() {
        let found = messages(
            r#"
image {
    name "X"
    base "quay.io/fedora/fedora-bootc:44" { family "fedora" }
    flavours { }
    modules { }
}
"#,
        );
        assert_eq!(found, ["`flavours` has no flavours in it"]);
    }
}

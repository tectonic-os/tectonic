//! repo.kdl, and the walk over every image file beside it.

use crate::diag::{Issue, Issues, Source, Span};
use crate::model::image::{List, WorkflowToggle, REPO_FILE, SCHEMA_VERSION};
use crate::parse::image::IMAGE;
use crate::parse::remote::{parse_collection, COLLECTION};
use crate::parse::schema::{check_doc, Arg, Kind, Node, Prop, Say};
use crate::parse::{boolean, int_arg, kids, string_arg, syntax_issue};
use kdl::{KdlDocument, KdlNode};
use std::path::Path;

/// repo.kdl's grammar, and the whole of it.
#[rustfmt::skip]
pub const REPO: Node = Node::new("repo",
    "What is true of the repository rather than of any image in it.")
    .children(&[
        Node::new("schema-version",
            "The schema release this repository is written against, which picks the reader.")
            .arg(Arg::Int, Say::new("`{}` needs a number", "not a version", "`schema-version 1`"))
            .once(""),
        Node::new("default-image", "The image a build that names no target builds.")
            .arg(Arg::Str, Say::new("`{}` needs an image name", "no image given",
                "`default-image \"workstation\"`, naming one of the images declared at the root"))
            .once(""),
        Node::new("pr-image", "The image a pull request builds.")
            .arg(Arg::Str, Say::new("`{}` needs an image name", "no image given",
                "`pr-image \"workstation\"`, since a pull request builds one target"))
            .once(""),
        Node::new("workflows",
            "The shipped workflows this repository turns off, named by file stem.")
            .once("a second block would split one set of toggles in two")
            .empty(Say::new("`workflows` has no workflows in it", "empty block",
                "omit the block entirely; every workflow in .github/workflows/ runs unless \
                 something here says otherwise"))
            .children(&[
                Node::new("", "One workflow, named by the node, and whether it runs.")
                    .arg(Arg::None, Say::new("a workflow takes no arguments", "unexpected value",
                        "the file stem is the node name: `smoke-test enabled=#false`"))
                    .props(&[
                        Prop { name: "enabled", kind: Kind::Bool,
                            desc: "Whether the workflow runs at all.",
                            say: Say::new("`{}` must be #true or #false", "not a boolean", ""),
                            missing: Say::new("`{}` says nothing about whether it runs",
                                "no `enabled`",
                                "`{} enabled=#false` turns it off; a workflow nobody wants to \
                                 change belongs outside this block") },
                    ], Say::new("unknown workflow property `{}`", "not part of the schema",
                        "a workflow accepts `enabled`")),
            ], Say::NONE),
        Node::new("sources",
            "The module collections `tect module import` resolves a name against.")
            .once("a second block would split one registry in two")
            .empty(Say::new("`sources` has no collections in it", "empty block",
                "omit the block entirely; a repository with nothing here imports from nothing"))
            .children(&[COLLECTION], Say::NONE),
    ], Say::new("unknown node `{}` in repo.kdl", "not part of the schema",
        "repo.kdl holds `schema-version`, `default-image`, `pr-image`, a `workflows` block and a \
         `sources` block: what is true of the repository rather than of any image in it. An image \
         goes in a file of its own"));

/// Every other root `.kdl`, which is one image and nothing else.
#[rustfmt::skip]
const IMAGE_FILE: Node = Node::new("image file", "One image, in a file of its own.")
    .children(&[IMAGE], Say::new("unknown top-level node `{}`", "not part of the schema",
        "every root .kdl but repo.kdl is one `image` node; `base`, `flavours` and `modules` are \
         declared inside it, because they are what the image is rather than what the repository \
         is"));

/// Everything one schema version's grammar produces.
type Reader = fn(&Path) -> (List, Issues);

/// The reader for a schema version. One version, one reader, today: a version
/// this release cannot read is refused rather than parsed against the wrong
/// grammar, and a new version adds an arm here rather than forking the reader.
fn reader(version: i128) -> Option<Reader> {
    match version {
        v if v == i128::from(SCHEMA_VERSION) => Some(List::read),
        _ => None,
    }
}

/// The version repo.kdl declares, read before anything else because it decides
/// which reader sees the rest. A repo.kdl that is missing, unparseable or
/// declares nothing usable falls through to the current reader, which is what
/// reports it.
fn declared_version(root: &Path) -> Option<(i128, Span, Source)> {
    let path = root.join(REPO_FILE);
    let text = std::fs::read_to_string(&path).ok()?;
    let doc: KdlDocument = text.parse().ok()?;
    let node = doc
        .nodes()
        .iter()
        .find(|n| n.name().value() == "schema-version")?;
    let version = int_arg(node)?;
    let src = Source::new(path.display().to_string(), text);
    Some((version, node.name().span().into(), src))
}

impl List {
    /// The declared schema version, then the reader it selects.
    pub fn load(root: &Path) -> (Self, Issues) {
        let Some((version, span, src)) = declared_version(root) else {
            return List::read(root);
        };
        if let Some(read) = reader(version) {
            return read(root);
        }

        let mut issues = Issues::default();
        issues.push(
            Issue::new(
                format!("this repository is written against schema version {version}"),
                &src,
            )
            .at(span, format!("this tool knows {SCHEMA_VERSION}"))
            .help(if version > i128::from(SCHEMA_VERSION) {
                "the repository is ahead of the tool; take a newer release"
            } else {
                "the tool is ahead of the repository; nothing else is read, because every \
                 diagnostic under a grammar this release does not have would be noise"
            }),
        );
        (List::empty(root), issues)
    }

    fn empty(root: &Path) -> Self {
        List {
            images: Vec::new(),
            workflows: Vec::new(),
            sources: Vec::new(),
            default_image_id: None,
            pr_image_id: None,
            schema_version: None,
            schema_version_seen: false,
            repo_src: Source::new(root.join(REPO_FILE).display().to_string(), ""),
            files: Vec::new(),
        }
    }

    /// Every `*.kdl` at the repository root: one image apiece, plus repo.kdl,
    /// which is repo context and declares no image.
    fn read(root: &Path) -> (Self, Issues) {
        let mut issues = Issues::default();
        let mut list = List::empty(root);

        let mut names: Vec<String> = Vec::new();
        match std::fs::read_dir(root) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    if !entry.path().is_file() {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".kdl") {
                        names.push(name);
                    }
                }
            }
            Err(err) => {
                issues.push(Issue::new(
                    format!("cannot read {}: {err}", root.display()),
                    &list.repo_src,
                ));
                return (list, issues);
            }
        }
        names.sort();

        for name in &names {
            let path = root.join(name).display().to_string();
            let text = match std::fs::read_to_string(root.join(name)) {
                Ok(text) => text,
                Err(err) => {
                    issues.push(Issue::new(
                        format!("cannot read {path}: {err}"),
                        &Source::new(&path, ""),
                    ));
                    continue;
                }
            };
            list.files.push(path.clone());
            let src = Source::new(&path, text.clone());
            if name == REPO_FILE {
                list.repo_src = src.clone();
            }
            list.parse_file(&src, &text, name == REPO_FILE, &mut issues);
        }

        list.check_images(&mut issues);
        (list, issues)
    }

    /// One file, which is either an image or the repo context.
    fn parse_file(&mut self, src: &Source, text: &str, is_repo: bool, issues: &mut Issues) {
        let doc: KdlDocument = match text.parse() {
            Ok(doc) => doc,
            Err(err) => {
                issues.push(syntax_issue(&err, src.name(), src));
                return;
            }
        };

        check_doc(&doc, if is_repo { &REPO } else { &IMAGE_FILE }, src, issues);

        for node in doc.nodes() {
            match (is_repo, node.name().value()) {
                (false, "image") => self.parse_image(node, src, issues),
                (true, "workflows") => self.parse_workflows(node, src, issues),
                (true, "sources") => self.parse_sources(node, src, issues),
                (true, "default-image") => {
                    self.default_image_id = string_arg(node).map(str::to_string);
                }
                (true, "pr-image") => {
                    self.pr_image_id = string_arg(node).map(str::to_string);
                }
                (true, "schema-version") => {
                    self.schema_version_seen = true;
                    self.schema_version = int_arg(node).map(|_| SCHEMA_VERSION);
                }
                _ => {}
            }
        }

        if !is_repo && !doc.nodes().iter().any(|n| n.name().value() == "image") {
            issues.push(
                Issue::new(format!("{} declares no image", src.name()), src).help(format!(
                    "every root .kdl but {REPO_FILE} holds one `image` node: \
                     `image {{ name \"Name\" }}`, what the image calls itself in os-release \
                     and what it publishes as"
                )),
            );
        }
    }

    fn check_images(&self, issues: &mut Issues) {
        for (index, image) in self.images.iter().enumerate() {
            if image.id.is_empty() {
                continue; // already reported as underivable
            }
            for other in &self.images[..index] {
                if other.id == image.id {
                    issues.push(
                        Issue::new(
                            format!("`{}` is declared by two files", image.id),
                            &image.src,
                        )
                        .at(image.span, format!("also declared in {}", other.src.name()))
                        .help(
                            "two images cannot publish under one name; declare `id` on one of them",
                        ),
                    );
                }
            }
            for flavour in &image.flavours {
                let published = format!("{}-{}", image.id, flavour.name);
                for other in &self.images {
                    if other.id == published {
                        issues.push(
                            Issue::new(
                                format!("two builds would publish as `{published}`"),
                                &image.src,
                            )
                            .at(flavour.span, "this flavour")
                            .at(image.span, "of this image")
                            .help(format!(
                                "the image declared in {} publishes under that name too; \
                                 rename one of them",
                                other.src.name()
                            )),
                        );
                    }
                }
            }
        }

        if !self.schema_version_seen {
            issues.push(
                Issue::new(
                    format!("{REPO_FILE} declares no `schema-version`"),
                    &self.repo_src,
                )
                .help(format!(
                    "`schema-version {SCHEMA_VERSION}`, so a tool from a different release \
                     says so plainly instead of reporting every node it does not recognise"
                )),
            );
        }

        if self.images.len() > 1 {
            match &self.default_image_id {
                None => issues.push(
                    Issue::new(
                        format!(
                            "{} images are declared and none is the default",
                            self.images.len()
                        ),
                        &self.repo_src,
                    )
                    .help(format!(
                        "`default-image \"{}\"` in {REPO_FILE}, naming which one a build \
                         with no target builds",
                        self.images[0].id
                    )),
                ),
                Some(id) if !self.images.iter().any(|i| &i.id == id) => issues.push(
                    Issue::new(
                        format!("`default-image` names `{id}`, which is not a declared image"),
                        &self.repo_src,
                    )
                    .help(format!(
                        "images: {}",
                        self.images
                            .iter()
                            .map(|i| i.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                ),
                Some(_) => {}
            }
        }

        if let Some(id) = &self.pr_image_id {
            if !self.images.iter().any(|i| &i.id == id) {
                issues.push(
                    Issue::new(
                        format!("`pr-image` names `{id}`, which is not a declared image"),
                        &self.repo_src,
                    )
                    .help("a pull request builds one target, of one declared image"),
                );
            }
        }
    }

    /// `workflows { smoke-test enabled=#false }` Each child names a workflow
    /// by its file stem.
    fn parse_workflows(&mut self, block: &KdlNode, src: &Source, issues: &mut Issues) {
        for node in kids(block) {
            let name = node.name().value().to_string();
            let span: Span = node.name().span().into();

            if let Some(dup) = self.workflows.iter().find(|w| w.name == name) {
                issues.push(
                    Issue::new(format!("workflow `{name}` is declared twice"), src)
                        .at(dup.span, "first here")
                        .at(span, "and again here")
                        .help("one workflow is either on or off; two answers means the file below wins silently"),
                );
                continue;
            }

            if let Some(enabled) = boolean(node, "enabled") {
                self.workflows.push(WorkflowToggle {
                    name,
                    enabled,
                    span,
                });
            }
        }
    }

    /// `sources { tectonic-os "..." }` Each child names a collection by the
    /// owner its modules land under.
    fn parse_sources(&mut self, block: &KdlNode, src: &Source, issues: &mut Issues) {
        for node in kids(block) {
            let Some(collection) = parse_collection(node, src, issues) else {
                continue;
            };
            if let Some(dup) = self.sources.iter().find(|c| c.name == collection.name) {
                issues.push(
                    Issue::new(
                        format!("collection `{}` is declared twice", collection.name),
                        src,
                    )
                    .at(dup.span, "first here")
                    .at(collection.span, "and again here")
                    .help("both would import into the same directory, so one of them would be shadowed silently"),
                );
                continue;
            }
            self.sources.push(collection);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(text: &str) -> Vec<String> {
        let doc: KdlDocument = text.parse().expect("valid KDL");
        let src = Source::new(REPO_FILE, text);
        let mut issues = Issues::default();
        check_doc(&doc, &REPO, &src, &mut issues);
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
schema-version
schema-version 1
default-image
pr-image
workflows {
    smoke-test "on" trigger="push"
    build enabled="yes"
    lint
}
colour "blue"
"#,
        );
        assert_eq!(
            found,
            [
                "`schema-version` needs a number",
                "`schema-version` is declared twice",
                "`default-image` needs an image name",
                "`pr-image` needs an image name",
                "a workflow takes no arguments",
                "unknown workflow property `trigger`",
                "`smoke-test` says nothing about whether it runs",
                "`enabled` must be #true or #false",
                "`lint` says nothing about whether it runs",
                "unknown node `colour` in repo.kdl",
            ]
        );
    }

    #[test]
    fn an_empty_workflows_block_is_a_block_with_nothing_in_it() {
        let found = messages("schema-version 1\nworkflows { }\n");
        assert_eq!(found, ["`workflows` has no workflows in it"]);
    }

    /// The collection table, which the broken fixture reaches the meaning of
    /// but not the shape.
    #[test]
    fn a_collection_is_a_location_and_what_pins_it() {
        let found = messages(
            r#"
schema-version 1
sources {
    owner "https://host/owner/modules/{ref}.tar.gz" branch="main" {
        ref "v1"
        ref "v2"
        subtree "modules"
    }
}
"#,
        );
        assert_eq!(
            found,
            [
                "unknown collection property `branch`",
                "`ref` is declared twice",
                "unknown node `subtree` in a collection",
            ]
        );
    }
}

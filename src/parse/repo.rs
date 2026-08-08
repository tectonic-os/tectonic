//! repo.kdl, and the walk over every image file beside it.

use crate::diag::{Issue, Issues, Source, Span};
use crate::model::image::{List, WorkflowToggle, REPO_FILE, SCHEMA_VERSION};
use crate::parse::{string_arg, syntax_issue};
use kdl::{KdlDocument, KdlNode};
use std::path::Path;

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
    let version = node
        .entries()
        .iter()
        .find(|e| e.name().is_none())?
        .value()
        .as_integer()?;
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

        if !names.iter().any(|n| n != REPO_FILE) {
            issues.push(
                Issue::new(
                    format!("{} declares no image", root.display()),
                    &list.repo_src,
                )
                .help(
                    "an image is a `.kdl` file at the repository root holding one `image` \
                     node; image.kdl is the name a repository with one image tends to use",
                ),
            );
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

        for node in doc.nodes() {
            match (is_repo, node.name().value()) {
                (false, "image") => self.parse_image(node, src, issues),
                (true, "workflows") => self.parse_workflows(node, src, issues),
                (true, "default-image") => {
                    self.default_image_id = string_arg(node).map(str::to_string);
                }
                (true, "pr-image") => {
                    self.pr_image_id = string_arg(node).map(str::to_string);
                }
                (true, "schema-version") => self.parse_schema_version(node, src, issues),
                (true, other) => issues.push(
                    Issue::new(format!("unknown node `{other}` in {REPO_FILE}"), src)
                        .at(node.name().span(), "not part of the schema")
                        .help(
                            "repo.kdl holds `schema-version`, `default-image`, `pr-image` \
                             and a `workflows` block: what is true of the repository rather \
                             than of any image in it. An image goes in a file of its own",
                        ),
                ),
                (false, other) => issues.push(
                    Issue::new(format!("unknown top-level node `{other}`"), src)
                        .at(node.name().span(), "not part of the schema")
                        .help(format!(
                            "every root .kdl but {REPO_FILE} is one `image` node; `base`, \
                             `flavours` and `modules` are declared inside it, because they \
                             are what the image is rather than what the repository is"
                        )),
                ),
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

    /// A version this reader does not know never reaches it: `load` picks the
    /// reader from the same node first.
    fn parse_schema_version(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        self.schema_version_seen = true;
        let declared = node
            .entries()
            .iter()
            .find(|e| e.name().is_none())
            .and_then(|e| e.value().as_integer());
        match declared {
            Some(_) => self.schema_version = Some(SCHEMA_VERSION),
            None => issues.push(
                Issue::new("`schema-version` needs a number", src)
                    .at(node.name().span(), "not a version")
                    .help(format!("`schema-version {SCHEMA_VERSION}`")),
            ),
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
        let Some(children) = block.children() else {
            issues.push(
                Issue::new("`workflows` has no workflows in it", src)
                    .at(block.name().span(), "empty block")
                    .help("omit the block entirely; every workflow in .github/workflows/ runs unless something here says otherwise"),
            );
            return;
        };

        for node in children.nodes() {
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

            let mut enabled: Option<bool> = None;
            let mut stated = false;
            for entry in node.entries() {
                let Some(key) = entry.name().map(|n| n.value()) else {
                    issues.push(
                        Issue::new("a workflow takes no arguments", src)
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
                                Issue::new("`enabled` must be #true or #false", src)
                                    .at(entry.span(), "not a boolean"),
                            ),
                        }
                    }
                    other => issues.push(
                        Issue::new(format!("unknown workflow property `{other}`"), src)
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
                            src,
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
}

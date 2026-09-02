//! repo.kdl, and the walk over every image file beside it.

use crate::diag::{Issue, Issues, Source, Span};
use crate::layout;
use crate::model::image::{List, Seed, Workflow, SCHEMA_VERSION, TECT_VERSION};
use crate::parse::image::IMAGE;
use crate::parse::remote::{parse_collection, COLLECTION};
use crate::parse::schema::{check_doc, Arg, Kind, Node, Prop, Say};
use crate::parse::{
    bool_arg, check_sha256, child, int_arg, kids, prop, prop_span, string_arg, syntax_issue,
};
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
        Node::new("tect-version",
            "The tect release this repository is built with, which `scripts/tect.sh` fetches for the build.")
            .arg(Arg::Str, Say::new("`{}` needs a release", "not a version",
                concat!("`tect-version \"", env!("CARGO_PKG_VERSION"),
                        "\"`, the release the build fetches; another release reads the \
                         repository and says so")))
            .props(&[
                Prop { name: "sha256", kind: Kind::Str,
                    desc: "The release tarball's sha256, which `scripts/tect.sh` holds the \
                           download to. Absent, the script checks against the checksum fetched \
                           beside the tarball, which proves the download and nothing more.",
                    say: Say::new("`{}` must be a hash", "not a string", ""),
                    missing: Say::NONE },
            ], Say::new("unknown tect-version property `{}`", "not part of the schema",
                "a `tect-version` accepts `sha256`"))
            .once(""),
        Node::new("name",
            "What the repository calls itself, whatever the directory holding it is called.")
            .arg(Arg::Str, Say::new("`{}` needs a name", "no name given",
                "`name \"Workstation\"`, which the directory is free to disagree with"))
            .once("")
            .missing(Say::new("repo.kdl says nothing about what this repository is called",
                "no name", "`name \"Workstation\"`, so a rename of the directory changes \
                 nothing about what this repository answers to")),
        Node::new("default-image",
            "The image a command given no image answers about, and a build with no target builds.")
            .arg(Arg::Str, Say::new("`{}` needs an image name", "no image given",
                "`default-image \"workstation\"`, naming one of the images declared at the root"))
            .once(""),
        Node::new("pr-image", "The image a pull request builds.")
            .arg(Arg::Str, Say::new("`{}` needs an image name", "no image given",
                "`pr-image \"workstation\"`, since a pull request builds one target"))
            .once(""),
        Node::new("seed",
            "The image this repository publishes a declaration of, for a new repository to start \
             from.")
            .arg(Arg::Str, Say::new("`{}` needs an image name", "no image given",
                "`seed \"workstation\" collection=\"owner\"`, naming one of the images declared \
                 at the root"))
            .once("a repository publishes one seed")
            .props(&[
                Prop { name: "collection", kind: Kind::Str,
                    desc: "The collection this repository publishes its own modules as, which is \
                           what names them in the seed.",
                    say: Say::new("`{}` must be a collection name", "not a string", ""),
                    missing: Say::new("`{}` says nothing about where its modules are published",
                        "no `collection`",
                        "`{} collection=\"owner\"`, one of the collections in `sources`: every \
                         module in a seed is fetched through one, so a repository publishing no \
                         collection of its own has nothing a seeded repository can import") },
            ], Say::new("unknown seed property `{}`", "not part of the schema",
                "a seed accepts `collection`")),
        Node::new("workflows",
            "The CI `tect generate` writes into .github/workflows/, named by file stem. One \
             this does not name is not written.")
            .once("a second block would split one set of workflows in two")
            .empty(Say::new("`workflows` has no workflows in it", "empty block",
                "omit the block entirely; a repository with nothing here generates no CI"))
            .props(&[
                Prop { name: "at", kind: Kind::Str,
                    desc: "The hour and minute the daily build runs, UTC. Every other schedule \
                           is an offset from it.",
                    say: Say::new("`{}` must be a time of day", "not a string", ""),
                     missing: Say::NONE },
                Prop { name: "publish", kind: Kind::Str,
                    desc: "When images publish. `scheduled` moves publishing off pushes while \
                           keeping the daily build.",
                    say: Say::new("`{}` must be a cadence", "not a string", "`publish=\"scheduled\"`"),
                    missing: Say::NONE },
                Prop { name: "scan", kind: Kind::Str,
                    desc: "When image scans run. `scheduled` moves them off pushes; scheduled \
                           publishing does too because scans consume published images.",
                    say: Say::new("`{}` must be a cadence", "not a string", "`scan=\"scheduled\"`"),
                    missing: Say::NONE },
            ], Say::new("unknown workflows property `{}`", "not part of the schema",
                "a workflows block accepts `at`, `publish` and `scan`"))
            .children(&[
                Node::new("", "One workflow, named by the node.")
                    .arg(Arg::None, Say::new("a workflow takes no arguments", "unexpected value",
                        "the file stem is the node name: `smoke-test`"))
                    .props(&[], Say::new("unknown workflow property `{}`", "not part of the schema",
                        "a workflow is named and nothing else; `at` on the block moves every \
                         schedule at once")),
            ], Say::NONE),
        Node::new("sources",
            "The module collections `tect import module` and `tect copy module` resolve against.")
            .once("a second block would split one registry in two")
            .empty(Say::new("`sources` has no collections in it", "empty block",
                "omit the block entirely; a repository with nothing here references or copies from nothing"))
            .children(&[COLLECTION], Say::NONE),
        Node::new("manifest",
            "Whether a build stamps the generated manifest onto the image as an OCI label.")
            .once("a second block would split one setting in two")
            .empty(Say::new("`manifest` has no `label` in it", "empty block",
                "omit the block entirely; a build with nothing here stamps no label"))
            .children(&[
                Node::new("label",
                    "Whether the build stamps `org.tectonic.manifest` with the path to the \
                     baked manifest file.")
                    .arg(Arg::Bool, Say::new("`label` needs #true or #false", "not a boolean",
                        "`label #true` stamps the built image with an `org.tectonic.manifest` \
                         label"))
                    .once(""),
            ], Say::new("unknown node `{}` in manifest", "not part of the schema",
                "a manifest block holds `label`")),

        Node::new("audit",
            "How strictly the provenance facts are held. Every one of them is recorded either \
             way; this decides only which of them is fatal.")
            .once("a second block would split one posture in two")
            .empty(Say::new("`audit` has no `enforce` in it", "empty block",
                "omit the block entirely; a repository with nothing here records every \
                 provenance fact and fails on none of them"))
            .children(&[
                Node::new("enforce",
                    "Whether a provenance fact that is missing or does not match stops the run \
                     rather than being reported.")
                    .arg(Arg::Bool, Say::new("`enforce` needs #true or #false", "not a boolean",
                        "`enforce #true` makes an unverified import, a module edited since \
                         import, a base that will not resolve and an unstamped build into \
                         errors"))
                    .once(""),
            ], Say::new("unknown node `{}` in audit", "not part of the schema",
                "an audit block holds `enforce`")),
    ], Say::new("unknown node `{}` in repo.kdl", "not part of the schema",
        "repo.kdl holds `schema-version`, `tect-version`, `name`, `default-image`, `pr-image`, \
         `seed`, a \
         `workflows` block, a `sources` block, a `manifest` block and an `audit` block: what is \
         true of the \
         repository rather than of any image in it. An image goes in a file of its own"));

/// `image.kdl` or `<name>.image.kdl` at the root, holding whatever images it
/// likes.
#[rustfmt::skip]
const IMAGE_FILE: Node = Node::new("image file",
    "Images, in a file named `image.kdl` or `<name>.image.kdl`. The name in front is decorative: \
     an image is called what it declares, so one file may hold as many as suit the repository.")
    .children(&[IMAGE], Say::new("unknown top-level node `{}`", "not part of the schema",
        "an image file holds `image` nodes and nothing else; `base`, `flavours` and `modules` are \
         declared inside one, because they are what the image is rather than what the repository \
         is"));

/// What repo.kdl declares about which tool reads it.
struct Pins {
    schema: Option<(i128, Span)>,
    tect: Option<(String, Span)>,
    tect_sha: Option<String>,
    src: Source,
}

/// Read directly rather than through the grammar, and before anything else,
/// because they decide whether this release reads the rest at all. A repo.kdl
/// that is missing, unparseable or declares neither falls through to the
/// reader, which is what reports it.
fn pins(root: &Path) -> Option<Pins> {
    let path = root.join(layout::REPO_FILE);
    let text = std::fs::read_to_string(&path).ok()?;
    let doc: KdlDocument = text.parse().ok()?;
    let node = |name: &str| {
        doc.nodes()
            .iter()
            .find(|n| n.name().value() == name)
            .cloned()
    };
    Some(Pins {
        schema: node("schema-version").and_then(|n| Some((int_arg(&n)?, n.name().span().into()))),
        tect: node("tect-version")
            .and_then(|n| Some((string_arg(&n)?.to_string(), n.name().span().into()))),
        tect_sha: node("tect-version").and_then(|n| prop(&n, "sha256").map(str::to_string)),
        src: Source::new(path.display().to_string(), text),
    })
}

/// Whether this release may work in the repository at all. `parse/` understands
/// one schema, so a repository written against another is refused rather than
/// read against the wrong grammar. That is the whole gate: `tect-version` names
/// a release rather than a grammar, and every node the walker accepts is in a
/// schema table, so a pin naming another release says nothing about whether the
/// declarations here can be read. See `pinned_elsewhere`.
pub fn compatible(root: &Path) -> Issues {
    let mut issues = Issues::default();
    let Some(pins) = pins(root) else {
        return issues;
    };

    if let Some((version, span)) = pins
        .schema
        .filter(|(v, _)| *v != i128::from(SCHEMA_VERSION))
    {
        let ahead = version > i128::from(SCHEMA_VERSION);
        issues.push(
            Issue::new(
                format!("this repository is written against schema version {version}"),
                &pins.src,
            )
            .at(span, format!("this tool knows {SCHEMA_VERSION}"))
            .help(match ahead {
                true => "the repository is ahead of the tool; `tect-version` names the release \
                         that reads it, and `scripts/tect.sh` fetches that one"
                    .to_string(),
                false => format!(
                    "nothing here moves a repository to schema {SCHEMA_VERSION}, and nothing \
                     else in it is read either, because every diagnostic under a grammar this \
                     release does not have would be noise; run the release `tect-version` names, \
                     which `scripts/tect.sh` fetches"
                ),
            }),
        );
        return issues;
    }

    issues
}

/// The release a repository pins, when that is not this one.
///
/// A notice rather than a refusal. What a pin protects is *generated output*,
/// not readability: a different release writes different workflow bodies, and
/// `verify` already reports that as drift with `generate` to resolve it.
/// Refusing instead made every repository unusable between releases, patch
/// bumps included, while `schema-version` — the thing that does decide whether
/// the declarations parse — sat unchanged.
pub fn pinned_elsewhere(root: &Path) -> Option<String> {
    let version = pins(root)?.tect?.0;
    (version != TECT_VERSION).then_some(version)
}

/// A release pinned with no declared sha256, which `scripts/tect.sh` then
/// holds to the checksum fetched beside the tarball. `check` reports it;
/// nothing refuses, since a repository predating the first release that carries
/// one declares none.
pub fn pinned_unverified(root: &Path) -> Option<String> {
    let pins = pins(root)?;
    pins.tect
        .filter(|_| pins.tect_sha.is_none())
        .map(|(version, _)| version)
}

/// The collections a `sources` block declares, read out of text rather than
/// out of a repository: what a repository that is not written yet will have,
/// which is the only thing `create repo` can offer modules against. Anything
/// wrong with the block is the scaffold's own and is reported where the
/// repository is read.
pub fn sources_in(text: &str) -> Vec<crate::model::remote::Collection> {
    let src = Source::new(layout::REPO_FILE, text);
    let Ok(doc) = text.parse::<KdlDocument>() else {
        return Vec::new();
    };
    let mut list = List::empty(Path::new(""));
    let mut issues = Issues::default();
    for node in doc.nodes().iter().filter(|n| n.name().value() == "sources") {
        list.parse_sources(node, &src, &mut issues);
    }
    list.sources
}

impl List {
    /// The versions first: a repository this release cannot work in is refused
    /// before anything in it is read.
    pub fn load(root: &Path) -> (Self, Issues) {
        let issues = compatible(root);
        match issues.is_empty() {
            true => List::read(root),
            false => (List::empty(root), issues),
        }
    }

    fn empty(root: &Path) -> Self {
        List {
            name: String::new(),
            id: String::new(),
            images: Vec::new(),
            workflows: Vec::new(),
            workflows_at: crate::resolve::workflow::DEFAULT_AT,
            publishes_scheduled: false,
            scans_scheduled: false,
            sources: Vec::new(),
            default_image_id: None,
            pr_image_id: None,
            seed: None,
            manifest_label: false,
            audit_enforce: false,
            schema_version: None,
            schema_version_seen: false,
            repo_src: Source::new(root.join(layout::REPO_FILE).display().to_string(), ""),
            files: Vec::new(),
        }
    }

    /// repo.kdl, which is repo context, and every image file beside it. A root
    /// `.kdl` that is neither is nobody's, and is reported rather than read.
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
            if name != layout::REPO_FILE && !layout::is_image_file(name) {
                issues.push(
                    Issue::new(format!("nothing reads `{name}`"), &Source::new(&path, "")).help(
                        format!(
                            "an image file is `{}` or `<name>{}`; rename it to `{}`, or move it \
                             out of the repository root",
                            layout::IMAGE_FILE,
                            layout::IMAGE_SUFFIX,
                            layout::as_image_file(name)
                        ),
                    ),
                );
                continue;
            }
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
            if name == layout::REPO_FILE {
                list.repo_src = src.clone();
            }
            list.parse_file(&src, &text, name == layout::REPO_FILE, &mut issues);
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
                (true, "seed") => {
                    self.seed = string_arg(node).map(|image| Seed {
                        image: image.to_string(),
                        collection: prop(node, "collection").unwrap_or_default().to_string(),
                    });
                }
                (true, "schema-version") => {
                    self.schema_version_seen = true;
                    self.schema_version = int_arg(node).map(|_| SCHEMA_VERSION);
                }
                (true, "tect-version") => {
                    if let Some(sha256) = prop(node, "sha256") {
                        check_sha256(
                            sha256,
                            "`tect-version`",
                            prop_span(node, "sha256").unwrap_or_default(),
                            src,
                            issues,
                        );
                    }
                }
                (true, "name") => {
                    self.name = string_arg(node).unwrap_or_default().to_string();
                    self.id = self.name.to_lowercase().replace(' ', "-");
                }
                (true, "audit") => {
                    self.audit_enforce = child(node, "enforce").and_then(bool_arg).unwrap_or(false)
                }
                (true, "manifest") => {
                    self.manifest_label = child(node, "label").and_then(bool_arg).unwrap_or(false);
                }
                _ => {}
            }
        }

        if !is_repo && !doc.nodes().iter().any(|n| n.name().value() == "image") {
            issues.push(
                Issue::new(format!("{} declares no image", src.name()), src).help(
                    "an image file holds at least one `image` node: \
                     `image { name \"Name\" }`, what the image calls itself in os-release \
                     and what it publishes as",
                ),
            );
        }
    }

    fn check_images(&self, issues: &mut Issues) {
        for (index, image) in self.images.iter().enumerate() {
            for entry in &image.entries {
                let Some(source) = &entry.source else {
                    continue;
                };
                let declared = self
                    .sources
                    .iter()
                    .find(|declared| &declared.name == source);
                if declared.is_none() {
                    issues.push(
                        Issue::new(format!("`{source}` is not declared in `sources`"), &image.src)
                            .at(entry.span, "this module has nowhere to come from")
                            .help("declare the collection in repo.kdl, or list a local module outside a source block"),
                    );
                } else if self.audit_enforce && declared.is_some_and(|source| source.unpinned()) {
                    issues.push(
                        Issue::new(format!("`{source}` follows an unverified ref"), &image.src)
                            .at(entry.span, "this reference cannot be verified")
                            .help("pin the collection to a version and sha256, or drop audit enforcement"),
                    );
                }
            }
            if image.id.is_empty() {
                continue; // already reported as underivable
            }
            for other in &self.images[..index] {
                if other.id == image.id {
                    let same = other.src.name() == image.src.name();
                    issues.push(
                        Issue::new(
                            match same {
                                true => format!("`{}` is declared twice", image.id),
                                false => format!("`{}` is declared by two files", image.id),
                            },
                            &image.src,
                        )
                        .at(
                            image.span,
                            match same {
                                true => "also declared above".to_string(),
                                false => format!("also declared in {}", other.src.name()),
                            },
                        )
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
                                "the image declared {} publishes under that name too; \
                                 rename one of them",
                                match other.src.name() == image.src.name() {
                                    true => "in this file".to_string(),
                                    false => format!("in {}", other.src.name()),
                                }
                            )),
                        );
                    }
                }
            }
        }

        if !self.schema_version_seen {
            issues.push(
                Issue::new(
                    format!("{} declares no `schema-version`", layout::REPO_FILE),
                    &self.repo_src,
                )
                .help(format!(
                    "`schema-version {SCHEMA_VERSION}`, so a tool from a different release \
                     says so plainly instead of reporting every node it does not recognise"
                )),
            );
        }

        if let Some(id) = &self.default_image_id {
            if !self.images.iter().any(|i| &i.id == id) {
                issues.push(
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
                );
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

        if let Some(seed) = &self.seed {
            self.check_seed(seed, issues);
        }
    }

    /// Whether the seeded image is one a seeded repository could resolve: it
    /// carries module names and nothing else, so every one of them has to be
    /// fetchable from a collection this repository declares.
    fn check_seed(&self, seed: &Seed, issues: &mut Issues) {
        let Some(image) = self.images.iter().find(|i| i.id == seed.image) else {
            issues.push(
                Issue::new(
                    format!(
                        "`seed` names `{}`, which is not a declared image",
                        seed.image
                    ),
                    &self.repo_src,
                )
                .help("a repository publishes a seed of one of the images declared at its root"),
            );
            return;
        };

        let declared = |name: &str| self.sources.iter().any(|c| c.name == name);
        if !declared(&seed.collection) {
            issues.push(
                Issue::new(
                    format!(
                        "`{}` is not a collection this repository declares",
                        seed.collection
                    ),
                    &self.repo_src,
                )
                .help(
                    "a repository is seedable only if it publishes its own modules/ as a \
                     collection, and declares it in `sources` under the owner they are imported \
                     as: that is what a seeded repository fetches them through",
                ),
            );
        }

        for entry in &image.entries {
            let owner = entry
                .qualified(&seed.collection)
                .and_then(|name| name.split('/').next().map(str::to_string));
            match &owner {
                Some(owner) if declared(owner) => continue,
                _ => {}
            }
            issues.push(
                Issue::new(
                    format!("`{}` is in no collection the seed can name", entry.path),
                    &image.src,
                )
                .at(
                    entry.span,
                    match owner {
                        Some(owner) => format!("`{owner}` is not declared in `sources`"),
                        None => "pinned to a source of its own".to_string(),
                    },
                )
                .help(
                    "a seed lists a module by name and nothing else, so one nothing can import \
                     leaves a seeded repository unbuildable",
                ),
            );
        }
    }

    /// `workflows at="12:30" { build; smoke-test }` Each child names a
    /// workflow by its file stem, and `at` is the one time the rest hang off.
    fn parse_workflows(&mut self, block: &KdlNode, src: &Source, issues: &mut Issues) {
        if let Some(at) = prop(block, "at") {
            match time(at) {
                Some(at) => self.workflows_at = at,
                None => issues.push(
                    Issue::new(format!("`{at}` is not a time of day"), src)
                        .at(prop_span(block, "at").unwrap_or_default(), "not `HH:MM`")
                        .help(
                            "`workflows at=\"12:30\"`, the hour and minute the daily build \
                               runs, UTC",
                        ),
                ),
            }
        }
        if let Some(publish) = prop(block, "publish") {
            match publish {
                "scheduled" => self.publishes_scheduled = true,
                _ => issues.push(
                    Issue::new(format!("`{publish}` is not a publish cadence"), src)
                        .at(
                            prop_span(block, "publish").unwrap_or_default(),
                            "not `scheduled`",
                        )
                        .help(
                            "`workflows publish=\"scheduled\"`, to publish only on the daily \
                             build; omit `publish` to publish on pushes too",
                        ),
                ),
            }
        }
        if let Some(scan) = prop(block, "scan") {
            match scan {
                "scheduled" => self.scans_scheduled = true,
                _ => issues.push(
                    Issue::new(format!("`{scan}` is not a scan cadence"), src)
                        .at(
                            prop_span(block, "scan").unwrap_or_default(),
                            "not `scheduled`",
                        )
                        .help(
                            "`workflows scan=\"scheduled\"`, to scan only on the daily build; \
                               omit `scan` to scan on pushes too",
                        ),
                ),
            }
        }
        for node in kids(block) {
            let name = node.name().value().to_string();
            let span: Span = node.name().span().into();

            if let Some(dup) = self.workflows.iter().find(|w| w.name == name) {
                issues.push(
                    Issue::new(format!("workflow `{name}` is declared twice"), src)
                        .at(dup.span, "first here")
                        .at(span, "and again here")
                        .help("a workflow is either generated or absent, so naming it twice says nothing the once did not"),
                );
                continue;
            }

            self.workflows.push(Workflow { name, span });
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

/// `HH:MM`, as cron's hour and minute.
pub fn time(value: &str) -> Option<(u32, u32)> {
    let (hour, minute) = value.split_once(':')?;
    let (hour, minute) = (hour.parse().ok()?, minute.parse().ok()?);
    (hour < 24 && minute < 60).then_some((hour, minute))
}

/// Where the `workflows` block sits, for the one command that rewrites a node
/// it did not write.
pub fn workflows_span(text: &str) -> Option<Span> {
    let doc: KdlDocument = text.parse().ok()?;
    let node = doc
        .nodes()
        .iter()
        .find(|n| n.name().value() == "workflows")?;
    Some(node.span().into())
}

/// The `at` a repository declaring the default writes, which is what `set
/// workflows` puts back and what a hand-edit is compared against.
pub fn at_text((hour, minute): (u32, u32)) -> String {
    format!("{hour:02}:{minute:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repo.kdl holding `text` and nothing else, since both readers under
    /// test take a root rather than a document.
    fn root(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tect-pin-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp root");
        std::fs::write(dir.join(layout::REPO_FILE), text).expect("repo.kdl");
        dir
    }

    #[test]
    fn a_pin_naming_another_release_is_reported_and_this_one_is_not() {
        let other = root("other", "schema-version 1\ntect-version \"0.0.1\"\n");
        assert_eq!(pinned_elsewhere(&other).as_deref(), Some("0.0.1"));
        let ours = root(
            "ours",
            &format!("schema-version 1\ntect-version \"{TECT_VERSION}\"\n"),
        );
        assert_eq!(pinned_elsewhere(&ours), None);
        let none = root("none", "schema-version 1\n");
        assert_eq!(pinned_elsewhere(&none), None);
    }

    #[test]
    fn a_pin_naming_another_release_refuses_nothing() {
        let dir = root("open", "schema-version 1\ntect-version \"0.0.1\"\n");
        let issues = compatible(&dir);
        assert!(issues.is_empty(), "{}", issues.plain());
    }

    #[test]
    fn a_declared_sha256_is_the_only_verifier() {
        let hash = "a".repeat(64);
        let declared = root(
            "sha",
            &format!(
                "schema-version 1\n  tect-version \"{TECT_VERSION}\" sha256=\"{hash}\"   // pinned\n"
            ),
        );
        assert_eq!(pinned_unverified(&declared), None);
        let bare = root("bare", "schema-version 1\ntect-version \"0.0.1\"\n");
        assert_eq!(pinned_unverified(&bare).as_deref(), Some("0.0.1"));
        let none = root("none", "schema-version 1\n");
        assert_eq!(pinned_unverified(&none), None);
    }

    #[test]
    fn malformed_tect_hashes_are_issues() {
        for sha256 in ["", "bad"] {
            let text = format!(
                "schema-version 1\ntect-version \"{TECT_VERSION}\" sha256=\"{sha256}\"\nname \"Example\"\n"
            );
            let mut list = List::empty(Path::new("."));
            let mut issues = Issues::default();
            list.parse_file(
                &Source::new(layout::REPO_FILE, &text),
                &text,
                true,
                &mut issues,
            );
            let found = issues.plain();
            assert!(
                found.contains("`tect-version` has a malformed sha256"),
                "{found}"
            );
        }
    }

    fn messages(text: &str) -> Vec<String> {
        let doc: KdlDocument = text.parse().expect("valid KDL");
        let src = Source::new(layout::REPO_FILE, text);
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
name "Tectonic"
tect-version sha256=1 wat="x"
tect-version "0.0.0"
default-image
pr-image
workflows every="day" {
    smoke-test "on" trigger="push"
    build
}
colour "blue"
"#,
        );
        assert_eq!(
            found,
            [
                "`schema-version` needs a number",
                "`schema-version` is declared twice",
                "`tect-version` needs a release",
                "`sha256` must be a hash",
                "unknown tect-version property `wat`",
                "`tect-version` is declared twice",
                "`default-image` needs an image name",
                "`pr-image` needs an image name",
                "unknown workflows property `every`",
                "a workflow takes no arguments",
                "unknown workflow property `trigger`",
                "unknown node `colour` in repo.kdl",
            ]
        );
    }

    #[test]
    fn an_empty_workflows_block_is_a_block_with_nothing_in_it() {
        let found = messages("schema-version 1\nname \"Tectonic\"\nworkflows { }\n");
        assert_eq!(found, ["`workflows` has no workflows in it"]);
    }

    #[test]
    fn publish_has_one_cadence() {
        let read = |value: &str| {
            let text = format!(
                "schema-version 1\nname \"Tectonic\"\nworkflows publish=\"{value}\" {{ build }}\n"
            );
            let mut list = List::empty(Path::new("."));
            let mut issues = Issues::default();
            list.parse_file(
                &Source::new(layout::REPO_FILE, &text),
                &text,
                true,
                &mut issues,
            );
            (list.publishes_scheduled, issues.plain())
        };
        assert_eq!(read("scheduled"), (true, String::new()));
        let (scheduled, issues) = read("push");
        assert!(!scheduled);
        assert!(
            issues.contains("`push` is not a publish cadence"),
            "{issues}"
        );
    }

    #[test]
    fn manifest_label_is_off_unless_declared() {
        let read = |text: &str| {
            let mut list = List::empty(Path::new("."));
            let mut issues = Issues::default();
            list.parse_file(
                &Source::new(layout::REPO_FILE, text),
                text,
                true,
                &mut issues,
            );
            list.manifest_label
        };
        assert!(!read("schema-version 1\n"));
        assert!(read("schema-version 1\nmanifest {\n    label #true\n}\n"));
    }

    /// The collection table, which the broken fixture reaches the meaning of
    /// but not the shape.
    #[test]
    fn a_collection_is_a_location_and_what_pins_it() {
        let found = messages(
            r#"
schema-version 1
name "Tectonic"
sources {
    owner branch="main" {
        pin {
            unpinned
            version "v1"
            version "v2"
        }
        subtree "modules"
    }
}
"#,
        );
        assert_eq!(
            found,
            [
                "unknown collection property `branch`",
                "`unpinned` needs a reason",
                "`version` is declared twice",
                "unknown node `subtree` in a collection",
            ]
        );
    }
}

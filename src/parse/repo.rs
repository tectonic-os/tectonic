//! repo.kdl, and the walk over every image file beside it.

use crate::diag::{Issue, Issues, Source, Span};
use crate::layout;
use crate::model::image::{List, NetRule, Network, Seed, Workflow, SCHEMA_VERSION, TECT_VERSION};
use crate::parse::image::IMAGE;
use crate::parse::remote::{parse_collection, COLLECTION};
use crate::parse::schema::{check_doc, Arg, Kind, Node, Prop, Say};
use crate::parse::{
    bool_arg, check_sha256, child, int_arg, kids, prop, prop_span, string_arg, syntax_issue,
};
use kdl::{KdlDocument, KdlNode};
use std::path::Path;

/// What a module layer's steps may reach, under `security-policy`. Lifted out
/// of `REPO` so the policy block holds it by name.
#[rustfmt::skip]
const NETWORK: Node = Node::new("network", "The rule that decides whether each step of a module layer reaches the network.").about("Controls network access while the image builds. The user can open the network to every step, or with `strict` open it only for the modules that declare a need, so an undeclared download fails the build. `packages` and `scripts` set the rule for one kind of step.").example("\"strict\"")
    .arg(Arg::MaybeOne(&["allow", "strict"]), Say::new("`{}` is not a network rule",
        "not a rule",
        "`network \"strict\"` holds every module to what it declares; a block sets \
         `packages` and `scripts` one at a time"))
    .once("a second rule would contradict the first")
    .children(&[
        Node::new("packages", "The rule that every package step runs under.").example("\"allow\"")
            .arg(Arg::One(&["allow", "strict", "deny"]), Say::new(
                "`{}` is not a network rule", "not a rule",
                "`packages` takes `allow`, `strict` or `deny`"))
            .once("")
            .values(&[
                ("`allow`", "Every package step reaches the network."),
                ("`strict`", "Every package step reaches the network, because a package step \
                  runs only where the module declares packages, a COPR or a repo file."),
                ("`deny`", "Every package step runs under `--network=none`."),
            ]),
        Node::new("scripts", "The rule that every script step runs under.").example("\"strict\"")
            .arg(Arg::One(&["allow", "strict", "deny"]), Say::new(
                "`{}` is not a network rule", "not a rule",
                "`scripts` takes `allow`, `strict` or `deny`; under `strict` a module \
                 opens its script step with `network \"scripts\"`"))
            .once("")
            .values(&[
                ("`allow`", "Every script step reaches the network."),
                ("`strict`", "A script step reaches the network only if its module declares \
                  `network \"scripts\"`."),
                ("`deny`", "Every script step runs under `--network=none`."),
            ])
            .notes(&[
                "If the rule is `strict` or `deny`, then `tect check` names each module that \
                 ships a Containerfile fragment. The rule does not reach the `RUN` lines of a \
                 fragment.",
            ]),
    ], Say::new("unknown node `{}` in network", "not part of the schema",
        "a network block holds `packages` and `scripts`"))
    .lists(&[
        ("A module layer runs two kinds of step:", &[
            "A package step installs the repo files, the COPRs and the packages of the module.",
            "A script step runs the `module.sh` of the module and its finalize hook.",
        ]),
    ])
    .values(&[
        ("`allow`", "Both kinds of step take `allow`, unless `packages` or `scripts` sets \
          another rule."),
        ("`strict`", "Both kinds of step take `strict`, unless `packages` or `scripts` sets \
          another rule."),
        ("No value", "Both kinds of step take `allow`, unless `packages` or `scripts` sets \
          another rule."),
    ]);

/// repo.kdl's grammar, and the whole of it.
#[rustfmt::skip]
pub const REPO: Node = Node::new("repo",
    "What is true of the repository as a whole, apart from its images.").about("`repo.kdl` sits at the root of the repository and holds what applies to every image in it: which `tect` release builds it, where its modules come from, which CI workflows run and which security rules every image follows. The images themselves live in image files beside it.").scaffolds(&["schema-version", "name", "sources", "workflows"])
    .children(&[
        Node::new("schema-version", "The schema release that this repository is written against.").about("The schema version tells `tect` which reader to use for the repository. A repository written against an earlier release keeps building, because the version picks the reader. `tect create repo` writes it, and `tect` never changes it.").example("1")
            .arg(Arg::Int, Say::new("`{}` needs a number", "not a version", "`schema-version 1`"))
            .missing(Say::new("repo.kdl declares no `{}`", "no schema version",
                "`schema-version 1`, so a tool from a different release says so plainly. Without \
                 it, the tool reports every node it does not recognise"))
            .once("")
            .notes(&[
                "The version picks the reader, so a repository written against an earlier \
                 release keeps building.",
                "If `tect` does not read the version, then it says so once and reports no node \
                 it cannot place.",
                "`tect` does not move a repository to a newer version. The release that the \
                 repository pins reads it.",
            ]),
        Node::new("tect-version", "The `tect` release that builds this repository.").about("Pins the `tect` release that builds the repository, so every build uses the same tool. `tect.sh` fetches the pinned release, so a build does not depend on the `tect` installed on the machine.").example("\"0.6.49\"")
            .arg(Arg::Str, Say::new("`{}` needs a release", "not a version",
                concat!("`tect-version \"", env!("CARGO_PKG_VERSION"),
                        "\"`, the release the build fetches; another release reads the \
                         repository and says so")))
            .props(&[
                Prop { name: "sha256", kind: Kind::Str,
                    desc: "The sha256 that `tect.sh` holds the release tarball to.",
                    say: Say::new("`{}` must be a hash", "not a string", ""),
                    missing: Say::NONE },
            ], Say::new("unknown tect-version property `{}`", "not part of the schema",
                "a `tect-version` accepts `sha256`"))
            .once("")
            .lists(&[
                ("If a different release of `tect` reads the repository:", &[
                    "`tect` says once that the pin names a different release.",
                    "`schema-version` decides whether that release can read the repository.",
                    "`tect verify` reports the difference as drift.",
                    "`tect generate` resolves the drift.",
                ]),
            ])
            .notes(&[
                "`tect.sh` fetches this release, so the build does not use the `tect` \
                 installed on the machine.",
                "If `sha256=` is absent, then `tect.sh` checks the checksum published \
                 beside the tarball. That check proves only that the download is complete.",
                "If the repository pins no release, then `tect` says nothing about the release.",
            ]),
        Node::new("name", "An optional human-readable label for the repository.").about("A human-readable label for the repository. It labels the result tree printed by commands that add or change files, and does not affect generation or builds. When it is omitted, that tree uses the repository directory name.").example("\"Workstation\"")
            .arg(Arg::Str, Say::new("`{}` needs a name", "no name given",
                "`name \"Workstation\"`, which the directory is free to disagree with"))
            .once(""),
        Node::new("default-image",
            "The image that a command with no image, or a build with no target, uses.").about("If the repository defines more than one image, then this names the image that a command uses when the user names none. If the repository defines one image, then that image is the default and this field is not needed.").example("\"workstation\"")
            .arg(Arg::Str, Say::new("`{}` needs an image name", "no image given",
                "`default-image \"workstation\"`, naming one of the images declared at the root"))
            .once(""),
        Node::new("pr-image", "The image a pull request builds.").example("\"workstation\"")
            .arg(Arg::Str, Say::new("`{}` needs an image name", "no image given",
                "`pr-image \"workstation\"`, since a pull request builds one target"))
            .once(""),
        Node::new("seed",
            "The image that this repository publishes as a starting point for a new repository.").about("A seed lets another user start a new repository from this one. `tect generate` writes the base, the module list and the collections of the named image into the seed, so a new repository starts from the same image.").example("\"workstation\" collection=\"owner\"")
            .arg(Arg::Str, Say::new("`{}` needs an image name", "no image given",
                "`seed \"workstation\" collection=\"owner\"`, naming one of the images declared \
                 at the root"))
            .once("a repository publishes one seed")
            .props(&[
                Prop { name: "collection", kind: Kind::Str,
                    desc: "The collection that this repository publishes its own modules as.",
                    say: Say::new("`{}` must be a collection name", "not a string", ""),
                    missing: Say::new("`{}` says nothing about where its modules are published",
                        "no `collection`",
                        "`{} collection=\"owner\"`, one of the collections in `sources`: every \
                         module in a seed is fetched through one, so a repository publishing no \
                         collection of its own has nothing a seeded repository can import") },
            ], Say::new("unknown seed property `{}`", "not part of the schema",
                "a seed accepts `collection`"))
            .lists(&[
                ("`tect generate` writes the seed to `generated/seed.kdl`. The seed carries:", &[
                    "the base of the image;",
                    "the module list of the image;",
                    "the collections that those modules come from.",
                ]),
            ])
            .notes(&[
                "The seed carries no name, URL or owner of this repository.",
                "The seed names each module through the collection it is fetched from, so \
                 `collection=` names a collection in `sources`. A repository is seedable only \
                 if it publishes its own `modules/` as a collection.",
                "If the seed image lists a module that no collection can import, then `tect` \
                 reports it, because a new repository could not build that seed.",
            ]),
        Node::new("workflows", "The CI workflows that `tect generate` writes into `.github/workflows/`.").about("Names the GitHub Actions workflows that `tect generate` writes and keeps current, and sets when scheduled builds, publishing and scans run. The repository gets only the workflows named here.").scaffolds(&[""]).example("at=\"12:30\"")
            .once("a second block would split one set of workflows in two")
            .empty(Say::new("`workflows` has no workflows in it", "empty block",
                "omit the block entirely; a repository with nothing here generates no CI"))
            .props(&[
                Prop { name: "at", kind: Kind::Str,
                    desc: "The hour and minute in UTC that the daily build runs at. Every other \
                        scheduled workflow runs at its own offset from it.",
                    say: Say::new("`{}` must be a time of day", "not a string", ""),
                     missing: Say::NONE },
                Prop { name: "publish", kind: Kind::One(&["scheduled"]),
                    desc: "If `scheduled`, then images publish from the daily build alone, and \
                        image scans move off pushes too, because a scan reads a published image. \
                        If absent, then images also publish on pushes.",
                    say: Say::new("`{}` is not a publish cadence", "not `scheduled`",
                        "`workflows publish=\"scheduled\"`, to publish only on the daily build; \
                         omit `publish` to publish on pushes too"),
                    missing: Say::NONE },
                Prop { name: "scan", kind: Kind::One(&["scheduled"]),
                    desc: "If `scheduled`, then image scans run on the daily build alone. If \
                        absent, then scans run on pushes and on scheduled builds.",
                    say: Say::new("`{}` is not a scan cadence", "not `scheduled`",
                        "`workflows scan=\"scheduled\"`, to scan only on the daily build; \
                         omit `scan` to scan on pushes too"),
                    missing: Say::NONE },
            ], Say::new("unknown workflows property `{}`", "not part of the schema",
                "a workflows block accepts `at`, `publish` and `scan`"))
            .children(&[
                Node::new("", "One workflow, which the stem of its file names.").example("build")
                    .arg(Arg::None, Say::new("a workflow takes no arguments", "unexpected value",
                        "the file stem is the node name: `smoke-test`"))
                    .props(&[], Say::new("unknown workflow property `{}`", "not part of the schema",
                        "a workflow is named and nothing else; `at` on the block moves every \
                         schedule at once")),
            ], Say::NONE)
            .notes(&[
                "`tect generate` writes only the workflows named here. After a tool upgrade, it \
                 rewrites each named workflow.",
                "`tect set workflows` edits this block.",
                "The `conforms` of each image decides whether SCAP content exists to scan.",
            ]),
        Node::new("sources",
            "The module collections that `tect import module` and `tect copy module` resolve \
             against.").about("Lists the module collections that this repository takes modules from. A collection is a shared set of modules, which `tect` fetches as a pinned archive or reads from a directory on this machine. `tect import module` and `tect copy module` look modules up here.").scaffolds(&[""])
            .once("a second block would split one registry in two")
            .empty(Say::new("`sources` has no collections in it", "empty block",
                "omit the block entirely; a repository with nothing here references or copies from nothing"))
            .children(&[COLLECTION], Say::NONE)
            .lists(&[
                ("A module from a collection reaches the repository in one of two ways:", &[
                    "`tect import module` references the module under the name of its \
                     collection.",
                    "`tect copy module` puts the module unqualified at `modules/<name>`, and \
                     `provenance.kdl` records the collection.",
                ]),
            ]),
        Node::new("manifest",
            "Whether a build stamps the generated manifest onto the image as an OCI label.").about("The build writes a manifest of what went into the image into the image itself. This block decides whether the build also stamps an OCI label that points at that file, so a tool that reads image labels can find it.")
            .once("a second block would split one setting in two")
            .empty(Say::new("`manifest` has no `label` in it", "empty block",
                "omit the block entirely; a build with nothing here stamps no label"))
            .children(&[
                Node::new("label",
                    "Whether the build stamps `org.tectonic.manifest` with the path of the \
                     baked manifest file.").example("#true")
                    .arg(Arg::Bool, Say::new("`label` needs #true or #false", "not a boolean",
                        "`label #true` stamps the built image with an `org.tectonic.manifest` \
                         label"))
                    .once(""),
            ], Say::new("unknown node `{}` in manifest", "not part of the schema",
                "a manifest block holds `label`")),

        Node::new("audit", "How the repository treats a provenance check that fails.").about("Every build records where its parts came from. This block decides whether a failed provenance check is only reported or stops the command, so a repository can move from reporting to enforcing when it is ready.")
            .once("a second block would split one posture in two")
            .empty(Say::new("`audit` has no `enforce` in it", "empty block",
                "omit the block entirely; a repository with nothing here records every \
                 provenance fact and fails on none of them"))
            .children(&[
                Node::new("enforce", "Whether a failed provenance check stops the command that runs it.").example("#true")
                    .arg(Arg::Bool, Say::new("`enforce` needs #true or #false", "not a boolean",
                        "`enforce #true` makes an unverified import, a module edited since \
                         import, a base that will not resolve and an unstamped build into \
                         errors"))
                    .once("")
                    .values(&[
                        ("`#true`", "A failed check is an error, and the command stops."),
                        ("`#false`, or no `enforce`", "`tect` reports a failed check, and the \
                          command continues."),
                    ])
                    .lists(&[
                        ("Each check runs in the command named before it:", &[
                            "`tect import module` and `tect copy module`: the collection is \
                             pinned to a tag and its hash.",
                            "`tect check`: each imported module still matches its import record.",
                            "`tect build`: the base tag resolves to a manifest digest.",
                            "`tect build`: the repository is at a commit.",
                            "`tect scap`: the scan report passes every rule of the profile.",
                        ]),
                    ])
                    .notes(&[
                        "`tect` also reports a `module.sh` or `finalize.sh` that reaches the \
                         network with no `asset` that declares the download. This report does \
                         not depend on `enforce`.",
                    ]),
            ], Say::new("unknown node `{}` in audit", "not part of the schema",
                "an audit block holds `enforce`"))
            .lists(&[
                ("`tect` records these provenance facts whether or not this block exists:", &[
                    "the hash of each module;",
                    "the source of each import;",
                    "the digest of the base tag;",
                    "the commit of each cloned asset.",
                ]),
            ]),
        Node::new("security-policy",
            "The security rules that the repository sets for every image it defines.").about("Sets the security rules that every image in the repository follows, whatever each image declares. It holds one rule: what each build step can reach on the network.")
            .once("a second block would contradict the first")
            .children(&[NETWORK],
                Say::new("unknown node `{}` in security-policy", "not part of the schema",
                    "a security-policy block holds a `network` rule"))
            .notes(&[
                "If the `security-policy` block is absent, then every step keeps the network.",
            ]),
    ], Say::new("unknown node `{}` in repo.kdl", "not part of the schema",
        "repo.kdl holds `schema-version`, `tect-version`, `name`, `default-image`, `pr-image`, \
         `seed`, a \
         `workflows` block, a `sources` block, a `manifest` block, an `audit` block and a \
         `security-policy` block: what is true of the \
         repository. An image goes in a file of its own"));

/// The node's argument sets both kinds, and a child of its block overrides one
/// kind. The walker reports a value outside the grammar, so this reader leaves
/// that kind at its default.
fn network(node: &KdlNode) -> Network {
    let both = string_arg(node).and_then(NetRule::of).unwrap_or_default();
    let kind = |name| child(node, name).and_then(string_arg).and_then(NetRule::of);
    Network {
        packages: kind("packages").unwrap_or(both),
        scripts: kind("scripts").unwrap_or(both),
    }
}

/// `image.kdl` or `<name>.image.kdl` at the root, holding whatever images it
/// likes.
#[rustfmt::skip]
pub(crate) const IMAGE_FILE: Node = Node::new("image file",
    "The images that a file named `image.kdl` or `<name>.image.kdl` holds.").about("An image file holds one or more images. Each image says what it is called, which base it builds on and which modules it is made of.").scaffolds(&["image"])
    .children(&[IMAGE], Say::new("unknown top-level node `{}`", "not part of the schema",
        "an image file holds `image` nodes and nothing else; `base`, `flavours` and `modules` are \
         declared inside one, because they are what the image is"))
    .notes(&[
        "A root `.kdl` file is an image file only if it is named `image.kdl` or ends in \
         `.image.kdl`. `tect` reports each other root `.kdl` file and does not read it.",
        "The part of the file name in front of `.image.kdl` does not name the image. An image is \
         called what it declares, so one file can hold as many images as suit the repository. \
         `plan.json` records the file name only to say where each image is declared.",
    ]);

/// What repo.kdl declares about which tool reads it.
struct Pins {
    schema: Option<(i128, Span)>,
    tect: Option<(String, Span)>,
    tect_sha: Option<String>,
    src: Source,
}

/// Read directly, and before anything else, because they decide whether this
/// release reads the rest at all. A repo.kdl that is missing, unparseable or
/// declares neither falls through to the reader, which is what reports it.
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
/// one schema, so a repository written against another is refused. A pin naming
/// another release says nothing about whether the declarations here can be
/// read — see `pinned_elsewhere`.
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
                         that reads it, and `tect.sh` fetches that one"
                    .to_string(),
                false => format!(
                    "nothing here moves a repository to schema {SCHEMA_VERSION}, and nothing \
                     else in it is read either, because every diagnostic under a grammar this \
                     release does not have would be noise; run the release `tect-version` names, \
                     which `tect.sh` fetches"
                ),
            })
            .blocks_edit(),
        );
        return issues;
    }

    issues
}

/// The release a repository pins, when that is not this one.
///
/// A notice. A pin protects generated output: a different release writes
/// different workflow bodies, and `verify` already reports that as drift with
/// `generate` to resolve it. `schema-version` is what decides whether the
/// declarations parse.
pub fn pinned_elsewhere(root: &Path) -> Option<String> {
    let version = pins(root)?.tect?.0;
    (version != TECT_VERSION).then_some(version)
}

/// A release pinned with no declared sha256, which `tect.sh` then
/// holds to the checksum fetched beside the tarball. `check` reports it;
/// nothing refuses, since a repository predating the first release that carries
/// one declares none.
pub fn pinned_unverified(root: &Path) -> Option<String> {
    let pins = pins(root)?;
    pins.tect
        .filter(|_| pins.tect_sha.is_none())
        .map(|(version, _)| version)
}

/// The collections a `sources` block declares, read out of text: what a
/// repository that is not written yet will have, which is the only thing
/// `create repo` can offer modules against. Anything wrong with the block is
/// reported where the repository is read.
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

    pub(crate) fn editable(root: &Path) -> Result<Self, Issues> {
        let (list, issues) = Self::load(root);
        let blocking = issues.blocking_edits();
        match blocking.is_empty() {
            true => Ok(list),
            false => Err(blocking),
        }
    }

    pub(super) fn empty(root: &Path) -> Self {
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
            network: Network::default(),
            schema_version: None,
            repo_src: Source::new(root.join(layout::REPO_FILE).display().to_string(), ""),
            files: Vec::new(),
            capabilities: Vec::new(),
        }
    }

    /// repo.kdl, which is repo context, and every image file beside it. A root
    /// `.kdl` that is neither is nobody's, and is reported.
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
                issues.push(
                    Issue::new(
                        format!("cannot read {}: {err}", root.display()),
                        &list.repo_src,
                    )
                    .blocks_edit(),
                );
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
                    issues.push(
                        Issue::new(
                            format!("cannot read {path}: {err}"),
                            &Source::new(&path, ""),
                        )
                        .blocks_edit(),
                    );
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
        // `check` reports what is wrong with the catalog; this only reads it.
        list.capabilities = crate::base::capabilities(root, &list.sources, &mut Issues::default());
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
                (true, "security-policy") => {
                    self.network = child(node, "network").map(network).unwrap_or_default()
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
        // The grammar reports a cadence other than `scheduled`.
        self.publishes_scheduled = prop(block, "publish") == Some("scheduled");
        self.scans_scheduled = prop(block, "scan") == Some("scheduled");
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

    #[test]
    fn a_cadence_other_than_scheduled_is_refused() {
        let text = "workflows publish=\"weekly\" scan=\"daily\" {\n    build\n}\n";
        let issues = crate::parse::schema::check_text(text, &REPO, false)
            .expect("the text is KDL")
            .plain();
        assert!(
            issues.contains("`weekly` is not a publish cadence"),
            "{issues}"
        );
        assert!(issues.contains("`daily` is not a scan cadence"), "{issues}");
    }
    use super::*;

    /// A repo.kdl holding `text` and nothing else, since both readers under
    /// test take a root.
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
        // Its own directory: `root` writes at a fixed path, and two tests
        // sharing one race each other's `remove_dir_all`.
        let none = root("unpinned", "schema-version 1\n");
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
        issues.findings()
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
    fn a_repository_name_is_optional() {
        assert!(messages("schema-version 1\n").is_empty());
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

    /// The reader is given the `network` node out of `security-policy`, so
    /// these texts are that node alone.
    #[test]
    fn a_network_block_overrides_the_rule_one_kind_at_a_time() {
        let read = |text: &str| {
            let doc: KdlDocument = text.parse().expect("valid KDL");
            network(&doc.nodes()[0])
        };
        assert_eq!(read("network\n"), Network::default());
        assert_eq!(
            read("network \"strict\" {\n    packages \"allow\"\n}\n"),
            Network {
                packages: NetRule::Allow,
                scripts: NetRule::Strict,
            }
        );
    }

    #[test]
    fn a_network_rule_outside_the_set_is_refused() {
        let found = messages(
            "schema-version 1\nname \"Tectonic\"\nsecurity-policy {\n    network \"block\"\n}\n",
        );
        assert_eq!(found, ["`block` is not a network rule"]);
    }

    /// `network #false` reads as a closed network to the user, and the reader
    /// would take it as the allow rule.
    #[test]
    fn a_network_rule_that_is_not_a_string_is_refused() {
        let found = messages(
            "schema-version 1\nname \"Tectonic\"\nsecurity-policy {\n    network #false\n}\n",
        );
        assert_eq!(found, ["`#false` is not a network rule"]);
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

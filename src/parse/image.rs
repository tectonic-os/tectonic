//! The `image` node: what an image calls itself, builds on, and is made of.

use crate::diag::{Issue, Issues, Source, Span};
use crate::model::component::Refusal;
use crate::model::image::{is_name, Decl, Entry, Flavour, Image};
use crate::model::image::{Layout, List, NO_FLAVOUR};
use crate::parse::prop;
use crate::parse::schema::{Arg, Kind, Node, Prop, Say, NEEDS_VALUE};
use crate::parse::{bool_arg, child, flag, kids, options};
use crate::parse::{string_arg, string_args, text};
use kdl::{KdlDocument, KdlNode};

/// A path segment, which is the same node ungated and inside a flavour block.
/// The image parse reads what its children mean, because a child is a
/// manifest node, an option or a further segment.
#[rustfmt::skip]
const SEGMENT: Node = Node::new("", "One part of a component path, which lists the component at that path or holds further parts.").about("Names one directory of a component path under `components/`. A segment lists the component at its path. If no component is there, then the segment only holds the segments below it. A child with an argument sets an option that the component declares, such as `version \"44\"`. A child with no argument is the next segment. A segment that holds a manifest node, such as `packages` or `base`, defines its component inline, and no directory is needed.").example("desktop-environment")
    .notes(&[
        "A component that the image file defines inline must not also be a component under \
         `components/`. `tect check` refuses the pair and names both.",
        "`tect import component` copies a component from a library into `components/` and \
         records where it came from in `components/components.lock.kdl`.",
        "The base is the listed component that declares `base`. Its segment also takes \
         `digest` and `signed`, which record the digest and the signature state of its \
         reference.",
    ]);

/// The root filesystems the installer formats.
const FILESYSTEMS: &[&str] = &["xfs", "ext4", "btrfs"];

/// The bootloaders the installer writes.
pub(crate) const BOOTLOADERS: &[&str] = &["grub2", "systemd"];

/// The UKI boot chains the image can declare.
pub(crate) const BOOT_CHAINS: &[&str] = &["uki-shim", "uki-db"];

/// The image file's grammar, and the whole of it.
#[rustfmt::skip]
pub const IMAGE: Node = Node::new("image",
    "One image, which declares its names and every component that it is made of, its base among them.").about("Declares one bootable operating system image: its names, how the installer lays it down and the components that make it up. Each component is a path of segments, such as `desktop-environment { kde-plasma }`, and one of them is the base that every layer builds on. The build publishes the image under its `id`.").scaffolds(&["name", "url", "issues-url", "layout", ""])
    .minimal()
    .arg(Arg::None, Say::new("`image` takes no argument", "the name belongs in the block",
        "`image { id \"{}\" }` is the machine name, and `name` is the human one it derives from \
         when absent"))
    .children(&[
        Node::new("id", "os-release `DEFAULT_HOSTNAME`. The machine name of the image.").example("\"workstation\"")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .lists(&[
                ("The `id` names:", &[
                    "the published image;",
                    "the build target;",
                    "the cache tag;",
                    "the os-release `DEFAULT_HOSTNAME`.",
                ]),
            ])
            .notes(&[
                "If `id` is absent, then `tect` derives it from `name` in lower case and turns \
                 each space into a dash.",
            ]),
        Node::new("name", "os-release `NAME`. The name that the boot menu and the desktop read.").example("\"Workstation\"")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .notes(&["If `pretty-name` is absent, then `PRETTY_NAME` is `name` followed by the image version."])
            .missing(Say::new("`image` declares no `name`", "no name",
                "`name \"Tectonic\"` is os-release NAME, which the boot menu and the desktop read")),
        Node::new("pretty-name", "os-release `PRETTY_NAME`. The full name that the user sees.").example("\"Workstation 44\"")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("url", "os-release `HOME_URL` and `DOCUMENTATION_URL`. The URL that serves the home page of the project.").example("\"https://github.com/owner/workstation\"")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("issues-url", "os-release `SUPPORT_URL` and `BUG_REPORT_URL`. Where the user reports a problem with the image.").example("\"https://github.com/owner/workstation/issues\"")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("description", "OCI label `org.opencontainers.image.description`. One line that summarises the image.").example("\"A KDE desktop for daily work\"")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("keywords", "OCI label `io.artifacthub.package.keywords`. The keywords of the image, which the build joins with commas.").example("\"desktop\" \"kde\"")
            .arg(Arg::Strs, Say::NONE),
        Node::new("logo-url", "OCI label `io.artifacthub.package.logo-url`. The URL that points at the logo of the image.").example("\"https://github.com/owner/workstation/raw/main/logo.svg\"")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("conforms", "The benchmark profile that a scan measures the ungated target against.").about("Names the benchmark profile that a scan measures the image against. Setting it turns the scan on: every build measures the image against the profile and publishes the score.").example("\"standard\"")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .notes(&["A scan reports the result. A failed rule fails `tect scap` only if \
                `audit { enforce #true }` is set."]),

        Node::new("boot-chain", "The UKI boot chain, which is owner-signed through shim or enrolled directly into Secure Boot.").about("Builds the image to boot as a unified kernel image (UKI) under Secure Boot. `uki-shim` boots through shim with a key the owner signs, and `uki-db` enrolls the key directly into the firmware key database.").example("\"uki-shim\"")
            .arg(Arg::One(BOOT_CHAINS), Say::new("`{}` is not a boot chain",
                "not a boot chain",
                "`boot-chain \"uki-shim\"` for owner-signed through shim, or `boot-chain \"uki-db\"` for \
                 direct enrollment into the firmware key database"))
            .once(""),

        Node::new("layout", "What the installer lays down if the answer of the base family is not the one that the image wants.").about("Each base family comes with installer defaults for the filesystem, composefs, the generic image and the administrator group. This block replaces those defaults for this image, so the installer lays the image down the way it needs. No family supplies a bootloader, so a declared `layout` names `bootloader`.").scaffolds(&["bootloader"])
            .arg(Arg::None, Say::new("`layout` takes no argument", "the declarations belong in the block",
                "`layout { filesystem \"btrfs\" }`"))
            .once("an image is installed one way")
            .empty(Say::new("`layout` declares nothing", "empty block",
                "omit the block to take the family's filesystem and no separate `/var`"))
            .children(&[
                Node::new("filesystem", "The root filesystem that the installer formats in place of the one that the base family settles.").example("\"btrfs\"")
                    .arg(Arg::One(FILESYSTEMS), Say::new("`{}` is not a filesystem the installer formats",
                        "not a filesystem",
                        "the installer auto-partitions `xfs`, `ext4` and `btrfs`"))
                    .once("")
                    .props(&[], Say::new("unknown filesystem property `{}`", "not part of the schema",
                        "`filesystem` takes only its filesystem name")),
                Node::new("composefs", "Whether the install seals the deployment, which replaces the answer of the base family.").example("#true")
                    .arg(Arg::Bool, Say::new("`composefs` needs #true or #false", "not a boolean",
                        "`composefs #true` passes `--composefs-backend`, which needs a \
                         filesystem with fs-verity"))
                    .once(""),
                Node::new("generic-image", "Whether the install skips the bootupd check, which replaces the answer of the base family.").example("#false")
                    .arg(Arg::Bool, Say::new("`generic-image` needs #true or #false", "not a boolean",
                        "`generic-image #true` passes `--generic-image`, which a base packaging \
                         no bootupd needs or the install aborts"))
                    .once(""),
                Node::new("admin-group", "The group that the installer creates an administrator in, which replaces the group of the base family.").example("\"wheel\"")
                    .arg(Arg::Str, Say::new("`admin-group` needs a group name", "no group given",
                        "`admin-group \"wheel\"`; `useradd` refuses the whole call when a listed \
                         group is missing on the target, so this names one"))
                    .once(""),
                Node::new("bootloader", "The bootloader that the installer installs. The catalog entry of the base lists the bootloaders that the base carries.").about("The bootloader that the installer writes to the disk. No base family supplies a default, so `tect check` fails a `layout` without one. If `boot` is set, then the bootloader is `systemd`. If neither is set, then the image builds no install media.").example("\"grub2\"")
                    .arg(Arg::One(BOOTLOADERS), Say::new("`{}` is not a bootloader the installer installs",
                        "not a bootloader",
                        "the installer writes `grub2` or `systemd`; no family answers this, so \
                         an image without it builds no install media"))
                    .once(""),
            ], Say::new("unknown layout property `{}`", "not part of the schema",
                "a layout accepts `filesystem`, `composefs`, `generic-image`, `admin-group` \
                 and `bootloader`")),

        Node::new("allow-remediation",
            "One rule an installed component refuses that this image lets remediation set anyway.").about("A component can refuse a hardening rule because the rule breaks what the component does. This lets one image accept that rule anyway, for a machine where the reason of the component does not apply. `because=` records why.").example("\"grub2_nousb_argument\" because=\"this machine has no USB keyboard\"")
            .arg(Arg::Str, Say::new("`allow-remediation` needs a rule ID", "nothing named",
                "`allow-remediation \"grub2_nousb_argument\" because=\"this machine has no USB \
                 keyboard\"`"))
            .unique(Say::new("`{}` is allowed twice", "already allowed above", ""))
            .props(&[
                Prop { name: "because", kind: Kind::Str,
                    desc: "Why this image overrides the component's judgement.",
                    say: Say::new("`because` must be a string", "not a string", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `allow-remediation` property `{}`", "not part of the schema",
                "`allow-remediation` accepts `because`")),

        Node::new("flavours", "The flavours that this image publishes beside its ungated build.").about("A flavour builds a variant of the same image with extra components, such as a `dev` build with developer tools, and publishes it as `<id>-<flavour>`. Flavours suit two builds that share almost every component.")
            .once("a second block would split one set of flavours in two")
            .empty(Say::new("`flavours` has no flavours in it", "empty block",
                "omit the block entirely to build one unnamed image"))
            .children(&[
                Node::new("",
                    "One flavour that publishes a gated component set as `<id>-<flavour>`, where \
                     `<flavour>` is the node name.").example("dev")
                    .arg(Arg::None, Say::new("a flavour takes no arguments", "unexpected value",
                        "the flavour's name is the node name: `desktop default=#true`"))
                    .props(&[
                        Prop { name: "default", kind: Kind::Bool,
                            desc: "Whether a build that names no flavour builds this one.",
                            say: Say::new("`{}` must be #true or #false", "not a boolean", ""),
                            missing: Say::NONE },
                        Prop { name: "pr-build", kind: Kind::Bool,
                            desc: "Whether a pull request builds this flavour in place of the \
                                   default.",
                            say: Say::new("`{}` must be #true or #false", "not a boolean", ""),
                            missing: Say::NONE },
                        Prop { name: "conforms", kind: Kind::Str,
                            desc: "The benchmark profile that a scan measures this flavour against.",
                            say: Say::new("`{}` must be a profile name", "not a string", ""),
                            missing: Say::NONE },
                    ], Say::new("unknown flavour property `{}`", "not part of the schema",
                        "a flavour accepts `default`, `pr-build` and `conforms`")),
            ], Say::NONE)
            .notes(&[
                "An image builds one target for its ungated component set and one more for each \
                 flavour. The targets publish as `<id>` and `<id>-<flavour>`.",
                "A flavour gates components inside one image. A component listed inside \
                 `flavour \"dev\"` builds only for that target, and each other component builds for \
                 every target.",
                "The `conforms=` of a flavour measures that flavour. The `conforms` of the image \
                 measures the ungated target only.",
            ]),

        Node::new("flavour", "The components one flavour adds, which build only for that flavour.").about("Holds the path segments of the components that build only for one flavour. Every component outside a `flavour` block builds for every target.").example("\"dev\"")
            .arg(Arg::Str, Say::new("`flavour` needs a flavour name", "no name given",
                "`flavour \"desktop\" { tools { devtools } }`")),

        SEGMENT,
    ], Say::NONE);

/// Answers whether `name` is a node of the image grammar. Every other node
/// inside `image` is a path segment.
pub fn is_keyword(name: &str) -> bool {
    IMAGE
        .children
        .iter()
        .any(|node| !node.name.is_empty() && node.name == name)
}

/// Answers whether `name` is a node of the manifest grammar, which makes the
/// segment that holds it an inline definition.
fn is_manifest_node(name: &str) -> bool {
    crate::parse::component::COMPONENT
        .children
        .iter()
        .any(|node| !node.name.is_empty() && node.name == name)
}

/// Answers whether a segment's child declares part of an inline definition.
/// A manifest node in use carries an argument, a property or a block. A bare
/// child is therefore a segment, even where a manifest node has its name.
fn declares(kid: &KdlNode) -> bool {
    is_manifest_node(kid.name().value()) && (!kid.entries().is_empty() || kid.children().is_some())
}

/// Answers whether a segment's child is the next segment. A child with an
/// argument sets an option, and a child that declares belongs to a definition.
fn is_segment(kid: &KdlNode) -> bool {
    !kid.entries().iter().any(|entry| entry.name().is_none()) && !declares(kid)
}

/// Finds the node at the end of `chain`, walking down from the image that
/// `image` names. An empty chain is the image node itself.
fn descend<'a>(
    doc: &'a KdlDocument,
    image: &str,
    chain: &[(&str, Option<&str>)],
) -> Option<&'a KdlNode> {
    let image = doc
        .nodes()
        .iter()
        .filter(|node| node.name().value() == "image")
        .find(|node| text(node, "name") == image)?;
    Some(trail(image, chain)?.pop().unwrap_or(image))
}

/// Finds the nodes that spell `chain` below `node`, outermost first. A path
/// may be spelled out in more than one segment of the same name, so each
/// candidate is tried in turn. A candidate with a block is tried first,
/// because only a block holds further nodes.
fn trail<'a>(node: &'a KdlNode, chain: &[(&str, Option<&str>)]) -> Option<Vec<&'a KdlNode>> {
    let Some(((name, arg), rest)) = chain.split_first() else {
        return Some(Vec::new());
    };
    let matching: Vec<&KdlNode> = kids(node)
        .iter()
        .filter(|kid| kid.name().value() == *name && (arg.is_none() || string_arg(kid) == *arg))
        .collect();
    let blocks = matching.iter().filter(|kid| kid.children().is_some());
    let bare = matching.iter().filter(|kid| kid.children().is_none());
    blocks.chain(bare).find_map(|kid| {
        let mut found = trail(kid, rest)?;
        found.insert(0, kid);
        Some(found)
    })
}

/// Removes the segment at the end of `chain` and keeps every other byte in
/// place. A segment that the removal empties goes too, because an empty
/// segment would list the component at its own path. If `held` answers that
/// a component is at the emptied segment's path, then the segment keeps its
/// name and loses only its empty block. A segment that holds further
/// segments is refused, because removing it would unlist them as well.
pub fn unlisted(
    kdl: &str,
    image: &str,
    chain: &[(&str, Option<&str>)],
    held: impl Fn(&str) -> bool,
) -> Result<String, String> {
    let doc: KdlDocument = kdl.parse().map_err(|_| "is not valid KDL".to_string())?;
    let path: Vec<&str> = chain
        .iter()
        .filter(|(_, arg)| arg.is_none())
        .map(|(name, _)| *name)
        .collect();
    let named = path.join("/");
    let nodes = doc
        .nodes()
        .iter()
        .filter(|node| node.name().value() == "image")
        .find(|node| text(node, "name") == image)
        .and_then(|node| trail(node, chain))
        .ok_or_else(|| format!("does not list `{named}` in `{image}`"))?;
    let target = nodes
        .last()
        .ok_or_else(|| format!("does not list `{named}` in `{image}`"))?;
    let below: Vec<&str> = kids(target)
        .iter()
        .filter(|kid| is_segment(kid))
        .map(|kid| kid.name().value())
        .collect();
    if !below.is_empty() {
        return Err(format!(
            "lists `{named}` with {} below it, so unlisting it would unlist them too",
            below.join(", ")
        ));
    }
    // A segment ancestor keeps its offset when a later node is cut, so each
    // one is found again by where it starts.
    let ancestors: Vec<(usize, String)> = nodes[..nodes.len() - 1]
        .iter()
        .zip(chain)
        .filter(|(_, (_, arg))| arg.is_none())
        .scan(Vec::new(), |parts: &mut Vec<&str>, (node, (name, _))| {
            parts.push(name);
            Some((Span::from(node.span()).offset, parts.join("/")))
        })
        .collect();
    let mut out = kdl.to_string();
    cut(&mut out, target.span().into());
    for (offset, path) in ancestors.iter().rev() {
        let Some(doc) = out.parse::<KdlDocument>().ok() else {
            break;
        };
        let Some(node) = starting_at(doc.nodes(), *offset) else {
            break;
        };
        if node
            .children()
            .is_none_or(|block| !block.nodes().is_empty())
        {
            break;
        }
        let span: Span = node.span().into();
        if held(path) {
            let end = span.offset + span.len;
            let open = out[span.offset..end]
                .rfind('{')
                .map_or(end, |at| span.offset + at);
            let from = out[..open].trim_end().len();
            out.replace_range(from..end, "");
            break;
        }
        cut(&mut out, span);
    }
    Ok(out)
}

/// Finds the node whose text starts at `offset`.
fn starting_at(nodes: &[KdlNode], offset: usize) -> Option<&KdlNode> {
    nodes
        .iter()
        .find_map(|node| match Span::from(node.span()).offset == offset {
            true => Some(node),
            false => starting_at(kids(node), offset),
        })
}

/// Removes one node's text. A node that shares its line with another one
/// loses only its own text.
fn cut(text: &mut String, span: Span) {
    let from = span.offset;
    // A node's span stops before its `;`, which goes with it on a shared line.
    let after = &text[span.offset + span.len..];
    let to = match after.trim_start_matches([' ', '\t']).strip_prefix(';') {
        Some(rest) => text.len() - rest.trim_start_matches([' ', '\t']).len(),
        None => span.offset + span.len,
    };
    let start = text[..from].rfind('\n').map_or(0, |at| at + 1);
    let end = text[to..].find('\n').map_or(text.len(), |at| to + at + 1);
    let alone = text[start..from].trim().is_empty() && text[to..end].trim().is_empty();
    // The last node on a shared line takes the blanks in front of it too.
    let last = text[to..end].trim().is_empty();
    match (alone, last) {
        (true, _) => text.replace_range(start..end, ""),
        (false, true) => {
            let from = text[..from].trim_end_matches([' ', '\t']).len();
            text.replace_range(from..to, "");
        }
        (false, false) => text.replace_range(from..to, ""),
    }
}

/// Where a declaration goes: the offset of the closing brace of the last block
/// on `chain`, walking down from the image `image` names. An empty chain is the
/// image's own brace. A node with no block has no brace, so it answers None.
/// Appending is a text splice over this offset.
pub fn block_close(kdl: &str, image: &str, chain: &[(&str, Option<&str>)]) -> Option<usize> {
    let doc: KdlDocument = kdl.parse().ok()?;
    let node = descend(&doc, image, chain)?;
    node.children()?;
    let span: Span = node.span().into();
    kdl[..span.offset + span.len].rfind('}')
}

/// Lists the flavour of each listing of `path` in the file, None for an
/// ungated one. A segment that only organises lists nothing, so the path
/// must end on a node that the parse reads as an entry.
pub fn listings(kdl: &str, path: &str) -> Vec<Option<String>> {
    let Ok(doc) = kdl.parse::<KdlDocument>() else {
        return Vec::new();
    };
    let src = Source::new("image.kdl", kdl);
    doc.nodes()
        .iter()
        .filter(|node| node.name().value() == "image")
        .flat_map(|node| Image::parse(node, &src, &mut Issues::default()).entries)
        .filter(|entry| entry.path == path && !entry.organises)
        .map(|entry| entry.flavour)
        .collect()
}

/// Reads back the nodes of an inline definition, which the entry holds by
/// their spans in the image file.
pub(crate) fn nodes_at(src: &Source, spans: &[Span]) -> Vec<KdlNode> {
    fn walk(nodes: &[KdlNode], spans: &[Span], out: &mut Vec<KdlNode>) {
        for node in nodes {
            if spans.contains(&Span::from(node.span())) {
                out.push(node.clone());
            }
            walk(kids(node), spans, out);
        }
    }
    let mut out = Vec::new();
    if let Ok(doc) = src.text().parse::<KdlDocument>() {
        walk(doc.nodes(), spans, &mut out);
    }
    out
}

/// The chain of segment nodes that spells a component path.
pub fn segments(path: &str) -> Vec<(&str, Option<&str>)> {
    path.split('/').map(|part| (part, None)).collect()
}

/// The span an image's `conforms` declaration takes, which is the node itself
/// where there is one and an empty span where the next one would go: in front
/// of the first path segment, and otherwise in front of the image's closing
/// brace. `None` only when the file declares no image by that name.
pub fn conforms_span(kdl: &str, image: &str) -> Option<Span> {
    let doc: KdlDocument = kdl.parse().ok()?;
    let node = descend(&doc, image, &[])?;
    let at = |want: &dyn Fn(&str) -> bool| -> Option<Span> {
        kids(node)
            .iter()
            .find(|kid| want(kid.name().value()))
            .map(|kid| kid.span().into())
    };
    match (
        at(&|name| name == "conforms"),
        at(&|name| !is_keyword(name) || name == "flavour"),
    ) {
        (Some(span), _) => Some(span),
        (None, Some(first)) => Some(Span {
            offset: first.offset,
            len: 0,
        }),
        (None, None) => block_close(kdl, image, &[]).map(|offset| Span { offset, len: 0 }),
    }
}

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
        self.images.push(Image::parse(node, src, issues));
    }
}

impl Image {
    /// Reads one `image` node. The base and the components load with the
    /// repository, because they need its manifests.
    fn parse(node: &KdlNode, src: &Source, issues: &mut Issues) -> Self {
        let mut image = Image {
            src: src.clone(),
            id: text(node, "id"),
            name: text(node, "name"),
            pretty_name: text(node, "pretty-name"),
            url: text(node, "url"),
            issues_url: text(node, "issues-url"),
            description: text(node, "description"),
            keywords: decls(node, "keywords")
                .into_iter()
                .map(|d| d.name)
                .collect(),
            logo_url: text(node, "logo-url"),
            conforms: text(node, "conforms"),
            boot: text(node, "boot-chain"),
            base: None,
            base_entry: None,
            defined: Vec::new(),
            layout: None,
            allows: kids(node)
                .iter()
                .filter(|kid| kid.name().value() == "allow-remediation")
                .filter_map(|kid| {
                    Some(Refusal {
                        rule: string_arg(kid)?.to_string(),
                        because: prop(kid, "because").unwrap_or_default().to_string(),
                        span: kid.name().span().into(),
                    })
                })
                .collect(),
            flavours: Vec::new(),
            entries: Vec::new(),
            suppressed: Vec::new(),
            span: node.name().span().into(),
        };

        if let Some(layout) = child(node, "layout") {
            image.parse_layout(layout);
        }
        if let Some(flavours) = child(node, "flavours") {
            image.parse_flavours(flavours, issues);
        }
        image.parse_segments(node, issues);

        image.check_id(issues);
        image.check_flavours(issues);
        image
    }

    fn parse_layout(&mut self, node: &KdlNode) {
        self.layout = Some(Layout {
            filesystem: text(node, "filesystem"),
            bootloader: text(node, "bootloader"),
            composefs: child(node, "composefs").and_then(bool_arg),
            generic: child(node, "generic-image").and_then(bool_arg),
            admin_group: text(node, "admin-group"),
            span: node.name().span().into(),
        });
    }

    /// The two things a declared layout can be wrong about.
    ///
    /// A sealed deployment needs fs-verity, and xfs has none. Refusing the
    /// pair here prevents an install that cannot boot.
    ///
    /// And an image on a family this project has not measured answers for
    /// itself, in full or not at all; on any family it names its bootloader.
    /// Half an answer is the one state that would reach `bootc install` on a
    /// default nobody chose. An image that declares no layout at all is left
    /// alone: it publishes, and it builds no install media.
    pub(super) fn check_layout(&mut self, issues: &mut Issues) {
        let src = &self.src.clone();
        let (Some(layout), Some(base)) = (&self.layout, &self.base) else {
            return;
        };
        let missing = crate::emit::recipe::unanswered(&base.family, Some(layout), &self.boot);
        if missing == ["bootloader"] {
            issues.push(
                Issue::new(format!("`{}` names no bootloader", self.id), src)
                    .at(layout.span, "no bootloader")
                    .help(
                        "no family answers the bootloader: `bootloader \"grub2\"` here, from what \
                         what the base component lists",
                    ),
            );
        } else if !missing.is_empty() {
            issues.push(
                Issue::new(
                    format!("`{}` declares half an install layout", self.id),
                    src,
                )
                .at(layout.span, format!("no {}", missing.join(", ")))
                .help(format!(
                    "nothing here has measured how `{}` installs, so this image answers for \
                     itself: a layout naming all of `filesystem`, `composefs`, `generic-image`, \
                     `admin-group` and `bootloader` builds install media, and no layout at all \
                     builds none",
                    base.family
                )),
            );
        }
        let mut conflicts = Vec::new();
        if !self.boot.is_empty() {
            if !layout.filesystem.is_empty() && layout.filesystem != "ext4" {
                conflicts.push("filesystem");
            }
            if layout.composefs == Some(false) {
                conflicts.push("composefs");
            }
            if layout.generic == Some(false) {
                conflicts.push("generic-image");
            }
            if !layout.bootloader.is_empty() && layout.bootloader != "systemd" {
                conflicts.push("bootloader");
            }
        }
        if !conflicts.is_empty() {
            issues.push(
                Issue::new(
                    format!("`{}` declares a layout that conflicts with its UKI chain", self.id),
                    src,
                )
                .at(layout.span, format!("conflicting {}", conflicts.join(", ")))
                .help("a UKI chain settles ext4, composefs, generic-image and systemd-boot; drop those layout rows or make them agree"),
            );
        }
        if layout.filesystem == "xfs"
            && self.boot.is_empty()
            && crate::emit::recipe::seals(&base.family, Some(layout), &self.boot)
        {
            issues.push(
                Issue::new(
                    format!("`{}` installs a sealed deployment onto xfs", self.id),
                    src,
                )
                .at(layout.span, "xfs has no fs-verity")
                .help(format!(
                    "the `{}` family installs with the composefs backend, which needs fs-verity: \
                     declare `ext4` or `btrfs`, or drop `filesystem` and take the family's own",
                    base.family
                )),
            );
        }
    }

    /// The id derives from `name` when it is not declared, and either way has to
    /// survive being an image tag.
    fn check_id(&mut self, issues: &mut Issues) {
        let src = &self.src.clone();
        if self.id.is_empty() {
            self.id = self.name.to_lowercase().replace(' ', "-");
            if !self.name.is_empty() && !is_name(&self.id) {
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
        } else if !is_name(&self.id) {
            issues.push(
                Issue::new(format!("invalid image name `{}`", self.id), src)
                    .at(self.span, "must be lowercase letters, digits and dashes, starting with a letter")
                    .help("it becomes an image tag, a cache tag and the default hostname, all of which restrict it"),
            );
        }
        // `generated/<id>/` is a sibling of the one directory the repository
        // shares between images, so an image called `lib` would write its
        // Containerfile and scripts over the helper library every component build
        // mounts at `/ctx/lib`.
        if self.id == crate::layout::LIB {
            issues.push(
                Issue::new(format!("`{}` is not available as an image name", self.id), src)
                    .at(self.span, "generated/lib/ is the shared helper library")
                    .help("declare `id \"something\"`: everything else this image generates lands in generated/<id>/, which would be that directory"),
            );
            self.id = String::new();
        }
    }

    fn parse_flavours(&mut self, block: &KdlNode, issues: &mut Issues) {
        let src = &self.src.clone();
        for node in kids(block) {
            let flavour = Flavour {
                name: node.name().value().to_string(),
                default: flag(node, "default"),
                pr_build: flag(node, "pr-build"),
                conforms: prop(node, "conforms").unwrap_or_default().to_string(),
                span: node.name().span().into(),
            };

            if !is_name(&flavour.name) {
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

    /// Reads every path segment of the image, ungated and inside each
    /// `flavour` block.
    fn parse_segments(&mut self, image: &KdlNode, issues: &mut Issues) {
        let src = &self.src.clone();
        for node in kids(image) {
            let name = node.name().value();
            if name != "flavour" {
                if !is_keyword(name) {
                    self.parse_segment(node, "", None, issues);
                }
                continue;
            }
            let Some(flavour) = string_arg(node).map(str::to_string) else {
                continue;
            };
            if !self.flavours.iter().any(|f| f.name == flavour) {
                let known: Vec<&str> = self.flavours.iter().map(|f| f.name.as_str()).collect();
                issues.push(
                    Issue::new(format!("`{flavour}` is not a declared flavour"), src)
                        .at(node.name().span(), "no such flavour")
                        .help(if known.is_empty() {
                            "no flavours are declared; add a `flavours` block above".to_string()
                        } else {
                            format!("declared flavours: {}", known.join(", "))
                        }),
                );
            }
            for inner in kids(node) {
                self.parse_segment(inner, "", Some(&flavour), issues);
            }
        }
    }

    /// Reads one path segment and the segments below it. A child with an
    /// argument sets an option, and a child with no argument is the next
    /// segment. A manifest node among the children makes the segment an
    /// inline definition, and then every child with an argument belongs to
    /// the definition.
    fn parse_segment(
        &mut self,
        node: &KdlNode,
        parent: &str,
        flavour: Option<&str>,
        issues: &mut Issues,
    ) {
        let src = &self.src.clone();
        let name = node.name().value();
        let path = match parent.is_empty() {
            true => name.to_string(),
            false => format!("{parent}/{name}"),
        };
        let span: Span = node.name().span().into();
        if node.entries().iter().any(|entry| entry.name().is_none()) {
            let help = match parent.is_empty() {
                true => format!(
                    "`{name}` is no image node, so it reads as a component path, and a path \
                     segment takes no argument"
                ),
                false => format!(
                    "a child with an argument sets an option of `{parent}`, and `{name}` is \
                     one of those inside a segment of its own"
                ),
            };
            issues.push(
                Issue::new(format!("`{name}` takes no argument"), src)
                    .at(span, "a path segment")
                    .help(help),
            );
            return;
        }
        if !crate::model::image::is_local_path(&path) {
            issues.push(
                Issue::new(format!("`{path}` is not a component path"), src)
                    .at(span, "not a path under components/")
                    .help(
                        "a segment is lowercase letters, digits and dashes, starting with a letter",
                    ),
            );
            return;
        }
        for entry in node.entries() {
            if let Some(key) = entry.name().map(|key| key.value()) {
                if key != "variant" || entry.value().as_string().is_none() {
                    issues.push(
                        Issue::new(format!("`{key}` is not a property of `{path}`"), src)
                            .at(entry.span(), "not part of the schema")
                            .help("a segment accepts `variant=\"<name>\"`; options are set as child nodes"),
                    );
                }
            }
        }
        let defines = kids(node).iter().any(declares);
        let mut inline = Vec::new();
        let mut options = Vec::new();
        let mut nested = Vec::new();
        for kid in kids(node) {
            // `digest` and `signed` belong to a base's segment, even where the
            // segment defines the base inline.
            let tool = matches!(kid.name().value(), "digest" | "signed");
            match (defines && !tool, !is_segment(kid)) {
                (true, true) => inline.push(Span::from(kid.span())),
                (false, true) => options.push((
                    kid.name().value().to_string(),
                    options::args(kid),
                    kid.name().span().into(),
                )),
                (_, false) => nested.push(kid),
            }
        }
        let flavour = flavour.map(str::to_string);
        let variant = prop(node, "variant").map(str::to_string);
        let organises = !nested.is_empty() && options.is_empty() && !defines && variant.is_none();
        // A segment that only organises adds nothing to a segment of the same
        // path, so a path may be spelled out more than once to reach its
        // children.
        let held = self
            .entries
            .iter()
            .position(|entry| entry.path == path && entry.flavour == flavour);
        if let Some(at) = held.filter(|_| !organises) {
            let dup = &self.entries[at];
            if !dup.organises {
                issues.push(
                    Issue::new(format!("`{path}` is listed twice"), src)
                        .at(dup.span, "first here")
                        .at(span, "and again here")
                        .help("a component builds once per flavour it is listed under"),
                );
            } else {
                if defines {
                    self.defined.push(path.clone());
                }
                let dup = &mut self.entries[at];
                dup.variant = variant;
                dup.options = options;
                dup.span = span;
                dup.inline = defines.then_some(inline);
                dup.organises = false;
            }
        } else if held.is_none() {
            if defines {
                self.defined.push(path.clone());
            }
            self.entries.push(Entry {
                path: path.clone(),
                flavour: flavour.clone(),
                variant,
                options,
                span,
                component: None,
                imported_by: None,
                inline: defines.then_some(inline),
                organises,
            });
        }
        for kid in nested {
            self.parse_segment(kid, &path, flavour.as_deref(), issues);
        }
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

    fn messages(text: &str) -> Vec<String> {
        let doc: KdlDocument = text.parse().expect("valid KDL");
        let src = Source::new("image.kdl", text);
        let mut issues = Issues::default();
        check(&doc.nodes()[0], &IMAGE, &src, &mut issues);
        issues.findings()
    }

    /// Writes a repository whose `components/` holds `dirs`, each a path and
    /// its manifest. The image file holds `text`.
    fn repository(name: &str, text: &str, dirs: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("tect-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the temporary directory is writable");
        std::fs::write(root.join("repo.kdl"), "schema-version 1\n")
            .expect("the temporary directory is writable");
        std::fs::write(root.join("image.kdl"), format!("schema-version 1\n{text}"))
            .expect("the temporary directory is writable");
        for (dir, manifest) in dirs {
            let dir = root.join("components").join(dir);
            std::fs::create_dir_all(&dir).expect("the temporary directory is writable");
            std::fs::write(
                dir.join("manifest.kdl"),
                format!("schema-version 1\n{manifest}"),
            )
            .expect("the temporary directory is writable");
        }
        root
    }

    /// Loads `text` as the one image of a repository whose base component
    /// `x` is on the `fedora` family.
    fn semantic_messages(text: &str) -> Vec<String> {
        let root = repository(
            "semantic",
            text,
            &[("x", "base \"x\" { family \"fedora\"; }\n")],
        );
        let (_, issues) = List::load(&root);
        let _ = std::fs::remove_dir_all(&root);
        issues.findings()
    }

    /// The three places a `conforms` can land: over the one that is there,
    /// in front of the first segment, and last of all in an image that lists
    /// nothing.
    #[test]
    fn the_span_a_conforms_takes_is_the_node_or_where_the_next_one_goes() {
        let held = "image {\n    name \"E\"\n    conforms \"standard\"\n    x\n}\n";
        let span = conforms_span(held, "E").expect("the image is there");
        assert_eq!(
            &held[span.offset..span.offset + span.len],
            "conforms \"standard\""
        );

        let bare = "image {\n    name \"E\"\n    layout { bootloader \"grub2\" }\n\n    x\n}\n";
        let span = conforms_span(bare, "E").expect("the image is there");
        assert_eq!(span.len, 0);
        assert!(bare[span.offset..].starts_with("x\n"));

        let empty = "image {\n    name \"E\"\n}\n";
        let span = conforms_span(empty, "E").expect("the image is there");
        assert_eq!((span.len, &empty[span.offset..]), (0, "}\n"));

        assert!(conforms_span(held, "Other").is_none());
    }

    /// Every shape of the image grammar that the golden corpus has no broken
    /// fixture for.
    #[test]
    fn the_table_catches_what_the_corpus_does_not() {
        let found = messages(
            r#"
image "stray" {
    id
    boot-chain "uki-db"
    boot-chain "uki-shim"
    flavours {
        dev sparkle=#true
        dim default="yes"
        bad "arg"
    }
    flavour
}
"#,
        );
        assert_eq!(
            found,
            [
                "`image` takes no argument",
                "`id` needs a value",
                "`boot-chain` is declared twice",
                "unknown flavour property `sparkle`",
                "`default` must be #true or #false",
                "a flavour takes no arguments",
                "`flavour` needs a flavour name",
                "`image` declares no `name`",
            ]
        );
    }

    /// The segment shapes the parse refuses, because the schema reads every
    /// segment as the catch-all node.
    #[test]
    fn a_segment_takes_no_argument_and_a_name_the_path_rule_allows() {
        let found = semantic_messages(
            "image {\n name \"X\"\n x\n palette \"red\"\n Bad_Name\n tools flavor=\"x\"\n}",
        );
        for wanted in [
            "`palette` takes no argument",
            "`Bad_Name` is not a component path",
            "`flavor` is not a property of `tools`",
        ] {
            assert!(
                found.iter().any(|message| message.contains(wanted)),
                "{wanted}: {found:?}"
            );
        }
    }

    #[test]
    fn the_image_block_ends_at_a_closing_brace() {
        let text = "image {\n    name \"X\"\n    tools {\n        one\n    }\n}\n";
        let at = block_close(text, "X", &[("tools", None)]).expect("a tools block");
        assert_eq!(&text[at..=at], "}");
        assert_eq!(&text[at - 5..at], "\n    ");
        assert!(block_close(text, "X", &[("tools", None), ("one", None)]).is_none());
    }

    /// Two images in one file, which is what makes the name part of the walk.
    #[test]
    fn a_block_is_found_under_the_image_that_was_named() {
        let text = "image {\n    name \"A\"\n    flavour \"dev\" { }\n}\nimage {\n    name \"B\"\n    flavour \"dev\" {\n    }\n}\n";
        let chain = [("flavour", Some("dev"))];
        let at = block_close(text, "B", &chain).expect("B's flavour block");
        assert!(text[..at].ends_with("flavour \"dev\" {\n    "), "{at}");
        assert!(block_close(text, "C", &chain).is_none());
        let whole = block_close(text, "A", &[]).expect("A's own brace");
        assert!(whole < at);
    }

    #[test]
    fn an_empty_flavours_block_is_a_block_with_nothing_in_it() {
        let found = messages("image {\n    name \"X\"\n    flavours { }\n    x\n}\n");
        assert_eq!(found, ["`flavours` has no flavours in it"]);
    }

    #[test]
    fn boot_chain_accepts_only_one_known_chain() {
        for chain in BOOT_CHAINS {
            assert!(messages(&format!(
                "image {{\n name \"X\"\n boot-chain \"{chain}\"\n x\n}}"
            ))
            .is_empty());
        }
        assert_eq!(
            messages("image {\n name \"X\"\n boot-chain \"grub\"\n x\n}"),
            ["`grub` is not a boot chain"]
        );
    }

    #[test]
    fn a_uki_chain_refuses_a_conflicting_layout() {
        let found = semantic_messages(
            "image {\n name \"X\"\n boot-chain \"uki-db\"\n x\n layout { filesystem \"xfs\"\n composefs #false\n generic-image #false\n bootloader \"grub2\" }\n}",
        );
        assert!(found
            .iter()
            .any(|message| message.contains("conflicts with its UKI chain")));
    }

    /// `base-image` holds a library base below it, so it is an organising
    /// directory and a definition may sit at its path. The definition carries
    /// a digest, which reaches the generated `FROM`.
    #[test]
    fn an_inline_base_at_an_organising_directory_builds_on_its_reference() {
        let root = repository(
            "inline-base",
            "image {\n name \"X\"\n base-image {\n  base \"quay.io/fedora/fedora-bootc:44\" { family \"fedora\"; }\n  digest \"sha256:abc\"\n }\n}\n",
            &[("base-image/fedora-bootc", "base \"quay.io/fedora/fedora-bootc:44\"\n")],
        );
        let (list, issues) = List::load(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert!(issues.findings().is_empty(), "{:?}", issues.findings());
        let base = list.images[0]
            .base
            .as_ref()
            .expect("the definition is a base");
        assert_eq!(base.path, "base-image");
        assert_eq!(base.image, "quay.io/fedora/fedora-bootc:44@sha256:abc");
        assert_eq!(base.family, "fedora");
        assert!(base.answers(crate::parse::component::BASE_IMAGE));
    }

    #[test]
    fn an_inline_definition_that_meets_a_component_is_refused_naming_both() {
        let root = repository(
            "inline-clash",
            "image {\n name \"X\"\n x\n tools {\n  packages \"vim\"\n }\n}\n",
            &[
                ("x", "base \"x\" { family \"fedora\"; }\n"),
                ("tools", "packages \"vim\"\n"),
            ],
        );
        let found = crate::load(&root).issues.findings();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            found.iter().any(|message| message
                .contains("`tools` is defined inline, and components/tools is a component too")),
            "{found:?}"
        );
    }

    #[test]
    fn a_second_base_is_refused() {
        let root = repository(
            "two-bases",
            "image {\n name \"X\"\n x\n y\n}\n",
            &[("x", "base \"x\"\n"), ("y", "base \"y\"\n")],
        );
        let (_, issues) = List::load(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            issues
                .findings()
                .iter()
                .any(|message| message.contains("`x` lists two bases")),
            "{:?}",
            issues.findings()
        );
    }

    #[test]
    fn a_top_level_component_named_after_an_image_node_is_refused() {
        let root = repository(
            "keyword-dir",
            "image {\n name \"X\"\n x\n}\n",
            &[
                ("x", "base \"x\"\n"),
                ("boot-chain/uki", "packages \"a\"\n"),
            ],
        );
        let (_, issues) = List::load(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            issues
                .findings()
                .iter()
                .any(|message| message
                    .contains("components/boot-chain is named after an image node")),
            "{:?}",
            issues.findings()
        );
    }

    /// `hardware` holds no component, so it lists nothing. Its second
    /// spelling only reaches a further child.
    #[test]
    fn an_organising_segment_lists_nothing_and_may_repeat() {
        let root = repository(
            "organise",
            "image {\n name \"X\"\n x\n hardware { nvidia }\n hardware { audio }\n}\n",
            &[
                ("x", "base \"x\"\n"),
                ("hardware/nvidia", "packages \"a\"\n"),
                ("hardware/audio", "packages \"b\"\n"),
            ],
        );
        let (list, issues) = List::load(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert!(issues.findings().is_empty(), "{:?}", issues.findings());
        let paths: Vec<&str> = list.images[0]
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        assert_eq!(paths, ["hardware/nvidia", "hardware/audio"]);
    }

    #[test]
    fn a_child_with_an_argument_sets_an_option_and_one_without_is_a_segment() {
        let root = repository(
            "options",
            "image {\n name \"X\"\n x\n hardware {\n  nvidia {\n   version \"550\"\n   cuda\n  }\n }\n}\n",
            &[("x", "base \"x\"\n")],
        );
        let (list, _) = List::load(&root);
        let _ = std::fs::remove_dir_all(&root);
        let entries = &list.images[0].entries;
        let nvidia = entries
            .iter()
            .find(|entry| entry.path == "hardware/nvidia")
            .expect("nvidia sets an option, so it lists its component");
        assert_eq!(nvidia.options[0].0, "version");
        assert!(entries
            .iter()
            .any(|entry| entry.path == "hardware/nvidia/cuda"));
    }

    /// Loads a repository whose base component `x` is on the `fedora` family,
    /// and returns every finding the full resolution reports.
    fn resolved(case: &str, image: &str, dirs: &[(&str, &str)]) -> Vec<String> {
        let mut all = vec![("x", "base \"x\" { family \"fedora\"; }\n")];
        all.extend_from_slice(dirs);
        let root = repository(case, image, &all);
        let found = crate::load(&root).issues.findings();
        let _ = std::fs::remove_dir_all(&root);
        found
    }

    fn holds(found: &[String], wanted: &str) {
        assert!(
            found.iter().any(|message| message.contains(wanted)),
            "{wanted}: {found:?}"
        );
    }

    #[test]
    fn an_inline_definition_is_held_to_the_manifest_grammar() {
        let found = resolved(
            "inline-grammar",
            "image {\n name \"X\"\n x\n tools {\n  packages \"vim\"\n  palette \"red\"\n }\n}\n",
            &[],
        );
        holds(&found, "unknown node `palette`");
    }

    /// `digest` under `base` would be dropped, and the build would start from
    /// an unpinned reference.
    #[test]
    fn a_block_under_base_is_refused() {
        let found = resolved(
            "base-block",
            "image {\n name \"X\"\n base-image {\n  base \"quay.io/x:1\" { digest \"sha256:ab\" }\n }\n}\n",
            &[],
        );
        holds(&found, "`base` holds no `digest`");
    }

    #[test]
    fn a_hand_written_base_image_capability_is_refused() {
        let found = resolved(
            "base-image-by-hand",
            "image {\n name \"X\"\n x\n tools {\n  provides \"base-image\"\n }\n}\n",
            &[],
        );
        holds(&found, "`base-image` is what a `base` node provides");
    }

    #[test]
    fn a_malformed_signed_and_a_second_digest_are_refused() {
        let found = semantic_messages(
            "image {\n name \"X\"\n x {\n  signed \"true\"\n  digest \"sha256:ab\"\n  digest \"sha256:cd\"\n }\n}",
        );
        holds(&found, "`signed` needs #true or #false");
        holds(&found, "`digest` is declared twice");
    }

    #[test]
    fn a_seed_refuses_an_inline_definition() {
        let root = repository(
            "seed-inline",
            "image {\n name \"example\"\n x\n tools {\n  packages \"vim\"\n }\n}\n",
            &[("x", "base \"x\" { family \"fedora\"; }\n")],
        );
        std::fs::write(
            root.join("repo.kdl"),
            "schema-version 1\nsources {\n components \"owner\" { dir \".\" }\n}\nseed \"example\" collection=\"owner\"\n",
        )
        .expect("the temporary directory is writable");
        let (_, issues) = List::load(&root);
        let _ = std::fs::remove_dir_all(&root);
        holds(
            &issues.findings(),
            "`tools` is defined inline, so the seed cannot name it",
        );
    }

    /// `fragment position="after"` carries a property alone, and it is still
    /// a manifest node. Only a definition gets the refusal that it ships no
    /// Containerfile.inc.
    #[test]
    fn a_manifest_node_with_a_property_alone_defines_inline() {
        let found = resolved(
            "inline-fragment",
            "image {\n name \"X\"\n x\n tools {\n  fragment position=\"after\"\n }\n}\n",
            &[],
        );
        holds(
            &found,
            "`tools` declares `fragment` but ships no Containerfile.inc",
        );
    }
}

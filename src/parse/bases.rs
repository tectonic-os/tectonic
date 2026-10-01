//! `bases.kdl`: the bases a collection describes, read the same way as any
//! other manifest it holds.

use crate::base::{Base, Capability};
use crate::diag::{Issue, Issues, Source, Span};
use crate::parse::schema::{check_doc, Arg, Node, Say};
use crate::parse::{bool_arg, check_capability, check_path, child, string_arg, string_args, text};
use crate::parse::{kids, syntax_issue};
use kdl::{KdlDocument, KdlNode};
use std::path::Path;

/// One entry, which is what the seed compiled into the tool holds a row of.
#[rustfmt::skip]
const BASE: Node = Node::new("base",
    "One base that an image builds on, which the catalog entry names by its image reference.").about("One entry describes one upstream base image: what it is, which family it belongs to, what it ships and what it still needs. `tect create image` copies these facts into each image that it scaffolds on the base.").example("\"quay.io/fedora/fedora-bootc:44\"").minimal()
    .arg(Arg::Str, Say::new("`base` needs an image reference", "no image given",
        "`base \"ghcr.io/ublue-os/bazzite:stable\"`, the reference an image writes verbatim"))
    .unique(Say::new("`{}` is described twice", "already described above",
        "one base is one entry; a second would shadow the first silently"))
    .children(&[
        Node::new("about", "The line that the base picker shows beside the reference.").example("\"Fedora bootc, the upstream base image\"")
            .arg(Arg::Str, Say::new("`about` needs a line", "nothing given",
                "`about \"KDE, gaming and hardware support\"`, what a person picks between bases on"))
            .once("")
            .missing(Say::new("`base` declares no `about`", "nothing to pick it by",
                "`about \"...\"` is the line `tect create image` shows beside the reference")),
        Node::new("family",
            "The family that an image on this base declares, which each module's `supports` \
             must match.").example("\"fedora\"")
            .arg(Arg::Str, Say::new("`family` needs a name", "no family given",
                "`family \"fedora\"`, written into the `base` block of every image scaffolded on it"))
            .once("")
            .missing(Say::new("`base` describes no `family`", "no family",
                "the family is what an image scaffolded on this base declares, and an entry \
                 without one describes nothing the tool can write")),
        Node::new("provides", "The capabilities that this base already ships.").example("\"rechunking\" \"bootc\"")
            .arg(Arg::Strs, Say::NONE),
        Node::new("requires",
            "The capabilities that an enabled module must provide before this base is usable.").example("\"bootc-base\"")
            .arg(Arg::Strs, Say::NONE),
        Node::new("signed", "Whether this base publishes a cosign signature.").example("#true")
            .arg(Arg::Bool, Say::new("`signed` needs #true or #false", "not a boolean",
                "`signed #true` records that this base publishes a cosign signature"))
            .once(""),
        Node::new("scap-content",
            "The bare filename of the SSG datastream that measures this base.").example("\"ssg-fedora-ds.xml\"")
            .arg(Arg::Str, Say::new("`scap-content` needs a filename", "no datastream given",
                "`scap-content \"ssg-cs10-ds.xml\"`, the file under \
                 /usr/share/xml/scap/ssg/content a scan of this base reads"))
            .once(""),
        Node::new("bootloader",
            "The bootloaders that an image on this base can install, with the default first.").example("\"grub2\"")
            .arg(Arg::Strs, Say::new("`bootloader` needs a name", "no bootloader given",
                "`bootloader \"grub2\"`, or `bootloader \"grub2\" \"systemd\"` where the base \
                 carries both"))
            .once("")
            .missing(Say::new("`base` names no `bootloader`", "no default bootloader",
                "`bootloader \"grub2\"` states what an image on it boots with, and `tect create \
                 image` writes it into the image")),
    ], Say::new("unknown node `{}` in a base", "not part of the schema",
        "a base entry holds `about`, `family`, `provides`, `requires`, `signed`, \
         `scap-content` and `bootloader`: what an image built on it may assume, what it still \
         needs, what a person picks it by, what measures it and what it boots with"))
    .notes(&[
        "`tect create image` writes `family`, `provides`, `requires`, `signed` and \
         `bootloader` into each image it scaffolds on this base.",
        "No catalog row carries a digest, because a digest in the catalog changes only with a \
         tool release. The image file that builds on the base holds the digest.",
    ]);

/// Where a capability's presence is read, for a name found neither at
/// `/usr/bin/<name>` nor at `/usr/sbin/<name>`.
#[rustfmt::skip]
const CAPABILITY: Node = Node::new("capability",
    "Where the presence of a capability is read, which is the path that witnesses it.").about("Tells `tect` how to prove that a capability is in a finished image: the path of a file that witnesses it. A row is needed only where the witness is not `/usr/bin/<name>` or `/usr/sbin/<name>`.").example("\"ssh\" \"/usr/sbin/sshd\"")
    .arg(Arg::Strs, Say::new("`capability` needs a name", "nothing named",
        "`capability \"ssh\" \"/usr/sbin/sshd\"`, or `capability \"rechunking\"` for one no \
         path witnesses"))
    .unique(Say::new("capability `{}` is described twice", "already described above",
        "one name is one row"))
    .lists(&[
        ("A path witnesses a capability name. `tect` reads the first of these that names one:", &[
            "the `file=` of a module's `provides \"<name>\"`;",
            "a `capability` row;",
            "`/usr/bin/<name>` or `/usr/sbin/<name>`.",
        ]),
    ])
    .notes(&[
        "A row with no path names a capability that nothing witnesses.",
        "`tect validate-image` checks the finished image for each name that the base claims, and \
         for each name that a module or a row locates. `base-sig-probe` reads the same \
         witnesses when it measures a base.",
        "The witness of `luks-initramfs` is inside an archive. If a target declares it, then \
         the build validates the initramfs with `lsinitrd`. If no target declares it, then the \
         installer does not offer root encryption.",
        "Declare `luks-initramfs` in a custom base or module only if its initramfs unlocks LUKS. \
         `tect build` then proves that the binary is present before the installer offers it.",
    ]);

/// bases.kdl's grammar, and the whole of it.
#[rustfmt::skip]
pub const BASES: Node = Node::new("bases",
    "The base images a collection describes, where a base is the Linux image that an \
     `image.kdl` builds its custom image on.").about("The base catalog lists the upstream images that `tect` knows, and what each one ships. `tect create image` offers these bases and writes what each ships into the new image.")
    .children(&[BASE, CAPABILITY], Say::new("unknown node `{}` in bases.kdl",
        "not part of the schema",
        "bases.kdl holds `base` and `capability` entries; a module goes in a directory of its own"))
    .notes(&[
        "`tect create image` offers the bases it knows, with the family of each and what each \
         ships. An image scaffolded on a base lists no module that the base already carries.",
    ])
    .lists(&[
        ("`tect` selects one catalog:", &[
            "Each `tect` release compiles one `bases.kdl` into the binary, and the release ships \
             the same file for replacement at runtime.",
            "If a `bases.kdl` sits beside the binary, then it replaces the compiled-in catalog \
             completely. The two do not merge.",
            "If the runtime `bases.kdl` is unreadable or malformed, then `tect` reports its exact \
             path. It does not fall back to the compiled-in catalog.",
        ]),
        ("A collection extends the selected catalog with a `bases.kdl` at its root, beside its \
          modules:", &[
            "A collection entry replaces the catalog entry for the same reference. `tect check` \
             names the collection wherever the two entries differ.",
            "If two collections describe one base, then `tect` reports an error.",
            "`tect` fetches nothing to read a catalog. A collection that is not on this machine \
             extends nothing.",
        ]),
    ]);

/// What one bases.kdl describes.
pub struct Catalog {
    pub bases: Vec<Base>,
    pub capabilities: Vec<Capability>,
    pub src: Source,
}

/// Every entry one collection describes, and the file they were read out of. A
/// file that is not there is a collection that extends nothing.
pub fn read(path: &Path, issues: &mut Issues) -> Option<Catalog> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(parse(&path.display().to_string(), &text, issues)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            let src = Source::new(path.display().to_string(), "");
            issues.push(Issue::new(
                format!("{} could not be read: {err}", path.display()),
                &src,
            ));
            Some(Catalog {
                bases: Vec::new(),
                capabilities: Vec::new(),
                src,
            })
        }
    }
}

/// Every entry in KDL text and the supplied diagnostic source.
pub fn parse(name: &str, text: &str, issues: &mut Issues) -> Catalog {
    let src = Source::new(name, text);
    let doc: KdlDocument = match text.parse() {
        Ok(doc) => doc,
        Err(err) => {
            issues.push(syntax_issue(&err, src.name(), &src));
            return Catalog {
                bases: Vec::new(),
                capabilities: Vec::new(),
                src,
            };
        }
    };
    check_doc(&doc, &BASES, &src, issues);

    let mut bases: Vec<Base> = Vec::new();
    for node in doc.nodes().iter().filter(|n| n.name().value() == "base") {
        let Some(base) = entry(node, &src, issues) else {
            continue;
        };
        // A second entry for one base is the walker's to report, and this is
        // what keeps it from being reported again as another file's.
        if !bases.iter().any(|first| first.image == base.image) {
            bases.push(base);
        }
    }
    let mut capabilities: Vec<Capability> = Vec::new();
    for node in doc
        .nodes()
        .iter()
        .filter(|n| n.name().value() == "capability")
    {
        let span: Span = node.name().span().into();
        let args = string_args(node);
        let Some(name) = args.first() else { continue };
        check_capability(name, span, &src, issues);
        let path = args.get(1).map(|path| {
            check_path(path, span, &src, issues);
            path.to_string()
        });
        if args.len() > 2 {
            issues.push(
                Issue::new(
                    format!("capability `{name}` names more than one path"),
                    &src,
                )
                .at(span, "one witness")
                .help("a capability is witnessed by one path; the first is where it is read"),
            );
        }
        if !capabilities.iter().any(|first| first.name == *name) {
            capabilities.push(Capability {
                name: name.to_string(),
                path,
                span,
            });
        }
    }
    Catalog {
        bases,
        capabilities,
        src,
    }
}

/// One entry, or nothing where it describes too little to write an image with.
fn entry(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Base> {
    for kid in kids(node) {
        let span: Span = kid.name().span().into();
        for value in string_args(kid) {
            match kid.name().value() {
                "provides" | "requires" => check_capability(value, span, src, issues),
                "bootloader" if !crate::parse::image::BOOTLOADERS.contains(&value) => issues.push(
                    Issue::new(
                        format!("`{value}` is not a bootloader the installer installs"),
                        src,
                    )
                    .at(span, "not a bootloader")
                    .help("the installer writes `grub2` or `systemd`"),
                ),
                _ => {}
            }
        }
    }
    let base = Base {
        image: string_arg(node)?.to_string(),
        family: text(node, "family"),
        provides: strings(node, "provides"),
        requires: strings(node, "requires"),
        about: text(node, "about"),
        signed: child(node, "signed").and_then(bool_arg).unwrap_or(false),
        scap_content: text(node, "scap-content"),
        bootloaders: strings(node, "bootloader"),
        span: node.name().span().into(),
    };
    (!base.family.is_empty()).then_some(base)
}

/// Every `name "a" "b"` under a node, as one list.
fn strings(node: &KdlNode, name: &str) -> Vec<String> {
    kids(node)
        .iter()
        .filter(|c| c.name().value() == name)
        .flat_map(|c| string_args(c).into_iter().map(str::to_string))
        .collect()
}

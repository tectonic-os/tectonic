//! Library files: one `base` node per `*.base.kdl` file in a base-images
//! library.

use crate::base::Base;
use crate::diag::{Issue, Issues, Source, Span};
use crate::parse::schema::{check_doc, Arg, Node, Say};
use crate::parse::{bool_arg, check_capability, child, string_args, text};
use crate::parse::{kids, syntax_issue};
use kdl::{KdlDocument, KdlNode};
use std::path::Path;

/// The one `base` node a `*.base.kdl` file holds.
#[rustfmt::skip]
const BASE: Node = Node::new("base",
    "One base that an image builds on, which the file stem names.").about("One file describes one base: what it is, which family it belongs to, what it ships and what it still needs. `tect create image` copies these facts into each image that it scaffolds on the base.").minimal()
    .children(&[
        Node::new("image", "The image reference that an image on this base builds from.").example("\"quay.io/fedora/fedora-bootc:44\"")
            .arg(Arg::Str, Say::new("`image` needs an image reference", "no image given",
                "`image \"ghcr.io/ublue-os/bazzite:stable\"`, the reference a build on this base starts from"))
            .once("")
            .missing(Say::new("this base names no `image`", "no image reference",
                "the reference the build starts from, such as `image \"quay.io/fedora/fedora-bootc:44\"`")),
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
        "a base entry holds `image`, `about`, `family`, `provides`, `requires`, `signed`, \
         `scap-content` and `bootloader`: what an image built on it may assume, what it still \
         needs, what a person picks it by, what measures it and what it boots with"))
    .notes(&[
        "`tect create image` writes `family`, `provides`, `requires`, `signed` and \
         `bootloader` into each image it scaffolds on this base.",
        "No catalog row carries a digest, because a digest in the catalog changes only with a \
         library commit. The image file that builds on the base holds the digest.",
    ])
    .once(
        "one file describes one base: merge the nodes or split the file, because the file stem \
         is the base's catalog name",
    )
    .missing(Say::new("this base file holds no `base` node", "no base",
        "a `*.base.kdl` file holds exactly one `base` node; delete the file or add it"));

/// The grammar of one `*.base.kdl` file.
#[rustfmt::skip]
pub const BASE_FILE: Node = Node::new("base file",
    "A file in a base-images library, holding one base.").about("One file describes one base, and its file stem is the base's catalog name. A `base-images` source reads every `*.base.kdl` file in the directory its `path` names.").scaffolds(&["base"])
    .children(&[BASE], Say::new("unknown top-level node `{}`", "not part of the schema",
        "a base file holds one `base` node; everything about the base is declared inside it"))
    .notes(&[
        "The file stem is the base's catalog name: `fedora-bootc-44.base.kdl` describes the base \
         `fedora-bootc-44`.",
        "Two files that name one image reference are refused, because which of them an image \
         got its family and its `provides` from would depend on the order the sources are read \
         in.",
    ]);

/// One base out of its `*.base.kdl` file, named by the file stem, with the
/// source its spans point into.
pub fn read_base(path: &Path, issues: &mut Issues) -> Option<(Base, Source)> {
    let text = read(path, issues)?;
    let src = Source::new(path.display().to_string(), &text);
    let doc: KdlDocument = match text.parse() {
        Ok(doc) => doc,
        Err(err) => {
            issues.push(syntax_issue(&err, src.name(), &src));
            return None;
        }
    };
    check_doc(&doc, &BASE_FILE, &src, issues);
    let mut found = doc
        .nodes()
        .iter()
        .filter(|node| node.name().value() == "base");
    let node = match (found.next(), found.next()) {
        (Some(one), None) => one,
        // The walker said what is wrong with the count.
        _ => return None,
    };
    let name = path
        .file_name()
        .and_then(|file| file.to_str())
        .and_then(|file| file.strip_suffix(crate::base::BASE_SUFFIX))?
        .to_string();
    Some((entry(node, name, &src, issues)?, src))
}

/// One entry, or nothing where it describes too little to build an image with.
fn entry(node: &KdlNode, name: String, src: &Source, issues: &mut Issues) -> Option<Base> {
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
        name,
        image: text(node, "image"),
        family: text(node, "family"),
        provides: strings(node, "provides"),
        requires: strings(node, "requires"),
        about: text(node, "about"),
        signed: child(node, "signed").and_then(bool_arg).unwrap_or(false),
        scap_content: text(node, "scap-content"),
        bootloaders: strings(node, "bootloader"),
        span: node.name().span().into(),
    };
    (!base.image.is_empty() && !base.family.is_empty()).then_some(base)
}

/// Every `name "a" "b"` under a node, as one list.
fn strings(node: &KdlNode, name: &str) -> Vec<String> {
    kids(node)
        .iter()
        .filter(|c| c.name().value() == name)
        .flat_map(|c| string_args(c).into_iter().map(str::to_string))
        .collect()
}

/// A file that is not there is a source that describes nothing, and one that
/// cannot be read is said.
fn read(path: &Path, issues: &mut Issues) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            issues.push(Issue::new(
                format!("{} could not be read: {err}", path.display()),
                &Source::new(path.display().to_string(), ""),
            ));
            None
        }
    }
}

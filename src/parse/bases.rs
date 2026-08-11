//! `bases.kdl`: the bases a collection describes, read the same way as any
//! other manifest it holds.

use crate::base::Base;
use crate::diag::{Issues, Source, Span};
use crate::parse::schema::{check_doc, Arg, Node, Say};
use crate::parse::{bool_arg, check_capability, check_path, child, string_arg, string_args, text};
use crate::parse::{kids, syntax_issue};
use kdl::{KdlDocument, KdlNode};
use std::path::Path;

/// One entry, which is what the seed compiled into the tool holds a row of.
#[rustfmt::skip]
const BASE: Node = Node::new("base",
    "One base a collection describes, named by the reference an image builds on.")
    .arg(Arg::Str, Say::new("`base` needs an image reference", "no image given",
        "`base \"ghcr.io/ublue-os/bazzite:stable\"`, the reference an image writes verbatim"))
    .unique(Say::new("`{}` is described twice", "already described above",
        "one base is one entry; a second would shadow the first silently"))
    .children(&[
        Node::new("about", "The line a base picker shows beside the reference.")
            .arg(Arg::Str, Say::new("`about` needs a line", "nothing given",
                "`about \"KDE, gaming and hardware support\"`, what a person picks between bases on"))
            .once("")
            .missing(Say::new("`base` declares no `about`", "nothing to pick it by",
                "`about \"...\"` is the line `tect create image` shows beside the reference")),
        Node::new("family",
            "The family an image built on this base declares, matched against every module's \
             `supports`.")
            .arg(Arg::Str, Say::new("`family` needs a name", "no family given",
                "`family \"fedora\"`, written into the `base` block of every image scaffolded on it"))
            .once("")
            .missing(Say::new("`base` describes no `family`", "no family",
                "the family is what an image scaffolded on this base declares, and an entry \
                 without one describes nothing the tool can write")),
        Node::new("provides",
            "Capabilities this base already ships, written into every image scaffolded on it.")
            .arg(Arg::Strs, Say::NONE),
        Node::new("provides-file",
            "Absolute paths this base guarantees, written into every image scaffolded on it.")
            .arg(Arg::Strs, Say::NONE),
        Node::new("signed",
            "Whether this base publishes a cosign signature, which a scaffolded image records.")
            .arg(Arg::Bool, Say::new("`signed` needs #true or #false", "not a boolean",
                "`signed #true` records that this base publishes a cosign signature"))
            .once(""),
    ], Say::new("unknown node `{}` in a base", "not part of the schema",
        "a base entry holds `about`, `family`, `provides`, `provides-file` and `signed`: what an \
         image built on it may assume, and what a person picks it by"));

/// bases.kdl's grammar, and the whole of it.
#[rustfmt::skip]
pub const BASES: Node = Node::new("bases",
    "The bases a collection describes, which extend the ones the tool ships with.")
    .children(&[BASE], Say::new("unknown node `{}` in bases.kdl", "not part of the schema",
        "bases.kdl holds `base` entries and nothing else; a module goes in a directory of its own"));

/// Every base one collection describes, and the file they were read out of. A
/// file that is not there is a collection that extends nothing.
pub fn read(path: &Path, issues: &mut Issues) -> Option<(Vec<Base>, Source)> {
    let text = std::fs::read_to_string(path).ok()?;
    let src = Source::new(path.display().to_string(), text.clone());
    let doc: KdlDocument = match text.parse() {
        Ok(doc) => doc,
        Err(err) => {
            issues.push(syntax_issue(&err, src.name(), &src));
            return Some((Vec::new(), src));
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
    Some((bases, src))
}

/// One entry, or nothing where it describes too little to write an image with.
fn entry(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Base> {
    for kid in kids(node) {
        let span: Span = kid.name().span().into();
        for value in string_args(kid) {
            match kid.name().value() {
                "provides" => check_capability(value, span, src, issues),
                "provides-file" => check_path(value, span, src, issues),
                _ => {}
            }
        }
    }
    let base = Base {
        image: string_arg(node)?.to_string(),
        family: text(node, "family"),
        provides: strings(node, "provides"),
        provides_files: strings(node, "provides-file"),
        about: text(node, "about"),
        signed: child(node, "signed").and_then(bool_arg).unwrap_or(false),
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

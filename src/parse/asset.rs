//! `asset`, the pinned upstream payload a module fetches.

use crate::diag::{Issue, Issues, Source, Span};
use crate::model::asset::Asset;
use crate::parse::schema::{Arg, Node, Say};
use crate::parse::{kids, string_arg};
use crate::provenance::evidence::{read, Role, PIN};
use crate::provenance::Evidence;
use kdl::KdlNode;

#[rustfmt::skip]
pub const ASSET: Node = Node::new("asset",
    "A pinned upstream payload the module fetches, reaching the build as ASSET_*.")
    .arg(Arg::Str, Say::new("`asset` needs a name", "no name given",
        "`asset \"starship\" { ... }`; the name becomes the ASSET_* env prefix"))
    .unique(Say::new("asset `{}` is declared twice", "already declared above",
        "two assets under one name would resolve to the same ASSET_* env"))
    .props(&[], Say::new("unknown asset property `{}`", "not part of the schema",
        "an asset carries its fields as child nodes, not properties"))
    .children(&[PIN],
        Say::new("unknown node `{}` in an asset", "not part of the schema",
            "an asset holds one `pin`, which is where it comes from and what verifies it"))
    .empty(Say::new("`asset` has no `pin` in it", "empty block",
        "an asset is its pin: `asset \"starship\" { pin { url \"...\" } }`"));

/// `asset "starship" { pin { renovate ...; version "1.26.0"; url "..."; sha256
/// "..." } }`
pub fn parse(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Asset> {
    let span: Span = node.name().span().into();
    let name = string_arg(node)?.to_string();

    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        issues.push(
            Issue::new(format!("invalid asset name `{name}`"), src)
                .at(span, "lowercase, digits and dashes only")
                .help("the name becomes an env var, uppercased with dashes as underscores and prefixed ASSET_"),
        );
    }

    let pin = kids(node)
        .iter()
        .find(|child| child.name().value() == "pin")
        .map(|child| read(child, Role::Asset, src, issues))
        .unwrap_or_else(|| Evidence::new(span));

    Some(Asset { name, pin, span })
}

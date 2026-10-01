//! `sources`, the module collections references and copies resolve against.

use crate::diag::{Issue, Issues, Source, Span};
use crate::model::image::is_name;
use crate::model::remote::{At, Collection};
use crate::parse::schema::{Arg, Node, Say};
use crate::parse::{kids, string_arg};
use crate::provenance::evidence::{read, Role, PIN};
use kdl::KdlNode;

/// One collection in the registry, named by the owner, which is why the node's
/// name is empty.
#[rustfmt::skip]
pub const COLLECTION: Node = Node::new("",
    "One module collection, where the node name is the owner that its references use.").about("A collection is a set of modules that another repository publishes. The node name is the owner name that module references use. A `pin` or a location says where the collection comes from.").scaffolds(&["pin"]).example("tectonic-os")
    .arg(Arg::Str, Say::NONE)
    .props(&[], Say::new("unknown collection property `{}`", "not part of the schema",
        "a collection carries its fields as child nodes, not properties"))
    .children(&[PIN], Say::new("unknown node `{}` in a collection", "not part of the schema",
        "a collection is either a directory on this machine, `{} \"../modules\"`, or a `pin` \
         naming the archive it is fetched from"))
    .lists(&[
        ("A collection is one of two kinds:", &[
            "An archive, if the collection holds a `pin`. `tect` fetches and verifies it like an \
             out-of-tree module.",
            "A directory on this machine, if the collection has a location argument. The location \
             is relative to the repository root. `tect` reads it in place, and nothing is \
             downloaded, pinned or hashed. The user can change the collection with no new archive \
             for each edit.",
        ]),
        ("If the pin of a collection carries `unpinned`, then the collection follows a branch:", &[
            "`tect create repo` scaffolds the collection that way.",
            "Every import or copy downloads the branch again, and nothing checks what arrived.",
            "Under `audit { enforce #true }`, an import or a copy is an error.",
            "Without enforcement, an import runs the unverified content, and a copy lands in the \
             tracked tree for review.",
            "`tect check` names the collection above its counts and does not treat it as an \
             error.",
        ]),
    ])
    .notes(&[
        "`tect` checks a collection location when a command reads the collection. A check of \
         the repository does not check it, because a directory can exist on one machine only.",
        "A tagged collection verifies every fetch, and every module in it shares the one \
         version.",
    ]);

/// `sources { tectonic-os { pin { url "https://host/{version}.tar.gz" ... } };
/// scratch "../modules" }` The node's name is the owner. An argument is a
/// directory on this machine, which is read where it is, so nothing is fetched
/// and there is nothing to pin or hash.
pub fn parse_collection(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Collection> {
    let name = node.name().value().to_string();
    let span: Span = node.name().span().into();

    if !is_name(&name) {
        issues.push(
            Issue::new(format!("invalid collection name `{name}`"), src)
                .at(span, "lowercase, digits and dashes, starting with a letter")
                .help("the name qualifies references as `<name>/<module>` and reaches every image that lists one of them"),
        );
        return None;
    }

    let dir = string_arg(node).filter(|at| !at.is_empty());
    let pin = kids(node)
        .iter()
        .find(|child| child.name().value() == "pin");

    match (dir, pin) {
        (Some(_), Some(pin)) => {
            issues.push(
                Issue::new(format!("`{name}` is a directory, so its `pin` says nothing"), src)
                    .at(pin.name().span(), "nothing is fetched")
                    .help("a collection on this machine is read where it is; a pin belongs on one that is downloaded"),
            );
            Some(Collection {
                name,
                at: At::Dir(dir.unwrap_or_default().to_string()),
                span,
            })
        }
        (Some(at), None) => Some(Collection {
            name,
            at: At::Dir(at.to_string()),
            span,
        }),
        (None, Some(pin)) => Some(Collection {
            name,
            at: At::Archive(read(pin, Role::Collection, src, issues)),
            span,
        }),
        (None, None) => {
            issues.push(
                Issue::new(format!("`{name}` says nothing about where the collection is"), src)
                    .at(span, "no location given")
                    .help("a directory on this machine, `{name} \"../modules\"`, or a pinned archive, `{name} { pin { url \"https://host/{version}.tar.gz\" } }`".replace("{name}", &name))
            );
            None
        }
    }
}

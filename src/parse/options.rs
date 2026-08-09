//! `option` and `variant`, and the boundary the model's own value type is
//! converted at.

use crate::diag::{Issue, Issues, Source};
use crate::model::options::{check_values, Opt, OptType, Value, Variant};
use crate::parse::schema::{Arg, Kind, Node, Prop, Say};
use crate::parse::{child, kids, prop, string_arg};
use kdl::{KdlNode, KdlValue};

#[rustfmt::skip]
pub const OPTION: Node = Node::new("option",
    "One value an image may set on this module, reaching the build as OPT_*.")
    .arg(Arg::Str, Say::new("`option` needs a name", "no name given",
        "`option \"fonts\" type=\"list\" { ... }`"))
    .unique(Say::new("option `{}` is declared twice", "already declared above", ""))
    .props(&[
        Prop { name: "type", kind: Kind::Str,
            desc: "What the option holds: string, bool or list.",
            say: Say::NONE },
    ], Say::new("unknown option property `{}`", "not part of the schema",
        "an option carries `type`, and everything else as child nodes"))
    .children(&[
        Node::new("description", "What setting the option does, for the generated reference.")
            .once(""),
        Node::new("default", "What the module builds with when no image sets it.")
            .arg(Arg::Strs, Say::NONE)
            .once(""),
    ], Say::new("unknown node `{}` in an option", "not part of the schema",
        "an option holds `description` and `default`"));

#[rustfmt::skip]
pub const VARIANT: Node = Node::new("variant",
    "A named set of option values an image selects with `variant=`.")
    .arg(Arg::Str, Say::new("`variant` needs a name", "no name given", ""))
    .unique(Say::new("variant `{}` is declared twice", "already declared above", ""))
    .props(&[], Say::new("unknown variant property `{}`", "not part of the schema",
        "a variant carries its name, and everything else as child nodes"))
    .children(&[
        Node::new("description", "What the variant is for.").once(""),
        Node::new("set", "One option this variant sets, and what it sets it to.")
            .arg(Arg::Str, Say::new("`set` needs an option name", "no option named", "")),
    ], Say::new("unknown node `{}` in a variant", "not part of the schema",
        "a variant holds `description` and `set`"));

impl From<&KdlValue> for Value {
    fn from(value: &KdlValue) -> Self {
        match value {
            KdlValue::String(s) => Self::String(s.clone()),
            KdlValue::Bool(b) => Self::Bool(*b),
            KdlValue::Integer(i) => Self::Integer(*i),
            KdlValue::Float(f) => Self::Float(*f),
            KdlValue::Null => Self::Null,
        }
    }
}

/// Every unnamed entry of a node, as model values.
pub fn args(node: &KdlNode) -> Vec<Value> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .map(|e| Value::from(e.value()))
        .collect()
}

/// `option "fonts" type="list" { description "..."; default "A" "B" }`
pub fn parse_option(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Opt> {
    let name = string_arg(node)?.to_string();

    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || name.is_empty()
    {
        issues.push(
            Issue::new(format!("invalid option name `{name}`"), src)
                .at(node.name().span(), "lowercase, digits and dashes only")
                .help("the name becomes an env var, uppercased with dashes as underscores and prefixed OPT_"),
        );
    }

    if name == "source" {
        issues.push(
            Issue::new("`source` is not usable as an option name", src)
                .at(node.name().span(), "reserved")
                .help("a `source` child of a list entry is the out-of-tree module pin, so this option could never be set"),
        );
    }

    let ty = match prop(node, "type") {
        Some(t) => match OptType::parse(t) {
            Some(ty) => ty,
            None => {
                issues.push(
                    Issue::new(format!("unknown option type `{t}`"), src)
                        .at(node.name().span(), "not a type")
                        .help("string, bool or list"),
                );
                return None;
            }
        },
        None => {
            issues.push(
                Issue::new(format!("option `{name}` declares no type"), src)
                    .at(node.name().span(), "type= is required")
                    .help("string, bool or list; an untyped option cannot be checked"),
            );
            return None;
        }
    };

    let Some(default) = child(node, "default").map(args) else {
        issues.push(
            Issue::new(format!("option `{name}` declares no default"), src)
                .at(node.name().span(), "every option needs one")
                .help("an option with no default is a required argument in disguise; express that as a `requires` instead"),
        );
        return None;
    };

    check_values(&name, ty, &default, src, node.name().span().into(), issues);

    Some(Opt {
        name,
        ty,
        default,
        span: node.name().span().into(),
    })
}

/// `variant "wine-only" { description "..."; set "dotnet" #false }`
pub fn parse_variant(node: &KdlNode) -> Option<Variant> {
    let sets = kids(node)
        .iter()
        .filter(|c| c.name().value() == "set")
        .filter_map(|c| {
            let values = args(c);
            let opt = values.first()?.as_string()?.to_string();
            Some((opt, values[1..].to_vec(), c.name().span().into()))
        })
        .collect();

    Some(Variant {
        name: string_arg(node)?.to_string(),
        sets,
        span: node.name().span().into(),
    })
}

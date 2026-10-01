//! `option` and `variant`, and the boundary the model's own value type is
//! converted at.

use crate::diag::{Issue, Issues, Source};
use crate::model::options::{check_values, Opt, OptType, Value, Variant};
use crate::parse::schema::{Arg, Kind, Node, Prop, Say};
use crate::parse::{child, kids, prop, string_arg};
use kdl::{KdlNode, KdlValue};

#[rustfmt::skip]
pub const OPTION: Node = Node::new("option",
    "One value that an image can set on this module.").about("Lets an image change how this module builds, such as which fonts it installs. The module declares the option with a type and a default, and each image can set its own value.").example("\"fonts\" type=\"list\"")
    .arg(Arg::Str, Say::new("`option` needs a name", "no name given",
        "`option \"fonts\" type=\"list\" { ... }`"))
    .unique(Say::new("option `{}` is declared twice", "already declared above", ""))
    .props(&[
        Prop { name: "type", kind: Kind::One(&["string", "bool", "list"]),
            desc: "The type of the value. A `string` arrives verbatim, a `bool` arrives as `1` or \
                `0`, and a `list` arrives as a bash array of its strings.",
            say: Say::new("unknown option type `{}`", "not a type", "string, bool or list"),
            missing: Say::new("this `{}` declares no `type`", "type= is required",
                "string, bool or list; an untyped option cannot be checked") },
    ], Say::new("unknown option property `{}`", "not part of the schema",
        "an option carries `type`, and everything else as child nodes"))
    .children(&[
        Node::new("description", "What the option does.").example("\"Nerd Font families to install\"")
            .arg(Arg::Str, Say::NONE).once(""),
        Node::new("default", "The value that the module builds with if no image sets one.").example("\"JetBrainsMono\" \"FiraCode\"")
            .arg(Arg::Strs, Say::NONE)
            .once("")
            .missing(Say::new("this option declares no `{}`", "every option needs one",
                "an option with no default is a required argument in disguise; express that as a `requires` instead")),
    ], Say::new("unknown node `{}` in an option", "not part of the schema",
        "an option holds `description` and `default`"))
    .notes(&[
        "Every declared option reaches the layer of its module as `OPT_<NAME>`. `<NAME>` is the \
         option name in upper case, and each dash becomes an underscore.",
        "A default reaches the layer too, so `module.sh` reads a variable and does not test \
         whether one is set.",
    ]);

#[rustfmt::skip]
pub const VARIANT: Node = Node::new("variant",
    "A named set of option values that an image selects with `variant=`.").about("Bundles option values under one name, so an image takes a whole configuration of the module with `variant=` in place of setting each option.").example("\"wine-only\"")
    .arg(Arg::Str, Say::new("`variant` needs a name", "no name given", ""))
    .unique(Say::new("variant `{}` is declared twice", "already declared above", ""))
    .props(&[], Say::new("unknown variant property `{}`", "not part of the schema",
        "a variant carries its name, and everything else as child nodes"))
    .children(&[
        Node::new("description", "What the variant is for.").example("\"Skip the metadata and .NET payloads\"")
            .arg(Arg::Str, Say::NONE).once(""),
        Node::new("set", "One option that this variant sets, where the option value follows the option name.").example("\"dotnet\" #false")
            .arg(Arg::Str, Say::new("`set` needs an option name", "no option named", "")),
    ], Say::new("unknown node `{}` in a variant", "not part of the schema",
        "a variant holds `description` and `set`"))
    .notes(&[
        "An image can still set an option on an entry that selects a variant. The option that \
         the image sets wins.",
    ]);

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

    // The grammar reports an absent or unknown `type=` and an absent `default`.
    let ty = prop(node, "type").and_then(OptType::parse)?;

    let Some(default) = child(node, "default").map(args) else {
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

#[cfg(test)]
mod tests {
    use super::OPTION;
    use crate::parse::schema::check_text;

    #[test]
    fn an_option_without_type_or_default_is_refused() {
        let issues = check_text(
            "option \"fonts\" {\n    description \"d\"\n}\n",
            &OPTION,
            true,
        )
        .expect("the text is KDL")
        .plain();
        assert!(
            issues.contains("this `option` declares no `type`"),
            "{issues}"
        );
        assert!(
            issues.contains("this option declares no `default`"),
            "{issues}"
        );
    }

    #[test]
    fn an_option_of_an_unknown_type_is_refused() {
        let issues = check_text(
            "option \"fonts\" type=\"number\" {\n    default \"1\"\n}\n",
            &OPTION,
            true,
        )
        .expect("the text is KDL")
        .plain();
        assert!(issues.contains("unknown option type `number`"), "{issues}");
    }
}

//! `option` and `variant`, and the boundary the model's own value type is
//! converted at.

use crate::diag::{Issue, Issues, Source};
use crate::model::options::{check_values, Opt, OptType, Value, Variant, VersionTracker};
use crate::parse::schema::{Arg, Kind, Node, Prop, Say};
use crate::parse::{child, kids, prop, string_arg};
use kdl::{KdlNode, KdlValue};

#[rustfmt::skip]
pub const OPTION: Node = Node::new("option",
    "One value that an image can set on this module.").about("Lets an image change how this module builds, such as which fonts it installs. The module declares the option with a type and a default, and each image can set its own value.").example("\"version\" type=\"string\"")
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
        Node::new("description", "What the option does.").example("\"The Firefox release to install\"")
            .arg(Arg::Str, Say::NONE).once(""),
        Node::new("default", "The value that the module builds with if no image sets one.").example("\"128.0\"")
            .arg(Arg::Strs, Say::NONE)
            .once("")
            .missing(Say::new("this option declares no `{}`", "every option needs one",
                "an option with no default is a required argument in disguise; express that as a `requires` instead")),
        Node::new("renovate", "What keeps the value of a `version` option current.").about("Names where Renovate looks up new versions of the software that a `version` option installs. `tect generate` writes one Renovate manager for each image entry that sets the option, so Renovate updates the value in the image file and the module stays as it is.").example("datasource=\"repology\" depName=\"fedora_rawhide/firefox\"")
            .arg(Arg::None, Say::new("`renovate` takes no arguments", "unexpected value",
                "`renovate datasource=\"repology\" depName=\"fedora_rawhide/firefox\"`"))
            .once("")
            .props(&[
                Prop { name: "datasource", kind: Kind::Str,
                    desc: "The Renovate datasource that publishes the versions.",
                    say: Say::new("`{}` must be a string", "not a string", ""),
                    missing: Say::new("`{}` declares no datasource", "datasource= is required",
                        "a Renovate datasource, such as `repology` or `github-releases`") },
                Prop { name: "depName", kind: Kind::Str,
                    desc: "The name that the datasource knows the software by.",
                    say: Say::new("`{}` must be a string", "not a string", ""),
                    missing: Say::new("`{}` declares no depName", "depName= is required",
                        "the package as the datasource names it") },
                Prop { name: "extractVersion", kind: Kind::Str,
                    desc: "The pattern that extracts the version from what the datasource reports.",
                    say: Say::new("`{}` must be a string", "not a string", ""),
                    missing: Say::NONE },
            ], Say::new("unknown renovate property `{}`", "not part of the schema",
                "datasource, depName and extractVersion, spelled as Renovate spells them")),
    ], Say::new("unknown node `{}` in an option", "not part of the schema",
        "an option holds `description`, `default` and `renovate`"))
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

    if name == "pin" {
        issues.push(
            Issue::new("`pin` is not usable as an option name", src)
                .at(node.name().span(), "reserved")
                .help("an image refuses a `pin` child of a list entry, so this option could never be set"),
        );
    }

    // The grammar reports an absent or unknown `type=` and an absent `default`.
    let ty = prop(node, "type").and_then(OptType::parse)?;

    let default = child(node, "default").map(args)?;

    check_values(&name, ty, &default, src, node.name().span().into(), issues);

    let renovate = child(node, "renovate").and_then(|tracker| {
        if name != "version" || ty != OptType::String {
            issues.push(
                Issue::new("`renovate` tracks a `version` option alone", src)
                    .at(
                        tracker.name().span(),
                        "not a `version` option of type `string`",
                    )
                    .help(
                        "the software version is the value that changes, so the option is \
                         `option \"version\" type=\"string\"`",
                    ),
            );
        }
        // The grammar reports an absent datasource or depName.
        Some(VersionTracker {
            datasource: prop(tracker, "datasource")?.to_string(),
            dep_name: prop(tracker, "depName")?.to_string(),
            extract_version: prop(tracker, "extractVersion").map(str::to_string),
        })
    });

    Some(Opt {
        name,
        ty,
        default,
        span: node.name().span().into(),
        renovate,
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
    use crate::diag::{Issues, Source};
    use crate::parse::schema::check_text;

    fn parsed(text: &str) -> (Option<crate::model::options::Opt>, String) {
        let doc: kdl::KdlDocument = text.parse().expect("the text is KDL");
        let mut issues = Issues::default();
        let opt = super::parse_option(
            &doc.nodes()[0],
            &Source::new("module.kdl", text),
            &mut issues,
        );
        (opt, issues.plain())
    }

    #[test]
    fn a_version_option_carries_its_tracker() {
        let (opt, said) = parsed(
            "option \"version\" type=\"string\" {\n    default \"1\"\n    renovate datasource=\"repology\" depName=\"fedora_rawhide/firefox\"\n}\n",
        );
        assert!(said.is_empty(), "{said}");
        let tracker = opt
            .and_then(|opt| opt.renovate)
            .expect("the tracker is read");
        assert_eq!(tracker.datasource, "repology");
        assert_eq!(tracker.dep_name, "fedora_rawhide/firefox");
        assert_eq!(tracker.extract_version, None);
    }

    /// Generate matches a tracker against the image entry's `version` value,
    /// so a tracker on any other option would update a value nothing reads.
    #[test]
    fn a_tracker_on_another_option_is_refused() {
        for text in [
            "option \"fonts\" type=\"string\" {\n    default \"1\"\n    renovate datasource=\"a\" depName=\"b\"\n}\n",
            "option \"version\" type=\"list\" {\n    default \"1\"\n    renovate datasource=\"a\" depName=\"b\"\n}\n",
        ] {
            let (_, said) = parsed(text);
            assert!(said.contains("tracks a `version` option alone"), "{text}: {said}");
        }
    }

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

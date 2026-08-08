//! `option` and `variant`, and the boundary the model's own value type is
//! converted at.

use crate::diag::{Issue, Issues, Source};
use crate::model::options::{check_values, Opt, OptType, Value, Variant};
use kdl::{KdlNode, KdlValue};

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

fn prop<'a>(node: &'a KdlNode, key: &str) -> Option<&'a KdlValue> {
    node.entries()
        .iter()
        .find(|e| e.name().map(|n| n.value()) == Some(key))
        .map(|e| e.value())
}

/// `option "fonts" type="list" { description "..."; default "A" "B" }`
pub fn parse_option(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Opt> {
    let name = match args(node)
        .first()
        .and_then(|v| v.as_string().map(str::to_string))
    {
        Some(n) => n,
        None => {
            issues.push(
                Issue::new("`option` needs a name", src)
                    .at(node.name().span(), "no name given")
                    .help("`option \"fonts\" type=\"list\" { ... }`"),
            );
            return None;
        }
    };

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

    let ty = match prop(node, "type").and_then(|v| v.as_string()) {
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

    let mut default = None;
    for child in node.children().map(|c| c.nodes()).unwrap_or_default() {
        match child.name().value() {
            "description" => {}
            "default" => default = Some(args(child)),
            other => issues.push(
                Issue::new(format!("unknown node `{other}` in an option"), src)
                    .at(child.name().span(), "not part of the schema")
                    .help("an option holds `description` and `default`"),
            ),
        }
    }

    let Some(default) = default else {
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
pub fn parse_variant(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Variant> {
    let Some(name) = args(node)
        .first()
        .and_then(|v| v.as_string().map(str::to_string))
    else {
        issues.push(
            Issue::new("`variant` needs a name", src).at(node.name().span(), "no name given"),
        );
        return None;
    };

    let mut sets = Vec::new();
    for child in node.children().map(|c| c.nodes()).unwrap_or_default() {
        match child.name().value() {
            "description" => {}
            "set" => {
                let values = args(child);
                let Some(opt) = values.first().and_then(|v| v.as_string()) else {
                    issues.push(
                        Issue::new("`set` needs an option name", src)
                            .at(child.name().span(), "no option named"),
                    );
                    continue;
                };
                sets.push((
                    opt.to_string(),
                    values[1..].to_vec(),
                    child.name().span().into(),
                ));
            }
            other => issues.push(
                Issue::new(format!("unknown node `{other}` in a variant"), src)
                    .at(child.name().span(), "not part of the schema")
                    .help("a variant holds `description` and `set`"),
            ),
        }
    }

    Some(Variant {
        name,
        sets,
        span: node.name().span().into(),
    })
}

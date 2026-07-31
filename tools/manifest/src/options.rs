//! Typed options: declared by the module, set by the image author, resolved
//! here and passed to the layer as env.

use crate::diag::{Issue, Issues};
use crate::list::{Entry, List};
use kdl::{KdlNode, KdlValue};
use miette::SourceSpan;

#[derive(Clone, Copy, PartialEq)]
pub enum OptType {
    String,
    Bool,
    List,
}

impl OptType {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "string" => Some(Self::String),
            "bool" => Some(Self::Bool),
            "list" => Some(Self::List),
            _ => None,
        }
    }
}

pub struct Opt {
    pub name: String,
    pub ty: OptType,
    pub default: Vec<KdlValue>,
    pub span: SourceSpan,
}

pub struct Variant {
    pub name: String,
    pub sets: Vec<(String, Vec<KdlValue>, SourceSpan)>,
    pub span: SourceSpan,
}

fn args(node: &KdlNode) -> Vec<KdlValue> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .map(|e| e.value().clone())
        .collect()
}

fn prop<'a>(node: &'a KdlNode, key: &str) -> Option<&'a KdlValue> {
    node.entries()
        .iter()
        .find(|e| e.name().map(|n| n.value()) == Some(key))
        .map(|e| e.value())
}

/// `option "fonts" type="list" { description "..."; default "A" "B" }`
pub fn parse_option(node: &KdlNode, file: &str, text: &str, issues: &mut Issues) -> Option<Opt> {
    let name = match args(node)
        .first()
        .and_then(|v| v.as_string().map(str::to_string))
    {
        Some(n) => n,
        None => {
            issues.push(
                Issue::new("`option` needs a name", file, text)
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
            Issue::new(format!("invalid option name `{name}`"), file, text)
                .at(node.name().span(), "lowercase, digits and dashes only")
                .help("the name becomes an env var, uppercased with dashes as underscores and prefixed OPT_"),
        );
    }

    if name == "source" {
        issues.push(
            Issue::new("`source` is not usable as an option name", file, text)
                .at(node.name().span(), "reserved")
                .help("a `source` child of a list entry is the out-of-tree module pin, so this option could never be set"),
        );
    }

    let ty = match prop(node, "type").and_then(|v| v.as_string()) {
        Some(t) => match OptType::parse(t) {
            Some(ty) => ty,
            None => {
                issues.push(
                    Issue::new(format!("unknown option type `{t}`"), file, text)
                        .at(node.name().span(), "not a type")
                        .help("string, bool or list"),
                );
                return None;
            }
        },
        None => {
            issues.push(
                Issue::new(format!("option `{name}` declares no type"), file, text)
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
                Issue::new(format!("unknown node `{other}` in an option"), file, text)
                    .at(child.name().span(), "not part of the schema")
                    .help("an option holds `description` and `default`"),
            ),
        }
    }

    let Some(default) = default else {
        issues.push(
            Issue::new(format!("option `{name}` declares no default"), file, text)
                .at(node.name().span(), "every option needs one")
                .help("an option with no default is a required argument in disguise; express that as a `requires` instead"),
        );
        return None;
    };

    check_values(&name, ty, &default, file, text, node.name().span(), issues);

    Some(Opt {
        name,
        ty,
        default,
        span: node.name().span(),
    })
}

/// `variant "wine-only" { description "..."; set "dotnet" #false }`
pub fn parse_variant(
    node: &KdlNode,
    file: &str,
    text: &str,
    issues: &mut Issues,
) -> Option<Variant> {
    let Some(name) = args(node)
        .first()
        .and_then(|v| v.as_string().map(str::to_string))
    else {
        issues.push(
            Issue::new("`variant` needs a name", file, text)
                .at(node.name().span(), "no name given"),
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
                        Issue::new("`set` needs an option name", file, text)
                            .at(child.name().span(), "no option named"),
                    );
                    continue;
                };
                sets.push((opt.to_string(), values[1..].to_vec(), child.name().span()));
            }
            other => issues.push(
                Issue::new(format!("unknown node `{other}` in a variant"), file, text)
                    .at(child.name().span(), "not part of the schema")
                    .help("a variant holds `description` and `set`"),
            ),
        }
    }

    Some(Variant {
        name,
        sets,
        span: node.name().span(),
    })
}

/// Values have to survive being written into a RUN env prefix, so the
/// characters that would end the quoting or start an expansion are rejected
/// rather than escaped.
fn check_values(
    name: &str,
    ty: OptType,
    values: &[KdlValue],
    file: &str,
    text: &str,
    span: SourceSpan,
    issues: &mut Issues,
) -> bool {
    let mut ok = true;
    let mut bad = |msg: String, help: &str| {
        issues.push(
            Issue::new(msg, file, text)
                .at(span, "not usable as this option's value")
                .help(help.to_string()),
        );
    };

    match ty {
        OptType::Bool => {
            if values.len() != 1 || values[0].as_bool().is_none() {
                bad(
                    format!("option `{name}` is a bool, so it takes one #true or #false"),
                    "bools reach the build as 1 or 0",
                );
                ok = false;
            }
        }
        OptType::String => {
            if values.len() != 1 || values[0].as_string().is_none() {
                bad(
                    format!("option `{name}` is a string, so it takes exactly one"),
                    "use type=\"list\" for zero or more values",
                );
                ok = false;
            }
        }
        OptType::List => {
            for value in values {
                let Some(s) = value.as_string() else {
                    bad(
                        format!("option `{name}` is a list, so every value must be a string"),
                        "quote the value",
                    );
                    ok = false;
                    continue;
                };
                if s.chars().any(char::is_whitespace) {
                    bad(
                        format!("list value {s:?} on option `{name}` contains whitespace"),
                        "list values join on spaces to reach the build, so one containing a space would arrive as two",
                    );
                    ok = false;
                }
            }
        }
    }

    for value in values {
        if let Some(s) = value.as_string() {
            if s.contains(['"', '\\', '$', '`', '\n']) {
                bad(
                    format!("value {s:?} on option `{name}` contains a shell metacharacter"),
                    "option values are written into a RUN env prefix, so \" \\ $ ` and newlines are rejected rather than escaped",
                );
                ok = false;
            }
        }
    }
    ok
}

fn env_value(ty: OptType, values: &[KdlValue]) -> String {
    match ty {
        OptType::Bool => match values.first().and_then(|v| v.as_bool()) {
            Some(true) => "1".into(),
            _ => "0".into(),
        },
        OptType::String => values
            .first()
            .and_then(|v| v.as_string())
            .unwrap_or_default()
            .to_string(),
        OptType::List => values
            .iter()
            .filter_map(|v| v.as_string())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// `gpu-accel` becomes `OPT_GPU_ACCEL`.
pub fn env_name(option: &str) -> String {
    format!("OPT_{}", option.to_uppercase().replace('-', "_"))
}

/// Single pass, in one order, with no merging: the module's default, then the
/// selected variant, then the value in image.kdl.
pub fn resolve(
    options: &[Opt],
    variants: &[Variant],
    file: &str,
    text: &str,
    entry: &Entry,
    list: &List,
    issues: &mut Issues,
) -> Vec<(String, String)> {
    let selected = entry.variant.as_ref();
    let set = &entry.options;
    let module_path = entry.path.as_str();
    let (list_file, list_text) = (list.file.as_str(), list.text.as_str());

    let mut resolved: Vec<(String, Vec<KdlValue>)> = options
        .iter()
        .map(|o| (o.name.clone(), o.default.clone()))
        .collect();

    let find = |name: &str| options.iter().find(|o| o.name == name);

    if let Some(want) = selected {
        match variants.iter().find(|v| &v.name == want) {
            Some(variant) => {
                for (name, values, span) in &variant.sets {
                    let Some(opt) = find(name) else {
                        issues.push(
                            Issue::new(
                                format!("variant `{want}` sets `{name}`, which this module does not declare"),
                                file,
                                text,
                            )
                            .at(*span, "no such option")
                            .help("a variant may only set options declared in the same manifest"),
                        );
                        continue;
                    };
                    if check_values(name, opt.ty, values, file, text, *span, issues) {
                        if let Some(slot) = resolved.iter_mut().find(|(n, _)| n == name) {
                            slot.1 = values.clone();
                        }
                    }
                }
            }
            None => {
                let known: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
                issues.push(
                    Issue::new(
                        format!("`{module_path}` has no variant `{want}`"),
                        list_file,
                        list_text,
                    )
                    .help(if known.is_empty() {
                        "this module declares no variants".to_string()
                    } else {
                        format!("declared variants: {}", known.join(", "))
                    }),
                );
            }
        }
    }

    let mut seen: Vec<&str> = Vec::new();
    for (name, values, span) in set {
        let Some(opt) = find(name) else {
            let known: Vec<&str> = options.iter().map(|o| o.name.as_str()).collect();
            issues.push(
                Issue::new(
                    format!("`{module_path}` has no option `{name}`"),
                    list_file,
                    list_text,
                )
                .at(*span, "not declared by this module")
                .help(if known.is_empty() {
                    "this module declares no options".to_string()
                } else {
                    format!("declared options: {}", known.join(", "))
                }),
            );
            continue;
        };
        if seen.contains(&name.as_str()) {
            issues.push(
                Issue::new(
                    format!("`{name}` is set twice on `{module_path}`"),
                    list_file,
                    list_text,
                )
                .at(*span, "set again here")
                .help("resolution is a single pass, so a second value is an error rather than a merge"),
            );
            continue;
        }
        seen.push(name.as_str());

        if check_values(name, opt.ty, values, list_file, list_text, *span, issues) {
            if let Some(slot) = resolved.iter_mut().find(|(n, _)| n == name) {
                slot.1 = values.clone();
            }
        }
    }

    options
        .iter()
        .map(|opt| {
            let values = resolved
                .iter()
                .find(|(n, _)| n == &opt.name)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            (env_name(&opt.name), env_value(opt.ty, &values))
        })
        .collect()
}

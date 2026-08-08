//! Typed options: declared by the module, set by the image author, resolved
//! against both and passed to the layer as env.

use crate::diag::{Issue, Issues, Source, Span};

/// A declared value, owned by the model: `parse` is the only thing that sees
/// KDL's own value type.
#[derive(Clone)]
pub enum Value {
    String(String),
    Bool(bool),
    Integer(i128),
    Float(f64),
    Null,
}

impl Value {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum OptType {
    String,
    Bool,
    List,
}

impl OptType {
    pub(crate) fn parse(name: &str) -> Option<Self> {
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
    pub default: Vec<Value>,
    pub span: Span,
}

pub struct Variant {
    pub name: String,
    pub sets: Vec<(String, Vec<Value>, Span)>,
    pub span: Span,
}

/// `gpu-accel` becomes `OPT_GPU_ACCEL`.
pub fn env_name(option: &str) -> String {
    format!("OPT_{}", option.to_uppercase().replace('-', "_"))
}

pub(crate) fn env_value(ty: OptType, values: &[Value]) -> String {
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

/// Values have to survive being written into a RUN env prefix, so the
/// characters that would end the quoting or start an expansion are rejected
/// rather than escaped.
pub(crate) fn check_values(
    name: &str,
    ty: OptType,
    values: &[Value],
    src: &Source,
    span: Span,
    issues: &mut Issues,
) -> bool {
    let mut ok = true;
    let mut bad = |msg: String, help: &str| {
        issues.push(
            Issue::new(msg, src)
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

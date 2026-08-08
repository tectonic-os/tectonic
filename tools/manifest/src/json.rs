//! Just enough JSON to write the plan.

use std::fmt::Write as _;

pub enum Json {
    Null,
    Bool(bool),
    Number(u32),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn string(value: impl Into<String>) -> Self {
        Json::String(value.into())
    }

    /// An absent value as null rather than an absent key, so every object of a
    /// given kind has the same shape and a reader never has to tell "not
    /// declared" from "misspelled the key".
    pub fn optional(value: Option<impl Into<String>>) -> Self {
        match value {
            Some(value) => Json::String(value.into()),
            None => Json::Null,
        }
    }

    pub fn array(items: impl IntoIterator<Item = Json>) -> Self {
        Json::Array(items.into_iter().collect())
    }

    pub fn strings(items: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Json::Array(items.into_iter().map(Json::string).collect())
    }

    pub fn object(fields: impl IntoIterator<Item = (&'static str, Json)>) -> Self {
        Json::Object(
            fields
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
        )
    }

    /// An object whose keys come out of the data rather than the schema: a
    /// path to the module that owns it, an option name to its value.
    pub fn map(entries: impl IntoIterator<Item = (String, Json)>) -> Self {
        Json::Object(entries.into_iter().collect())
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        let pad = "  ".repeat(depth + 1);
        let close = "  ".repeat(depth);
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Json::Number(value) => {
                let _ = write!(out, "{value}");
            }
            Json::String(value) => escape(value, out),
            Json::Array(items) if items.is_empty() => out.push_str("[]"),
            Json::Array(items) => {
                out.push_str("[\n");
                for (index, item) in items.iter().enumerate() {
                    out.push_str(&pad);
                    item.write(out, depth + 1);
                    out.push_str(if index + 1 == items.len() { "\n" } else { ",\n" });
                }
                out.push_str(&close);
                out.push(']');
            }
            Json::Object(fields) if fields.is_empty() => out.push_str("{}"),
            Json::Object(fields) => {
                out.push_str("{\n");
                for (index, (name, value)) in fields.iter().enumerate() {
                    out.push_str(&pad);
                    escape(name, out);
                    out.push_str(": ");
                    value.write(out, depth + 1);
                    out.push_str(if index + 1 == fields.len() { "\n" } else { ",\n" });
                }
                out.push_str(&close);
                out.push('}');
            }
        }
    }
}

/// Nothing here is trusted to be printable: a description, an option value and
/// a module path all come out of a file somebody else wrote.
fn escape(value: &str, out: &mut String) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

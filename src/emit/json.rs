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

    pub fn parse(text: &str) -> Result<Json, String> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            pos: 0,
        };
        parser.skip();
        let value = parser.value(0)?;
        parser.skip();
        if parser.pos != parser.bytes.len() {
            return Err(parser.error("trailing content"));
        }
        Ok(value)
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
                    out.push_str(if index + 1 == items.len() {
                        "\n"
                    } else {
                        ",\n"
                    });
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
                    out.push_str(if index + 1 == fields.len() {
                        "\n"
                    } else {
                        ",\n"
                    });
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

/// How far a document may nest before it is refused rather than recursed into.
const MAX_DEPTH: usize = 128;

/// A byte cursor: positions are byte offsets, which is what an error reports.
struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        format!("{message} at byte {}", self.pos)
    }

    fn skip(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > MAX_DEPTH {
            return Err(self.error("nesting too deep"));
        }
        match self.bytes.get(self.pos) {
            None => Err(self.error("expected a value")),
            Some(b'n') => self.literal(b"null", Json::Null),
            Some(b't') => self.literal(b"true", Json::Bool(true)),
            Some(b'f') => self.literal(b"false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::String),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'0'..=b'9') => self.number().map(Json::Number),
            _ => Err(self.error("expected a value")),
        }
    }

    fn literal(&mut self, word: &[u8], value: Json) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("expected a value"))
        }
    }

    /// Unsigned integers only: a leading digit, then digits, then a terminator.
    fn number(&mut self) -> Result<u32, String> {
        let mut value: u32 = 0;
        while let Some(byte) = self.bytes.get(self.pos) {
            match byte {
                b'0'..=b'9' => {
                    value = value
                        .checked_mul(10)
                        .and_then(|v| v.checked_add((byte - b'0') as u32))
                        .ok_or_else(|| self.error("number too large"))?;
                    self.pos += 1;
                }
                b'.' | b'e' | b'E' => return Err(self.error("expected a digit")),
                _ => break,
            }
        }
        Ok(value)
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.bytes.get(self.pos) {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let escaped = *self
                        .bytes
                        .get(self.pos)
                        .ok_or_else(|| self.error("unterminated string"))?;
                    self.pos += 1;
                    match escaped {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.escape()?),
                        _ => return Err(self.error("invalid escape")),
                    }
                }
                Some(_) => {
                    let rest = &self.bytes[self.pos..];
                    let c = rest[0] as char;
                    if c.is_ascii() && (c as u8) < 0x20 {
                        return Err(self.error("invalid character in string"));
                    }
                    let width = utf8_width(rest[0]).min(rest.len());
                    let c = std::str::from_utf8(&rest[..width])
                        .map_err(|_| self.error("invalid utf-8"))?
                        .chars()
                        .next()
                        .unwrap();
                    out.push(c);
                    self.pos += width;
                }
            }
        }
    }

    /// One `\uXXXX`, including the half of a surrogate pair it belongs to.
    fn escape(&mut self) -> Result<char, String> {
        let code = self.hex4()?;
        if !(0xD800..=0xDBFF).contains(&code) {
            if (0xDC00..=0xDFFF).contains(&code) {
                return Err(self.error("invalid surrogate pair"));
            }
            return char::from_u32(code).ok_or_else(|| self.error("invalid unicode escape"));
        }
        if self.bytes.get(self.pos) != Some(&b'\\') || self.bytes.get(self.pos + 1) != Some(&b'u') {
            return Err(self.error("invalid surrogate pair"));
        }
        self.pos += 2;
        let low = self.hex4()?;
        if !(0xDC00..=0xDFFF).contains(&low) {
            return Err(self.error("invalid surrogate pair"));
        }
        Ok(char::from_u32(0x1_0000 + ((code - 0xD800) << 10) + (low - 0xDC00)).unwrap())
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut code: u32 = 0;
        for _ in 0..4 {
            let digit = self
                .bytes
                .get(self.pos)
                .copied()
                .and_then(hex_value)
                .ok_or_else(|| self.error("invalid unicode escape"))?;
            self.pos += 1;
            code = code * 16 + digit as u32;
        }
        Ok(code)
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.pos += 1;
        self.skip();
        if self.bytes.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(Json::Array(Vec::new()));
        }
        let mut items = Vec::new();
        loop {
            items.push(self.value(depth + 1)?);
            self.skip();
            match self.bytes.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                    self.skip();
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.pos += 1;
        self.skip();
        if self.bytes.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(Json::Object(Vec::new()));
        }
        let mut fields = Vec::new();
        loop {
            if self.bytes.get(self.pos) != Some(&b'"') {
                return Err(self.error("expected a string key"));
            }
            let key = self.string()?;
            self.skip();
            if self.bytes.get(self.pos) != Some(&b':') {
                return Err(self.error("expected ':'"));
            }
            self.pos += 1;
            self.skip();
            let value = self.value(depth + 1)?;
            fields.push((key, value));
            self.skip();
            match self.bytes.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                    self.skip();
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(fields));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn utf8_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Reading a document back, which the manifest and the build record an image
/// carries are the only cases of: everything else here writes one.
pub fn field<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    match value {
        Json::Object(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, v)| v),
        _ => None,
    }
}

pub fn text(value: &Json, key: &str) -> Option<String> {
    match field(value, key) {
        Some(Json::String(found)) => Some(found.clone()),
        _ => None,
    }
}

pub fn number(value: &Json, key: &str) -> Option<u32> {
    match field(value, key) {
        Some(Json::Number(found)) => Some(*found),
        _ => None,
    }
}

pub fn items<'a>(value: &'a Json, key: &str) -> &'a [Json] {
    match field(value, key) {
        Some(Json::Array(found)) => found,
        _ => &[],
    }
}

pub fn strings(value: &Json, key: &str) -> Vec<String> {
    items(value, key)
        .iter()
        .filter_map(|item| match item {
            Json::String(found) => Some(found.clone()),
            _ => None,
        })
        .collect()
}

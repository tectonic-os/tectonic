//! Just enough XML for the two documents a scan produces. Both are namespaced
//! and the namespace moves between revisions, so every match is on the local
//! name.

/// One thing the scanner found. Text arrives as it lies between two tags, so a
/// value an entity or a comment splits is not reassembled; nothing read here
/// is prose.
pub enum Event<'a> {
    Open { name: &'a str, attrs: &'a str },
    Close { name: &'a str },
    Text(&'a str),
}

pub fn scan(text: &str) -> Scan<'_> {
    Scan {
        rest: text,
        empty: None,
    }
}

pub struct Scan<'a> {
    rest: &'a str,
    /// The close an empty element still owes.
    empty: Option<&'a str>,
}

impl<'a> Iterator for Scan<'a> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Event<'a>> {
        if let Some(name) = self.empty.take() {
            return Some(Event::Close { name });
        }
        loop {
            let rest = self.rest;
            let Some(tail) = rest.strip_prefix('<') else {
                if rest.is_empty() {
                    return None;
                }
                let at = rest.find('<').unwrap_or(rest.len());
                self.rest = &rest[at..];
                match rest[..at].trim() {
                    "" => continue,
                    text => return Some(Event::Text(text)),
                }
            };
            if let Some(after) = tail.strip_prefix("![CDATA[") {
                let (text, rest) = until(after, "]]>");
                self.rest = rest;
                match text.trim() {
                    "" => continue,
                    text => return Some(Event::Text(text)),
                }
            }
            if tail.starts_with('!') || tail.starts_with('?') {
                self.rest = until(tail, if tail.starts_with("!--") { "-->" } else { ">" }).1;
                continue;
            }
            let (tag, rest) = tag(tail);
            self.rest = rest;
            if let Some(name) = tag.strip_prefix('/') {
                return Some(Event::Close {
                    name: local(name.trim()),
                });
            }
            let closes = tag.trim_end().ends_with('/');
            let tag = tag.trim_end().trim_end_matches('/');
            let at = tag
                .find(|c: char| c.is_ascii_whitespace())
                .unwrap_or(tag.len());
            let name = local(&tag[..at]);
            if closes {
                self.empty = Some(name);
            }
            return Some(Event::Open {
                name,
                attrs: &tag[at..],
            });
        }
    }
}

/// One attribute's value, matched on the local name like every element is.
pub fn attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = attrs;
    while let Some(at) = rest.find(name) {
        let before = &rest[..at];
        rest = &rest[at + name.len()..];
        let named = before
            .chars()
            .last()
            .is_none_or(|c| c.is_ascii_whitespace() || c == ':');
        let value = rest.trim_start();
        if !named || !value.starts_with('=') {
            continue;
        }
        let value = value[1..].trim_start();
        match value.get(..1) {
            Some(quote @ ("\"" | "'")) => return Some(until(&value[1..], quote).0),
            _ => continue,
        }
    }
    None
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn until<'a>(text: &'a str, end: &str) -> (&'a str, &'a str) {
    match text.find(end) {
        Some(at) => (&text[..at], &text[at + end.len()..]),
        None => (text, ""),
    }
}

/// The tag text up to the `>` that closes it, which is never one inside a
/// quoted attribute value.
fn tag(text: &str) -> (&str, &str) {
    let mut quote = 0u8;
    for (at, byte) in text.bytes().enumerate() {
        match (quote, byte) {
            (0, b'"' | b'\'') => quote = byte,
            (0, b'>') => return (&text[..at], &text[at + 1..]),
            (open, byte) if open == byte => quote = 0,
            _ => {}
        }
    }
    (text, "")
}

//! quick-xml owns XML syntax while this adapter emits the events SCAP readers
//! need.

use quick_xml::events::{BytesDecl, BytesStart, Event as XmlEvent};
use quick_xml::{Reader, XmlVersion};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    Open {
        name: String,
        attrs: Vec<(String, String)>,
    },
    Close {
        name: String,
    },
    Text(String),
}

pub(super) fn scan(text: &str, mut emit: impl FnMut(Event)) -> Result<(), String> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().enable_all_checks(true);
    let mut text = String::new();
    let mut depth = 0usize;
    let mut saw_root = false;
    let mut may_declare = true;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| format!("invalid XML at byte {}: {error}", reader.error_position()))?;
        match event {
            XmlEvent::Start(start) => {
                flush(&mut text, depth, &mut emit)?;
                if depth == 0 && saw_root {
                    return Err("invalid XML: more than one root element".to_string());
                }
                saw_root = true;
                depth += 1;
                emit(open(start)?);
            }
            XmlEvent::Empty(start) => {
                flush(&mut text, depth, &mut emit)?;
                if depth == 0 && saw_root {
                    return Err("invalid XML: more than one root element".to_string());
                }
                saw_root = true;
                let opened = open(start)?;
                let Event::Open { name, attrs } = opened else {
                    unreachable!("open always returns an opening event");
                };
                emit(Event::Open {
                    name: name.clone(),
                    attrs,
                });
                emit(Event::Close { name });
            }
            XmlEvent::End(end) => {
                flush(&mut text, depth, &mut emit)?;
                if depth == 0 {
                    return Err("invalid XML: closing tag outside the root element".to_string());
                }
                valid_qname(end.name().as_ref())?;
                emit(Event::Close {
                    name: end.name().as_ref().to_string(),
                });
                depth -= 1;
            }
            XmlEvent::Text(found) => {
                let found = found.xml_content(XmlVersion::Implicit1_0);
                valid_characters(&found)?;
                if !found.is_empty() {
                    may_declare = false;
                }
                text.push_str(&found);
            }
            XmlEvent::CData(found) => {
                if depth == 0 {
                    return Err("invalid XML: CDATA outside the root element".to_string());
                }
                let found = found.xml_content(XmlVersion::Implicit1_0);
                valid_characters(&found)?;
                text.push_str(&found);
            }
            XmlEvent::GeneralRef(reference) => {
                if depth == 0 {
                    return Err("invalid XML: reference outside the root element".to_string());
                }
                if let Some(found) = reference
                    .resolve_char_ref()
                    .map_err(|error| error.to_string())?
                {
                    if !is_xml_character(found) {
                        return Err(format!(
                            "invalid XML character reference `&{};`",
                            reference.as_ref()
                        ));
                    }
                    text.push(found);
                } else if let Some(found) = quick_xml::escape::resolve_predefined_entity(&reference)
                {
                    text.push_str(found);
                } else {
                    return Err(format!("unsupported XML entity `&{};`", reference.as_ref()));
                }
            }
            XmlEvent::Decl(decl) => {
                if !may_declare || saw_root {
                    return Err("invalid XML: declaration is not first".to_string());
                }
                declaration(&decl)?;
                may_declare = false;
            }
            XmlEvent::PI(instruction) => {
                valid_name(instruction.target())?;
                if instruction.target().eq_ignore_ascii_case("xml") {
                    return Err("invalid XML: reserved processing-instruction target".to_string());
                }
                valid_characters(instruction.content())?;
                may_declare = false;
            }
            XmlEvent::Comment(comment) => {
                valid_characters(comment.as_ref())?;
                may_declare = false;
            }
            XmlEvent::DocType(_) => {
                return Err("unsupported XML document type declaration".to_string());
            }
            XmlEvent::Eof => {
                flush(&mut text, depth, &mut emit)?;
                return match (saw_root, depth) {
                    (false, _) => Err("invalid XML: no root element".to_string()),
                    (true, 0) => Ok(()),
                    (true, _) => Err("invalid XML: unclosed root element".to_string()),
                };
            }
        }
    }
}

fn open(start: BytesStart<'_>) -> Result<Event, String> {
    let name = start.name().as_ref().to_string();
    valid_qname(&name)?;
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|error| error.to_string())?;
        valid_qname(attr.key.as_ref())?;
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| error.to_string())?
            .into_owned();
        valid_characters(&value)?;
        attrs.push((attr.key.as_ref().to_string(), value));
    }
    Ok(Event::Open { name, attrs })
}

fn declaration(decl: &BytesDecl<'_>) -> Result<(), String> {
    match decl.xml_version().map_err(|error| error.to_string())? {
        XmlVersion::Explicit1_0 => {}
        XmlVersion::Explicit1_1 => {
            return Err("unsupported XML version 1.1".to_string());
        }
        XmlVersion::Implicit1_0 => unreachable!("a declaration names its version"),
    }

    let start = BytesStart::from_content(decl.as_ref(), 3);
    let mut fields = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|error| error.to_string())?;
        fields.push((
            attr.key.as_ref().to_string(),
            attr.value.as_ref().to_string(),
        ));
    }
    let mut at = 1;
    if fields.get(at).is_some_and(|(name, _)| name == "encoding") {
        let encoding = &fields[at].1;
        if !encoding.eq_ignore_ascii_case("utf-8") {
            return Err(format!("unsupported XML encoding `{encoding}`"));
        }
        at += 1;
    }
    if fields.get(at).is_some_and(|(name, _)| name == "standalone") {
        let standalone = &fields[at].1;
        if !matches!(standalone.as_str(), "yes" | "no") {
            return Err("invalid XML standalone declaration".to_string());
        }
        at += 1;
    }
    if at != fields.len() {
        return Err("invalid XML declaration".to_string());
    }
    Ok(())
}

fn flush(text: &mut String, depth: usize, emit: &mut impl FnMut(Event)) -> Result<(), String> {
    if depth == 0 {
        if !text.chars().all(is_xml_space) {
            return Err("invalid XML: text outside the root element".to_string());
        }
    } else if !text.is_empty() {
        emit(Event::Text(std::mem::take(text)));
    }
    text.clear();
    Ok(())
}

fn valid_qname(name: &str) -> Result<(), String> {
    let mut parts = name.split(':');
    let first = parts.next().unwrap_or_default();
    let second = parts.next();
    if valid_ncname(first) && second.is_none_or(valid_ncname) && parts.next().is_none() {
        Ok(())
    } else {
        Err(format!("invalid XML qualified name `{name}`"))
    }
}

fn valid_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    if chars.next().is_some_and(is_name_start) && chars.all(is_name_character) {
        Ok(())
    } else {
        Err(format!("invalid XML name `{name}`"))
    }
}

fn valid_ncname(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(is_name_start) && chars.all(is_name_character)
}

fn is_name_start(found: char) -> bool {
    matches!(
        found as u32,
        0x3A | 0x41..=0x5A | 0x5F | 0x61..=0x7A | 0xC0..=0xD6 | 0xD8..=0xF6
            | 0xF8..=0x2FF | 0x370..=0x37D | 0x37F..=0x1FFF | 0x200C..=0x200D
            | 0x2070..=0x218F | 0x2C00..=0x2FEF | 0x3001..=0xD7FF | 0xF900..=0xFDCF
            | 0xFDF0..=0xFFFD | 0x10000..=0xEFFFF
    )
}

fn is_name_character(found: char) -> bool {
    is_name_start(found)
        || matches!(found as u32, 0x2D | 0x2E | 0x30..=0x39 | 0xB7 | 0x300..=0x36F | 0x203F..=0x2040)
}

fn valid_characters(text: &str) -> Result<(), String> {
    match text.chars().find(|found| !is_xml_character(*found)) {
        Some(found) => Err(format!("invalid XML character U+{:04X}", found as u32)),
        None => Ok(()),
    }
}

fn is_xml_character(found: char) -> bool {
    matches!(found as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn is_xml_space(found: char) -> bool {
    matches!(found, ' ' | '\t' | '\n' | '\r')
}

pub(super) fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

pub(super) fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(found, _)| local(found) == name)
        .map(|(_, value)| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_attributes_entities_and_empty_elements_are_normalised() {
        let mut events = Vec::new();
        scan(
            r#"<?xml version="1.0"?><x:root x:id="one &amp; two">before&amp;<![CDATA[after]]><x:empty/></x:root>"#,
            |event| events.push(event),
        )
        .unwrap();
        assert_eq!(
            events,
            [
                Event::Open {
                    name: "x:root".into(),
                    attrs: vec![("x:id".into(), "one & two".into())],
                },
                Event::Text("before&after".into()),
                Event::Open {
                    name: "x:empty".into(),
                    attrs: Vec::new(),
                },
                Event::Close {
                    name: "x:empty".into(),
                },
                Event::Close {
                    name: "x:root".into(),
                },
            ]
        );
    }

    #[test]
    fn the_supported_document_prolog_is_checked() {
        assert!(scan(
            "<?xml version='1.0' encoding='UTF-8' standalone='yes'?><!--before--><root/><?after?>",
            |_| {}
        )
        .is_ok());
        for text in [
            " <?xml version='1.0'?><root/>",
            "<!--before--><?xml version='1.0'?><root/>",
            "<?xml version='1.0' encoding='latin1'?><root/>",
            "<?xml version='1.0' standalone='maybe'?><root/>",
            "<?xml version='1.0' extra='value'?><root/>",
            "<?xml version='1.0'?><?xml version='1.0'?><root/>",
            "<!DOCTYPE root><root/>",
        ] {
            assert!(scan(text, |_| {}).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn malformed_xml_is_refused() {
        assert!(scan("<root><child></root>", |_| {}).is_err());
        assert!(scan("<root><child></child>", |_| {}).is_err());
        assert!(scan("", |_| {}).is_err());
        assert!(scan("<one/><two/>", |_| {}).is_err());
        assert!(scan("<root><!-- one -- two --></root>", |_| {}).is_err());
        assert!(scan("<root>&unknown;</root>", |_| {}).is_err());
        assert!(scan("before<root/>", |_| {}).is_err());
        assert!(scan("<root/><!DOCTYPE later>", |_| {}).is_err());
        assert!(scan("<root><![CDATA[text]]></root>", |_| {}).is_ok());
        assert!(scan("<![CDATA[ ]]><root/>", |_| {}).is_err());
        for text in ["<1root/>", "<root!/> "] {
            let refused = scan(text, |_| {}).expect_err("an invalid XML name");
            assert!(refused.contains("qualified name"), "{text:?}: {refused}");
        }
        assert!(scan("<root bad:name:again='value'/>", |_| {}).is_err());
        assert!(scan("<root>&#1;</root>", |_| {}).is_err());
        assert!(scan("<?xml version='1.1'?><root/>", |_| {}).is_err());
        assert!(scan("<root/><?xml version='1.0'?>", |_| {}).is_err());
        assert!(scan("<?XmL instruction?><root/>", |_| {}).is_err());
        assert!(scan("<root><?1bad?></root>", |_| {}).is_err());
        assert!(scan("<root>\u{1}</root>", |_| {}).is_err());
    }
}

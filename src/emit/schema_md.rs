//! The generated half of docs/schema.md, rendered from the tables the parser
//! already reads.

use crate::parse::schema::{Arg, Kind, Node, Prop};
use crate::parse::{asset, bases, image, module, options, repo};
use crate::provenance::{evidence, record};
use std::fmt::Write as _;

/// One splice region. `declared` is false for a grammar whose own node is not
/// written in the file, the file's top-level nodes being its children.
struct Section {
    name: &'static str,
    node: &'static Node,
    declared: bool,
}

#[rustfmt::skip]
const SECTIONS: &[Section] = &[
    Section { name: "repo", node: &repo::REPO, declared: false },
    Section { name: "image", node: &image::IMAGE, declared: true },
    Section { name: "bases", node: &bases::BASES, declared: false },
    Section { name: "module", node: &module::MODULE, declared: false },
    Section { name: "option", node: &options::OPTION, declared: true },
    Section { name: "variant", node: &options::VARIANT, declared: true },
    Section { name: "asset", node: &asset::ASSET, declared: true },
    Section { name: "pin", node: &evidence::PIN, declared: true },
    Section { name: "imported", node: &record::IMPORTED, declared: true },
];

/// A node with a region of its own, which is documented there and linked to
/// from every grammar holding it. Matched on the pair, `module` being both the
/// manifest and a list entry.
fn section_of(node: &Node) -> Option<&'static Section> {
    SECTIONS
        .iter()
        .find(|s| s.node.name == node.name && s.node.desc == node.desc)
}

/// The name a document writes, an author-named node having none of its own.
fn named(node: &Node) -> String {
    match node.name.is_empty() {
        true => "`<name>`".to_string(),
        false => format!("`{}`", node.name),
    }
}

/// What the walker accepts of one node, as the reference reads it.
fn shape(node: &Node) -> Vec<String> {
    let mut facts: Vec<String> = Vec::new();
    match node.arg {
        Arg::None => {}
        Arg::Str => facts.push("a string".into()),
        Arg::Bool => facts.push("`#true` or `#false`".into()),
        Arg::Int => facts.push("a number".into()),
        Arg::Strs => facts.push("one or more strings".into()),
        Arg::StrPair(roles) => facts.push(format!("two strings: {roles}")),
        Arg::One(set) => facts.push(closed(set)),
    }
    match (!node.missing.text.is_empty(), node.once) {
        (true, true) => facts.push("exactly one".into()),
        (true, false) => facts.push("required".into()),
        (false, true) => facts.push("at most one".into()),
        (false, false) => {}
    }
    if !node.unique.text.is_empty() {
        facts.push("one per name".into());
    }
    if !node.empty.text.is_empty() {
        facts.push("never empty".into());
    }
    facts
}

fn closed(set: &[&str]) -> String {
    set.iter()
        .map(|v| format!("`{v}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn value(prop: &Prop) -> String {
    let mut out = match prop.kind {
        Kind::Str => "a string".to_string(),
        Kind::Bool => "`#true` or `#false`".to_string(),
        Kind::Int(low, high) => format!("{low} to {high}"),
        Kind::One(set) => closed(set),
    };
    if !prop.missing.text.is_empty() {
        out.push_str(", required");
    }
    out
}

/// A list as a sentence reads it.
fn sentence(parts: &[String]) -> String {
    match parts.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// One node under its own heading, then everything below it.
fn node(node: &Node, depth: usize, seen: &mut Vec<&'static str>, out: &mut String) {
    seen.push(node.desc);
    let _ = writeln!(out, "{} {}\n", "#".repeat(depth), named(node));
    let _ = writeln!(out, "{}\n", node.desc);
    let facts = shape(node);
    if !facts.is_empty() {
        let _ = writeln!(out, "*{}*\n", facts.join(", "));
    }

    if !node.props.is_empty() {
        let _ = writeln!(out, "| Property | Value | Meaning |\n| --- | --- | --- |");
        for prop in node.props {
            let _ = writeln!(
                out,
                "| `{}=` | {} | {} |",
                prop.name,
                value(prop),
                prop.desc
            );
        }
        out.push('\n');
    }

    children(node, depth, seen, out);
}

/// Every child: the ones documented elsewhere as a link or a back-reference,
/// the leaves as one table, and the rest under headings of their own.
fn children(parent: &Node, depth: usize, seen: &mut Vec<&'static str>, out: &mut String) {
    let mut elsewhere: Vec<String> = Vec::new();
    let mut here: Vec<&Node> = Vec::new();
    for child in parent.children {
        match section_of(child) {
            Some(s) => elsewhere.push(format!("[`{}`](#{})", s.node.name, s.name)),
            None if seen.contains(&child.desc) => {
                elsewhere.push(format!("{}, as above", named(child)))
            }
            None => here.push(child),
        }
    }
    if !elsewhere.is_empty() {
        let _ = writeln!(out, "Also holds {}.\n", sentence(&elsewhere));
    }

    let (leaves, blocks): (Vec<&Node>, Vec<&Node>) = here
        .into_iter()
        .partition(|child| child.props.is_empty() && child.children.is_empty());

    if !leaves.is_empty() {
        let _ = writeln!(out, "| Node | Takes | Meaning |\n| --- | --- | --- |");
        for leaf in leaves {
            let _ = writeln!(
                out,
                "| {} | {} | {} |",
                named(leaf),
                shape(leaf).join(", "),
                leaf.desc
            );
        }
        out.push('\n');
    }

    for block in blocks {
        node(block, depth + 1, seen, out);
    }
}

/// `<!-- schema: name -->`, or the closing form, as the schema it names.
fn marker<'a>(line: &'a str, tag: &str) -> Option<&'a str> {
    line.trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim()
        .strip_prefix(tag)
        .map(str::trim)
}

/// The document with every splice region rewritten from the tables. Fails when
/// a marker names no schema, and when a schema has no marker, so neither half
/// can be added without the other.
pub fn render(doc: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut open: Option<&str> = None;
    let mut spliced: Vec<&str> = Vec::new();

    for line in doc.lines() {
        if let Some(name) = marker(line, "schema:") {
            if let Some(open) = open {
                return Err(format!("`{open}` is still open at `{name}`"));
            }
            let Some(section) = SECTIONS.iter().find(|s| s.name == name) else {
                let known: Vec<&str> = SECTIONS.iter().map(|s| s.name).collect();
                return Err(format!(
                    "`{name}` marks no schema; the schemas are {}",
                    known.join(", ")
                ));
            };
            let _ = writeln!(out, "{line}\n");
            let seen = &mut Vec::new();
            match section.declared {
                true => node(section.node, 3, seen, &mut out),
                false => children(section.node, 2, seen, &mut out),
            }
            open = Some(section.name);
            spliced.push(section.name);
            continue;
        }
        if let Some(name) = marker(line, "/schema:") {
            if open != Some(name) {
                return Err(format!("`{name}` closes nothing"));
            }
            open = None;
        }
        if open.is_none() {
            let _ = writeln!(out, "{line}");
        }
    }

    if let Some(open) = open {
        return Err(format!("`{open}` is never closed"));
    }
    match SECTIONS.iter().find(|s| !spliced.contains(&s.name)) {
        Some(missed) => Err(format!(
            "`{}` has no marker; every schema is documented, so add \
             `<!-- schema: {} -->` under a heading that says what it is for",
            missed.name, missed.name
        )),
        None => Ok(out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_naming_no_schema_fails() {
        let err = render("<!-- schema: colour -->\n<!-- /schema: colour -->\n").unwrap_err();
        assert!(err.starts_with("`colour` marks no schema"), "{err}");
    }

    #[test]
    fn a_schema_with_no_marker_fails() {
        let err = render("nothing here\n").unwrap_err();
        assert!(err.starts_with("`repo` has no marker"), "{err}");
    }

    #[test]
    fn the_generated_half_is_replaced_and_the_rest_is_kept() {
        let doc: String = SECTIONS
            .iter()
            .map(|s| {
                format!(
                    "## {}\n\n<!-- schema: {0} -->\nstale\n<!-- /schema: {0} -->\n",
                    s.name
                )
            })
            .collect();
        let out = render(&doc).expect("every schema is marked");
        assert!(!out.contains("stale"));
        assert!(out.contains("### `image`"));
        assert!(out.contains("## repo"));
    }
}

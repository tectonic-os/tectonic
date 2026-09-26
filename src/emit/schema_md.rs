//! The generated half of the schema reference, rendered from the tables the
//! parser already reads. `docs/schema.md` indexes one file per reader area
//! under `docs/schema/`.

use crate::parse::schema::{Arg, Kind, Node, Prop};
use crate::parse::{asset, bases, image, module, options, repo};
use crate::provenance::{evidence, record};
use std::fmt::Write as _;

/// A reader area groups the schemas one reader needs onto one page under
/// `docs/schema/`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Area {
    Repo,
    Image,
    Module,
    Bases,
    Pins,
}

impl Area {
    pub fn file(self) -> &'static str {
        match self {
            Area::Repo => "repo.md",
            Area::Image => "image.md",
            Area::Module => "module.md",
            Area::Bases => "bases.md",
            Area::Pins => "pins.md",
        }
    }
}

/// One splice region. `declared` is false for a grammar whose own node is not
/// written in the file, the file's top-level nodes being its children.
struct Section {
    name: &'static str,
    node: &'static Node,
    declared: bool,
    area: Area,
}

#[rustfmt::skip]
const SECTIONS: &[Section] = &[
    Section { name: "repo", node: &repo::REPO, declared: false, area: Area::Repo },
    Section { name: "image", node: &image::IMAGE, declared: true, area: Area::Image },
    Section { name: "bases", node: &bases::BASES, declared: false, area: Area::Bases },
    Section { name: "module", node: &module::MODULE, declared: false, area: Area::Module },
    Section { name: "option", node: &options::OPTION, declared: true, area: Area::Module },
    Section { name: "variant", node: &options::VARIANT, declared: true, area: Area::Module },
    Section { name: "asset", node: &asset::ASSET, declared: true, area: Area::Pins },
    Section { name: "pin", node: &evidence::PIN, declared: true, area: Area::Pins },
    Section { name: "imported", node: &record::IMPORTED, declared: true, area: Area::Pins },
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

/// A link to a section's heading from a page of `from`. The anchor is the
/// section name, which is also the slug of its node's heading.
fn link(section: &Section, from: Area) -> String {
    let file = match section.area == from {
        true => "",
        false => section.area.file(),
    };
    format!("[`{}`]({file}#{})", section.node.name, section.name)
}

fn node(node: &Node, depth: usize, from: Area, seen: &mut Vec<&'static str>, out: &mut String) {
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

    children(node, depth, from, seen, out);
}

/// Every child: a link to another page, an `as above` repeat, a table row for a
/// leaf, a heading for a nested block.
fn children(
    parent: &Node,
    depth: usize,
    from: Area,
    seen: &mut Vec<&'static str>,
    out: &mut String,
) {
    let mut elsewhere: Vec<String> = Vec::new();
    let mut here: Vec<&Node> = Vec::new();
    for child in parent.children {
        match section_of(child) {
            Some(s) => elsewhere.push(link(s, from)),
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
        node(block, depth + 1, from, seen, out);
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

/// The document with every region named in `names` rewritten by `fill`, which
/// is given the region's position in `names`. Fails when a marker names no
/// region, and when a region has no marker, so neither half can be added
/// without the other.
fn splice(doc: &str, names: &[&str], fill: impl Fn(usize, &mut String)) -> Result<String, String> {
    let mut out = String::new();
    let mut open: Option<&str> = None;
    let mut spliced: Vec<&str> = Vec::new();

    for line in doc.lines() {
        if let Some(name) = marker(line, "schema:") {
            if let Some(open) = open {
                return Err(format!("`{open}` is still open at `{name}`"));
            }
            let Some(at) = names.iter().position(|n| *n == name) else {
                return Err(format!(
                    "`{name}` marks no schema in this file; the schemas it documents are {}",
                    names.join(", ")
                ));
            };
            let _ = writeln!(out, "{line}\n");
            fill(at, &mut out);
            open = Some(names[at]);
            spliced.push(names[at]);
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
    match names.iter().find(|n| !spliced.contains(n)) {
        Some(missed) => Err(format!(
            "`{missed}` has no marker; every schema is documented, so add \
             `<!-- schema: {missed} -->` under a heading that says what it is for"
        )),
        None => Ok(out),
    }
}

/// The areas `SECTIONS` tags, in the order the index lists them. The page set
/// comes from here, so a schema tagged with an area always has a page.
pub fn areas() -> Vec<Area> {
    let mut areas: Vec<Area> = Vec::new();
    for s in SECTIONS {
        if !areas.contains(&s.area) {
            areas.push(s.area);
        }
    }
    areas
}

pub fn render(area: Area, doc: &str) -> Result<String, String> {
    let sections: Vec<&Section> = SECTIONS.iter().filter(|s| s.area == area).collect();
    let names: Vec<&str> = sections.iter().map(|s| s.name).collect();
    splice(doc, &names, |at, out| {
        let section = sections[at];
        let seen = &mut Vec::new();
        match section.declared {
            true => node(section.node, 3, area, seen, out),
            false => children(section.node, 2, area, seen, out),
        }
    })
}

/// The index page with its table of every schema rewritten, so a schema added
/// to `SECTIONS` is listed with the page that documents it.
pub fn index(doc: &str) -> Result<String, String> {
    splice(doc, &["index"], |_, out| {
        let _ = writeln!(
            out,
            "| Schema | Documented in | Meaning |\n| --- | --- | --- |"
        );
        for s in SECTIONS {
            let page = format!("schema/{}", s.area.file());
            let target = match s.declared {
                true => format!("{page}#{}", s.name),
                false => page.clone(),
            };
            let _ = writeln!(
                out,
                "| `{}` | [`{page}`]({target}) | {} |",
                s.name, s.node.desc
            );
        }
        out.push('\n');
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_naming_no_schema_fails() {
        let err = render(
            Area::Repo,
            "<!-- schema: colour -->\n<!-- /schema: colour -->\n",
        )
        .unwrap_err();
        assert!(err.starts_with("`colour` marks no schema"), "{err}");
    }

    #[test]
    fn a_marker_naming_another_areas_schema_fails() {
        let err = render(Area::Repo, "<!-- schema: pin -->\n<!-- /schema: pin -->\n").unwrap_err();
        assert!(
            err.starts_with("`pin` marks no schema in this file"),
            "{err}"
        );
    }

    #[test]
    fn a_schema_with_no_marker_fails() {
        let err = render(Area::Repo, "nothing here\n").unwrap_err();
        assert!(err.starts_with("`repo` has no marker"), "{err}");
    }

    /// The page for `area` holds a stale region per schema, as the page would
    /// read before the renderer ran.
    fn stale(area: Area) -> String {
        SECTIONS
            .iter()
            .filter(|s| s.area == area)
            .map(|s| {
                format!(
                    "## {}\n\n<!-- schema: {0} -->\nstale\n<!-- /schema: {0} -->\n",
                    s.name
                )
            })
            .collect()
    }

    #[test]
    fn the_generated_half_is_replaced_and_the_rest_is_kept() {
        let out = render(Area::Image, &stale(Area::Image)).expect("every image schema is marked");
        assert!(!out.contains("stale"));
        assert!(out.contains("### `image`"));
        assert!(out.contains("## image"));
    }

    #[test]
    fn a_link_names_the_page_only_across_pages() {
        let out =
            render(Area::Module, &stale(Area::Module)).expect("every module schema is marked");
        assert!(out.contains("[`option`](#option)"), "{out}");
        assert!(out.contains("[`asset`](pins.md#asset)"), "{out}");
    }
}

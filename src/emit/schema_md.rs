//! The schema reference, rendered whole from the tables the parser already
//! reads, as one page per reader area under `docs/schema/`.

use crate::layout;
use crate::parse::schema::{Arg, Kind, Node, Prop};
use crate::parse::{bases, module, repo};
use crate::provenance::record;
use std::fmt::Write as _;

/// A reader area groups the schemas one reader needs onto one page under
/// `docs/schema/`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Area {
    Repository,
    Repo,
    Image,
    Modules,
    Module,
    Provenance,
    Bases,
}

impl Area {
    pub fn file(self) -> &'static str {
        match self {
            Area::Repository => "repository.md",
            Area::Repo => "repo.md",
            Area::Image => "image.md",
            Area::Modules => "modules.md",
            Area::Module => "module.md",
            Area::Bases => "bases.md",
            Area::Provenance => "provenance.md",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Area::Repository => "The repository",
            Area::Repo => "The repository file",
            Area::Image => "Image files",
            Area::Modules => "Modules",
            Area::Module => "Module manifests",
            Area::Bases => "The base catalog",
            Area::Provenance => "The import record",
        }
    }
}

/// One schema the reference documents. `declared` is false for a file, whose
/// own node is not written and whose top-level nodes are its children. It is
/// true for a block that several files hold.
struct Section {
    name: &'static str,
    node: &'static Node,
    declared: bool,
    area: Area,
    /// Where the file or the block sits, as the section states it.
    at: &'static str,
}

#[rustfmt::skip]
const SECTIONS: &[Section] = &[
    Section { name: "repo.kdl", node: &repo::REPO, declared: false, area: Area::Repo,
        at: "The root of the repository." },
    Section { name: "image.kdl", node: &repo::IMAGE_FILE, declared: false, area: Area::Image,
        at: "The root of the repository, as `image.kdl` or `<name>.image.kdl`." },
    Section { name: "bases.kdl", node: &bases::BASES, declared: false, area: Area::Bases,
        at: "Compiled into `tect`. A `bases.kdl` beside the `tect` binary replaces it, and one at \
            the root of a collection extends it." },
    Section { name: "module.kdl", node: &module::MODULE, declared: false, area: Area::Module,
        at: "`modules/<module-name>/module.kdl`, in the directory of the module." },
    Section { name: "provenance.kdl", node: &record::RECORD_FILE, declared: false, area: Area::Provenance,
        at: "`modules/<module-name>/provenance.kdl`, beside the `module.kdl` of a copied module." },
];

/// A block with a section of its own, which is documented there and linked to
/// from every grammar holding it. Matched on the pair, because a name alone
/// can repeat across grammars.
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

/// What one node accepts, as prose the user reads left to right. `fields` is
/// where the block's link points, which is the table of the fields that the
/// block accepts.
fn accepts(node: &Node, fields: &str) -> Option<String> {
    let value = match node.arg {
        Arg::None => None,
        Arg::Str => Some("*string*".to_string()),
        Arg::Bool => Some("*boolean*".to_string()),
        Arg::Int => Some("*number*".to_string()),
        Arg::Strs => Some("*list of strings*".to_string()),
        Arg::StrPair(first, second) => Some(format!("*{first}*, then *{second}*")),
        Arg::One(set) => Some(closed(set)),
        Arg::MaybeOne(set) => Some(format!("optionally {}", closed(set))),
    };
    let block = match (node.children.is_empty(), block_required(node)) {
        (true, _) => None,
        (false, true) => Some(format!("{{&nbsp;[fields]({fields})&nbsp;}}")),
        (false, false) => Some(format!("optionally {{&nbsp;[fields]({fields})&nbsp;}}")),
    };
    match (value, block) {
        (Some(value), Some(block)) => Some(format!("{value}, then {block}")),
        (value, block) => value.or(block),
    }
}

/// A block that is never empty, or that holds a field the file cannot omit,
/// has to be written.
fn block_required(node: &Node) -> bool {
    !node.empty.text.is_empty() || node.children.iter().any(is_minimal)
}

/// Whether the file must write the field, and whether the field can be
/// written again.
fn presence(node: &Node) -> String {
    let need = match is_minimal(node) && !node.pick {
        true => "required",
        false => "optional",
    };
    match (node.once, node.unique.text.is_empty()) {
        (true, _) => format!("({need})"),
        (false, false) => format!("({need}, unique)"),
        (false, true) => format!("({need}, repeatable)"),
    }
}

fn closed(set: &[&str]) -> String {
    set.iter()
        .map(|v| format!("`{v}`"))
        .collect::<Vec<_>>()
        .join(" or ")
}

fn value(prop: &Prop) -> String {
    match prop.kind {
        Kind::Str => "*string*".to_string(),
        Kind::Bool => "*boolean*".to_string(),
        Kind::Int(low, high) => format!("*number* from {low} to {high}"),
        Kind::One(set) => closed(set),
    }
}

/// Where a link to a section points from a page of `from`.
fn href(section: &Section, from: Area) -> String {
    let file = match section.area == from {
        true => "",
        false => section.area.file(),
    };
    format!("{file}#{}", section.name)
}

fn props(node: &Node, out: &mut String) {
    if node.props.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "| Property | Accepts | Description |\n| --- | --- | --- |"
    );
    for prop in node.props {
        let need = match prop.missing.text.is_empty() {
            true => "optional",
            false => "required",
        };
        let _ = writeln!(
            out,
            "| `{}=` ({need}) | {} | {} |",
            prop.name,
            value(prop),
            prop.desc
        );
    }
    out.push('\n');
}

/// Each list the node carries, then a table of what each of its values does.
fn cases(node: &Node, out: &mut String) {
    lists(node.lists, out);
    if node.values.is_empty() {
        return;
    }
    out.push_str("| Value | Effect |\n| --- | --- |\n");
    for (label, effect) in node.values {
        let _ = writeln!(out, "| {label} | {effect} |");
    }
    out.push('\n');
}

/// Each list as its lead sentence, then one bullet for each item.
fn lists(lists: &[(&str, &[&str])], out: &mut String) {
    for (lead, items) in lists {
        let _ = writeln!(out, "{lead}\n");
        for item in *items {
            let _ = writeln!(out, "- {item}");
        }
        out.push('\n');
    }
}

/// The notes of one item as one GitHub alert, which GitHub draws as a note box
/// and the site hook turns into the theme's callout. The alert stands apart, so
/// a note that follows a list does not join the list.
fn alert(notes: &[&str], out: &mut String) {
    if notes.is_empty() {
        return;
    }
    out.push_str("> [!NOTE]\n");
    // Every line of a note opens on the marker, so a list inside a note stays in the alert.
    for line in notes.join("\n\n").lines() {
        match line.is_empty() {
            true => out.push_str(">\n"),
            false => {
                let _ = writeln!(out, "> {line}");
            }
        }
    }
    out.push('\n');
}

/// A closed set as the skeleton writes it, each value a KDL string.
fn either(set: &[&str]) -> String {
    set.iter()
        .map(|v| format!("\"{v}\""))
        .collect::<Vec<_>>()
        .join("|")
}

/// One skeleton line up to its children: the name, a placeholder for the
/// argument and one for each property.
fn head(node: &Node) -> String {
    let mut line = match node.name.is_empty() {
        true => "<name>".to_string(),
        false => node.name.to_string(),
    };
    let arg = match node.arg {
        Arg::None => String::new(),
        Arg::Str => " \"…\"".into(),
        Arg::Bool => " #true|#false".into(),
        Arg::Int => " <number>".into(),
        Arg::Strs => " \"…\" …".into(),
        Arg::StrPair(..) => " \"…\" \"…\"".into(),
        Arg::One(set) | Arg::MaybeOne(set) => format!(" {}", either(set)),
    };
    line.push_str(&arg);
    for prop in node.props {
        let value = match prop.kind {
            Kind::Str => "\"…\"".to_string(),
            Kind::Bool => "#true|#false".to_string(),
            Kind::Int(..) => "<number>".to_string(),
            Kind::One(set) => either(set),
        };
        let _ = write!(line, " {}={value}", prop.name);
    }
    line
}

/// The section intro of `node`, which falls back to its one-line description.
fn intro(node: &Node) -> &'static str {
    match node.about.is_empty() {
        true => node.desc,
        false => node.about,
    }
}

/// Whether the minimal example shows the node.
fn is_minimal(node: &Node) -> bool {
    node.minimal || !node.missing.text.is_empty()
}

/// One example line up to the node's children. If the grammar gives no
/// example, then the line carries placeholders.
fn example(node: &Node) -> String {
    match (node.example.is_empty(), node.name.is_empty()) {
        (true, _) => head(node),
        (false, true) => node.example.to_string(),
        (false, false) => format!("{} {}", node.name, node.example),
    }
}

/// Which nodes an example writes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Show {
    /// The nodes a file cannot omit.
    Minimal,
    /// What `tect create` writes.
    Scaffold,
    /// Every node in the grammar, which the example test walks.
    #[cfg(test)]
    All,
}

impl Show {
    /// Whether the example writes `node` inside `parent`.
    fn keeps(self, parent: &Node, node: &Node) -> bool {
        match self {
            Show::Minimal => is_minimal(node),
            // A block that names what the scaffold writes into it shows exactly
            // that, so a choice of one among its fields is never shown twice.
            Show::Scaffold => match parent.scaffolds.is_empty() {
                true => is_minimal(node),
                false => parent.scaffolds.contains(&node.name) || !node.missing.text.is_empty(),
            },
            #[cfg(test)]
            Show::All => true,
        }
    }
}

/// The node as example KDL at `depth`, with the children that `show` keeps. A
/// block that is never empty keeps its first child. A child with a section of
/// its own is written minimal, with a pointer to that section.
fn kdl(
    node: &Node,
    depth: usize,
    show: Show,
    from: Area,
    pointer: Option<String>,
    out: &mut String,
) {
    let pad = "    ".repeat(depth);
    let comment = pointer.map(|p| format!("  // {p}")).unwrap_or_default();
    let mut shown: Vec<&Node> = node
        .children
        .iter()
        .filter(|c| show.keeps(node, c))
        .collect();
    if shown.is_empty() && !node.empty.text.is_empty() {
        shown.extend(node.children.first());
    }
    if shown.is_empty() {
        let _ = writeln!(out, "{pad}{}{comment}", example(node));
        return;
    }
    let _ = writeln!(out, "{pad}{} {{{comment}", example(node));
    for child in shown {
        match section_of(child) {
            None => kdl(child, depth + 1, show, from, None, out),
            Some(s) => {
                let page = match s.area == from {
                    true => String::new(),
                    false => format!(" in {}", s.area.file()),
                };
                let pointer = format!("see {}{page}", s.name);
                kdl(child, depth + 1, Show::Minimal, from, Some(pointer), out);
            }
        }
    }
    let _ = writeln!(out, "{pad}}}");
}

/// The schema's example as the file writes it, with the nodes that `show`
/// keeps. If the file does not write the root node, then the root's children
/// sit at the top level.
fn file(section: &Section, show: Show) -> String {
    let mut out = String::new();
    match section.declared {
        true => kdl(section.node, 0, show, section.area, None, &mut out),
        false => {
            for child in section
                .node
                .children
                .iter()
                .filter(|c| show.keeps(section.node, c))
            {
                kdl(child, 0, show, section.area, None, &mut out);
            }
        }
    }
    out
}

/// The paths of every node `tect create` writes into the file of `area`,
/// with `*` for a name the author chooses. A path ending in `**` stands for a
/// block that another section documents, and for every node inside it.
pub fn scaffold(area: Area) -> Vec<String> {
    let mut out = Vec::new();
    for s in SECTIONS.iter().filter(|s| s.area == area && !s.declared) {
        scaffolded(s.node, "", &mut out);
    }
    out
}

fn scaffolded(parent: &Node, prefix: &str, out: &mut Vec<String>) {
    for child in parent
        .children
        .iter()
        .filter(|c| parent.scaffolds.contains(&c.name))
    {
        let name = match child.name.is_empty() {
            true => "*",
            false => child.name,
        };
        let path = match prefix.is_empty() {
            true => name.to_string(),
            false => format!("{prefix}/{name}"),
        };
        match section_of(child) {
            Some(_) => out.push(format!("{path}/**")),
            None => {
                scaffolded(child, &path, out);
                out.push(path);
            }
        }
    }
}

/// The node's minimal example inside the blocks on `path` that hold it.
fn snippet(path: &[&Node], node: &Node, from: Area, out: &mut String) {
    out.push_str("```kdl\n");
    for (depth, outer) in path.iter().enumerate() {
        let _ = writeln!(out, "{}{} {{", "    ".repeat(depth), example(outer));
    }
    kdl(node, path.len(), Show::Minimal, from, None, out);
    for depth in (0..path.len()).rev() {
        let _ = writeln!(out, "{}}}", "    ".repeat(depth));
    }
    out.push_str("```\n\n");
}

/// The anchor of a node's section, which is the names on its path joined by
/// dashes. A page can hold two nodes of one name, so the name alone is not
/// unique.
fn anchor(path: &[&Node], node: &Node) -> String {
    path.iter()
        .copied()
        .chain(std::iter::once(node))
        .map(|n| match n.name.is_empty() {
            true => "name",
            false => n.name,
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// The children of `node` as one table, each linked to the section that
/// documents it. Returns the children that get a section below `node`. A
/// child that another section documents, or that an earlier section already
/// documents, links there instead. If `id` names the section of `node`, then
/// the table carries the anchor that the `fields` link of `node` points at.
fn table(
    node: &'static Node,
    path: &[&'static Node],
    from: Area,
    id: Option<&str>,
    seen: &mut Vec<(&'static str, String)>,
    out: &mut String,
) -> Vec<&'static Node> {
    let mut here: Vec<&'static Node> = Vec::new();
    if node.children.is_empty() {
        return here;
    }
    if let Some(id) = id {
        let _ = writeln!(out, "<a id=\"{id}-fields\"></a>\n");
    }
    out.push_str("| Field | Accepts | Description |\n| --- | --- | --- |\n");
    for child in node.children {
        let link = match (
            section_of(child),
            seen.iter().find(|(desc, _)| *desc == child.desc),
        ) {
            (Some(s), _) => href(s, from),
            (None, Some((_, at))) => format!("#{at}"),
            (None, None) => {
                let at = anchor(path, child);
                seen.push((child.desc, at.clone()));
                here.push(child);
                format!("#{at}")
            }
        };
        let _ = writeln!(
            out,
            "| [{}]({link}) {} | {} | {} |",
            named(child),
            // The qualifier wraps as a whole, onto the line below the name.
            presence(child).replace(' ', "&nbsp;"),
            accepts(child, &format!("{link}-fields")).unwrap_or_default(),
            child.desc
        );
    }
    out.push('\n');
    here
}

/// The node's section at heading level `depth`, then a section one level
/// down for each node inside it.
fn node_section(
    node: &'static Node,
    path: &mut Vec<&'static Node>,
    depth: usize,
    from: Area,
    seen: &mut Vec<(&'static str, String)>,
    out: &mut String,
) {
    let id = anchor(path, node);
    let _ = writeln!(
        out,
        "<a id=\"{id}\"></a>\n\n{} {} {}\n\n{}\n",
        "#".repeat(depth.min(6)),
        named(node),
        presence(node),
        intro(node)
    );
    snippet(path, node, from, out);
    if let Some(what) = accepts(node, &format!("#{id}-fields")) {
        let _ = writeln!(out, "Accepts: {what}\n");
    }
    cases(node, out);
    props(node, out);
    path.push(node);
    let here = table(node, path, from, Some(&id), seen, out);
    alert(node.notes, out);
    for child in here {
        node_section(child, path, depth + 1, from, seen, out);
    }
    path.pop();
}

/// One schema at heading level `depth`: its minimal example, a table of its
/// top-level nodes and its notes, then a section for each node.
fn section(section: &Section, depth: usize, out: &mut String) {
    let root = section.node;
    let _ = writeln!(
        out,
        "<a id=\"{}\"></a>\n\n{} `{}`\n\nLocation: {}\n\n{}\n",
        section.name,
        "#".repeat(depth),
        section.name,
        section.at,
        intro(root)
    );
    let _ = writeln!(out, "```kdl\n{}```\n", file(section, Show::Scaffold));
    let mut path: Vec<&'static Node> = Vec::new();
    if section.declared {
        if let Some(what) = accepts(root, &format!("#{}-fields", section.name)) {
            let _ = writeln!(out, "Accepts: {what}\n");
        }
        cases(root, out);
        props(root, out);
        path.push(root);
    }
    let id = section.declared.then_some(section.name);
    let seen = &mut Vec::new();
    let here = table(root, &path, section.area, id, seen, out);
    // A declared root states its cases above its table. A file root has no
    // Accepts line, so its cases follow the table of its fields.
    if !section.declared {
        cases(root, out);
    }
    alert(root.notes, out);
    for child in here {
        node_section(child, &mut path, depth + 1, section.area, seen, out);
    }
}

/// Every page under `docs/schema/`.
const AREAS: [Area; 7] = [
    Area::Repository,
    Area::Repo,
    Area::Image,
    Area::Modules,
    Area::Module,
    Area::Provenance,
    Area::Bases,
];

pub fn areas() -> Vec<Area> {
    AREAS.to_vec()
}

/// One directory of a drawn tree.
#[derive(Default)]
struct Dir {
    children: std::collections::BTreeMap<String, Dir>,
    dir: bool,
    /// Whether git ignores this path itself. A directory that only holds an
    /// ignored path stays tracked.
    ignored: bool,
}

/// The paths of `places` as a tree hung off `root`, with the glyphs and the
/// order of the tree that `tect` prints after a command writes files, so the
/// reference and the terminal look alike. A path that git does not track says
/// so.
fn drawn(root: &str, places: &[layout::Place]) -> String {
    let mut top = Dir::default();
    for place in places {
        let path = place.path.concat();
        let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        let mut node = &mut top;
        for (at, part) in parts.iter().enumerate() {
            node = node.children.entry(part.to_string()).or_default();
            node.dir |= at + 1 < parts.len() || path.ends_with('/');
            if at + 1 == parts.len() {
                node.ignored = !place.tracked;
            }
        }
    }
    // The branches hang under the last directory of `root`, so the tree starts
    // below its first character.
    let indent = root.trim_end_matches('/').rfind('/').map_or(0, |at| at + 1);
    let mut out = format!("```text\n{root}\n");
    branch(&top, &" ".repeat(indent), &mut out);
    out.push_str("```\n\n");
    out
}

fn branch(node: &Dir, prefix: &str, out: &mut String) {
    let mut names: Vec<&String> = node.children.keys().collect();
    names.sort_by_key(|name| !node.children[*name].dir);
    for (index, name) in names.iter().enumerate() {
        let child = &node.children[*name];
        let (fork, carry) = match index + 1 == names.len() {
            true => ("└── ", "    "),
            false => ("├── ", "│   "),
        };
        let slash = match child.dir {
            true => "/",
            false => "",
        };
        let untracked = match child.ignored {
            true => "  (not in git)",
            false => "",
        };
        let _ = writeln!(out, "{prefix}{fork}{name}{slash}{untracked}");
        branch(child, &format!("{prefix}{carry}"), out);
    }
}

/// A page that says where things sit: its intro, the paths drawn as a tree, a
/// table of the paths, then its lists and notes.
fn tree_page(title: &str, tree: &layout::Tree) -> String {
    let mut out = format!("# {title}\n\n{}\n\n", tree.intro);
    out.push_str(&drawn(tree.root, tree.places));
    let writer = tree.places.iter().any(|place| !place.writer.is_empty());
    match writer {
        true => out
            .push_str("| Path | Written by | In git | Description |\n| --- | --- | --- | --- |\n"),
        false => out.push_str("| Path | Description |\n| --- | --- |\n"),
    }
    for place in tree.places {
        let path = place.path.concat();
        let _ = match writer {
            true => writeln!(
                out,
                "| `{path}` | {} | {} | {} |",
                place.writer,
                match place.tracked {
                    true => "yes",
                    false => "no",
                },
                place.what
            ),
            false => writeln!(out, "| `{path}` | {} |", place.what),
        };
    }
    out.push('\n');
    lists(tree.lists, &mut out);
    alert(tree.notes, &mut out);
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    out.push('\n');
    out
}

/// The whole page for `area`. A page with one schema opens on it, and a page
/// with more holds each one under the page title.
pub fn page(area: Area) -> String {
    match area {
        Area::Repository => return tree_page(area.title(), &layout::REPOSITORY),
        Area::Modules => return tree_page(area.title(), &layout::MODULE_DIR),
        Area::Repo | Area::Image | Area::Module | Area::Provenance | Area::Bases => {}
    }
    let schemas: Vec<&Section> = SECTIONS.iter().filter(|s| s.area == area).collect();
    let mut out = String::new();
    let depth = match schemas.len() {
        1 => 1,
        _ => {
            let _ = writeln!(out, "# {}\n", area.title());
            2
        }
    };
    for s in schemas {
        section(s, depth, &mut out);
    }
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests;

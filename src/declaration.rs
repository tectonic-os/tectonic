//! The text edits that insert or remove one declaration in an image or repo
//! file.
//!
//! Each edit keeps every other byte in place, so the user's own formatting and
//! comments survive.

use crate::layout;
use std::path::Path;

/// The flavour block an entry is listed in. An ungated entry sits in the
/// image block itself.
fn listed_in(flavour: Option<&str>) -> Vec<(&str, Option<&str>)> {
    flavour
        .map(|flavour| ("flavour", Some(flavour)))
        .into_iter()
        .collect()
}

/// Places a component path: the blocks that hold its last segment, and the
/// line that lists it. `block` holds the option values the entry sets, or is
/// empty.
pub(crate) fn placed<'a>(
    flavour: Option<&'a str>,
    name: &'a str,
    block: &str,
) -> (Vec<(&'a str, Option<&'a str>)>, String) {
    let mut chain = listed_in(flavour);
    let (parents, last) = name.rsplit_once('/').unwrap_or(("", name));
    chain.extend(
        parents
            .split('/')
            .filter(|part| !part.is_empty())
            .map(|part| (part, None)),
    );
    (chain, format!("{last}{block}"))
}

pub(crate) fn wrap(blocks: &[(&str, Option<&str>)], leaf: &str) -> String {
    blocks
        .iter()
        .rev()
        .fold(leaf.to_string(), |inner, (node, arg)| {
            let arg = arg.map_or(String::new(), |arg| format!(" \"{arg}\""));
            let inner: String = inner.lines().map(|line| format!("    {line}\n")).collect();
            format!("{node}{arg} {{\n{inner}}}")
        })
}

/// Append inserts one declaration at the deepest existing block and wraps the
/// missing blocks. Every other byte stays in place.
pub(crate) fn append(
    file: &Path,
    image: &str,
    chain: &[(&str, Option<&str>)],
    leaf: &str,
) -> Result<(), String> {
    let mut text =
        std::fs::read_to_string(file).map_err(|err| format!("{}: {err}", file.display()))?;
    let (kept, close) = (0..=chain.len())
        .rev()
        .find_map(|kept| {
            crate::parse::image::block_close(&text, image, &chain[..kept]).map(|at| (kept, at))
        })
        .ok_or_else(|| format!("{} declares no image `{image}`", file.display()))?;
    let declaration = wrap(&chain[kept..], leaf);

    let start = text[..close].rfind('\n').map_or(0, |at| at + 1);
    let indent = &text[start..close];
    let (at, line) = match indent.trim().is_empty() {
        true => (
            start,
            declaration
                .lines()
                .map(|line| format!("{indent}    {line}\n"))
                .collect(),
        ),
        // A node that shares the block's line ends at a `;`, or the new node
        // would read as its argument.
        false => {
            let end = text[..close].trim_end().len();
            let gap = match text[..end].ends_with(['{', ';']) {
                true => " ",
                false => "; ",
            };
            (end, format!("{gap}{}", declaration.replace('\n', " ")))
        }
    };
    text.insert_str(at, &line);
    std::fs::write(file, text).map_err(|err| format!("{}: {err}", file.display()))
}

/// Removes the segment that lists `name` in one image of `file`, whose text
/// is `text`, and keeps every other byte in place. The calling command writes
/// the result, so a refusal leaves the file as it was.
pub(crate) fn unlisting(
    file: &Path,
    text: &str,
    image: &str,
    flavour: Option<&str>,
    name: &str,
) -> Result<String, String> {
    let root = file.parent().unwrap_or(Path::new("."));
    let mut chain = listed_in(flavour);
    chain.extend(crate::parse::image::segments(name));
    crate::parse::image::unlisted(text, image, &chain, |path| {
        layout::is_component(&layout::component(root, path))
    })
    .map_err(|err| format!("{} {err}", file.display()))
}

/// A top-level block put back where it was, else at the end of the file.
/// Taking one away takes the blank line above it with it.
pub(crate) fn splice(text: &str, was: Option<crate::diag::Span>, block: &str) -> String {
    let mut out = text.to_string();
    match was {
        Some(was) => {
            let end = was.offset + was.len;
            let start = match block.is_empty() {
                true => text[..was.offset].trim_end_matches(['\n', ' ']).len(),
                false => was.offset,
            };
            out.replace_range(start..end, block);
        }
        None if block.is_empty() => {}
        None => {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&format!("\n{block}\n"));
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The whitespace before `at`, which is the line's indent where `at` stands on
/// that line's own node and empty where the line holds code before it.
pub(crate) fn line_indent(text: &str, at: usize) -> &str {
    let start = text[..at].rfind('\n').map_or(0, |found| found + 1);
    let line = &text[start..at];
    &line[..line.len() - line.trim_start().len()]
}

//! What a command wrote, drawn where it wrote it.

use ratatui::crossterm::terminal;
use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// What a terminal that will not say how wide it is is taken to be.
const NARROWEST: usize = 80;

/// The gap between the longest branch and the descriptions beside it.
const GAP: usize = 2;

/// A description cut shorter than this is dropped instead: less of a phrase
/// than this says nothing.
const LEAST: usize = 12;

#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    dir: bool,
}

/// `wrote` is every path a command wrote, relative to `root`, and `describe`
/// names what one is for — empty for a file that speaks for itself.
pub fn print(root: &Path, wrote: &[PathBuf], describe: fn(&Path) -> &'static str) {
    let mut lines = Vec::new();
    walk(&of(wrote), Path::new(""), "", describe, &mut lines);

    let column = lines
        .iter()
        .filter(|(_, desc)| !desc.is_empty())
        .map(|(branch, _)| branch.chars().count() + GAP)
        .max()
        .unwrap_or(0);
    let room = width().saturating_sub(column);

    println!("\n{}/", root.display());
    for (branch, desc) in &lines {
        match fit(desc, room) {
            "" => println!("{branch}"),
            desc => println!("{branch:column$}{desc}"),
        }
    }
}

fn of(wrote: &[PathBuf]) -> Node {
    let mut tree = Node::default();
    for path in wrote {
        let depth = path.components().count();
        let mut node = &mut tree;
        for (at, part) in path.components().enumerate() {
            node = node
                .children
                .entry(part.as_os_str().to_string_lossy().into_owned())
                .or_default();
            node.dir |= at + 1 < depth;
        }
    }
    tree
}

fn walk(
    node: &Node,
    at: &Path,
    prefix: &str,
    describe: fn(&Path) -> &'static str,
    out: &mut Vec<(String, &'static str)>,
) {
    let mut names: Vec<&String> = node.children.keys().collect();
    names.sort_by_key(|name| !node.children[*name].dir);
    for (index, name) in names.iter().enumerate() {
        let child = &node.children[*name];
        let (branch, carry) = match index + 1 == names.len() {
            true => ("└── ", "    "),
            false => ("├── ", "│   "),
        };
        let path = at.join(name);
        let slash = match child.dir {
            true => "/",
            false => "",
        };
        out.push((format!("{prefix}{branch}{name}{slash}"), describe(&path)));
        walk(child, &path, &format!("{prefix}{carry}"), describe, out);
    }
}

/// `desc` cut to `room` at a word boundary, and empty where too little of it
/// survives to be worth reading. Never folded: a wrapped tree stops lining up,
/// and the shape is the point.
fn fit(desc: &str, room: usize) -> &str {
    if desc.chars().count() <= room {
        return desc;
    }
    let cut = desc
        .char_indices()
        .take(room + 1)
        .filter(|(_, c)| *c == ' ')
        .last()
        .map_or(0, |(at, _)| at);
    match cut >= LEAST {
        true => &desc[..cut],
        false => "",
    }
}

/// Asked of the terminal only where the output is one, so a redirected run and
/// a piped one draw the same tree whatever is behind them.
fn width() -> usize {
    match std::io::stdout().is_terminal() {
        true => terminal::size().map_or(NARROWEST, |(cols, _)| cols as usize),
        false => NARROWEST,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrase(path: &Path) -> &'static str {
        match path.to_string_lossy().as_ref() {
            "modules" => "every module the repo holds",
            "repo.kdl" => "what the repo pins",
            _ => "",
        }
    }

    fn drawn(wrote: &[&str]) -> Vec<(String, &'static str)> {
        let wrote: Vec<PathBuf> = wrote.iter().map(PathBuf::from).collect();
        let mut lines = Vec::new();
        walk(&of(&wrote), Path::new(""), "", phrase, &mut lines);
        lines
    }

    #[test]
    fn directories_come_first_and_carry_the_branch_their_children_hang_off() {
        let lines = drawn(&["repo.kdl", "modules/mine/module.kdl", "README.md"]);
        let branches: Vec<&str> = lines.iter().map(|(branch, _)| branch.as_str()).collect();
        assert_eq!(
            branches,
            [
                "├── modules/",
                "│   └── mine/",
                "│       └── module.kdl",
                "├── README.md",
                "└── repo.kdl",
            ]
        );
        assert_eq!(lines[0].1, "every module the repo holds");
        assert_eq!(lines[3].1, "");
    }

    #[test]
    fn a_description_is_cut_at_a_word_and_dropped_before_it_says_nothing() {
        assert_eq!(fit("what the repo pins", 18), "what the repo pins");
        assert_eq!(fit("what the repo pins", 17), "what the repo");
        assert_eq!(fit("what the repo pins", 13), "what the repo");
        assert_eq!(fit("what the repo pins", 12), "");
        assert_eq!(fit("what the repo pins", 0), "");
    }
}

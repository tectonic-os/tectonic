//! The schema as data, and the one walker that reads a document against it.
//! Shape only: what a node holds, how much of it, and what to say when the
//! document does not match. Meaning stays in `resolve`.

use crate::diag::{Issue, Issues, Source, Span};
use crate::parse::{bool_arg, int_arg, kids, string_arg, string_args};
use kdl::{KdlDocument, KdlNode};

/// One thing the shape has to say. `{}` stands for the name or value it is
/// about, and an empty `text` says nothing at all.
pub struct Say {
    pub text: &'static str,
    pub label: &'static str,
    pub help: &'static str,
}

impl Say {
    /// Nothing to say: the shape allows it, or allows it silently.
    pub const NONE: Say = Say::new("", "", "");

    pub const fn new(text: &'static str, label: &'static str, help: &'static str) -> Say {
        Say { text, label, help }
    }

    const fn silent(&self) -> bool {
        self.text.is_empty()
    }

    fn raise(&self, about: &str, span: impl Into<Span>, src: &Source, issues: &mut Issues) {
        if self.silent() {
            return;
        }
        let issue = Issue::new(self.text.replace("{}", about), src).at(span, self.label);
        issues.push(match self.help.is_empty() {
            true => issue,
            false => issue.help(self.help.replace("{}", about)),
        });
    }
}

/// A node whose whole content is one string, said the same way everywhere.
pub const NEEDS_VALUE: Say = Say::new("`{}` needs a value", "nothing given", "");

/// The positional argument a node carries.
pub enum Arg {
    None,
    Str,
    Bool,
    Int,
    /// Every positional string, as the list it looks like.
    Strs,
    /// Exactly two positional strings, named by their roles in order.
    StrPair(&'static str, &'static str),
    /// One of a closed set of strings.
    One(&'static [&'static str]),
    /// One of a closed set of strings, or no argument. A node takes it where
    /// its children can set the same value one part at a time.
    MaybeOne(&'static [&'static str]),
}

pub enum Kind {
    Str,
    Bool,
    /// An integer, and the range it has to fall in.
    Int(i128, i128),
    /// One of a closed set of strings.
    One(&'static [&'static str]),
}

/// A named entry on a node.
pub struct Prop {
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: Kind,
    /// When the value is not of `kind`.
    pub say: Say,
    /// When the node carries no such entry at all.
    pub missing: Say,
}

/// One node in the grammar. A node named `""` inside `children` matches any
/// name, which is how a block whose children the author names is declared.
pub struct Node {
    pub name: &'static str,
    pub desc: &'static str,
    /// The section intro of the schema reference, which says what the node is
    /// for and why the user writes it. A table row keeps the one-line `desc`.
    pub about: &'static str,
    pub arg: Arg,
    /// A missing argument, or one given to a node that takes none.
    pub arg_say: Say,
    /// When the node is absent from its parent.
    pub missing: Say,
    /// Whether a second one is a problem, and what to help with.
    pub once: bool,
    pub dup_help: &'static str,
    /// When a second one carries the same argument.
    pub unique: Say,
    /// When the node has no children.
    pub empty: Say,
    pub props: &'static [Prop],
    /// A property not in `props`.
    pub prop_say: Say,
    pub children: &'static [Node],
    /// A child not in `children`.
    pub child_say: Say,
    /// Behaviour the schema reference states after its tables, one sentence
    /// or two per note.
    pub notes: &'static [&'static str],
    /// What each value of the node does, as a Markdown label and its effect.
    /// The schema reference tabulates them under the value the node accepts.
    pub values: &'static [(&'static str, &'static str)],
    /// A lead sentence and the items it introduces, which the schema reference
    /// renders as a list. A list holds cases that a note would chain in one
    /// sentence.
    pub lists: &'static [(&'static str, &'static [&'static str])],
    /// What the schema reference writes after the node name in an example.
    /// An author-named node writes its name here too.
    pub example: &'static str,
    /// Whether the minimal example shows the node although the walker does
    /// not require it. A check outside the walker requires it, or a choice of
    /// one among its siblings does.
    pub minimal: bool,
    /// The children that `tect create` writes into this block, by name. A
    /// shared node sits in several blocks, so the block names it, and the node
    /// does not flag itself.
    pub scaffolds: &'static [&'static str],
}

impl Node {
    pub const fn new(name: &'static str, desc: &'static str) -> Node {
        Node {
            name,
            desc,
            about: "",
            arg: Arg::None,
            arg_say: Say::NONE,
            missing: Say::NONE,
            once: false,
            dup_help: "",
            unique: Say::NONE,
            empty: Say::NONE,
            props: &[],
            prop_say: Say::NONE,
            children: &[],
            child_say: Say::NONE,
            notes: &[],
            values: &[],
            lists: &[],
            example: "",
            minimal: false,
            scaffolds: &[],
        }
    }

    pub const fn scaffolds(mut self, names: &'static [&'static str]) -> Node {
        self.scaffolds = names;
        self
    }

    pub const fn about(mut self, about: &'static str) -> Node {
        self.about = about;
        self
    }

    pub const fn example(mut self, example: &'static str) -> Node {
        self.example = example;
        self
    }

    pub const fn minimal(mut self) -> Node {
        self.minimal = true;
        self
    }

    pub const fn notes(mut self, notes: &'static [&'static str]) -> Node {
        self.notes = notes;
        self
    }

    pub const fn values(mut self, values: &'static [(&'static str, &'static str)]) -> Node {
        self.values = values;
        self
    }

    pub const fn lists(
        mut self,
        lists: &'static [(&'static str, &'static [&'static str])],
    ) -> Node {
        self.lists = lists;
        self
    }

    pub const fn arg(mut self, arg: Arg, say: Say) -> Node {
        self.arg = arg;
        self.arg_say = say;
        self
    }

    pub const fn once(mut self, help: &'static str) -> Node {
        self.once = true;
        self.dup_help = help;
        self
    }

    pub const fn unique(mut self, say: Say) -> Node {
        self.unique = say;
        self
    }

    pub const fn missing(mut self, say: Say) -> Node {
        self.missing = say;
        self
    }

    pub const fn empty(mut self, say: Say) -> Node {
        self.empty = say;
        self
    }

    pub const fn props(mut self, props: &'static [Prop], say: Say) -> Node {
        self.props = props;
        self.prop_say = say;
        self
    }

    pub const fn children(mut self, children: &'static [Node], say: Say) -> Node {
        self.children = children;
        self.child_say = say;
        self
    }
}

/// A document whose top-level nodes are the schema's children, which is what a
/// file with no one node wrapping it looks like.
/// The walker's findings on `text`. If `declared` is true, then the text
/// writes `schema` itself, and otherwise it writes the children of `schema`.
/// A text that is not KDL fails with the parser's message.
#[cfg(test)]
pub fn check_text(text: &str, schema: &Node, declared: bool) -> Result<Issues, String> {
    let doc: KdlDocument = text.parse().map_err(|err| format!("{err}"))?;
    let src = Source::new(schema.name, text);
    let mut issues = Issues::default();
    match (declared, doc.nodes().first()) {
        (true, Some(node)) => check(node, schema, &src, &mut issues),
        (true, None) => return Err(format!("`{}` writes no node", schema.name)),
        (false, _) => check_doc(&doc, schema, &src, &mut issues),
    }
    Ok(issues)
}

pub fn check_doc(doc: &KdlDocument, schema: &Node, src: &Source, issues: &mut Issues) {
    walk(doc.nodes(), schema, Span::default(), src, issues);
}

/// One node against its schema, and everything under it.
pub fn check(node: &KdlNode, schema: &Node, src: &Source, issues: &mut Issues) {
    let here: Span = node.name().span().into();
    // An author-named node is `{}` as the document writes it, having no name here.
    let about = match schema.name.is_empty() {
        true => node.name().value(),
        false => schema.name,
    };
    match schema.arg {
        Arg::None => {
            if let Some(stray) = string_arg(node) {
                schema.arg_say.raise(stray, here, src, issues);
            }
        }
        Arg::Str => {
            if string_arg(node).is_none_or(str::is_empty) {
                schema.arg_say.raise(about, here, src, issues);
            }
        }
        Arg::Bool => {
            if bool_arg(node).is_none() {
                schema.arg_say.raise(about, here, src, issues);
            }
        }
        Arg::Int => {
            if int_arg(node).is_none() {
                schema.arg_say.raise(about, here, src, issues);
            }
        }
        Arg::Strs => {
            if string_args(node).is_empty() {
                schema.arg_say.raise(about, here, src, issues);
            }
        }
        Arg::StrPair(..) => {
            let args: Vec<_> = node
                .entries()
                .iter()
                .filter(|entry| entry.name().is_none())
                .collect();
            if args.len() != 2 || args.iter().any(|entry| entry.value().as_string().is_none()) {
                schema.arg_say.raise(about, here, src, issues);
            }
        }
        Arg::One(set) => {
            let given = string_arg(node);
            if !given.is_some_and(|value| set.contains(&value)) {
                schema
                    .arg_say
                    .raise(given.unwrap_or(about), here, src, issues);
            }
        }
        Arg::MaybeOne(set) => {
            let positional = node.entries().iter().find(|entry| entry.name().is_none());
            if let Some(entry) = positional {
                let given = entry.value().to_string();
                let given = entry.value().as_string().unwrap_or(&given);
                if !set.contains(&given) {
                    schema.arg_say.raise(given, here, src, issues);
                }
            }
        }
    }

    for entry in node.entries() {
        let Some(key) = entry.name().map(|n| n.value()) else {
            continue; // the argument, checked above
        };
        match schema.props.iter().find(|p| p.name == key) {
            Some(prop) => {
                let ok = match prop.kind {
                    Kind::Str => entry.value().as_string().is_some(),
                    Kind::Bool => entry.value().as_bool().is_some(),
                    Kind::Int(low, high) => entry
                        .value()
                        .as_integer()
                        .is_some_and(|v| (low..=high).contains(&v)),
                    Kind::One(set) => entry.value().as_string().is_some_and(|v| set.contains(&v)),
                };
                if !ok {
                    // A closed set is about the value, which is the thing not in it.
                    let about = match prop.kind {
                        Kind::One(_) => entry.value().as_string().unwrap_or(key),
                        _ => key,
                    };
                    prop.say.raise(about, entry.span(), src, issues);
                }
            }
            None => schema.prop_say.raise(key, entry.span(), src, issues),
        }
    }

    for prop in schema.props {
        if prop.missing.silent()
            || node
                .entries()
                .iter()
                .any(|e| e.name().map(|n| n.value()) == Some(prop.name))
        {
            continue;
        }
        prop.missing.raise(node.name().value(), here, src, issues);
    }

    walk(kids(node), schema, here, src, issues);
}

/// The children of one node against the schema's children, `here` being what a
/// diagnostic about an absent one points at.
fn walk(children: &[KdlNode], schema: &Node, here: Span, src: &Source, issues: &mut Issues) {
    if children.is_empty() {
        schema.empty.raise(schema.name, here, src, issues);
    }

    let mut seen: Vec<(&str, &str, Span)> = Vec::new();
    for child in children {
        let name = child.name().value();
        let span: Span = child.name().span().into();
        let Some(sub) = schema
            .children
            .iter()
            .find(|c| c.name == name)
            .or_else(|| schema.children.iter().find(|c| c.name.is_empty()))
        else {
            schema.child_say.raise(name, span, src, issues);
            continue;
        };
        let key = match sub.once {
            true => Some(name),
            false => match sub.unique.silent() {
                true => None,
                // An author-named child is keyed by its own name, since its
                // arguments are the payload.
                false if sub.name.is_empty() => (!string_args(child).is_empty()).then_some(name),
                false => string_arg(child),
            },
        };
        if let Some(key) = key {
            if let Some((_, _, first)) = seen.iter().find(|(n, k, _)| *n == sub.name && *k == key) {
                match sub.once {
                    true => {
                        let issue = Issue::new(format!("`{name}` is declared twice"), src)
                            .at(*first, "first here")
                            .at(span, "and again here");
                        issues.push(match sub.dup_help.is_empty() {
                            true => issue,
                            false => issue.help(sub.dup_help),
                        });
                    }
                    false => sub.unique.raise(key, span, src, issues),
                }
                continue;
            }
            seen.push((sub.name, key, span));
        }
        check(child, sub, src, issues);
    }

    for sub in schema.children {
        if sub.missing.silent() || children.iter().any(|c| c.name().value() == sub.name) {
            continue;
        }
        sub.missing.raise(sub.name, here, src, issues);
    }
}

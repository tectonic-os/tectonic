//! The schema as data, and the one walker that reads a document against it.
//! Shape only: what a node holds, how much of it, and what to say when the
//! document does not match. Meaning stays in `resolve`.

use crate::diag::{Issue, Issues, Source, Span};
use crate::parse::{bool_arg, kids, string_arg};
use kdl::KdlNode;

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

/// The positional argument a node carries.
pub enum Arg {
    None,
    Str,
    Bool,
    /// Every positional string, as the list it looks like.
    Strs,
}

pub enum Kind {
    Str,
    Bool,
}

/// A named entry on a node.
pub struct Prop {
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: Kind,
    /// When the value is not of `kind`.
    pub say: Say,
}

/// One node in the grammar. A node named `""` inside `children` matches any
/// name, which is how a block whose children the author names is declared.
pub struct Node {
    pub name: &'static str,
    pub desc: &'static str,
    pub arg: Arg,
    /// A missing argument, or one given to a node that takes none.
    pub arg_say: Say,
    /// When the node is absent from its parent.
    pub missing: Say,
    /// Whether a second one is a problem, and what to help with.
    pub once: bool,
    pub dup_help: &'static str,
    /// When the node has no children.
    pub empty: Say,
    pub props: &'static [Prop],
    /// A property not in `props`.
    pub prop_say: Say,
    pub children: &'static [Node],
    /// A child not in `children`.
    pub child_say: Say,
}

impl Node {
    pub const fn new(name: &'static str, desc: &'static str) -> Node {
        Node {
            name,
            desc,
            arg: Arg::None,
            arg_say: Say::NONE,
            missing: Say::NONE,
            once: false,
            dup_help: "",
            empty: Say::NONE,
            props: &[],
            prop_say: Say::NONE,
            children: &[],
            child_say: Say::NONE,
        }
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

/// One node against its schema, and everything under it.
pub fn check(node: &KdlNode, schema: &Node, src: &Source, issues: &mut Issues) {
    let here = node.name().span();
    match schema.arg {
        Arg::None => {
            if let Some(stray) = string_arg(node) {
                schema.arg_say.raise(stray, here, src, issues);
            }
        }
        Arg::Str => {
            if string_arg(node).is_none() {
                schema.arg_say.raise(schema.name, here, src, issues);
            }
        }
        Arg::Bool => {
            if bool_arg(node).is_none() {
                schema.arg_say.raise(schema.name, here, src, issues);
            }
        }
        Arg::Strs => {}
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
                };
                if !ok {
                    prop.say.raise(key, entry.span(), src, issues);
                }
            }
            None => schema.prop_say.raise(key, entry.span(), src, issues),
        }
    }

    let children = kids(node);
    if children.is_empty() {
        schema.empty.raise(schema.name, here, src, issues);
    }

    let mut seen: Vec<(&str, Span)> = Vec::new();
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
        if sub.once {
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == name) {
                let issue = Issue::new(format!("`{name}` is declared twice"), src)
                    .at(*first, "first here")
                    .at(span, "and again here");
                issues.push(match sub.dup_help.is_empty() {
                    true => issue,
                    false => issue.help(sub.dup_help),
                });
                continue;
            }
            seen.push((name, span));
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

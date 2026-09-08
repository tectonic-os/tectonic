//! module.kdl: the module author's file.

use crate::diag::{Issue, Issues, Source, Span};
use crate::layout;
use crate::model::image::{Entry, Image, List};
use crate::model::module::{
    Collect, Contribution, Copr, Coverage, Decl, FileMode, Key, Module, PackageGroup, Position,
    VerifyException,
};
use crate::model::remote::REMOTE_DIR;
use crate::parse::disk::Disk;
use crate::parse::prop_span;
use crate::parse::schema::{check_doc, Arg, Kind, Node, Prop, Say};
use crate::parse::{asset, boolean, check_capability, child, flag, int_prop, kids, options, prop};
use crate::parse::{string_arg, string_args, syntax_issue};
use crate::resolve::options as resolve_options;
use crate::runtime::{class_names, VERIFY_CLASSES};
use kdl::{KdlDocument, KdlNode};
use std::collections::BTreeSet;
use std::path::Path;

/// The base families this repository knows how to build on, which is also the
/// set of directory names a module may gate files behind.
pub const FAMILIES: [&str; 3] = ["fedora", "debian", "ubuntu"];

const TOKEN_HELP: &str = "package names and repo IDs are emitted straight into the RUN line, so they are limited to letters, digits and . _ + : -; anything else belongs in module.sh, where it can be quoted deliberately";

/// Why a package name or repo ID is not safe to emit, or None when it is.
fn bad_token(value: &str) -> Option<&'static str> {
    if value.is_empty() {
        return Some("is empty");
    }
    if value.starts_with('-') {
        return Some("starts with a dash");
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._+:-".contains(c))
    {
        return Some("has a character that is not allowed");
    }
    None
}

/// The generators the tool implements. A manifest picks one; it never names a
/// command of its own.
const GENERATORS: [&str; 3] = ["cosign", "openssl", "ssh-keygen"];

/// The RSA size an `openssl` key is generated at when the declaration names
/// none.
const BITS: u32 = 4096;

const NEEDED: Say = Say::new("a `key` needs `{}`", "incomplete", "");

const PRIORITY: Say = Say::new(
    "`priority` is a number from 0 to 9999",
    "not a priority",
    "it becomes the NNNN in the staged filename, so four digits is the whole range there is",
);

#[rustfmt::skip]
const KEY: Node = Node::new("key",
    "A key `tect create key` generates for this module, and where each half of it goes.")
    .arg(Arg::Str, Say::new("`key` needs a kind", "no kind given",
        "`key \"cosign\" { ... }`, the kind being what `tect create key` names"))
    .unique(Say::new("key `{}` is declared twice", "already declared above", ""))
    .props(&[], Say::new("unknown key property `{}`", "not part of the schema",
        "a key carries its kind, and everything else as child nodes"))
    .children(&[
        Node::new("generator", "Which of the generators the tool implements writes this key.")
            .arg(Arg::One(&GENERATORS), Say::new("`{}` is not a generator the tool has",
                "not a generator",
                "the generators are `cosign`, `openssl` and `ssh-keygen`; a manifest picks one \
                 of them by name"))
            .once("")
            .missing(NEEDED)
            .props(&[
                Prop { name: "profile", kind: Kind::One(&["module-signing"]),
                    desc: "What the generator is set up for, where it can do more than one thing.",
                    say: Say::new("`profile` must be \"module-signing\"", "not a profile", ""),
                    missing: Say::NONE },
                Prop { name: "bits", kind: Kind::Int(2048, 16384),
                    desc: "The RSA key size, 4096 where none is named.",
                    say: Say::new("`bits` is a number from 2048 to 16384", "not a key size", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `generator` property `{}`", "not part of the schema",
                "`generator` accepts `profile` and `bits`")),
        Node::new("public",
            "Where the public half is shipped, which is a contract path this module provides.")
            .arg(Arg::Str, Say::new("`public` needs an absolute path", "no path given", ""))
            .once("")
            .missing(NEEDED)
            .props(&[
                Prop { name: "format", kind: Kind::One(&["pem", "der"]),
                    desc: "What the public half is written as, PEM where none is named.",
                    say: Say::new("`format` must be \"pem\" or \"der\"", "not a format", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `public` property `{}`", "not part of the schema",
                "`public` accepts `format`")),
        Node::new("private", "What the private half is called under `keys/private/`.")
            .arg(Arg::Str, Say::new("`private` needs a filename", "no filename given", ""))
            .once("")
            .missing(NEEDED),
    ], Say::new("unknown node `{}` in a key", "not part of the schema",
        "a key holds `generator`, `public` and `private`"));

/// The nodes a `family` block may hold, which `MODULE` holds too: outside a
/// gate is every family the module supports, inside one is the families it
/// names. Each is a const of its own because `MODULE` is a `const Node` whose
/// `.children(&[..])` cannot name the list it is itself part of. Everything
/// absent here is family-neutral.
const GATED: &[Node] = &[PACKAGES, PACKAGE_GROUPS, COPR, REQUIRES, AFTER, SATISFIES];

#[rustfmt::skip]
const REQUIRES: Node = Node::new("requires",
    "A capability another module has to provide, which also orders the build.")
    .arg(Arg::Strs, Say::new("`{}` needs a capability name", "nothing named", ""));

#[rustfmt::skip]
const AFTER: Node = Node::new("after",
    "A module this one builds after without requiring anything of it.")
    .arg(Arg::Strs, Say::new("`{}` needs a capability name", "nothing named", ""));

#[rustfmt::skip]
const PACKAGES: Node = Node::new("packages",
    "The packages this module installs, on every family it supports or, inside a `family` block, \
     on the families that names.")
    .arg(Arg::Strs, Say::new("`packages` needs at least one name", "nothing to install",
        "`packages \"htop\" \"tmux\"`, or `family \"fedora\" { packages \"vim-enhanced\" }` where \
         the names differ by family"))
    .props(&[
        Prop { name: "enablerepo", kind: Kind::Str,
            desc: "A repository enabled for this install and disabled otherwise. Fedora only, so \
                   the batch has to resolve to Fedora alone.",
            say: Say::NONE,
            missing: Say::NONE },
    ], Say::new("unknown `packages` property `{}`", "not part of the schema",
        "`packages` accepts `enablerepo`"));

#[rustfmt::skip]
const PACKAGE_GROUPS: Node = Node::new("package-groups",
    "The package groups this module installs. Fedora only, so an ungated one is a module \
     supporting Fedora alone.")
    .arg(Arg::Strs, Say::new("`package-groups` needs at least one name", "nothing to install",
        "`package-groups \"kde-desktop\"`"))
    .props(&[
        Prop { name: "enablerepo", kind: Kind::Str,
            desc: "A repository enabled for this install and disabled otherwise.",
            say: Say::NONE,
            missing: Say::NONE },
    ], Say::new("unknown `package-groups` property `{}`", "not part of the schema",
        "`package-groups` accepts `enablerepo`"));

#[rustfmt::skip]
const COPR: Node = Node::new("copr",
    "A COPR repository this module enables for its own installs, as owner/project. Fedora only.")
    .arg(Arg::Str, Say::new("`copr` needs one owner/project string", "nothing named",
        "`copr \"owner/project\"`"))
    .unique(Say::new("copr `{}` is declared twice", "already declared above", ""))
    .props(&[], Say::new("`{}` is not a `copr` property", "not part of the schema",
        "`copr \"owner/project\"`"));

#[rustfmt::skip]
const SATISFIES: Node = Node::new("satisfies",
    "The benchmarks and rules this module claims to harden, as an audit declaration. The tool \
     records it and certifies nothing.")
    .once("a module makes one claim set per gate; two blocks in one place split it")
    .children(BENCHMARKS, Say::NONE);

/// The rows inside a `satisfies` block, which read the same wherever the block
/// is declared: a module gates one, an image's base declares one flat.
#[rustfmt::skip]
pub(crate) const BENCHMARKS: &[Node] = &[
    Node::new("", "One benchmark, and the rule IDs it covers.")
        .arg(Arg::Strs, Say::new("`{}` has no rules listed", "nothing to cover", ""))
        .unique(Say::new("benchmark `{}` is declared twice", "already declared above", "")),
];

/// The gate. Nodes inside are taken only on the families it names; nodes
/// outside any gate are taken on every family the module supports. Files are
/// gated by a directory of the same name -- see `layout::family_dir` -- and
/// never by a node here.
#[rustfmt::skip]
const FAMILY: Node = Node::new("family",
    "The declarations inside taken only on the base families named, everything outside a gate \
     being taken on every family the module supports.")
    .arg(Arg::Strs, Say::new("`family` needs at least one family name", "nothing named",
        "`family \"debian\" \"ubuntu\" { packages \"vim\" }`; one gate takes as many families as \
         share the declaration"))
    .empty(Say::new("`{}` gates nothing", "an empty block",
        "a gate with nothing in it says the module does something on that family and then does \
         not; drop it, or move the declaration inside"))
    .children(GATED, Say::new("`{}` is not gated by family", "not allowed in a `family` block",
        "a `family` block holds `packages`, `package-groups`, `copr`, `requires`, `after` and \
         `satisfies`; `module.sh`, `finalize.sh` and `files/` are gated by putting them in a \
         `<family>/` directory, and everything else a manifest declares is family-neutral"));

/// The manifest's grammar, and the whole of it.
#[rustfmt::skip]
pub const MODULE: Node = Node::new("module",
    "One module: what it builds on, what it needs from the rest, and what it installs.")
    .children(&[
        Node::new("description", "One line naming the module in the resolved build summary.")
            .arg(Arg::Str, Say::new("`description` needs a string", "no description given", ""))
            .once(""),
        Node::new("supports", "The base families this module builds on, matched against the \
             image's `family`.")
            .arg(Arg::Strs, Say::NONE),

        Node::new("provides", "A capability this module satisfies for the modules that require it.")
            .arg(Arg::Strs, Say::new("`{}` needs a capability name", "nothing named", "")),
        REQUIRES,
        AFTER,

        Node::new("provides-file", "An absolute path this module guarantees, which another module \
             may require.")
            .arg(Arg::Strs, Say::NONE)
            .props(&[
                Prop { name: "build-only", kind: Kind::Bool,
                    desc: "Whether the path exists only while the build runs.",
                    say: Say::new("`build-only` takes #true or #false", "not a boolean", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `provides-file` property `{}`", "not part of the schema",
                "`provides-file` accepts `build-only`")),
        Node::new("requires-file", "An absolute path some other module has to ship.")
            .arg(Arg::Strs, Say::NONE)
            .props(&[], Say::new("`{}` is not a `requires-file` property",
                "only `provides-file` declares a lifetime", "")),
        Node::new("overrides", "An absolute path this module replaces deliberately.")
            .arg(Arg::Strs, Say::NONE)
            .props(&[], Say::new("`{}` is not an `overrides` property",
                "only `provides-file` declares a lifetime", "")),
        Node::new("mode", "An octal file mode applied to one path in this module's overlay.")
            .arg(Arg::StrPair("path, then octal mode"),
                Say::new("`mode` needs one path and one octal file mode", "incomplete",
                    "`mode \"/etc/example.conf\" \"0644\"`"))
            .unique(Say::new("mode for `{}` is declared twice", "already declared above", ""))
            .props(&[], Say::new("`{}` is not a `mode` property", "not part of the schema",
                "`mode \"/etc/example.conf\" \"0440\"`")),

        KEY,

        Node::new("secret", "A build secret this module's layer mounts.")
            .arg(Arg::Strs, Say::new("`{}` needs a name", "nothing named", "")),
        Node::new("arg", "A build argument this module's layer reads.")
            .arg(Arg::Strs, Say::new("`{}` needs a name", "nothing named", "")),
        Node::new("helpers", "Files from this module mounted by basename into /ctx/lib in every module layer.")
            .arg(Arg::Strs, Say::new("`helpers` needs a path", "nothing named", "")),

        Node::new("allow-verify",
            "One `tect validate-image` diagnostic accepted on one unit, leaving the rest of the image checked.")
            .arg(Arg::Str, Say::NONE)
            .props(&[
                Prop { name: "unit", kind: Kind::Str,
                    desc: "The unit the exception applies to.",
                    say: Say::new("`unit` must be a string", "not a string", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `allow-verify` property `{}`", "not part of the schema",
                "`allow-verify` accepts `unit`")),

        Node::new("refuses",
            "One benchmark rule this module deliberately leaves unsatisfied, which no remediation may set on its behalf.")
            .arg(Arg::Str, Say::new("`refuses` needs a rule ID", "nothing named",
                "`refuses \"grub2_nousb_argument\" because=\"it removes the keyboard\"`"))
            .unique(Say::new("`{}` is refused twice", "already refused above", ""))
            .props(&[
                Prop { name: "because", kind: Kind::Str,
                    desc: "Why the rule is left unsatisfied, which is the whole point of declaring it.",
                    say: Say::new("`because` must be a string", "not a string", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `refuses` property `{}`", "not part of the schema",
                "`refuses` accepts `because`")),

        Node::new("collects", "A filename this module gathers from every module that ships one.")
            .arg(Arg::Str, Say::NONE)
            .props(&[
                Prop { name: "into", kind: Kind::Str,
                    desc: "The absolute path the assembled file is written to.",
                    say: Say::NONE,
                    missing: Say::NONE },
                Prop { name: "priority", kind: Kind::Int(0, 9999),
                    desc: "Where a contribution lands when it declares none.",
                    say: PRIORITY,
                    missing: Say::NONE },
            ], Say::new("unknown `collects` property `{}`", "not part of the schema",
                "`collects` accepts `into` and `priority`")),
        Node::new("contributes", "A file this module ships for another module to collect.")
            .arg(Arg::Str, Say::NONE)
            .props(&[
                Prop { name: "priority", kind: Kind::Int(0, 9999),
                    desc: "Where this file lands in the assembled one.",
                    say: PRIORITY,
                    missing: Say::NONE },
            ], Say::new("unknown `contributes` property `{}`", "not part of the schema",
                "`contributes` accepts `priority`")),

        Node::new("fragment",
            "Where the module's Containerfile.inc goes relative to the generated layer.")
            .arg(Arg::None, Say::new("`fragment` takes no arguments", "unexpected value",
                "`fragment position=\"after\"`"))
            .once("")
            .props(&[
                Prop { name: "position", kind: Kind::One(&["before", "after", "tail"]),
                    desc: "Whether the fragment goes above the generated block, below it, or below the finalize layer.",
                    say: Say::new("`position` must be \"before\", \"after\" or \"tail\"", "not a position",
                        "before, the default, puts the fragment above the generated block; after \
                         puts it below; tail puts it below the finalize layer, where the lineage \
                         stage has ended and can be named"),
                    missing: Say::NONE },
                Prop { name: "standard-layer", kind: Kind::Bool,
                    desc: "Whether the generated block is emitted at all.",
                    say: Say::new("`standard-layer` must be #true or #false", "not a boolean", ""),
                    missing: Say::NONE },
            ], Say::new("unknown fragment property `{}`", "not part of the schema",
                "a fragment accepts `position` and `standard-layer`")),

        options::OPTION,
        options::VARIANT,
        asset::ASSET,

        PACKAGES,
        PACKAGE_GROUPS,
        COPR,
        SATISFIES,

        FAMILY,
    ], Say::new("unknown node `{}`", "not part of the schema",
        "docs/schema.md documents every node a manifest may hold"));

/// `owner/project` split into its two segments, or None when it is not that.
/// The one place a COPR name is read, so `copr` and an `enablerepo` naming one
/// cannot disagree about the shape.
fn copr_parts(value: &str) -> Option<(&str, &str)> {
    let (owner, project) = value.split_once('/')?;
    let both_safe = bad_token(owner).is_none() && bad_token(project).is_none();
    both_safe.then_some((owner, project))
}

/// One `packages` or `package-groups` node: the names it carries, held to what
/// a RUN line can hold, and one batch per family the enclosing gate names. A
/// node outside a gate yields one batch naming no family, which `spread`
/// resolves once `supports` has been read.
fn batch(
    node: &KdlNode,
    gate: &[String],
    src: &Source,
    issues: &mut Issues,
    noun: &str,
    sample: &str,
) -> Vec<PackageGroup> {
    let block = node.name().value();
    let mut names: Vec<String> = Vec::new();
    for arg in node.entries().iter().filter(|e| e.name().is_none()) {
        let Some(value) = arg.value().as_string() else {
            issues.push(
                Issue::new(format!("a {noun} name has to be a string"), src)
                    .at(arg.span(), "not a string")
                    .help(format!("quote it: `{block} \"{sample}\"`")),
            );
            continue;
        };
        if let Some(problem) = bad_token(value) {
            issues.push(
                Issue::new(format!("{noun} name `{value}` {problem}"), src)
                    .at(arg.span(), "would not survive the RUN line")
                    .help(TOKEN_HELP),
            );
            continue;
        }
        names.push(value.to_string());
    }
    if names.is_empty() {
        // An empty node is what the schema table already reported.
        return Vec::new();
    }

    let mut enablerepo: Option<String> = None;
    if let Some(span) = prop_span(node, "enablerepo") {
        match prop(node, "enablerepo").filter(|v| !v.is_empty()) {
            // `owner/project` names one of this module's `copr` nodes and is
            // turned into the selector below, once every node is read.
            Some(repo) => match bad_token(repo).filter(|_| copr_parts(repo).is_none()) {
                Some(problem) => issues.push(
                    Issue::new(format!("repo ID `{repo}` {problem}"), src)
                        .at(span, "would not survive the RUN line")
                        .help(TOKEN_HELP),
                ),
                None => enablerepo = Some(repo.to_string()),
            },
            None => issues.push(
                Issue::new("`enablerepo` needs a repo ID string", src).at(span, "not a string"),
            ),
        }
    }

    // No family here is not "no families": it is every family the module
    // supports, which `spread` fills in.
    let families: Vec<String> = match gate.is_empty() {
        true => vec![String::new()],
        false => gate.to_vec(),
    };
    families
        .into_iter()
        .map(|family| PackageGroup {
            family,
            packages: names.clone(),
            enablerepo: enablerepo.clone(),
            span: node.name().span().into(),
        })
        .collect()
}

/// A batch declared outside a gate installs on every family the module
/// supports. It waits for the whole manifest because `supports` may be written
/// below the `packages` leaning on it, and because a Fedora-only diagnostic is
/// about the families a batch resolved to.
fn spread(
    batches: &mut Vec<PackageGroup>,
    supports: &[String],
    block: &str,
    fedora_only: bool,
    src: &Source,
    issues: &mut Issues,
) {
    // A gate naming two families is two batches off one declaration, so what is
    // said about it is said once, against the node.
    let mut said: Option<Span> = None;
    for mut batch in std::mem::take(batches) {
        let families: Vec<String> = match batch.family.is_empty() {
            false => vec![std::mem::take(&mut batch.family)],
            true => supports.to_vec(),
        };
        let first = said.replace(batch.span) != Some(batch.span);
        if let Some(stray) = families.iter().find(|family| *family != "fedora") {
            if fedora_only {
                if first {
                    issues.push(
                        Issue::new(format!("`{block}` is Fedora-only, and this one covers `{stray}`"), src)
                        .at(batch.span, "no group installer on this family")
                            .help("only the Fedora adapter installs package groups; a module supporting more than Fedora declares it inside `family \"fedora\"`"),
                    );
                }
                continue;
            }
            if batch.enablerepo.is_some() {
                if first {
                    issues.push(
                        Issue::new(format!("`enablerepo` is Fedora-only, and this batch covers `{stray}`"), src)
                            .at(batch.span, "no repo to enable on this family")
                            .help("declare the batch inside `family \"fedora\"`; a Debian or Ubuntu install takes the base image's configured sources"),
                    );
                }
                batch.enablerepo = None;
            }
        }
        batches.extend(families.into_iter().map(|family| PackageGroup {
            family,
            packages: batch.packages.clone(),
            enablerepo: batch.enablerepo.clone(),
            span: batch.span,
        }));
    }
}

/// The families one `family` gate names, each held to the set this repository
/// builds on. A gate takes as many as share the declaration.
fn gate_families(node: &KdlNode, src: &Source, issues: &mut Issues) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in string_args(node) {
        if !FAMILIES.contains(&name) {
            issues.push(
                Issue::new(format!("unknown base family `{name}`"), src)
                    .at(node.name().span(), "not a family this repository builds on")
                    .help(format!("known families: {}", FAMILIES.join(", "))),
            );
            continue;
        }
        if out.iter().any(|seen| seen == name) {
            issues.push(
                Issue::new(format!("`{name}` is named twice on one gate"), src)
                    .at(node.name().span(), "already named here"),
            );
            continue;
        }
        out.push(name.to_string());
    }
    out
}

/// What the gated nodes have put into a module so far, so a gate this image is
/// not built for can be read for its diagnostics and then dropped. `packages`
/// and `package-groups` are absent because a batch carries the family it
/// resolved to and is filtered where it is used.
struct Gated {
    coprs: usize,
    requires: usize,
    after: usize,
    satisfies: usize,
}

/// A declared `priority=`, which the schema table holds to its range. `None`
/// where one is declared is what the table already reported.
fn priority(node: &KdlNode) -> Option<u32> {
    int_prop(node, "priority").map(|value| value as u32)
}

/// Whether a `priority=` is there but not a number the schema accepts, which is
/// its diagnostic. A second one here would say it twice.
fn bad_priority(node: &KdlNode) -> bool {
    prop_span(node, "priority").is_some() && priority(node).is_none()
}

fn regular_overlay_file(root: &Path, path: &str) -> bool {
    let mut at = root.to_path_buf();
    let mut parts = path.trim_start_matches('/').split('/').peekable();
    while let Some(part) = parts.next() {
        at.push(part);
        let Ok(metadata) = at.symlink_metadata() else {
            return false;
        };
        if match parts.peek() {
            Some(_) => !metadata.is_dir(),
            None => !metadata.is_file(),
        } {
            return false;
        }
    }
    true
}

impl Module {
    pub fn load(entry: &Entry, image: &Image, root: &Path, issues: &mut Issues) -> Option<Self> {
        if (entry.remote.is_some() || entry.source.is_some())
            && layout::module(root, &entry.path).is_dir()
        {
            issues.push(
                Issue::new(
                    format!("`{}` is referenced but also exists in tree", entry.path),
                    &image.src,
                )
                .at(entry.span, "two modules would answer to this name")
                .help(format!(
                    "rename the referenced one, or drop modules/{}",
                    entry.path
                )),
            );
        }

        let dir_rel = entry.dir();
        let file = root
            .join(layout::MODULES)
            .join(&dir_rel)
            .join(layout::MODULE_FILE)
            .display()
            .to_string();

        let Ok(text) = std::fs::read_to_string(&file) else {
            issues.push(
                Issue::new(
                    format!("`{}` has no module.kdl", entry.path),
                    &image.src,
                )
                .at(entry.span, "every module needs a manifest")
                .help(match (&entry.source, &entry.remote) {
                    (Some(_), _) | (_, Some(_)) => "run ./scripts/tect.sh fetch modules to fetch what the image references"
                        .to_string(),
                    (None, None) => format!(
                        "create {file}; modules/_template/module-name/module.kdl is a copy-me reference"
                    ),
                }),
            );
            return None;
        };

        let mut module = Self::parse(
            &entry.path,
            &dir_rel,
            root,
            text,
            image.base.as_ref().map(|base| base.family.as_str()),
            issues,
        )?;
        module.flavour = entry.flavour.clone();
        let src = &module.src.clone();
        module.resolved =
            resolve_options::resolve(&module.options, &module.variants, src, entry, image, issues);
        Some(module)
    }

    /// Everything a manifest says on its own, so a module no image lists is
    /// still held to the schema.
    fn parse(
        path: &str,
        dir_rel: &str,
        root: &Path,
        text: String,
        family: Option<&str>,
        issues: &mut Issues,
    ) -> Option<Self> {
        let dir = layout::module(root, dir_rel);
        let file = dir.join(layout::MODULE_FILE).display().to_string();

        let src = &Source::new(&file, text.clone());
        let doc: KdlDocument = match text.parse() {
            Ok(doc) => doc,
            Err(err) => {
                issues.push(syntax_issue(&err, &file, src));
                return None;
            }
        };

        let mut module = Module {
            path: path.to_string(),
            dir: dir_rel.to_string(),
            description: String::new(),
            supports: Vec::new(),
            provides: Vec::new(),
            requires: Vec::new(),
            after: Vec::new(),
            policies: [layout::SELINUX, layout::APPARMOR]
                .iter()
                .filter(|policy| !policy.files(&dir).is_empty())
                .map(|policy| policy.capability)
                .collect(),
            provides_files: Vec::new(),
            provides_files_build_only: Vec::new(),
            requires_files: Vec::new(),
            overrides: Vec::new(),
            verify_exceptions: Vec::new(),
            refuses: Vec::new(),
            flavour: None,
            collects: Vec::new(),
            contributes: Vec::new(),
            modes: Vec::new(),
            keys: Vec::new(),
            secrets: Vec::new(),
            args: Vec::new(),
            options: Vec::new(),
            variants: Vec::new(),
            assets: Vec::new(),
            packages: Vec::new(),
            groups: Vec::new(),
            coprs: Vec::new(),
            helpers: Vec::new(),
            satisfies: Vec::new(),
            resolved: Vec::new(),
            fragment: std::fs::read_to_string(dir.join("Containerfile.inc")).ok(),
            fragment_position: Position::default(),
            standard_layer: true,
            content: crate::provenance::record::hash(&dir),
            imported: crate::provenance::record::read(&dir, issues),
            // Every family, since a manifest is read without one: a `repo`
            // gated to a family this build is not for is still an archive the
            // module configures somewhere.
            repo: layout::FAMILY_DIRS
                .iter()
                .map(|(gated, _)| dir.join(gated).join("repo"))
                .chain(std::iter::once(dir.join("repo")))
                .any(|at| at.is_file()),
            src: src.clone(),
        };

        check_doc(&doc, &MODULE, src, issues);

        let mut fragment_seen = false;
        let mut gated: Vec<(String, Span)> = Vec::new();
        for node in doc.nodes() {
            if node.name().value() != "family" {
                module.take(node, &[], &dir, src, issues, &mut fragment_seen);
                continue;
            }
            // A gate this image is not built for is read all the same, so a
            // module published for other families is held to the same checks.
            // What it declared is then dropped.
            let gate = gate_families(node, src, issues);
            gated.extend(
                gate.iter()
                    .map(|name| (name.clone(), node.name().span().into())),
            );
            let keep = family.is_none_or(|want| gate.iter().any(|name| name == want));
            let mark = module.gated();
            for child in kids(node) {
                module.take(child, &gate, &dir, src, issues, &mut fragment_seen);
            }
            if !keep {
                module.ungate(mark);
            }
        }

        // Every batch outside a gate resolved against `supports`, which is only
        // whole now, and the Fedora-only nodes held to what they resolved to.
        let supports = module.supports.clone();
        spread(
            &mut module.packages,
            &supports,
            "packages",
            false,
            src,
            issues,
        );
        spread(
            &mut module.groups,
            &supports,
            "package-groups",
            true,
            src,
            issues,
        );

        // The public half is a contract path, derived.
        Self::rest(module, path, root, &dir, &gated, src, issues)
    }

    /// One node of a manifest, from the top level or from inside a `family`
    /// gate. `gate` is the families that gate names, and is empty for a node
    /// outside one.
    fn take(
        &mut self,
        node: &KdlNode,
        gate: &[String],
        dir: &Path,
        src: &Source,
        issues: &mut Issues,
        fragment_seen: &mut bool,
    ) {
        let module = self;
        let path = module.path.clone();
        {
            match node.name().value() {
                "description" => {
                    if module.description.is_empty() {
                        module.description = string_arg(node).unwrap_or_default().to_string();
                    }
                }
                "supports" => module.parse_supports(node, src, issues),
                kind @ ("provides" | "requires" | "after") => {
                    module.parse_capabilities(kind, node, src, issues)
                }
                kind @ ("provides-file" | "requires-file" | "overrides") => {
                    module.parse_paths(kind, node, src, issues)
                }
                "mode" => module.parse_mode(node, dir, src, issues),
                kind @ ("secret" | "arg") => {
                    for name in string_args(node) {
                        let decl = Decl {
                            name: name.to_string(),
                            span: node.name().span().into(),
                        };
                        if kind == "secret" {
                            module.secrets.push(decl);
                        } else {
                            module.args.push(decl);
                        }
                    }
                }
                "helpers" => module.parse_helpers(node, dir, src, issues),
                "allow-verify" => module.parse_allow_verify(node, src, issues),
                "refuses" => module.parse_refuses(node, src, issues),
                "collects" => module.parse_collects(node, src, issues),
                "contributes" => module.parse_contributes(node, dir, src, issues),
                "fragment" => {
                    if *fragment_seen {
                        return;
                    }
                    *fragment_seen = true;
                    if module.fragment.is_none() {
                        issues.push(
                            Issue::new(
                                format!("`{}` declares `fragment` but ships no Containerfile.inc", path),
                                src,
                            )
                            .at(node.name().span(), "nothing to place")
                            .help("shipping the file is what adds a fragment; this node only says where it goes"),
                        );
                    }
                    module.parse_fragment(node, src, issues);
                }
                "option" => {
                    if let Some(opt) = options::parse_option(node, src, issues) {
                        // A duplicate is a schema issue; keeping the first leaves the read-outs at one.
                        if !module.options.iter().any(|o| o.name == opt.name) {
                            module.options.push(opt);
                        }
                    }
                }
                "asset" => {
                    if let Some(pin) = asset::parse(node, src, issues) {
                        if !module.assets.iter().any(|a| a.name == pin.name) {
                            module.assets.push(pin);
                        }
                    }
                }
                "variant" => {
                    if let Some(variant) = options::parse_variant(node) {
                        if !module.variants.iter().any(|v| v.name == variant.name) {
                            module.variants.push(variant);
                        }
                    }
                }
                "key" => {
                    if let Some(key) = parse_key(node, src, issues) {
                        if !module.keys.iter().any(|k| k.kind == key.kind) {
                            module.keys.push(key);
                        }
                    }
                }
                "packages" => module.parse_packages(node, gate, src, issues),
                "package-groups" => module.parse_package_groups(node, gate, src, issues),
                "copr" => module.parse_copr(node, src, issues),
                "satisfies" => module.satisfies.extend(coverages(node, src, issues)),
                _ => {}
            }
        }
    }

    /// What the gated nodes have contributed so far.
    fn gated(&self) -> Gated {
        Gated {
            coprs: self.coprs.len(),
            requires: self.requires.len(),
            after: self.after.len(),
            satisfies: self.satisfies.len(),
        }
    }

    /// Everything one gate contributed, dropped: the block was read for its
    /// diagnostics and this image is not built for the families it names.
    fn ungate(&mut self, mark: Gated) {
        self.coprs.truncate(mark.coprs);
        self.requires.truncate(mark.requires);
        self.after.truncate(mark.after);
        self.satisfies.truncate(mark.satisfies);
    }

    fn rest(
        mut module: Module,
        path: &str,
        root: &Path,
        dir: &Path,
        gated: &[(String, Span)],
        src: &Source,
        issues: &mut Issues,
    ) -> Option<Self> {
        let derived: Vec<Decl> = module
            .keys
            .iter()
            .filter(|key| !module.provides_files.iter().any(|d| d.name == key.public))
            .map(|key| Decl {
                name: key.public.clone(),
                span: key.span,
            })
            .collect();
        module.provides_files.extend(derived);

        for key in &module.keys {
            let file = layout::public_key(root, &key.public);
            if !layout::nonempty(&file) {
                issues.push(
                    Issue::new(
                        format!("`{}` has no public half for its {} key", path, key.kind),
                        src,
                    )
                    .at(key.span, format!("{} is missing or empty", file.display()))
                    .help(format!("run `tect create key {}`", key.kind)),
                );
            }
        }

        // An `enablerepo` naming a COPR is a reference to a `copr` node, which
        // may be read after it: the selector is derived once both are in.
        let coprs: Vec<(String, String)> = module
            .coprs
            .iter()
            .map(|copr| (copr.name(), copr.selector()))
            .collect();
        for batch in module.packages.iter_mut().chain(&mut module.groups) {
            let Some(named) = batch.enablerepo.as_deref().filter(|v| v.contains('/')) else {
                continue;
            };
            match coprs.iter().find(|(name, _)| name == named) {
                Some((_, selector)) => batch.enablerepo = Some(selector.clone()),
                None => issues.push(
                    Issue::new(
                        format!("`enablerepo` names COPR `{named}`, which is not declared here"),
                        src,
                    )
                    .at(batch.span, "no `copr` for it")
                    .help(format!("add `copr \"{named}\"`; a repository is reachable because the module says so, not because an install names it")),
                ),
            }
        }

        if module.description.is_empty() {
            issues.push(
                Issue::new(format!("`{}` declares no description", path), src)
                    .help("one line, present tense, no trailing period; it names the module in the resolved build summary"),
            );
        }
        if !module.standard_layer {
            let dropped = module
                .secrets
                .iter()
                .map(|d| ("secret", d.name.as_str(), d.span))
                .chain(module.args.iter().map(|d| ("arg", d.name.as_str(), d.span)))
                .chain(
                    module
                        .options
                        .iter()
                        .map(|o| ("option", o.name.as_str(), o.span)),
                )
                .chain(
                    module
                        .assets
                        .iter()
                        .map(|a| ("asset", a.name.as_str(), a.span)),
                )
                .chain(
                    module
                        .modes
                        .iter()
                        .map(|m| ("mode", m.path.as_str(), m.span)),
                )
                .chain(
                    std::iter::zip(&coprs, &module.coprs)
                        .map(|((name, _), copr)| ("copr", name.as_str(), copr.span)),
                );
            for (kind, name, span) in dropped {
                issues.push(
                    Issue::new(
                        format!(
                            "`{}` declares `{kind} \"{name}\"` with no standard layer to carry it",
                            path
                        ),
                        src,
                    )
                    .at(span, "nowhere to land")
                    .help("`standard-layer #false` makes the fragment the whole layer, so it has to carry this itself; drop one or the other"),
                );
            }
        }

        for (gated, taken_on) in layout::FAMILY_DIRS {
            if !dir.join(gated).is_dir()
                || taken_on
                    .iter()
                    .any(|family| module.supports.iter().any(|claimed| claimed == family))
            {
                continue;
            }
            let named = taken_on.join("` or `");
            issues.push(
                Issue::new(
                    format!("`{path}` ships a `{gated}/` directory and supports no family it is taken on"),
                    src,
                )
                .help(format!(
                    "nothing would ever read it, since the directory is taken on `{named}` and \
                     no image on either may enable this module: widen `supports`, or drop the \
                     directory"
                )),
            );
        }
        for (family, span) in gated {
            if module.supports.iter().any(|claimed| claimed == family) {
                continue;
            }
            issues.push(
                Issue::new(
                    format!("`{path}` gates on `{family}` and does not support it"),
                    src,
                )
                .at(*span, "no image reaches this block")
                .help(format!(
                    "a gate narrows what a module already builds on: add `{family}` to \
                         `supports`, or drop the block"
                )),
            );
        }

        if module.supports.is_empty() {
            issues.push(
                Issue::new(format!("`{}` declares no `supports`", path), src)
                    .help("a module has to say which base families it can build on, so a portability gap surfaces at lint, while the build is still cheap"),
            );
        }

        crate::provenance::check_fetch(&module, dir, issues);

        Some(module)
    }

    /// `supports "fedora"` The families this repository knows how to build on.
    fn parse_supports(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        for family in string_args(node) {
            if !FAMILIES.contains(&family) {
                issues.push(
                    Issue::new(format!("unknown base family `{family}`"), src)
                        .at(node.name().span(), "not a family this repository builds on")
                        .help(format!("known families: {}", FAMILIES.join(", "))),
                );
            }
            self.supports.push(family.to_string());
        }
    }

    /// `provides "a" "b"`, and the two nodes that carry the same list: what
    /// another module may require, and what only orders the build.
    fn parse_capabilities(
        &mut self,
        kind: &str,
        node: &KdlNode,
        src: &Source,
        issues: &mut Issues,
    ) {
        let decls = string_args(node)
            .iter()
            .map(|c| Decl {
                name: c.to_string(),
                span: node.name().span().into(),
            })
            .collect::<Vec<_>>();
        for decl in &decls {
            check_capability(&decl.name, decl.span, src, issues);
        }
        match kind {
            "provides" => self.provides.extend(decls),
            "requires" => self.requires.extend(decls),
            _ => self.after.extend(decls),
        }
    }

    /// `provides-file "/usr/bin/x" build-only=#true`, and the two nodes with the
    /// same shape: what has to be there, and what is replaced deliberately.
    fn parse_paths(&mut self, kind: &str, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let build_only = kind == "provides-file" && flag(node, "build-only");
        for path in string_args(node) {
            if !path.starts_with('/') {
                issues.push(
                    Issue::new(format!("`{path}` is not an absolute path"), src)
                        .at(node.name().span(), "an exact path in the image"),
                );
            }
            let decl = Decl {
                name: path.to_string(),
                span: node.name().span().into(),
            };
            match kind {
                "provides-file" => {
                    if build_only {
                        self.provides_files_build_only.push(path.to_string());
                    }
                    self.provides_files.push(decl);
                }
                "requires-file" => self.requires_files.push(decl),
                _ => self.overrides.push(decl),
            }
        }
    }

    /// `mode "/etc/example.conf" "0644"` A chmod mode for one shipped overlay
    /// file. The path stays separate from `provides-file`, which is a contract.
    fn parse_mode(&mut self, node: &KdlNode, dir: &Path, src: &Source, issues: &mut Issues) {
        let args: Vec<_> = node
            .entries()
            .iter()
            .filter(|entry| entry.name().is_none())
            .collect();
        if args.len() != 2 {
            return;
        }
        let Some(path) = args[0].value().as_string() else {
            return;
        };
        let Some(given) = args[1].value().as_string() else {
            return;
        };

        if path == "/"
            || !path.starts_with('/')
            || path
                .split('/')
                .skip(1)
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            issues.push(
                Issue::new(
                    format!("`{path}` is not a normal absolute overlay path"),
                    src,
                )
                .at(args[0].span(), "not a file path under `files/`")
                .help(
                    "name the path exactly as it lands in the image, such as `/etc/example.conf`",
                ),
            );
            return;
        }

        if given.is_empty() || !given.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
            issues.push(
                Issue::new(format!("`{given}` is not an octal file mode"), src)
                    .at(args[1].span(), "use only digits 0 through 7")
                    .help("a chmod file mode such as `0440`"),
            );
            return;
        }
        let mode = match u32::from_str_radix(given, 8) {
            Ok(mode) if mode <= 0o7777 => mode,
            _ => {
                issues.push(
                    Issue::new(format!("file mode `{given}` is out of range"), src)
                        .at(args[1].span(), "greater than 07777"),
                );
                return;
            }
        };

        if !regular_overlay_file(&dir.join(layout::OVERLAY), path) {
            issues.push(
                Issue::new(
                    format!(
                        "`{}` declares a mode for a file it does not ship",
                        self.path
                    ),
                    src,
                )
                .at(args[0].span(), format!("{path} is missing from `files/`"))
                .help("shipping the overlay file is what makes its mode meaningful"),
            );
            return;
        }
        self.modes.push(FileMode {
            path: path.to_string(),
            mode,
            span: args[0].span().into(),
        });
    }

    /// `allow-verify "man-page-missing" unit="x.service"` One known diagnostic
    /// accepted on one unit, which is why both halves are required.
    /// A refusal is a judgement, so it is worth nothing without its reason:
    /// the rule alone reads as an oversight the next reader will try to fix.
    fn parse_refuses(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let span: Span = node.name().span().into();
        let Some(rule) = string_arg(node).map(str::to_string) else {
            return;
        };
        let because = prop(node, "because").unwrap_or_default().to_string();
        if because.is_empty() {
            issues.push(
                Issue::new(format!("`{rule}` is refused without a reason"), src)
                    .at(span, "no `because`")
                    .help(
                        "a refusal keeps a rule out of remediation for good, so the reason is \
                         what stops the next reader undoing it: `because=\"the base is on \
                         yescrypt, which is stronger\"`",
                    ),
            );
        }
        self.refuses.push(crate::model::module::Refusal {
            rule,
            because,
            span,
        });
    }

    fn parse_allow_verify(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let span: Span = node.name().span().into();
        let class = string_arg(node).map(str::to_string);
        let unit = prop(node, "unit").map(str::to_string);

        let (Some(class), Some(unit)) = (class, unit) else {
            let missing = match (string_arg(node), prop(node, "unit")) {
                (None, _) => "a diagnostic class",
                (_, None) => "unit=, the unit it applies to",
                _ => "both a class and a unit",
            };
            issues.push(
                Issue::new(format!("`allow-verify` needs {missing}"), src)
                    .at(span, "incomplete")
                    .help(
                        "`allow-verify \"man-page-missing\" unit=\"plasmalogin.service\"`, \
                         which accepts one diagnostic on one unit and leaves the rest of the \
                         image checked",
                    ),
            );
            return;
        };

        if !VERIFY_CLASSES.iter().any(|(name, _)| *name == class) {
            issues.push(
                Issue::new(format!("`{class}` is not a verify diagnostic class"), src)
                    .at(span, "not one of the known classes")
                    .help(format!(
                        "known classes: {}. Each is named in full, and `tect validate-image` holds what it stands for",
                        class_names()
                    )),
            );
        } else if let Some(dup) = self
            .verify_exceptions
            .iter()
            .find(|e| e.class == class && e.unit == unit)
        {
            issues.push(
                Issue::new(format!("`{class}` is allowed twice on `{unit}`"), src)
                    .at(dup.span, "first here")
                    .at(span, "and again here"),
            );
        } else {
            self.verify_exceptions
                .push(VerifyException { class, unit, span });
        }
    }

    /// `collects "justfile.inc" into="/usr/share/just/justfile.apps"
    /// priority=500` The filename gathered from every module shipping one, and
    /// where the assembled result goes.
    fn parse_collects(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        if bad_priority(node) {
            return;
        }
        let collected = string_args(node).first().map(|s| s.to_string());
        let into = prop(node, "into");
        match (collected, into, priority(node)) {
            (Some(collected), Some(into), Some(priority)) if into.starts_with('/') => {
                self.collects.push(Collect {
                    file: collected,
                    into: into.to_string(),
                    priority,
                    span: node.name().span().into(),
                })
            }
            (collected, into, priority) => {
                let missing = if collected.is_none() {
                    "the filename it collects"
                } else if into.is_none() {
                    "into=, where the build puts them"
                } else if priority.is_none() {
                    "priority=, where a contribution lands when it names none"
                } else {
                    "an absolute into="
                };
                issues.push(
                    Issue::new(format!("`collects` needs {missing}"), src)
                        .at(node.name().span(), "incomplete")
                        .help("`collects \"justfile.inc\" into=\"/usr/share/just/justfile.apps\" priority=500`"),
                );
            }
        }
    }

    /// `contributes "justfile.inc" priority=900` A file this module ships for
    /// another to collect, so shipping it is what the node is about.
    fn parse_contributes(&mut self, node: &KdlNode, dir: &Path, src: &Source, issues: &mut Issues) {
        if bad_priority(node) {
            return;
        }
        let contributed = string_args(node).first().map(|s| s.to_string());
        match (contributed, priority(node)) {
            (Some(contributed), Some(priority)) => {
                // Every family, since a manifest is read without one: a
                // contribution gated to a family this build is not for is
                // still a file the module ships.
                let anywhere = layout::FAMILY_DIRS
                    .iter()
                    .map(|(gated, _)| dir.join(gated).join(&contributed))
                    .chain(std::iter::once(dir.join(&contributed)))
                    .any(|at| at.is_file());
                if !anywhere {
                    issues.push(
                        Issue::new(
                            format!("`{}` orders a {contributed} it does not ship", self.path),
                            src,
                        )
                        .at(node.name().span(), "nothing to order")
                        .help("shipping the file is what contributes it; this node only says where it lands"),
                    );
                } else if let Some(dup) = self.contributes.iter().find(|c| c.file == contributed) {
                    issues.push(
                        Issue::new(format!("`{contributed}` is ordered twice"), src)
                            .at(dup.span, "first here")
                            .at(node.name().span(), "and again here"),
                    );
                } else {
                    self.contributes.push(Contribution {
                        file: contributed,
                        priority,
                        span: node.name().span().into(),
                    });
                }
            }
            (contributed, _) => {
                let missing = match contributed.is_none() {
                    true => "the filename it contributes",
                    false => "priority=, which is the only thing it declares",
                };
                issues.push(
                    Issue::new(format!("`contributes` needs {missing}"), src)
                        .at(node.name().span(), "incomplete")
                        .help("`contributes \"justfile.inc\" priority=900`, for a module that has to land after the rest"),
                );
            }
        }
    }

    /// `fragment position="after" standard-layer=#false` Defaults are the
    /// additive case: the fragment goes above the generated block and the
    /// block is still emitted.
    fn parse_fragment(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let position = prop(node, "position").filter(|p| matches!(*p, "before" | "after" | "tail"));
        self.fragment_position = match position {
            Some("after") => Position::After,
            Some("tail") => Position::Tail,
            _ => Position::Before,
        };
        self.standard_layer = boolean(node, "standard-layer").unwrap_or(true);

        if !self.standard_layer {
            if let Some(span) = position.and_then(|_| prop_span(node, "position")) {
                issues.push(
                    Issue::new(
                        "`position` says nothing without a standard layer",
                        src,
                    )
                    .at(span, "there is nothing to be before or after")
                    .help("`standard-layer #false` makes the fragment the only thing this module emits"),
                );
            }
        }
    }

    /// `helpers "lib/family.sh"` Files mounted from this module into every
    /// standard layer, including layers ordered before this module.
    fn parse_helpers(&mut self, node: &KdlNode, dir: &Path, src: &Source, issues: &mut Issues) {
        for helper in string_args(node) {
            let path = Path::new(helper);
            if path.is_absolute()
                || path
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_)))
            {
                issues.push(
                    Issue::new(format!("`{helper}` is not a path inside this module"), src)
                        .at(node.name().span(), "helpers are module content")
                        .help("name a relative file shipped inside this module, such as `lib/family.sh`"),
                );
            } else if !dir.join(path).is_file() {
                issues.push(
                    Issue::new(
                        format!("`{}` declares a helper it does not ship", self.path),
                        src,
                    )
                    .at(node.name().span(), format!("{helper} is missing"))
                    .help("shipping the file is what makes it available to module layers"),
                );
            } else {
                self.helpers.push(Decl {
                    name: helper.to_string(),
                    span: node.name().span().into(),
                });
            }
        }
    }

    /// `packages "htop" "tmux"` The names as positional arguments, taken on the
    /// families of the `family` block holding it, or on every family the module
    /// supports where there is none.
    fn parse_packages(
        &mut self,
        node: &KdlNode,
        gate: &[String],
        src: &Source,
        issues: &mut Issues,
    ) {
        self.packages
            .extend(batch(node, gate, src, issues, "package", "htop"));
    }

    /// `copr "owner/project"` The two path segments, which is all a COPR is;
    /// the repository id, its URL and the selector an install enables are
    /// derived from them and never declared.
    fn parse_copr(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let Some(value) = string_arg(node) else {
            return;
        };
        let Some((owner, project)) = copr_parts(value) else {
            issues.push(
                Issue::new(format!("`{value}` is not a COPR owner/project"), src)
                    .at(node.name().span(), "not two usable path segments")
                    .help(format!("`copr \"owner/project\"`; {TOKEN_HELP}")),
            );
            return;
        };
        self.coprs.push(Copr {
            owner: owner.to_string(),
            project: project.to_string(),
            span: node.name().span().into(),
        });
    }

    /// `package-groups "kde-desktop"` The same shape as `packages`, installed
    /// by the family adapter's group verb.
    fn parse_package_groups(
        &mut self,
        node: &KdlNode,
        gate: &[String],
        src: &Source,
        issues: &mut Issues,
    ) {
        self.groups.extend(batch(
            node,
            gate,
            src,
            issues,
            "package group",
            "kde-desktop",
        ));
    }
}

/// `satisfies { cis-fedora "1.1.1.1" }` Each child names a benchmark and carries
/// the rule IDs the claimant covers. A row with no benchmark name is refused
/// here, so nothing downstream carries a claim it cannot name.
pub(crate) fn coverages(node: &KdlNode, src: &Source, issues: &mut Issues) -> Vec<Coverage> {
    let Some(children) = node.children() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for child in children.nodes() {
        let benchmark = child.name().value().to_string();
        if benchmark.is_empty() {
            issues.push(
                Issue::new("a benchmark name is required inside `satisfies`", src)
                    .at(child.name().span(), "empty name")
                    .help("`satisfies { cis-fedora \"1.1.1.1\" }`"),
            );
            continue;
        }
        out.push(Coverage {
            benchmark,
            rules: child
                .entries()
                .iter()
                .filter(|entry| entry.name().is_none())
                .filter_map(|entry| entry.value().as_string().map(str::to_string))
                .collect(),
            span: child.name().span().into(),
        });
    }
    out
}

/// `key "cosign" { generator "cosign"; public "/etc/..."; private "cosign.key" }`
/// The walker has already held the generator, the profile and the format to
/// their sets, so what is left is the two paths, which are written to.
pub fn parse_key(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Key> {
    let kind = string_arg(node)?.to_string();
    let generator = child(node, "generator")?;
    let public = child(node, "public")?;
    let private = child(node, "private")?;

    let path = string_arg(public)?;
    if !path.starts_with('/') || path.split('/').any(|part| part == "..") {
        issues.push(
            Issue::new(format!("`{path}` is not an absolute path in the image"), src)
                .at(public.name().span(), "the public half goes here")
                .help("it is the path the built image ships the public half at, and the module's files/ overlay is what puts it there"),
        );
        return None;
    }

    let name = string_arg(private)?;
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        issues.push(
            Issue::new(format!("`{name}` is not a filename"), src)
                .at(private.name().span(), "the private half is written here")
                .help("the private half is written under keys/private/ and never committed, so it is a plain name"),
        );
        return None;
    }

    Some(Key {
        kind,
        generator: string_arg(generator)
            .filter(|g| GENERATORS.contains(g))?
            .to_string(),
        profile: prop(generator, "profile").map(str::to_string),
        bits: int_prop(generator, "bits").unwrap_or(BITS as i128) as u32,
        public: path.to_string(),
        format: prop(public, "format").unwrap_or("pem").to_string(),
        private: name.to_string(),
        span: node.name().span().into(),
    })
}

/// The span the `satisfies` block takes, which is what a claim written by hand
/// or by a picker replaces. `None` is a manifest declaring none, which the
/// writer appends to instead.
pub fn satisfies_span(kdl: &str) -> Option<Span> {
    let doc: KdlDocument = kdl.parse().ok()?;
    let node = doc
        .nodes()
        .iter()
        .find(|node| node.name().value() == "satisfies")?;
    Some(node.span().into())
}

/// What a manifest says about itself, read without resolving it: what anything
/// asking about a module no image has loaded goes on. Nothing where it does
/// not parse.
#[derive(Default)]
pub struct Summary {
    pub description: String,
    /// Everything it declares as available to another module: `provides`,
    /// `provides-file`, and the public half of every key it declares.
    pub provides: Vec<String>,
    pub supports: Vec<String>,
    pub requires: Vec<String>,
    /// The key kinds it declares, which is what an absent one is traced back
    /// to this module by.
    pub keys: Vec<String>,
    /// The build args its layer reads, which is what decides whether a
    /// workflow may run here at all.
    pub args: Vec<String>,
    /// Every benchmark number it claims, over all the benchmarks it names,
    /// since a number resolves against the content.
    pub satisfies: Vec<String>,
}

pub fn summary(file: &Path) -> Summary {
    let Some(doc) = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| text.parse::<KdlDocument>().ok())
    else {
        return Summary::default();
    };
    // A gate holds `requires` and `satisfies` among others, and the index
    // answers for the module whatever family it is built on, so a gated
    // declaration counts here the same as one outside.
    let nodes: Vec<&KdlNode> = doc
        .nodes()
        .iter()
        .flat_map(|node| match node.name().value() {
            "family" => node
                .children()
                .map_or(Vec::new(), |kids| kids.nodes().iter().collect()),
            _ => vec![node],
        })
        .collect();
    let strings = |name: &str| -> Vec<String> {
        nodes
            .iter()
            .filter(|node| node.name().value() == name)
            .flat_map(|node| node.entries())
            .filter_map(|entry| entry.value().as_string().map(str::to_string))
            .collect()
    };
    let mut provides = strings("provides");
    provides.extend(strings("provides-file"));
    // A key's public half is a file the image gets, like any other.
    provides.extend(
        doc.nodes()
            .iter()
            .filter(|node| node.name().value() == "key")
            .filter_map(|node| {
                node.children()?
                    .get("public")?
                    .entries()
                    .first()?
                    .value()
                    .as_string()
            })
            .map(str::to_string),
    );
    Summary {
        description: strings("description").join(" "),
        provides,
        supports: strings("supports"),
        requires: strings("requires"),
        keys: strings("key"),
        args: strings("arg"),
        satisfies: nodes
            .iter()
            .filter(|node| node.name().value() == "satisfies")
            .filter_map(|node| node.children())
            .flat_map(KdlDocument::nodes)
            .flat_map(KdlNode::entries)
            .filter(|entry| entry.name().is_none())
            .filter_map(|entry| entry.value().as_string().map(str::to_string))
            .collect(),
    }
}

/// Every module on disk that no image lists, held to the schema on its own.
pub fn check_unlisted(list: &List, root: &Path, disk: &Disk, issues: &mut Issues) {
    let listed: BTreeSet<String> = list
        .images
        .iter()
        .flat_map(|image| image.entries.iter())
        .map(Entry::dir)
        .collect();

    for dir in disk.modules() {
        if listed.contains(dir) || dir.starts_with(REMOTE_DIR) {
            continue;
        }
        let file = layout::manifest(root, dir);
        if let Ok(text) = std::fs::read_to_string(&file) {
            Module::parse(dir, dir, root, text, None, issues);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(text: &str) -> Vec<String> {
        let doc: KdlDocument = text.parse().expect("valid KDL");
        let src = Source::new("module.kdl", text);
        let mut issues = Issues::default();
        check_doc(&doc, &MODULE, &src, &mut issues);
        issues.findings()
    }

    /// One manifest parsed as a module on its own: no image lists it, so no
    /// family gate is taken.
    fn parsed(name: &str, text: &str) -> Vec<String> {
        let mut issues = Issues::default();
        Module::parse(
            name,
            name,
            Path::new("."),
            text.to_string(),
            None,
            &mut issues,
        );
        issues.findings()
    }

    /// A refusal without its reason reads as an oversight, and the next reader
    /// undoes it. The rule is kept either way: refusing is still the module's
    /// decision, and dropping it would silently let remediation back in.
    #[test]
    fn a_refusal_wants_its_reason_and_is_kept_without_one() {
        let with = parsed(
            "refuses-well",
            r#"
description "refuses one rule"
supports "fedora"
refuses "set_password_hashing_algorithm_logindefs" \
    because="the base is on yescrypt, which is stronger than the SHA512 this asks for"
"#,
        );
        assert!(with.is_empty(), "{with:?}");

        let without = parsed(
            "refuses-bare",
            r#"
description "refuses one rule and says nothing"
supports "fedora"
refuses "grub2_nousb_argument"
"#,
        );
        assert!(
            without
                .iter()
                .any(|m| m.contains("refused without a reason")
                    && m.contains("grub2_nousb_argument")),
            "{without:?}"
        );

        // Two refusals of one rule is a manifest arguing with itself.
        let twice = messages(
            r#"
description "twice"
supports "fedora"
refuses "grub2_nousb_argument" because="one"
refuses "grub2_nousb_argument" because="two"
"#,
        );
        assert!(
            twice.iter().any(|m| m.contains("is refused twice")),
            "{twice:?}"
        );
    }

    /// The index answers for a module whatever family it is built on, so a
    /// gated declaration counts the same as one outside a gate.
    #[test]
    fn a_summary_reads_a_gated_requirement_and_a_gated_claim() {
        let dir = std::env::temp_dir().join(format!("tect-gated-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(layout::MODULE_FILE);
        std::fs::write(
            &file,
            r#"
description "gated"
supports "fedora" "debian"
requires "everywhere"
satisfies { cis-fedora "1.1.1.1" }
family "fedora" {
    requires "on-fedora"
    satisfies { stig "CCI-000199" }
}
"#,
        )
        .unwrap();
        let read = summary(&file);
        assert_eq!(read.requires, ["everywhere", "on-fedora"]);
        assert_eq!(read.satisfies, ["1.1.1.1", "CCI-000199"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Every shape the golden corpus has no broken fixture for.
    #[test]
    fn the_table_catches_what_the_corpus_does_not() {
        let found = messages(
            r#"
description
description "twice"
provides
requires-file "/usr/bin/one" build-only=#true
provides-file "/usr/bin/two" build-only="yes" lifetime="long"
mode "/etc/one"
secret
allow-verify "man-page-missing" unit=1 scope="image"
collects "justfile.inc" into="/etc/one" priority=90000 mode="append"
contributes "justfile.inc" priority=100 mode="append"
fragment "here" position="sideways" standard-layer="no" placement="last"
fragment
packages "one" enablerepo="two" weak=#true
package-groups "one" weak=#true
drives "a truck"
satisfies {
    "" "1.1.1.1"
    cis-fedora
    cis-fedora "1"
    cis-fedora "2"
}
satisfies
family
family "fedora" { }
family "fedora" { supports "debian"; packages }
"#,
        );
        assert_eq!(
            found,
            [
                "`description` needs a string",
                "`description` is declared twice",
                "`provides` needs a capability name",
                "`build-only` is not a `requires-file` property",
                "`build-only` takes #true or #false",
                "unknown `provides-file` property `lifetime`",
                "`mode` needs one path and one octal file mode",
                "`secret` needs a name",
                "`unit` must be a string",
                "unknown `allow-verify` property `scope`",
                "`priority` is a number from 0 to 9999",
                "unknown `collects` property `mode`",
                "unknown `contributes` property `mode`",
                "`fragment` takes no arguments",
                "`position` must be \"before\", \"after\" or \"tail\"",
                "`standard-layer` must be #true or #false",
                "unknown fragment property `placement`",
                "`fragment` is declared twice",
                "unknown `packages` property `weak`",
                "unknown `package-groups` property `weak`",
                "unknown node `drives`",
                "`cis-fedora` has no rules listed",
                "benchmark `cis-fedora` is declared twice",
                "`satisfies` is declared twice",
                "`family` needs at least one family name",
                "`family` gates nothing",
                "`family` gates nothing",
                "`supports` is not gated by family",
                "`packages` needs at least one name",
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn modes_need_a_regular_overlay_file_and_a_standard_layer() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("tect-mode-{}", std::process::id()));
        let module = layout::module(&root, "one");
        let files = module.join(layout::OVERLAY).join("etc");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("regular.conf"), "regular\n").unwrap();
        symlink("regular.conf", files.join("link.conf")).unwrap();
        std::fs::write(module.join("Containerfile.inc"), "RUN true\n").unwrap();

        let mut issues = Issues::default();
        Module::parse(
            "one",
            "one",
            &root,
            r#"
description "mode checks"
supports "fedora"
mode "/etc/link.conf" "0440"
mode "/etc/regular.conf" "0440"
fragment standard-layer=#false
"#
            .to_string(),
            None,
            &mut issues,
        );
        let found = issues.plain();
        assert!(
            found.contains("declares a mode for a file it does not ship"),
            "{found}"
        );
        assert!(
            found.contains("declares `mode \"/etc/regular.conf\"` with no standard layer"),
            "{found}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A package group is the Fedora adapter's verb, so no other family may
    /// declare one at all.
    #[test]
    fn package_groups_are_fedora_only() {
        assert_eq!(
            parsed(
                "groups",
                r#"
description "package groups"
supports "fedora" "debian"
package-groups "kde-desktop"
"#
            ),
            ["`package-groups` is Fedora-only, and this one covers `debian`"]
        );
    }

    /// A COPR is two path segments and nothing else, so anything the RUN line
    /// or the URL could not carry is refused where it is written.
    #[test]
    fn a_copr_needs_two_usable_path_segments() {
        assert_eq!(
            parsed(
                "coprs",
                r#"
description "coprs"
supports "fedora"
copr "noslash"
copr "/project"
copr "owner/"
copr "owner/project/extra"
copr "owner/pro ject"
"#
            ),
            [
                "`noslash` is not a COPR owner/project",
                "`/project` is not a COPR owner/project",
                "`owner/` is not a COPR owner/project",
                "`owner/project/extra` is not a COPR owner/project",
                "`owner/pro ject` is not a COPR owner/project",
            ]
        );
    }

    /// An `enablerepo` naming a COPR is a reference, so the module has to
    /// declare the repository it reaches into.
    #[test]
    fn an_enablerepo_naming_a_copr_needs_the_declaration() {
        assert_eq!(
            parsed(
                "coprs",
                r#"
description "coprs"
supports "fedora"
copr "owner/project"
packages "thing" enablerepo="other/project"
"#
            ),
            ["`enablerepo` names COPR `other/project`, which is not declared here"]
        );
    }

    /// The gate, on the two nodes it exists for. A block the image is not built
    /// for contributes nothing and is still read.
    #[test]
    fn a_gate_is_taken_on_its_families_and_read_on_every_other() {
        let manifest = r#"
description "gated"
supports "fedora" "debian" "ubuntu"
packages "curl"
family "fedora" {
    after "hardened-malloc"
    copr "secureblue/hardened_malloc"
    satisfies { stig "CCI-000199" }
    packages "vim-enhanced" "BAD NAME"
}
family "debian" "ubuntu" { packages "vim" }
"#;
        let read = |family: Option<&str>| {
            let mut issues = Issues::default();
            let module = Module::parse(
                "gated",
                "gated",
                Path::new("."),
                manifest.to_string(),
                family,
                &mut issues,
            )
            .expect("the manifest parses");
            let packages: Vec<String> = module
                .packages
                .iter()
                .map(|batch| format!("{}:{}", batch.family, batch.packages.join(" ")))
                .collect();
            let said: Vec<String> = issues.findings();
            (
                module.after.len(),
                module.coprs.len(),
                module.satisfies.len(),
                packages,
                said,
            )
        };

        // A Fedora build takes the Fedora gate and nothing else.
        let (after, coprs, satisfies, packages, said) = read(Some("fedora"));
        assert_eq!((after, coprs, satisfies), (1, 1, 1));
        assert_eq!(
            packages,
            [
                "fedora:curl",
                "debian:curl",
                "ubuntu:curl",
                "fedora:vim-enhanced",
                "debian:vim",
                "ubuntu:vim",
            ]
        );
        // The gate is read wherever it is built, so the bad name is reported once.
        assert_eq!(
            said,
            ["package name `BAD NAME` has a character that is not allowed"]
        );

        // A Debian build takes neither the edge, the COPR nor the claim, and
        // reports the same thing about a block it does not build.
        let (after, coprs, satisfies, packages, said) = read(Some("debian"));
        assert_eq!((after, coprs, satisfies), (0, 0, 0));
        assert_eq!(
            packages,
            [
                "fedora:curl",
                "debian:curl",
                "ubuntu:curl",
                "fedora:vim-enhanced",
                "debian:vim",
                "ubuntu:vim",
            ]
        );
        assert_eq!(
            said,
            ["package name `BAD NAME` has a character that is not allowed"]
        );

        // A module no image lists is held to all of it and keeps all of it.
        let (after, coprs, satisfies, _, _) = read(None);
        assert_eq!((after, coprs, satisfies), (1, 1, 1));
    }

    /// A gate or a directory naming a family the module does not support is a
    /// block no image can reach, which is a typo far more often than intent.
    #[test]
    fn a_gate_on_an_unsupported_family_is_refused() {
        assert_eq!(
            parsed(
                "narrow",
                r#"
description "narrow"
supports "fedora"
family "debian" { packages "vim" }
family "redhat" { packages "vim" }
"#
            ),
            [
                "unknown base family `redhat`",
                "`narrow` gates on `debian` and does not support it",
            ]
        );
    }

    /// `supports` and `packages` walk every family the tool recognises, and
    /// `enablerepo` stays Fedora-only across all three -- both for a batch that
    /// spread over `supports` and for one a gate placed.
    #[test]
    fn debian_and_ubuntu_are_known_families_and_enablerepo_stays_fedora_only() {
        assert_eq!(
            parsed(
                "known",
                r#"
description "known families and packages"
supports "fedora" "debian" "ubuntu"
packages "curl" enablerepo="rpmfusion"
family "debian" "ubuntu" { packages "curl" enablerepo="backports" }
"#
            ),
            [
                "`enablerepo` is Fedora-only, and this batch covers `debian`",
                "`enablerepo` is Fedora-only, and this batch covers `debian`",
            ]
        );
    }

    /// The three nodes a module may repeat under one name, and what they hold.
    #[test]
    fn the_table_holds_the_option_variant_and_asset_grammars() {
        let found = messages(
            r#"
option "fonts" type="list" scope="image" {
    default "A"
    default "B"
    describe "the fonts"
}
option "fonts" type="list"
variant "lean" scope="image" {
    set
}
variant "lean"
asset "starship" tracked=#true {
    pin {
        version
        sha256 "abc" from="upstream" algorithm="sha512"
    }
    checksum "abc"
}
asset "starship"
"#,
        );
        assert_eq!(
            found,
            [
                "unknown option property `scope`",
                "`default` is declared twice",
                "unknown node `describe` in an option",
                "option `fonts` is declared twice",
                "unknown variant property `scope`",
                "`set` needs an option name",
                "variant `lean` is declared twice",
                "unknown asset property `tracked`",
                "`version` needs a value",
                "`from` must be asset, sidecar or manual",
                "unknown sha256 property `algorithm`",
                "unknown node `checksum` in an asset",
                "asset `starship` is declared twice",
            ]
        );
    }
}

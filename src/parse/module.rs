//! module.kdl: the module author's file.

use crate::diag::{Issue, Issues, Source, Span};
use crate::layout;
use crate::model::image::{Entry, Image, List};
use crate::model::module::{
    Collect, Contribution, Coverage, Decl, FileMode, Key, Module, PackageGroup, VerifyException,
};
use crate::model::remote::REMOTE_DIR;
use crate::parse::disk::Disk;
use crate::parse::prop_span;
use crate::parse::schema::{check_doc, Arg, Kind, Node, Prop, Say};
use crate::parse::{asset, boolean, check_capability, child, flag, int_prop, options, prop};
use crate::parse::{string_arg, string_args, syntax_issue};
use crate::resolve::options as resolve_options;
use crate::runtime::{class_names, VERIFY_CLASSES};
use kdl::{KdlDocument, KdlNode};
use std::collections::BTreeSet;
use std::path::Path;

/// The base families this repository knows how to build on.
const FAMILIES: [&str; 3] = ["fedora", "debian", "ubuntu"];

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
const GENERATORS: [&str; 2] = ["cosign", "openssl"];

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
                "the generators are `cosign` and `openssl`; a manifest picks one of them rather \
                 than naming a command of its own"))
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
        Node::new("requires", "A capability another module has to provide, which also orders the \
             build.")
            .arg(Arg::Strs, Say::new("`{}` needs a capability name", "nothing named", "")),
        Node::new("after", "A module this one builds after without requiring anything of it.")
            .arg(Arg::Strs, Say::new("`{}` needs a capability name", "nothing named", "")),

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
            .arg(Arg::Strs, Say::NONE)
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
            "One `tect validate-image` diagnostic accepted on one unit rather than image-wide.")
            .arg(Arg::Str, Say::NONE)
            .props(&[
                Prop { name: "unit", kind: Kind::Str,
                    desc: "The unit the exception applies to.",
                    say: Say::new("`unit` must be a string", "not a string", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `allow-verify` property `{}`", "not part of the schema",
                "`allow-verify` accepts `unit`")),

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
                Prop { name: "position", kind: Kind::One(&["before", "after"]),
                    desc: "Whether the fragment goes above or below the generated block.",
                    say: Say::new("`position` must be \"before\" or \"after\"", "not a position",
                        "before, the default, puts the fragment above the generated block; after \
                         puts it below"),
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

        Node::new("packages", "The packages this module installs, listed per base family.")
            .children(&[
                Node::new("", "One base family, and the packages to install on it.")
                    .arg(Arg::Strs, Say::NONE)
                    .props(&[
                        Prop { name: "enablerepo", kind: Kind::Str,
                            desc: "A repository enabled for this install and disabled otherwise. \
                                   Fedora only.",
                            say: Say::NONE,
                            missing: Say::NONE },
                    ], Say::new("unknown property `{}` in packages block", "not part of the schema",
                        "a family entry in `packages` accepts `enablerepo`")),
            ], Say::NONE),
        Node::new("satisfies", "The benchmarks and rules this module claims to harden, as an audit declaration the tool records rather than certifies.")
            .once("a module makes one claim set; two blocks split it")
            .children(&[
                Node::new("", "One benchmark, and the rule IDs it covers.")
                    .arg(Arg::Strs, Say::new("`{}` has no rules listed", "nothing to cover", ""))
                    .unique(Say::new("benchmark `{}` is declared twice", "already declared above", "")),
            ], Say::NONE),
    ], Say::new("unknown node `{}`", "not part of the schema",
        "docs/schema.md documents every node a manifest may hold"));

/// A declared `priority=`, which the schema table holds to its range. `None`
/// where one is declared is what the table already reported.
fn priority(node: &KdlNode) -> Option<u32> {
    int_prop(node, "priority").map(|value| value as u32)
}

/// Whether a `priority=` is there but not a number the schema accepts, which is
/// its diagnostic rather than a second one here.
fn bad_priority(node: &KdlNode) -> bool {
    prop_span(node, "priority").is_some() && priority(node).is_none()
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

        let mut module = Self::parse(&entry.path, &dir_rel, root, text, issues)?;
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
            provides_files: Vec::new(),
            provides_files_build_only: Vec::new(),
            requires_files: Vec::new(),
            overrides: Vec::new(),
            verify_exceptions: Vec::new(),
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
            helpers: Vec::new(),
            satisfies: Vec::new(),
            resolved: Vec::new(),
            fragment: std::fs::read_to_string(dir.join("Containerfile.inc")).ok(),
            fragment_after: false,
            standard_layer: true,
            content: crate::provenance::record::hash(&dir),
            imported: crate::provenance::record::read(&dir, issues),
            repo: dir.join("repo").is_file(),
            src: src.clone(),
        };

        check_doc(&doc, &MODULE, src, issues);

        let mut fragment_seen = false;
        for node in doc.nodes() {
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
                "mode" => module.parse_mode(node, &dir, src, issues),
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
                "helpers" => module.parse_helpers(node, &dir, src, issues),
                "allow-verify" => module.parse_allow_verify(node, src, issues),
                "collects" => module.parse_collects(node, src, issues),
                "contributes" => module.parse_contributes(node, &dir, src, issues),
                "fragment" => {
                    if fragment_seen {
                        continue;
                    }
                    fragment_seen = true;
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
                "packages" => module.parse_packages(node, src, issues),
                "satisfies" => module.parse_satisfies(node, src, issues),
                _ => {}
            }
        }

        // The public half is a contract path, derived rather than declared a
        // second line down.
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
                    .help("`standard-layer #false` makes the fragment the whole layer, so it has to spell out its own mounts, args and env; drop one or the other"),
                );
            }
        }

        if module.supports.is_empty() {
            issues.push(
                Issue::new(format!("`{}` declares no `supports`", path), src)
                    .help("a module has to say which base families it can build on, so a portability gap surfaces at lint rather than mid-build"),
            );
        }

        if dir.join("repo").is_file() {
            for group in &module.packages {
                issues.push(
                    Issue::new(
                        format!("`{}` declares both a `repo` file and `packages`", path),
                        src,
                    )
                    .at(group.span, "installed before the repo file is sourced")
                    .help("the generated build script sources `repo` after installing these, so call the family's package manager in module.sh instead"),
                );
            }
        }

        crate::provenance::check_fetch(&module, &dir, issues);

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
            issues.push(
                Issue::new("`mode` needs one path and one octal file mode", src)
                    .at(node.name().span(), "incomplete")
                    .help("`mode \"/etc/example.conf\" \"0644\"`"),
            );
            return;
        }
        let Some(path) = args[0].value().as_string() else {
            issues.push(
                Issue::new("a mode path has to be a string", src)
                    .at(args[0].span(), "not a string"),
            );
            return;
        };
        let Some(given) = args[1].value().as_string() else {
            issues.push(
                Issue::new("a file mode has to be an octal string", src)
                    .at(args[1].span(), "not a string")
                    .help("quote it: `\"0440\"`"),
            );
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

        let overlay = dir.join(layout::OVERLAY).join(path.trim_start_matches('/'));
        if overlay.symlink_metadata().is_err() || overlay.is_dir() {
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
        });
    }

    /// `allow-verify "man-page-missing" unit="x.service"` One known diagnostic
    /// accepted on one unit, which is why both halves are required.
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
                         which accepts one diagnostic on one unit rather than image-wide",
                    ),
            );
            return;
        };

        if !VERIFY_CLASSES.iter().any(|(name, _)| *name == class) {
            issues.push(
                Issue::new(format!("`{class}` is not a verify diagnostic class"), src)
                    .at(span, "not one of the known classes")
                    .help(format!(
                        "known classes: {}. They are named rather than written as patterns, and `tect validate-image` holds what each one stands for",
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
                if !dir.join(&contributed).is_file() {
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
        let position = prop(node, "position").filter(|p| matches!(*p, "before" | "after"));
        self.fragment_after = position == Some("after");
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

    /// `packages { fedora "pkg1" "pkg2" }` Each child node names a base family
    /// and carries the package names as positional arguments.
    fn parse_packages(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let Some(children) = node.children() else {
            return;
        };
        for child in children.nodes() {
            let family = child.name().value().to_string();
            if family.is_empty() {
                issues.push(
                    Issue::new("a family name is required inside `packages`", src)
                        .at(child.name().span(), "empty name")
                        .help("`packages { fedora \"pkg1\" \"pkg2\" }`"),
                );
                continue;
            }
            if !FAMILIES.contains(&family.as_str()) {
                issues.push(
                    Issue::new(format!("unknown base family `{family}`"), src)
                        .at(
                            child.name().span(),
                            "not a family this repository builds on",
                        )
                        .help(format!("known families: {}", FAMILIES.join(", "))),
                );
                continue;
            }
            let mut packages: Vec<String> = Vec::new();
            for arg in child.entries().iter().filter(|e| e.name().is_none()) {
                let Some(value) = arg.value().as_string() else {
                    issues.push(
                        Issue::new("a package name has to be a string", src)
                            .at(arg.span(), "not a string")
                            .help("quote it: `fedora \"7zip\"`"),
                    );
                    continue;
                };
                if let Some(problem) = bad_token(value) {
                    issues.push(
                        Issue::new(format!("package name `{value}` {problem}"), src)
                            .at(arg.span(), "would not survive the RUN line")
                            .help(TOKEN_HELP),
                    );
                    continue;
                }
                packages.push(value.to_string());
            }
            if packages.is_empty() {
                issues.push(
                    Issue::new(format!("`{family}` has no packages listed"), src)
                        .at(child.name().span(), "nothing to install"),
                );
                continue;
            }
            let mut enablerepo: Option<String> = None;
            if let Some(span) = prop_span(child, "enablerepo") {
                if family != "fedora" {
                    issues.push(
                        Issue::new(format!("`enablerepo` is Fedora-only, not `{family}`"), src)
                            .at(span, "no repo to enable on this family")
                            .help("only Fedora groups take `enablerepo`; a Debian or Ubuntu group installs from the base image's configured sources"),
                    );
                } else if let Some(repo) = prop(child, "enablerepo").filter(|v| !v.is_empty()) {
                    match bad_token(repo) {
                        Some(problem) => issues.push(
                            Issue::new(format!("repo ID `{repo}` {problem}"), src)
                                .at(span, "would not survive the RUN line")
                                .help(TOKEN_HELP),
                        ),
                        None => enablerepo = Some(repo.to_string()),
                    }
                } else {
                    issues.push(
                        Issue::new("`enablerepo` needs a repo ID string", src)
                            .at(span, "not a string"),
                    );
                }
            }
            self.packages.push(PackageGroup {
                family,
                packages,
                enablerepo,
                span: child.name().span().into(),
            });
        }
    }

    /// `satisfies { cis-fedora "1.1.1.1" }` Each child names a benchmark and
    /// carries the rule IDs this module claims to cover.
    fn parse_satisfies(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let Some(children) = node.children() else {
            return;
        };
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
            let rules = child
                .entries()
                .iter()
                .filter(|entry| entry.name().is_none())
                .filter_map(|entry| entry.value().as_string().map(str::to_string))
                .collect();
            self.satisfies.push(Coverage {
                benchmark,
                rules,
                span: child.name().span().into(),
            });
        }
    }
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
                .help("the private half is written under keys/private/ and never committed, so it is a plain name rather than a path"),
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
    /// since a number resolves against the content rather than against the
    /// benchmark it was written under.
    pub satisfies: Vec<String>,
}

pub fn summary(file: &Path) -> Summary {
    let Some(doc) = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| text.parse::<KdlDocument>().ok())
    else {
        return Summary::default();
    };
    let strings = |name: &str| -> Vec<String> {
        doc.nodes()
            .iter()
            .filter(|node| node.name().value() == name)
            .flat_map(KdlNode::entries)
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
        satisfies: doc
            .nodes()
            .iter()
            .filter(|node| node.name().value() == "satisfies")
            .filter_map(KdlNode::children)
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
            Module::parse(dir, dir, root, text, issues);
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
        issues
            .plain()
            .lines()
            .filter_map(|line| line.strip_prefix("  x "))
            .map(str::to_string)
            .collect()
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
secret
allow-verify "man-page-missing" unit=1 scope="image"
collects "justfile.inc" into="/etc/one" priority=90000 mode="append"
contributes "justfile.inc" priority=100 mode="append"
fragment "here" position="sideways" standard-layer="no" placement="last"
fragment
packages {
    fedora "one" enablerepo="two" weak=#true
}
drives "a truck"
satisfies {
    "" "1.1.1.1"
    cis-fedora
    cis-fedora "1"
    cis-fedora "2"
}
satisfies
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
                "`secret` needs a name",
                "`unit` must be a string",
                "unknown `allow-verify` property `scope`",
                "`priority` is a number from 0 to 9999",
                "unknown `collects` property `mode`",
                "unknown `contributes` property `mode`",
                "`fragment` takes no arguments",
                "`position` must be \"before\" or \"after\"",
                "`standard-layer` must be #true or #false",
                "unknown fragment property `placement`",
                "`fragment` is declared twice",
                "unknown property `weak` in packages block",
                "unknown node `drives`",
                "`cis-fedora` has no rules listed",
                "benchmark `cis-fedora` is declared twice",
                "`satisfies` is declared twice",
            ]
        );
    }

    /// `supports` and `packages` walk every family the tool recognises, and
    /// `enablerepo` stays Fedora-only across all three.
    #[test]
    fn debian_and_ubuntu_are_known_families_and_enablerepo_stays_fedora_only() {
        let mut issues = Issues::default();
        Module::parse(
            "known",
            "known",
            Path::new("."),
            r#"
description "known families and packages"
supports "fedora" "debian" "ubuntu"
packages {
    fedora "curl" enablerepo="rpmfusion"
    debian "curl" enablerepo="backports"
    ubuntu "curl" enablerepo="backports"
}
"#
            .to_string(),
            &mut issues,
        );
        assert_eq!(
            issues
                .plain()
                .lines()
                .filter_map(|line| line.strip_prefix("  x "))
                .collect::<Vec<_>>(),
            [
                "`enablerepo` is Fedora-only, not `debian`",
                "`enablerepo` is Fedora-only, not `ubuntu`",
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

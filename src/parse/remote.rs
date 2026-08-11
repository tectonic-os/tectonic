//! `source`, the out-of-tree module pin on a list entry, and `sources`, the
//! collections an import resolves against.

use crate::diag::{Issue, Issues, Source, Span};
use crate::model::image::is_name;
use crate::model::remote::{At, Collection, Remote};
use crate::parse::asset::{MANUAL, RENOVATE};
use crate::parse::schema::{Arg, Node, Say, NEEDS_VALUE};
use crate::parse::{check_sha256, kids, placeholders, string_arg};
use kdl::KdlNode;

/// The archives the fetch can extract.
const ARCHIVES: [&str; 5] = [".tar.gz", ".tgz", ".tar.xz", ".tar.zst", ".tar.bz2"];

/// What every pin declares: the ref it is taken at, and the hash it is held to.
const REF: Node = Node::new("ref", "The tag or commit the archive is fetched at.")
    .arg(Arg::Str, NEEDS_VALUE)
    .once("");
const SHA256: Node = Node::new("sha256", "What the fetched archive is verified against.")
    .arg(Arg::Str, NEEDS_VALUE)
    .once("");

/// The pin on a list entry, which is the same shape as an `asset`.
#[rustfmt::skip]
pub const SOURCE: Node = Node::new("source",
    "Where a module that lives outside this repository is fetched from, and what pins it.")
    .arg(Arg::Str, Say::new("`{}` needs a URL", "no URL given",
        "`source \"https://host/owner/repo/archive/{ref}.tar.gz\" { ... }`"))
    .props(&[], Say::new("unknown source property `{}`", "not part of the schema",
        "a source carries its fields as child nodes, not properties"))
    .children(&[
        RENOVATE,
        MANUAL,
        REF,
        SHA256,
        Node::new("path", "The module's directory inside the archive.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
    ], Say::new("unknown node `{}` in a source", "not part of the schema",
        "a source holds `renovate` or `manual`, `ref`, `sha256` and `path`"));

/// One collection in the registry, named by the owner rather than by the
/// schema, which is why the node's name is empty.
#[rustfmt::skip]
pub const COLLECTION: Node = Node::new("",
    "One module collection, named by the owner its modules land under in modules/.")
    .arg(Arg::Str, Say::new("`{}` says nothing about where the collection is",
        "no location given",
        "a directory on this machine, `{} \"../modules\"`, or a pinned archive, \
         `{} \"https://host/owner/repo/archive/{ref}.tar.gz\" { ... }`"))
    .props(&[], Say::new("unknown collection property `{}`", "not part of the schema",
        "a collection carries its fields as child nodes, not properties"))
    .children(&[
        RENOVATE,
        MANUAL,
        REF,
        SHA256,
        Node::new("path", "The directory inside the archive the modules sit in, when they are \
                           not at its root.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
    ], Say::new("unknown node `{}` in a collection", "not part of the schema",
        "a collection holds `renovate` or `manual`, `ref`, `sha256` and `path`"));

fn datasource(node: &KdlNode) -> Option<&str> {
    node.entries()
        .iter()
        .find(|e| e.name().map(|n| n.value()) == Some("datasource"))
        .and_then(|e| e.value().as_string())
}

/// `source "https://host/owner/repo/archive/{ref}.tar.gz" { renovate ...; ref
/// "..."; sha256 "..."; path "modules/name" }` Exactly one of `renovate` and
/// `manual`, and the tracked line directly below `renovate`, since one regex
/// matches the two together.
pub fn parse(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Remote> {
    let span: Span = node.name().span().into();
    let url = string_arg(node)?.to_string();

    let mut remote = Remote {
        url,
        git_ref: String::new(),
        sha256: String::new(),
        path: None,
        span,
    };

    let mut manual: Option<Span> = None;
    let mut renovate: Option<Span> = None;
    let mut ref_span: Option<Span> = None;
    let mut previous: Option<&str> = None;

    for child in kids(node) {
        let kind = child.name().value();
        let child_span: Span = child.name().span().into();
        let value = string_arg(child)
            .filter(|v| !v.is_empty())
            .map(str::to_string);

        match kind {
            "renovate" => {
                renovate = Some(child_span);
                if datasource(child) == Some("git-refs") {
                    issues.push(
                        Issue::new("`git-refs` does not track a module pin", src)
                            .at(child_span, "no custom manager matches it")
                            .help("github-tags or github-releases, against the publishing repository's per-module tags; a repository with no tags is pinned `manual`"),
                    );
                }
            }
            "manual" => manual = Some(child_span),
            "ref" => {
                ref_span = Some(child_span);
                remote.git_ref = value.unwrap_or_default();
                if renovate.is_some() && previous != Some("renovate") {
                    issues.push(
                        Issue::new("something sits between `renovate` and `ref`", src)
                            .at(child_span, "has to be the line directly below `renovate`")
                            .help("Renovate matches the two together, so anything between them stops the pin being tracked, silently"),
                    );
                }
            }
            "sha256" => remote.sha256 = value.unwrap_or_default(),
            "path" => remote.path = value,
            _ => {}
        }
        previous = Some(kind);
    }

    match (renovate, manual) {
        (Some(_), Some(manual)) => issues.push(
            Issue::new("the pin is declared both tracked and manual", src)
                .at(manual, "pick one")
                .help("`renovate` says Renovate bumps this ref, `manual` says nothing does and why"),
        ),
        (None, None) => issues.push(
            Issue::new("the pin says nothing about how it is kept current", src)
                .at(span, "needs `renovate` or `manual`")
                .help("`renovate datasource=\"github-tags\" depName=\"owner/repo\"`, or `manual \"why nothing tracks it\"`"),
        ),
        _ => {}
    }

    if ref_span.is_none() {
        issues.push(
            Issue::new("the pin declares no `ref`", src)
                .at(span, "nothing pinned")
                .help("an exact tag or commit; a moving ref would make the build depend on when it ran"),
        );
    }

    if remote.sha256.is_empty() {
        issues.push(
            Issue::new("the pin declares no `sha256`", src)
                .at(span, "nothing to verify the archive against")
                .help("a remote module is arbitrary shell running as root in the build, so the content hash is required, not optional"),
        );
    } else {
        check_sha256(&remote.sha256, "the pin", span, src, issues);
    }

    check_url(&remote, renovate.is_some(), src, issues);
    check_path(&remote, src, issues);

    Some(remote)
}

/// `sources { tectonic-os "https://host/owner/modules/archive/{ref}.tar.gz" {
/// ... }; scratch "../modules" }` The node's name is the owner. A location that
/// is not a URL is a directory on this machine, which is read where it is, so
/// nothing is fetched and there is nothing to pin or hash.
pub fn parse_collection(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Collection> {
    let name = node.name().value().to_string();
    let span: Span = node.name().span().into();
    let at = string_arg(node)?;

    if !is_name(&name) {
        issues.push(
            Issue::new(format!("invalid collection name `{name}`"), src)
                .at(span, "lowercase, digits and dashes, starting with a letter")
                .help("the name is the directory imports land in, `modules/<name>/<module>`, and reaches every image that lists one of them"),
        );
        return None;
    }

    if at.starts_with("https://") || at.starts_with("file://") {
        let remote = parse(node, src, issues)?;
        return Some(Collection {
            name,
            at: At::Archive(remote),
            span,
        });
    }

    for child in kids(node) {
        let kind = child.name().value();
        if matches!(kind, "renovate" | "manual" | "ref" | "sha256") {
            issues.push(
                Issue::new(format!("`{name}` is a directory, so `{kind}` says nothing"), src)
                    .at(child.name().span(), "nothing is fetched")
                    .help("a collection on this machine is read where it is; a pin belongs on one that is downloaded"),
            );
        }
    }

    Some(Collection {
        name,
        at: At::Dir(at.to_string()),
        span,
    })
}

fn check_url(remote: &Remote, tracked: bool, src: &Source, issues: &mut Issues) {
    let url = &remote.url;
    if !url.starts_with("https://") && !url.starts_with("file://") {
        issues.push(
            Issue::new("the source URL is not https", src)
                .at(remote.span, "unencrypted or unsupported scheme")
                .help("https://, or file:// for a local archive"),
        );
    }

    for placeholder in placeholders(url).filter(|found| *found != "{ref}") {
        issues.push(
            Issue::new(
                format!("the source URL has an unknown placeholder {placeholder}"),
                src,
            )
            .at(remote.span, "not substituted")
            .help("a source URL expands `{ref}` and nothing else"),
        );
    }

    if tracked && !url.contains("{ref}") {
        issues.push(
            Issue::new("the source URL does not expand `{ref}`", src)
                .at(remote.span, "a bump would fetch the same archive")
                .help("put `{ref}` in the URL, or mark the pin `manual` if the URL genuinely does not follow the ref"),
        );
    }

    let resolved = remote.url_resolved();
    if !ARCHIVES.iter().any(|ext| resolved.ends_with(ext)) {
        issues.push(
            Issue::new("the source is not a tar archive", src)
                .at(remote.span, "cannot be extracted")
                .help(format!(
                    "one of {}; the fetch strips the archive's leading directory, which is a tar option",
                    ARCHIVES.join(", ")
                )),
        );
    }

    for (what, value) in [("URL", resolved.as_str()), ("ref", remote.git_ref.as_str())] {
        if value.contains(['|', '"', '\'', '\\', '$', '`', ' ', '\n', '\t']) {
            issues.push(
                Issue::new(format!("the {what} contains a shell metacharacter"), src)
                    .at(remote.span, "not usable in the fetch")
                    .help("pins are passed to the fetch as pipe-separated fields, so quotes, spaces, pipes and expansions are rejected"),
            );
        }
    }
}

fn check_path(remote: &Remote, src: &Source, issues: &mut Issues) {
    let Some(path) = &remote.path else {
        return;
    };
    let bad = if path.starts_with('/') {
        Some("relative to the archive root, so it cannot start with /")
    } else if path.split('/').any(|seg| seg == ".." || seg.is_empty()) {
        Some("no empty or `..` segments: the subtree has to stay inside the archive")
    } else if path.contains(['|', '"', '\'', '\\', '$', '`', ' ', '\n', '\t']) {
        Some("not usable in the fetch")
    } else {
        None
    };
    if let Some(reason) = bad {
        issues.push(
            Issue::new(format!("invalid subtree path `{path}`"), src)
                .at(remote.span, reason)
                .help(
                    "`path \"modules/module-name\"`, the module's directory inside the repository",
                ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::schema::check;
    use kdl::KdlDocument;

    fn messages(text: &str) -> Vec<String> {
        let doc: KdlDocument = text.parse().expect("valid KDL");
        let src = Source::new("image.kdl", text);
        let mut issues = Issues::default();
        check(&doc.nodes()[0], &SOURCE, &src, &mut issues);
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
source pin="tag" {
    renovate "now" datasource="crates" flavour="x"
    manual
    ref
    sha256 "abc"
    sha256 "def"
    subtree "modules/x"
}
"#,
        );
        assert_eq!(
            found,
            [
                "`source` needs a URL",
                "unknown source property `pin`",
                "`renovate` takes no arguments",
                "unsupported datasource `crates`",
                "unknown renovate property `flavour`",
                "`renovate` declares no depName",
                "`manual` needs a reason",
                "`ref` needs a value",
                "`sha256` is declared twice",
                "unknown node `subtree` in a source",
            ]
        );
    }

    /// The annotation both this and an `asset` point at.
    #[test]
    fn a_renovate_annotation_says_what_tracks_the_pin() {
        let found = messages("source \"https://host/a.tar.gz\" { renovate }");
        assert_eq!(
            found,
            [
                "`renovate` declares no datasource",
                "`renovate` declares no depName",
            ]
        );
    }
}

//! Out-of-tree modules: an exact pin, fetched at generate time.

use crate::asset::check_renovate;
use crate::diag::{Issue, Issues, Source, Span};
use kdl::KdlNode;

/// Where fetched module trees land, relative to `modules/`.
pub const REMOTE_DIR: &str = ".remote";

/// The archives the fetch can extract.
const ARCHIVES: [&str; 5] = [".tar.gz", ".tgz", ".tar.xz", ".tar.zst", ".tar.bz2"];

pub struct Remote {
    /// Unexpanded, `{ref}` included, because this is what a reviewer reads and
    /// what the checksum workflow rewrites around.
    pub url: String,
    pub git_ref: String,
    pub sha256: String,
    /// The module's directory inside the archive, relative to its root once
    /// the leading directory is stripped.
    pub path: Option<String>,
    pub span: Span,
}

impl Remote {
    /// The URL the fetch actually requests.
    pub fn url_resolved(&self) -> String {
        self.url.replace("{ref}", &self.git_ref)
    }
}

/// The first unnamed entry of a node, as a string.
fn string_arg(node: &KdlNode) -> Option<&str> {
    node.entries()
        .iter()
        .find(|e| e.name().is_none())
        .and_then(|e| e.value().as_string())
}

fn datasource(node: &KdlNode) -> Option<&str> {
    node.entries()
        .iter()
        .find(|e| e.name().map(|n| n.value()) == Some("datasource"))
        .and_then(|e| e.value().as_string())
}

/// `source "https://host/owner/repo/archive/{ref}.tar.gz" { renovate ...; ref
/// "..."; sha256 "..."; path "modules/name" }` The same shape as an `asset`
/// block: exactly one of `renovate` and `manual`, and the tracked line
/// directly below `renovate`, since one regex matches the two together.
pub fn parse(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Remote> {
    let span: Span = node.name().span().into();
    let Some(url) = string_arg(node).map(str::to_string) else {
        issues.push(
            Issue::new("`source` needs a URL", src)
                .at(span, "no URL given")
                .help("`source \"https://host/owner/repo/archive/{ref}.tar.gz\" { ... }`"),
        );
        return None;
    };

    for entry in node.entries().iter().filter(|e| e.name().is_some()) {
        issues.push(
            Issue::new(
                format!(
                    "unknown source property `{}`",
                    entry.name().map(|n| n.value()).unwrap_or_default()
                ),
                src,
            )
            .at(entry.span(), "not part of the schema")
            .help("a source carries its fields as child nodes, not properties"),
        );
    }

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

    for child in node.children().map(|c| c.nodes()).unwrap_or_default() {
        let kind = child.name().value();
        let child_span: Span = child.name().span().into();
        let string = |issues: &mut Issues| match string_arg(child) {
            Some(v) if !v.is_empty() => Some(v.to_string()),
            _ => {
                issues.push(
                    Issue::new(format!("`{kind}` needs a value"), src)
                        .at(child_span, "nothing given"),
                );
                None
            }
        };

        match kind {
            "renovate" => {
                renovate = Some(child_span);
                check_renovate(child, src, issues);
                if datasource(child) == Some("git-refs") {
                    issues.push(
                        Issue::new("`git-refs` does not track a module pin", src)
                            .at(child_span, "no custom manager matches it")
                            .help("github-tags or github-releases, against the publishing repository's per-module tags; a repository with no tags is pinned `manual`"),
                    );
                }
            }
            "manual" => {
                manual = Some(child_span);
                if string_arg(child).unwrap_or_default().is_empty() {
                    issues.push(
                        Issue::new("`manual` needs a reason", src)
                            .at(child_span, "no reason given")
                            .help("say why nothing tracks this pin, or the next reader takes the absence for an oversight"),
                    );
                }
            }
            "ref" => {
                ref_span = Some(child_span);
                remote.git_ref = string(issues).unwrap_or_default();
                if renovate.is_some() && previous != Some("renovate") {
                    issues.push(
                        Issue::new("something sits between `renovate` and `ref`", src)
                            .at(child_span, "has to be the line directly below `renovate`")
                            .help("Renovate matches the two together, so anything between them stops the pin being tracked, silently"),
                    );
                }
            }
            "sha256" => remote.sha256 = string(issues).unwrap_or_default(),
            "path" => remote.path = string(issues),
            other => issues.push(
                Issue::new(format!("unknown node `{other}` in a source"), src)
                    .at(child_span, "not part of the schema")
                    .help("a source holds `renovate` or `manual`, `ref`, `sha256` and `path`"),
            ),
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
    } else if remote.sha256.len() != 64 || !remote.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        issues.push(
            Issue::new("the pin has a malformed sha256", src)
                .at(span, "not 64 hex digits")
                .help("sha256sum output, lowercase"),
        );
    } else if remote.sha256.chars().any(|c| c.is_ascii_uppercase()) {
        issues.push(
            Issue::new("the pin has an uppercase sha256", src)
                .at(span, "lowercase, as sha256sum writes it")
                .help("the checksum workflow rewrites this line by matching the pinned value, so its case has to be the one sha256sum produces"),
        );
    }

    check_url(&remote, renovate.is_some(), src, issues);
    check_path(&remote, src, issues);

    Some(remote)
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

    for (index, _) in url.match_indices('{') {
        let placeholder = url[index..]
            .find('}')
            .map(|end| &url[index..=index + end])
            .unwrap_or(&url[index..]);
        if placeholder != "{ref}" {
            issues.push(
                Issue::new(
                    format!("the source URL has an unknown placeholder {placeholder}"),
                    src,
                )
                .at(remote.span, "not substituted")
                .help("a source URL expands `{ref}` and nothing else"),
            );
        }
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
                .help("`path \"modules/module-name\"`, the module's directory inside the repository"),
        );
    }
}

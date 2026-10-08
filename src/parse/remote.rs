//! Typed sources for module and base-image libraries.

use crate::diag::{Issue, Issues, Source, Span};
use crate::model::image::is_name;
use crate::model::remote::{At, Collection, Kind as SourceKind};
use crate::parse::schema::{Arg, Node, Say};
use crate::parse::{check_sha256, kids, string_arg};
use crate::provenance::{Evidence, ShaFrom, Tracker};
use kdl::KdlNode;

const DIR: Node = Node::new("dir", "The local directory that holds this source.")
    .example("\"../library/modules\"")
    .arg(
        Arg::Str,
        Say::new(
            "`dir` needs a path",
            "not a string",
            "`dir \"../library/modules\"`",
        ),
    )
    .once("");

const URL: Node = Node::new("url", "The HTTPS Git repository that holds this source.")
    .example("\"https://github.com/tectonic-os/library\"")
    .arg(
        Arg::Str,
        Say::new(
            "`url` needs a repository URL",
            "not a string",
            "`url \"https://github.com/owner/repository\"`",
        ),
    )
    .once("");

/// One `path` per source kind, so each example shows the directory that kind
/// is read from.
const fn path(desc: &'static str, example: &'static str) -> Node {
    Node::new("path", desc)
        .example(example)
        .arg(
            Arg::Str,
            Say::new(
                "`path` needs a relative directory",
                "not a string",
                "a directory inside the repository, such as the one `path` is documented with",
            ),
        )
        .once("")
}

const MODULES_PATH: Node = path(
    "The directory inside the Git repository that holds this module library.",
    "\"modules\"",
);
const BASE_IMAGES_PATH: Node = path(
    "The directory inside the Git repository that holds this base-image library.",
    "\"base-images/core\"",
);
const CAPABILITIES_PATH: Node = path(
    "The directory inside the Git repository that holds this capability library.",
    "\"capabilities\"",
);

const VERSION: Node = Node::new(
    "version",
    "The branch, tag or commit that selects the repository tree.",
)
.example("\"v1\"")
.arg(
    Arg::Str,
    Say::new(
        "`version` needs a Git ref",
        "not a string",
        "`version \"v1\"`",
    ),
)
.once("");

const SHA256: Node = Node::new(
    "sha256",
    "The hash of the canonical tar archive for the selected commit.",
)
.example("\"b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3\"")
.arg(
    Arg::Str,
    Say::new(
        "`sha256` needs a hash",
        "not a string",
        "the 64 lowercase hex digits from `sha256sum` over `git archive --format=tar`",
    ),
)
.once("");

const UNPINNED: Node = Node::new(
    "unpinned",
    "Why this source follows a ref without a verified archive hash.",
)
.example("\"This repository follows the library's default ref\"")
.arg(
    Arg::Str,
    Say::new(
        "`unpinned` needs a reason",
        "not a string",
        "`unpinned \"why the moving ref is trusted here\"`",
    ),
)
.once("");

const fn source(
    kind: &'static str,
    what: &'static str,
    example: &'static str,
    fields: &'static [Node],
) -> Node {
    Node::new(kind, what)
        .example(example)
        .arg(
            Arg::Str,
            Say::new(
                "a source needs an alias",
                "not a string",
                "name the source after the library it reads",
            ),
        )
        .props(
            &[],
            Say::new(
                "unknown source property `{}`",
                "not part of the schema",
                "a source carries its fields as child nodes, not properties",
            ),
        )
        .children(
            fields,
            Say::new(
                "unknown node `{}` in a source",
                "not part of the schema",
                "a source accepts `dir`, or `url` with optional `path`, `version`, `sha256` and `unpinned`",
            ),
        )
}

pub const MODULES: Node = source(
    "modules",
    "A module library that imports and copies resolve against.",
    "\"tectonic-modules\"",
    &[DIR, URL, MODULES_PATH, VERSION, SHA256, UNPINNED],
)
.scaffolds(&["url", "path"]);

pub const BASE_IMAGES: Node = source(
    "base-images",
    "A base-image library that the base catalog is read out of.",
    "\"core\"",
    &[DIR, URL, BASE_IMAGES_PATH, VERSION, SHA256, UNPINNED],
)
.scaffolds(&["url", "path"]);

pub const CAPABILITIES: Node = source(
    "capabilities",
    "A capability library that locates capability witness paths.",
    "\"core\"",
    &[DIR, URL, CAPABILITIES_PATH, VERSION, SHA256, UNPINNED],
)
.scaffolds(&["url", "path"]);

pub const SOURCES: [Node; 3] = [MODULES, BASE_IMAGES, CAPABILITIES];

fn value<'a>(node: &'a KdlNode, name: &str) -> Option<(&'a str, Span)> {
    kids(node)
        .iter()
        .find(|child| child.name().value() == name)
        .and_then(|child| {
            string_arg(child)
                .filter(|value| !value.is_empty())
                .map(|value| (value, child.name().span().into()))
        })
}

fn valid_subtree(path: &str) -> bool {
    !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn parse_source(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Collection> {
    let kind = SourceKind::parse(node.name().value())?;
    let span: Span = node.name().span().into();
    // The alias is the argument, so a diagnostic about it points there and not
    // at the kind that every source shares.
    let name_span: Span = node
        .entries()
        .first()
        .filter(|entry| entry.value().as_string().is_some())
        .map_or(span, |entry| entry.span().into());
    let name = string_arg(node).unwrap_or_default().to_string();
    if !is_name(&name) {
        issues.push(
            Issue::new(
                format!("invalid {} source name `{name}`", kind.as_str()),
                src,
            )
            .at(
                name_span,
                "lowercase, digits and dashes, starting with a letter",
            )
            .help("the alias qualifies references and names its local cache"),
        );
        return None;
    }

    let dir = value(node, "dir");
    let url = value(node, "url");
    let path = value(node, "path");
    let version = value(node, "version");
    let sha256 = value(node, "sha256");
    let unpinned = value(node, "unpinned");

    if dir.is_some() == url.is_some() {
        issues.push(
            Issue::new(format!("`{name}` needs one source location"), src)
                .at(name_span, "pick `dir` or `url`")
                .help("a local source declares `dir`; a fetched source declares `url`, not both"),
        );
        return None;
    }

    if let Some((dir, _)) = dir {
        if let Some((_, at)) = path.or(version).or(sha256).or(unpinned) {
            issues.push(
                Issue::new(format!("`{name}` is a local directory"), src)
                    .at(at, "only a Git source accepts this field")
                    .help("a local source needs only `dir`; pinning fields describe a fetched repository"),
            );
        }
        return Some(Collection {
            kind,
            name,
            at: At::Dir(dir.to_string()),
            span: name_span,
        });
    }

    let (url, url_span) = url.unwrap_or_default();
    if !url.starts_with("https://") {
        issues.push(
            Issue::new(format!("`{name}` does not use an HTTPS Git URL"), src)
                .at(url_span, "not HTTPS")
                .help("`url \"https://host/owner/repository\"`; credentials and host policy stay with Git"),
        );
        return None;
    }
    if url.contains('{') || url.contains('}') {
        issues.push(
            Issue::new(format!("`{name}` puts a template in its Git URL"), src)
                .at(url_span, "a repository URL is fixed")
                .help(
                    "put the branch, tag or commit in `version`; `url` names the repository alone",
                ),
        );
    }
    if let Some((path, at)) = path {
        if !valid_subtree(path) {
            issues.push(
                Issue::new(format!("`{path}` is not a source subtree"), src)
                    .at(at, "a clean relative path")
                    .help("name a directory inside the repository without `/`, `.` or `..` components"),
            );
        }
    }
    if let Some((sha256, at)) = sha256 {
        check_sha256(sha256, "the source", at, src, issues);
        if version.is_none() {
            issues.push(
                Issue::new(
                    format!("`{name}` verifies a hash but selects no version"),
                    src,
                )
                .at(at, "the remote HEAD can move")
                .help("add `version` with the branch, tag or commit this canonical archive hashes"),
            );
        }
        if let Some((_, unpinned_at)) = unpinned {
            issues.push(
                Issue::new(format!("`{name}` is both pinned and unpinned"), src)
                    .at(unpinned_at, "one of these is not true")
                    .help("drop `unpinned` when `sha256` verifies the selected version"),
            );
        }
    }

    let mut pin = Evidence::new(span);
    pin.url = Some(url.trim_end_matches('/').to_string());
    pin.path = path.map(|(value, _)| value.to_string());
    pin.version = version.map(|(value, _)| value.to_string());
    pin.sha256 = sha256.map(|(value, _)| value.to_string());
    pin.from = ShaFrom::Manual;
    pin.tracker = match (&pin.sha256, unpinned) {
        (Some(_), _) => Tracker::Manual(String::new()),
        (None, Some((why, _))) => Tracker::Unpinned(why.to_string()),
        (None, None) => Tracker::Unpinned(String::new()),
    };

    Some(Collection {
        kind,
        name,
        at: At::Git(pin),
        span: name_span,
    })
}

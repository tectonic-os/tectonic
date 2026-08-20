//! The `pin` table: one grammar for the four slots, and the one reader that
//! fills an `Evidence` from it.

use crate::diag::{Issue, Issues, Source, Span};
use crate::parse::schema::{Arg, Kind, Node, Prop, Say, NEEDS_VALUE};
use crate::parse::{check_sha256, kids, placeholders, prop, string_arg};
use crate::provenance::{Evidence, ShaFrom, Tracker};
use kdl::KdlNode;

/// The datasources the Renovate custom managers in .github/renovate.json5
/// match.
const DATASOURCES: [&str; 3] = ["github-releases", "github-tags", "git-refs"];

/// The archives the fetch can extract.
const ARCHIVES: [&str; 5] = [".tar.gz", ".tgz", ".tar.xz", ".tar.zst", ".tar.bz2"];

/// The annotation Renovate matches, declared as data rather than as a comment
/// so this can check it.
#[rustfmt::skip]
const RENOVATE: Node = Node::new("renovate",
    "The custom manager Renovate matches to keep the selector current.")
    .arg(Arg::None, Say::new("`renovate` takes no arguments", "unexpected value",
        "`renovate datasource=\"github-releases\" depName=\"owner/repo\"`"))
    .once("")
    .props(&[
        Prop { name: "datasource", kind: Kind::One(&DATASOURCES),
            desc: "Which Renovate datasource the pin is tracked through.",
            say: Say::new("unsupported datasource `{}`", "no custom manager matches it",
                "the managers in .github/renovate.json5 cover github-releases, github-tags, \
                 git-refs; a fourth would leave this pin unmanaged without saying so"),
            missing: Say::new("`{}` declares no datasource", "datasource= is required",
                "one of: github-releases, github-tags, git-refs") },
        Prop { name: "depName", kind: Kind::Str,
            desc: "What that datasource calls the thing being tracked.",
            say: Say::new("`{}` must be a string", "not a string", ""),
            missing: Say::new("`{}` declares no depName", "depName= is required",
                "`owner/repo` for the github datasources, the clone URL for git-refs") },
        Prop { name: "extractVersion", kind: Kind::Str,
            desc: "The pattern Renovate pulls the version out of the tag with.",
            say: Say::NONE, missing: Say::NONE },
    ], Say::new("unknown renovate property `{}`", "not part of the schema",
        "datasource, depName and extractVersion, spelled as Renovate spells them"));

/// The second answer: nothing tracks it, and why.
#[rustfmt::skip]
const MANUAL: Node = Node::new("manual", "Why nothing tracks this pin.")
    .arg(Arg::Str, Say::new("`{}` needs a reason", "no reason given",
        "say why nothing tracks this pin, or the next reader takes the absence for an \
         oversight"))
    .once("");

/// The third, and the only one that leaves the content unverified. A
/// collection's alone: everything else is fetched and run.
#[rustfmt::skip]
const UNPINNED: Node = Node::new("unpinned",
    "Why this follows a moving ref with no `sha256`, so every fetch takes whatever the ref \
     holds then and nothing checks what arrived.")
    .arg(Arg::Str, Say::new("`{}` needs a reason", "no reason given",
        "say why this is trusted enough to fetch unverified; with no `sha256` a mistaken or \
         compromised commit lands with nothing to catch it"))
    .once("");

/// One table for the four slots, held by `asset`, an out-of-tree module and a
/// collection. Which of the trackers and which of the trailing nodes say
/// anything is meaning, so the reader answers it rather than the shape.
#[rustfmt::skip]
pub const PIN: Node = Node::new("pin",
    "Where this comes from, which version of it, what proves you got that one, and what keeps \
     the selector current.")
    .arg(Arg::None, Say::new("`pin` takes no arguments", "unexpected value",
        "a pin carries its slots as child nodes: `pin { url \"...\"; version \"...\" }`"))
    .once("")
    .props(&[], Say::new("unknown pin property `{}`", "not part of the schema",
        "a pin carries its fields as child nodes, not properties"))
    .children(&[
        RENOVATE,
        MANUAL,
        UNPINNED,
        Node::new("version",
            "The selector: the version, tag or commit this is taken at, which the URL expands \
             and Renovate rewrites.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("url", "The locator: where the content comes from.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("sha256", "The verifier: what the fetched content is held to.")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .props(&[
                Prop { name: "from", kind: Kind::One(&["asset", "sidecar", "manual"]),
                    desc: "Where the hash is refreshed from.",
                    say: Say::new("`from` must be asset, sidecar or manual", "not a source",
                        "asset, the default, hashes the payload itself; sidecar reads the \
                         <url>.sha256 upstream publishes; manual means nothing recomputes it"),
                    missing: Say::NONE },
            ], Say::new("unknown sha256 property `{}`", "not part of the schema",
                "`sha256` accepts `from`")),
        Node::new("path", "The directory inside the archive the content sits in.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
    ], Say::new("unknown node `{}` in a pin", "not part of the schema",
        "a pin holds one of `renovate`, `manual` and `unpinned`, then `url`, `version`, \
         `sha256` and `path`"));

/// What a pin is on, which is what decides which of its slots say anything.
#[derive(Clone, Copy, PartialEq)]
pub enum Role {
    /// A payload a module fetches inside its build layer.
    Asset,
    /// A module that lives outside this repository, fetched and run as root.
    Module,
    /// A module collection references and copies resolve against.
    Collection,
    /// The record `copy module` wrote: the collection as it stood then, read
    /// back rather than authored, so nothing about it is diagnosed twice.
    Record,
}

impl Role {
    /// What the pin is on, as a sentence names it.
    fn word(self) -> &'static str {
        match self {
            Self::Asset => "an asset",
            Self::Module => "a module pin",
            Self::Collection | Self::Record => "a collection",
        }
    }

    /// Whether the content is downloaded as one archive, which is what makes a
    /// subtree path and a tar extension mean anything.
    fn archive(self) -> bool {
        matches!(self, Self::Module | Self::Collection | Self::Record)
    }
}

/// `pin { renovate ...; version "1.2.0"; url "..."; sha256 "..." }` Exactly one
/// of `renovate`, `manual` and `unpinned`, and the selector directly below
/// `renovate`, since one regex matches the two together.
pub fn read(node: &KdlNode, role: Role, src: &Source, issues: &mut Issues) -> Evidence {
    let span: Span = node.name().span().into();
    let mut pin = Evidence::new(span);

    let mut answers: Vec<(&str, Span)> = Vec::new();
    let mut version_span: Option<Span> = None;
    let mut previous: Option<&str> = None;

    for child in kids(node) {
        let kind = child.name().value();
        let at: Span = child.name().span().into();
        let value = string_arg(child)
            .filter(|v| !v.is_empty())
            .map(str::to_string);

        match kind {
            "renovate" => {
                answers.push(("renovate", at));
                let datasource = prop(child, "datasource").unwrap_or_default();
                if datasource == "git-refs" && role == Role::Module {
                    issues.push(
                        Issue::new("`git-refs` does not track a module pin", src)
                            .at(at, "no custom manager matches it")
                            .help("github-tags or github-releases, against the publishing repository's per-module tags; a repository with no tags is pinned `manual`"),
                    );
                }
                pin.tracker = Tracker::Renovate {
                    datasource: datasource.to_string(),
                    dep_name: prop(child, "depName").unwrap_or_default().to_string(),
                };
            }
            "manual" => {
                answers.push(("manual", at));
                pin.tracker = Tracker::Manual(value.clone().unwrap_or_default());
            }
            "unpinned" => {
                answers.push(("unpinned", at));
                pin.tracker = Tracker::Unpinned(value.clone().unwrap_or_default());
                if role != Role::Collection && role != Role::Record {
                    issues.push(
                        Issue::new(format!("{} cannot be `unpinned`", role.word()), src)
                            .at(at, "nothing would verify what arrived")
                            .help("only a module collection follows a moving ref: a fetched module is arbitrary shell running as root in the build, and an asset lands in the image"),
                    );
                }
            }
            "version" => {
                version_span = Some(at);
                pin.version = value;
                if answers.iter().any(|(w, _)| *w == "renovate") && previous != Some("renovate") {
                    issues.push(
                        Issue::new("something sits between `renovate` and `version`", src)
                            .at(at, "has to be the line directly below `renovate`")
                            .help("Renovate matches the two together, so anything between them stops the pin being tracked, silently"),
                    );
                }
            }
            "url" => pin.url = value,
            "sha256" => {
                pin.sha256 = value;
                if let Some(from) = prop(child, "from").and_then(ShaFrom::parse) {
                    pin.from = from;
                }
            }
            "path" => {
                pin.path = value;
                if !role.archive() {
                    issues.push(
                        Issue::new("`path` says nothing on an asset pin", src)
                            .at(at, "nothing is unpacked")
                            .help("a subtree path names a directory inside a fetched archive; an asset's URL already names the payload"),
                    );
                }
            }
            _ => {}
        }
        previous = Some(kind);
    }

    if role == Role::Record {
        return pin;
    }

    check_tracker(&answers, version_span, role, span, src, issues);
    check_verifier(&pin, &answers, role, span, src, issues);
    check_locator(&pin, role, src, issues);
    check_path(&pin, src, issues);
    pin
}

/// One of the three answers, and what a tracked pin has to carry for the
/// manager to bump it.
fn check_tracker(
    answers: &[(&str, Span)],
    version: Option<Span>,
    role: Role,
    span: Span,
    src: &Source,
    issues: &mut Issues,
) {
    // `unpinned` is a collection's alone, so everything else is told about the
    // two answers it has rather than the three.
    let (needs, how) = match role {
        Role::Collection => (
            "needs `renovate`, `manual` or `unpinned`",
            "`renovate datasource=\"github-tags\" depName=\"owner/repo\"`, `manual \"why nothing tracks it\"`, or `unpinned \"why a moving ref is trusted here\"`",
        ),
        _ => (
            "needs `renovate` or `manual`",
            "`renovate datasource=\"github-tags\" depName=\"owner/repo\"`, or `manual \"why nothing tracks it\"`",
        ),
    };
    match answers {
        [] => issues.push(
            Issue::new("the pin says nothing about how it is kept current", src)
                .at(span, needs)
                .help(how),
        ),
        [_] => {}
        [(first, _), .., (last, at)] => issues.push(
            Issue::new(format!("the pin is declared `{first}` and `{last}`"), src)
                .at(*at, "pick one")
                .help(how),
        ),
    }

    let tracked = answers.iter().any(|(w, _)| *w == "renovate");
    if tracked && version.is_none() {
        issues.push(
            Issue::new("the pin is tracked but declares no `version`", src)
                .at(span, "nothing to bump")
                .help("a tracked pin needs a `version` line directly below `renovate` for Renovate to rewrite"),
        );
    }
    if !tracked && version.is_none() && role.archive() {
        issues.push(
            Issue::new("the pin declares no `version`", src)
                .at(span, "nothing pinned")
                .help("an exact tag or commit; a moving ref would make the build depend on when it ran"),
        );
    }
}

/// What proves the content is the one that was reviewed.
fn check_verifier(
    pin: &Evidence,
    answers: &[(&str, Span)],
    role: Role,
    span: Span,
    src: &Source,
    issues: &mut Issues,
) {
    let unpinned = answers
        .iter()
        .find(|(w, _)| *w == "unpinned")
        .map(|(_, at)| *at);

    match &pin.sha256 {
        Some(sha256) => {
            check_sha256(sha256, "the pin", span, src, issues);
            if let Some(at) = unpinned {
                issues.push(
                    Issue::new("the pin is declared `unpinned` but pins a `sha256`", src)
                        .at(at, "one of the two is not true")
                        .help("a moving ref does not hash the same twice, so the fetch would fail as soon as it moved; drop whichever of them is wrong"),
                );
            }
            if pin.url.is_none() {
                issues.push(
                    Issue::new("the pin declares a `sha256` with no `url`", src)
                        .at(span, "a hash of nothing")
                        .help("add the url the hash belongs to, or drop the hash"),
                );
            }
        }
        None => {
            if pin.from != ShaFrom::Asset {
                issues.push(
                    Issue::new("the pin says where its hash comes from but pins none", src)
                        .at(span, "nothing to refresh"),
                );
            }
            if unpinned.is_some() {
                return;
            }
            let cloned = pin.cloned();
            let help = match role {
                Role::Collection => "the content hash is what says the archive is the one that was reviewed; `unpinned \"why it is trusted\"` is how a collection following a moving ref says it has none on purpose",
                Role::Module => "a remote module is arbitrary shell running as root in the build, so the content hash is required, not optional",
                _ => "every downloaded asset is verified against its pin; one whose url is a git repository is cloned at the commit or tag its version names instead, and declares none",
            };
            if role.archive() || (pin.url.is_some() && !cloned) {
                issues.push(
                    Issue::new("the pin declares no `sha256`", src)
                        .at(span, "nothing to verify what arrived")
                        .help(help),
                );
            }
        }
    }
}

/// Where the content comes from, and what the URL template may expand.
fn check_locator(pin: &Evidence, role: Role, src: &Source, issues: &mut Issues) {
    let span = pin.span;
    let Some(url) = &pin.url else {
        if role.archive() {
            issues.push(
                Issue::new("the pin declares no `url`", src)
                    .at(span, "nothing to fetch")
                    .help("`url \"https://host/owner/repo/archive/refs/tags/{version}.tar.gz\"`"),
            );
        } else if !matches!(pin.tracker, Tracker::None) {
            issues.push(
                Issue::new("the pin declares no `url`", src)
                    .at(span, "where it comes from is not recorded")
                    .help("the payload's download URL, or the repository a `git-refs` pin is cloned from"),
            );
        }
        return;
    };

    for placeholder in placeholders(url).filter(|found| *found != "{version}") {
        issues.push(
            Issue::new(
                format!("the URL has an unknown placeholder {placeholder}"),
                src,
            )
            .at(span, "not substituted")
            .help("a URL template expands `{version}` and nothing else"),
        );
    }
    if url.contains("{version}") && pin.version.is_none() {
        issues.push(
            Issue::new("the URL expands {version} but the pin declares none", src)
                .at(span, "would fetch a URL with braces in it"),
        );
    }
    if matches!(pin.tracker, Tracker::Renovate { .. })
        && !url.contains("{version}")
        && role.archive()
    {
        issues.push(
            Issue::new("the URL does not expand `{version}`", src)
                .at(span, "a bump would fetch the same archive")
                .help("put `{version}` in the URL, or mark the pin `manual` if the URL genuinely does not follow the selector"),
        );
    }

    let resolved = pin.url_resolved().unwrap_or_default();
    if role.archive() {
        if !url.starts_with("https://") && !url.starts_with("file://") {
            issues.push(
                Issue::new("the URL is not https", src)
                    .at(span, "unencrypted or unsupported scheme")
                    .help("https://, or file:// for a local archive"),
            );
        }
        if !ARCHIVES.iter().any(|ext| resolved.ends_with(ext)) {
            issues.push(
                Issue::new("the pin is not a tar archive", src)
                    .at(span, "cannot be extracted")
                    .help(format!(
                        "one of {}; the fetch strips the archive's leading directory, which is a tar option",
                        ARCHIVES.join(", ")
                    )),
            );
        }
    }

    let bad = match role.archive() {
        true => &['|', '"', '\'', '\\', '$', '`', ' ', '\n', '\t'][..],
        false => &['"', '\\', '$', '`', '\n'][..],
    };
    for (what, value) in [
        ("URL", resolved.as_str()),
        ("version", pin.version.as_deref().unwrap_or_default()),
    ] {
        if value.contains(bad) {
            issues.push(
                Issue::new(format!("the {what} contains a shell metacharacter"), src)
                    .at(span, "not usable in the fetch")
                    .help("a pin reaches the fetch as pipe-separated fields and a layer's env, so quotes, spaces, pipes and expansions are rejected rather than escaped"),
            );
        }
    }
}

fn check_path(pin: &Evidence, src: &Source, issues: &mut Issues) {
    let Some(path) = &pin.path else {
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
                .at(pin.span, reason)
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
        check(&doc.nodes()[0], &PIN, &src, &mut issues);
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
pin "tag" at="tag" {
    renovate "now" datasource="crates" flavour="x"
    manual
    version
    sha256 "abc"
    sha256 "def"
    subtree "modules/x"
}
"#,
        );
        assert_eq!(
            found,
            [
                "`pin` takes no arguments",
                "unknown pin property `at`",
                "`renovate` takes no arguments",
                "unsupported datasource `crates`",
                "unknown renovate property `flavour`",
                "`renovate` declares no depName",
                "`manual` needs a reason",
                "`version` needs a value",
                "`sha256` is declared twice",
                "unknown node `subtree` in a pin",
            ]
        );
    }

    /// The annotation every holder of the table points at.
    #[test]
    fn a_renovate_annotation_says_what_tracks_the_pin() {
        let found = messages("pin { renovate }");
        assert_eq!(
            found,
            [
                "`renovate` declares no datasource",
                "`renovate` declares no depName",
            ]
        );
    }
}

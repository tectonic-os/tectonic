//! The `pin` table of an asset: its grammar, and the one reader that fills an
//! `Evidence` from it.

use crate::diag::{Issue, Issues, Source, Span};
use crate::parse::schema::{Arg, Kind, Node, Prop, Say, NEEDS_VALUE};
use crate::parse::{check_sha256, kids, placeholders, prop, string_arg};
use crate::provenance::{Evidence, ShaFrom, Tracker};
use kdl::KdlNode;

/// The datasources the Renovate custom managers in .github/renovate.json5
/// match.
const DATASOURCES: [&str; 3] = ["github-releases", "github-tags", "git-refs"];

/// The annotation Renovate matches, declared as data so this can check it.
#[rustfmt::skip]
const RENOVATE: Node = Node::new("renovate",
    "The Renovate custom manager that keeps `version` current.").example("datasource=\"github-tags\" depName=\"owner/repo\"").pick()
    .arg(Arg::None, Say::new("`renovate` takes no arguments", "unexpected value",
        "`renovate datasource=\"github-releases\" depName=\"owner/repo\"`"))
    .once("")
    .props(&[
        Prop { name: "datasource", kind: Kind::One(&DATASOURCES),
            desc: "The Renovate datasource that tracks the pin.",
            say: Say::new("unsupported datasource `{}`", "no custom manager matches it",
                "the managers in .github/renovate.json5 cover github-releases, github-tags, \
                 git-refs; a fourth would leave this pin unmanaged without saying so"),
            missing: Say::new("`{}` declares no datasource", "datasource= is required",
                "one of: github-releases, github-tags, git-refs") },
        Prop { name: "depName", kind: Kind::Str,
            desc: "The name the datasource knows the dependency by.",
            say: Say::new("`{}` must be a string", "not a string", ""),
            missing: Say::new("`{}` declares no depName", "depName= is required",
                "`owner/repo` for the github datasources, the clone URL for git-refs") },
        Prop { name: "extractVersion", kind: Kind::Str,
            desc: "The pattern that extracts the version from a tag.",
            say: Say::NONE, missing: Say::NONE },
    ], Say::new("unknown renovate property `{}`", "not part of the schema",
        "datasource, depName and extractVersion, spelled as Renovate spells them"));

/// The second answer: nothing tracks it, and why.
#[rustfmt::skip]
const MANUAL: Node = Node::new("manual", "Why nothing tracks this pin.").example("\"upstream publishes no releases\"")
    .arg(Arg::Str, Say::new("`{}` needs a reason", "no reason given",
        "say why nothing tracks this pin, or the next reader takes the absence for an \
         oversight"))
    .once("");

/// The slots an asset's pin holds. Which of them say anything is meaning, so
/// the reader answers it.
#[rustfmt::skip]
pub const PIN: Node = Node::new("pin",
    "Where content comes from, which version it is, what verifies it, and what keeps the \
     version current.").about("A pin fixes where a download comes from and which version it is, and says how the version stays current. `tect` verifies each download against the pinned hash, so a changed upstream file is caught.")
    .scaffolds(&["version", "url"])
    .arg(Arg::None, Say::new("`pin` takes no arguments", "unexpected value",
        "a pin carries its slots as child nodes: `pin { url \"...\"; version \"...\" }`"))
    .once("")
    .props(&[], Say::new("unknown pin property `{}`", "not part of the schema",
        "a pin carries its fields as child nodes, not properties"))
    .children(&[
        RENOVATE,
        MANUAL,
        Node::new("version", "The version, tag or commit that `url` expands and Renovate rewrites.").example("\"v1.0.0\"").minimal()
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("url", "Where the content comes from.").example("\"https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz\"").minimal()
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("sha256", "The hash the fetched content must match.").example("\"b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3\"").pick()
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .props(&[
                Prop { name: "from", kind: Kind::One(&["asset", "sidecar", "manual"]),
                    desc: "Where the hash is refreshed from. `asset`, the default, hashes the \
                        payload itself. `sidecar` reads the `<url>.sha256` file that upstream \
                        publishes. `manual` means nothing recomputes the hash.",
                    say: Say::new("`from` must be asset, sidecar or manual", "not a source",
                        "asset, the default, hashes the payload itself; sidecar reads the \
                         <url>.sha256 upstream publishes; manual means nothing recomputes it"),
                    missing: Say::NONE },
            ], Say::new("unknown sha256 property `{}`", "not part of the schema",
                "`sha256` accepts `from`"))
            .notes(&[
                "If a pin names a download and no `sha256`, then `tect check` reports an \
                 error. A pin whose `url` is a Git repository is cloned at its `version` and \
                 declares none.",
            ]),
    ], Say::new("unknown node `{}` in a pin", "not part of the schema",
        "a pin holds one of `renovate` and `manual`, then `url`, `version` and `sha256`"))
    .lists(&[
        ("A pin holds exactly one of these, so every pin says how it stays current:", &[
            "`renovate`, if Renovate keeps the version current;",
            "`manual`, if nothing tracks the pin.",
        ]),
    ])
    .notes(&[
        "An `asset` holds a `pin`. A library in `sources` declares its own `version`, \
         `sha256` and `unpinned`.",
        "A base holds no `pin`. Its image reference holds the location and the version, and \
         `signed` records whether the base publishes a cosign signature.",
        "Renovate matches `renovate` together with the line directly below it. Put `version` on \
         that line.",
    ]);

/// `pin { renovate ...; version "1.2.0"; url "..."; sha256 "..." }` Exactly one
/// of `renovate` and `manual`, and the selector directly below `renovate`,
/// since one regex matches the two together.
pub fn read(node: &KdlNode, src: &Source, issues: &mut Issues) -> Evidence {
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
                pin.tracker = Tracker::Renovate {
                    datasource: datasource.to_string(),
                    dep_name: prop(child, "depName").unwrap_or_default().to_string(),
                };
            }
            "manual" => {
                answers.push(("manual", at));
                pin.tracker = Tracker::Manual(value.clone().unwrap_or_default());
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
            _ => {}
        }
        previous = Some(kind);
    }

    check_tracker(&answers, version_span, span, src, issues);
    check_verifier(&pin, span, src, issues);
    check_locator(&pin, src, issues);
    pin
}

/// One of the three answers, and what a tracked pin has to carry for the
/// manager to bump it.
fn check_tracker(
    answers: &[(&str, Span)],
    version: Option<Span>,
    span: Span,
    src: &Source,
    issues: &mut Issues,
) {
    let needs = "needs `renovate` or `manual`";
    let how = "`renovate datasource=\"github-tags\" depName=\"owner/repo\"`, or `manual \"why nothing tracks it\"`";
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
}

/// What proves the content is the one that was reviewed.
fn check_verifier(pin: &Evidence, span: Span, src: &Source, issues: &mut Issues) {
    match &pin.sha256 {
        Some(sha256) => {
            check_sha256(sha256, "the pin", span, src, issues);
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
            if pin.url.is_some() && !pin.cloned() {
                issues.push(
                    Issue::new("the pin declares no `sha256`", src)
                        .at(span, "nothing to verify what arrived")
                        .help("every downloaded asset is verified against its pin; one whose url is a git repository is cloned at the commit or tag its version names instead, and declares none"),
                );
            }
        }
    }
}

/// Where the content comes from, and what the URL template may expand.
fn check_locator(pin: &Evidence, src: &Source, issues: &mut Issues) {
    let span = pin.span;
    let Some(url) = &pin.url else {
        if !matches!(pin.tracker, Tracker::None) {
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
    let resolved = pin.url_resolved().unwrap_or_default();
    let bad = &['"', '\\', '$', '`', '\n'][..];
    for (what, value) in [
        ("URL", resolved.as_str()),
        ("version", pin.version.as_deref().unwrap_or_default()),
    ] {
        if value.contains(bad) {
            issues.push(
                Issue::new(format!("the {what} contains a shell metacharacter"), src)
                    .at(span, "not usable in the fetch")
                    .help("a pin reaches the fetch as pipe-separated fields and a layer's env, so quotes, spaces, pipes and expansions are rejected"),
            );
        }
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
        issues.findings()
    }

    /// A manual pin with no `url` records no download. The reader names the
    /// missing `url` and asks for no hash of nothing.
    #[test]
    fn a_pin_with_no_url_asks_for_the_url_alone() {
        let text = "pin { manual \"vendored by hand\" }";
        let doc: KdlDocument = text.parse().expect("valid KDL");
        let src = Source::new("module.kdl", text);
        let mut issues = Issues::default();
        read(&doc.nodes()[0], &src, &mut issues);
        let found = issues.findings();
        assert!(
            found.iter().any(|said| said.contains("declares no `url`")),
            "{found:?}"
        );
        assert!(
            !found
                .iter()
                .any(|said| said.contains("declares no `sha256`")),
            "{found:?}"
        );
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

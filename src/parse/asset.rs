//! `asset`, the pinned upstream payload a module fetches.

use crate::diag::{Issue, Issues, Source, Span};
use crate::model::asset::{Asset, ShaFrom};
use crate::parse::schema::{Arg, Kind, Node, Prop, Say};
use crate::parse::{kids, prop, string_arg};
use kdl::KdlNode;

const NEEDS_VALUE: Say = Say::new("`{}` needs a value", "nothing given", "");

/// The datasources the Renovate custom managers in .github/renovate.json5
/// match.
const DATASOURCES: [&str; 3] = ["github-releases", "github-tags", "git-refs"];

/// The annotation Renovate matches, declared as data rather than as a comment
/// so this can check it. A module pin's `source` carries the same node.
#[rustfmt::skip]
pub const RENOVATE: Node = Node::new("renovate",
    "The custom manager Renovate matches to keep the pin current.")
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

/// The other half of the same choice, carried by both grammars.
#[rustfmt::skip]
pub const MANUAL: Node = Node::new("manual", "Why nothing tracks this pin.")
    .arg(Arg::Str, Say::new("`{}` needs a reason", "no reason given",
        "say why nothing tracks this pin, or the next reader takes the absence for an \
         oversight"))
    .once("");

#[rustfmt::skip]
pub const ASSET: Node = Node::new("asset",
    "A pinned upstream payload the module fetches, reaching the build as ASSET_*.")
    .arg(Arg::Str, Say::new("`asset` needs a name", "no name given",
        "`asset \"starship\" { ... }`; the name becomes the ASSET_* env prefix"))
    .unique(Say::new("asset `{}` is declared twice", "already declared above",
        "two assets under one name would resolve to the same ASSET_* env"))
    .props(&[], Say::new("unknown asset property `{}`", "not part of the schema",
        "an asset carries its fields as child nodes, not properties"))
    .children(&[
        RENOVATE,
        MANUAL,
        Node::new("version", "The pinned version, which the URL expands and Renovate rewrites.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("url", "Where the payload is fetched from.")
            .arg(Arg::Str, NEEDS_VALUE).once(""),
        Node::new("sha256", "What the fetched payload is verified against.")
            .arg(Arg::Str, NEEDS_VALUE).once("")
            .props(&[
                Prop { name: "from", kind: Kind::One(&["asset", "sidecar", "manual"]),
                    desc: "Where the hash is refreshed from.",
                    say: Say::new("`from` must be asset, sidecar or manual", "not a source",
                        "asset, the default, hashes the asset itself; sidecar reads the \
                         <url>.sha256 upstream publishes; manual means nothing recomputes it"),
                    missing: Say::NONE },
            ], Say::new("unknown sha256 property `{}`", "not part of the schema",
                "`sha256` accepts `from`")),
    ], Say::new("unknown node `{}` in an asset", "not part of the schema",
        "an asset holds `renovate` or `manual`, `version`, `url` and `sha256`"));

/// `asset "starship" { renovate ...; version "1.26.0"; url "..."; sha256 "..."
/// }`
pub fn parse(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Asset> {
    let span: Span = node.name().span().into();
    let name = string_arg(node)?.to_string();

    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        issues.push(
            Issue::new(format!("invalid asset name `{name}`"), src)
                .at(span, "lowercase, digits and dashes only")
                .help("the name becomes an env var, uppercased with dashes as underscores and prefixed ASSET_"),
        );
    }

    let mut asset = Asset {
        name,
        version: None,
        url: None,
        sha256: None,
        from: ShaFrom::Asset,
        span,
    };

    let mut manual: Option<Span> = None;
    let mut renovate: Option<Span> = None;
    let mut version_span: Option<Span> = None;
    let mut previous: Option<&str> = None;

    for child in kids(node) {
        let kind = child.name().value();
        let child_span: Span = child.name().span().into();
        let value = string_arg(child)
            .filter(|v| !v.is_empty())
            .map(str::to_string);

        match kind {
            "renovate" => renovate = Some(child_span),
            "manual" => manual = Some(child_span),
            "version" => {
                version_span = Some(child_span);
                asset.version = value;
                if renovate.is_some() && previous != Some("renovate") {
                    issues.push(
                        Issue::new(
                            format!("`{}` puts something between `renovate` and `version`", asset.name),
                            src,
                        )
                        .at(child_span, "has to be the line directly below `renovate`")
                        .help("Renovate matches the two together, so anything between them stops the pin being tracked, silently"),
                    );
                }
            }
            "url" => asset.url = value,
            "sha256" => {
                asset.sha256 = value;
                if let Some(from) = prop(child, "from").and_then(ShaFrom::parse) {
                    asset.from = from;
                }
            }
            _ => {}
        }
        previous = Some(kind);
    }

    match (renovate, manual) {
        (Some(_), Some(manual)) => issues.push(
            Issue::new(
                format!("`{}` is declared both tracked and manual", asset.name),
                src,
            )
            .at(manual, "pick one")
            .help("`renovate` says Renovate bumps this pin, `manual` says nothing does and why"),
        ),
        (None, None) => issues.push(
            Issue::new(
                format!("`{}` says nothing about how its pin is kept current", asset.name),
                src,
            )
            .at(span, "needs `renovate` or `manual`")
            .help("`renovate datasource=\"github-releases\" depName=\"owner/repo\"`, or `manual \"why nothing tracks it\"`"),
        ),
        _ => {}
    }

    if renovate.is_some() && version_span.is_none() {
        issues.push(
            Issue::new(
                format!("`{}` is Renovate-tracked but pins no version", asset.name),
                src,
            )
            .at(span, "nothing to bump")
            .help("a tracked pin needs a `version` line directly below `renovate` for Renovate to rewrite"),
        );
    }

    match (&asset.url, &asset.sha256) {
        (Some(_), None) => issues.push(
            Issue::new(
                format!("`{}` declares a url with no sha256", asset.name),
                src,
            )
            .at(span, "an unverified fetch")
            .help("every downloaded asset is verified against its pin; a module that clones a git ref instead declares neither"),
        ),
        (None, Some(_)) => issues.push(
            Issue::new(
                format!("`{}` declares a sha256 with no url", asset.name),
                src,
            )
            .at(span, "a hash of nothing")
            .help("add the url the hash belongs to, or drop the hash"),
        ),
        _ => {}
    }

    if let Some(sha256) = &asset.sha256 {
        if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            issues.push(
                Issue::new(format!("`{}` has a malformed sha256", asset.name), src)
                    .at(span, "not 64 hex digits")
                    .help("sha256sum output, lowercase"),
            );
        } else if sha256.chars().any(|c| c.is_ascii_uppercase()) {
            issues.push(
                Issue::new(format!("`{}` has an uppercase sha256", asset.name), src)
                    .at(span, "lowercase, as sha256sum writes it")
                    .help("the checksum workflow rewrites this line by matching the pinned value, so its case has to be the one sha256sum produces"),
            );
        }
    }

    if asset.sha256.is_none() && asset.from != ShaFrom::Asset {
        issues.push(
            Issue::new(
                format!(
                    "`{}` says where its hash comes from but pins no hash",
                    asset.name
                ),
                src,
            )
            .at(span, "nothing to refresh"),
        );
    }

    check_url(&asset, src, issues);

    for (what, value) in [
        ("version", asset.version.as_deref()),
        ("url", asset.url.as_deref()),
    ] {
        if value.is_some_and(|v| v.contains(['"', '\\', '$', '`', '\n'])) {
            issues.push(
                Issue::new(
                    format!("the {what} of `{}` contains a shell metacharacter", asset.name),
                    src,
                )
                .at(span, "not usable in a layer's env")
                .help("asset fields are written into a RUN env prefix, so \" \\ $ ` and newlines are rejected rather than escaped; the URL template's only expansion is {version}"),
            );
        }
    }

    Some(asset)
}

/// A URL template expands `{version}` and nothing else.
fn check_url(asset: &Asset, src: &Source, issues: &mut Issues) {
    let Some(url) = &asset.url else {
        return;
    };
    for (index, _) in url.match_indices('{') {
        let placeholder = url[index..]
            .find('}')
            .map(|end| &url[index..=index + end])
            .unwrap_or(&url[index..]);
        if placeholder != "{version}" {
            issues.push(
                Issue::new(
                    format!("`{}` has an unknown placeholder {placeholder}", asset.name),
                    src,
                )
                .at(asset.span, "not substituted")
                .help("a URL template expands `{version}` and nothing else"),
            );
        }
    }
    if url.contains("{version}") && asset.version.is_none() {
        issues.push(
            Issue::new(
                format!("`{}` expands {{version}} but pins no version", asset.name),
                src,
            )
            .at(asset.span, "would fetch a URL with braces in it"),
        );
    }
}

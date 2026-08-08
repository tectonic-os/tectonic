//! Asset pins: the upstream payloads a module fetches, as data.

use crate::diag::{Issue, Issues, Source, Span};
use kdl::KdlNode;

/// Where the expected hash comes from when a version bump makes the pinned one
/// stale.
#[derive(Clone, Copy, PartialEq)]
pub enum ShaFrom {
    /// Hash the asset at `url`.
    Asset,
    /// Upstream publishes `<url>.sha256` beside the asset.
    Sidecar,
    /// Nothing derives it.
    Manual,
}

impl ShaFrom {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "asset" => Some(Self::Asset),
            "sidecar" => Some(Self::Sidecar),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Sidecar => "sidecar",
            Self::Manual => "manual",
        }
    }
}

pub struct Asset {
    pub name: String,
    /// The pinned upstream ref: a version, a tag or a commit.
    pub version: Option<String>,
    /// Unexpanded, `{version}` included, because this is also what the
    /// checksum workflow rewrites.
    pub url: Option<String>,
    pub sha256: Option<String>,
    pub from: ShaFrom,
    pub span: Span,
}

/// The datasources the Renovate custom managers in .github/renovate.json5
/// match.
const DATASOURCES: [&str; 3] = ["github-releases", "github-tags", "git-refs"];

impl Asset {
    /// `ASSET_NERD_FONTS_VERSION`, and the same for `_URL` and `_SHA256`.
    pub fn env_prefix(&self) -> String {
        format!("ASSET_{}", self.name.to_uppercase().replace('-', "_"))
    }

    /// The URL a build actually fetches.
    pub fn url_resolved(&self) -> Option<String> {
        let url = self.url.as_ref()?;
        Some(match &self.version {
            Some(version) => url.replace("{version}", version),
            None => url.clone(),
        })
    }

    /// Every env pair this asset puts on its module's layer, in the order they
    /// are written: the pin, where it comes from, what it must hash to.
    pub fn env(&self) -> Vec<(String, String)> {
        let prefix = self.env_prefix();
        let mut out = Vec::new();
        if let Some(version) = &self.version {
            out.push((format!("{prefix}_VERSION"), version.clone()));
        }
        if let Some(url) = self.url_resolved() {
            out.push((format!("{prefix}_URL"), url));
        }
        if let Some(sha256) = &self.sha256 {
            out.push((format!("{prefix}_SHA256"), sha256.clone()));
        }
        out
    }
}

/// `asset "starship" { renovate ...; version "1.26.0"; url "..."; sha256 "..."
/// }`
pub fn parse(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Asset> {
    let span: Span = node.name().span().into();
    let Some(name) = node
        .entries()
        .iter()
        .find(|e| e.name().is_none())
        .and_then(|e| e.value().as_string())
        .map(str::to_string)
    else {
        issues.push(
            Issue::new("`asset` needs a name", src)
                .at(span, "no name given")
                .help("`asset \"starship\" { ... }`; the name becomes the ASSET_* env prefix"),
        );
        return None;
    };

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

    for entry in node.entries().iter().filter(|e| e.name().is_some()) {
        issues.push(
            Issue::new(
                format!(
                    "unknown asset property `{}`",
                    entry.name().map(|n| n.value()).unwrap_or_default()
                ),
                src,
            )
            .at(entry.span(), "not part of the schema")
            .help("an asset carries its fields as child nodes, not properties"),
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

    for child in node.children().map(|c| c.nodes()).unwrap_or_default() {
        let kind = child.name().value();
        let child_span: Span = child.name().span().into();
        let string = |issues: &mut Issues| match child
            .entries()
            .iter()
            .find(|e| e.name().is_none())
            .and_then(|e| e.value().as_string())
        {
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
            }
            "manual" => {
                manual = Some(child_span);
                let reason = child
                    .entries()
                    .iter()
                    .find(|e| e.name().is_none())
                    .and_then(|e| e.value().as_string())
                    .unwrap_or_default();
                if reason.is_empty() {
                    issues.push(
                        Issue::new("`manual` needs a reason", src)
                            .at(child_span, "no reason given")
                            .help("say why nothing tracks this pin, or the next reader takes the absence for an oversight"),
                    );
                }
            }
            "version" => {
                version_span = Some(child_span);
                asset.version = string(issues);
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
            "url" => asset.url = string(issues),
            "sha256" => {
                asset.sha256 = string(issues);
                parse_from(child, &mut asset, src, issues);
            }
            other => issues.push(
                Issue::new(format!("unknown node `{other}` in an asset"), src)
                    .at(child_span, "not part of the schema")
                    .help("an asset holds `renovate` or `manual`, `version`, `url` and `sha256`"),
            ),
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
                format!("`{}` says where its hash comes from but pins no hash", asset.name),
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

/// `renovate datasource="github-releases" depName="owner/repo"` Declared as
/// data rather than as a comment, so this can check it.
pub fn check_renovate(node: &KdlNode, src: &Source, issues: &mut Issues) {
    let span: Span = node.name().span().into();
    let mut datasource = None;
    let mut dep_name = None;

    for entry in node.entries() {
        let Some(key) = entry.name().map(|n| n.value()) else {
            issues.push(
                Issue::new("`renovate` takes no arguments", src)
                    .at(entry.span(), "unexpected value")
                    .help("`renovate datasource=\"github-releases\" depName=\"owner/repo\"`"),
            );
            continue;
        };
        let value = entry.value().as_string().map(str::to_string);
        match key {
            "datasource" => datasource = value,
            "depName" => dep_name = value,
            "extractVersion" => {}
            other => issues.push(
                Issue::new(format!("unknown renovate property `{other}`"), src)
                    .at(entry.span(), "not part of the schema")
                    .help("datasource, depName and extractVersion, spelled as Renovate spells them"),
            ),
        }
    }

    let Some(datasource) = datasource else {
        issues.push(
            Issue::new("`renovate` declares no datasource", src)
                .at(span, "datasource= is required")
                .help(format!("one of: {}", DATASOURCES.join(", "))),
        );
        return;
    };
    if !DATASOURCES.contains(&datasource.as_str()) {
        issues.push(
            Issue::new(format!("unsupported datasource `{datasource}`"), src)
                .at(span, "no custom manager matches it")
                .help(format!(
                    "the managers in .github/renovate.json5 cover {}; a fourth would leave this pin unmanaged without saying so",
                    DATASOURCES.join(", ")
                )),
        );
    }
    if dep_name.is_none() {
        issues.push(
            Issue::new("`renovate` declares no depName", src)
                .at(span, "depName= is required")
                .help("`owner/repo` for the github datasources, the clone URL for git-refs"),
        );
    }
}

fn parse_from(node: &KdlNode, asset: &mut Asset, src: &Source, issues: &mut Issues) {
    for entry in node.entries().iter().filter(|e| e.name().is_some()) {
        let key = entry.name().map(|n| n.value()).unwrap_or_default();
        if key != "from" {
            issues.push(
                Issue::new(format!("unknown sha256 property `{key}`"), src)
                    .at(entry.span(), "not part of the schema")
                    .help("`sha256` accepts `from`"),
            );
            continue;
        }
        match entry.value().as_string().and_then(ShaFrom::parse) {
            Some(from) => asset.from = from,
            None => issues.push(
                Issue::new("`from` must be asset, sidecar or manual", src)
                    .at(entry.span(), "not a source")
                    .help("asset, the default, hashes the asset itself; sidecar reads the <url>.sha256 upstream publishes; manual means nothing recomputes it"),
            ),
        }
    }
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

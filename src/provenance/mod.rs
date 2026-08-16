//! Where everything in a repository came from, in one shape.
//!
//! Four slots answer it, and every pin in the tree fills the same four: the
//! **locator** says where it comes from, the **selector** which version of it,
//! the **verifier** what proves you got that one, and the **tracker** who keeps
//! the selector current. `asset`, an out-of-tree module and a collection each
//! hold the one `pin` table; the base carries its locator and selector joined
//! in the image reference, and `signed` as its verifier.

pub mod evidence;
pub mod record;

use crate::diag::Span;
use crate::emit::json::Json;

/// Where the expected hash comes from when a version bump makes the pinned one
/// stale.
#[derive(Clone, Copy, PartialEq)]
pub enum ShaFrom {
    /// Hash the payload at the locator.
    Asset,
    /// Upstream publishes `<url>.sha256` beside it.
    Sidecar,
    /// Nothing derives it.
    Manual,
}

impl ShaFrom {
    pub(crate) fn parse(name: &str) -> Option<Self> {
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

/// Who keeps the selector current.
pub enum Tracker {
    /// Renovate, through the custom manager matching this datasource, against
    /// what that datasource calls the thing.
    Renovate {
        datasource: String,
        dep_name: String,
    },
    /// Nothing does, and why.
    Manual(String),
    /// Nothing does, the ref moves, and there is no verifier: every fetch takes
    /// whatever it held then.
    Unpinned(String),
    /// Nothing was declared, which is a diagnostic everywhere a pin is read.
    None,
}

impl Tracker {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Renovate { .. } => "renovate",
            Self::Manual(_) => "manual",
            Self::Unpinned(_) => "unpinned",
            Self::None => "none",
        }
    }

    /// What the author said about it, which only the two untracked answers
    /// carry.
    pub fn reason(&self) -> Option<String> {
        match self {
            Self::Manual(why) | Self::Unpinned(why) => Some(why.clone()),
            _ => None,
        }
    }

}

/// One pin, filled the same way whatever declared it.
pub struct Evidence {
    /// Unexpanded, `{version}` included, because this is what a reviewer reads
    /// and what the checksum workflow rewrites around.
    pub url: Option<String>,
    pub version: Option<String>,
    pub sha256: Option<String>,
    pub from: ShaFrom,
    /// The directory inside the archive the content sits in.
    pub path: Option<String>,
    pub tracker: Tracker,
    pub span: Span,
}

impl Evidence {
    pub fn new(span: Span) -> Self {
        Evidence {
            url: None,
            version: None,
            sha256: None,
            from: ShaFrom::Asset,
            path: None,
            tracker: Tracker::None,
            span,
        }
    }

    /// The URL a fetch actually requests.
    pub fn url_resolved(&self) -> Option<String> {
        let url = self.url.as_ref()?;
        Some(match &self.version {
            Some(version) => url.replace("{version}", version),
            None => url.clone(),
        })
    }

    /// Whether the content is cloned rather than downloaded, which is what
    /// makes the commit the selector names the verifier instead of a hash.
    pub fn cloned(&self) -> bool {
        self.url.as_deref().is_some_and(|url| url.ends_with(".git"))
    }

    /// Whether it follows a moving ref, which is what makes every fetch of it a
    /// different tree with nothing to verify it against.
    pub fn unpinned(&self) -> bool {
        matches!(self.tracker, Tracker::Unpinned(_))
    }

    /// The four slots, then what each of them was spelled with. Everything that
    /// records a provenance fact goes through here, so one shape reaches
    /// `plan.json`, the build record and `tect why`.
    pub fn json(&self) -> Json {
        Json::object([
            ("locator", Json::optional(self.url.clone())),
            ("selector", Json::optional(self.version.clone())),
            ("verifier", Json::optional(self.sha256.clone())),
            ("tracker", Json::string(self.tracker.as_str())),
            ("reason", Json::optional(self.tracker.reason())),
            ("from", Json::string(self.from.as_str())),
            ("path", Json::optional(self.path.clone())),
            ("resolved", Json::optional(self.url_resolved())),
        ])
    }
}

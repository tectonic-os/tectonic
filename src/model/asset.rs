//! Asset pins: the upstream payloads a module fetches, as data.

use crate::diag::Span;

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

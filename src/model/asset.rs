//! Asset pins: the upstream payloads a module fetches, as data.

use crate::diag::Span;
use crate::provenance::Evidence;

pub struct Asset {
    pub name: String,
    /// Where it comes from, which version of it, and what proves you got that
    /// one, in the shape every other pin in the tree uses.
    pub pin: Evidence,
    pub span: Span,
}

impl Asset {
    /// `ASSET_NERD_FONTS_VERSION`, and the same for `_URL` and `_SHA256`.
    pub fn env_prefix(&self) -> String {
        format!("ASSET_{}", self.name.to_uppercase().replace('-', "_"))
    }

    /// Every env pair this asset puts on its module's layer, in the order they
    /// are written: the pin, where it comes from, what it must hash to.
    pub fn env(&self) -> Vec<(String, String)> {
        let prefix = self.env_prefix();
        let mut out = Vec::new();
        if let Some(version) = &self.pin.version {
            out.push((format!("{prefix}_VERSION"), version.clone()));
        }
        if let Some(url) = self.pin.url_resolved() {
            out.push((format!("{prefix}_URL"), url));
        }
        if let Some(sha256) = &self.pin.sha256 {
            out.push((format!("{prefix}_SHA256"), sha256.clone()));
        }
        out
    }
}

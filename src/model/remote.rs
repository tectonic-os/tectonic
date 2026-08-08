//! Out-of-tree modules: an exact pin, fetched at generate time.

use crate::diag::Span;

/// Where fetched module trees land, relative to `modules/`.
pub const REMOTE_DIR: &str = ".remote";

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

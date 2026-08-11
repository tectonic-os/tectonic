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
    /// Why this follows a moving ref with no hash, where it declared one.
    pub unpinned: Option<String>,
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

/// One module collection an import resolves against, named by the owner its
/// modules land under in `modules/`.
pub struct Collection {
    pub name: String,
    pub at: At,
    pub span: Span,
}

/// Where a collection is.
pub enum At {
    /// A directory on this machine, read where it is: nothing is fetched, so
    /// there is nothing to hash.
    Dir(String),
    /// A pinned archive, fetched and verified like any other pin.
    Archive(Remote),
}

impl Collection {
    /// The directory inside the collection holding the modules, which is its
    /// root unless the archive says otherwise.
    pub fn subtree(&self) -> Option<&str> {
        match &self.at {
            At::Dir(_) => None,
            At::Archive(remote) => remote.path.as_deref(),
        }
    }

    /// Whether it follows a moving ref, which is what makes every fetch of it
    /// a different tree with nothing to verify it against.
    pub fn unpinned(&self) -> bool {
        match &self.at {
            At::Dir(_) => false,
            At::Archive(remote) => remote.unpinned.is_some(),
        }
    }
}

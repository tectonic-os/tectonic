//! Out-of-tree modules and the collections an import resolves against. Both are
//! one `Evidence`, the pin being the whole of what either declares.

use crate::diag::Span;
use crate::provenance::Evidence;

/// Where fetched module trees land, relative to `modules/`.
pub const REMOTE_DIR: &str = ".remote";

/// One module collection an import resolves against, named by the owner its
/// modules land under in `modules/`.
pub struct Collection {
    pub name: String,
    pub at: At,
    pub span: Span,
}

/// Where a collection is.
pub enum At {
    /// A directory on this machine, copied without a content hash.
    Dir(String),
    /// A pinned archive, fetched and verified like any other pin.
    Archive(Evidence),
}

impl Collection {
    /// The directory inside the collection holding the modules, which is its
    /// root unless the archive says otherwise.
    pub fn subtree(&self) -> Option<&str> {
        match &self.at {
            At::Dir(_) => None,
            At::Archive(pin) => pin.path.as_deref(),
        }
    }

    /// The pin, for a collection that has one to record.
    pub fn pin(&self) -> Option<&Evidence> {
        match &self.at {
            At::Dir(_) => None,
            At::Archive(pin) => Some(pin),
        }
    }

    /// Whether it follows a moving ref, which is what makes every fetch of it
    /// a different tree with nothing to verify it against.
    pub fn unpinned(&self) -> bool {
        self.pin().is_some_and(Evidence::unpinned)
    }
}

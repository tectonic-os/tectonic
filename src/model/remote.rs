//! Out-of-tree libraries and the sources that resolve them.

use crate::diag::Span;
use crate::provenance::Evidence;

/// What one source supplies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Components,
}

impl Kind {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "components" => Some(Self::Components),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Components => "components",
        }
    }
}

/// One named source and the kind of library content it supplies.
pub struct Collection {
    pub kind: Kind,
    pub name: String,
    pub at: At,
    pub span: Span,
}

/// Where a source is.
pub enum At {
    /// A directory on this machine, copied without a content hash.
    Dir(String),
    /// A Git repository fetched at one ref and archived canonically.
    Git(Evidence),
}

impl Collection {
    /// The directory inside the fetched repository that holds this source's
    /// content, which is none for a directory on this machine or for a source
    /// that names none.
    pub fn subtree(&self) -> Option<&str> {
        match &self.at {
            At::Dir(_) => None,
            At::Git(pin) => pin.path.as_deref(),
        }
    }

    /// The pin, for a source that has one to record.
    pub fn pin(&self) -> Option<&Evidence> {
        match &self.at {
            At::Dir(_) => None,
            At::Git(pin) => Some(pin),
        }
    }

    /// Whether it follows a moving ref, which is what makes every fetch of it
    /// a different tree with nothing to verify it against.
    pub fn unpinned(&self) -> bool {
        self.pin().is_some_and(|pin| pin.sha256.is_none())
    }
}

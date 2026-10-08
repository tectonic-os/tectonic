//! Out-of-tree libraries and the sources that resolve them.

use crate::diag::Span;
use crate::provenance::Evidence;

/// Where fetched module trees land, relative to `modules/`.
pub const REMOTE_DIR: &str = ".remote";

/// What one source supplies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Modules,
    BaseImages,
    Capabilities,
}

impl Kind {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "modules" => Some(Self::Modules),
            "base-images" => Some(Self::BaseImages),
            "capabilities" => Some(Self::Capabilities),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Modules => "modules",
            Self::BaseImages => "base-images",
            Self::Capabilities => "capabilities",
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

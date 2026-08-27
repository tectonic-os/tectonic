//! What the manifests mean together, once the base and the list are known.

pub mod collect;
pub mod graph;
pub mod name;
pub mod options;
pub mod order;
pub mod overlay;
pub mod workflow;

use collect::Collection;

/// The two indexes built while resolving one image, beside the manifests the
/// image's own entries now carry.
pub struct Resolved {
    pub shipped: overlay::Index,
    pub collected: Collection,
}

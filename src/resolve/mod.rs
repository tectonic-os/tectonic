//! What the manifests mean together, once the base and the list are known.

pub mod collect;
pub mod graph;
pub mod name;
pub mod options;
pub mod order;
pub mod overlay;
pub mod workflow;

use collect::Collection;

/// Answers whether a module that declares `supports` builds on `family`. A
/// module with no `supports` builds on every family. `check`, the provider
/// index and the import picker all read this one rule, so the picker never
/// offers a module that `check` then refuses.
pub fn supports(declared: &[String], family: &str) -> bool {
    declared.is_empty() || declared.iter().any(|has| has == family)
}

/// Answers whether the providers of one capability conflict. Each flag says
/// whether one provider declares the capability exclusive. Providers coexist
/// unless one of them does. `check` and the import clash read this one rule.
pub fn exclusive(providers: &[bool]) -> bool {
    providers.len() > 1 && providers.contains(&true)
}

/// The two indexes built while resolving one image, beside the manifests the
/// image's own entries now carry.
pub struct Resolved {
    pub shipped: overlay::Index,
    pub collected: Collection,
}

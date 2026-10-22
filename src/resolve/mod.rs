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

/// Answers whether two providers of one capability conflict. Each side is the
/// provider's flavour and whether it declares the capability exclusive.
/// Providers coexist unless they build into one target and one of them
/// declares it. `check` and the import clash read this one rule.
pub fn conflict(one: (Option<&str>, bool), other: (Option<&str>, bool)) -> bool {
    (one.1 || other.1) && meet(one.0, other.0)
}

/// Answers whether two image entries build into one target. An ungated entry
/// builds into every target, so it meets every entry. Two gated entries meet
/// only if they sit in the same flavour.
pub fn meet(one: Option<&str>, other: Option<&str>) -> bool {
    one.is_none() || other.is_none() || one == other
}

/// The two indexes built while resolving one image, beside the manifests the
/// image's own entries now carry.
pub struct Resolved {
    pub shipped: overlay::Index,
    pub collected: Collection,
}

//! What the manifests mean together, once the base and the list are known.

pub mod collect;
pub mod graph;
pub mod imports;
pub mod name;
pub mod options;
pub mod order;
pub mod overlay;
pub mod workflow;

use collect::Collection;

/// Answers whether a component that declares `targets` builds on `family`. A
/// component with no `targets` builds on every family. A base with no family
/// skips the family check, so every component builds on it. `check`, the provider
/// index and the import picker all read this one rule, so the picker never
/// offers a component that `check` then refuses.
pub fn targets(declared: &[String], family: &str) -> bool {
    family.is_empty() || declared.is_empty() || declared.iter().any(|has| has == family)
}

/// Answers whether a declared capability meets a requirement. A declared name
/// meets a requirement for itself and for each of its ancestors, so `rpm/dnf5`
/// meets `rpm`.
pub fn meets(declared: &str, required: &str) -> bool {
    declared
        .strip_prefix(required)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Answers whether the component at `path` that declares `provides` meets a
/// requirement. A component meets a requirement for its own path exactly. An
/// organising directory above it never becomes a capability.
pub fn answers<'a>(
    path: &str,
    mut provides: impl Iterator<Item = &'a str>,
    required: &str,
) -> bool {
    path == required || provides.any(|has| meets(has, required))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ungated_entry_meets_every_target() {
        assert!(meet(None, Some("dev")));
        assert!(meet(Some("dev"), None));
        assert!(meet(Some("dev"), Some("dev")));
        assert!(!meet(Some("dev"), Some("prod")));
    }
}

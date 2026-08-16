//! The bases the tool knows: what family each belongs to, and what it already
//! ships. Reference data, not a module. A base that is not in here is not an
//! error; it is a base nothing can describe, so `check` reports what is
//! unsatisfied.
//!
//! The shipped catalog is compiled in as a fallback for the runtime asset. A
//! collection extends the selected catalog with a `bases.kdl` at its root,
//! which wins on a base both of them describe.

use crate::diag::{Issue, Issues, Span};
use crate::model::remote::Collection;
use std::path::Path;

/// What a collection extends the catalog with, at its root.
pub const BASES_FILE: &str = "bases.kdl";

const BUILT_IN: &str = include_str!("../assets/bases.kdl");

pub struct Base {
    /// The full image reference, written verbatim into `base`.
    pub image: String,
    pub family: String,
    /// Capabilities the upstream image already ships, which suppress a module
    /// providing only these.
    pub provides: Vec<String>,
    pub provides_files: Vec<String>,
    /// What a person needs to see to pick between this and the rest.
    pub about: String,
    pub signed: bool,
    /// Where it was declared, for a diagnostic about a second declaration.
    pub span: Span,
}

impl Base {
    /// Whether two entries describe a base the same way, which is what makes
    /// one of them worth reporting.
    fn differs(&self, other: &Base) -> bool {
        self.family != other.family
            || self.provides != other.provides
            || self.provides_files != other.provides_files
            || self.about != other.about
            || self.signed != other.signed
    }
}

/// A tool-owned base a collection describes differently, which is how a stale
/// entry is corrected without a tool release. One that repeats the tool entry
/// corrects nothing and is not reported.
pub struct Shadow {
    pub image: String,
    pub collection: String,
}

/// The runtime catalog when present, otherwise its embedded snapshot, and then
/// what every collection already on this machine adds to it.
/// A collection that is not there is not read: the catalog costs no network, so
/// a base picker works in a repository nothing has been fetched into.
pub fn catalog(
    root: &Path,
    sources: &[Collection],
    issues: &mut Issues,
) -> (Vec<Base>, Vec<Shadow>) {
    let runtime = crate::init::assets()
        .ok()
        .map(|assets| assets.join(BASES_FILE));
    let mut bases = match runtime
        .as_deref()
        .and_then(|path| crate::parse::bases::read(path, issues))
    {
        Some((bases, _)) => bases,
        None => crate::parse::bases::parse("built-in bases.kdl", BUILT_IN, issues).0,
    };
    let mut shadows: Vec<Shadow> = Vec::new();
    let mut declared: Vec<(String, String)> = Vec::new();

    for collection in sources {
        let Some(dir) = crate::import::cached(root, collection) else {
            continue;
        };
        let Some((found, src)) = crate::parse::bases::read(&dir.join(BASES_FILE), issues) else {
            continue;
        };
        for base in found {
            if let Some((_, first)) = declared.iter().find(|(image, _)| *image == base.image) {
                issues.push(
                    Issue::new(
                        format!("`{}` is described by two collections", base.image),
                        &src,
                    )
                    .at(base.span, format!("`{first}` describes it too"))
                    .help(
                        "one base is one entry: two of them would each write a different family \
                         and a different `provides` into an image scaffolded on it, and which one \
                         did would depend on the order repo.kdl lists the collections in",
                    ),
                );
                continue;
            }
            declared.push((base.image.clone(), collection.name.clone()));
            match bases.iter().position(|known| known.image == base.image) {
                Some(at) => {
                    if bases[at].differs(&base) {
                        shadows.push(Shadow {
                            image: base.image.clone(),
                            collection: collection.name.clone(),
                        });
                    }
                    bases[at] = base;
                }
                None => bases.push(base),
            }
        }
    }
    (bases, shadows)
}

pub fn find<'a>(bases: &'a [Base], image: &str) -> Option<&'a Base> {
    bases.iter().find(|base| base.image == image)
}

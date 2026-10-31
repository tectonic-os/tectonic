//! The bases a new image can build on: every base component in the repository
//! and in each declared library on this machine.
//!
//! A declared library that is not on this machine describes nothing.

use crate::model::remote::{Collection, Kind as SourceKind};
use std::path::Path;

/// The repository every default library is read from.
pub const LIBRARY_URL: &str = "https://github.com/tectonic-os/library";

/// One base component that `tect create image` offers.
pub struct Base {
    /// Holds the component's path, which is what the image lists the base by.
    pub name: String,
    /// Names the library that holds the component. It is None for a base
    /// component the repository already holds.
    pub owner: Option<String>,
    /// Holds the image reference, with the default `version` written in.
    pub image: String,
    pub family: String,
    /// Capabilities the upstream image already ships, which suppress a component
    /// providing only these.
    pub provides: Vec<String>,
    /// Capabilities the base is not usable without, which a component in the
    /// image has to provide.
    pub requires: Vec<String>,
    /// Lists the components that the base imports.
    pub imports: Vec<String>,
    /// What the user needs to see to pick between this and the rest.
    pub about: String,
    /// Holds the default of the base's `version` option. `create image`
    /// writes it on the base entry, so Renovate keeps it current.
    pub version: Option<String>,
    /// The SSG datastream file an image on this base is measured against, as a
    /// bare filename under the content directory.
    pub scap_content: String,
    /// The bootloaders an image on this base can install, its default first.
    pub bootloaders: Vec<String>,
}

impl Base {
    /// Says how the picker shows the base: its image reference, and where it
    /// comes from.
    pub fn label(&self) -> String {
        match &self.owner {
            Some(owner) => format!("{} ({owner}/{})", self.image, self.name),
            None => format!("{} ({})", self.image, self.name),
        }
    }
}

/// One library this repository reads by default, which `create repo` offers
/// and `set library` adds by its kind alone.
pub struct DefaultLibrary {
    pub kind: SourceKind,
    /// The alias the source is declared under.
    pub alias: &'static str,
    /// The directory inside the library's repository that holds this kind.
    pub path: &'static str,
    /// What a person picks it by, in the `create repo` question.
    pub about: &'static str,
}

pub const DEFAULT_LIBRARIES: [DefaultLibrary; 1] = [DefaultLibrary {
    kind: SourceKind::Components,
    alias: "tectonic-components",
    path: "components",
    about: "Feature components and base images for many use cases",
}];

/// The default library of one kind, which every kind has.
pub fn default_library(kind: SourceKind) -> &'static DefaultLibrary {
    DEFAULT_LIBRARIES
        .iter()
        .find(|library| library.kind == kind)
        .expect("every source kind has a default library")
}

/// Lists every base component on this machine: the repository's own first,
/// then each declared library's in source order. A library base the
/// repository already holds under the same path is the repository's.
pub fn catalog(root: &Path, sources: &[Collection]) -> Vec<Base> {
    let mut bases: Vec<Base> = Vec::new();
    let components = crate::layout::components(root);
    let mut trees: Vec<(Option<String>, std::path::PathBuf)> = vec![(None, components)];
    for collection in sources
        .iter()
        .filter(|collection| collection.kind == SourceKind::Components)
    {
        if let Some(dir) = crate::collection::cached(root, collection) {
            trees.push((
                Some(collection.name.clone()),
                dir.join(collection.subtree().unwrap_or("")),
            ));
        }
    }
    for (owner, tree) in trees {
        for dir in crate::layout::member_dirs(&tree) {
            let Ok(name) = dir.strip_prefix(&tree) else {
                continue;
            };
            let name = name.display().to_string();
            if bases.iter().any(|base| base.name == name) {
                continue;
            }
            if let Some(base) = crate::parse::component::offered_base(&dir, name, owner.clone()) {
                bases.push(base);
            }
        }
    }
    bases
}

/// Brings every declared library that is not on this machine onto it, for a
/// command that reads the bases while the user can wait. A failure is the
/// calling command's to report: what is already cached stays readable.
pub fn fetch(root: &Path, sources: &[Collection]) -> Result<(), String> {
    for collection in sources {
        if crate::collection::cached(root, collection).is_none() {
            crate::collection::tree(root, collection)?;
        }
    }
    Ok(())
}

/// The reference with any `@sha256:…` taken off. A digest pins a base the
/// catalog already describes; the family it belongs to and what it ships do not
/// change with it, so both sides of a lookup are compared without one.
fn undigested(image: &str) -> &str {
    image.split_once('@').map_or(image, |(before, _)| before)
}

/// Finds the base a typed `--base` names, by its component path or by its
/// image reference.
pub fn find<'a>(bases: &'a [Base], image: &str) -> Option<&'a Base> {
    let image = undigested(image);
    bases
        .iter()
        .find(|base| undigested(&base.image) == image || base.name == image)
}

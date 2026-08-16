//! What a repository publishes for a new one to start from: one image's
//! declarations, and nothing that says whose they are.

use crate::model::image::{Entry, List, SCHEMA_VERSION};
use crate::model::remote::{At, Collection};
use crate::provenance::Tracker;
use std::fmt::Write as _;
use std::path::PathBuf;

/// The seed, or None when the repository nominates no image to publish one of.
pub fn file(list: &List) -> Option<(PathBuf, String)> {
    let seed = list.seed.as_ref()?;
    let image = list.images.iter().find(|i| i.id == seed.image)?;
    let base = image.base.as_ref()?;

    let modules: Vec<(String, &Entry)> = image
        .entries
        .iter()
        .filter_map(|entry| Some((entry.qualified(&seed.collection)?, entry)))
        .collect();

    let mut out = format!(
        "// GENERATED FILE, do not edit. Produced by `tect generate` from the\n\
         // {} image definition.\n\n\
         schema-version {SCHEMA_VERSION}\n\n\
         base \"{}\" {{\n\
         \x20   family \"{}\"\n\
         }}\n",
        image.id, base.image, base.family
    );

    let owners: Vec<&str> = modules
        .iter()
        .filter_map(|(name, _)| name.split('/').next())
        .collect();
    let sources: Vec<&Collection> = list
        .sources
        .iter()
        .filter(|c| owners.contains(&c.name.as_str()))
        .collect();
    if !sources.is_empty() {
        out.push_str("\nsources {\n");
        for collection in sources {
            source(collection, &mut out);
        }
        out.push_str("}\n");
    }

    out.push('\n');
    for (name, entry) in modules {
        let _ = write!(out, "module {name:?}");
        for (prop, value) in [("flavour", &entry.flavour), ("variant", &entry.variant)] {
            if let Some(value) = value {
                let _ = write!(out, " {prop}={value:?}");
            }
        }
        out.push('\n');
    }

    Some((PathBuf::from("generated").join("seed.kdl"), out))
}

/// One collection the seeded repository fetches its modules through. How the
/// pin is kept current is left out: that is a decision about the repository
/// holding it, which this is not. One that has no hash to be kept current
/// against is not, since a seeded repository is fetching it unverified.
fn source(collection: &Collection, out: &mut String) {
    let name = &collection.name;
    match &collection.at {
        At::Dir(path) => {
            let _ = writeln!(out, "\x20   {name} {path:?}");
        }
        At::Archive(pin) => {
            let _ = writeln!(out, "\x20   {name} {{");
            out.push_str("\x20       pin {\n");
            if let Tracker::Unpinned(why) = &pin.tracker {
                let _ = writeln!(out, "\x20           unpinned {why:?}");
            }
            let _ = writeln!(
                out,
                "\x20           version {:?}",
                pin.version.clone().unwrap_or_default()
            );
            let _ = writeln!(
                out,
                "\x20           url {:?}",
                pin.url.clone().unwrap_or_default()
            );
            if let Some(sha256) = &pin.sha256 {
                let _ = writeln!(out, "\x20           sha256 {sha256:?}");
            }
            if let Some(path) = &pin.path {
                let _ = writeln!(out, "\x20           path {path:?}");
            }
            out.push_str("\x20       }\n");
            out.push_str("\x20   }\n");
        }
    }
}

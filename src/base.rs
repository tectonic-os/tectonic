//! The bases the tool knows: what family each belongs to, and what it already
//! ships. A base that is not in here is not an error; it is a base nothing can
//! describe, so `check` reports what is unsatisfied.
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
    /// Capabilities the base is not usable without, which a module in the
    /// image has to provide. A base that is already a bootc image requires
    /// nothing; one that a module set makes into one says so here.
    pub requires: Vec<String>,
    /// What a person needs to see to pick between this and the rest.
    pub about: String,
    pub signed: bool,
    /// The SSG datastream file an image on this base is measured against, as a
    /// bare filename under the content directory. Empty where SSG publishes
    /// nothing for the release, which refuses `conforms` on it.
    pub scap_content: String,
    /// The bootloaders an image on this base can install, its default first.
    pub bootloaders: Vec<String>,
    /// Where it was declared, for a diagnostic about a second declaration.
    pub span: Span,
}

impl Base {
    /// Whether two entries describe a base the same way, which is what makes
    /// one of them worth reporting.
    fn differs(&self, other: &Base) -> bool {
        self.family != other.family
            || self.provides != other.provides
            || self.requires != other.requires
            || self.about != other.about
            || self.signed != other.signed
            || self.scap_content != other.scap_content
            || self.bootloaders != other.bootloaders
    }
}

/// Where a capability's presence is read: `capability "ssh" "/usr/sbin/sshd"`.
/// A row with no path names an abstract capability, which nothing witnesses.
pub struct Capability {
    pub name: String,
    pub path: Option<String>,
    pub span: Span,
}

/// Where a name found by default is looked for, in order.
const DEFAULT_DIRS: [&str; 2] = ["/usr/bin", "/usr/sbin"];

/// The paths whose presence witnesses `name`, any one of them enough: its row,
/// or `/usr/bin/<name>` then `/usr/sbin/<name>`. None for an abstract row.
pub fn witness(name: &str, rows: &[Capability]) -> Option<Vec<String>> {
    match rows.iter().find(|row| row.name == name) {
        Some(row) => row.path.clone().map(|path| vec![path]),
        None => Some(
            DEFAULT_DIRS
                .iter()
                .map(|dir| format!("{dir}/{name}"))
                .collect(),
        ),
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
/// what every collection already on this machine adds to it. A collection that
/// is not there is not read, so the catalog costs no network.
pub fn catalog(
    root: &Path,
    sources: &[Collection],
    issues: &mut Issues,
) -> (Vec<Base>, Vec<Shadow>) {
    let loaded = load(root, sources, issues);
    (loaded.bases, loaded.shadows)
}

/// The capability rows of the same catalog: a collection's row replaces the
/// tool's, and two collections describing one name differently are refused.
pub fn capabilities(root: &Path, sources: &[Collection], issues: &mut Issues) -> Vec<Capability> {
    load(root, sources, issues).capabilities
}

struct Loaded {
    bases: Vec<Base>,
    shadows: Vec<Shadow>,
    capabilities: Vec<Capability>,
}

fn load(root: &Path, sources: &[Collection], issues: &mut Issues) -> Loaded {
    let runtime = crate::init::assets()
        .ok()
        .map(|assets| assets.join(BASES_FILE));
    let tool = match runtime
        .as_deref()
        .and_then(|path| crate::parse::bases::read(path, issues))
    {
        Some(read) => read,
        None => crate::parse::bases::parse("built-in bases.kdl", BUILT_IN, issues),
    };
    let mut bases = tool.bases;
    let mut capabilities = tool.capabilities;
    let mut shadows: Vec<Shadow> = Vec::new();
    let mut declared: Vec<(String, String)> = Vec::new();
    let mut named: Vec<(String, String)> = Vec::new();

    for collection in sources {
        let Some(dir) = crate::import::cached(root, collection) else {
            continue;
        };
        let Some(read) = crate::parse::bases::read(&dir.join(BASES_FILE), issues) else {
            continue;
        };
        let src = read.src;
        for base in read.bases {
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
        for row in read.capabilities {
            let at = capabilities.iter().position(|known| known.name == row.name);
            if let Some((_, first)) = named.iter().find(|(name, _)| *name == row.name) {
                if at.is_some_and(|at| capabilities[at].path != row.path) {
                    issues.push(
                        Issue::new(
                            format!("capability `{}` is described by two collections", row.name),
                            &src,
                        )
                        .at(row.span, format!("`{first}` reads it elsewhere"))
                        .help(
                            "one name is read at one path; which row won would depend on the \
                             order repo.kdl lists the collections in",
                        ),
                    );
                }
                continue;
            }
            named.push((row.name.clone(), collection.name.clone()));
            match at {
                Some(at) => capabilities[at] = row,
                None => capabilities.push(row),
            }
        }
    }
    Loaded {
        bases,
        shadows,
        capabilities,
    }
}

/// The reference with any `@sha256:…` taken off. A digest pins a base the
/// catalog already describes; the family it belongs to and what it ships do not
/// change with it, so both sides of a lookup are compared without one.
fn undigested(image: &str) -> &str {
    image.split_once('@').map_or(image, |(before, _)| before)
}

pub fn find<'a>(bases: &'a [Base], image: &str) -> Option<&'a Base> {
    let image = undigested(image);
    bases.iter().find(|base| undigested(&base.image) == image)
}

//! The declared base-image and capability libraries: what family each base
//! belongs to, what it ships and what a capability's presence is read from.
//!
//! Nothing is compiled in. A repository declares the libraries it reads, and a
//! declared library that is not on this machine describes nothing.

use crate::diag::{Issue, Issues, Span};
use crate::model::remote::{Collection, Kind as SourceKind};
use std::path::{Path, PathBuf};

/// The suffix of one base's file inside a base-images library.
pub const BASE_SUFFIX: &str = ".base.kdl";

/// The file one capabilities library holds.
pub const CAPABILITIES_FILE: &str = "capabilities.kdl";

/// The repository every default library is read from.
pub const LIBRARY_URL: &str = "https://github.com/tectonic-os/library";

pub struct Base {
    /// The catalog name, which is the file stem.
    pub name: String,
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
    pub fn differs(&self, other: &Base) -> bool {
        self.family != other.family
            || self.provides != other.provides
            || self.requires != other.requires
            || self.about != other.about
            || self.signed != other.signed
            || self.scap_content != other.scap_content
            || self.bootloaders != other.bootloaders
    }
}

/// Where a capability's presence is read. A row with no path and no family
/// block names an abstract capability, which nothing witnesses.
pub struct Capability {
    pub name: String,
    /// The path every family reads, where the row carries one.
    pub path: Option<String>,
    /// The families that read a different path, each block naming one or more
    /// families and the path they share.
    pub families: Vec<(Vec<String>, String)>,
    pub span: Span,
}

impl Capability {
    /// The path that witnesses this capability on `family`: the block that
    /// names the family, else the bare path. None for an abstract row.
    fn path_for(&self, family: &str) -> Option<&str> {
        self.families
            .iter()
            .find(|(families, _)| families.iter().any(|named| named == family))
            .map(|(_, path)| path.as_str())
            .or(self.path.as_deref())
    }

    /// Whether two rows describe one name the same way, which is what makes
    /// one of them worth reporting.
    fn same_as(&self, other: &Capability) -> bool {
        self.path == other.path && self.families == other.families
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

pub const DEFAULT_LIBRARIES: [DefaultLibrary; 3] = [
    DefaultLibrary {
        kind: SourceKind::Modules,
        alias: "tectonic-modules",
        path: "modules",
        about: "Feature modules for many use cases",
    },
    DefaultLibrary {
        kind: SourceKind::BaseImages,
        alias: "core",
        path: "base-images/core",
        about: "Fedora, CentOS, RHEL, AlmaLinux, Rocky, Debian and Ubuntu",
    },
    DefaultLibrary {
        kind: SourceKind::Capabilities,
        alias: "core",
        path: "capabilities",
        about: "Where each capability's witness is read",
    },
];

/// The default library of one kind, which every kind has.
pub fn default_library(kind: SourceKind) -> &'static DefaultLibrary {
    DEFAULT_LIBRARIES
        .iter()
        .find(|library| library.kind == kind)
        .expect("every source kind has a default library")
}

/// Where a name found by default is looked for, in order.
const DEFAULT_DIRS: [&str; 2] = ["/usr/bin", "/usr/sbin"];

/// The paths that witness `name` on `family`: the claim's own `file=`, else
/// the capability row, and nothing else. A name with neither is abstract, so
/// no build stops on it and no boilerplate is owed for it.
pub fn witness(
    name: &str,
    family: &str,
    own: Option<&str>,
    rows: &[Capability],
) -> Option<Vec<String>> {
    if let Some(own) = own {
        return Some(vec![own.to_string()]);
    }
    let row = rows.iter().find(|row| row.name == name)?;
    row.path_for(family).map(|path| vec![path.to_string()])
}

/// The paths a probe of a base reads: the witness chain, or the conventional
/// directories where no row names the capability at all. An abstract row
/// suppresses the fallback.
pub fn probe(name: &str, family: &str, rows: &[Capability]) -> Option<Vec<String>> {
    match rows.iter().any(|row| row.name == name) {
        true => witness(name, family, None, rows),
        false => Some(
            DEFAULT_DIRS
                .iter()
                .map(|dir| format!("{dir}/{name}"))
                .collect(),
        ),
    }
}

/// The runtime catalog: what every declared base-images library on this
/// machine holds. A source that is not there describes nothing.
pub fn catalog(root: &Path, sources: &[Collection], issues: &mut Issues) -> Vec<Base> {
    load(root, sources, issues).bases
}

/// The capability rows of the same libraries: two libraries describing one
/// name differently are refused, and one that is not there describes nothing.
pub fn capabilities(root: &Path, sources: &[Collection], issues: &mut Issues) -> Vec<Capability> {
    load(root, sources, issues).capabilities
}

/// Brings every declared base-image and capability library that is not on
/// this machine onto it, for a command that reads a catalog while the user can
/// wait. A failure is the caller's to report: what is already cached stays
/// readable.
pub fn fetch(root: &Path, sources: &[Collection]) -> Result<(), String> {
    for collection in sources
        .iter()
        .filter(|collection| collection.kind != SourceKind::Modules)
    {
        if crate::import::cached(root, collection).is_none() {
            crate::import::tree(root, collection)?;
        }
    }
    Ok(())
}

struct Loaded {
    bases: Vec<Base>,
    capabilities: Vec<Capability>,
}

fn load(root: &Path, sources: &[Collection], issues: &mut Issues) -> Loaded {
    let mut bases: Vec<Base> = Vec::new();
    let mut capabilities: Vec<Capability> = Vec::new();
    let mut declared: Vec<(String, String)> = Vec::new();
    let mut named: Vec<(String, String)> = Vec::new();

    for collection in sources {
        if collection.kind == SourceKind::Modules {
            continue;
        }
        let Some(dir) = crate::import::cached(root, collection) else {
            continue;
        };
        let dir = dir.join(collection.subtree().unwrap_or(""));
        match collection.kind {
            SourceKind::Modules => {}
            SourceKind::BaseImages => {
                for path in base_files(&dir) {
                    let Some((base, src)) = crate::parse::bases::read_base(&path, issues) else {
                        continue;
                    };
                    if let Some((_, first)) =
                        declared.iter().find(|(image, _)| *image == base.image)
                    {
                        issues.push(
                            Issue::new(
                                format!("`{}` is described by two base files", base.image),
                                &src,
                            )
                            .at(base.span, format!("`{first}` describes it too"))
                            .help(
                                "one image reference is one base: two files would each write a \
                                 different family and a different `provides` into an image \
                                 scaffolded on it, and which one did would depend on the order the \
                                 sources are read in",
                            ),
                        );
                        continue;
                    }
                    declared.push((base.image.clone(), path.display().to_string()));
                    bases.push(base);
                }
            }
            SourceKind::Capabilities => {
                let path = dir.join(CAPABILITIES_FILE);
                let Some((read, src)) = crate::parse::bases::read_capabilities(&path, issues)
                else {
                    continue;
                };
                for row in read {
                    if let Some((_, first)) = named.iter().find(|(name, _)| *name == row.name) {
                        if let Some(at) =
                            capabilities.iter().position(|known| known.name == row.name)
                        {
                            if !capabilities[at].same_as(&row) {
                                issues.push(
                                    Issue::new(
                                        format!(
                                            "capability `{}` is described by two libraries",
                                            row.name
                                        ),
                                        &src,
                                    )
                                    .at(row.span, format!("`{first}` reads it elsewhere"))
                                    .help(
                                        "one name is read at one path; which row won would depend \
                                         on the order repo.kdl lists the sources in",
                                    ),
                                );
                            }
                        }
                        continue;
                    }
                    named.push((row.name.clone(), path.display().to_string()));
                    capabilities.push(row);
                }
            }
        }
    }
    Loaded {
        bases,
        capabilities,
    }
}

/// Every `*.base.kdl` file in one library, in name order.
pub(crate) fn base_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(BASE_SUFFIX))
        })
        .collect();
    files.sort();
    files
}

/// The reference with any `@sha256:…` taken off. A digest pins a base the
/// catalog already describes; the family it belongs to and what it ships do not
/// change with it, so both sides of a lookup are compared without one.
fn undigested(image: &str) -> &str {
    image.split_once('@').map_or(image, |(before, _)| before)
}

pub fn find<'a>(bases: &'a [Base], image: &str) -> Option<&'a Base> {
    let image = undigested(image);
    bases
        .iter()
        .find(|base| undigested(&base.image) == image || base.name == image)
}

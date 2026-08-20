//! The image files: one `.kdl` at the repository root per image.

use crate::diag::{Issue, Source, Span};
use crate::model::module::Module;
use crate::model::options::Value;
use crate::model::remote::{Collection, REMOTE_DIR};
use crate::provenance::Evidence;

/// The schema every file in the repository is written against.
pub const SCHEMA_VERSION: u32 = 1;

/// This release, which is what a repository pins in `tect-version`.
pub const TECT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The build target that carries no flavour: the ungated set, published
/// unsuffixed.
pub const NO_FLAVOUR: &str = "none";

/// Lowercase letters, digits and dashes, starting with a letter: image ids,
/// flavour names and capabilities.
pub fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A build target: which image, and which flavour of it.
#[derive(Clone)]
pub struct Target {
    pub image: String,
    /// A declared flavour, or `NO_FLAVOUR` for the ungated build.
    pub flavour: String,
}

impl Target {
    /// What this target publishes as: the image name alone for the ungated
    /// build, suffixed with the flavour otherwise.
    pub fn published(&self) -> String {
        match self.flavour.as_str() {
            NO_FLAVOUR => self.image.clone(),
            flavour => format!("{}-{flavour}", self.image),
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.flavour.as_str() {
            NO_FLAVOUR => write!(f, "{}", self.image),
            flavour => write!(f, "{}/{flavour}", self.image),
        }
    }
}

/// One image: what it calls itself, what it builds on, and everything it is
/// made of.
pub struct Image {
    /// The file this was declared in, so a diagnostic about anything under it
    /// points at the right one.
    pub src: Source,
    /// The machine name: published image, build target, cache tag, os-release
    /// DEFAULT_HOSTNAME, MOK key directory.
    pub id: String,
    pub name: String,
    pub pretty_name: String,
    pub url: String,
    pub issues_url: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub logo_url: String,
    pub conforms: String,
    /// None only when the `base` node is missing or malformed, which is
    /// already an issue: nothing downstream invents a default for it.
    pub base: Option<Base>,
    pub flavours: Vec<Flavour>,
    pub entries: Vec<Entry>,
    /// Entries the base makes redundant: everything the module provides, the
    /// base already ships, so nothing builds it.
    pub suppressed: Vec<Entry>,
    pub span: Span,
}

/// The base image, and what building on it may assume.
pub struct Base {
    /// The full image reference, emitted verbatim as the generated `FROM`.
    pub image: String,
    pub family: String,
    /// Capabilities the base satisfies that no module could implement
    /// portably: rechunking, initramfs generation, MAC policy.
    pub provides: Vec<Decl>,
    /// Binaries the base guarantees.
    pub provides_files: Vec<Decl>,
    /// Whether the base image publishes a cosign signature.
    pub signed: bool,
    pub span: Span,
}

/// A name the base declares, with the span to point at when something about it
/// is wrong.
pub struct Decl {
    pub name: String,
    pub span: Span,
}

pub struct Flavour {
    pub name: String,
    pub default: bool,
    pub pr_build: bool,
    pub span: Span,
}

/// One workflow the repository asks to be generated, named by its file stem
/// under `.github/workflows/`.
pub struct Workflow {
    pub name: String,
    pub span: Span,
}

/// One entry in the list: a module, and the decisions the image author makes
/// about it.
pub struct Entry {
    /// The collection this entry references, for a member listed under a
    /// `source` block.
    pub source: Option<String>,
    pub path: String,
    pub flavour: Option<String>,
    pub variant: Option<String>,
    /// Option name to the values set on it.
    pub options: Vec<(String, Vec<Value>, Span)>,
    /// The pin, for a module that lives outside this repository.
    pub remote: Option<Evidence>,
    pub span: Span,
    /// The manifest this entry names, loaded during resolution. None when it
    /// could not be read, which is already an issue.
    pub module: Option<Module>,
}

impl Entry {
    /// Where the module's directory is, relative to `modules/`.
    pub fn dir(&self) -> String {
        match (&self.source, &self.remote) {
            (Some(_), _) | (_, Some(_)) => format!("{REMOTE_DIR}/{}", self.path),
            (None, None) => self.path.clone(),
        }
    }

    /// The name inside its collection, or its whole local name.
    pub fn name(&self) -> &str {
        match &self.source {
            Some(_) => self
                .path
                .split_once('/')
                .map_or(&self.path, |(_, name)| name),
            None => &self.path,
        }
    }

    /// The pin that fetches it, whether declared on the entry or its source.
    pub fn pin<'a>(&'a self, sources: &'a [Collection]) -> Option<&'a Evidence> {
        self.remote.as_ref().or_else(|| {
            self.source
                .as_ref()
                .and_then(|name| sources.iter().find(|source| &source.name == name))
                .and_then(Collection::pin)
        })
    }

    /// What a seed calls this module: a reference or copy keeps its source
    /// collection, and one this repository owns takes the collection it
    /// publishes as.
    pub fn qualified(&self, publishes_as: &str) -> Option<String> {
        if self.remote.is_some() {
            return None;
        }
        if self.source.is_some() {
            return Some(self.path.clone());
        }
        let owner = self
            .module
            .as_ref()
            .and_then(|module| module.imported.as_ref())
            .map_or(publishes_as, |record| record.collection.as_str());
        Some(format!("{owner}/{}", self.path))
    }
}

/// The image a repository publishes a seed of, and the collection it publishes
/// its own modules as, which is what every module in the seed is named by.
pub struct Seed {
    pub image: String,
    pub collection: String,
}

/// The repository's declarations: every image in it, and the handful of
/// decisions that are about the repository rather than about any image.
pub struct List {
    /// What repo.kdl calls the repository, shown as the name of a tree.
    pub name: String,
    /// The machine name `name` derives, which a tree reads as its root.
    pub id: String,
    /// Every image, ordered by the file it was declared in, so the build
    /// matrix and every list this produces are the same on every machine
    /// whatever the files are called.
    pub images: Vec<Image>,
    /// The workflows repo.kdl asks for, which are the only ones written.
    pub workflows: Vec<Workflow>,
    /// The hour and minute the daily build runs, UTC, which every other
    /// schedule is an offset from.
    pub workflows_at: (u32, u32),
    /// The module collections references and copies resolve against.
    pub sources: Vec<Collection>,
    /// Which image a build with nothing named builds, and which one a pull
    /// request builds.
    pub default_image_id: Option<String>,
    pub pr_image_id: Option<String>,
    /// The image this repository publishes a seed of, when it publishes one.
    pub seed: Option<Seed>,
    /// Whether a build stamps the generated manifest onto the image as an OCI label.
    pub manifest_label: bool,
    /// Whether a provenance fact that is missing or does not match is an error
    /// rather than a read-out. Every fact is recorded either way.
    pub audit_enforce: bool,
    /// What repo.kdl declares, which is `SCHEMA_VERSION` or the load failed.
    pub schema_version: Option<u32>,
    /// Whether the node was there at all, so a malformed one is reported once
    /// rather than as both wrong and missing.
    pub(crate) schema_version_seen: bool,
    /// repo.kdl, for a diagnostic about either of the two above.
    pub repo_src: Source,
    /// Every file read, in order, for the count line a failure ends with.
    pub files: Vec<String>,
}

impl Image {
    /// The manifests this image's entries resolved to, in build order.
    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.entries.iter().filter_map(|e| e.module.as_ref())
    }

    pub fn default_flavour(&self) -> Option<&str> {
        self.flavours
            .iter()
            .find(|f| f.default)
            .map(|f| f.name.as_str())
    }

    /// Falls back to the default: a repository that has not thought about
    /// which flavour covers the most build surface still gets a PR build.
    pub fn pr_flavour(&self) -> Option<&str> {
        self.flavours
            .iter()
            .find(|f| f.pr_build)
            .map(|f| f.name.as_str())
            .or_else(|| self.default_flavour())
    }
}

impl List {
    /// The image a command answers about when it is given no image, and the
    /// one a bare build builds.
    pub fn default_image(&self) -> Option<&Image> {
        match &self.default_image_id {
            Some(id) => self.images.iter().find(|i| &i.id == id),
            None => match self.images.len() {
                1 => self.images.first(),
                _ => None,
            },
        }
    }

    /// Why there is no default, for a command that had to pick one. Raised
    /// there rather than at load: a command given an image never needed one.
    pub fn no_default(&self) -> Option<Issue> {
        let first = self.images.first().filter(|_| self.images.len() > 1)?;
        self.default_image_id.is_none().then(|| {
            Issue::new(
                format!(
                    "{} images are declared and none is the default",
                    self.images.len()
                ),
                &self.repo_src,
            )
            .help(format!(
                "the default image is what a command given no image answers about, and what a \
                 bare `tect build` builds; a repository with one image falls back to that one, \
                 and a repository with more says which: `default-image \"{}\"` in {}",
                first.id,
                crate::layout::REPO_FILE
            ))
        })
    }

    /// The image a pull request builds, which falls back to the default the
    /// way `pr-build` falls back to `default` within an image.
    pub fn pr_image(&self) -> Option<&Image> {
        match &self.pr_image_id {
            Some(id) => self.images.iter().find(|i| &i.id == id),
            None => self.default_image(),
        }
    }

    /// Every target: for each image, the ungated set and then its flavours.
    pub fn targets(&self) -> Vec<Target> {
        let mut out = Vec::new();
        for image in &self.images {
            out.push(Target {
                image: image.id.clone(),
                flavour: NO_FLAVOUR.to_string(),
            });
            out.extend(image.flavours.iter().map(|f| Target {
                image: image.id.clone(),
                flavour: f.name.clone(),
            }));
        }
        out
    }

    /// The one `name` names, refused naming what there is. Every caller that
    /// takes a `--target` asks this, so an unknown one is the same answer
    /// wherever it was typed.
    pub fn find_target(&self, name: &str) -> Result<Target, String> {
        let known = self.targets();
        known
            .iter()
            .find(|have| have.to_string() == name)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "`{name}` is not a build target (have: {})",
                    known
                        .iter()
                        .map(Target::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
    }

    /// What a build with nothing named builds: the default image, at its
    /// default flavour, or its ungated set when it declares no flavours.
    pub fn default_target(&self) -> Option<Target> {
        self.default_image().map(|image| Target {
            image: image.id.clone(),
            flavour: image.default_flavour().unwrap_or(NO_FLAVOUR).to_string(),
        })
    }

    /// The default image's ungated build, which is what the installer ISO and
    /// the disk builds lay down: an installer has no hardware to gate on, so
    /// it wants the set that gates on none.
    pub fn ungated_target(&self) -> Option<Target> {
        self.default_image().map(|image| Target {
            image: image.id.clone(),
            flavour: NO_FLAVOUR.to_string(),
        })
    }

    /// The image name the registry layer cache is kept under.
    pub fn cache_image(&self) -> Option<String> {
        self.default_image()
            .map(|image| format!("{}-cache", image.id))
    }

    /// The one target a pull request builds, for half the runner time.
    pub fn pr_target(&self) -> Option<Target> {
        self.pr_image().map(|image| Target {
            image: image.id.clone(),
            flavour: image.pr_flavour().unwrap_or(NO_FLAVOUR).to_string(),
        })
    }
}

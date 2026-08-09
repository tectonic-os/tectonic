//! The image files: one `.kdl` at the repository root per image.

use crate::diag::{Source, Span};
use crate::model::module::Module;
use crate::model::options::Value;
use crate::model::remote::{Collection, Remote, REMOTE_DIR};

/// The schema every file in the repository is written against.
pub const SCHEMA_VERSION: u32 = 1;

/// This release, which is what a repository pins in `tect-version`.
pub const TECT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The build target that carries no flavour: the ungated set, published
/// unsuffixed.
pub const NO_FLAVOUR: &str = "none";

/// Repo context, not an image: the file every image file is not.
pub const REPO_FILE: &str = "repo.kdl";

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
        write!(f, "{}/{}", self.image, self.flavour)
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

/// One workflow the image author has decided about, named by its file stem
/// under `.github/workflows/`.
pub struct WorkflowToggle {
    pub name: String,
    pub enabled: bool,
    pub span: Span,
}

/// One entry in the list: a module, and the decisions the image author makes
/// about it.
pub struct Entry {
    pub path: String,
    pub flavour: Option<String>,
    pub variant: Option<String>,
    /// Option name to the values set on it.
    pub options: Vec<(String, Vec<Value>, Span)>,
    /// The pin, for a module that lives outside this repository.
    pub remote: Option<Remote>,
    pub span: Span,
    /// The manifest this entry names, loaded during resolution. None when it
    /// could not be read, which is already an issue.
    pub module: Option<Module>,
}

impl Entry {
    /// Where the module's directory is, relative to `modules/`.
    pub fn dir(&self) -> String {
        match self.remote {
            Some(_) => format!("{REMOTE_DIR}/{}", self.path),
            None => self.path.clone(),
        }
    }
}

/// The repository's declarations: every image in it, and the handful of
/// decisions that are about the repository rather than about any image.
pub struct List {
    /// Every image, ordered by the file it was declared in, so the build
    /// matrix and every list this produces are the same on every machine
    /// whatever the files are called.
    pub images: Vec<Image>,
    /// Only the workflows named in repo.kdl.
    pub workflows: Vec<WorkflowToggle>,
    /// The module collections an import resolves a name against.
    pub sources: Vec<Collection>,
    /// Which image a build with nothing named builds, and which one a pull
    /// request builds.
    pub default_image_id: Option<String>,
    pub pr_image_id: Option<String>,
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
    pub fn images(&self) -> Vec<&Image> {
        self.images.iter().collect()
    }

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
        for image in self.images() {
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

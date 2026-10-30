//! The image files: one `.kdl` at the repository root per image.

use crate::diag::{Issue, Source, Span};
use crate::model::component::{Access, Component, Coverage, Refusal};
use crate::model::options::Value;
use crate::model::remote::Collection;

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

/// Answers whether `path` names a component directory the repository may hold.
/// The last part may end in one `@<tag>`, which marks a local copy of a library
/// component imported under a second name.
pub fn is_local_path(path: &str) -> bool {
    let (parents, last) = match path.rsplit_once('/') {
        Some((parents, last)) => (Some(parents), last),
        None => (None, path),
    };
    let last = match last.split_once('@') {
        Some((name, tag)) => {
            let tagged = !tag.is_empty()
                && tag
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !tagged {
                return false;
            }
            name
        }
        None => last,
    };
    is_name(last) && parents.is_none_or(|parents| parents.split('/').all(is_name))
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
    /// `uki-shim` or `uki-db` from `boot-chain`, empty when the image uses
    /// today's chain.
    pub boot: String,
    /// None only when the image lists no base or a malformed one, which is
    /// already an issue: nothing downstream invents a default for it.
    pub base: Option<Base>,
    /// Holds the entry of the listed component that declares `base`.
    /// Resolution loads that component into `base`.
    pub base_entry: Option<BaseEntry>,
    /// Holds the path of every component that the image file defines inline.
    pub defined: Vec<String>,
    /// What the image says about the disk it is installed onto. None where it
    /// says nothing, which leaves the family's answers standing.
    pub layout: Option<Layout>,
    /// Rules an installed component refuses that this image lets remediation set
    /// anyway. A component's refusal is a judgement about a deployment, and the
    /// deployment is the image's: `grub2_nousb_argument` removes a keyboard a
    /// kiosk has not got. The scan measures the result either way, which is
    /// what keeps a lift honest.
    pub allows: Vec<Refusal>,
    pub flavours: Vec<Flavour>,
    pub entries: Vec<Entry>,
    /// Entries the base makes redundant: everything the component provides, the
    /// base already ships, so nothing builds it.
    pub suppressed: Vec<Entry>,
    pub span: Span,
}

impl Base {
    /// Lists what the base ships. `base-image` is what makes the component a
    /// base, and no file in the image witnesses it.
    pub fn shipped(&self) -> impl Iterator<Item = &Decl> {
        self.provides
            .iter()
            .filter(|decl| decl.name != crate::parse::component::BASE_IMAGE)
    }

    /// Answers whether the base meets a requirement.
    pub fn answers(&self, required: &str) -> bool {
        self.provides
            .iter()
            .any(|decl| crate::resolve::meets(&decl.name, required))
    }

    /// Answers whether the base declares every name a component provides. A
    /// component that provides nothing is never covered.
    pub fn covers<'a>(&self, provides: impl IntoIterator<Item = &'a str>) -> bool {
        let mut provides = provides.into_iter().peekable();
        provides.peek().is_some()
            && provides.all(|name| self.provides.iter().any(|decl| decl.name == name))
    }
}

/// The image's base entry. It names the base component, sets its options and
/// records what the probes keep current.
pub struct BaseEntry {
    pub entry: Entry,
    /// Holds the digest that pins the reference.
    pub digest: Option<String>,
    pub signed: bool,
}

/// The base image, and what building on it may assume.
pub struct Base {
    /// Holds the path of the base component.
    pub path: String,
    /// The full image reference, emitted verbatim as the generated `FROM`.
    pub image: String,
    pub family: String,
    /// Capabilities the base image already ships, by name.
    pub provides: Vec<Decl>,
    /// Capabilities the base is unusable without: a container base a component
    /// set turns into a bootc image declares what that component set has to
    /// provide, and `check` refuses the image until something does.
    pub requires: Vec<Decl>,
    /// Holds the components that the base component imports.
    pub imports: Vec<crate::model::component::Import>,
    /// Holds the base component's manifest. An issue about a base import points
    /// into it.
    pub src: Source,
    /// Benchmark rules the base image is published as satisfying, which count
    /// towards `conforms` the way a listed component's claims do.
    pub satisfies: Vec<Coverage>,
    /// Whether the base image publishes a cosign signature.
    pub signed: bool,
    /// Lists the bootloaders an image on this base can install, the default
    /// first.
    pub bootloaders: Vec<String>,
    /// Holds the witness path of each provided name that the base component
    /// locates with `witness=`.
    pub files: Vec<crate::model::component::Located>,
    /// Holds what keeps the base entry's `version` current, where the base
    /// component declares it.
    pub tracker: Option<crate::model::options::VersionTracker>,
    /// Holds the lock entry of a base component that an import copied in.
    pub imported: Option<crate::provenance::lock::Record>,
    /// The SSG datastream file a scan of an image on this base reads, as a
    /// bare filename under the content directory. Empty where the family's
    /// default stands.
    pub scap_content: String,
    pub span: Span,
}

/// What the installer lays down, in place of what the family settles. An
/// installer still asks a person for the disk, the account and the encryption;
/// this is the half that is a property of the image.
pub struct Layout {
    /// The root filesystem, empty where the family's answer stands.
    pub filesystem: String,
    /// `grub2` or `systemd`, empty where the family's answer stands. A family
    /// whose answer is itself empty reads as grub2 in the recipe.
    pub bootloader: String,
    /// `--composefs-backend`, None where the family's answer stands. False is
    /// an answer, so this is not a bare bool.
    pub composefs: Option<bool>,
    /// `--generic-image`, None where the family's answer stands.
    pub generic: Option<bool>,
    /// The group an administrator is created in, empty where the family's
    /// answer stands. `useradd` refuses the whole call when a listed group is
    /// missing, so this names one.
    pub admin_group: String,
    pub span: Span,
}

/// A base declares capabilities and paths the way a component does.
pub use crate::model::component::Decl;

pub struct Flavour {
    pub name: String,
    pub default: bool,
    pub pr_build: bool,
    /// The profile a scan measures this flavour's target against. A flavour is
    /// a different image from the set it extends, so it asks for its own
    /// measurement or gets none.
    pub conforms: String,
    pub span: Span,
}

/// One workflow the repository asks to be generated, named by its file stem
/// under `.github/workflows/`.
pub struct Workflow {
    pub name: String,
    pub span: Span,
    /// Holds the cron line that replaces the one `at` places.
    pub schedule: Option<String>,
}

/// One entry in the list: a component, and the decisions the image author makes
/// about it.
pub struct Entry {
    /// The component's directory, relative to `components/`.
    pub path: String,
    pub flavour: Option<String>,
    pub variant: Option<String>,
    /// Option name to the values set on it.
    pub options: Vec<(String, Vec<Value>, Span)>,
    pub span: Span,
    /// The manifest this entry names, loaded during resolution. None when it
    /// could not be read, which is already an issue.
    pub component: Option<Component>,
    /// Names the component whose `imports` added this entry to the image. It
    /// is None for an entry that the image file lists.
    pub imported_by: Option<String>,
    /// Holds the span of each manifest node of a component that the image
    /// file defines inline. It is None for a component under `components/`.
    pub inline: Option<Vec<Span>>,
    /// Marks a path segment that only holds other segments. If no component
    /// is at its path, then the segment organises and lists nothing.
    pub organises: bool,
}

impl Entry {
    /// Names this component as a seed calls it. An imported component keeps its
    /// library and its path there. A component this repository owns takes the
    /// collection it publishes as.
    pub fn qualified(&self, publishes_as: &str) -> String {
        match self
            .component
            .as_ref()
            .and_then(|component| component.imported.as_ref())
        {
            Some(record) => format!("{}/{}", record.library, record.upstream),
            None => format!("{publishes_as}/{}", self.path),
        }
    }
}

/// The image a repository publishes a seed of, and the collection it publishes
/// its own components as, which is what every component in the seed is named by.
pub struct Seed {
    pub image: String,
    pub collection: String,
}

/// The tool that builds every image in the repository.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Backend {
    #[default]
    Buildah,
    Buildx,
}

impl Backend {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "buildah" => Some(Self::Buildah),
            "buildx" => Some(Self::Buildx),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Buildah => "buildah",
            Self::Buildx => "buildx",
        }
    }
}

/// The repository's declarations: every image in it, and the handful of
/// decisions that are about the repository.
pub struct List {
    /// The optional label repo.kdl gives the repository.
    pub name: String,
    /// The machine form of `name`, used as the heading of change trees.
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
    /// Whether images publish only as part of the scheduled build.
    pub publishes_scheduled: bool,
    /// Whether image scans run only as part of the scheduled build.
    pub scans_scheduled: bool,
    /// The sources this repository reads, of whichever kind.
    pub sources: Vec<Collection>,
    /// Which image a build with nothing named builds, and which one a pull
    /// request builds.
    pub default_image_id: Option<String>,
    pub pr_image_id: Option<String>,
    /// The image this repository publishes a seed of, when it publishes one.
    pub seed: Option<Seed>,
    /// Whether a build stamps the generated manifest onto the image as an OCI label.
    pub manifest_label: bool,
    /// The tool that `tect build` runs when neither its flag nor
    /// `$BUILD_BACKEND` names one. It also selects the build job that the
    /// generated `build.yml` runs.
    pub backend: Backend,
    /// Holds the JSON members that the `renovate` block maps to. If the block
    /// is absent, then `generate` writes no Renovate settings.
    pub renovate: Option<Vec<(String, common::json::Json)>>,
    /// Whether a provenance fact that is missing or does not match is an error.
    /// Every fact is recorded either way.
    pub audit_enforce: bool,
    /// What repo.kdl lets each step of a component layer reach.
    pub network: Network,
    /// What repo.kdl declares, which is `SCHEMA_VERSION` or the load failed.
    pub schema_version: Option<u32>,
    /// repo.kdl, for a diagnostic about either of the two above.
    pub repo_src: Source,
    /// Every file read, in order, for the count line a failure ends with.
    pub files: Vec<String>,
}

/// What repo.kdl lets one kind of step reach.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NetRule {
    #[default]
    Allow,
    /// A step is open only where its component declares the access.
    Strict,
    Deny,
}

impl NetRule {
    pub fn of(word: &str) -> Option<Self> {
        match word {
            "allow" => Some(Self::Allow),
            "strict" => Some(Self::Strict),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// The repository holds one network rule per kind of step.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Network {
    pub packages: NetRule,
    pub scripts: NetRule,
}

impl Network {
    /// A package step exists only where the component declares packages, a COPR
    /// or a repo file, so `strict` opens it wherever it runs.
    pub fn access(&self, component: &Component) -> Access {
        Access {
            packages: self.packages != NetRule::Deny,
            scripts: match self.scripts {
                NetRule::Allow => true,
                NetRule::Strict => component.network.is_some(),
                NetRule::Deny => false,
            },
        }
    }
}

impl Image {
    /// The manifests this image's entries resolved to, in build order.
    pub fn components(&self) -> impl Iterator<Item = &Component> {
        self.entries.iter().filter_map(|e| e.component.as_ref())
    }

    /// Answers whether the image's base meets a requirement.
    pub fn base_answers(&self, required: &str) -> bool {
        self.base
            .as_ref()
            .is_some_and(|base| base.answers(required))
    }

    /// Answers whether the base or an enabled component meets a requirement on
    /// every target. A name that an ungated component removes meets nothing.
    pub fn answers(&self, required: &str) -> bool {
        let kept = |name: &str| self.removing(name, |at| at.is_none()).is_none();
        self.base.as_ref().is_some_and(|base| {
            base.provides
                .iter()
                .any(|decl| crate::resolve::meets(&decl.name, required) && kept(&decl.name))
        }) || self.components().any(|component| {
            crate::resolve::answers(
                &component.path,
                component
                    .provides
                    .iter()
                    .map(|decl| decl.name.as_str())
                    .filter(|name| kept(name)),
                required,
            )
        })
    }

    /// Finds the enabled component that removes a name on a target that an entry
    /// gated to `flavour` builds into. A component that removes a name also
    /// removes every name below it.
    pub fn remover(&self, name: &str, flavour: Option<&str>) -> Option<&Component> {
        self.removing(name, |at| crate::resolve::meet(at, flavour))
    }

    /// Finds the component that removes a name, among those whose flavour
    /// `reaches` accepts.
    fn removing(&self, name: &str, reaches: impl Fn(Option<&str>) -> bool) -> Option<&Component> {
        self.components()
            .find(|component| component.withdraws(name) && reaches(component.flavour.as_deref()))
    }

    /// Answers whether the base meets a requirement on the targets of
    /// `flavour`, once the removals on those targets apply.
    pub fn base_meets(&self, required: &str, flavour: Option<&str>) -> bool {
        self.base.as_ref().is_some_and(|base| {
            base.provides.iter().any(|decl| {
                crate::resolve::meets(&decl.name, required)
                    && self.remover(&decl.name, flavour).is_none()
            })
        })
    }

    /// Answers whether a component meets a requirement on the targets of
    /// `flavour`, once the removals on those targets apply.
    pub fn component_meets(
        &self,
        component: &Component,
        required: &str,
        flavour: Option<&str>,
    ) -> bool {
        crate::resolve::answers(
            &component.path,
            component
                .provides
                .iter()
                .map(|decl| decl.name.as_str())
                .filter(|name| self.remover(name, flavour).is_none()),
            required,
        )
    }

    /// What a scan measures one of this image's targets against: the image's
    /// own `conforms` is the ungated target's alone, and every other target
    /// declares its own or is not measured.
    pub fn conforms_of(&self, flavour: &str) -> &str {
        match flavour {
            NO_FLAVOUR => &self.conforms,
            name => self
                .flavours
                .iter()
                .find(|f| f.name == name)
                .map_or("", |f| f.conforms.as_str()),
        }
    }

    /// Whether any target of this image declares a benchmark profile.
    pub fn declares_conforms(&self) -> bool {
        !self.conforms.is_empty() || self.flavours.iter().any(|f| !f.conforms.is_empty())
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
    /// there: a command given an image never needed one.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_path_admits_one_tag_on_its_last_part() {
        assert!(is_local_path("browser"));
        assert!(is_local_path("browser@1"));
        assert!(is_local_path("desktop/kde@2"));
        assert!(!is_local_path("browser@"));
        assert!(!is_local_path("browser@1@2"));
        assert!(!is_local_path("desktop@1/kde"));
        assert!(!is_local_path("Browser"));
        assert!(!is_local_path("/browser"));
    }
}

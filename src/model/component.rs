//! manifest.kdl: the component author's file.

use crate::diag::{Source, Span};
use crate::model::asset::Asset;
use crate::model::options::{Opt, Value, Variant};
use crate::provenance::lock::Record;

/// A batch of names keyed to a base family, with an optional repo to enable
/// for just this install. Ordinary packages and package groups have the same
/// shape and differ only in the helper the layer calls.
#[derive(Debug)]
pub struct PackageGroup {
    pub family: String,
    pub packages: Vec<String>,
    pub enablerepo: Option<String>,
    pub span: Span,
}

/// A COPR repository this component enables for its own installs. Only the two
/// path segments are declared; every name dnf is told is derived from them.
pub struct Copr {
    pub owner: String,
    pub project: String,
    /// The families a `family` gate names, or empty for every family the
    /// component targets.
    pub family: Vec<String>,
    pub span: Span,
}

impl Copr {
    /// What `dnf5 copr enable` takes, and what an `enablerepo` names it by.
    pub fn name(&self) -> String {
        format!("{}/{}", self.owner, self.project)
    }

    pub fn url(&self) -> String {
        format!(
            "https://copr.fedorainfracloud.org/coprs/{}/{}/",
            self.owner, self.project
        )
    }

    /// What the layer calls the selector, for a component whose install is
    /// conditional and so cannot be declared in `packages`.
    pub fn env_name(&self) -> String {
        format!(
            "COPR_{}",
            self.name()
                .to_uppercase()
                .replace(['/', '-', '.', '+'], "_")
        )
    }

    /// The repository id dnf gives it, left disabled so it is enabled for one
    /// install.
    pub fn selector(&self) -> String {
        format!(
            "copr:copr.fedorainfracloud.org:{}:{}",
            self.owner, self.project
        )
    }
}
/// A benchmark and the rules within it this component claims to satisfy. The
/// tool records the claim; a later phase verifies it with `oscap`.
pub struct Coverage {
    pub benchmark: String,
    pub rules: Vec<String>,
    pub span: Span,
}

/// A capability or contract path, and where it was declared.
pub struct Decl {
    pub name: String,
    pub span: Span,
}

/// Holds one component that this component brings into each image that holds it.
pub struct Import {
    /// Holds the imported component's path. Resolution interpolates this
    /// component's option values into it.
    pub path: String,
    /// Holds the option values that the import passes to the imported component.
    pub options: Vec<(String, Vec<Value>)>,
    pub span: Span,
}

/// Holds what a base declares about the image it names. A component that
/// provides `base-image` is a base.
pub struct BaseFacts {
    /// Holds the image reference. Resolution interpolates the base entry's
    /// option values into it.
    pub image: String,
    /// Holds the image reference as the manifest writes it, before any value
    /// is interpolated.
    pub template: String,
    pub family: String,
    /// Lists the bootloaders an image on this base can install, the default
    /// first.
    pub bootloaders: Vec<String>,
    pub scap_content: String,
}

/// A provided name and the absolute path that witnesses it.
pub struct Located {
    pub name: String,
    pub file: String,
    /// Real while the image builds, and gone from the shipped one because the
    /// providing component removes it again.
    pub build_only: bool,
}

/// A filename this component collects from every other component that ships one,
/// where the build puts them, and where a contribution lands in the result
/// when the contributor says nothing.
pub struct Collect {
    pub file: String,
    pub into: String,
    pub priority: u32,
    pub span: Span,
}

/// Where one contribution lands, for a component that cares.
pub struct Contribution {
    pub file: String,
    pub priority: u32,
    pub span: Span,
}

/// A mode applied after this component's files/ overlay is copied into the image.
pub struct FileMode {
    pub path: String,
    pub mode: u32,
    pub span: Span,
}

/// A key `tect create key` generates for this component: which of the generators
/// the tool has makes it, where the public half goes in the image, and what the
/// private half is called under keys/private/.
pub struct Key {
    pub kind: String,
    pub generator: String,
    /// What the generator is set up for, where it can do more than one thing.
    pub profile: Option<String>,
    pub bits: u32,
    pub public: String,
    /// `pem` or `der`, which is what the public half is written as.
    pub format: String,
    pub private: String,
    pub span: Span,
}

/// One `systemd-analyze verify` diagnostic a component accepts on one of its
/// units, so that a known-benign complaint does not have to be tolerated
/// image-wide.
pub struct VerifyException {
    pub class: String,
    pub unit: String,
    pub span: Span,
}

/// One benchmark rule a component deliberately leaves unsatisfied, which nothing
/// may remediate on its behalf. Written as a rule ID rather than as the number
/// a `satisfies` takes: 710 of the 994 rules carry no number that reaches them,
/// and a rule worth refusing is often one of them.
pub struct Refusal {
    pub rule: String,
    pub because: String,
    pub span: Span,
}

/// Where a component's `Containerfile.inc` is inlined.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Position {
    #[default]
    Before,
    After,
    /// Below the finalize layer, with the lineage stage closed first. A component
    /// whose work runs over the finished rootfs needs a stage to bind it from,
    /// and the lineage stage is only nameable once it has ended.
    Tail,
}

impl Component {
    /// Answers whether this component removes a name or a name above it.
    pub fn withdraws(&self, name: &str) -> bool {
        self.removes
            .iter()
            .any(|decl| crate::resolve::meets(name, &decl.name))
    }

    /// Answers whether this component meets a requirement.
    pub fn answers(&self, required: &str) -> bool {
        crate::resolve::answers(
            &self.path,
            self.provides.iter().map(|decl| decl.name.as_str()),
            required,
        )
    }
}

pub struct Component {
    /// The list path, which is the component's identity everywhere.
    pub path: String,
    /// Where the directory actually is, relative to `components/`.
    pub dir: String,
    pub src: Source,
    pub description: String,
    pub targets: Vec<String>,
    /// Capabilities.
    pub provides: Vec<Decl>,
    /// Names each provided capability that this component declares exclusive. A
    /// second enabled provider of one of them is a conflict.
    pub exclusive: Vec<String>,
    pub requires: Vec<Decl>,
    /// Holds the base facts where this component provides `base-image`.
    pub base: Option<BaseFacts>,
    /// Holds the components that this component brings into the image.
    pub imports: Vec<Import>,
    /// Names the capabilities this component withdraws from the image.
    pub removes: Vec<Decl>,
    /// Soft: ordering and cache preference, never fails.
    pub after: Vec<Decl>,
    /// The MAC capabilities this component ships policy for, read off its
    /// directory: a profile is emitted against whatever the image turned out to
    /// carry, so the component never names one. An ordering edge and nothing else.
    pub policies: Vec<&'static str>,
    /// The file a provided name is witnessed by, where the component states one:
    /// its `witness=`, and every key's public half.
    pub files: Vec<Located>,
    /// Paths this component's files/ overlay knowingly replaces.
    pub overrides: Vec<Decl>,
    /// Verify diagnostics this component's own units are allowed to produce.
    pub verify_exceptions: Vec<VerifyException>,
    pub refuses: Vec<Refusal>,
    /// The flavour this component is gated to, from the list: a component never names
    /// a flavour.
    pub flavour: Option<String>,
    pub collects: Vec<Collect>,
    pub contributes: Vec<Contribution>,
    pub modes: Vec<FileMode>,
    /// Keys this component declares. Each one's `public` is a contract path,
    /// derived.
    pub keys: Vec<Key>,
    /// The `<kind>-key` capability of every key the schema or `parse_key`
    /// refused. The refusal is already reported, so a component that requires the
    /// key gets no second report naming the declaring component as missing.
    pub refused_keys: Vec<String>,
    /// Build inputs the field sets cover, so that needing a secret or a build
    /// arg does not force a component to hand-write a whole RUN block.
    pub secrets: Vec<Decl>,
    pub args: Vec<Decl>,
    pub options: Vec<Opt>,
    pub variants: Vec<Variant>,
    /// Pinned upstream payloads, resolved into env on the layer.
    pub assets: Vec<Asset>,
    /// Packages keyed to base family, installed by the generator before
    /// component.sh runs.
    pub packages: Vec<PackageGroup>,
    /// Package groups keyed to base family, installed after the ordinary
    /// packages and before component.sh runs.
    pub groups: Vec<PackageGroup>,
    /// COPR repositories made reachable before anything is installed. Fedora
    /// only, since COPR is a Fedora build service.
    pub coprs: Vec<Copr>,
    /// Files mounted by basename into /ctx/lib in every standard component layer.
    pub helpers: Vec<Decl>,
    pub satisfies: Vec<Coverage>,
    /// Resolved option name to value, ready to become env on the layer.
    pub resolved: Vec<(String, String)>,
    /// A Containerfile.inc, inlined verbatim, for a component whose needs the
    /// field sets cannot express.
    pub fragment: Option<String>,
    /// Where the fragment goes relative to the generated block.
    pub fragment_position: Position,
    pub standard_layer: bool,
    /// Holds what the component directory hashes to. `plan.json` carries it, so
    /// `verify` fails on an edit that was never regenerated.
    pub content: Option<String>,
    /// Holds the lock entry of a component that `import component` copied in.
    pub imported: Option<Record>,
    /// Whether it ships a `repo.sh` file, which enables a third-party package
    /// repository inside the layer. There is no grammar for one: it is shell
    /// calling the family's config manager.
    pub repo: bool,
    /// Where manifest.kdl declares that its scripts reach the network. A
    /// `strict` repository rule opens the script step only for a component with
    /// this declaration.
    pub network: Option<Span>,
    /// The network each step of this component's layer runs with, once the
    /// repository rule is applied.
    pub access: Access,
}

/// The network each step of one component layer runs with. A `true` step keeps
/// the network, and a `false` step runs under `--network=none`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Access {
    pub packages: bool,
    pub scripts: bool,
}

impl Access {
    pub const OPEN: Self = Self {
        packages: true,
        scripts: true,
    };
}

//! module.kdl: the module author's file.

use crate::diag::{Source, Span};
use crate::model::asset::Asset;
use crate::model::options::{Opt, Variant};
use crate::provenance::record::Record;

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

/// A COPR repository this module enables for its own installs. Only the two
/// path segments are declared; every name dnf is told is derived from them.
pub struct Copr {
    pub owner: String,
    pub project: String,
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

    /// What the layer calls the selector, for a module whose install is
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
    /// install rather than for the shipped image.
    pub fn selector(&self) -> String {
        format!(
            "copr:copr.fedorainfracloud.org:{}:{}",
            self.owner, self.project
        )
    }
}
/// A benchmark and the rules within it this module claims to satisfy. The
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

/// A filename this module collects from every other module that ships one,
/// where the build puts them, and where a contribution lands in the result
/// when the contributor says nothing.
pub struct Collect {
    pub file: String,
    pub into: String,
    pub priority: u32,
    pub span: Span,
}

/// Where one contribution lands, for a module that cares.
pub struct Contribution {
    pub file: String,
    pub priority: u32,
    pub span: Span,
}

/// A mode applied after this module's files/ overlay is copied into the image.
pub struct FileMode {
    pub path: String,
    pub mode: u32,
    pub span: Span,
}

/// A key `tect create key` generates for this module: which of the generators
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

/// One `systemd-analyze verify` diagnostic a module accepts on one of its
/// units, so that a known-benign complaint does not have to be tolerated
/// image-wide.
pub struct VerifyException {
    pub class: String,
    pub unit: String,
    pub span: Span,
}

pub struct Module {
    /// The list path, which is the module's identity everywhere.
    pub path: String,
    /// Where the directory actually is, relative to `modules/`.
    pub dir: String,
    pub src: Source,
    pub description: String,
    pub supports: Vec<String>,
    /// Capabilities.
    pub provides: Vec<Decl>,
    pub requires: Vec<Decl>,
    /// Soft: ordering and cache preference, never fails.
    pub after: Vec<Decl>,
    /// The MAC capabilities this module ships policy for, read off its
    /// directory rather than declared: a profile is emitted against whatever
    /// the image turned out to carry, so the module never names one. It is an
    /// ordering edge and nothing else — an image with no MAC ships no policy
    /// and owes no provider.
    pub policies: Vec<&'static str>,
    /// Exact paths one module writes and another reads.
    pub provides_files: Vec<Decl>,
    /// The subset of `provides_files` declared `build-only=#true`: a real
    /// contract while the image builds, and gone from the shipped one because
    /// the providing module removes it again.
    pub provides_files_build_only: Vec<String>,
    pub requires_files: Vec<Decl>,
    /// Paths this module's files/ overlay knowingly replaces.
    pub overrides: Vec<Decl>,
    /// Verify diagnostics this module's own units are allowed to produce.
    pub verify_exceptions: Vec<VerifyException>,
    /// The flavour this module is gated to, from the list rather than the
    /// manifest: a module never names a flavour.
    pub flavour: Option<String>,
    pub collects: Vec<Collect>,
    pub contributes: Vec<Contribution>,
    pub modes: Vec<FileMode>,
    /// Keys this module declares. Each one's `public` is a contract path,
    /// derived rather than declared a second time.
    pub keys: Vec<Key>,
    /// Build inputs the field sets cover, so that needing a secret or a build
    /// arg does not force a module to hand-write a whole RUN block.
    pub secrets: Vec<Decl>,
    pub args: Vec<Decl>,
    pub options: Vec<Opt>,
    pub variants: Vec<Variant>,
    /// Pinned upstream payloads, resolved into env on the layer.
    pub assets: Vec<Asset>,
    /// Packages keyed to base family, installed by the generator before
    /// module.sh runs.
    pub packages: Vec<PackageGroup>,
    /// Package groups keyed to base family, installed after the ordinary
    /// packages and before module.sh runs.
    pub groups: Vec<PackageGroup>,
    /// COPR repositories this module declares for its package installs.
    /// COPR repositories made reachable before anything is installed. Fedora
    /// only, since COPR is a Fedora build service.
    pub coprs: Vec<Copr>,
    /// Files mounted by basename into /ctx/lib in every standard module layer.
    pub helpers: Vec<Decl>,
    pub satisfies: Vec<Coverage>,
    /// Resolved option name to value, ready to become env on the layer.
    pub resolved: Vec<(String, String)>,
    /// A Containerfile.inc, inlined verbatim, for a module whose needs the
    /// field sets cannot express.
    pub fragment: Option<String>,
    /// Where the fragment goes relative to the generated block, and whether
    /// that block is emitted at all.
    pub fragment_after: bool,
    pub standard_layer: bool,
    /// What the module directory hashes to, every file in it except the import
    /// record. `plan.json` carries it, so `verify` fails on an edit that was
    /// never regenerated.
    pub content: Option<String>,
    /// The record `copy module` left inside it, for a module that was
    /// copied rather than written here.
    pub imported: Option<Record>,
    /// Whether it ships a `repo` file, which enables a third-party package
    /// repository inside the layer. There is no grammar for one: it is shell
    /// calling the family's config manager, and a node restating it would be a
    /// second source of truth that drifts.
    pub repo: bool,
}

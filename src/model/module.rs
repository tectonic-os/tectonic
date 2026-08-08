//! module.kdl: the module author's file.

use crate::diag::{Source, Span};
use crate::model::asset::Asset;
use crate::model::options::{Opt, Variant};

/// A batch of packages keyed to a base family, with an optional repo to enable
/// for just this install.
#[derive(Debug)]
pub struct PackageGroup {
    pub family: String,
    pub packages: Vec<String>,
    pub enablerepo: Option<String>,
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
    /// Resolved option name to value, ready to become env on the layer.
    pub resolved: Vec<(String, String)>,
    /// A Containerfile.inc, inlined verbatim, for a module whose needs the
    /// field sets cannot express.
    pub fragment: Option<String>,
    /// Where the fragment goes relative to the generated block, and whether
    /// that block is emitted at all.
    pub fragment_after: bool,
    pub standard_layer: bool,
}

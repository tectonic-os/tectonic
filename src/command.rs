//! The command surface. clap parses from this tree, `usage` and the picker are
//! renderings of it, and `Verb` is what `dispatch` matches on.

use crate::emit::schema_md::Area;
use clap::{ArgMatches, ColorChoice, CommandFactory, Parser, Subcommand};
use common::ui::Choice;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

mod help;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Upgrade,
    CreateRepo,
    CreateGit,
    CreateImage,
    CreateFlavour,
    CreateModule,
    CreateKey,
    CreateScripts,
    ImportModule,
    CopyModule,
    SetWorkflows,
    SetLibrary,
    SetConforms,
    SetClaims,
    SetKey,
    Check,
    Generate,
    Build,
    VmBuild,
    VmRun,
    VmSpawn,
    Section,
    Graph,
    Why,
    Coverage,
    Plan,
    Verify,
    Summary,
    Sbom,
    FetchModules,
    Scap,
    ScapContent,
    ScapRules,
    ScapTailoring,
    RegistryNamespace,
    RegistryRef,
    Recipe,
    OsRelease,
    BuildRecord,
    Fetch,
    ValidateImage,
}

impl Verb {
    /// The schema that the command writes or reads, as the label the commands
    /// reference shows, the page and the anchor on it. An empty anchor links
    /// the page itself.
    pub fn schema(self) -> Option<(&'static str, Area, &'static str)> {
        match self {
            Verb::CreateRepo => Some(("repository&nbsp;layout", Area::Repository, "")),
            Verb::CreateImage => Some(("`image.kdl`", Area::Image, "")),
            Verb::CreateFlavour => Some((
                "`image.kdl`&nbsp;›&nbsp;`flavours`",
                Area::Image,
                "image-flavours",
            )),
            Verb::CreateModule => Some(("`module.kdl`", Area::Module, "")),
            Verb::CreateKey | Verb::SetKey => {
                Some(("`module.kdl`&nbsp;›&nbsp;`key`", Area::Module, "key"))
            }
            Verb::CreateScripts => Some(("`scripts/`", Area::Repository, "")),
            Verb::ImportModule => Some((
                "`image.kdl`&nbsp;›&nbsp;`source`",
                Area::Image,
                "image-modules-source",
            )),
            Verb::CopyModule => Some(("`provenance.kdl`", Area::Provenance, "")),
            Verb::SetWorkflows => Some((
                "`repo.kdl`&nbsp;›&nbsp;`workflows`",
                Area::Repo,
                "workflows",
            )),
            Verb::SetLibrary => Some(("`repo.kdl`&nbsp;›&nbsp;`sources`", Area::Repo, "sources")),
            Verb::SetConforms | Verb::Coverage => Some((
                "`image.kdl`&nbsp;›&nbsp;`conforms`",
                Area::Image,
                "image-conforms",
            )),
            Verb::SetClaims => Some((
                "`module.kdl`&nbsp;›&nbsp;`satisfies`",
                Area::Module,
                "satisfies",
            )),
            Verb::Generate => Some(("`generated/`", Area::Repository, "")),
            Verb::FetchModules => Some((
                "`image.kdl`&nbsp;›&nbsp;`pin`",
                Area::Image,
                "image-modules-module-pin",
            )),
            Verb::OsRelease => Some(("`image.kdl`&nbsp;›&nbsp;`image`", Area::Image, "image")),
            Verb::Fetch => Some(("`module.kdl`&nbsp;›&nbsp;`asset`", Area::Module, "asset")),
            Verb::ValidateImage => Some((
                "`module.kdl`&nbsp;›&nbsp;`allow-verify`",
                Area::Module,
                "allow-verify",
            )),
            Verb::Upgrade
            | Verb::Check
            | Verb::Build
            | Verb::VmBuild
            | Verb::VmRun
            | Verb::VmSpawn
            | Verb::Section
            | Verb::Graph
            | Verb::Why
            | Verb::Plan
            | Verb::Verify
            | Verb::Summary
            | Verb::Sbom
            | Verb::Scap
            | Verb::ScapContent
            | Verb::ScapRules
            | Verb::ScapTailoring
            | Verb::RegistryNamespace
            | Verb::RegistryRef
            | Verb::Recipe
            | Verb::BuildRecord
            | Verb::CreateGit => None,
        }
    }
}

/// Every variant, so a test fails a verb that has no surface row.
pub const ALL: &[Verb] = &[
    Verb::Upgrade,
    Verb::CreateRepo,
    Verb::CreateGit,
    Verb::CreateImage,
    Verb::CreateFlavour,
    Verb::CreateModule,
    Verb::CreateKey,
    Verb::CreateScripts,
    Verb::ImportModule,
    Verb::CopyModule,
    Verb::SetWorkflows,
    Verb::SetLibrary,
    Verb::SetConforms,
    Verb::SetClaims,
    Verb::SetKey,
    Verb::Check,
    Verb::Generate,
    Verb::Build,
    Verb::VmBuild,
    Verb::VmRun,
    Verb::VmSpawn,
    Verb::Section,
    Verb::Graph,
    Verb::Why,
    Verb::Coverage,
    Verb::Plan,
    Verb::Verify,
    Verb::Summary,
    Verb::Sbom,
    Verb::FetchModules,
    Verb::Scap,
    Verb::ScapContent,
    Verb::ScapRules,
    Verb::ScapTailoring,
    Verb::RegistryNamespace,
    Verb::RegistryRef,
    Verb::Recipe,
    Verb::OsRelease,
    Verb::BuildRecord,
    Verb::Fetch,
    Verb::ValidateImage,
];

/// Where a command runs, which decides what `usage` and the picker show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Anywhere,
    Repo,
    /// The contract a build calls, so the help hides it.
    Script,
    /// Runs inside a build, where the binary reads the image around it.
    Layer,
}

/// What a command is for, which groups the commands in `usage`, the picker and
/// `docs/cli.md` alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topic {
    Tool,
    Repository,
    Image,
    Modules,
    Keys,
    Check,
    Build,
    Ci,
    Inspect,
    Audit,
    Layer,
}

/// The order the groups appear in.
pub const TOPICS: [Topic; 11] = [
    Topic::Repository,
    Topic::Image,
    Topic::Modules,
    Topic::Keys,
    Topic::Check,
    Topic::Build,
    Topic::Ci,
    Topic::Inspect,
    Topic::Audit,
    Topic::Layer,
    Topic::Tool,
];

impl Topic {
    pub fn heading(self) -> &'static str {
        match self {
            Topic::Tool => "CLI tool",
            Topic::Repository => "Repository",
            Topic::Image => "Image",
            Topic::Modules => "Modules",
            Topic::Keys => "Keys",
            Topic::Check => "Check and generate",
            Topic::Build => "Build and boot",
            Topic::Ci => "CI",
            Topic::Inspect => "Inspect",
            Topic::Audit => "Audit",
            Topic::Layer => "Inside a build",
        }
    }
}

/// The place table: one row per leaf, keyed by the words the user types. A test
/// binds every row to the tree and back.
const SURFACE: &[(&str, Verb, Family, bool, Topic)] = &[
    (
        "upgrade",
        Verb::Upgrade,
        Family::Anywhere,
        false,
        Topic::Tool,
    ),
    (
        "create repo",
        Verb::CreateRepo,
        Family::Anywhere,
        false,
        Topic::Repository,
    ),
    (
        "create git",
        Verb::CreateGit,
        Family::Anywhere,
        false,
        Topic::Repository,
    ),
    (
        "create image",
        Verb::CreateImage,
        Family::Repo,
        false,
        Topic::Image,
    ),
    (
        "create flavour",
        Verb::CreateFlavour,
        Family::Repo,
        false,
        Topic::Image,
    ),
    (
        "create module",
        Verb::CreateModule,
        Family::Repo,
        false,
        Topic::Modules,
    ),
    (
        "create key",
        Verb::CreateKey,
        Family::Repo,
        false,
        Topic::Keys,
    ),
    (
        "create scripts",
        Verb::CreateScripts,
        Family::Repo,
        false,
        Topic::Repository,
    ),
    (
        "import module",
        Verb::ImportModule,
        Family::Repo,
        false,
        Topic::Modules,
    ),
    (
        "copy module",
        Verb::CopyModule,
        Family::Repo,
        false,
        Topic::Modules,
    ),
    (
        "set workflows",
        Verb::SetWorkflows,
        Family::Repo,
        false,
        Topic::Ci,
    ),
    (
        "set library",
        Verb::SetLibrary,
        Family::Repo,
        false,
        Topic::Repository,
    ),
    (
        "set conforms",
        Verb::SetConforms,
        Family::Repo,
        false,
        Topic::Audit,
    ),
    (
        "set claims",
        Verb::SetClaims,
        Family::Repo,
        false,
        Topic::Audit,
    ),
    ("set key", Verb::SetKey, Family::Repo, false, Topic::Keys),
    ("check", Verb::Check, Family::Repo, false, Topic::Check),
    (
        "generate",
        Verb::Generate,
        Family::Repo,
        false,
        Topic::Check,
    ),
    ("build", Verb::Build, Family::Repo, false, Topic::Build),
    ("vm build", Verb::VmBuild, Family::Repo, false, Topic::Build),
    ("vm run", Verb::VmRun, Family::Repo, false, Topic::Build),
    ("vm spawn", Verb::VmSpawn, Family::Repo, false, Topic::Build),
    (
        "section",
        Verb::Section,
        Family::Repo,
        false,
        Topic::Inspect,
    ),
    ("graph", Verb::Graph, Family::Repo, false, Topic::Inspect),
    ("why", Verb::Why, Family::Repo, true, Topic::Inspect),
    (
        "coverage",
        Verb::Coverage,
        Family::Repo,
        false,
        Topic::Audit,
    ),
    ("plan", Verb::Plan, Family::Script, true, Topic::Inspect),
    ("verify", Verb::Verify, Family::Script, false, Topic::Check),
    (
        "summary",
        Verb::Summary,
        Family::Script,
        true,
        Topic::Inspect,
    ),
    ("sbom", Verb::Sbom, Family::Script, false, Topic::Inspect),
    (
        "fetch modules",
        Verb::FetchModules,
        Family::Script,
        false,
        Topic::Modules,
    ),
    (
        "scap content",
        Verb::ScapContent,
        Family::Script,
        true,
        Topic::Audit,
    ),
    (
        "scap rules",
        Verb::ScapRules,
        Family::Script,
        true,
        Topic::Audit,
    ),
    (
        "scap tailoring",
        Verb::ScapTailoring,
        Family::Script,
        false,
        Topic::Audit,
    ),
    ("scap", Verb::Scap, Family::Script, false, Topic::Audit),
    (
        "registry namespace",
        Verb::RegistryNamespace,
        Family::Script,
        false,
        Topic::Inspect,
    ),
    (
        "registry ref",
        Verb::RegistryRef,
        Family::Script,
        false,
        Topic::Inspect,
    ),
    (
        "recipe",
        Verb::Recipe,
        Family::Script,
        false,
        Topic::Inspect,
    ),
    (
        "os-release",
        Verb::OsRelease,
        Family::Layer,
        false,
        Topic::Layer,
    ),
    (
        "build-record",
        Verb::BuildRecord,
        Family::Layer,
        false,
        Topic::Layer,
    ),
    ("fetch", Verb::Fetch, Family::Layer, false, Topic::Layer),
    (
        "validate-image",
        Verb::ValidateImage,
        Family::Layer,
        false,
        Topic::Layer,
    ),
];

#[derive(Parser)]
#[command(
    name = "tect",
    about = "the build tool for a bootc image repository",
    long_about = "The build tool for a bootc image repository. With no command, it opens a picker \
                  of the commands that run where it is typed. If no terminal is watching, then it \
                  lists those commands instead.",
    after_long_help = help::ROOT_NOTES,
    disable_help_subcommand = true,
    disable_version_flag = true,
    color = ColorChoice::Never,
    // A global is listed after every command's own flags.
    next_display_order = 100
)]
pub struct Cli {
    /// the repository, else the nearest `repo.kdl` at or above the working
    /// directory
    #[arg(long, global = true, value_name = "dir")]
    pub root: Option<PathBuf>,
    /// ask nothing, using defaults or failing when an answer has no default
    #[arg(long = "no-tui", global = true)]
    pub no_tui: bool,
    #[command(subcommand)]
    pub verb: Option<Surface>,
}

// The tree every word is parsed by. A word whose forms all take a noun parses
// with `what` absent, which is the picker's half. A `///` here would become the
// root command's help.
#[derive(Subcommand)]
pub enum Surface {
    /// replace this tect and its assets with the latest
    #[command(long_about = help::UPGRADE, after_long_help = help::UPGRADE_NOTES)]
    Upgrade,
    /// start a repository, or add an image, flavour, module or key to one
    Create {
        #[command(subcommand)]
        what: Option<CreateWhat>,
    },
    /// reference a collection module from an image
    Import {
        #[command(subcommand)]
        what: Option<ImportWhat>,
    },
    /// copy a collection module into this repository
    Copy {
        #[command(subcommand)]
        what: Option<CopyWhat>,
    },
    /// choose what this repository declares, or record a key
    Set {
        #[command(subcommand)]
        what: Option<SetWhat>,
    },
    /// read every manifest and say what is wrong with it
    #[command(long_about = help::CHECK, after_long_help = help::CHECK_NOTES)]
    Check {
        /// the SSG content, for the conformance read-out
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
    },
    /// write the build files, and list what was written
    #[command(long_about = help::GENERATE, after_long_help = help::GENERATE_NOTES)]
    Generate,
    /// verify the build files, then build the image
    #[command(long_about = help::BUILD, after_long_help = help::BUILD_NOTES)]
    Build {
        /// the image, or `<image>/<flavour>` for a flavour; the default image if absent
        #[arg(value_name = "target")]
        target_arg: Option<String>,
        /// the target, where the positional argument is not used
        #[arg(long, value_name = "target")]
        target: Option<String>,
        /// tag the result; repeatable, and $TAGS adds to it
        #[arg(long, value_name = "tag")]
        tag: Vec<String>,
        /// the KERNEL build arg
        #[arg(long, value_name = "name")]
        kernel: Option<String>,
        /// buildx or buildah, else $BUILD_BACKEND, else buildah
        #[arg(long, value_name = "name")]
        backend: Option<String>,
        /// write an OCI archive instead of loading the image
        #[arg(long = "oci-output", value_name = "path")]
        oci_output: Option<String>,
        /// mount <path> as the build secret <id>; repeatable
        #[arg(long, value_name = "id=path")]
        secret: Vec<String>,
        /// export the layer cache to the registry cache repository
        #[arg(long = "cache-to")]
        cache_to: bool,
        /// do not import the layer cache
        #[arg(long = "no-cache-from")]
        no_cache_from: bool,
    },
    /// turn the built image into a disk, and boot it
    #[command(long_about = help::VM, after_long_help = help::VM_NOTES)]
    Vm {
        #[command(subcommand)]
        what: Option<VmWhat>,
    },
    /// print the Containerfile section an image generates
    #[command(long_about = help::SECTION)]
    Section {
        /// the image; the default image if absent
        #[arg(value_name = "image")]
        image: Option<String>,
    },
    /// print what provides what, and what the base carries
    #[command(long_about = help::GRAPH)]
    Graph {
        /// markdown holding a mermaid diagram by default, or json
        #[arg(long, value_name = "md|json")]
        format: Option<String>,
    },
    /// print one module's trust read-out, byte by byte
    #[command(long_about = help::WHY, after_long_help = help::WHY_NOTES)]
    Why {
        /// the module name
        #[arg(value_name = "module")]
        module: Option<String>,
        /// markdown, the default, or JSON
        #[arg(long, value_name = "md|json")]
        format: Option<String>,
    },
    /// print who claims each rule the image conforms to
    #[command(long_about = help::COVERAGE, after_long_help = help::COVERAGE_NOTES)]
    Coverage {
        /// the image; the default image if absent, or a picker in a terminal
        #[arg(value_name = "image")]
        image: Option<String>,
        /// markdown, the default, or json
        #[arg(long, value_name = "md|json")]
        format: Option<String>,
        /// the SSG content the profile is read out of
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
    },
    /// print every fact this repository derives, as json
    #[command(long_about = help::PLAN)]
    Plan {
        /// the output is JSON with or without it
        #[arg(long)]
        json: bool,
    },
    /// compare the build files byte for byte with what tect writes
    #[command(long_about = help::VERIFY)]
    Verify,
    /// print what one target is made of, as a markdown table
    #[command(long_about = help::SUMMARY)]
    Summary {
        /// the image, or `<image>/<flavour>` for a flavour
        #[arg(value_name = "target")]
        target: Option<String>,
    },
    /// print the pinned payloads one target carries, as SPDX
    #[command(long_about = help::SBOM)]
    Sbom {
        /// the image, or `<image>/<flavour>` for a flavour
        #[arg(value_name = "target")]
        target: Option<String>,
    },
    /// print what one scan says about the target
    #[command(long_about = help::SCAP, after_long_help = help::SCAP_NOTES)]
    Scap {
        #[command(subcommand)]
        sub: Option<ScapWhat>,
        /// the ARF report that the scan wrote
        #[arg(value_name = "arf.xml")]
        arf: Option<String>,
        /// the target, else the ungated one
        #[arg(long, value_name = "target")]
        target: Option<String>,
        /// the SSG content, else the one `scap content` names
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
        /// the last scan's pass set, read then rewritten
        #[arg(long, value_name = "file")]
        baseline: Option<PathBuf>,
        /// what the bare base passed alone, read only
        #[arg(long = "base-scan", value_name = "file")]
        base_scan: Option<PathBuf>,
    },
    /// download one payload, verify it, and place it
    #[command(long_about = help::FETCH, subcommand_negates_reqs = true)]
    Fetch {
        #[command(subcommand)]
        sub: Option<FetchWhat>,
        /// the kind of payload, which is `file`, `tree`, `bin`, `rpm` or `deb`
        #[arg(value_name = "what", required = true)]
        what: Option<String>,
        /// the URL to download
        #[arg(value_name = "url", required = true)]
        url: Option<String>,
        /// the hash that the download must match
        #[arg(value_name = "sha256", required = true)]
        sha256: Option<String>,
        /// the path, the directory or the executable name that the kind places
        #[arg(value_name = "target")]
        target: Option<String>,
        /// for `tree`, more arguments to tar; for `bin`, the file inside the archive
        #[arg(num_args(0..), value_name = "extra")]
        extra: Vec<String>,
    },
    /// print where images publish, and under what reference
    Registry {
        #[command(subcommand)]
        what: Option<RegistryWhat>,
    },
    /// print the installer recipe one target installs from
    #[command(long_about = help::RECIPE, after_long_help = help::RECIPE_NOTES)]
    Recipe {
        /// the target, else the ungated one
        #[arg(long, value_name = "target")]
        target: Option<String>,
        /// the tag, else $DEFAULT_TAG, else latest
        #[arg(long, value_name = "tag")]
        tag: Vec<String>,
        /// the bytes installed, else the published reference
        #[arg(long, value_name = "ref")]
        image: Vec<String>,
    },
    /// write the image identity the build ARGs carry
    #[command(long_about = help::OS_RELEASE)]
    OsRelease,
    /// write the record of what the build resolved
    #[command(long_about = help::BUILD_RECORD)]
    BuildRecord,
    /// run every check a built image has to pass
    #[command(long_about = help::VALIDATE_IMAGE)]
    ValidateImage,
}

#[derive(Subcommand)]
pub enum CreateWhat {
    /// start a repository of images
    #[command(long_about = help::CREATE_REPO, after_long_help = help::CREATE_REPO_NOTES)]
    Repo {
        /// the repository name, which also names its directory
        #[arg(value_name = "name")]
        name: Option<String>,
        /// where the repository is hosted, github.com by default; with `--owner`,
        /// it forms the address every image URL starts from
        #[arg(long, value_name = "domain")]
        host: Option<String>,
        /// the account or organisation on the repository host
        #[arg(long, value_name = "name")]
        owner: Option<String>,
        /// write a first image too; `create image` adds one later
        #[arg(long, value_name = "name")]
        image: Vec<String>,
        /// the bootc image the first image is based on
        #[arg(long, value_name = "ref")]
        base: Option<String>,
    },
    /// initialise git and the ignore file in a directory
    #[command(long_about = help::CREATE_GIT, after_long_help = help::CREATE_GIT_NOTES)]
    Git,
    /// add an image, with its name and the base it builds on
    #[command(long_about = help::CREATE_IMAGE, after_long_help = help::CREATE_IMAGE_NOTES)]
    Image {
        /// the image name, which also names its file
        #[arg(value_name = "name")]
        name: Option<String>,
        /// the account or organisation, where no image already carries one
        #[arg(long, value_name = "name")]
        owner: Option<String>,
        /// any bootc image reference, in the catalog or not; skips the picker
        #[arg(long, value_name = "ref")]
        base: Option<String>,
    },
    /// add a gated module set an image also publishes
    #[command(long_about = help::CREATE_FLAVOUR, after_long_help = help::CREATE_FLAVOUR_NOTES)]
    Flavour {
        /// the flavour name
        #[arg(value_name = "name")]
        name: Option<String>,
        /// the image that publishes the flavour
        #[arg(long, value_name = "name")]
        image: Vec<String>,
    },
    /// write a module, and offer to list it in an image
    #[command(long_about = help::CREATE_MODULE, after_long_help = help::CREATE_MODULE_NOTES)]
    Module {
        /// the module name, which can be a path under `modules/`
        #[arg(value_name = "name")]
        name: Option<String>,
        /// list the module in this image or flavour; repeatable
        #[arg(long, value_name = "name")]
        image: Vec<String>,
        /// a package the module installs; repeatable
        #[arg(long, value_name = "name")]
        pkg: Vec<String>,
        /// one more line in the manifest, such as `--with provides=browser`;
        /// repeatable
        #[arg(long, value_name = "verb=value")]
        with: Vec<String>,
    },
    /// generate a key one of this repository's modules declares
    #[command(long_about = help::CREATE_KEY, after_long_help = help::CREATE_KEY_NOTES)]
    Key {
        /// the kind of key, as a module declares it
        #[arg(value_name = "kind")]
        kind: Option<String>,
        /// which module, where two of them declare the same kind
        #[arg(long, value_name = "name")]
        module: Option<String>,
        /// the certificate common name; the repository directory name by default
        #[arg(long, value_name = "name")]
        cn: Option<String>,
    },
    /// keep a copy of a script tect supplies in scripts/
    #[command(long_about = help::CREATE_SCRIPTS, after_long_help = help::CREATE_SCRIPTS_NOTES)]
    Scripts {
        /// each script to keep
        #[arg(value_name = "name")]
        names: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum ImportWhat {
    /// reference a module from a collection repo.kdl declares
    #[command(long_about = help::IMPORT_MODULE, after_long_help = help::IMPORT_MODULE_NOTES)]
    Module {
        /// one module, as `<name>` or `<owner>/<name>`
        #[arg(value_name = "name")]
        name: Option<String>,
        /// list the module in this image or flavour; repeatable
        #[arg(long, value_name = "name")]
        image: Vec<String>,
        /// the SCAP content the profile offer is read out of; the family's
        /// installed copy by default, and no content is no offer
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
        /// add provider modules for dependencies the selected modules require
        #[arg(long)]
        dependencies: bool,
    },
}

#[derive(Subcommand)]
pub enum CopyWhat {
    /// copy a collection module into this repository
    #[command(long_about = help::COPY_MODULE, after_long_help = help::COPY_MODULE_NOTES)]
    Module {
        /// one module, as `<name>` or `<owner>/<name>`
        #[arg(value_name = "name")]
        name: Option<String>,
        /// list the module in this image or flavour; repeatable
        #[arg(long, value_name = "name")]
        image: Vec<String>,
        /// the SCAP content the profile offer is read out of; the family's
        /// installed copy by default, and no content is no offer
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
        /// add provider modules for dependencies the selected modules require
        #[arg(long)]
        dependencies: bool,
    },
}

#[derive(Subcommand)]
pub enum SetWhat {
    /// choose the CI this repository generates
    #[command(long_about = help::SET_WORKFLOWS, after_long_help = help::SET_WORKFLOWS_NOTES)]
    Workflows,
    /// add a module, base-image or capability library
    #[command(long_about = help::SET_LIBRARY)]
    Library {
        /// the kind of library: `base-images`, `capabilities` or `modules`
        #[arg(value_name = "kind")]
        kind: Option<String>,
    },
    /// choose the benchmark profile an image is measured by
    #[command(long_about = help::SET_CONFORMS, after_long_help = help::SET_CONFORMS_NOTES)]
    Conforms {
        /// the image; the only image if absent, else a picker
        #[arg(value_name = "image")]
        image: Option<String>,
        /// the SCAP content the profile is chosen out of; the installed copy
        /// for the image's family by default
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
    },
    /// choose the benchmark rules a module claims to cover
    #[command(long_about = help::SET_CLAIMS, after_long_help = help::SET_CLAIMS_NOTES)]
    Claims {
        /// the module in the repository
        #[arg(value_name = "module")]
        module: Option<String>,
        /// the SCAP content the rules are read out of; the installed copy for
        /// the module's first declared family by default, and required when
        /// the module declares no family restriction
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
    },
    /// record a key that already exists, in place of generating one
    #[command(long_about = help::SET_KEY, after_long_help = help::SET_KEY_NOTES)]
    Key {
        /// the kind of key, as a module declares it
        #[arg(value_name = "kind")]
        kind: Option<String>,
        /// which module, where two of them declare the same kind
        #[arg(long, value_name = "name")]
        module: Option<String>,
        /// the public half to record
        #[arg(long, value_name = "path")]
        from: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum VmWhat {
    /// convert the built image into a qcow2, raw or iso
    Build {
        /// the disk type, which is `qcow2`, `raw` or `iso`
        #[arg(value_name = "type")]
        kind: Option<String>,
        #[command(flatten)]
        disk: Disk,
    },
    /// boot that disk under qemu, building it if missing
    Run {
        /// the disk type, which is `qcow2`, `raw` or `iso`
        #[arg(value_name = "type")]
        kind: Option<String>,
        #[command(flatten)]
        disk: Disk,
    },
    /// boot a qcow2 or raw disk with systemd-vmspawn
    Spawn {
        /// the disk type, which is `qcow2` or `raw`
        #[arg(value_name = "type")]
        kind: Option<String>,
        #[command(flatten)]
        disk: Disk,
    },
}

// The flags every `vm` noun takes, declared once so their help is too.
#[derive(clap::Args)]
pub struct Disk {
    /// what a rebuild builds, and what an iso installs
    #[arg(long, value_name = "target")]
    target: Option<String>,
    /// the container image to convert, without its tag
    #[arg(long, value_name = "ref")]
    image: Vec<String>,
    /// its tag, else $DEFAULT_TAG, else latest
    #[arg(long, value_name = "tag")]
    tag: Vec<String>,
    /// memory for the virtual machine
    #[arg(long, value_name = "size")]
    ram: Option<String>,
    /// fetch, generate and build the container image first
    #[arg(long)]
    rebuild: bool,
}

#[derive(Subcommand)]
pub enum ScapWhat {
    /// print the datastream the target is measured with
    #[command(long_about = help::SCAP_CONTENT)]
    Content {
        /// the target, else the ungated one
        #[arg(long, value_name = "target")]
        target: Option<String>,
    },
    /// print the tailoring the target is scanned with
    #[command(long_about = help::SCAP_TAILORING, after_long_help = help::SCAP_TAILORING_NOTES)]
    Tailoring {
        /// the target, else the ungated one
        #[arg(long, value_name = "target")]
        target: Option<String>,
        /// the SSG content, else the one `scap content` names
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
    },
    /// print the rule each benchmark number reaches, one per line
    Rules {
        /// the SSG content, else the one `scap content` names
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
        /// a benchmark number, such as `1.1.1.1`
        #[arg(num_args(0..), value_name = "number")]
        number: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum FetchWhat {
    /// fetch every out-of-tree module the images reference
    #[command(long_about = help::FETCH_MODULES, after_long_help = help::FETCH_MODULES_NOTES)]
    Modules,
}

#[derive(Subcommand)]
pub enum RegistryWhat {
    /// print where images publish
    #[command(long_about = help::REGISTRY_NAMESPACE)]
    Namespace,
    /// print the full reference one target publishes under
    #[command(long_about = help::REGISTRY_REF)]
    Ref {
        /// the target, else the ungated one
        #[arg(long, value_name = "target")]
        target: Option<String>,
        /// the tag, else $DEFAULT_TAG, else latest
        #[arg(long, value_name = "tag")]
        tag: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Row {
    pub verb: Verb,
    pub path: String,
    pub label: String,
    pub about: String,
}

fn surface(path: &str) -> Option<(&'static str, Verb, Family, bool, Topic)> {
    SURFACE.iter().copied().find(|(words, ..)| *words == path)
}

/// The identity a command path names, or nothing where the path is a group
/// word like `create` whose rows all take a noun.
pub fn verb(path: &str) -> Option<Verb> {
    surface(path).map(|(_, verb, ..)| verb)
}

pub fn on_host(verb: Verb) -> bool {
    SURFACE
        .iter()
        .find(|(_, row, ..)| *row == verb)
        .is_some_and(|(_, _, _, host, _)| *host)
}

/// The levels from the root to the command that was read, so a flag answers
/// from whichever level declared it.
fn levels(matches: &ArgMatches) -> Vec<&ArgMatches> {
    let mut levels = vec![matches];
    let mut at = matches;
    while let Some((_, sub)) = at.subcommand() {
        levels.push(sub);
        at = sub;
    }
    levels
}

/// The flag's value from the nearest level that declared it, so a noun's own
/// declaration wins over its parent's.
pub fn flag<T: Clone + Send + Sync + 'static>(matches: &ArgMatches, id: &str) -> Option<T> {
    levels(matches)
        .into_iter()
        .rev()
        .find_map(|at| at.try_get_one::<T>(id).ok().flatten().cloned())
}

pub fn flag_all<T: Clone + Send + Sync + 'static>(matches: &ArgMatches, id: &str) -> Vec<T> {
    levels(matches)
        .into_iter()
        .rev()
        .find_map(|at| {
            at.try_get_many::<T>(id)
                .ok()
                .flatten()
                .map(|values| values.cloned().collect())
        })
        .unwrap_or_default()
}

/// The flags a parent level took that the leaf does not declare, which is a
/// refusal and not a silent no-op. A global belongs to every level, so it never
/// lands here; the tree answers what the leaf takes because clap rejects an
/// unknown id only in a debug build.
pub fn ignored(matches: &ArgMatches) -> Vec<String> {
    let tree = Cli::command();
    let mut commands: Vec<&clap::Command> = vec![&tree];
    let mut at = matches;
    while let Some((name, sub)) = at.subcommand() {
        let Some(next) = commands
            .last()
            .expect("a command was just pushed")
            .get_subcommands()
            .find(|command| command.get_name() == name)
        else {
            break;
        };
        commands.push(next);
        at = sub;
    }
    let levels = levels(matches);
    let Some((leaf, above)) = commands.split_last() else {
        return Vec::new();
    };
    let mut ignored = Vec::new();
    for (command, at) in above.iter().zip(levels.iter()) {
        for arg in command.get_arguments() {
            // A global belongs to every command, and the tree the leaf came
            // from did not copy it down.
            if arg.is_global_set() {
                continue;
            }
            let id = arg.get_id().as_str();
            if leaf.get_arguments().any(|own| own.get_id().as_str() == id) {
                continue;
            }
            if at.try_get_raw(id).ok().flatten().is_some() {
                ignored.push(id.replace('_', "-"));
            }
        }
    }
    ignored
}

/// A leaf's placeholder for a usage line, lowercased because a derived tree
/// renders its value names in capitals.
fn placeholder(arg: &clap::Arg) -> String {
    let name = arg
        .get_value_names()
        .and_then(|names| names.first())
        .map(|name| name.to_string().to_lowercase())
        .unwrap_or_else(|| arg.get_id().to_string());
    let many = arg
        .get_num_args()
        .is_some_and(|range| range.max_values() > 1);
    let shaped = match arg.is_required_set() {
        true => format!("<{name}>"),
        false => format!("[{name}]"),
    };
    format!("{shaped}{}", if many { "..." } else { "" })
}

/// The words and the argument, as the user types them.
fn label(path: &str, command: &clap::Command) -> String {
    let mut label = path.to_string();
    for arg in command.get_positionals() {
        label.push(' ');
        label.push_str(&placeholder(arg));
    }
    label
}

fn collect(command: &clap::Command, prefix: &str, rows: &mut Vec<Row>) {
    for sub in command.get_subcommands() {
        let path = match prefix.is_empty() {
            true => sub.get_name().to_string(),
            false => format!("{prefix} {}", sub.get_name()),
        };
        let nested = sub.has_subcommands();
        if nested {
            collect(sub, &path, rows);
        }
        // A command with subcommands is also a form of its own where it takes
        // an argument: `scap <arf.xml>` and `fetch <what> ...` sit beside the
        // nouns a picker would otherwise list.
        if !nested || sub.get_positionals().next().is_some() {
            let (_, verb, ..) = surface(&path).expect("every leaf has a place row");
            rows.push(Row {
                verb,
                label: label(&path, sub),
                about: sub
                    .get_about()
                    .map(|about| about.to_string())
                    .unwrap_or_default(),
                path,
            });
        }
    }
}

fn leaves() -> Vec<Row> {
    let mut rows = Vec::new();
    collect(&Cli::command(), "", &mut rows);
    rows
}

/// The rows a picker offers. With no word, what the user can do now, so the
/// script commands stay out; with a word, that word's nouns.
pub fn rows(word: Option<&str>) -> Vec<Row> {
    let leaves = leaves();
    match word {
        Some(word) => {
            let prefix = format!("{word} ");
            leaves
                .into_iter()
                .filter(|row| row.path.starts_with(&prefix))
                .collect()
        }
        None => leaves.into_iter().filter(|row| shown(&row.path)).collect(),
    }
}

/// Whether `path` runs where `tect` is being typed. A `Layer` command reads
/// the image around it during a build and answers for itself.
pub fn runs_in(path: &str, here: &Context) -> bool {
    let (_, _, family, host, _) = surface(path).expect("every command path has a place row");
    match family {
        Family::Anywhere | Family::Layer => true,
        Family::Repo | Family::Script => match here {
            Context::Repo(_) => true,
            Context::Host => host,
            Context::Loose => false,
        },
    }
}

/// The rows a picker offers here, with the choices that draw them. A row the
/// picker cannot answer with is dropped; `usage` still lists everything. One
/// function returns both, so the drawn list and the index into it cannot
/// disagree. If the rows span more than one topic, then a heading row, which
/// holds no command, opens each group.
pub fn choices(rows: &[Row], here: &Context) -> (Vec<Option<Row>>, Vec<Choice>) {
    let runs: Vec<Row> = rows
        .iter()
        .filter(|row| runs_in(&row.path, here))
        .cloned()
        .collect();
    let groups = grouped(&runs);
    let headed = groups.len() > 1;
    let mut kept = Vec::new();
    let mut drawn = Vec::new();
    for (topic, rows) in groups {
        if headed {
            kept.push(None);
            drawn.push(Choice::new(topic.heading(), "").heading());
        }
        for row in rows {
            kept.push(Some(row.clone()));
            drawn.push(Choice::new(row.label.clone(), row.about.clone()));
        }
    }
    (kept, drawn)
}

const INTRO: &str = "\
# The `tect` CLI

`tect` is the one tool that manages a bootc image repository. The user describes each image \
in KDL files, and `tect` turns those files into the Containerfiles and the CI workflows that \
build the image. `tect` works out the module order, the options, the names and the tags in \
one place, so the build scripts hold no logic of their own and the user can see what goes \
into each image.

## Where it runs

`tect` runs in three places:

- On the user's machine, it scaffolds the repository and its images, modules and keys. It \
checks the KDL files, writes the build files, and builds and boots an image.
- In CI, the generated workflows call it to fetch modules, write the build files, build and \
publish each image, and scan it.
- Inside a build, the generated Containerfile calls it to write the image identity, record \
what the build resolved and check the finished image. A module script can call `tect fetch` \
to download a payload and check it against its hash.

";

/// The schema that `verb` writes or reads, as a link from `docs/cli.md`.
fn link(verb: Verb) -> Option<String> {
    verb.schema().map(|(label, area, at)| match at {
        "" => format!("[{label}](schema/{})", area.file()),
        at => format!("[{label}](schema/{}#{at})", area.file()),
    })
}

/// The schema link of each command that has one, keyed by the words the user
/// types after `tect`.
pub fn schema_links() -> Vec<(String, String)> {
    leaves()
        .into_iter()
        .filter_map(|row| link(row.verb).map(|link| (row.path, link)))
        .collect()
}

/// The commands overview that opens `docs/cli.md`: each group as a table
/// of its commands, each linked to its section of the reference below.
pub fn overview() -> String {
    let mut out = String::from(INTRO);
    globals(&mut out);
    out.push_str("## Commands\n\n");
    for (topic, rows) in grouped(&leaves()) {
        let _ = write!(
            out,
            "### {}\n\n| Command | Description | Schema |\n| --- | --- | --- |\n",
            topic.heading()
        );
        // The bare `tect` is no surface row, because the picker it opens is
        // built from the surface rows.
        if topic == Topic::Tool {
            out.push_str(
                "| [`tect`](#tect) | open a picker of the commands that run here, or list them \
                 where no terminal is watching |  |\n",
            );
        }
        for row in rows {
            let anchor = format!("tect-{}", row.path.replace(' ', "-"));
            let schema = link(row.verb).unwrap_or_default();
            let _ = writeln!(
                out,
                "| [`tect {}`](#{anchor}) | {} | {schema} |",
                row.label, row.about
            );
        }
        out.push('\n');
    }
    out
}

/// The flags that every command takes. The reference lists each command's own
/// flags alone, so the overview lists these once.
fn globals(out: &mut String) {
    out.push_str(
        "## Global options\n\nEvery command takes these options, before or after its own \
         words. [`tect`](#tect) says how a command asks for an answer, and what `--no-tui` \
         changes.\n\n| Option | Description |\n| --- | --- |\n",
    );
    for arg in Cli::command()
        .get_arguments()
        .filter(|arg| arg.is_global_set())
    {
        let long = arg.get_long().expect("a global flag has a long name");
        // A switch still reports a value name, so its action decides whether one shows.
        let value = match (arg.get_action().takes_values(), arg.get_value_names()) {
            (true, Some(names)) => format!(" <{}>", names.join("> <")),
            _ => String::new(),
        };
        let help = arg.get_help().map(ToString::to_string).unwrap_or_default();
        let _ = writeln!(out, "| `--{long}{value}` | {help} |");
    }
    out.push('\n');
}

/// The tails a group word refuses with, as `create` names `repo [name]`.
pub fn takes(word: &str) -> String {
    let tree = Cli::command();
    let Some(command) = tree
        .get_subcommands()
        .find(|command| command.get_name() == word)
    else {
        return String::new();
    };
    let quoted: Vec<String> = command
        .get_subcommands()
        .map(|sub| format!("`{}`", label(sub.get_name(), sub)))
        .collect();
    join_or(&quoted)
}

fn join_or(quoted: &[String]) -> String {
    match quoted.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

pub fn host_labels() -> Vec<String> {
    leaves()
        .into_iter()
        .filter(|row| on_host(row.verb))
        .map(|row| row.label)
        .collect()
}

/// Where `tect` is being run, asked once at the top of a run and passed. A
/// place is not a `Family`: a family says what a command needs, and `why`
/// needs a repository *or* a host, which no family can say.
#[derive(Debug, PartialEq)]
pub enum Context {
    /// A `repo.kdl` here or above, named the way `--root .` names one, so every
    /// path a command prints hangs off it and the user reads `modules/x`. An
    /// absolute path would print where their home is.
    Repo(PathBuf),
    /// A booted tectonic image: `/usr/share/tectonic/` carries the manifest
    /// the build baked and the record it wrote beside it.
    Host,
    Loose,
}

impl Context {
    /// `--root` names a repository outright. Otherwise a repository wins over a
    /// host: it is the one with the source. Without it the baked manifest is
    /// what there is to read.
    pub fn of(root: Option<&Path>) -> Self {
        if let Some(root) = root {
            return Self::Repo(root.to_path_buf());
        }
        let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        if let Some(found) = crate::find_root(&here) {
            let up = here.strip_prefix(&found).map(|d| d.components().count());
            return Self::Repo(match up {
                Ok(0) => PathBuf::from("."),
                Ok(up) => (0..up).map(|_| "..").collect(),
                Err(_) => found,
            });
        }
        match Path::new(crate::provenance::build::MANIFEST).is_file() {
            true => Self::Host,
            false => Self::Loose,
        }
    }
}

const HEAD: &str = "usage: tect [--root <dir>] <command>\n";

const RULE: &str = "\
Every answer a command may ask for has a matching flag. What no flag gave is
asked for, and `--no-tui` asks nothing. An unattended question uses its default
or fails and names the flag when it has no default.

docs/cli.md is the reference. Data goes to stdout and diagnostics to
stderr; exit 1 is the invocation, exit 2 the repository.
";

fn family(path: &str) -> Family {
    surface(path).expect("every command path has a place row").2
}

pub fn topic(path: &str) -> Topic {
    surface(path).expect("every command path has a place row").4
}

/// `rows` in the order of `TOPICS`, each group keeping the order of the tree.
pub fn grouped<'a>(rows: &'a [Row]) -> Vec<(Topic, Vec<&'a Row>)> {
    TOPICS
        .iter()
        .map(|&at| {
            (
                at,
                rows.iter()
                    .filter(|row| topic(&row.path) == at)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, rows)| !rows.is_empty())
        .collect()
}

fn is_host(path: &str) -> bool {
    surface(path).expect("every command path has a place row").3
}

fn shown(path: &str) -> bool {
    matches!(family(path), Family::Anywhere | Family::Repo)
}

/// The whole surface the user is taught. In a repository it groups the commands
/// by topic, and elsewhere by where they run. Unlike the picker, it keeps the
/// rows that will not run here, because a reference teaches what exists and a
/// menu asks what to do now.
pub fn usage(here: &Context) -> String {
    let rows = leaves();
    let kept: Vec<&Row> = rows.iter().filter(|row| shown(&row.path)).collect();
    let width = kept.iter().map(|row| row.label.len()).max().unwrap_or(0);
    let block = |keep: &dyn Fn(&str) -> bool| -> String {
        let rows: Vec<Row> = kept
            .iter()
            .filter(|row| keep(&row.path))
            .map(|row| (*row).clone())
            .collect();
        grouped(&rows)
            .iter()
            .map(|(topic, rows)| {
                let lines: String = rows
                    .iter()
                    .map(|row| format!("  {:width$}  {}\n", row.label, row.about))
                    .collect();
                format!("{}:\n{lines}", topic.heading().to_lowercase())
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let anywhere = block(&|path| family(path) == Family::Anywhere);
    let repo =
        |host: bool| block(&move |path| family(path) == Family::Repo && is_host(path) == host);
    match here {
        // Everything runs here, so one list groups it all by topic.
        Context::Repo(_) => format!("{HEAD}\n{}\n{RULE}", block(&|_| true)),
        Context::Host => format!(
            "{HEAD}\n{anywhere}\nthis is a tectonic image, and it answers these about itself:\n\n\
             {}\nthese read the source tree, and there is none here:\n\n{}\n{RULE}",
            repo(true),
            repo(false),
        ),
        Context::Loose => format!(
            "{HEAD}\n{anywhere}\nthese need a repository, and there is none here or above:\n\n\
             {}\n{RULE}",
            block(&|path| family(path) == Family::Repo)
        ),
    }
}

/// What `run` performs: the commands that read the repository, and `generate`,
/// which writes the generated tree, `scripts/` and the declared workflows.
/// Nothing that edits the declarations, builds, or runs inside a layer comes
/// here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Plan,
    Check,
    Generate,
    Verify,
    Section,
    Graph,
    GraphJson,
    Summary,
    Sbom,
    Why,
    WhyJson,
    Coverage,
    CoverageJson,
}

pub enum Arg {
    Image,
    Target,
    Module,
}

impl Command {
    pub fn arg(self) -> Option<Arg> {
        match self {
            Self::Plan
            | Self::Check
            | Self::Generate
            | Self::Verify
            | Self::Graph
            | Self::GraphJson => None,
            Self::Section | Self::Coverage | Self::CoverageJson => Some(Arg::Image),
            Self::Summary | Self::Sbom => Some(Arg::Target),
            Self::Why | Self::WhyJson => Some(Arg::Module),
        }
    }
}

impl Verb {
    pub fn reads(self) -> Option<Command> {
        Some(match self {
            Self::Plan => Command::Plan,
            Self::Check => Command::Check,
            Self::Generate => Command::Generate,
            Self::Verify => Command::Verify,
            Self::Section => Command::Section,
            Self::Graph => Command::Graph,
            Self::Summary => Command::Summary,
            Self::Sbom => Command::Sbom,
            Self::Why => Command::Why,
            Self::Coverage => Command::Coverage,
            Self::Upgrade
            | Self::CreateRepo
            | Self::CreateGit
            | Self::CreateImage
            | Self::CreateFlavour
            | Self::CreateModule
            | Self::CreateKey
            | Self::CreateScripts
            | Self::ImportModule
            | Self::CopyModule
            | Self::SetWorkflows
            | Self::SetLibrary
            | Self::SetConforms
            | Self::SetClaims
            | Self::SetKey
            | Self::Build
            | Self::VmBuild
            | Self::VmRun
            | Self::VmSpawn
            | Self::FetchModules
            | Self::Scap
            | Self::ScapContent
            | Self::ScapRules
            | Self::ScapTailoring
            | Self::RegistryNamespace
            | Self::RegistryRef
            | Self::Recipe
            | Self::OsRelease
            | Self::BuildRecord
            | Self::Fetch
            | Self::ValidateImage => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    fn paths() -> Vec<String> {
        leaves().into_iter().map(|row| row.path).collect()
    }

    #[test]
    fn every_command_path_has_one_row_and_every_row_names_a_leaf() {
        let tree = paths();
        let mut rows: Vec<&str> = SURFACE.iter().map(|(path, ..)| *path).collect();
        rows.sort_unstable();
        rows.dedup();
        assert_eq!(rows.len(), SURFACE.len(), "a path has two rows");
        let mut sorted = tree.clone();
        sorted.sort();
        assert_eq!(sorted, rows, "the tree and the place table disagree");
        assert_eq!(tree.len(), ALL.len(), "a verb has no path or two");
        for verb in ALL {
            assert_eq!(
                SURFACE.iter().filter(|(_, row, ..)| row == verb).count(),
                1,
                "{verb:?} has no one row"
            );
        }
    }

    #[test]
    fn a_word_whose_forms_all_take_a_noun_has_no_verb_of_its_own() {
        for word in ["create", "import", "copy", "registry", "set", "vm"] {
            assert!(verb(word).is_none(), "{word}");
        }
        for word in ["scap", "fetch", "check", "build"] {
            assert!(verb(word).is_some(), "{word}");
        }
    }

    #[test]
    fn a_parent_flag_reaches_its_noun_at_either_spelling() {
        let matches = Cli::command()
            .try_get_matches_from(["tect", "scap", "--target", "foo", "content"])
            .expect("a flag before the noun parses");
        assert_eq!(flag::<String>(&matches, "target").as_deref(), Some("foo"));

        let matches = Cli::command()
            .try_get_matches_from(["tect", "scap", "rules", "--datastream", "x.xml", "1"])
            .expect("a noun takes its own flag");
        assert_eq!(
            flag::<PathBuf>(&matches, "datastream"),
            Some(PathBuf::from("x.xml"))
        );
    }

    #[test]
    fn a_flag_the_leaf_does_not_take_is_refused() {
        let matches = Cli::command()
            .try_get_matches_from(["tect", "scap", "--datastream", "x.xml", "content"])
            .expect("the parent parses the flag");
        assert_eq!(ignored(&matches), ["datastream"]);

        let matches = Cli::command()
            .try_get_matches_from(["tect", "scap", "--target", "foo", "content"])
            .expect("the leaf takes the flag");
        assert!(ignored(&matches).is_empty());
    }

    #[test]
    fn a_switch_reaches_only_the_command_that_reads_it() {
        Cli::command()
            .try_get_matches_from(["tect", "build", "--cache-to"])
            .expect("build reads the cache switches");
        Cli::command()
            .try_get_matches_from(["tect", "vm", "run", "--rebuild"])
            .expect("the vm nouns read the rebuild switch");
        let error = Cli::command()
            .try_get_matches_from(["tect", "check", "--cache-to"])
            .expect_err("check does not read the cache switches");
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn import_and_copy_take_the_dependencies_switch() {
        for verb in ["import", "copy"] {
            let matches = Cli::command()
                .try_get_matches_from(["tect", verb, "module", "browser", "--dependencies"])
                .unwrap_or_else(|error| panic!("{verb} module refused --dependencies: {error}"));
            assert_eq!(flag::<bool>(&matches, "dependencies"), Some(true));
        }
    }

    #[test]
    fn fetch_takes_its_three_positionals_unless_a_noun_does() {
        Cli::command()
            .try_get_matches_from(["tect", "fetch", "modules"])
            .expect("the noun negates the required positionals");
        let error = Cli::command()
            .try_get_matches_from(["tect", "fetch", "file"])
            .expect_err("the bare form needs its url and sha256");
        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn build_and_recipe_take_their_target_as_a_flag() {
        let matches = Cli::command()
            .try_get_matches_from(["tect", "build", "--target", "foo"])
            .expect("the flag parses");
        assert_eq!(flag::<String>(&matches, "target").as_deref(), Some("foo"));

        let matches = Cli::command()
            .try_get_matches_from(["tect", "build", "foo"])
            .expect("the argument parses");
        let (_, build) = matches.subcommand().expect("build was given");
        assert_eq!(
            build.get_one::<String>("target_arg").map(String::as_str),
            Some("foo")
        );

        Cli::command()
            .try_get_matches_from(["tect", "recipe", "--target", "foo"])
            .expect("recipe takes the flag");
    }

    /// A place decides what runs, and the two renderings read the same
    /// answer. The host arm is the one that cannot be reached from a test
    /// process, since it is a file at an absolute path.
    #[test]
    fn a_place_decides_what_runs_and_the_help_groups_by_it() {
        let repo = Context::Repo(".".into());
        for path in ["why", "check", "upgrade"] {
            assert!(runs_in(path, &repo), "{path}");
        }
        assert!(runs_in("why", &Context::Host) && !runs_in("check", &Context::Host));
        assert!(runs_in("upgrade", &Context::Loose) && !runs_in("why", &Context::Loose));

        // A picker offers what runs and keeps every description; the help
        // keeps every row and groups them.
        let (kept, drawn) = choices(&rows(None), &Context::Loose);
        assert!(kept
            .iter()
            .flatten()
            .all(|row| surface(&row.path)
                .is_some_and(|(_, _, family, ..)| family == Family::Anywhere)));
        assert_eq!(kept.len(), drawn.len());
        let (kept, _) = choices(&rows(None), &Context::Host);
        let verbs: Vec<Verb> = kept.iter().flatten().map(|row| row.verb).collect();
        assert!(verbs.contains(&Verb::Why) && !verbs.contains(&Verb::Check));

        let host = usage(&Context::Host);
        let (answers, rest) = host
            .split_once("these read the source tree")
            .expect("the host help groups what it can answer");
        assert!(answers.contains("  why [module]"), "{host}");
        assert!(!answers.contains("  check "), "{host}");
        assert!(rest.contains("  check "), "{host}");
        // The reference still teaches the whole surface, wherever it is read.
        for row in rows(None) {
            for here in [Context::Repo(".".into()), Context::Host, Context::Loose] {
                assert!(usage(&here).contains(&row.label), "{}", row.label);
            }
        }
    }

    #[test]
    fn a_refusal_lists_the_tails_a_word_wanted() {
        assert_eq!(takes("registry"), "`namespace` or `ref`");
        assert_eq!(takes("import"), "`module [name]`");
        assert!(takes("create").starts_with("`repo [name]`"));
    }

    #[test]
    fn every_schema_link_lands_on_an_anchor_its_page_holds() {
        for verb in ALL {
            let Some((label, area, at)) = verb.schema() else {
                continue;
            };
            let page = crate::emit::schema_md::page(area);
            assert!(
                at.is_empty() || page.contains(&format!("<a id=\"{at}\"></a>")),
                "{verb:?} links {label} to `{at}`, which {} does not hold",
                area.file()
            );
        }
    }

    #[test]
    fn the_picker_heads_each_topic_only_where_rows_span_several() {
        let repo = Context::Repo(".".into());
        let (_, drawn) = choices(&rows(None), &repo);
        let headings: Vec<&str> = drawn
            .iter()
            .filter(|choice| choice.heading)
            .map(|choice| choice.label.as_str())
            .collect();
        assert_eq!(headings.last(), Some(&"CLI tool"), "{headings:?}");
        assert!(headings.contains(&"Modules"), "{headings:?}");
        let (_, drawn) = choices(&rows(Some("vm")), &repo);
        assert!(drawn.iter().all(|choice| !choice.heading));
    }

    #[test]
    fn the_picker_and_prompt_rows_come_from_the_tree() {
        let repo = Context::Repo(".".into());
        let (kept, drawn) = choices(&rows(Some("create")), &repo);
        assert_eq!(kept.len(), drawn.len());
        assert!(kept
            .iter()
            .flatten()
            .all(|row| row.path.starts_with("create ")));
        // A heading row holds no command, and every command row carries its
        // description.
        for (row, choice) in kept.iter().zip(&drawn) {
            assert_eq!(row.is_none(), choice.heading, "{}", choice.label);
            assert!(
                row.is_none() || !choice.detail.is_empty(),
                "{}",
                choice.label
            );
        }
        assert_eq!(
            host_labels(),
            [
                "why [module]",
                "plan",
                "summary [target]",
                "scap content",
                "scap rules [number]..."
            ]
        );
    }
}

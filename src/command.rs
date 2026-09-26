//! The command surface. clap parses from this tree, `usage` and the picker are
//! renderings of it, and `Verb` is what `dispatch` matches on.

use clap::{ArgMatches, ColorChoice, CommandFactory, Parser, Subcommand};
use common::ui::Choice;
use std::path::{Path, PathBuf};

mod help;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Upgrade,
    CreateRepo,
    CreateImage,
    CreateFlavour,
    CreateModule,
    CreateKey,
    ImportModule,
    CopyModule,
    SetWorkflows,
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

/// Every variant, so a test fails a verb that has no surface row.
pub const ALL: &[Verb] = &[
    Verb::Upgrade,
    Verb::CreateRepo,
    Verb::CreateImage,
    Verb::CreateFlavour,
    Verb::CreateModule,
    Verb::CreateKey,
    Verb::ImportModule,
    Verb::CopyModule,
    Verb::SetWorkflows,
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

/// The place table: one row per leaf, keyed by the words the user types. A test
/// binds every row to the tree and back.
const SURFACE: &[(&str, Verb, Family, bool)] = &[
    ("upgrade", Verb::Upgrade, Family::Anywhere, false),
    ("create repo", Verb::CreateRepo, Family::Anywhere, false),
    ("create image", Verb::CreateImage, Family::Repo, false),
    ("create flavour", Verb::CreateFlavour, Family::Repo, false),
    ("create module", Verb::CreateModule, Family::Repo, false),
    ("create key", Verb::CreateKey, Family::Repo, false),
    ("import module", Verb::ImportModule, Family::Repo, false),
    ("copy module", Verb::CopyModule, Family::Repo, false),
    ("set workflows", Verb::SetWorkflows, Family::Repo, false),
    ("set conforms", Verb::SetConforms, Family::Repo, false),
    ("set claims", Verb::SetClaims, Family::Repo, false),
    ("set key", Verb::SetKey, Family::Repo, false),
    ("check", Verb::Check, Family::Repo, false),
    ("generate", Verb::Generate, Family::Repo, false),
    ("build", Verb::Build, Family::Repo, false),
    ("vm build", Verb::VmBuild, Family::Repo, false),
    ("vm run", Verb::VmRun, Family::Repo, false),
    ("vm spawn", Verb::VmSpawn, Family::Repo, false),
    ("section", Verb::Section, Family::Repo, false),
    ("graph", Verb::Graph, Family::Repo, false),
    ("why", Verb::Why, Family::Repo, true),
    ("coverage", Verb::Coverage, Family::Repo, false),
    ("plan", Verb::Plan, Family::Script, true),
    ("verify", Verb::Verify, Family::Script, false),
    ("summary", Verb::Summary, Family::Script, true),
    ("sbom", Verb::Sbom, Family::Script, false),
    ("fetch modules", Verb::FetchModules, Family::Script, false),
    ("scap content", Verb::ScapContent, Family::Script, true),
    ("scap rules", Verb::ScapRules, Family::Script, true),
    ("scap tailoring", Verb::ScapTailoring, Family::Script, false),
    ("scap", Verb::Scap, Family::Script, false),
    (
        "registry namespace",
        Verb::RegistryNamespace,
        Family::Script,
        false,
    ),
    ("registry ref", Verb::RegistryRef, Family::Script, false),
    ("recipe", Verb::Recipe, Family::Script, false),
    ("os-release", Verb::OsRelease, Family::Layer, false),
    ("build-record", Verb::BuildRecord, Family::Layer, false),
    ("fetch", Verb::Fetch, Family::Layer, false),
    ("validate-image", Verb::ValidateImage, Family::Layer, false),
];

#[derive(Parser)]
#[command(
    name = "tect",
    about = "the build tool for a bootc image repository",
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
    /// ask nothing, and fail naming the flag a missing answer needs
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
        #[arg(value_name = "module")]
        module: Option<String>,
        /// markdown, the default, or JSON
        #[arg(long, value_name = "md|json")]
        format: Option<String>,
    },
    /// print who claims each rule the image conforms to
    #[command(long_about = help::COVERAGE, after_long_help = help::COVERAGE_NOTES)]
    Coverage {
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
    /// byte-compare what is generated against what is committed
    #[command(long_about = help::VERIFY)]
    Verify,
    /// print what one target is made of, as a markdown table
    #[command(long_about = help::SUMMARY)]
    Summary {
        #[arg(value_name = "target")]
        target: Option<String>,
    },
    /// print the pinned payloads one target carries, as SPDX
    #[command(long_about = help::SBOM)]
    Sbom {
        #[arg(value_name = "target")]
        target: Option<String>,
    },
    /// print what one scan says about the target
    #[command(long_about = help::SCAP, after_long_help = help::SCAP_NOTES)]
    Scap {
        #[command(subcommand)]
        sub: Option<ScapWhat>,
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
        #[arg(value_name = "what", required = true)]
        what: Option<String>,
        #[arg(value_name = "url", required = true)]
        url: Option<String>,
        #[arg(value_name = "sha256", required = true)]
        sha256: Option<String>,
        #[arg(value_name = "target")]
        target: Option<String>,
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
    /// start a repository for your own images
    #[command(long_about = help::CREATE_REPO, after_long_help = help::CREATE_REPO_NOTES)]
    Repo {
        #[arg(value_name = "name")]
        name: Option<String>,
        /// where the repository is hosted; github.com by default
        #[arg(long, value_name = "domain")]
        host: Option<String>,
        /// your account or org on the repository host
        #[arg(long, value_name = "name")]
        owner: Option<String>,
        /// write a first image too; `create image` adds one later
        #[arg(long, value_name = "name")]
        image: Vec<String>,
        /// the bootc image the first image is based on
        #[arg(long, value_name = "ref")]
        base: Option<String>,
    },
    /// add an image: its name, and what it builds on
    #[command(long_about = help::CREATE_IMAGE, after_long_help = help::CREATE_IMAGE_NOTES)]
    Image {
        #[arg(value_name = "name")]
        name: Option<String>,
        /// your account or org, where no image already carries one
        #[arg(long, value_name = "name")]
        owner: Option<String>,
        /// the bootc image this image is based on; skips the picker
        #[arg(long, value_name = "ref")]
        base: Option<String>,
    },
    /// add a gated module set an image also publishes
    #[command(long_about = help::CREATE_FLAVOUR, after_long_help = help::CREATE_FLAVOUR_NOTES)]
    Flavour {
        #[arg(value_name = "name")]
        name: Option<String>,
        /// the image that publishes the flavour
        #[arg(long, value_name = "name")]
        image: Vec<String>,
    },
    /// write a module, and offer to list it in an image
    #[command(long_about = help::CREATE_MODULE, after_long_help = help::CREATE_MODULE_NOTES)]
    Module {
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
        #[arg(value_name = "kind")]
        kind: Option<String>,
        /// which module, where two of them declare the same kind
        #[arg(long, value_name = "name")]
        module: Option<String>,
        /// the certificate common name; the repository directory name by default
        #[arg(long, value_name = "name")]
        cn: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ImportWhat {
    /// reference a module from a collection repo.kdl declares
    #[command(long_about = help::IMPORT_MODULE, after_long_help = help::IMPORT_MODULE_NOTES)]
    Module {
        #[arg(value_name = "name")]
        name: Option<String>,
        /// list the module in this image or flavour; repeatable
        #[arg(long, value_name = "name")]
        image: Vec<String>,
        /// the SCAP content the profile offer is read out of; the family's
        /// installed copy by default, and no content is no offer
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum CopyWhat {
    /// copy a collection module into this repository
    #[command(long_about = help::COPY_MODULE)]
    Module {
        #[arg(value_name = "name")]
        name: Option<String>,
        /// list the module in this image or flavour; repeatable
        #[arg(long, value_name = "name")]
        image: Vec<String>,
        /// the SCAP content the profile offer is read out of; the family's
        /// installed copy by default, and no content is no offer
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum SetWhat {
    /// choose the CI this repository generates
    #[command(long_about = help::SET_WORKFLOWS, after_long_help = help::SET_WORKFLOWS_NOTES)]
    Workflows,
    /// choose the benchmark profile an image is measured by
    #[command(long_about = help::SET_CONFORMS, after_long_help = help::SET_CONFORMS_NOTES)]
    Conforms {
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
        #[arg(value_name = "module")]
        module: Option<String>,
        /// the SCAP content the rules are read out of; the installed copy for
        /// the module's family by default
        #[arg(long, value_name = "file")]
        datastream: Option<PathBuf>,
    },
    /// record a key you already hold, in place of generating one
    #[command(long_about = help::SET_KEY, after_long_help = help::SET_KEY_NOTES)]
    Key {
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
        #[arg(value_name = "type")]
        kind: Option<String>,
        #[command(flatten)]
        disk: Disk,
    },
    /// boot that disk under qemu, building it if missing
    Run {
        #[arg(value_name = "type")]
        kind: Option<String>,
        #[command(flatten)]
        disk: Disk,
    },
    /// boot a qcow2 or raw disk with systemd-vmspawn
    Spawn {
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

fn surface(path: &str) -> Option<(&'static str, Verb, Family, bool)> {
    SURFACE.iter().copied().find(|(words, ..)| *words == path)
}

/// The identity a command path names, or nothing where the path is a group
/// word like `create` whose rows all take a noun.
pub fn verb(path: &str) -> Option<Verb> {
    surface(path).map(|(_, verb, _, _)| verb)
}

pub fn on_host(verb: Verb) -> bool {
    SURFACE
        .iter()
        .find(|(_, row, _, _)| *row == verb)
        .is_some_and(|(.., host)| *host)
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
            let (_, verb, _, _) = surface(&path).expect("every leaf has a place row");
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
    let (_, _, family, host) = surface(path).expect("every command path has a place row");
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
/// disagree.
pub fn choices(rows: &[Row], here: &Context) -> (Vec<Row>, Vec<Choice>) {
    let kept: Vec<Row> = rows
        .iter()
        .filter(|row| runs_in(&row.path, here))
        .cloned()
        .collect();
    let drawn = kept
        .iter()
        .map(|row| Choice::new(row.label.clone(), row.about.clone()))
        .collect();
    (kept, drawn)
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
Every command takes a flag for everything it needs. What no flag gave is asked
for, and `--no-tui` asks nothing, failing and naming the flag instead.

docs/commands.md is the reference. Data goes to stdout and diagnostics to
stderr; exit 1 is the invocation, exit 2 the repository.
";

fn family(path: &str) -> Family {
    surface(path).expect("every command path has a place row").2
}

fn is_host(path: &str) -> bool {
    surface(path).expect("every command path has a place row").3
}

fn shown(path: &str) -> bool {
    matches!(family(path), Family::Anywhere | Family::Repo)
}

/// The whole surface the user is taught, grouped by where it runs. Unlike the
/// picker, it keeps the rows that will not run here: a reference teaches what
/// exists, and a menu asks what to do now.
pub fn usage(here: &Context) -> String {
    let rows = leaves();
    let kept: Vec<&Row> = rows.iter().filter(|row| shown(&row.path)).collect();
    let width = kept.iter().map(|row| row.label.len()).max().unwrap_or(0);
    let block = |keep: &dyn Fn(&str) -> bool| -> String {
        kept.iter()
            .filter(|row| keep(&row.path))
            .map(|row| format!("  {:width$}  {}\n", row.label, row.about))
            .collect()
    };
    let anywhere = block(&|path| family(path) == Family::Anywhere);
    let repo =
        |host: bool| block(&move |path| family(path) == Family::Repo && is_host(path) == host);
    match here {
        Context::Repo(_) => format!(
            "{HEAD}\n{anywhere}{}\n{RULE}",
            block(&|path| family(path) == Family::Repo)
        ),
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
            | Self::CreateImage
            | Self::CreateFlavour
            | Self::CreateModule
            | Self::CreateKey
            | Self::ImportModule
            | Self::CopyModule
            | Self::SetWorkflows
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
                SURFACE.iter().filter(|(_, row, _, _)| row == verb).count(),
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
            .all(|row| surface(&row.path)
                .is_some_and(|(_, _, family, _)| family == Family::Anywhere)));
        assert_eq!(kept.len(), drawn.len());
        let (kept, _) = choices(&rows(None), &Context::Host);
        let verbs: Vec<Verb> = kept.iter().map(|row| row.verb).collect();
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
    fn the_picker_and_prompt_rows_come_from_the_tree() {
        let repo = Context::Repo(".".into());
        let (kept, drawn) = choices(&rows(Some("create")), &repo);
        assert_eq!(kept.len(), drawn.len());
        assert!(kept.iter().all(|row| row.path.starts_with("create ")));
        assert!(drawn.iter().all(|choice| !choice.detail.is_empty()));
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

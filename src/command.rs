//! The command surface as one table. `usage` and the picker are two renderings
//! of it, resolution reads it, and every dispatch arm answers a `Verb` that
//! came out of it.

use crate::ui::Choice;
use std::path::{Path, PathBuf};

/// Every command word the binary answers, one variant per dispatch arm.
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
    RegistryNamespace,
    RegistryRef,
    OsRelease,
    BuildRecord,
    Fetch,
    ValidateImage,
}

/// Beside the enum, so a variant with no row is a failed test rather than a
/// word nothing resolves to.
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
    Verb::RegistryNamespace,
    Verb::RegistryRef,
    Verb::OsRelease,
    Verb::BuildRecord,
    Verb::Fetch,
    Verb::ValidateImage,
];

/// Who runs it, which is what decides whether it is in the help a person reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Runs wherever it is typed, repository or not.
    Anywhere,
    /// Needs a repository at or above the working directory.
    Repo,
    /// The contract a build runs against, not in the help.
    Script,
    /// Reads the image around it, where the binary is mounted into a build.
    Layer,
}

/// One row: the words that name it, what it takes, and what it is for.
#[derive(Debug)]
pub struct Spec {
    pub verb: Verb,
    pub word: &'static str,
    /// The word after it, empty where the command is one word.
    pub noun: &'static str,
    /// What follows the words, for the line the help draws.
    pub arg: &'static str,
    /// One line, and the same line the picker puts beside the label.
    pub about: &'static str,
    pub family: Family,
    /// Whether a booted tectonic image answers it, off the two documents the
    /// build baked. A place is not a family, so this is a column rather than a
    /// fifth `Family`: `why` needs a repository *or* a host.
    pub host: bool,
    /// The flags it reads. One it does not is a failure, not a silent no-op.
    pub takes: &'static [&'static str],
}

impl Spec {
    /// The words and the argument, as a person types them.
    pub fn label(&self) -> String {
        [self.word, self.noun, self.arg]
            .iter()
            .filter(|part| !part.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The words alone, which is what a refusal calls the command.
    pub fn name(&self) -> String {
        match self.noun.is_empty() {
            true => self.word.to_string(),
            false => format!("{} {}", self.word, self.noun),
        }
    }

    /// Whether it runs where `tect` is being typed. A `Layer` command reads
    /// the image around it during a build and answers for itself.
    pub fn runs_in(&self, here: &Context) -> bool {
        match self.family {
            Family::Anywhere | Family::Layer => true,
            Family::Repo | Family::Script => match here {
                Context::Repo(_) => true,
                Context::Host => self.host,
                Context::Loose => false,
            },
        }
    }

    /// The noun and the argument, which is what a picker of one verb's nouns
    /// shows.
    pub fn tail(&self) -> String {
        [self.noun, self.arg]
            .iter()
            .filter(|part| !part.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

const ROOT: &[&str] = &["root"];
/// `vm.sh`'s own flags, under the names the rest of the surface uses.
const VM: &[&str] = &["root", "target", "image", "tag", "ram"];

pub const COMMANDS: &[Spec] = &[
    Spec {
        verb: Verb::Upgrade,
        word: "upgrade",
        noun: "",
        arg: "",
        about: "replace this tect and its assets with the latest",
        family: Family::Anywhere,
        host: false,
        takes: &[],
    },
    Spec {
        verb: Verb::CreateRepo,
        word: "create",
        noun: "repo",
        arg: "[name]",
        about: "start a repository for your own images",
        family: Family::Anywhere,
        host: false,
        takes: &["root", "host", "owner", "image", "base"],
    },
    Spec {
        verb: Verb::CreateImage,
        word: "create",
        noun: "image",
        arg: "[name]",
        about: "add an image: its name, and what it builds on",
        family: Family::Repo,
        host: false,
        takes: &["root", "owner", "base"],
    },
    Spec {
        verb: Verb::CreateFlavour,
        word: "create",
        noun: "flavour",
        arg: "[name]",
        about: "add a gated module set an image also publishes",
        family: Family::Repo,
        host: false,
        takes: &["root", "image"],
    },
    Spec {
        verb: Verb::CreateModule,
        word: "create",
        noun: "module",
        arg: "[name]",
        about: "write a module, and offer to list it in an image",
        family: Family::Repo,
        host: false,
        takes: &["root", "image", "pkg", "with"],
    },
    Spec {
        verb: Verb::ImportModule,
        word: "import",
        noun: "module",
        arg: "[name]",
        about: "reference a module from a collection repo.kdl declares",
        family: Family::Repo,
        host: false,
        takes: &["root", "image", "datastream"],
    },
    Spec {
        verb: Verb::CopyModule,
        word: "copy",
        noun: "module",
        arg: "[name]",
        about: "copy a collection module into this repository",
        family: Family::Repo,
        host: false,
        takes: &["root", "image", "datastream"],
    },
    Spec {
        verb: Verb::CreateKey,
        word: "create",
        noun: "key",
        arg: "<kind>",
        about: "generate a key one of this repository's modules declares",
        family: Family::Repo,
        host: false,
        takes: &["root", "module", "cn"],
    },
    Spec {
        verb: Verb::SetWorkflows,
        word: "set",
        noun: "workflows",
        arg: "",
        about: "choose the CI this repository generates",
        family: Family::Repo,
        host: false,
        takes: ROOT,
    },
    Spec {
        verb: Verb::SetConforms,
        word: "set",
        noun: "conforms",
        arg: "[image]",
        about: "choose the benchmark profile an image is measured by",
        family: Family::Repo,
        host: false,
        takes: &["root", "datastream"],
    },
    Spec {
        verb: Verb::SetClaims,
        word: "set",
        noun: "claims",
        arg: "<module>",
        about: "choose the benchmark rules a module claims to cover",
        family: Family::Repo,
        host: false,
        takes: &["root", "datastream"],
    },
    Spec {
        verb: Verb::Check,
        word: "check",
        noun: "",
        arg: "",
        about: "read every manifest and say what is wrong with it",
        family: Family::Repo,
        host: false,
        takes: &["root", "datastream"],
    },
    Spec {
        verb: Verb::Generate,
        word: "generate",
        noun: "",
        arg: "",
        about: "write the build files, and list what was written",
        family: Family::Repo,
        host: false,
        takes: ROOT,
    },
    Spec {
        verb: Verb::Build,
        word: "build",
        noun: "",
        arg: "[target]",
        about: "verify the build files, then build the image",
        family: Family::Repo,
        host: false,
        takes: &[
            "root",
            "target",
            "tag",
            "kernel",
            "backend",
            "oci-output",
            "secret",
        ],
    },
    Spec {
        verb: Verb::VmBuild,
        word: "vm",
        noun: "build",
        arg: "<type>",
        about: "convert the built image into a qcow2, raw or iso",
        family: Family::Repo,
        host: false,
        takes: VM,
    },
    Spec {
        verb: Verb::VmRun,
        word: "vm",
        noun: "run",
        arg: "<type>",
        about: "boot that disk under qemu, building it if missing",
        family: Family::Repo,
        host: false,
        takes: VM,
    },
    Spec {
        verb: Verb::VmSpawn,
        word: "vm",
        noun: "spawn",
        arg: "<type>",
        about: "boot a qcow2 or raw disk with systemd-vmspawn",
        family: Family::Repo,
        host: false,
        takes: VM,
    },
    Spec {
        verb: Verb::Section,
        word: "section",
        noun: "",
        arg: "[image]",
        about: "print the Containerfile section an image generates",
        family: Family::Repo,
        host: false,
        takes: ROOT,
    },
    Spec {
        verb: Verb::Graph,
        word: "graph",
        noun: "",
        arg: "",
        about: "print what provides what, and what the base carries",
        family: Family::Repo,
        host: false,
        takes: &["root", "format"],
    },
    Spec {
        verb: Verb::Why,
        word: "why",
        noun: "",
        arg: "[module]",
        about: "print one module's trust read-out, byte by byte",
        family: Family::Repo,
        host: true,
        takes: &["root", "format"],
    },
    Spec {
        verb: Verb::Coverage,
        word: "coverage",
        noun: "",
        arg: "[image]",
        about: "print who claims each rule the image conforms to",
        family: Family::Repo,
        host: false,
        takes: &["root", "format", "datastream"],
    },
    Spec {
        verb: Verb::Plan,
        word: "plan",
        noun: "",
        arg: "",
        about: "print every fact this repository derives, as json",
        family: Family::Script,
        host: true,
        takes: ROOT,
    },
    Spec {
        verb: Verb::Verify,
        word: "verify",
        noun: "",
        arg: "",
        about: "byte-compare what is generated against what is committed",
        family: Family::Script,
        host: false,
        takes: ROOT,
    },
    Spec {
        verb: Verb::Summary,
        word: "summary",
        noun: "",
        arg: "[target]",
        about: "print what one target is made of, as a markdown table",
        family: Family::Script,
        host: true,
        takes: ROOT,
    },
    Spec {
        verb: Verb::Sbom,
        word: "sbom",
        noun: "",
        arg: "[target]",
        about: "print the pinned payloads one target carries, as SPDX",
        family: Family::Script,
        host: false,
        takes: ROOT,
    },
    Spec {
        verb: Verb::FetchModules,
        word: "fetch",
        noun: "modules",
        arg: "",
        about: "fetch every out-of-tree module the images reference",
        family: Family::Script,
        host: false,
        takes: ROOT,
    },
    Spec {
        verb: Verb::ScapContent,
        word: "scap",
        noun: "content",
        arg: "",
        about: "print the datastream the target is measured with",
        family: Family::Script,
        host: true,
        takes: &["root", "target"],
    },
    Spec {
        verb: Verb::Scap,
        word: "scap",
        noun: "",
        arg: "<arf.xml>",
        about: "print what one scan says about the target",
        family: Family::Script,
        host: false,
        takes: &["root", "target", "datastream", "baseline", "base-scan"],
    },
    Spec {
        verb: Verb::RegistryNamespace,
        word: "registry",
        noun: "namespace",
        arg: "",
        about: "print where images publish",
        family: Family::Script,
        host: false,
        takes: ROOT,
    },
    Spec {
        verb: Verb::RegistryRef,
        word: "registry",
        noun: "ref",
        arg: "",
        about: "print the full reference one target publishes under",
        family: Family::Script,
        host: false,
        takes: &["root", "target", "tag"],
    },
    Spec {
        verb: Verb::OsRelease,
        word: "os-release",
        noun: "",
        arg: "",
        about: "write the image identity the build ARGs carry",
        family: Family::Layer,
        host: false,
        takes: &[],
    },
    Spec {
        verb: Verb::BuildRecord,
        word: "build-record",
        noun: "",
        arg: "",
        about: "write the record of what the build resolved",
        family: Family::Layer,
        host: false,
        takes: &[],
    },
    Spec {
        verb: Verb::Fetch,
        word: "fetch",
        noun: "",
        arg: "<what> <url> <sha256> [target] [extra...]",
        about: "download one payload, verify it, and place it",
        family: Family::Layer,
        host: false,
        takes: &[],
    },
    Spec {
        verb: Verb::ValidateImage,
        word: "validate-image",
        noun: "",
        arg: "",
        about: "run every check a built image has to pass",
        family: Family::Layer,
        host: false,
        takes: &[],
    },
];

impl Verb {
    pub fn spec(self) -> &'static Spec {
        COMMANDS
            .iter()
            .find(|spec| spec.verb == self)
            .expect("every verb has a row")
    }
}

/// Every row named by `word`, in table order.
pub fn nouns(word: &str) -> Vec<&'static Spec> {
    COMMANDS.iter().filter(|spec| spec.word == word).collect()
}

/// Whether `word` alone is a list to pick from, which it is when every form of
/// it takes a noun. `scap` and `fetch` also take an argument, so a picker of
/// their nouns would hide half the surface and their bare form keeps its
/// refusal.
pub fn all_nouns(word: &str) -> bool {
    let rows = nouns(word);
    !rows.is_empty() && rows.iter().all(|spec| !spec.noun.is_empty())
}

/// The row `words` names, and what is left after the words that named it. A
/// verb whose rows all carry a noun is refused listing them.
pub fn resolve<'a>(words: &'a [&'a str]) -> Result<(&'static Spec, &'a [&'a str]), String> {
    let word = words[0];
    let rows = nouns(word);
    if rows.is_empty() {
        return Err(format!("unknown command `{word}`"));
    }
    if let Some(spec) = words
        .get(1)
        .and_then(|next| rows.iter().find(|spec| spec.noun == *next))
    {
        return Ok((spec, &words[2..]));
    }
    match rows.iter().find(|spec| spec.noun.is_empty()) {
        Some(spec) => Ok((spec, &words[1..])),
        None => Err(format!("`{word}` takes {}", either(&rows))),
    }
}

/// `a`, `b` or `c`, which is how a refusal lists what it wanted instead.
fn either(rows: &[&'static Spec]) -> String {
    let quoted: Vec<String> = rows
        .iter()
        .map(|spec| format!("`{}`", spec.tail()))
        .collect();
    match quoted.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// What a person is shown: what runs anywhere, then what needs a repository.
pub fn listed() -> Vec<&'static Spec> {
    let mut rows: Vec<&'static Spec> = COMMANDS
        .iter()
        .filter(|spec| spec.family == Family::Anywhere)
        .collect();
    rows.extend(COMMANDS.iter().filter(|spec| spec.family == Family::Repo));
    rows
}

/// Where `tect` is being run, asked once at the top of a run and passed. A
/// place is not a `Family`: a family says what a command needs, and `why`
/// needs a repository *or* a host, which no family can say.
#[derive(Debug, PartialEq)]
pub enum Context {
    /// A `repo.kdl` here or above, named the way `--root .` names one, so
    /// every path a command prints hangs off it and a person reads `modules/x`
    /// rather than where their home is.
    Repo(PathBuf),
    /// A booted tectonic image: `/usr/share/tectonic/` carries the manifest
    /// the build baked and the record it wrote beside it.
    Host,
    /// Neither.
    Loose,
}

impl Context {
    /// `--root` names a repository outright. Otherwise a repository wins over
    /// a host, because a checkout on a booted tectonic machine is the more
    /// specific answer and it is the one with the source; without it the
    /// baked manifest is what there is to read.
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

/// What a booted image answers, for the refusal that has to name them.
pub fn on_host() -> Vec<&'static Spec> {
    COMMANDS.iter().filter(|spec| spec.host).collect()
}

/// The rows a picker offers here, and the choices that draw them.
///
/// It used to keep every row and put `needs a repository` where the `about`
/// went, so the list would not be shorter. That reasoning holds for a
/// reference and not for a menu: `usage` is the surface a person learns from
/// and it still lists everything, grouped by where it runs, while a picker is
/// a question about what to do *now*. A row it cannot answer with is neither
/// runnable nor readable, which is worse than either offering it whole or
/// leaving it out.
///
/// One function returns both, because a filtered list and an index into it
/// cannot be built apart without eventually disagreeing.
pub fn choices<'a>(rows: &[&'a Spec], here: &Context) -> (Vec<&'a Spec>, Vec<Choice>) {
    let kept: Vec<&Spec> = rows
        .iter()
        .copied()
        .filter(|spec| spec.runs_in(here))
        .collect();
    let drawn = kept
        .iter()
        .map(|spec| Choice::new(spec.label(), spec.about))
        .collect();
    (kept, drawn)
}

const HEAD: &str = "usage: tect [--root <dir>] <command>\n";

const RULE: &str = "\
Every command takes a flag for everything it needs. What no flag gave is asked
for, and `--no-tui` asks nothing, failing and naming the flag instead.

docs/commands.md is the reference. Data goes to stdout and diagnostics to
stderr; exit 1 is the invocation, exit 2 the repository.
";

/// The whole surface a person is taught, grouped by where it runs. Unlike the
/// picker this keeps the rows that will not run here, because a reference is
/// for learning what exists and a menu is a question about what to do now.
pub fn usage(here: &Context) -> String {
    let rows = listed();
    let width = rows
        .iter()
        .map(|spec| spec.label().len())
        .max()
        .unwrap_or(0);
    let block = |keep: &dyn Fn(&Spec) -> bool| -> String {
        rows.iter()
            .filter(|spec| keep(spec))
            .map(|spec| format!("  {:width$}  {}\n", spec.label(), spec.about))
            .collect()
    };
    let anywhere = block(&|spec| spec.family == Family::Anywhere);
    let repo = |host: bool| block(&move |spec| spec.family == Family::Repo && spec.host == host);
    match here {
        Context::Repo(_) => format!(
            "{HEAD}\n{anywhere}{}\n{RULE}",
            block(&|spec| spec.family == Family::Repo)
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
            block(&|spec| spec.family == Family::Repo),
        ),
    }
}

/// What `run` performs. The commands reached through the repository; the ones
/// that write it, build it or read the layer around them never come here.
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

/// What a command's one argument names.
pub enum Arg {
    Image,
    Target,
    Module,
}

impl Command {
    /// What it takes after the command word, for the ones that take anything.
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
    /// The `run` command it reads through, for the verbs that read one.
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
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_has_one_row_and_every_row_names_a_verb() {
        assert_eq!(ALL.len(), COMMANDS.len());
        for verb in ALL {
            let rows = COMMANDS.iter().filter(|spec| spec.verb == *verb).count();
            assert_eq!(rows, 1, "{verb:?} has {rows} rows");
        }
        for spec in COMMANDS {
            assert!(ALL.contains(&spec.verb), "{:?} is not in ALL", spec.verb);
        }
    }

    #[test]
    fn every_row_resolves_back_to_itself() {
        for spec in COMMANDS {
            let words: Vec<&str> = [spec.word, spec.noun]
                .iter()
                .filter(|part| !part.is_empty())
                .copied()
                .collect();
            let (found, rest) = resolve(&words).unwrap();
            assert_eq!(found.verb, spec.verb, "{} resolved elsewhere", spec.label());
            assert!(rest.is_empty());
        }
    }

    /// A place decides what runs, and the two renderings read the same
    /// answer. The host arm is the one that cannot be reached from a test
    /// process, since it is a file at an absolute path.
    #[test]
    fn a_place_decides_what_runs_and_the_help_groups_by_it() {
        let why = Verb::Why.spec();
        let check = Verb::Check.spec();
        let upgrade = Verb::Upgrade.spec();
        let repo = Context::Repo(".".into());

        for spec in [why, check, upgrade] {
            assert!(spec.runs_in(&repo), "{}", spec.name());
        }
        assert!(why.runs_in(&Context::Host) && !check.runs_in(&Context::Host));
        assert!(upgrade.runs_in(&Context::Loose) && !why.runs_in(&Context::Loose));

        // A picker offers what runs and keeps every description; the help
        // keeps every row and groups them.
        let (rows, drawn) = choices(&listed(), &Context::Loose);
        assert!(rows.iter().all(|spec| spec.family == Family::Anywhere));
        assert_eq!(rows.len(), drawn.len());
        assert!(drawn
            .iter()
            .all(|choice| choice.detail != "needs a repository"));
        let (rows, _) = choices(&listed(), &Context::Host);
        let verbs: Vec<Verb> = rows.iter().map(|spec| spec.verb).collect();
        assert!(verbs.contains(&Verb::Why) && !verbs.contains(&Verb::Check));

        let host = usage(&Context::Host);
        let (answers, rest) = host
            .split_once("these read the source tree")
            .expect("the host help groups what it can answer");
        assert!(answers.contains("  why [module]"), "{host}");
        assert!(!answers.contains("  check "), "{host}");
        assert!(rest.contains("  check "), "{host}");
        // The reference still teaches the whole surface, wherever it is read.
        for spec in listed() {
            for here in [Context::Repo(".".into()), Context::Host, Context::Loose] {
                assert!(usage(&here).contains(&spec.label()), "{}", spec.label());
            }
        }
    }

    #[test]
    fn only_a_verb_every_form_of_which_takes_a_noun_is_a_list() {
        for word in ["create", "import", "registry", "set", "vm"] {
            assert!(all_nouns(word), "{word}");
        }
        for word in ["scap", "fetch", "check", "build", "nonsense"] {
            assert!(!all_nouns(word), "{word}");
        }
    }

    #[test]
    fn a_verb_whose_rows_all_take_a_noun_lists_them() {
        let err = resolve(&["registry"]).unwrap_err();
        assert_eq!(err, "`registry` takes `namespace` or `ref`");
        let err = resolve(&["create"]).unwrap_err();
        assert_eq!(
            err,
            "`create` takes `repo [name]`, `image [name]`, `flavour [name]`, `module [name]` or `key <kind>`"
        );
    }
}

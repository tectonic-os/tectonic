//! The command surface as one table. `usage` and the picker are two renderings
//! of it, resolution reads it, and every dispatch arm answers a `Verb` that
//! came out of it.

use crate::ui::Choice;

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
    Check,
    Generate,
    Build,
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
    Verb::Check,
    Verb::Generate,
    Verb::Build,
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

pub const COMMANDS: &[Spec] = &[
    Spec {
        verb: Verb::Upgrade,
        word: "upgrade",
        noun: "",
        arg: "",
        about: "replace this tect and its assets with the latest",
        family: Family::Anywhere,
        takes: &[],
    },
    Spec {
        verb: Verb::CreateRepo,
        word: "create",
        noun: "repo",
        arg: "[name]",
        about: "start a repository for your own images",
        family: Family::Anywhere,
        takes: &["root", "host", "owner", "image", "base"],
    },
    Spec {
        verb: Verb::CreateImage,
        word: "create",
        noun: "image",
        arg: "[name]",
        about: "add an image: its name, and what it builds on",
        family: Family::Repo,
        takes: &["root", "owner", "base"],
    },
    Spec {
        verb: Verb::CreateFlavour,
        word: "create",
        noun: "flavour",
        arg: "[name]",
        about: "add a gated module set an image also publishes",
        family: Family::Repo,
        takes: &["root", "image"],
    },
    Spec {
        verb: Verb::CreateModule,
        word: "create",
        noun: "module",
        arg: "[name]",
        about: "write a module, and offer to list it in an image",
        family: Family::Repo,
        takes: &["root", "image", "pkg", "with"],
    },
    Spec {
        verb: Verb::ImportModule,
        word: "import",
        noun: "module",
        arg: "[name]",
        about: "reference a module from a collection repo.kdl declares",
        family: Family::Repo,
        takes: &["root", "image", "datastream"],
    },
    Spec {
        verb: Verb::CopyModule,
        word: "copy",
        noun: "module",
        arg: "[name]",
        about: "copy a collection module into this repository",
        family: Family::Repo,
        takes: &["root", "image"],
    },
    Spec {
        verb: Verb::CreateKey,
        word: "create",
        noun: "key",
        arg: "<kind>",
        about: "generate a key one of this repository's modules declares",
        family: Family::Repo,
        takes: &["root", "module", "cn"],
    },
    Spec {
        verb: Verb::SetWorkflows,
        word: "set",
        noun: "workflows",
        arg: "",
        about: "choose the CI this repository generates",
        family: Family::Repo,
        takes: ROOT,
    },
    Spec {
        verb: Verb::SetConforms,
        word: "set",
        noun: "conforms",
        arg: "[image]",
        about: "choose the benchmark profile an image is measured by",
        family: Family::Repo,
        takes: &["root", "datastream"],
    },
    Spec {
        verb: Verb::Check,
        word: "check",
        noun: "",
        arg: "",
        about: "read every manifest and say what is wrong with it",
        family: Family::Repo,
        takes: &["root", "datastream"],
    },
    Spec {
        verb: Verb::Generate,
        word: "generate",
        noun: "",
        arg: "",
        about: "write the build files, and list what was written",
        family: Family::Repo,
        takes: ROOT,
    },
    Spec {
        verb: Verb::Build,
        word: "build",
        noun: "",
        arg: "[target]",
        about: "verify the build files, then build the image",
        family: Family::Repo,
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
        verb: Verb::Section,
        word: "section",
        noun: "",
        arg: "[image]",
        about: "print the Containerfile section an image generates",
        family: Family::Repo,
        takes: ROOT,
    },
    Spec {
        verb: Verb::Graph,
        word: "graph",
        noun: "",
        arg: "",
        about: "print what provides what, and what the base carries",
        family: Family::Repo,
        takes: &["root", "format"],
    },
    Spec {
        verb: Verb::Why,
        word: "why",
        noun: "",
        arg: "[module]",
        about: "print one module's trust read-out, byte by byte",
        family: Family::Repo,
        takes: &["root", "format"],
    },
    Spec {
        verb: Verb::Coverage,
        word: "coverage",
        noun: "",
        arg: "[image]",
        about: "print who claims each rule the image conforms to",
        family: Family::Repo,
        takes: &["root", "format", "datastream"],
    },
    Spec {
        verb: Verb::Plan,
        word: "plan",
        noun: "",
        arg: "",
        about: "print every fact this repository derives, as json",
        family: Family::Script,
        takes: ROOT,
    },
    Spec {
        verb: Verb::Verify,
        word: "verify",
        noun: "",
        arg: "",
        about: "byte-compare what is generated against what is committed",
        family: Family::Script,
        takes: ROOT,
    },
    Spec {
        verb: Verb::Summary,
        word: "summary",
        noun: "",
        arg: "[target]",
        about: "print what one target is made of, as a markdown table",
        family: Family::Script,
        takes: ROOT,
    },
    Spec {
        verb: Verb::Sbom,
        word: "sbom",
        noun: "",
        arg: "[target]",
        about: "print the pinned payloads one target carries, as SPDX",
        family: Family::Script,
        takes: ROOT,
    },
    Spec {
        verb: Verb::FetchModules,
        word: "fetch",
        noun: "modules",
        arg: "",
        about: "fetch every out-of-tree module the images reference",
        family: Family::Script,
        takes: ROOT,
    },
    Spec {
        verb: Verb::ScapContent,
        word: "scap",
        noun: "content",
        arg: "",
        about: "print the datastream the target is measured with",
        family: Family::Script,
        takes: &["root", "target"],
    },
    Spec {
        verb: Verb::Scap,
        word: "scap",
        noun: "",
        arg: "<arf.xml>",
        about: "print what one scan says about the target",
        family: Family::Script,
        takes: &["root", "target", "datastream", "baseline"],
    },
    Spec {
        verb: Verb::RegistryNamespace,
        word: "registry",
        noun: "namespace",
        arg: "",
        about: "print where images publish",
        family: Family::Script,
        takes: ROOT,
    },
    Spec {
        verb: Verb::RegistryRef,
        word: "registry",
        noun: "ref",
        arg: "",
        about: "print the full reference one target publishes under",
        family: Family::Script,
        takes: &["root", "target", "tag"],
    },
    Spec {
        verb: Verb::OsRelease,
        word: "os-release",
        noun: "",
        arg: "",
        about: "write the image identity the build ARGs carry",
        family: Family::Layer,
        takes: &[],
    },
    Spec {
        verb: Verb::BuildRecord,
        word: "build-record",
        noun: "",
        arg: "",
        about: "write the record of what the build resolved",
        family: Family::Layer,
        takes: &[],
    },
    Spec {
        verb: Verb::Fetch,
        word: "fetch",
        noun: "",
        arg: "<what> <url> <sha256> [target] [extra...]",
        about: "download one payload, verify it, and place it",
        family: Family::Layer,
        takes: &[],
    },
    Spec {
        verb: Verb::ValidateImage,
        word: "validate-image",
        noun: "",
        arg: "",
        about: "run every check a built image has to pass",
        family: Family::Layer,
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

/// Whether there is a repository here or above, which is the one thing both
/// renderings below need to know.
fn in_repo() -> bool {
    std::env::current_dir()
        .ok()
        .and_then(|here| crate::find_root(&here))
        .is_some()
}

/// `rows` as a picker draws them. Outside a repository the second column says
/// why a command is there and will not run, rather than the list being
/// shorter.
pub fn choices(rows: &[&Spec]) -> Vec<Choice> {
    let here = in_repo();
    rows.iter()
        .map(|spec| match !here && spec.family == Family::Repo {
            true => Choice::new(spec.label(), "needs a repository"),
            false => Choice::new(spec.label(), spec.about),
        })
        .collect()
}

const HEAD: &str = "usage: tect [--root <dir>] <command>\n";

const RULE: &str = "\
Every command takes a flag for everything it needs. What no flag gave is asked
for, and `--no-tui` asks nothing, failing and naming the flag instead.

docs/commands.md is the reference. Data goes to stdout and diagnostics to
stderr; exit 1 is the invocation, exit 2 the repository.
";

/// The same list the picker draws, as text.
pub fn usage() -> String {
    let in_repo = in_repo();
    let rows = listed();
    let width = rows
        .iter()
        .map(|spec| spec.label().len())
        .max()
        .unwrap_or(0);
    let block = |family: Family| -> String {
        rows.iter()
            .filter(|spec| spec.family == family)
            .map(|spec| format!("  {:width$}  {}\n", spec.label(), spec.about))
            .collect()
    };
    let (anywhere, repo) = (block(Family::Anywhere), block(Family::Repo));
    match in_repo {
        true => format!("{HEAD}\n{anywhere}{repo}\n{RULE}"),
        false => format!(
            "{HEAD}\n{anywhere}\nthese need a repository, and there is none here or above:\n\n\
             {repo}\n{RULE}"
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

    #[test]
    fn only_a_verb_every_form_of_which_takes_a_noun_is_a_list() {
        for word in ["create", "import", "registry", "set"] {
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

//! Every string a person is asked by a prompt, in one place.
//!
//! Prompts only: the questions, the labels of the answers beside them, the
//! hints under the widgets, and the detail written for a choice rather than
//! read from a catalogue. A base's `about`, a module's description, a rule's
//! title and a workflow's `about` are catalogue content and stay where the
//! catalogue holds them.
//!
//! Not here, deliberately: diagnostics, errors, the `next` lines and the
//! command table. They are a different voice with a different job, and moving
//! them is a different session's question.

// The repository

pub const REPO_NAME: &str = "What will the repo be called?";
pub const SCHEDULED: &str = "Sync this repo to a provider?";
pub const REPO_HOST: &str = "Where will the repo be hosted?";
pub const FORGEJO_ADDRESS: &str = "What is the address of the Forgejo instance?";
pub const CREATE_REMOTE: &str = "Would you like to create this repo on Github now?";
pub const NO_GH: &str = "Install the Github CLI now?";

/// Github is asked for by name, and every other host by its address, which is
/// what a person who typed one recognises.
pub fn username(host: &str) -> String {
    match host {
        crate::create::HOST => "What is your github username?".to_string(),
        host => format!("What is your username on {host}?"),
    }
}

// The images

pub const IMAGES: &str = "Define an image now?";
pub const IMAGE_NAME: &str = "What will the image be called?";
pub const IMAGE_BASE: &str = "What is the base image for this image?";
pub const BASE_IMAGE: &str = "base image";
pub const BASE_FAMILY: &str = "base family";
pub const FLAVOUR_NAME: &str = "flavour name";
pub const FLAVOUR_IMAGE: &str = "Which image publishes it?";

// The modules

pub const MODULE_NAME: &str = "module name";
pub const MODULE_PACKAGES: &str = "Does this module install packages?";
pub const PACKAGE_NAMES: &str = "package names, separated by spaces";
pub const WHICH_MODULE: &str = "Which module?";
pub const WHICH_MODULES: &str = "Which modules?";
pub const LIST_IN_IMAGES: &str = "Which images list it?";

/// One image with no flavours is a yes or a no rather than a list.
pub fn list_in(target: &str) -> String {
    format!("List it in {target}?")
}

// What an import offers to bring with it. The clauses these questions used to
// carry are still reachable: the modules a `requires` offer named are the rows
// of the import that follows it, and the rules a claims offer counted are what
// `tect coverage` prints.

pub const IMPORT_REQUIRED: &str = "Import what these modules require?";
pub const IMPORT_CLAIMING: &str = "Import the modules claiming these rules?";

/// The workflows themselves stand above this, since nothing else names them:
/// the tree afterwards says only that `repo.kdl` gained workflows.
pub const GENERATE_WORKFLOWS: &str = "Generate the workflows this makes runnable?";

// The CI

pub const WORKFLOWS: &str = "Which workflows?";
pub const PUBLISH_SCHEDULED: &str = "Publish images only on scheduled builds?";
pub const SCAN_SCHEDULED: &str = "Run image scans only on scheduled builds?";
pub const DAILY_AT: &str = "what time the daily build runs, UTC";

// What is measured, and what claims it

pub const WHICH_IMAGE: &str = "Which image?";
pub const MEASURED_IMAGE: &str = "Which image is measured?";
pub const WHICH_PROFILE: &str = "Which profile?";

pub fn claimed_rules(named: &str) -> String {
    format!("Which rules does `{named}` claim?")
}

// The keys

pub const WHICH_KEY: &str = "Which key?";
pub const KEY_CN: &str = "common name, which is what the enrolment prompt shows";

/// Which module's, where more than one declares the kind. The modules are the
/// rows below, so the question does not list them.
pub fn key_provider(kind: &str) -> String {
    format!("Which module's {kind} key?")
}

// The command surface

pub const WHICH_COMMAND: &str = "Which command?";

// What the two answers are called, since not every one of them is a refusal.

pub const YES: &str = "Yes";
pub const NO: &str = "No";
pub const SKIP: &str = "Skip";
pub const SKIP_REMOTE: &str = "Skip Github repo creation";

// The detail beside a choice, where it is written rather than read.

pub const HOST_GITHUB: &str = "Github, and the workflows Tectonic ships";
pub const HOST_FORGEJO: &str = "a Forgejo instance, whose address you give";

// What each widget answers to.

pub const PICK: &str = "up and down to move, enter to choose, esc cancels";
pub const TOGGLE: &str = "space toggles, enter confirms, esc cancels";
pub const EITHER: &str = "up and down to move, enter to answer";
/// No `j` and `k` here: every printable key is the filter being typed.
pub const NEST: &str = "filter, space toggles, ←/→ opens, enter confirms";

#[cfg(test)]
mod tests {
    /// A question is drawn into a one-line head, so one that wraps loses
    /// everything after its first line. Reading the file is what keeps this
    /// true of a question added later without one being added here too.
    #[test]
    fn every_string_here_is_one_short_line() {
        for line in include_str!("copy.rs").lines() {
            let Some(rest) = line.strip_prefix("pub const ") else {
                continue;
            };
            assert!(rest.ends_with("\";"), "{line}");
            let text = rest.split_once("= \"").expect("a string literal").1;
            assert!(text.chars().count() - 2 < 60, "{line}");
        }
    }
}

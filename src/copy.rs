//! Every string a person is asked by a prompt, in one place.
//!
//! Prompts only: the questions, the labels of the answers beside them, and the
//! hints under the widgets. Catalogue content — a base's `about`, a module's
//! description, a rule's title, a workflow's `about` — stays where the catalogue
//! holds it, and so do diagnostics, errors, `next` lines and the command
//! table.

/// The name of the thing, which is what prose calls it. `tect` is the command
/// and belongs in a command line, a flag or a diagnostic prefix; a title or a
/// sentence a person reads says this.
pub const PRODUCT: &str = "Tectonic";

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
pub const IMAGE_BOOTLOADER: &str = "Which bootloader does it install?";
pub const BOOTLOADER_GRUB2: &str = "GRUB";
pub const BOOTLOADER_SYSTEMD: &str = "systemd-boot";
pub const FLAVOUR_NAME: &str = "flavour name";
pub const FLAVOUR_IMAGE: &str = "Which image publishes it?";

// The modules

pub const MODULE_NAME: &str = "module name";
pub const MODULE_PACKAGES: &str = "Does this module install packages?";
pub const PACKAGE_NAMES: &str = "package names, separated by spaces";
pub const WHICH_MODULE: &str = "Which module?";
pub const WHICH_MODULES: &str = "Which modules?";
pub const LIST_IN_IMAGES: &str = "Which images list it?";

/// One image with no flavours is answered yes or no.
pub fn list_in(target: &str) -> String {
    format!("List it in {target}?")
}

// What an import or a copy offers to bring with it. The modules a `requires`
// offer names are the rows of the import that follows it, and the rules a
// claims offer counts are what `tect coverage` prints.

/// Asked by both commands, and answered by whichever one asked: an import
/// references what it brings, a copy vendors it.
pub const BRING_REQUIRED: &str = "Bring what these modules require?";
/// The same question against the base row, asked one step earlier: a base that
/// is not a bootc image says what makes it one, and a fresh repository has
/// neither that nor the family adapter.
pub const BRING_FOR_BASE: &str = "Do you want to add them now?";
pub const IMPORT_CLAIMING: &str = "Import the modules claiming these rules?";

/// What a list says above itself about an answer it cleared. `unmet` is the
/// same reason the row carries beside itself; this line exists because a row
/// going quietly from `[x]` to a dim `[ ]` is a change nothing announced.
pub fn cleared(named: &str, unmet: &str) -> String {
    format!("{named} {unmet} and was cleared")
}

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
pub const KEY_FROM: &str = "path to the public half you already hold";

/// Which module's, where more than one declares the kind. The modules are the
/// rows below, so the question does not list them.
pub fn key_provider(kind: &str) -> String {
    format!("Which module's {kind} key?")
}

// The disk a virtual machine boots

pub const WHICH_DISK: &str = "Which kind of disk image?";
pub const DISK_QCOW2: &str = "a qemu disk, under out/qcow2/";
pub const DISK_RAW: &str = "a raw disk, under out/raw/";
pub const DISK_ISO: &str = "an installer iso, under out/bootiso/";
pub const NO_ISO_SPAWN: &str = "systemd-vmspawn cannot boot an iso";

// The command surface

pub const WHICH_COMMAND: &str = "Which command?";

// What the two answers are called, since not every one of them is a refusal.

pub const YES: &str = "Yes";
pub const NO: &str = "No";
pub const SKIP: &str = "Skip";
pub const SKIP_REMOTE: &str = "Skip Github repo creation";

// The detail beside a choice, for the choices no catalogue describes.

pub const HOST_GITHUB: &str = "Github, and the workflows Tectonic ships";
pub const HOST_FORGEJO: &str = "a Forgejo instance, whose address you give";

// What each widget answers to.

pub const REVIEW_KEYS: &str = "enter to change a field, Create to write, esc cancels";
pub const FORM_KEYS: &str = "up and down to move, enter to change a field";

// The review screen `create repo` draws over its collected answers before
// anything is written. Every row is a piece of configuration, said as what the
// repository will have.

pub const REVIEW: &str = "Review what will be created";
pub const CREATE: &str = "Create";
/// A gate answered No is still a row, so nothing collected disappears from the
/// screen and every decision stays reachable.
pub const NONE: &str = "none";
pub const ROW_NAME: &str = "name";
pub const ROW_PROVIDER: &str = "provider";
pub const ROW_REMOTE: &str = "github repo";
pub const ROW_IMAGE: &str = "image";
pub const ROW_BASE: &str = "base";
pub const ROW_WORKFLOWS: &str = "workflows";
pub const ROW_PUBLISH: &str = "publish";
pub const ROW_SCANS: &str = "image scans";
pub const ROW_DAILY: &str = "daily build";
pub const REMOTE_MADE: &str = "created on push";
pub const REMOTE_NOT: &str = "not created";
pub const ON_EVERY_PUSH: &str = "on every push";
pub const ON_EVERY_BUILD: &str = "on every build";
pub const ON_SCHEDULED: &str = "on scheduled builds only";

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
            // A constant split over two lines is one string still: its first
            // line is skipped here and its text is checked where it is
            // written.
            if !rest.ends_with("\";") {
                continue;
            }
            let name = rest.split_once(':').expect("a const name").0;
            // A legend is drawn along the border, not inside a panel: it is
            // the one string here allowed to run past a short line.
            if name.ends_with("_KEYS") {
                continue;
            }
            let text = rest.split_once("= \"").expect("a string literal").1;
            assert!(text.chars().count() - 2 < 60, "{line}");
        }
    }
}

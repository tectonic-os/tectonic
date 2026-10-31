//! Every string a prompt asks the user, in one place.
//!
//! Prompts only: the questions, the labels of the answers beside them, and the
//! hints under the widgets. Catalogue content — a base's `about`, a component's
//! description, a rule's title, a workflow's `about` — stays where the catalogue
//! holds it, and so do diagnostics, errors, `next` lines and the command
//! table.

/// The product name prose uses. `tect` is the command: it belongs in a command
/// line, a flag or a diagnostic prefix, and a title or a sentence says this.
pub const PRODUCT: &str = "Tectonic";

/// One host feeds the origin and image URLs because the tool has no second
/// forge model.
pub const HOST: &str = "github.com";

// The repository

pub const REPO_NAME: &str = "What will the repo be called?";
pub const SCHEDULED: &str = "Sync this repo to a provider?";
pub const REPO_HOST: &str = "Where will the repo be hosted?";
pub const FORGEJO_ADDRESS: &str = "What is the address of the Forgejo instance?";
pub const CREATE_REMOTE: &str = "Create this repo on Github now?";
pub const NO_GH: &str = "Install the Github CLI now?";

/// Github is asked for by name, and every other host by its address, which is
/// what the user typed.
pub fn username(host: &str) -> String {
    match host {
        HOST => "Github username?".to_string(),
        host => format!("Username on {host}?"),
    }
}

// The libraries a repository reads

pub const LIBRARIES: &str = "Included libraries:";
pub const WHICH_LIBRARY: &str = "Which library?";
pub const OTHER_SOURCE: &str = "Another source";
pub const OTHER_SOURCE_ABOUT: &str = "Type the alias, the Git URL and the directory";
pub const SOURCE_ALIAS: &str = "source alias";
pub const SOURCE_URL: &str = "repository URL";
pub const SOURCE_PATH: &str = "directory inside the repository";

// The images

pub const IMAGES: &str = "Define an image now?";
pub const IMAGE_NAME: &str = "What will the image be called?";
pub const IMAGE_BASE: &str = "What is the base image for this image?";
pub const BASE_IMAGE: &str = "base image";
pub const OTHER_BASE: &str = "Another base image";
pub const IS_BASE: &str = "a base, which an image lists once as its base";
pub const OTHER_BASE_ABOUT: &str = "Type an image reference that no base component names";
pub const BASE_FAMILY: &str = "base family";
pub const IMAGE_BOOTLOADER: &str = "Which bootloader does it install?";
pub const BOOTLOADER_GRUB2: &str = "GRUB";
pub const BOOTLOADER_SYSTEMD: &str = "systemd-boot";
pub const FLAVOUR_NAME: &str = "flavour name";
pub const FLAVOUR_IMAGE: &str = "Which image publishes it?";

// The components

pub const COMPONENT_NAME: &str = "component name";
pub const COMPONENT_DESCRIPTION: &str = "description (optional)";
pub const COMPONENT_SUPPORTS: &str = "supported base families, separated by spaces (optional)";
pub const COMPONENT_PACKAGES: &str = "Does this component install packages?";
pub const PACKAGE_NAMES: &str = "package names, separated by spaces";
pub const WHICH_COMPONENT: &str = "Which component?";
pub const WHICH_COMPONENTS: &str = "Which components?";
pub const LIST_IN_IMAGES: &str = "Which images list it?";

/// One image with no flavours is answered yes or no.
pub fn list_in(target: &str) -> String {
    format!("List it in {target}?")
}

// What an import asks before it copies a component in. The target image is asked
// first, because it decides which library components the picker marks
// unavailable.

pub const TARGET_IMAGE: &str = "Which image is this for?";
pub const TARGET_NONE: &str = "None, only add it to the repository";

/// Names the families a component targets and the family of the target image.
/// The picker shows this beside a component it marks unavailable.
pub fn unsupported(targets: &[String], family: &str) -> String {
    format!("targets {}; this image is {family}", targets.join(", "))
}

pub fn present(name: &str) -> String {
    format!("`{name}` is already in this repository.")
}

pub fn add_local(name: &str, image: &str) -> String {
    format!("Add the local `{name}` to `{image}`")
}

pub const ANOTHER_NAME: &str = "Import it under another name";

/// Names the component that already provides a capability one of them declares
/// exclusive. The picker shows this beside a component it marks unavailable.
pub fn exclusive_to(capability: &str, holder: &str) -> String {
    format!("`{capability}` is exclusive to `{holder}`")
}

pub fn exclusive_clash(capability: &str, holder: &str, image: &str) -> String {
    format!("`{capability}` is exclusive, and `{holder}` already provides it in `{image}`.")
}

pub fn replace(holder: &str) -> String {
    format!("Replace `{holder}`")
}

pub const CANCEL: &str = "Cancel";
pub const LOCAL_NAME: &str = "local name";

/// Asks once for the whole set the import copies in.
pub fn track(named: &str) -> String {
    format!("Track upstream for {named}?")
}

pub fn untrack(key: &str) -> String {
    format!("Stop tracking upstream for `components/{key}`, so the copy is the repository's own?")
}

// What an import offers to bring with it. The components a `requires` offer names
// are the rows of the import that follows it, and the rules a claims offer
// counts are what `tect coverage` prints.

pub const REQUIRED: &str =
    "The selected components have dependencies not currently provided by the images:";
pub const BRING_REQUIRED: &str = "Add these provider components now?";

/// Asks which of several library components meets one requirement.
pub fn which_provider(component: &str, capability: &str) -> String {
    format!("`{component}` requires `{capability}`. Which component provides it?")
}
/// The same question against the base row, asked one step earlier: a base that
/// is not a bootc image says what makes it one, and a fresh repository has
/// neither that nor the family adapter.
pub const BRING_FOR_BASE: &str = "Add them now?";
pub const IMPORT_CLAIMING: &str = "Import the components claiming these rules?";

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

// The scripts a repository keeps

pub const SCRIPTS: &str = "Which files to keep in scripts/?";
pub const SCRIPTS_NEXT: &str =
    "run `tect generate` so the generated scripts and workflows call the copies in scripts/";

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
pub const KEY_FROM: &str = "path to the public half already held";

/// Which component's, where more than one declares the kind. The components are the
/// rows below, so the question does not list them.
pub fn key_provider(kind: &str) -> String {
    format!("Which component's {kind} key?")
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

pub const HOST_FORGEJO: &str = "a Forgejo instance, at an address asked next";

/// The first host's detail. It names the product, so it reads `PRODUCT` rather
/// than repeating the name a rename would leave stale.
pub fn host_github() -> String {
    format!("Github, and the workflows {PRODUCT} ships")
}

// What each widget answers to.

pub const REVIEW_KEYS: &str =
    "\u{2191}\u{2193} navigate \u{2022} \u{23ce}  select \u{2022} Esc cancel";
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
pub const ROW_LIBRARIES: &str = "libraries";
pub const ROW_IMAGE: &str = "image";
pub const ROW_BASE: &str = "base";
pub const ROW_WORKFLOWS: &str = "workflows";
pub const ROW_PUBLISH: &str = "publish";
pub const ROW_SCANS: &str = "image scans";
pub const ROW_DAILY: &str = "daily build";
pub const ROW_SCRIPTS: &str = "scripts kept";
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

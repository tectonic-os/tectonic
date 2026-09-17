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

// The machine an install writes to. The image and the boot chain are the
// payload's; these five are the half nothing derives.

pub const INSTALL_DISK: &str = "Installation disk";
pub const INSTALL_NAME: &str = "Computer name";
pub const INSTALL_USER: &str = "Username";
pub const INSTALL_PASSWORD: &str = "Password";
pub const INSTALL_ENCRYPTION: &str = "Encryption type";
pub const LUKS_PASSPHRASE: &str = "LUKS passphrase";
/// The four descriptions, which are what the rows say. The wire names
/// `--encryption` takes are fisherman's and stay in `install::KINDS`.
pub const ENC_NONE: &str = "none";
pub const ENC_TPM2: &str = "luks with TPM2 decryption and recovery key";
pub const ENC_PASSPHRASE: &str = "luks with passphrase decryption";
pub const ENC_BOTH: &str = "luks with TPM2 decryption and recovery passphrase";
/// What each costs, beside the row. Nothing here is marked strongest: a TPM
/// binds the key to a measured boot, a passphrase binds it to a person.
pub const ENC_ANY_HOLDER: &str = "opens for anyone who has the machine";
pub const ENC_ONLY_YOU: &str = "nothing opens it without you";
pub const NO_TPM: &str = "No TPM available";
pub const NO_LUKS_INITRAMFS: &str = "the image has not declared and proved `luks-initramfs`";
pub const REMOVABLE: &str = "removable";

// Where `/home` goes. `/home` is `/var/home` on a bootc system, so a separate
// home is a separate `/var`, and that is a partition of the install disk or a
// disk of its own.

pub const DATA_HERE: &str = "this disk, sized";
pub const DATA_ERASED: &str = "erased";
pub const DATA_KEPT: &str = "keep what is on it";
/// Beside the row that keeps another disk's filesystem while the root is
/// encrypted: a kept filesystem is not re-encrypted, so it would be an
/// unencrypted home. The rows that erase what they take are encrypted by
/// fisherman with the root's passphrase, and are pickable.
pub const DATA_UNENCRYPTED: &str = "an unencrypted home under an encrypted root";
pub const DATA_SAME_DISK: &str = "that is the disk this installs to";
/// Beside the `/var` answer while the root is encrypted: fisherman wraps the
/// `/var` it creates in the root's passphrase, which is what a reinstall that
/// keeps the disk can open it with.
pub const DATA_ENCRYPTED: &str = "encrypted with the root's passphrase";
pub const CUSTOM_ENCRYPTION: &str = "custom layouts cannot create encryption";
pub const CUSTOM_OTHER_DISK: &str = "the custom layout belongs to another disk";
pub const CUSTOM_ROOT: &str = "still needs a / partition";
pub const CUSTOM_DUPLICATE: &str = "each mount point can be used only once";
pub const CUSTOM_NEEDS_DISK: &str = "choose a disk before editing its partitions";
pub const CUSTOM_FORMAT_FILESYSTEM: &str = "fisherman cannot format this filesystem";
pub const CUSTOM_KEEP_FILESYSTEM: &str = "not a supported Linux filesystem here";
pub const CUSTOM_KEEP_ESP: &str = "an EFI system partition must use FAT";
pub const LEAVE_PARTITION: &str = "leave unchanged";

/// The answer that makes an existing container usable: decrypt it and mount
/// what is inside it where the target says.
pub fn open_at(target: &str) -> String {
    format!("open it as {target}")
}

/// What opening it costs, beside the answer. A `/boot` the bootloader cannot
/// read is refused once the container is open, and the row says so first.
/// Both loaders read a `/boot` of their own, and neither opens a container:
/// GRUB is not configured to and systemd-boot uses no separate `/boot`.
pub const CUSTOM_OPEN_BOOT: &str = "the bootloader cannot read an encrypted /boot";

pub fn open_cost(target: &str) -> &'static str {
    match target {
        "/boot" => "decrypt it, and /boot has to be ext4",
        _ => "decrypt it and keep its filesystem",
    }
}

/// The question the key for one container is asked with, so a screen with
/// three rows still says which container it is about.
pub fn open_question(partition: &str, target: &str) -> String {
    format!("How does {partition} open as {target}?")
}

/// What the confirmation says about one container: that it is decrypted and
/// kept, and how the installed machine opens it again at boot. Its filesystem
/// is not visible until it is open, so this is the half of it the summary can
/// say.
pub fn opened(partition: &str, how: &str) -> String {
    format!("{partition}  {how}")
}

// A container the editor opens is not re-keyed by this installer. The
// encryption row becomes what each header holds and what may be added to it,
// because nothing is being formatted: how the installed machine opens it is
// the ladder's answer, and only an addition is a question.

pub const OPENED_KEEP: &str = "keep what is enrolled";
pub const OPENED_TPM2: &str = "add a TPM2 token";
pub const OPENED_ADD_KEY: &str = "add a key file for boot";
pub const OPENED_TPM2_COST: &str = "first boot asks once, then no prompt";
pub const OPENED_TPM2_HAS: &str = "this container already has one";
pub const OPENED_ADD_KEY_COST: &str = "this machine reads it, the old one keeps its own";
/// Why a key file is not offered while the root is unencrypted. A data
/// volume's key file — or the key a first-boot TPM2 enrolment is staged
/// with — is read from the installed root, and an unencrypted root would
/// leave it in the clear beside the volume it opens. The ladder falls to the
/// passphrase instead.
pub const OPENED_KEYFILE_PLAIN: &str = "the root is not encrypted, so the key would be readable";
/// Why a root opened with a key file cannot keep what it has: the file would
/// have to live on the filesystem its key opens, and a token staged inside it
/// cannot be enrolled before the first boot unlocks it.
pub const OPENED_ROOT_KEYFILE: &str = "a key file cannot open the root; use its passphrase";

/// What one container's header holds, as the row and the summary say it.
pub fn slots_said(keys: &[u32], tokens: &[String]) -> String {
    let said: Vec<String> = keys
        .iter()
        .map(|at| format!("slot {at}"))
        .chain(tokens.iter().map(|kind| format!("token {kind}")))
        .collect();
    match said.is_empty() {
        true => "no slots".to_string(),
        false => said.join(", "),
    }
}

pub fn slots_unknown(why: &str) -> String {
    format!("its slots could not be read: {why}")
}

/// How the installed machine opens a container it did not re-key. One per
/// rung of the ladder, said on the confirmation and on the last screen. The
/// first clause repeats what every one of them is, because the summary column
/// is a few words wide and a bare "passphrase at boot" reads as the cost.
pub const BOOT_KEYFILE: &str = "decrypted, keyfile at boot";
pub const BOOT_PASSPHRASE: &str = "decrypted, passphrase at boot";
pub const BOOT_TPM2: &str = "decrypted, TPM2 at boot";
pub const BOOT_ADDED_KEY: &str = "decrypted, key added here";

/// The end of the install says where a slot the machine no longer needs can
/// go, and never removes one itself: nothing in the header says which slot
/// holds what, and the old system may still open the volume with it. One row
/// per sentence, because the last screen draws a row as a line.
pub fn kill_slot(device: &str, slot: u32, count: usize) -> Vec<String> {
    vec![
        format!("{device} has {count} slots, and this machine no longer needs slot {slot}"),
        format!("both keys open it, so it can go: cryptsetup luksKillSlot {device} {slot}"),
    ]
}

pub fn kill_a_slot(device: &str, count: usize) -> Vec<String> {
    vec![
        format!("{device} has {count} slots, and this machine no longer needs one of them"),
        format!("cryptsetup luksKillSlot {device} <slot>"),
    ]
}

/// One line of the installed system's crypttab. `none` in the third field is
/// a passphrase the machine asks for at boot.
pub fn crypttab_line(name: &str, uuid: &str, keyfile: Option<&str>) -> String {
    format!("{name} UUID={uuid} {} luks", keyfile.unwrap_or("none"))
}

/// The first-boot oneshot that adds a TPM2 token to a container the layout
/// opened. `systemd-cryptenroll` seals against the PCRs of the machine it
/// runs on, and the live installer's are not the installed machine's, so the
/// enrollment belongs to the first boot and not to the install. The key that
/// opens the container is staged beside the unit and shredded once the token
/// is in.
///
/// `pcr_policy` says the installed image carries a signed PCR 11 policy (the
/// marker its build wrote beside the committed key), and then the token binds
/// to that policy as well as PCR 7 — the lock needs both. `systemd-stub`
/// places the booted UKI's `.pcrsig`/`.pcrpkey` in `/run/systemd/` as it
/// boots, and both paths are named explicitly: the option's default is to bind
/// to no PCRs at all when no public key and signature are found, and a machine
/// that silently seals to nothing is worse than one that says so. Without the
/// policy, PCR 7 alone is what every other chain gets.
pub fn tpm2_unit(name: &str, key: &str, uuid: &str, pcr_policy: bool) -> String {
    let enroll = match pcr_policy {
        true => {
            "--tpm2-pcrs=7 --tpm2-public-key=/run/systemd/tpm2-pcr-public-key.pem \
                 --tpm2-signature=/run/systemd/tpm2-pcr-signature.json"
        }
        false => "--tpm2-pcrs=7",
    };
    format!(
        "[Unit]\n\
         Description=Enroll the {name} container for TPM2 unlock\n\
         ConditionPathExists={key}\n\
         After=basic.target\n\
         DefaultDependencies=no\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         RemainAfterExit=no\n\
         ExecStart=/usr/bin/systemd-cryptenroll --tpm2-device=auto {enroll} \
         --unlock-key-file={key} /dev/disk/by-uuid/{uuid}\n\
         ExecStartPost=-/usr/bin/shred -u {key}\n\
         ExecStartPost=-/usr/bin/systemctl disable tect-tpm2-enroll-{name}.service\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n"
    )
}

/// The key that was just added does not open the container it went into,
/// which is a change nothing may leave behind.
pub fn added_key_wrong(device: &str) -> String {
    format!("the key just added to {device} does not open it")
}

pub const OPEN_WITH: &str = "opens with";
pub const USE_KEY: &str = "Open it";
pub const KEY_PASSPHRASE: &str = "passphrase";
pub const KEY_PASSPHRASE_COST: &str = "typed here, and asked for again at boot";
pub const KEY_FILE: &str = "key file";
pub const KEY_FILE_COST: &str = "a file this live system can read";
pub const KEY_FILE_PATH: &str = "key file path";
pub const KEY_FILE_MISSING: &str = "there is no file at that path";

// Why a key the old system already holds was not used, drawn on the question
// that asks for one anyway. Silence here tells a person their disk needs a
// new key when it does not, so every step the walk could not do is named.

/// The row those reasons are drawn on, which is a container's row in the
/// editor's question.
pub const OLD_SYSTEM: &str = "old system";
pub const OLD_ROOT_NONE: &str = "no partition on this disk reads as an old system";
pub fn old_root_unnamed(partition: &str) -> String {
    format!("no old system on this disk names a key for {partition}")
}
pub fn old_root_partly(unread: &str) -> String {
    format!("{unread}, and nothing readable named a key")
}
pub fn old_root_key_missing(volume: &str, at: &str, why: &str) -> String {
    format!("the key {at} that opens {volume} could not be read: {why}")
}
pub fn old_root_key_outside(at: &str) -> String {
    format!("the key {at} is not a file inside the system that names it")
}
pub fn old_root_key_wrong(partition: &str) -> String {
    format!("the key the old system named for {partition} does not open it")
}
pub fn old_root_key_unreadable(volume: &str) -> String {
    format!("{volume} opens with a script this installer cannot run")
}
/// The old system reads this volume's key from removable media — Debian's
/// `passdev`, or a `path:device` third field on a bus-attached disk. Refused
/// rather than copied onto the root: the media is a thing its owner keeps
/// apart from the machine, and moving the bytes would silently make it one
/// with the machine.
pub fn old_root_key_removable(volume: &str) -> String {
    format!("{volume} opens with a key file on removable media, which this installer does not copy onto the machine")
}

/// The old system waits for this volume's key file to appear at boot
/// (`keyfile-timeout=`), which is the same removable-media arrangement said
/// another way, and one this installer does not configure.
pub fn old_root_key_waited(volume: &str) -> String {
    format!("{volume} waits for its key file at boot, which this installer does not configure")
}

pub fn custom_keep_boot(bootloader: &str) -> String {
    match bootloader {
        "systemd" => "systemd-boot does not use a separate /boot".to_string(),
        _ => format!("{bootloader} installs with a supported ext4 /boot"),
    }
}

/// A whole disk, and which of the two things happens to it. The label is the
/// answer, so it says both.
pub fn on_disk(disk: &str, how: &str) -> String {
    format!("{disk}, {how}")
}

/// The question asked over the summary, once, after the form is complete and
/// before anything is written. It carries what installing costs, in one
/// sentence.
pub fn erasing(disk: &str) -> String {
    format!("Everything on {disk} will be erased. Are you sure?")
}

pub fn changing_partitions(disk: &str) -> String {
    format!("Partitions marked format on {disk} will be erased. Are you sure?")
}

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

pub const PICK: &str = "up and down to move, enter to choose, esc cancels";
pub const TOGGLE: &str = "space toggles, enter confirms, esc cancels";
pub const EITHER: &str = "up and down to move, enter to answer";
/// No `j` and `k` here: every printable key is the filter being typed.
pub const NEST: &str = "filter, space toggles, ←/→ opens, enter confirms";
pub const REVIEW_KEYS: &str = "enter to change a field, Create to write, esc cancels";
pub const INSTALL_KEYS: &str = "\u{2191}\u{2193} move, enter change, esc leave";
pub const FORM_KEYS: &str = "up and down to move, enter to change a field";
/// No esc here: with a default it takes the default and with none it
/// fails naming the flag, which is what enter on an empty line does too.
pub const LINE_KEYS: &str = "enter confirms";

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

// The same screen over an install's answers, where the action is a wipe.

pub const INSTALL: &str = "Install";
pub const SHUT_DOWN: &str = "Shut down";
pub const CONTINUE: &str = "Continue";
pub const GO_BACK: &str = "Go back";
pub const USE_LAYOUT: &str = "Use layout";
pub const AUTOMATIC_LAYOUT: &str = "Use automatic layout";
pub const ROW_CONFIRM: &str = "password (confirm)";
pub const ROW_LAYOUT: &str = "layout";

/// What the disk is cut into, said in one row on the form. Nobody chooses any
/// of it: the base family settles the filesystem and the bootloader settles
/// whether there is a separate `/boot` at all.
pub fn layout(filesystem: &str, bootloader: &str, boot: &str) -> String {
    let partitions = match bootloader {
        "systemd" => format!("esp + {filesystem} root"),
        _ => format!("esp + ext4 /boot + {filesystem} root"),
    };
    match boot_chain(boot) {
        Some(chain) => format!("{partitions}; {chain}"),
        None => partitions,
    }
}

pub fn custom_layout(disk: &str, mounts: usize, kept: usize, opened: usize) -> String {
    let said = format!("custom on {disk}: {mounts} mounted partitions, {kept} kept");
    match opened {
        0 => said,
        opened => format!("{said}, {opened} opened"),
    }
}

pub fn keep_as(target: &str) -> String {
    format!("keep as {target}")
}

pub fn format_at(filesystem: &str, target: &str) -> String {
    format!("format as {filesystem} at {target}")
}

pub fn boot_chain(boot: &str) -> Option<&'static str> {
    match boot {
        "uki-shim" => Some("owner-signed UKI through Microsoft shim"),
        "uki-db" => Some("owner-signed UKI through firmware keys"),
        _ => None,
    }
}

/// The same, spelled out over the confirmation, a row per partition. This is
/// the half of what is about to be written that no question above covers.
pub fn written_over(
    bootloader: &str,
    filesystem: &str,
    var: &str,
    boot: &str,
    encrypted: bool,
) -> Vec<(String, String)> {
    let mut rows = vec![("esp".to_string(), "2 GB  fat32".to_string())];
    if let Some(chain) = boot_chain(boot) {
        rows.push(("boot chain".to_string(), chain.to_string()));
    }
    if bootloader != "systemd" {
        rows.push(("/boot".to_string(), "2 GB  ext4".to_string()));
    }
    // Cut out of this disk, so it is a partition here. A `/var` on another
    // disk is a row of the summary above instead.
    if !var.is_empty() {
        rows.push((
            "/var".to_string(),
            match encrypted {
                true => format!("{var}  {filesystem}  {DATA_ENCRYPTED}"),
                false => format!("{var}  {filesystem}"),
            },
        ));
    }
    rows.push(("root".to_string(), format!("the rest  {filesystem}")));
    rows
}
/// A form holds both halves of a password at once, so the two are compared on
/// the screen.
pub const NO_MATCH_ROW: &str = "the passwords do not match";
pub const ROW_DISK: &str = "disk";
pub const ROW_HOSTNAME: &str = "computer name";
pub const ROW_ACCOUNT: &str = "username";
pub const ROW_PASSWORD: &str = "password";
pub const ROW_ENCRYPTION: &str = "encryption";
pub const ROW_PASSPHRASE: &str = "passphrase";
pub const ROW_DATA: &str = "home and data";
pub const ROW_SIZE: &str = "size";
pub const PASSWORD_SET: &str = "set";
/// What a field nobody has answered yet reads as. On a form the difference
/// between a value and a gap has to be on the screen.
pub const NOT_SET: &str = "not set";

/// Why the action cannot be taken yet, said beside it while it is dim. These
/// are the fields nothing derives and no default stands in for.
pub fn still_needs(fields: &[&str]) -> String {
    format!("still needs a {}", fields.join(", a "))
}

// The installer owns the console, so it also owns what leaving it means and
// what finishing it offers.

/// The title bar over every widget the installer draws, with the image it is
/// installing beside it: the one thing on screen that never changes.
pub fn installing(image: &str) -> String {
    format!("{PRODUCT} installer \u{2014} {image}")
}

/// Where the whole of the install's output went, or that it went nowhere.
/// A live environment with nothing writable on it is the second case, and
/// saying so is the difference between a log and a log nobody can find.
pub fn logging(log: Option<&std::path::Path>) -> String {
    match log {
        Some(at) => format!("the install log is at {}", at.display()),
        None => "nothing here is writable, so this screen is the only copy".to_string(),
    }
}

/// The line under the bar, which says both things a person watching an install
/// needs and cannot ask for: that the disk it is writing to is already gone,
/// and where the transcript is. Short, because it is one line inside a box.
pub fn writing(log: Option<&std::path::Path>) -> String {
    match log {
        Some(at) => format!("no going back from here \u{2014} log: {}", at.display()),
        None => "no going back from here \u{2014} and this screen is the log".to_string(),
    }
}

/// The only copy of it there will ever be, and the disk does not open
/// without it if the TPM stops answering.
pub fn recovery(key: &str) -> String {
    format!("{WRITE_DOWN} {key}")
}

/// Drawn above the key on the completion screen, and with the key appended
/// where there is no screen to draw one on.
pub const WRITE_DOWN: &str = "write this down, it is the recovery key:";

pub const LEAVING: &str = "Leave the installer?";
pub const LEAVE_BACK: &str = "Keep going";
pub const LEAVE_OVER: &str = "Start the form again";
/// Not "to a shell": on installer media the unit starts the installer again,
/// and only a run from a shell returns to one.
pub const LEAVE_SHELL: &str = "Quit the installer";
pub const INSTALL_DONE: &str = "Installation Complete!";
pub const RESTART: &str = "Restart now";
/// The other way off the last screen is esc, which the legend already names,
/// so it is not a row.
pub const DONE_KEYS: &str = "enter to restart, esc to quit the installer";
pub fn enrollment(boot: &str) -> Option<&'static str> {
    match boot {
        "uki-shim" => Some("on first boot: use MokManager to enroll EFI/BOOT/MOK.cer"),
        "uki-db" => Some("before restart: clear the platform key in firmware setup"),
        _ => None,
    }
}
/// Said under the key on the completion screen, because the log deliberately
/// has no copy of it.
pub const KEY_NOT_LOGGED: &str = "It is not in the install log.";

#[cfg(test)]
mod tests {
    /// The one question that costs a disk names the disk and ends in a question
    /// mark: it is the last thing between a person and a wipe.
    #[test]
    fn the_cost_names_the_disk_and_asks() {
        let said = super::erasing("/dev/vda");
        assert!(said.contains("/dev/vda"), "{said}");
        assert!(said.ends_with('?'), "{said}");
    }

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

    #[test]
    fn uki_chains_are_visible_before_and_after_install() {
        assert!(super::layout("ext4", "systemd", "uki-shim").contains("Microsoft shim"));
        assert!(super::written_over("systemd", "ext4", "", "uki-db", false)
            .iter()
            .any(|(name, value)| name == "boot chain" && value.contains("firmware keys")));
        assert!(super::enrollment("uki-db")
            .is_some_and(|instruction| instruction.contains("clear the platform key")));
        assert!(super::enrollment("uki-shim")
            .is_some_and(|instruction| instruction.contains("EFI/BOOT/MOK.cer")));
    }

    /// A UKI's first-boot enrolment names the two files the stub places in
    /// `/run/systemd/` and keeps PCR 7: the lock needs both the signed policy
    /// and the machine's Secure Boot state, and a missing policy fails loudly
    /// instead of binding the token to no PCRs at all. An image with no policy
    /// keeps PCR 7 alone, exactly as every other chain.
    #[test]
    fn a_pcr_policy_tpm2_unit_names_the_embedded_files() {
        let policy = super::tpm2_unit("root", "/etc/tect/tpm2-enroll-root.key", "u", true);
        assert!(policy.contains("--tpm2-pcrs=7"), "{policy}");
        assert!(
            policy.contains("--tpm2-public-key=/run/systemd/tpm2-pcr-public-key.pem"),
            "{policy}"
        );
        assert!(
            policy.contains("--tpm2-signature=/run/systemd/tpm2-pcr-signature.json"),
            "{policy}"
        );
        let other = super::tpm2_unit("root", "/etc/tect/tpm2-enroll-root.key", "u", false);
        assert!(other.contains("--tpm2-pcrs=7"), "{other}");
        assert!(!other.contains("tpm2-pcr-public-key"), "{other}");
    }

    /// The last screen before a disk is erased says a `/var` it will create is
    /// encrypted with the root's passphrase, and says nothing of the kind
    /// where the root is not encrypted.
    #[test]
    fn a_created_var_says_when_it_is_encrypted() {
        let var = |encrypted: bool| {
            super::written_over("grub2", "ext4", "200 GB", "", encrypted)
                .into_iter()
                .find(|(name, _)| name == "/var")
                .expect("a /var row")
                .1
        };
        assert!(var(true).contains(super::DATA_ENCRYPTED), "{}", var(true));
        assert!(
            !var(false).contains(super::DATA_ENCRYPTED),
            "{}",
            var(false)
        );
        assert!(super::DATA_ENCRYPTED.chars().count() < 60);
    }
}

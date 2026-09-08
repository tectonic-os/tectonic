//! What a root offers an installer, and the install that runs from it.
//!
//! Find a payload, read back the recipe `emit::recipe` wrote, add the person's
//! half — the disk and the account — and hand it to fisherman, which owns
//! partitioning, LUKS, TPM2 enrolment and `bootc install`.
//!
//! A payload root is one carrying `install-recipe.json`. A payload wins over a
//! repository here, the opposite of `command::Context::of`'s precedence. Roots
//! are found by the filesystem label `TECT`; more than one is refused.

use crate::copy;
use crate::emit::json::{self, Json};
use crate::prompt::Prompt;
use crate::ui::Choice;
use std::io::{BufRead as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The document a payload root carries, written by `emit::recipe::build` and
/// baked onto installer media by `tect vm build iso`.
pub const RECIPE: &str = "install-recipe.json";

/// The root `--from` defaults to: the directory the build's own documents live
/// in, which is where the media carries its recipe beside the manifest.
pub fn media() -> PathBuf {
    Path::new(crate::provenance::build::MANIFEST)
        .parent()
        .unwrap_or(Path::new("/"))
        .to_path_buf()
}

/// A payload: a recipe, and the two of its fields a person is shown before
/// they agree to erase a disk.
#[derive(Debug)]
pub struct Payload {
    pub recipe: PathBuf,
    /// The reference fisherman installs, which on media is the published name
    /// the local bytes are embedded under.
    pub image: String,
    /// The name the installed machine takes, and the one derived value a
    /// person is expected to replace.
    pub hostname: String,
    /// The root filesystem, which the base family settles and no question
    /// offers: a composefs-sealed deployment needs fs-verity, which xfs has
    /// not got. Shown so somebody about to erase a disk can see it.
    pub filesystem: String,
    /// `grub2` or `systemd`, which is what decides whether the layout has a
    /// separate `/boot` at all.
    pub bootloader: String,
}

/// Which of the three cases a root is.
#[derive(Debug)]
pub enum Found {
    /// Built bytes and the recipe for them: install, and no network.
    Image(Payload),
    /// A repository: the source, so it has to be built before anything can be
    /// installed from it.
    Repo(PathBuf),
    /// Neither. It keeps the root so the refusal can name where it looked,
    /// which matters most on the default one nobody typed.
    Nothing(PathBuf),
}

/// A malformed recipe is a refusal naming the file. Falling through to the
/// next case would treat a stick that carries a payload and cannot install it
/// as a stick that carries nothing.
pub fn classify(root: &Path) -> Result<Found, String> {
    let recipe = root.join(RECIPE);
    if recipe.is_file() {
        let raw = std::fs::read_to_string(&recipe)
            .map_err(|err| format!("{}: {err}", recipe.display()))?;
        let doc = Json::parse(&raw).map_err(|err| format!("{}: {err}", recipe.display()))?;
        let field = |key: &str| {
            json::text(&doc, key)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("{}: no `{key}`", recipe.display()))
        };
        // The two below are read and not required: a recipe this tool wrote
        // always carries them, and one that does not is still installable —
        // it is the summary that goes quiet, not the install.
        let told = |key: &str| json::text(&doc, key).unwrap_or_default();
        return Ok(Found::Image(Payload {
            image: field("image")?,
            hostname: field("hostname")?,
            filesystem: told("filesystem"),
            bootloader: told("bootloader"),
            recipe,
        }));
    }
    Ok(match root.join(crate::layout::REPO_FILE).is_file() {
        true => Found::Repo(root.to_path_buf()),
        false => Found::Nothing(root.to_path_buf()),
    })
}

/// The label a payload partition carries, which is the whole rule for finding
/// a root nobody named.
pub const LABEL: &str = "TECT";

/// Where a labelled partition is mounted: under `/run`, which in a live
/// environment is RAM, so reading a payload writes to no disk.
const MOUNTPOINT: &str = "/run/tect-payload";

/// The root to classify when no `--from` named one. A `TECT` partition
/// overrides the media's own payload; more than one is refused naming them,
/// since picking wrong erases a disk from the wrong image.
pub fn root() -> Result<PathBuf, String> {
    let listed = Command::new("blkid")
        .args(["-t", &format!("LABEL={LABEL}"), "-o", "device"])
        .output();
    let devices = match &listed {
        Ok(out) => labelled(&String::from_utf8_lossy(&out.stdout)),
        Err(_) => Vec::new(),
    };
    match devices.as_slice() {
        [] => Ok(media()),
        [device] => mounted(device),
        many => Err(format!(
            "{} partitions are labelled {LABEL}, so which one to install from is not \
             clear: {}\n\nhelp: `tect installer --from <root>` names one outright",
            many.len(),
            many.join(", ")
        )),
    }
}

fn labelled(listed: &str) -> Vec<String> {
    listed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Read-only, and left mounted: fisherman reads the store beside the recipe,
/// and this environment ends at the reboot.
fn mounted(device: &str) -> Result<PathBuf, String> {
    let at = PathBuf::from(MOUNTPOINT);
    // A second run finds its own mount and carries on over it.
    if at.join(RECIPE).is_file() {
        return Ok(at);
    }
    std::fs::create_dir_all(&at).map_err(|err| format!("{MOUNTPOINT}: {err}"))?;
    let out = Command::new("mount")
        .args(["-o", "ro", device, MOUNTPOINT])
        .output()
        .map_err(|err| format!("mount: {err}, and it is what reads a {LABEL} partition"))?;
    match out.status.success() {
        true => Ok(at),
        false => Err(format!(
            "mounting {device} at {MOUNTPOINT}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// The four fisherman takes. The two whose name ends in `passphrase` are the
/// two it refuses the recipe without one.
const KINDS: [(&str, &str); 4] = [
    (NONE, copy::ENC_NONE),
    ("tpm2-luks", copy::ENC_TPM2),
    ("luks-passphrase", copy::ENC_PASSPHRASE),
    ("tpm2-luks-passphrase", copy::ENC_BOTH),
];

const NONE: &str = "none";

/// What a machine with a TPM has, and what the two `tpm2-` forms need.
/// `$TECT_TPM` overrides the path, so the drawn golden does not depend on the
/// machine running it having one.
const TPM: &str = "/dev/tpmrm0";

fn tpm() -> PathBuf {
    std::env::var_os("TECT_TPM")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(TPM))
}

/// Where the whole disks are read from.
const SYS_BLOCK: &str = "/sys/block";

/// `$TECT_SYS_BLOCK` names it instead where it is set, for the same reason
/// `$TECT_TPM` exists: the form draws the disks this machine has, and no two
/// machines running the drawn golden agree on what those are.
fn sys_block() -> PathBuf {
    std::env::var_os("TECT_SYS_BLOCK")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SYS_BLOCK))
}

pub struct Encryption {
    pub kind: String,
    pub passphrase: String,
}

impl Encryption {
    fn none() -> Self {
        Self {
            kind: NONE.to_string(),
            passphrase: String::new(),
        }
    }

    fn wants_passphrase(kind: &str) -> bool {
        kind.ends_with("passphrase")
    }
}

/// The person's half, which nothing derives and no flag defaults.
pub struct Answers {
    pub disk: String,
    pub hostname: String,
    pub user: String,
    pub password: String,
    pub encryption: Encryption,
}

/// What the flags gave, which the first pass reads and a re-ask does not: a
/// field asked again opens on the answer it has, not on the flag that seeded
/// it.
#[derive(Clone, Default)]
pub struct Given {
    pub disk: Option<String>,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub encryption: Option<String>,
    pub passphrase: Option<String>,
}

/// What a leave key is answered with. All three are true for as long as no
/// disk has been touched, which is the whole of when this is asked.
enum Leave {
    /// Back to the screen the key was pressed on, with the answers kept.
    Back,
    /// The questions again, from the first one.
    Over,
    /// Out, having written nothing.
    Shell,
}

/// Whether a widget's error is a person wanting out. Esc is the other half of
/// that and is a `None`, not an error.
fn leaving(err: &str) -> bool {
    err == crate::ui::INTERRUPTED
}

/// What a leave key asks before it leaves.
fn leave(prompt: &Prompt) -> Result<Leave, String> {
    // Only a drawn run can reach a leave key at all, and this is what stops a
    // run that cannot be asked from looping over a question it never sees.
    if !prompt.draws() {
        return Ok(Leave::Shell);
    }
    let options = [
        Choice::new(copy::LEAVE_BACK, ""),
        Choice::new(copy::LEAVE_OVER, ""),
        Choice::new(copy::LEAVE_SHELL, ""),
    ];
    // A leave key on the question about leaving is not a second leaving.
    match prompt.choose(copy::LEAVING, &options) {
        Ok(Some(1)) => Ok(Leave::Over),
        Ok(Some(2)) => Ok(Leave::Shell),
        Ok(_) => Ok(Leave::Back),
        Err(err) if leaving(&err) => Ok(Leave::Back),
        Err(err) => Err(err),
    }
}

impl Answers {
    /// The screens, in order, then the review over them, and around both the
    /// question every leave key asks first. `None` is a leaving that was
    /// confirmed: nothing has been written and no disk has been touched.
    pub fn collect(
        payload: &Payload,
        given: Given,
        prompt: &Prompt,
    ) -> Result<Option<Self>, String> {
        let seeded = Self::seeded(payload, given, prompt)?;
        // No screen, no form: the flags are the whole of the answer and
        // `seeded` has already refused anything they left empty.
        if !prompt.draws() {
            return Ok(Some(seeded));
        }
        let mut fields = seeded.fields(&payload.filesystem, &payload.bootloader);
        loop {
            let actions = [copy::INSTALL, copy::SHUT_DOWN];
            let filled =
                crate::ui::form(&mut fields, &actions, short_of, asked, copy::INSTALL_KEYS);
            match filled {
                Ok(crate::ui::Filled::Took(0)) => {
                    let answers = Self::of(&fields);
                    // The one question that costs a disk, asked over what it
                    // would do, after the form is complete and never before.
                    if crate::ui::confirm_over(
                        &copy::erasing(&answers.disk),
                        &answers.summary(payload),
                        copy::CONTINUE,
                        copy::GO_BACK,
                    )? {
                        return Ok(Some(answers));
                    }
                }
                // Every other action is a way off this screen.
                Ok(crate::ui::Filled::Took(_)) => {
                    power_off()?;
                    return Ok(None);
                }
                Ok(crate::ui::Filled::Left) => match leave(prompt)? {
                    Leave::Shell => return Ok(None),
                    Leave::Over => {
                        fields = Self::seeded(payload, Given::default(), prompt)?
                            .fields(&payload.filesystem, &payload.bootloader)
                    }
                    Leave::Back => {}
                },
                Err(err) if leaving(&err) => match leave(prompt)? {
                    Leave::Shell => return Ok(None),
                    Leave::Over => {
                        fields = Self::seeded(payload, Given::default(), prompt)?
                            .fields(&payload.filesystem, &payload.bootloader)
                    }
                    Leave::Back => {}
                },
                Err(err) => return Err(err),
            }
        }
    }

    /// Every field before any of them is asked. On a screen nothing is asked
    /// here: the flags and defaults seed the form, and the form asks. With no
    /// screen there is no form, and a value no flag gave is a refusal naming
    /// the flag.
    fn seeded(payload: &Payload, given: Given, prompt: &Prompt) -> Result<Self, String> {
        if !prompt.draws() {
            return Ok(Self {
                disk: ask_disk(given.disk, None, prompt)?,
                hostname: prompt.text(
                    given.hostname,
                    copy::INSTALL_NAME,
                    "--hostname",
                    Some(&payload.hostname),
                )?,
                user: prompt.text(given.user, copy::INSTALL_USER, "--user", None)?,
                password: prompt.secret(given.password, copy::INSTALL_PASSWORD, "--password")?,
                encryption: ask_encryption(
                    given.encryption,
                    given.passphrase,
                    &Encryption::none(),
                    prompt,
                )?,
            });
        }
        Ok(Self {
            disk: given.disk.unwrap_or_default(),
            hostname: given
                .hostname
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| payload.hostname.clone()),
            user: given.user.unwrap_or_default(),
            password: given.password.unwrap_or_default(),
            encryption: Encryption {
                // A flag naming a kind fisherman has not got is refused here.
                // Drawn as a row, it would be one nobody can correct.
                kind: match given.encryption {
                    Some(kind) => named(kind)?,
                    None => NONE.to_string(),
                },
                passphrase: given.passphrase.unwrap_or_default(),
            },
        })
    }

    /// The form's rows, in the order `ROW_*` names them. The passphrase row is
    /// always present: the list is built once, so a row that came and went
    /// would rebuild the screen under the person editing it.
    fn fields(&self, filesystem: &str, bootloader: &str) -> Vec<crate::ui::Field> {
        use crate::ui::Field;
        let found = disks(&sys_block(), &in_use_now());
        let at = found.iter().position(|(disk, _)| *disk == self.disk);
        let disk = match found.is_empty() {
            // A machine whose `/sys/block` says nothing is typed into.
            true => Field::text(copy::ROW_DISK, &self.disk),
            false => Field::pick(
                copy::ROW_DISK,
                found
                    .iter()
                    .map(|(disk, detail)| Choice::new(disk, detail))
                    .collect(),
                at,
            ),
        };
        vec![
            disk,
            // Under the disk it describes, and answerable by nobody.
            Field::fixed(copy::ROW_LAYOUT, &copy::layout(filesystem, bootloader)),
            Field::text(copy::ROW_HOSTNAME, &self.hostname),
            Field::text(copy::ROW_ACCOUNT, &self.user),
            Field::secret(copy::ROW_PASSWORD, &self.password),
            Field::secret(copy::ROW_CONFIRM, &self.password),
            Field::pick(
                copy::ROW_ENCRYPTION,
                kinds(tpm().exists()),
                KINDS
                    .iter()
                    .position(|(name, _)| *name == self.encryption.kind),
            ),
            Field::secret(copy::ROW_PASSPHRASE, &self.encryption.passphrase),
        ]
    }

    /// The answers the form holds. Only reached where `short_of` is empty, so
    /// every value here is one somebody typed or chose.
    fn of(fields: &[crate::ui::Field]) -> Self {
        let at = |row: usize| fields[row].value();
        Self {
            disk: at(ROW_DISK),
            hostname: at(ROW_HOSTNAME),
            user: at(ROW_ACCOUNT),
            password: at(ROW_PASSWORD),
            encryption: Encryption {
                kind: at(ROW_ENCRYPTION),
                passphrase: at(ROW_PASSPHRASE),
            },
        }
    }

    /// What the confirmation is asked over: the answers, with the password as
    /// the one value that cannot read back as itself.
    fn summary(&self, payload: &Payload) -> Vec<(String, String)> {
        let mut rows = vec![
            (copy::ROW_DISK.to_string(), self.disk.clone()),
            (copy::ROW_HOSTNAME.to_string(), self.hostname.clone()),
            (copy::ROW_ACCOUNT.to_string(), self.user.clone()),
            (
                copy::ROW_PASSWORD.to_string(),
                copy::PASSWORD_SET.to_string(),
            ),
            (
                copy::ROW_ENCRYPTION.to_string(),
                self.encryption.kind.clone(),
            ),
        ];
        // What the disk is about to be cut into. Nobody chose any of it, which
        // is why it is here: it is the half of what is being written that no
        // question above covers.
        rows.extend(copy::written_over(&payload.bootloader, &payload.filesystem));
        rows
    }
}

/// The form's rows, by position, for the code that reads one back. Row 1 is the
/// layout, which is derived from the payload and read back by nothing.
const ROW_DISK: usize = 0;
const ROW_HOSTNAME: usize = 2;
const ROW_ACCOUNT: usize = 3;
const ROW_PASSWORD: usize = 4;
const ROW_CONFIRM: usize = 5;
const ROW_ENCRYPTION: usize = 6;
const ROW_PASSPHRASE: usize = 7;

/// Which rows are questions. The passphrase is one only for the two encryption
/// forms named for one; on the others it is not a field a person can answer
/// wrongly, so it is not a field.
fn asked(fields: &[crate::ui::Field]) -> Vec<usize> {
    let wants = Encryption::wants_passphrase(&fields[ROW_ENCRYPTION].value());
    (0..fields.len())
        .filter(|row| *row != ROW_PASSPHRASE || wants)
        .collect()
}

/// What the form is still short of, which is what `Install` says while it is
/// unpickable: the values nothing derives and no default covers, plus that both
/// halves of the password agree.
fn short_of(fields: &[crate::ui::Field]) -> Option<String> {
    let at = |row: usize| fields[row].value();
    if !at(ROW_PASSWORD).is_empty() && at(ROW_PASSWORD) != at(ROW_CONFIRM) {
        return Some(copy::NO_MATCH_ROW.to_string());
    }
    let wants = Encryption::wants_passphrase(&at(ROW_ENCRYPTION));
    let missing: Vec<&str> = [
        (at(ROW_DISK).is_empty(), copy::ROW_DISK),
        (at(ROW_ACCOUNT).is_empty(), copy::ROW_ACCOUNT),
        (at(ROW_PASSWORD).is_empty(), copy::ROW_PASSWORD),
        (wants && at(ROW_PASSPHRASE).is_empty(), copy::ROW_PASSPHRASE),
    ]
    .into_iter()
    .filter(|(missing, _)| *missing)
    .map(|(_, name)| name)
    .collect();
    match missing.is_empty() {
        true => None,
        false => Some(copy::still_needs(&missing)),
    }
}

/// The other way off the installer's screen. The live environment is a
/// systemd one, which is what put this on a console in the first place.
fn power_off() -> Result<(), String> {
    let out = Command::new("systemctl")
        .arg("poweroff")
        .output()
        .map_err(|err| format!("systemctl: {err}, and it is what stops a machine"))?;
    match out.status.success() {
        true => Ok(()),
        false => Err(format!(
            "systemctl poweroff: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// Half a kilobyte, which is what `/sys/block/<disk>/size` counts whatever the
/// device's own sector size is.
const SECTOR: u64 = 512;

/// Not disks anyone installs onto, and each one is only an option to get wrong.
const VIRTUAL: [&str; 7] = ["loop", "ram", "zram", "sr", "fd", "dm-", "md"];

/// What `/proc/mounts` says. Read once per listing and handed in, so the rule
/// below can be driven by a test.
fn in_use_now() -> String {
    std::fs::read_to_string("/proc/mounts").unwrap_or_default()
}

/// Whether the running system is already using this disk.
///
/// On installer media that is the medium itself: a whole disk like any other,
/// and `/sys/block` says nothing about which one was booted, so without this it
/// is offered beside the machine's own disks and partitioning it erases the
/// wrong thing. The question is what is mounted, not what is writable — a stick
/// written with `dd` is not read-only.
fn in_use(disk: &Path, name: &str, mounts: &str) -> bool {
    let is_source = |dev: &str| {
        mounts
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .any(|source| source == dev)
    };
    if is_source(&format!("/dev/{name}")) {
        return true;
    }
    let Ok(parts) = std::fs::read_dir(disk) else {
        return false;
    };
    parts.flatten().any(|part| {
        part.path().join("partition").is_file()
            && is_source(&format!("/dev/{}", part.file_name().to_string_lossy()))
    })
}

/// The whole disks this machine has, as `/sys/block` holds them, with what a
/// person needs to tell one from another beside each. `mounts` is
/// `/proc/mounts`, which keeps the medium this is running from out of the list.
pub fn disks(sys: &Path, mounts: &str) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(sys) else {
        return Vec::new();
    };
    let mut found: Vec<(String, String)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if VIRTUAL.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }
        let read = |leaf: &str| {
            std::fs::read_to_string(entry.path().join(leaf))
                .map(|text| text.trim().to_string())
                .unwrap_or_default()
        };
        let sectors: u64 = read("size").parse().unwrap_or(0);
        if sectors == 0 {
            continue;
        }
        // A disk nothing can be written to is not an installation target, and
        // a disk the running system is already using is the medium this is
        // installing from.
        if read("ro") == "1" || in_use(&entry.path(), &name, mounts) {
            continue;
        }
        let removable = match read("removable").as_str() {
            "1" => copy::REMOVABLE.to_string(),
            _ => String::new(),
        };
        let detail = [
            format!("{} GB", sectors * SECTOR / 1_000_000_000),
            read("device/model"),
            removable,
        ];
        found.push((
            format!("/dev/{name}"),
            detail
                .iter()
                .filter(|part| !part.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("  "),
        ));
    }
    found.sort();
    found
}

/// No default disk anywhere: with nobody to ask, a missing one is a refusal
/// naming `--disk`, and a machine whose `/sys/block` says nothing is typed
/// into.
fn ask_disk(
    given: Option<String>,
    current: Option<&str>,
    prompt: &Prompt,
) -> Result<String, String> {
    if let Some(disk) = given.filter(|disk| !disk.is_empty()) {
        return Ok(disk);
    }
    let found = disks(&sys_block(), &in_use_now());
    if !prompt.asks() || found.is_empty() {
        return prompt.text(None, copy::INSTALL_DISK, "--disk", current);
    }
    let options: Vec<Choice> = found
        .iter()
        .map(|(disk, detail)| Choice::new(disk, detail))
        .collect();
    let at = current
        .and_then(|held| found.iter().position(|(disk, _)| disk == held))
        .unwrap_or(0);
    match prompt.choose_current(copy::INSTALL_DISK, &options, at)? {
        Some(at) => Ok(found[at].0.clone()),
        // Left unanswered: keep whatever the form had. That is nothing the
        // first time, and it is what blocks the action.
        None => Ok(current.unwrap_or_default().to_string()),
    }
}

/// The kind, if fisherman has one by that name. The form validates a flag this
/// way without asking anything, which is why it is not inline in the question.
fn named(kind: String) -> Result<String, String> {
    if KINDS.iter().any(|(name, _)| *name == kind) {
        return Ok(kind);
    }
    Err(format!(
        "`{kind}` is not one of {}",
        KINDS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// A `tpm2-` form on a machine with no TPM is shown and refuses the key that
/// would pick it: what it needs is the reason it is worth showing.
fn kinds(tpm: bool) -> Vec<Choice> {
    KINDS
        .iter()
        .map(|(name, detail)| match tpm || !name.starts_with("tpm2") {
            true => Choice::new(*name, *detail),
            false => Choice::new(*name, copy::NO_TPM).unavailable(),
        })
        .collect()
}

/// A `tpm2-` form on a machine with no TPM is shown and not pickable. The
/// passphrase is asked every time the kind is, so editing the row can change it.
fn ask_encryption(
    given: Option<String>,
    passphrase: Option<String>,
    current: &Encryption,
    prompt: &Prompt,
) -> Result<Encryption, String> {
    let kind = match given {
        Some(kind) => named(kind)?,
        None if !prompt.asks() => current.kind.clone(),
        None => {
            let options = kinds(tpm().exists());
            let at = KINDS
                .iter()
                .position(|(name, _)| *name == current.kind)
                .unwrap_or(0);
            match prompt.choose_current(copy::INSTALL_ENCRYPTION, &options, at)? {
                Some(at) => KINDS[at].0.to_string(),
                None => current.kind.clone(),
            }
        }
    };
    Ok(Encryption {
        passphrase: match (Encryption::wants_passphrase(&kind), prompt.draws()) {
            (false, _) => String::new(),
            // The form's own re-ask, where leaving the question keeps the
            // passphrase already held and the action stays blocked while there
            // is none.
            (true, true) => prompt.secret_current(copy::LUKS_PASSPHRASE, &current.passphrase)?,
            (true, false) => prompt.secret(passphrase, copy::LUKS_PASSPHRASE, "--passphrase")?,
        },
        kind,
    })
}

/// Replaces the value under `key`, or appends it. Anything that is not an
/// object is left alone, so a recipe whose `user` is a string fails in
/// fisherman's own reader.
fn set(value: &mut Json, key: &str, field: Json) {
    let Json::Object(fields) = value else { return };
    match fields.iter_mut().find(|(name, _)| name == key) {
        Some((_, held)) => *held = field,
        None => fields.push((key.to_string(), field)),
    }
}

/// The recipe with the person's half in it. `user` is merged: the groups
/// already in it are the target's admin group, and `useradd` refuses the whole
/// call when it names a group the target has not got.
pub fn complete(recipe: &Path, answers: &Answers) -> Result<Json, String> {
    let raw =
        std::fs::read_to_string(recipe).map_err(|err| format!("{}: {err}", recipe.display()))?;
    let mut doc = Json::parse(&raw).map_err(|err| format!("{}: {err}", recipe.display()))?;
    set(&mut doc, "disk", Json::string(&answers.disk));
    set(&mut doc, "hostname", Json::string(&answers.hostname));
    set(
        &mut doc,
        "encryption",
        Json::object([
            ("type", Json::string(&answers.encryption.kind)),
            ("passphrase", Json::string(&answers.encryption.passphrase)),
        ]),
    );
    let mut user = match doc {
        Json::Object(ref mut fields) => match fields.iter().position(|(name, _)| name == "user") {
            Some(at) => fields.remove(at).1,
            None => Json::object([]),
        },
        _ => return Err(format!("{}: not an object", recipe.display())),
    };
    set(&mut user, "username", Json::string(&answers.user));
    set(
        &mut user,
        "password",
        Json::string(hashed(&answers.password)?),
    );
    set(&mut doc, "user", user);
    Ok(doc)
}

/// A `$`-prefixed crypt string: fisherman hands the field to `chpasswd` and
/// only a `$` takes the `-e` branch. Plaintext goes through PAM and dies
/// `pam_chauthtok() failed, error: Module is unknown` after the OS is already
/// on the disk. `openssl passwd` because crypt(3) lives in libcrypt, which this
/// binary does not link; `-stdin` keeps it out of `ps`.
fn hashed(password: &str) -> Result<String, String> {
    let mut child = Command::new("openssl")
        .args(["passwd", "-6", "-stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|err| format!("openssl: {err}, and it is what hashes the password"))?;
    child
        .stdin
        .take()
        .ok_or("openssl: no stdin")?
        .write_all(format!("{password}\n").as_bytes())
        .map_err(|err| format!("openssl: {err}"))?;
    let out = child
        .wait_with_output()
        .map_err(|err| format!("openssl: {err}"))?;
    let hash = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match out.status.success() && hash.starts_with('$') {
        true => Ok(hash),
        false => Err(format!(
            "openssl passwd wrote no crypt string, and a plaintext password loses the install at \
             its last step: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// Where the completed recipe is written. It carries a password hash, so it is
/// created 0600 and nothing widens it; on installer media `TMPDIR` is `/tmp`,
/// which is RAM and never reaches the disk.
fn stage(recipe: &Json) -> Result<PathBuf, String> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let path = std::env::temp_dir().join(format!("tect-install.{}.json", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|err| format!("{}: {err}", path.display()))?;
    file.write_all(recipe.render().as_bytes())
        .map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(path)
}

/// What fisherman is called. Not a path: the live environment puts it on
/// `PATH`, and a headless run elsewhere is free to shadow it.
const BACKEND: &str = "fisherman";

/// How long the screen waits for a line before it redraws anyway. Fast enough
/// that the spinner turns, slow enough that a quiet install is not a loop
/// repainting a console eight times a second.
const TICK: std::time::Duration = std::time::Duration::from_millis(120);

impl Found {
    /// The payload, or why this root has none. Asked *before* a person is
    /// asked for a disk to erase, because a precondition that can be checked
    /// early and is checked late is a bug in an installer.
    pub fn payload(&self) -> Result<&Payload, String> {
        match self {
            Self::Image(payload) => Ok(payload),
            // Not built here: the scratch has to go on the target disk, which
            // means partitioning before the build and an erased disk when the
            // build fails.
            Self::Repo(root) => Err(format!(
                "{} is a repository, so there is nothing here to install yet\n\nhelp: \
                 `tect build` in it, then `tect installer --from <the built payload>`",
                root.display()
            )),
            Self::Nothing(root) => Err(format!(
                "{} holds no {RECIPE} and no {}, so it is neither a payload nor a repository",
                root.display(),
                crate::layout::REPO_FILE
            )),
        }
    }
}

/// One line of fisherman's event stream as the thing it is. A gauge and a log
/// want different halves of the same line — how far through, and what it said
/// — so the line is read once and rendered where it is needed.
enum Event {
    /// How far through, the share this step carries, and what is happening.
    /// `cumulative_pct` is the work finished *before* this step, so without
    /// `weight_pct` beside it a bar cannot show the span it is inside.
    Step(u16, u16, String),
    /// A line under the step.
    Note(String),
    /// The last step, which is always the whole of it.
    Done(String),
    /// The only copy there will ever be, and the disk does not open without it
    /// if the TPM stops answering. It must never reach the log — a key on
    /// removable media turns the stick into the thing that opens the disk.
    Recovery(String),
    /// Not one of its events, and kept as it came: what fisherman's own
    /// backends write is half of what a failed install is read back from.
    Other(String),
}

impl Event {
    fn of(line: &str) -> Self {
        let Ok(event) = Json::parse(line) else {
            return Self::Other(line.to_string());
        };
        let text = |key: &str| json::text(&event, key).unwrap_or_default();
        let count = |key: &str| json::number(&event, key).unwrap_or(0);
        match json::text(&event, "type").as_deref() {
            Some("step") => Self::Step(
                count("cumulative_pct") as u16,
                count("weight_pct") as u16,
                format!(
                    "{}/{} {}",
                    count("step"),
                    count("total_steps"),
                    text("step_name")
                ),
            ),
            Some("info" | "substep") => Self::Note(text("message")),
            Some("complete") => Self::Done(text("message")),
            Some("recovery_key") => Self::Recovery(text("key")),
            _ => Self::Other(line.to_string()),
        }
    }

    /// What the log holds, which is everything except the recovery key.
    fn logged(&self) -> Option<String> {
        match self {
            Self::Recovery(_) => None,
            said => Some(said.say()),
        }
    }

    /// The line this reads as: what a run with no screen to draw on prints.
    fn say(&self) -> String {
        match self {
            Self::Step(pct, _, what) => format!("[{pct:>3}%] {what}"),
            Self::Note(message) => format!("       {message}"),
            Self::Done(message) => format!("[100%] {message}"),
            Self::Recovery(key) => format!("\n{}\n", copy::recovery(key)),
            Self::Other(line) => line.clone(),
        }
    }
}

/// The renderer a deb image ships and a fedora one does not: their signed GRUB
/// reads no BLS entries. Run from the image — a composefs deployment on the
/// disk is sealed erofs with no walkable `/usr`.
const RENDERER: &str = "/usr/libexec/grub-menu-from-bls";

/// Where the target's boot filesystem is mounted while the menu is written.
/// The renderer takes a root and looks under `<root>/boot`.
const TARGET: &str = "/run/tect-target";

/// The menu the installed machine boots from, written after fisherman has
/// finished and unmounted: `bootc` installs the bootloader before it writes the
/// entries, so nothing during the install can render them. An image with no
/// renderer is skipped; anything else is an error.
///
/// `render_menu` in `assets/scripts/vm.sh` is this algorithm over a raw disk
/// file: it runs as a user and has to `losetup -P` first, which is why the two
/// are separate.
fn render_menu(image: &str, disk: &str) -> Result<(), String> {
    let at = PathBuf::from(TARGET);
    let boot = at.join("boot");
    std::fs::create_dir_all(&boot).map_err(|err| format!("{TARGET}: {err}"))?;

    let Some((device, root)) = boot_partition(disk, &boot)? else {
        return Err(format!(
            "no partition of {disk} carries `loader/entries`, so there is no menu to render"
        ));
    };
    let rendered = run_renderer(image, root);
    let _ = Command::new("umount").arg(&boot).output();
    match rendered {
        Ok(true) => {
            eprintln!("tect: wrote the boot menu {device} needs, since its GRUB reads no BLS");
            Ok(())
        }
        Ok(false) => Ok(()),
        Err(err) => Err(err),
    }
}

/// The first partition of `disk` whose filesystem carries the boot entries,
/// left mounted at `boot`, with the root the renderer wants for it: the top of
/// a /boot partition, a level down where /boot is a directory on the root.
/// Found by content, not by partition label.
fn boot_partition(disk: &str, boot: &Path) -> Result<Option<(String, &'static str)>, String> {
    let listed = Command::new("lsblk")
        .args(["-nrpo", "NAME", disk])
        .output()
        .map_err(|err| format!("lsblk: {err}, and it is what lists a disk's partitions"))?;
    for device in labelled(&String::from_utf8_lossy(&listed.stdout)) {
        if device == disk {
            continue;
        }
        let mounted = Command::new("mount")
            .args([&device, &boot.to_string_lossy().to_string()])
            .output();
        if !matches!(&mounted, Ok(out) if out.status.success()) {
            continue;
        }
        if boot.join("loader/entries").is_dir() {
            return Ok(Some((device, "/target")));
        }
        if boot.join("boot/loader/entries").is_dir() {
            return Ok(Some((device, "/target/boot")));
        }
        let _ = Command::new("umount").arg(boot).output();
    }
    Ok(None)
}

/// `false` where the image ships no renderer, which is how a fedora target is
/// skipped without this knowing which families have `blscfg`.
fn run_renderer(image: &str, root: &str) -> Result<bool, String> {
    let out = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--net=none",
            "--security-opt",
            "label=disable",
            // A built image inheriting an entrypoint would take the shell line
            // as arguments to it. vm.sh and the scan workflow clear it too.
            "--entrypoint",
            "",
            "-v",
            &format!("{TARGET}:/target"),
            image,
            "/bin/sh",
            "-c",
            &format!("test -x {RENDERER} || exit 3; exec {RENDERER} {root}"),
        ])
        .output()
        .map_err(|err| format!("podman: {err}, and it is what runs the image's renderer"))?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(3) => Ok(false),
        _ => Err(format!(
            "{RENDERER} in {image}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// The name the whole of fisherman's output is written under.
const LOG: &str = "tect-install.log";

/// Where it goes when nothing else can hold it: RAM, which is the whole of
/// what an iso-only boot has.
const IN_RAM: &str = "/run";

/// Where the log goes: beside the payload where its partition can be remounted
/// writable — `root()` mounts it read-only, and an image's own
/// `/usr/share/tectonic` never can be — and in RAM otherwise, which is every
/// iso-only boot. Which of the two happened is printed.
fn open_log(payload: &Payload) -> (Option<std::fs::File>, Option<PathBuf>) {
    if payload.recipe.parent() == Some(Path::new(MOUNTPOINT)) && remounted_rw(MOUNTPOINT) {
        let path = Path::new(MOUNTPOINT).join(LOG);
        if let Ok(file) = std::fs::File::create(&path) {
            return (Some(file), Some(path));
        }
    }
    let path = Path::new(IN_RAM).join(LOG);
    match std::fs::File::create(&path) {
        Ok(file) => (Some(file), Some(path)),
        // Neither, which is a live environment with nothing writable at all.
        // The screen is then the only copy and says so.
        Err(_) => (None, None),
    }
}

/// The payload partition is mounted read-only, so a log beside the payload is
/// a deliberate remount.
fn remounted_rw(at: &str) -> bool {
    Command::new("mount")
        .args(["-o", "remount,rw", at])
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Completes the recipe and runs fisherman over it, drawing its event stream
/// into a bounded region and writing all of it to a file.
///
/// A failed draw is not a failed install, so nothing here is `?` on the region.
pub fn run(payload: &Payload, answers: &Answers, prompt: &Prompt) -> Result<(), String> {
    let path = stage(&complete(&payload.recipe, answers)?)?;
    let (mut log, at) = open_log(payload);
    // Only where nothing is drawn. On the installer's own screen this would
    // land above the box and stay there, because a bounded region redraws
    // itself and never the row over it.
    if !prompt.draws() {
        eprintln!(
            "tect: installing {} as {} onto {}, {}",
            payload.image,
            answers.hostname,
            answers.disk,
            copy::logging(at.as_deref())
        );
    }
    // One pipe for both streams, so fisherman's stderr is an event like any
    // other. Inherited, it would print straight onto the drawn region.
    let (events, writer) = std::io::pipe().map_err(|err| format!("{BACKEND}: {err}"))?;
    let errors = writer
        .try_clone()
        .map_err(|err| format!("{BACKEND}: {err}"))?;
    let mut child = Command::new(BACKEND)
        .arg(&path)
        .stdout(Stdio::from(writer))
        .stderr(Stdio::from(errors))
        .spawn()
        .map_err(|err| format!("{BACKEND}: {err}"))?;
    let mut region = match prompt.draws() {
        true => crate::ui::Progress::open(&copy::writing(at.as_deref())).ok(),
        false => None,
    };
    let mut recovery = None;
    {
        // Read on a thread and taken with a timeout, so the region keeps
        // drawing while fisherman is silent; a step can hold the machine for
        // minutes between two messages. Both write ends moved into the child,
        // so the read ends when the child does and this loop with it.
        let (lines, arriving) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(events)
                .lines()
                .map_while(Result::ok)
            {
                if lines.send(line).is_err() {
                    return;
                }
            }
        });
        loop {
            let line = match arriving.recv_timeout(TICK) {
                Ok(line) => line,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(region) = &mut region {
                        let _ = region.tick();
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            let event = Event::of(&line);
            if let Event::Recovery(key) = &event {
                recovery = Some(key.clone());
            }
            if let (Some(said), Some(log)) = (event.logged(), &mut log) {
                let _ = writeln!(log, "{said}");
            }
            match &mut region {
                None => println!("{}", event.say()),
                Some(region) => {
                    let _ = match &event {
                        Event::Step(pct, flight, what) => region.step(*pct, *flight, what),
                        Event::Done(message) => region.step(100, 0, message),
                        // Held back until the region closes: it is the one
                        // thing on this screen worth reading twice.
                        Event::Recovery(_) => Ok(()),
                        Event::Note(message) => region.note(message),
                        Event::Other(line) => region.note(line),
                    };
                }
            }
        }
    }
    let finished = child.wait().map_err(|err| format!("{BACKEND}: {err}"));
    if let Some(region) = region {
        region.close();
    }
    // It carries the password hash and the passphrase, and the install is over.
    let _ = std::fs::remove_file(&path);
    let status = finished?;
    if !status.success() {
        return Err(format!(
            "{BACKEND} did not finish: {status}, and {}",
            copy::logging(at.as_deref())
        ));
    }
    render_menu(&payload.image, &answers.disk)?;
    finish(recovery.as_deref(), at.as_deref(), prompt)
}

/// What the last screen owes: the recovery key, on screen because it is
/// deliberately in no file, and the restart, because the stick is still in the
/// machine and nothing else says what to do next.
fn finish(recovery: Option<&str>, log: Option<&Path>, prompt: &Prompt) -> Result<(), String> {
    // Nothing draws, so the streams are the only channel there is.
    if !prompt.draws() {
        if let Some(key) = recovery {
            println!("\n{}", copy::recovery(key));
            println!("{}\n", copy::KEY_NOT_LOGGED);
        }
        eprintln!("tect: {}", copy::logging(log));
        return Ok(());
    }
    // Inside the box, all of it. A key held in no file and shown on no screen
    // is a disk nobody can open.
    let mut rows = Vec::new();
    if let Some(key) = recovery {
        rows.push(Choice::new(copy::WRITE_DOWN, "").content());
        // The one row on the screen a person has to copy by eye.
        rows.push(Choice::new(key, "").content().tinted());
        rows.push(Choice::new(copy::KEY_NOT_LOGGED, "").content());
    }
    rows.push(Choice::new(copy::logging(log), "").content());
    match crate::ui::offer_over(copy::INSTALL_DONE, rows, copy::RESTART, copy::DONE_KEYS)? {
        false => Ok(()),
        true => restart(),
    }
}

/// The live environment is a systemd one, which is what puts the installer on
/// its console in the first place.
fn restart() -> Result<(), String> {
    let out = Command::new("systemctl")
        .arg("reboot")
        .output()
        .map_err(|err| format!("systemctl: {err}, and it is what restarts a machine"))?;
    match out.status.success() {
        true => Ok(()),
        false => Err(format!(
            "systemctl reboot: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// The installer takes the console here, and every widget after it draws full
/// screen under one title bar. What is above it is a login banner and a
/// discovery line, and neither is worth the room.
pub fn own_screen(payload: &Payload, prompt: &Prompt) {
    if !prompt.draws() {
        return;
    }
    // Not a terminal's own `clear`: none is open yet, and this has to work as
    // the first thing the process writes.
    print!("\u{1b}[2J\u{1b}[H");
    let _ = std::io::stdout().flush();
    crate::ui::own_screen(copy::installing(&payload.image));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tect-install-{name}.{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch tree");
        root
    }

    /// What `emit::recipe::build` emits for a debian target, which is what a
    /// payload root carries.
    const EMITTED: &str = r#"{
  "image": "ghcr.io/tectonic-os/deb2:latest",
  "targetImgref": "ghcr.io/tectonic-os/deb2:latest",
  "composeFsBackend": true,
  "genericImage": true,
  "bootloader": "grub2",
  "filesystem": "ext4",
  "hostname": "deb2",
  "user": { "groups": ["sudo"] },
  "additionalImageStores": ["/var/lib/tectonic/store"]
}"#;

    /// The three cases, and the precedence between the two that can both hold.
    #[test]
    fn a_payload_wins_over_the_repository_that_would_have_to_be_built() {
        let root = scratch("cases");
        assert!(matches!(classify(&root), Ok(Found::Nothing(_))));

        std::fs::write(root.join(crate::layout::REPO_FILE), "repo {\n}\n").expect("a repo.kdl");
        assert!(matches!(classify(&root), Ok(Found::Repo(_))));

        // Both present, and the artifact wins: it is already built, and a
        // rebuild is the slowest way to be less certain of the same bytes.
        std::fs::write(root.join(RECIPE), EMITTED).expect("a recipe");
        let Ok(Found::Image(payload)) = classify(&root) else {
            panic!("a payload beside a repository is still a payload");
        };
        assert_eq!(payload.image, "ghcr.io/tectonic-os/deb2:latest");
        assert_eq!(payload.hostname, "deb2");

        // A payload that cannot be read is named, not skipped past: a stick
        // carrying a broken recipe is not a stick carrying nothing.
        std::fs::write(root.join(RECIPE), "{").expect("a broken recipe");
        assert!(classify(&root).is_err());
        std::fs::write(root.join(RECIPE), "{}").expect("an empty recipe");
        assert!(classify(&root)
            .unwrap_err()
            .contains(&format!("{RECIPE}: no `image`")));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The person's half goes in, the derived half stays exactly as emitted,
    /// and the password is hashed before it is written anywhere.
    #[test]
    fn the_completed_recipe_keeps_every_derived_field_and_hashes_the_password() {
        let root = scratch("complete");
        let recipe = root.join(RECIPE);
        std::fs::write(&recipe, EMITTED).expect("a recipe");
        let answers = Answers {
            disk: "/dev/vda".to_string(),
            hostname: "deb2".to_string(),
            user: "tect".to_string(),
            password: "hunter2".to_string(),
            encryption: Encryption {
                kind: "luks-passphrase".to_string(),
                passphrase: "opensesame".to_string(),
            },
        };
        let done = complete(&recipe, &answers).expect("the person's half goes in");

        // Nothing derived moved.
        for (key, value) in [
            ("image", "\"ghcr.io/tectonic-os/deb2:latest\""),
            ("composeFsBackend", "true"),
            ("bootloader", "\"grub2\""),
            ("filesystem", "\"ext4\""),
            ("hostname", "\"deb2\""),
            ("disk", "\"/dev/vda\""),
        ] {
            let held = json::field(&done, key).map(|v| v.render().trim().to_string());
            assert_eq!(held.as_deref(), Some(value), "{key}");
        }

        // The account is merged into the groups the family derived, not put
        // over them: `useradd` refuses the whole call over a group the target
        // has not got, and `sudo` is the one this target has.
        let user = json::field(&done, "user").expect("an account");
        assert_eq!(json::strings(user, "groups"), ["sudo"]);
        assert_eq!(json::text(user, "username").as_deref(), Some("tect"));
        let hash = json::text(user, "password").expect("a password");
        assert!(hash.starts_with("$6$"), "{hash}");
        assert!(!hash.contains("hunter2"), "{hash}");

        // Fisherman refuses the recipe without the passphrase these two forms
        // name, so the pair goes in together or not at all.
        let encryption = json::field(&done, "encryption").expect("an encryption");
        assert_eq!(
            json::text(encryption, "type").as_deref(),
            Some("luks-passphrase")
        );
        assert_eq!(
            json::text(encryption, "passphrase").as_deref(),
            Some("opensesame")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The two of the four that take one, and the two that do not.
    #[test]
    fn only_the_forms_named_for_a_passphrase_are_asked_for_one() {
        let wants: Vec<&str> = KINDS
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| Encryption::wants_passphrase(name))
            .collect();
        assert_eq!(wants, ["luks-passphrase", "tpm2-luks-passphrase"]);
    }

    /// A machine with no TPM still sees the two forms that need one, dim and
    /// saying why. A shorter list explains nothing.
    #[test]
    fn the_tpm_forms_are_shown_and_unpickable_where_there_is_no_tpm() {
        let shown = kinds(false);
        let without: Vec<(&str, bool)> = shown
            .iter()
            .map(|choice| (choice.detail.as_str(), choice.available))
            .collect();
        assert_eq!(
            without,
            vec![
                (copy::ENC_NONE, true),
                (copy::NO_TPM, false),
                (copy::ENC_PASSPHRASE, true),
                (copy::NO_TPM, false),
            ]
        );
        assert!(kinds(true).iter().all(|choice| choice.available));
    }

    /// A kind fisherman does not take is refused before anything is asked,
    /// naming the four that it does.
    #[test]
    fn an_encryption_no_backend_takes_is_refused_by_name() {
        let refused = ask_encryption(
            Some("luks".to_string()),
            None,
            &Encryption::none(),
            &Prompt::silent(),
        )
        // `.err()`, because `unwrap_err` would want a `Debug` on a struct
        // holding a passphrase.
        .err()
        .expect("a refusal");
        assert!(refused.contains("tpm2-luks-passphrase"), "{refused}");
        let kept = ask_encryption(
            Some("tpm2-luks".to_string()),
            None,
            &Encryption::none(),
            &Prompt::silent(),
        )
        .expect("one of the four");
        assert_eq!(kept.kind, "tpm2-luks");
        assert!(kept.passphrase.is_empty());
    }

    /// The whole disks, what tells them apart, and nothing virtual.
    #[test]
    fn only_the_disks_a_person_could_install_onto_are_offered() {
        let sys = scratch("sys");
        let block = |name: &str, sectors: &str, model: Option<&str>, removable: &str, ro: &str| {
            let at = sys.join(name);
            std::fs::create_dir_all(at.join("device")).expect("a block device");
            std::fs::write(at.join("size"), sectors).expect("a size");
            std::fs::write(at.join("removable"), removable).expect("a removable");
            std::fs::write(at.join("ro"), ro).expect("a read-only flag");
            if let Some(model) = model {
                std::fs::write(at.join("device/model"), model).expect("a model");
            }
        };
        block(
            "sda",
            "937703088\n",
            Some("Samsung SSD 980\n"),
            "0\n",
            "0\n",
        );
        block("sdb", "60088320\n", None, "1\n", "0\n");
        block("loop0", "204800\n", None, "0\n", "0\n");
        // An empty card reader is a row that erases nothing.
        block("sdc", "0\n", None, "1\n", "0\n");
        // A medium attached read-only, which is what a virtual machine gives
        // an iso.
        block(
            "sdd",
            "6291456\n",
            Some("QEMU USB HARDDRIVE\n"),
            "1\n",
            "1\n",
        );
        // **And the case that matters: a stick written with `dd`.** It is
        // writable, so nothing about the device says not to install onto it —
        // only that the live root is mounted from one of its partitions.
        block("sde", "60088320\n", Some("Cruzer Blade\n"), "1\n", "0\n");
        let part = sys.join("sde/sde1");
        std::fs::create_dir_all(&part).expect("a partition");
        std::fs::write(part.join("partition"), "1\n").expect("a partition number");
        let mounts = "\
/dev/sde1 /run/initramfs/live iso9660 ro,relatime 0 0
tmpfs /run tmpfs rw,nosuid,nodev 0 0
";

        assert_eq!(
            disks(&sys, mounts),
            vec![
                (
                    "/dev/sda".to_string(),
                    "480 GB  Samsung SSD 980".to_string()
                ),
                (
                    "/dev/sdb".to_string(),
                    format!("30 GB  {}", copy::REMOVABLE)
                ),
            ]
        );
        // With nothing mounted from it, the same stick is a disk like any
        // other — which is what stops this rule hiding somebody's spare drive.
        assert!(disks(&sys, "").iter().any(|(disk, _)| disk == "/dev/sde"));
        let _ = std::fs::remove_dir_all(&sys);
    }

    /// The scan refuses, because picking wrong erases a disk from the wrong
    /// image.
    #[test]
    fn more_than_one_labelled_partition_is_named_rather_than_chosen() {
        assert!(labelled("\n").is_empty());
        assert_eq!(labelled("/dev/sdb2\n"), ["/dev/sdb2"]);
        assert_eq!(
            labelled("/dev/sdb2\n/dev/sdc1\n"),
            ["/dev/sdb2", "/dev/sdc1"]
        );
    }

    /// The progress lines, and the one thing fisherman says that cannot be
    /// asked for again.
    #[test]
    fn every_event_reads_as_a_line_and_anything_else_passes_through() {
        let said = |line: &str| Event::of(line).say();
        assert_eq!(
            said(
                r#"{"type":"step","step":7,"total_steps":12,"step_name":"install OS","cumulative_pct":9,"weight_pct":87,"elapsed_ms":4210}"#
            ),
            "[  9%] 7/12 install OS"
        );
        assert_eq!(
            said(r#"{"type":"info","message":"Live environment detected"}"#),
            "       Live environment detected"
        );
        assert_eq!(
            said(r#"{"type":"complete","message":"Installation complete"}"#),
            "[100%] Installation complete"
        );
        let key = said(r#"{"type":"recovery_key","key":"abcd-efgh"}"#);
        assert!(
            key.contains("abcd-efgh") && key.contains("write this down"),
            "{key}"
        );
        // Not an event, and the scrollback is where a failed install is read.
        assert_eq!(said("bootc: pulling layer 3/9"), "bootc: pulling layer 3/9");
    }

    /// The whole of the rule that lets the screen be a gauge: everything
    /// fisherman says is in the log except the one line that would put a
    /// recovery key on removable media.
    #[test]
    fn the_log_holds_every_event_but_the_key() {
        let logged = |line: &str| Event::of(line).logged();
        assert!(logged(r#"{"type":"recovery_key","key":"abcd-efgh"}"#).is_none());
        assert!(logged(r#"{"type":"info","message":"Live environment"}"#).is_some());
        assert!(logged("bootc: pulling layer 3/9").is_some());
    }

    /// What the last question is asked over, and the one value on it that
    /// cannot read back as itself.
    #[test]
    fn the_confirm_screen_shows_what_is_about_to_be_erased() {
        let answers = Answers {
            disk: "/dev/vda".to_string(),
            hostname: "deb2".to_string(),
            user: "tect".to_string(),
            password: "hunter2".to_string(),
            encryption: Encryption::none(),
        };
        assert_eq!(
            answers.summary(&Payload {
                recipe: "/mnt/tect/install-recipe.json".into(),
                image: "ghcr.io/tectonic-os/deb2:latest".to_string(),
                hostname: "deb2".to_string(),
                filesystem: "ext4".to_string(),
                bootloader: "grub2".to_string(),
            }),
            vec![
                (copy::ROW_DISK.to_string(), "/dev/vda".to_string()),
                (copy::ROW_HOSTNAME.to_string(), "deb2".to_string()),
                (copy::ROW_ACCOUNT.to_string(), "tect".to_string()),
                (
                    copy::ROW_PASSWORD.to_string(),
                    copy::PASSWORD_SET.to_string()
                ),
                (copy::ROW_ENCRYPTION.to_string(), NONE.to_string()),
                // The layout nobody chose, which the last screen is the only
                // place that spells out.
                ("esp".to_string(), "2 GB  fat32".to_string()),
                ("/boot".to_string(), "2 GB  ext4".to_string()),
                ("root".to_string(), "the rest  ext4".to_string()),
            ]
        );
        let question = copy::erasing(&answers.disk);
        assert!(
            question.contains("/dev/vda") && question.contains("erased"),
            "{question}"
        );
        assert!(!question.contains("hunter2"));
    }

    /// A passphrase is a question only for the two forms named for one. On the
    /// others it is not a field somebody can answer wrongly, so it is not a
    /// field at all.
    #[test]
    fn the_passphrase_is_a_row_only_where_it_is_owed() {
        use crate::ui::{Choice, Field};
        let form = |kind: &str| {
            vec![
                Field::text(copy::ROW_DISK, "/dev/vda"),
                Field::fixed(copy::ROW_LAYOUT, "esp + ext4 /boot + ext4 root"),
                Field::text(copy::ROW_HOSTNAME, "deb2"),
                Field::text(copy::ROW_ACCOUNT, "tect"),
                Field::secret(copy::ROW_PASSWORD, "hunter2"),
                Field::secret(copy::ROW_CONFIRM, "hunter2"),
                Field::pick(copy::ROW_ENCRYPTION, vec![Choice::new(kind, "")], Some(0)),
                Field::secret(copy::ROW_PASSPHRASE, ""),
            ]
        };
        assert!(!asked(&form(NONE)).contains(&ROW_PASSPHRASE));
        assert!(!asked(&form("tpm2-luks")).contains(&ROW_PASSPHRASE));
        assert!(asked(&form("luks-passphrase")).contains(&ROW_PASSPHRASE));
        assert!(asked(&form("tpm2-luks-passphrase")).contains(&ROW_PASSPHRASE));
        // Every other row is there whatever the encryption is, questions and
        // the layout alike.
        assert_eq!(asked(&form(NONE)).len(), 7);
    }

    /// The four things nothing derives, and the one thing a form can check
    /// that a sequence of questions had to ask twice for.
    #[test]
    fn the_action_says_what_the_form_is_short_of() {
        use crate::ui::{Choice, Field};
        let form = |password: &str, confirm: &str, kind: &str, passphrase: &str| {
            vec![
                Field::text(copy::ROW_DISK, "/dev/vda"),
                Field::fixed(copy::ROW_LAYOUT, "esp + ext4 /boot + ext4 root"),
                Field::text(copy::ROW_HOSTNAME, "deb2"),
                Field::text(copy::ROW_ACCOUNT, "tect"),
                Field::secret(copy::ROW_PASSWORD, password),
                Field::secret(copy::ROW_CONFIRM, confirm),
                Field::pick(copy::ROW_ENCRYPTION, vec![Choice::new(kind, "")], Some(0)),
                Field::secret(copy::ROW_PASSPHRASE, passphrase),
            ]
        };
        assert!(short_of(&form("hunter2", "hunter2", NONE, "")).is_none());
        // Both halves are on screen at once, so they are compared there.
        let differ = short_of(&form("hunter2", "hunter3", NONE, "")).unwrap();
        assert_eq!(differ, copy::NO_MATCH_ROW);
        // A passphrase is owed only by the forms named for one.
        assert!(short_of(&form("hunter2", "hunter2", "luks-passphrase", "")).is_some());
        assert!(short_of(&form("hunter2", "hunter2", "luks-passphrase", "x")).is_none());
        // And an empty account is named beside an empty password.
        let mut bare = form("", "", NONE, "");
        bare[ROW_ACCOUNT] = Field::text(copy::ROW_ACCOUNT, "");
        let short = short_of(&bare).unwrap();
        assert!(
            short.contains(copy::ROW_ACCOUNT) && short.contains(copy::ROW_PASSWORD),
            "{short}"
        );
    }

    /// Neither of the two cases that cannot install says nothing; each names
    /// what would make this root installable.
    #[test]
    fn a_root_with_nothing_built_on_it_refuses_by_name() {
        let nothing = Found::Nothing("/mnt/tect".into()).payload().unwrap_err();
        assert!(nothing.contains(RECIPE) && nothing.contains(crate::layout::REPO_FILE));
        assert!(nothing.contains("/mnt/tect"), "{nothing}");
        let repo = Found::Repo("/mnt/tect".into()).payload().unwrap_err();
        assert!(repo.contains("tect build"), "{repo}");
    }
}

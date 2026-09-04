//! What a root offers an installer, and the install that runs from it.
//!
//! `emit::recipe` writes the half of a recipe the declaration answers. This is
//! the other end: find a payload, read that document back, add the half that is
//! the person's — the disk and the account — and hand the result to fisherman,
//! which owns partitioning, LUKS, TPM2 enrolment and `bootc install`.
//!
//! **The discovery rule is one sentence: a payload root carries
//! `install-recipe.json`.** That document names the image and the store beside
//! it, so nothing here opens a container image or re-derives a boot chain —
//! which it could not do anyway, since the baked manifest carries no bootupd,
//! composefs or bootloader field and `emit::recipe` derives all three from the
//! base family in a resolved plan. A repository is the other case, and it is
//! the source rather than the artifact.
//!
//! **A payload wins over a repository, which is the opposite of
//! `command::Context::of`'s precedence, and the inversion is deliberate.** For
//! authoring, a repository beats a baked document because it is the source. For
//! installing, the payload wins: it is already built, and rebuilding it here to
//! reach the same bytes is the slowest possible way to be less certain. Do not
//! "fix" one to match the other.
//!
//! Where the roots come from is a separate question with its own rule — the
//! filesystem label `TECT`, one mount, refuse rather than pick when there is
//! more than one. `--from` names a root outright, and the media's own payload
//! is the default a `TECT` partition overrides.

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

/// A malformed recipe is a refusal naming the file rather than a fall-through
/// to the next case: a stick that carries a payload and cannot install it is
/// not a stick that carries nothing.
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
        return Ok(Found::Image(Payload {
            image: field("image")?,
            hostname: field("hostname")?,
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
/// overrides the media's own payload — someone who attached a labelled drive
/// did it deliberately — and more than one is refused naming them rather than
/// picked between, since picking wrong erases a disk from the wrong image.
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
             clear: {}\n\nhelp: `tect install --from <root>` names one outright",
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
    // A second run finds its own mount rather than failing over it.
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
///
/// `$TECT_TPM` names it instead where it is set, which is how the drawn golden
/// stops depending on whether the machine running it has one — the same reason
/// that flow is given a `--disk` rather than reading `/sys/block`.
const TPM: &str = "/dev/tpmrm0";

fn tpm() -> PathBuf {
    std::env::var_os("TECT_TPM")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(TPM))
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
#[derive(Default)]
pub struct Given {
    pub disk: Option<String>,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub encryption: Option<String>,
    pub passphrase: Option<String>,
}

impl Answers {
    /// The screens, in order, then the review over them. `None` is a leaving:
    /// nothing has been written and no disk has been touched.
    pub fn collect(
        payload: &Payload,
        given: Given,
        prompt: &Prompt,
    ) -> Result<Option<Self>, String> {
        let mut answers = Self {
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
        };
        while prompt.draws() {
            let rows = answers.rows();
            match crate::ui::review(
                &copy::erasing(&answers.disk),
                &rows,
                copy::INSTALL,
                copy::INSTALL_KEYS,
            )? {
                None => return Ok(None),
                Some(at) if at == rows.len() => break,
                Some(at) => answers.ask(at, prompt)?,
            }
        }
        Ok(Some(answers))
    }

    /// The password is a row so it can be asked again; it is the one value
    /// that cannot read back as itself.
    fn rows(&self) -> Vec<(String, String)> {
        vec![
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
        ]
    }

    /// One row asked again, in `rows`'s order. Nothing here gates anything
    /// else — a disk does not change what an account is — so a field is asked
    /// alone rather than by re-entering the whole procedure at it.
    fn ask(&mut self, row: usize, prompt: &Prompt) -> Result<(), String> {
        match row {
            0 => self.disk = ask_disk(None, Some(&self.disk), prompt)?,
            1 => {
                self.hostname =
                    prompt.text(None, copy::INSTALL_NAME, "--hostname", Some(&self.hostname))?
            }
            2 => self.user = prompt.text(None, copy::INSTALL_USER, "--user", Some(&self.user))?,
            3 => self.password = prompt.secret(None, copy::INSTALL_PASSWORD, "--password")?,
            _ => self.encryption = ask_encryption(None, None, &self.encryption, prompt)?,
        }
        Ok(())
    }
}

/// Half a kilobyte, which is what `/sys/block/<disk>/size` counts whatever the
/// device's own sector size is.
const SECTOR: u64 = 512;

/// Not disks anyone installs onto, and each one is only an option to get wrong.
const VIRTUAL: [&str; 7] = ["loop", "ram", "zram", "sr", "fd", "dm-", "md"];

/// The whole disks this machine has, as `/sys/block` holds them, with what a
/// person needs to tell one from another beside each.
pub fn disks(sys: &Path) -> Vec<(String, String)> {
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
/// into rather than guessed at.
fn ask_disk(
    given: Option<String>,
    current: Option<&str>,
    prompt: &Prompt,
) -> Result<String, String> {
    if let Some(disk) = given.filter(|disk| !disk.is_empty()) {
        return Ok(disk);
    }
    let found = disks(Path::new("/sys/block"));
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
        None => Err(format!(
            "give --disk, since nothing was chosen: {}",
            copy::INSTALL_DISK.trim_end_matches(':')
        )),
    }
}

/// A `tpm2-` form on a machine with no TPM is shown and not pickable rather
/// than left out: what it needs is the reason it is worth showing.
fn kinds(tpm: bool) -> Vec<Choice> {
    KINDS
        .iter()
        .map(|(name, detail)| match tpm || !name.starts_with("tpm2") {
            true => Choice::new(*name, *detail),
            false => Choice::new(*name, copy::NO_TPM).unavailable(),
        })
        .collect()
}

/// A `tpm2-` form on a machine with no TPM is shown and not pickable, the way
/// every unmet option in this tool is: what it needs is the reason it is worth
/// showing. The passphrase is asked every time the kind is, so editing the row
/// can change it.
fn ask_encryption(
    given: Option<String>,
    passphrase: Option<String>,
    current: &Encryption,
    prompt: &Prompt,
) -> Result<Encryption, String> {
    let names = || {
        KINDS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let kind = match given {
        Some(kind) if !KINDS.iter().any(|(name, _)| *name == kind) => {
            return Err(format!("`{kind}` is not one of {}", names()))
        }
        Some(kind) => kind,
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
        passphrase: match Encryption::wants_passphrase(&kind) {
            false => String::new(),
            true => prompt.secret(passphrase, copy::LUKS_PASSPHRASE, "--passphrase")?,
        },
        kind,
    })
}

/// Replaces the value under `key`, or appends it. Anything that is not an
/// object is left alone, so a recipe whose `user` is a string fails in
/// fisherman's own reader rather than here.
fn set(value: &mut Json, key: &str, field: Json) {
    let Json::Object(fields) = value else { return };
    match fields.iter_mut().find(|(name, _)| name == key) {
        Some((_, held)) => *held = field,
        None => fields.push((key.to_string(), field)),
    }
}

/// The recipe with the person's half in it. `user` is merged rather than
/// replaced: the groups already in it are the *target's* admin group, which
/// `emit::recipe` derives from the base family and `useradd` refuses the whole
/// call over when it names a group the target has not got.
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

/// A `$`-prefixed crypt string, because fisherman hands the field to
/// `chpasswd` and only a `$` takes the `-e` branch. A plaintext one goes
/// through PAM instead, which reads the *target's* `pam.d` with the live
/// environment's modules and dies `pam_chauthtok() failed, error: Module is
/// unknown` — **after the OS is already on the disk**, losing a completed
/// install to its last step. Measured `NEXT-40` stage 3, and it is not
/// something a person can be asked to remember.
///
/// `openssl passwd` rather than a crypt(3) of our own: the C one is in
/// libcrypt rather than libc on glibc, which is a link-time dependency this
/// binary does not have, and SHA-512 by hand is a hundred and fifty lines of
/// cryptography written to avoid one process. `-stdin` keeps it out of `ps`.
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

impl Found {
    /// The payload, or why this root has none. Asked *before* a person is
    /// asked for a disk to erase, because a precondition that can be checked
    /// early and is checked late is a bug in an installer.
    pub fn payload(&self) -> Result<&Payload, String> {
        match self {
            Self::Image(payload) => Ok(payload),
            // Building here is the middle row of the plan's three, and it is
            // not this: the scratch has to go on the target disk, which means
            // partitioning before the build and an erased disk when the build
            // fails. That ordering is a screen's decision, not a flag's.
            Self::Repo(root) => Err(format!(
                "{} is a repository and not a built image, so there is nothing here to install \
                 yet\n\nhelp: `tect build` in it, then `tect install --from <the built payload>`",
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

/// One line of fisherman's event stream as a line of ours. Anything that is
/// not one of its events passes through unchanged: what it writes is the
/// transcript a failure on someone else's machine is diagnosed from, and this
/// draws no screen that could take it away.
fn say(line: &str) -> String {
    let Ok(event) = Json::parse(line) else {
        return line.to_string();
    };
    let text = |key: &str| json::text(&event, key).unwrap_or_default();
    let count = |key: &str| json::number(&event, key).unwrap_or(0);
    match json::text(&event, "type").as_deref() {
        Some("step") => format!(
            "[{:>3}%] {}/{} {}",
            count("cumulative_pct"),
            count("step"),
            count("total_steps"),
            text("step_name")
        ),
        Some("info" | "substep") => format!("       {}", text("message")),
        Some("complete") => format!("[100%] {}", text("message")),
        // The only copy of it there will ever be, and the disk does not open
        // without it if the TPM stops answering.
        Some("recovery_key") => format!(
            "\nwrite this down, it is the recovery key: {}\n",
            text("key")
        ),
        _ => line.to_string(),
    }
}

/// The renderer a deb image ships and a fedora one does not: their signed GRUB
/// reads no BLS entries, so the menu is rendered from the entries `bootc` just
/// wrote. Run *from the image* — a composefs deployment on the disk is sealed
/// erofs with no walkable `/usr` to read it out of.
const RENDERER: &str = "/usr/libexec/grub-menu-from-bls";

/// Where the target's boot filesystem is mounted while the menu is written.
/// The renderer takes a root and looks under `<root>/boot`, so the filesystem
/// carrying the entries is mounted *at* `boot` beneath this, which is the same
/// shape whether the target keeps /boot on its own partition or on the root.
const TARGET: &str = "/run/tect-target";

/// The menu the installed machine boots from, written after fisherman has
/// finished and unmounted: `bootc` installs the bootloader before it writes
/// the entries, so nothing during the install itself can render them.
///
/// Silence is the failure this exists to avoid. An image with no renderer is a
/// family that needs none and is skipped; anything else is an error, because a
/// disk that installs and then reaches an empty GRUB menu looks like a broken
/// image rather than a missing file.
fn render_menu(image: &str, disk: &str) -> Result<(), String> {
    let at = PathBuf::from(TARGET);
    let boot = at.join("boot");
    std::fs::create_dir_all(&boot).map_err(|err| format!("{TARGET}: {err}"))?;

    let Some(device) = boot_partition(disk, &boot)? else {
        return Err(format!(
            "no partition of {disk} carries `loader/entries`, so there is no menu to render"
        ));
    };
    let rendered = run_renderer(image);
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

/// The first partition of `disk` whose filesystem holds `loader/entries`,
/// left mounted at `boot`. Found by content and not by label, so nothing here
/// depends on how the backend names its partitions.
fn boot_partition(disk: &str, boot: &Path) -> Result<Option<String>, String> {
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
            return Ok(Some(device));
        }
        let _ = Command::new("umount").arg(boot).output();
    }
    Ok(None)
}

/// `false` where the image ships no renderer, which is how a fedora target is
/// skipped without this knowing which families have `blscfg`.
fn run_renderer(image: &str) -> Result<bool, String> {
    let out = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--net=none",
            "--security-opt",
            "label=disable",
            "-v",
            &format!("{TARGET}:/target"),
            image,
            "/bin/sh",
            "-c",
            &format!("test -x {RENDERER} || exit 3; exec {RENDERER} /target"),
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

/// Completes the recipe and runs fisherman over it, rendering its event stream
/// as it goes. Not an `exec`: the events are only worth reading if something
/// reads them, and the staged recipe is only removable if something outlives
/// the install.
pub fn run(payload: &Payload, answers: &Answers) -> Result<(), String> {
    let path = stage(&complete(&payload.recipe, answers)?)?;
    eprintln!(
        "tect: installing {} as {} onto {}",
        payload.image, answers.hostname, answers.disk
    );
    let mut child = Command::new(BACKEND)
        .arg(&path)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{BACKEND}: {err}"))?;
    if let Some(events) = child.stdout.take() {
        for line in std::io::BufReader::new(events)
            .lines()
            .map_while(Result::ok)
        {
            println!("{}", say(&line));
        }
    }
    let status = child.wait().map_err(|err| format!("{BACKEND}: {err}"))?;
    // It carries the password hash and the passphrase, and the install is over.
    let _ = std::fs::remove_file(&path);
    if !status.success() {
        return Err(format!("{BACKEND} did not finish: {status}"));
    }
    render_menu(&payload.image, &answers.disk)
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
    /// saying why, rather than a shorter list that explains nothing.
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
        // `.err()` rather than `unwrap_err`, which would want a `Debug` on a
        // struct holding a passphrase.
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
        let block = |name: &str, sectors: &str, model: Option<&str>, removable: &str| {
            let at = sys.join(name);
            std::fs::create_dir_all(at.join("device")).expect("a block device");
            std::fs::write(at.join("size"), sectors).expect("a size");
            std::fs::write(at.join("removable"), removable).expect("a removable");
            if let Some(model) = model {
                std::fs::write(at.join("device/model"), model).expect("a model");
            }
        };
        block("sda", "937703088\n", Some("Samsung SSD 980\n"), "0\n");
        block("sdb", "60088320\n", None, "1\n");
        block("loop0", "204800\n", None, "0\n");
        // An empty card reader is a row that erases nothing.
        block("sdc", "0\n", None, "1\n");

        assert_eq!(
            disks(&sys),
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
        let _ = std::fs::remove_dir_all(&sys);
    }

    /// The scan refuses rather than picks, because picking wrong erases a disk
    /// from the wrong image.
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
        let said = |line: &str| say(line);
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

    /// Every row is asked again by the index the review answers with, and the
    /// password is the one that cannot read back as itself.
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
            answers.rows(),
            vec![
                (copy::ROW_DISK.to_string(), "/dev/vda".to_string()),
                (copy::ROW_HOSTNAME.to_string(), "deb2".to_string()),
                (copy::ROW_ACCOUNT.to_string(), "tect".to_string()),
                (
                    copy::ROW_PASSWORD.to_string(),
                    copy::PASSWORD_SET.to_string()
                ),
                (copy::ROW_ENCRYPTION.to_string(), NONE.to_string()),
            ]
        );
        let question = copy::erasing(&answers.disk);
        assert!(
            question.contains("/dev/vda") && question.contains("erased"),
            "{question}"
        );
        assert!(!question.contains("hunter2"));
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

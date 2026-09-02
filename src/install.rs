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
//! more than one — and it is not here: `--from` names a root outright, and the
//! media's own payload is the default one.

use crate::emit::json::{self, Json};
use std::io::Write as _;
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

/// The person's half, which nothing derives and no flag defaults.
pub struct Answers {
    pub disk: String,
    pub user: String,
    pub password: String,
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

/// Completes the recipe and hands it to fisherman, which replaces this
/// process. It only returns having failed.
pub fn run(payload: &Payload, answers: &Answers) -> Result<(), String> {
    use std::os::unix::process::CommandExt as _;
    let path = stage(&complete(&payload.recipe, answers)?)?;
    eprintln!(
        "tect: installing {} as {} onto {}",
        payload.image, payload.hostname, answers.disk
    );
    Err(format!(
        "{BACKEND}: {}",
        Command::new(BACKEND).arg(&path).exec()
    ))
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
  "bootloader": "systemd",
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
            user: "tect".to_string(),
            password: "hunter2".to_string(),
        };
        let done = complete(&recipe, &answers).expect("the person's half goes in");

        // Nothing derived moved.
        for (key, value) in [
            ("image", "\"ghcr.io/tectonic-os/deb2:latest\""),
            ("composeFsBackend", "true"),
            ("bootloader", "\"systemd\""),
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
        let _ = std::fs::remove_dir_all(&root);
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

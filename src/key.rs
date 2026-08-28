//! `create key <kind>`. Which key exists, where each half of it goes and what
//! generates it come out of the module declaring it; the generators, and the
//! text that follows one, are the tool's.

use crate::copy;
use crate::layout;
use crate::model::module::Key as Declared;
use crate::model::remote::REMOTE_DIR;
use crate::parse::disk::Disk;
use crate::prompt::Prompt;
use crate::ui::Choice;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One key, as the module declaring it describes it and as this repository
/// puts it on disk.
pub struct Key {
    declared: Declared,
    public: PathBuf,
    private: PathBuf,
    /// The certificate's common name, for the generator that writes one.
    cn: Option<String>,
}

impl Key {
    pub fn collect(
        root: &Path,
        kind: Option<String>,
        module: Option<String>,
        cn: Option<String>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
        let mut disk = Disk::scan(root);
        let kind = match kind {
            Some(kind) => kind,
            None => which(&disk, prompt)?,
        };

        let mut declaring = disk.keys.remove(&kind).unwrap_or_default();
        if declaring.is_empty() {
            return Err(absent(root, &kind, &disk));
        }
        let at = provider(&declaring, &kind, module, prompt)?;
        let (_, declared) = declaring.swap_remove(at);

        let public = layout::public_key(root, &declared.public);
        let private = layout::private_key(root, &declared.private);
        unwritten(&public)?;
        unwritten(&private)?;

        // The openssl generator is set up by its profile, and there is one.
        let cn = match (declared.generator.as_str(), declared.profile.as_deref()) {
            ("cosign", _) if cn.is_some() => {
                return Err(
                    "`--cn` is a certificate's common name, and `cosign` writes no certificate"
                        .into(),
                )
            }
            ("cosign", _) => None,
            ("openssl", Some("module-signing")) => Some(common_name(root, cn, prompt)?),
            (generator, _) => {
                return Err(format!(
                    "`{generator}` generates a key for a profile, and `key \"{kind}\"` names none; \
                     `generator \"{generator}\" profile=\"module-signing\"` is the one it has"
                ))
            }
        };

        Ok(Self {
            declared,
            public,
            private,
            cn,
        })
    }

    pub fn apply(&self, root: &Path) -> Result<(), String> {
        let work = workspace(&self.declared.kind)?;
        let (public, private) = match self.declared.generator.as_str() {
            "cosign" => cosign(&work),
            _ => openssl(
                &work,
                &self.declared,
                self.cn.as_deref().unwrap_or_default(),
            ),
        }?;

        install(&public, &self.public, 0o644)?;
        println!("wrote {}", shown(root, &self.public));
        install(&private, &self.private, 0o600)?;
        println!("wrote {}", shown(root, &self.private));
        let _ = std::fs::remove_dir_all(&work);

        print!("{}", self.next(root));
        warn_unignored(root);
        Ok(())
    }

    /// What the key is not usable without, which is prose and therefore the
    /// tool's rather than the manifest's. One follow-up per generator.
    fn next(&self, root: &Path) -> String {
        let public = shown(root, &self.public);
        let private = shown(root, &self.private);
        match self.declared.generator.as_str() {
            "cosign" => format!(
                "\nthe key carries no password, which is what the build workflow decrypts it with.\n\n\
                 next:\n\
                 \x20 commit {public}\n\
                 \x20 gh secret set SIGNING_SECRET < {private}\n"
            ),
            _ => format!(
                "\nnext:\n\
                 \x20 commit {public}\n\
                 \x20 gh secret set MOK_PRIVKEY < {private}\n\
                 \x20 MOK_KEY_PATH={private} is what a local build reads it from\n\
                 \n\
                 every machine that boots this image enrols the certificate once, and\n\
                 until it does the modules signed with it will not load:\n\
                 \x20 sudo mokutil --import {public}\n\
                 \x20 it asks for a one-time password; reboot, choose Enroll MOK, and\n\
                 \x20 give the same password\n"
            ),
        }
    }
}

/// Which key, where the command named none.
fn which(disk: &Disk, prompt: &Prompt) -> Result<String, String> {
    let kinds: Vec<&String> = disk.keys.keys().collect();
    if kinds.is_empty() {
        return Err(undeclared("", disk));
    }
    let options: Vec<Choice> = kinds.iter().map(|kind| Choice::new(*kind, "")).collect();
    prompt
        .choose(copy::WHICH_KEY, &options)?
        .map(|at| kinds[at].clone())
        .ok_or_else(|| {
            format!(
                "`tect create key <kind>` names which key; this repository declares {}",
                listed(disk)
            )
        })
}

/// Which module declares the key, where more than one declares the kind.
fn provider(
    declaring: &[(String, Declared)],
    kind: &str,
    given: Option<String>,
    prompt: &Prompt,
) -> Result<usize, String> {
    let dirs: Vec<&str> = declaring
        .iter()
        .map(|(dir, _)| dir.strip_prefix(&format!("{REMOTE_DIR}/")).unwrap_or(dir))
        .collect();
    if let Some(given) = given {
        return dirs
            .iter()
            .position(|dir| *dir == given)
            .ok_or_else(|| format!("`modules/{given}` does not declare `key \"{kind}\"`"));
    }
    match dirs.as_slice() {
        [_] => Ok(0),
        many => {
            let listed = many.join(", ");
            let options: Vec<Choice> = many.iter().map(|dir| Choice::new(*dir, "")).collect();
            prompt
                .choose(&copy::key_provider(kind), &options)?
                .ok_or_else(|| {
                    format!("the {kind} key is declared by {listed}; name one with `--module`")
                })
        }
    }
}

/// Nothing here declares the kind: which module in the declared collections
/// does, and the line that fetches it.
fn absent(root: &Path, kind: &str, disk: &Disk) -> String {
    let (list, ..) = crate::declarations(root);
    let index = crate::provider::Index::scan(root, &list.sources, disk, true);
    let found: Vec<String> = index
        .declaring_key(kind)
        .iter()
        .map(|module| module.qualified())
        .collect();
    match found.first() {
        Some(first) => format!(
            "The {} module must be imported before creating {kind} keys.\n\
             You can add it with `tect import module {first}`",
            found.join(" or ")
        ),
        None => undeclared(kind, disk),
    }
}

/// Nothing in the repository and nothing in its collections declares it.
fn undeclared(kind: &str, disk: &Disk) -> String {
    let noun = match kind.is_empty() {
        true => "a signing key".to_string(),
        false => format!("a {kind} key"),
    };
    match disk.keys.is_empty() {
        true => format!(
            "no module in this repository declares {noun}.\n\n\
             Keys come from modules: image signing from a module carrying the cosign public \
             key, Secure Boot from a kernel module carrying a MOK certificate. Import one and \
             run this again."
        ),
        false => format!(
            "no module in this repository declares {noun}.\n\n\
             The keys declared here are {}. Import a module carrying one and run this again.",
            listed(disk)
        ),
    }
}

fn listed(disk: &Disk) -> String {
    disk.keys
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The common name the enrolment prompt shows, defaulting to the repository's.
fn common_name(root: &Path, given: Option<String>, prompt: &Prompt) -> Result<String, String> {
    let named = crate::create::named_after_root(root).unwrap_or_else(|| "tectonic".to_string());
    let cn = prompt.text(
        given,
        copy::KEY_CN,
        "`--cn`",
        Some(&format!("{named} Secure Boot")),
    )?;
    usable(&cn)?;
    Ok(cn)
}

/// The keypair a published image is signed with, and the policy module verifies
/// updates against.
fn cosign(work: &Path) -> Result<(PathBuf, PathBuf), String> {
    let mut generate = Command::new("cosign");
    generate
        .arg("generate-key-pair")
        .current_dir(work)
        .env("COSIGN_PASSWORD", "");
    finish(
        generate,
        "cosign",
        "get it from https://github.com/sigstore/cosign/releases",
    )?;
    Ok((work.join("cosign.pub"), work.join("cosign.key")))
}

/// A self-signed certificate and its key, at the declared size and in the
/// declared form.
fn openssl(work: &Path, declared: &Declared, cn: &str) -> Result<(PathBuf, PathBuf), String> {
    let config = work.join("openssl.cnf");
    crate::init::put(&config, &module_signing(cn))?;
    let public = work.join("public");
    let private = work.join("private.pem");

    let mut generate = Command::new("openssl");
    generate
        .args([
            "req", "-x509", "-new", "-nodes", "-utf8", "-sha256", "-days", "36500", "-batch",
            "-newkey",
        ])
        .arg(format!("rsa:{}", declared.bits))
        .args(["-outform", &declared.format.to_uppercase()])
        .arg("-config")
        .arg(&config)
        .arg("-out")
        .arg(&public)
        .arg("-keyout")
        .arg(&private);
    finish(
        generate,
        "openssl",
        "install it from your platform's openssl package",
    )?;
    Ok((public, private))
}

/// A key already on disk is never replaced: the private half cannot be
/// recovered. The zero-byte file a module ships as a placeholder is not one.
fn unwritten(path: &Path) -> Result<(), String> {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > 0 => Err(format!(
            "{} holds a key already, and a key is never overwritten; move it aside first",
            path.display()
        )),
        _ => Ok(()),
    }
}

/// Outside the repository, so a run that fails leaves nothing in the tree, and
/// readable only by its owner while the private half sits in it.
fn workspace(what: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("tect-{what}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|err| format!("{}: {err}", dir.display()))?;
    Ok(dir)
}

fn install(from: &Path, to: &Path, mode: u32) -> Result<(), String> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    std::fs::copy(from, to).map_err(|err| format!("{}: {err}", to.display()))?;
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(mode))
        .map_err(|err| format!("{}: {err}", to.display()))
}

/// A tool that may not be installed here fails naming what to install.
fn finish(mut command: Command, tool: &str, install: &str) -> Result<(), String> {
    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!(
            "`{tool}` exited {}",
            status.code().unwrap_or_default()
        )),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "`{tool}` is not installed, and it is what generates this key: {install}"
        )),
        Err(err) => Err(format!("{tool}: {err}")),
    }
}

fn shown(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// A common name that survives being written into an openssl config as it is.
fn usable(cn: &str) -> Result<&str, String> {
    let ok = !cn.is_empty()
        && cn.len() <= 64
        && cn.trim() == cn
        && cn
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " -._".contains(c));
    match ok {
        true => Ok(cn),
        false => Err(format!(
            "`{cn}` is not a usable common name: up to 64 letters, digits, spaces, \
             dashes, dots and underscores"
        )),
    }
}

/// `keyUsage` and `codeSigning` are what the kernel checks a module signing
/// certificate for, and what shim accepts as a MOK.
fn module_signing(cn: &str) -> String {
    format!(
        "[req]\n\
         distinguished_name = dn\n\
         prompt = no\n\
         x509_extensions = ext\n\
         \n\
         [dn]\n\
         CN = {cn}\n\
         \n\
         [ext]\n\
         basicConstraints = critical,CA:FALSE\n\
         keyUsage = digitalSignature\n\
         extendedKeyUsage = codeSigning\n\
         subjectKeyIdentifier = hash\n"
    )
}

/// The scaffolded `.gitignore` covers every private half. One that does not is
/// said so rather than edited: the tool never rewrites a file it did not write.
fn warn_unignored(root: &Path) {
    let name = "keys/private/";
    let ignored =
        std::fs::read_to_string(root.join(".gitignore")).is_ok_and(|text| ignores_private(&text));
    if !ignored {
        eprintln!("tect: nothing in .gitignore covers {name}; add that line before committing");
    }
}

fn ignores_private(text: &str) -> bool {
    text.lines()
        .any(|line| line.trim().trim_start_matches('/').trim_end_matches('/') == "keys/private")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    const SECUREBOOT: &str = r#"description "x"

supports "fedora"

key "secureboot" {
    generator "openssl" profile="module-signing" bits=4096
    public "/usr/share/secureboot/sb_cert.der" format="der"
    private "MOK.priv"
}
"#;

    fn have(tool: &str) -> bool {
        Command::new(tool)
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn repo(name: &str, manifest: &str) -> PathBuf {
        let root = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&root);
        crate::init::put(&root.join("modules/keyholder/module.kdl"), manifest).unwrap();
        root
    }

    /// The pair the build needs: `sign-file` reads the PEM key and the DER
    /// certificate, so both parse and both carry the same public key. Then that
    /// the second run refuses rather than replacing either half.
    #[test]
    fn a_secure_boot_key_is_well_formed() {
        if !have("openssl") {
            return;
        }
        let root = repo("tect-secureboot-key-test", SECUREBOOT);
        let ask = || {
            Key::collect(
                &root,
                Some("secureboot".into()),
                None,
                Some("Test Key".into()),
                &Prompt::silent(),
            )
        };
        ask().unwrap().apply(&root).unwrap();

        let openssl = |args: &[&str]| {
            let out = Command::new("openssl").args(args).output().unwrap();
            assert!(out.status.success(), "openssl {args:?}");
            out.stdout
        };
        let cert = layout::public_key(&root, "/usr/share/secureboot/sb_cert.der");
        let private = layout::private_key(&root, "MOK.priv");
        let from_cert = openssl(&[
            "x509",
            "-inform",
            "DER",
            "-in",
            &cert.display().to_string(),
            "-noout",
            "-pubkey",
        ]);
        let from_key = openssl(&["pkey", "-in", &private.display().to_string(), "-pubout"]);
        assert_eq!(from_cert, from_key);

        let again = ask().map(|_| ());
        assert!(again.is_err_and(|message| message.contains("never overwritten")));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A kind no module here declares, and none of the collections either.
    #[test]
    fn a_module_has_to_declare_the_key() {
        let root = repo("tect-no-key-test", SECUREBOOT);
        let message = Key::collect(&root, Some("cosign".into()), None, None, &Prompt::silent())
            .map(|_| ())
            .unwrap_err();
        assert_eq!(
            message,
            "no module in this repository declares a cosign key.\n\n\
             The keys declared here are secureboot. Import a module carrying one and run this again."
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_referenced_module_can_declare_a_repository_key() {
        let root = std::env::temp_dir().join("tect-remote-key-test");
        let _ = std::fs::remove_dir_all(&root);
        crate::init::put(
            &root.join("modules/.remote/one/keyholder/module.kdl"),
            SECUREBOOT,
        )
        .unwrap();
        let key = Key::collect(
            &root,
            Some("secureboot".into()),
            Some("one/keyholder".into()),
            Some("Test Key".into()),
            &Prompt::silent(),
        )
        .unwrap();
        assert_eq!(
            key.public,
            root.join("keys/public/usr/share/secureboot/sb_cert.der")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn private_key_ignore_spelling_is_flexible() {
        for line in ["keys/private/", "/keys/private/", "keys/private"] {
            assert!(ignores_private(line));
        }
    }
}

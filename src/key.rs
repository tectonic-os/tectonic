//! `create mok-key`. The public half goes into the files/ overlay of the module
//! that declares the path, and the private half at the repository root, which is
//! what the scaffolded `.gitignore` names.

use crate::model::remote::REMOTE_DIR;
use crate::parse::disk::Disk;
use crate::prompt::Prompt;
use crate::ui::Choice;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What the module ships the certificate as, which is the path it declares
/// `provides-file` for.
const SB_CERT: &str = "/usr/share/secureboot/sb_cert.der";

/// The private half, at the repository root and never in a commit.
const MOK_PRIV: &str = "MOK.priv";

/// The Secure Boot key the build signs kernel modules and the kernel with. The
/// certificate is written as DER, which is what `sign-file` and `mokutil` read.
pub fn mok(
    root: &Path,
    module: Option<String>,
    cn: Option<String>,
    prompt: &Prompt,
) -> Result<(), String> {
    let dir = provider(root, SB_CERT, module, prompt)?;
    let cert = overlay(root, &dir, SB_CERT);
    let private = root.join(MOK_PRIV);
    unwritten(&cert)?;
    unwritten(&private)?;

    let fallback = format!("{} Secure Boot", named_after_root(root));
    let cn = prompt.text(
        cn,
        "common name, which is what the enrolment prompt shows",
        "`--cn`",
        Some(&fallback),
    )?;
    let cn = common_name(&cn)?;

    let work = workspace("mok")?;
    let config = work.join("openssl.cnf");
    crate::init::put(&config, &openssl_config(cn))?;
    let mut generate = Command::new("openssl");
    generate
        .args([
            "req", "-x509", "-new", "-nodes", "-utf8", "-sha256", "-days", "36500", "-batch",
            "-newkey", "rsa:4096", "-outform", "DER",
        ])
        .arg("-config")
        .arg(&config)
        .arg("-out")
        .arg(work.join("cert.der"))
        .arg("-keyout")
        .arg(work.join("key.pem"));
    finish(generate, "openssl", "`dnf install openssl`")?;

    install(&work.join("cert.der"), &cert, 0o644)?;
    install(&work.join("key.pem"), &private, 0o600)?;
    let _ = std::fs::remove_dir_all(&work);

    println!("wrote {}", shown(root, &cert));
    println!("wrote {MOK_PRIV}");
    println!(
        "\nnext:\n\
         \x20 commit {cert}\n\
         \x20 gh secret set MOK_PRIVKEY < {MOK_PRIV}\n\
         \x20 MOK_KEY_PATH={MOK_PRIV} is what a local build reads it from\n\
         \n\
         every machine that boots this image enrols the certificate once, and\n\
         until it does the modules signed with it will not load:\n\
         \x20 sudo mokutil --import {cert}\n\
         \x20 it asks for a one-time password; reboot, choose Enroll MOK, and\n\
         \x20 give the same password\n",
        cert = shown(root, &cert)
    );
    warn_unignored(root, MOK_PRIV);
    Ok(())
}

/// The module whose files/ overlay ships `path`. A fetched module is not
/// offered: its tree is replaced on the next fetch and is not committed.
fn provider(
    root: &Path,
    path: &str,
    given: Option<String>,
    prompt: &Prompt,
) -> Result<String, String> {
    let mut found = Disk::scan(root).providers.remove(path).unwrap_or_default();
    found.retain(|dir| !dir.starts_with(&format!("{REMOTE_DIR}/")));

    if let Some(given) = given {
        return match found.contains(&given) {
            true => Ok(given),
            false => Err(format!(
                "`modules/{given}` does not declare `provides-file \"{path}\"`"
            )),
        };
    }
    match found.as_slice() {
        [] => Err(format!(
            "no module declares `provides-file \"{path}\"`, and that module is where \
             the certificate goes; write or import one first"
        )),
        [one] => Ok(one.clone()),
        many => {
            let options: Vec<Choice> = many.iter().map(|dir| Choice::new(dir, "")).collect();
            let listed = many.join(", ");
            prompt
                .choose(&format!("{listed} all ship `{path}`; which one"), &options)?
                .map(|chosen| many[chosen].clone())
                .ok_or_else(|| {
                    format!("`{path}` is declared by {listed}; name one with `--module`")
                })
        }
    }
}

/// Where a module's files/ overlay puts `path` on disk.
fn overlay(root: &Path, dir: &str, path: &str) -> PathBuf {
    root.join("modules")
        .join(dir)
        .join("files")
        .join(path.trim_start_matches('/'))
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

fn named_after_root(root: &Path) -> String {
    std::fs::canonicalize(root)
        .ok()
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "tectonic".to_string())
}

/// A common name that survives being written into an openssl config as it is.
fn common_name(cn: &str) -> Result<&str, String> {
    let usable = !cn.is_empty()
        && cn.len() <= 64
        && cn.trim() == cn
        && cn
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " -._".contains(c));
    match usable {
        true => Ok(cn),
        false => Err(format!(
            "`{cn}` is not a usable common name: up to 64 letters, digits, spaces, \
             dashes, dots and underscores"
        )),
    }
}

/// `keyUsage` and `codeSigning` are what the kernel checks a module signing
/// certificate for, and what shim accepts as a MOK.
fn openssl_config(cn: &str) -> String {
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

/// The scaffolded `.gitignore` names both private halves. One that does not is
/// said so rather than edited: the tool never rewrites a file it did not write.
fn warn_unignored(root: &Path, name: &str) {
    let ignored = std::fs::read_to_string(root.join(".gitignore"))
        .is_ok_and(|text| text.lines().any(|line| line.trim() == name));
    if !ignored {
        eprintln!("tect: nothing in .gitignore covers {name}; add that line before committing");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    fn have(tool: &str) -> bool {
        Command::new(tool)
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn repo(name: &str, provides: &str) -> PathBuf {
        let root = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&root);
        crate::init::put(
            &root.join("modules/keyholder/module.kdl"),
            &format!("description \"x\"\n\nsupports \"fedora\"\n\nprovides-file \"{provides}\"\n"),
        )
        .unwrap();
        root
    }

    fn openssl(args: &[&str]) -> Vec<u8> {
        let out = Command::new("openssl").args(args).output().unwrap();
        assert!(out.status.success(), "openssl {args:?}");
        out.stdout
    }

    /// The pair the build needs: `sign-file` reads the PEM key and the DER
    /// certificate, so both parse and both carry the same public key. Then that
    /// the second run refuses rather than replacing either half.
    #[test]
    fn mok_key_is_well_formed() {
        if !have("openssl") {
            return;
        }
        let root = repo("tect-mok-key-test", SB_CERT);
        mok(
            &root,
            None,
            Some("Test Key".into()),
            &crate::prompt::Prompt::silent(),
        )
        .unwrap();

        let cert = overlay(&root, "keyholder", SB_CERT);
        let private = root.join(MOK_PRIV);
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

        let again = mok(&root, None, Some("Test Key".into()), &Prompt::silent());
        assert!(again.is_err_and(|message| message.contains("never overwritten")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_module_has_to_declare_the_file() {
        let root = repo("tect-no-provider-test", "/usr/bin/elsewhere");
        let message = provider(&root, SB_CERT, None, &Prompt::silent()).unwrap_err();
        assert!(message.contains("no module declares"));
        let _ = std::fs::remove_dir_all(&root);
    }
}

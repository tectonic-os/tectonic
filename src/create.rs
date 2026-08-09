//! `create repo`, `create image` and `create module`. Every step of a chain is
//! also a command: `create repo` calls `create image` in place rather than
//! writing an image of its own.

use crate::prompt::Prompt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What a repository that has chosen nothing builds on.
const BASE: &str = "quay.io/fedora/fedora-bootc:44";

const OWNERSHIP: &str = "your account or org on github (not tectonic-os)";

/// The tree, an image in it, then the remote, which is optional and last.
pub fn repo(
    name: Option<String>,
    owner: Option<String>,
    image_name: Option<String>,
    base: Option<String>,
    root_arg: Option<PathBuf>,
    prompt: &Prompt,
) -> Result<(), String> {
    let named_after_root = root_arg
        .as_ref()
        .and_then(|root| std::fs::canonicalize(root).ok())
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned());
    let name = prompt.text(
        name.or(named_after_root),
        "repository name",
        "a name argument",
        None,
    )?;
    let id = crate::init::id(&name)?;
    let root = root_arg.unwrap_or_else(|| PathBuf::from(&id));
    refuse_nesting(&root)?;

    let owner = prompt.text(owner, &format!("owner, {OWNERSHIP}"), "`--owner`", None)?;
    crate::init::write(&root, &name, &crate::init::assets()?)?;
    println!("wrote {name} into {}", root.display());

    if image_name.is_some() || prompt.confirm("create an image in it now")? {
        image(&root, image_name, base, Some(&owner), "`--image`", prompt)?;
    }

    let next = match remote(&owner, &id, prompt)? {
        true => format!(
            "git remote add origin https://github.com/{owner}/{id} && git push -u origin main"
        ),
        false => format!("gh repo create {owner}/{id} --source=. --push"),
    };
    println!(
        "\nnext, in {}:\n\
         \x20 git init && git add -A && git commit\n\
         \x20 {next}\n",
        root.display()
    );
    Ok(())
}

/// One image, in `<image-id>.kdl` at the repository root.
pub fn image(
    root: &Path,
    name: Option<String>,
    base: Option<String>,
    owner: Option<&str>,
    flag: &str,
    prompt: &Prompt,
) -> Result<String, String> {
    let name = prompt.text(name, "image name", flag, None)?;
    let id = crate::init::id(&name)?;
    let base = prompt.text(base, "base image", "`--base`", Some(BASE))?;

    let file = root.join(format!("{id}.kdl"));
    if file.exists() {
        return Err(format!("{} is already there", file.display()));
    }
    crate::init::put(&file, &image_kdl(&name, &id, owner, &base))?;
    println!("wrote {}", file.display());
    Ok(id)
}

fn image_kdl(name: &str, id: &str, owner: Option<&str>, base: &str) -> String {
    let urls = match owner {
        Some(owner) => format!(
            "\x20   url \"https://github.com/{owner}/{id}\"\n\
             \x20   issues-url \"https://github.com/{owner}/{id}/issues\"\n"
        ),
        None => String::new(),
    };
    format!(
        "image {{\n\
         \x20   name \"{name}\"\n\
         {urls}\n\
         \x20   base \"{base}\" {{\n\
         \x20       family \"fedora\"\n\
         \x20       provides \"rechunking\" \"initramfs-generation\" \"mac-policy\"\n\
         \x20   }}\n\
         \n\
         \x20   modules {{\n\
         \x20   }}\n\
         }}\n"
    )
}

/// A repository inside a repository: the outer one would read the inner tree as
/// its own modules and images.
fn refuse_nesting(root: &Path) -> Result<(), String> {
    let full = match root.is_absolute() {
        true => root.to_path_buf(),
        false => std::env::current_dir().unwrap_or_default().join(root),
    };
    let from = full.ancestors().find(|dir| dir.exists()).unwrap_or(&full);
    match crate::find_root(from) {
        Some(outer) => Err(format!(
            "{} is inside the repository at {}; a repository does not nest",
            root.display(),
            outer.display()
        )),
        None => Ok(()),
    }
}

/// Offered only where `gh` is installed, and only after the tree is written.
/// Whether the repository is created, committed and pushed is the user's.
fn remote(owner: &str, id: &str, prompt: &Prompt) -> Result<bool, String> {
    let present = Command::new("gh")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !present || !prompt.confirm(&format!("create {owner}/{id} on github now"))? {
        return Ok(false);
    }
    Command::new("gh")
        .args(["repo", "create", &format!("{owner}/{id}"), "--public"])
        .status()
        .map(|status| status.success())
        .map_err(|err| format!("gh: {err}"))
}

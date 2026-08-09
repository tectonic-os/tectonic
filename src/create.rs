//! `create repo`, `create image` and `create module`. Every step of a chain is
//! also a command: `create repo` calls `create image` in place rather than
//! writing an image of its own.

use crate::prompt::Prompt;
use crate::ui::Choice;
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
) -> Result<(), String> {
    let name = prompt.text(name, "image name", flag, None)?;
    let id = crate::init::id(&name)?;
    let base = prompt.text(base, "base image", "`--base`", Some(BASE))?;

    let file = root.join(format!("{id}.kdl"));
    if file.exists() {
        return Err(format!("{} is already there", file.display()));
    }
    crate::init::put(&file, &image_kdl(&name, &id, owner, &base))?;
    println!("wrote {}", file.display());
    Ok(())
}

/// One module in the repository, and the offer to list it in an image, which
/// is a separate operation.
pub fn module(
    root: &Path,
    name: Option<String>,
    pkgs: Vec<String>,
    with: Vec<(String, String)>,
    image_name: Option<String>,
    prompt: &Prompt,
) -> Result<(), String> {
    let name = prompt.text(name, "module name", "a name argument", None)?;
    let path = name
        .split('/')
        .map(crate::init::id)
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    let file = root.join("modules").join(&path).join("module.kdl");
    if file.exists() {
        return Err(format!("modules/{path} is already there"));
    }

    let pkgs = match pkgs.is_empty() && prompt.confirm("does it install packages")? {
        true => prompt
            .text(
                None,
                "package names, separated by spaces",
                "`--pkg`",
                Some(""),
            )?
            .split_whitespace()
            .map(str::to_string)
            .collect(),
        false => pkgs,
    };

    crate::init::put(&file, &module_kdl(&name, &pkgs, &with)?)?;
    println!("wrote modules/{path}/module.kdl");
    add_to_image(root, &path, image_name, prompt)
}

fn module_kdl(name: &str, pkgs: &[String], with: &[(String, String)]) -> Result<String, String> {
    let mut text = format!(
        "description \"{}\"\n\nsupports \"fedora\"\n",
        quotable(name)?
    );
    for (verb, value) in with {
        text.push_str(&format!("{} \"{}\"\n", quotable(verb)?, quotable(value)?));
    }
    if !pkgs.is_empty() {
        let mut listed = String::new();
        for pkg in pkgs {
            listed.push_str(&format!(" \"{}\"", quotable(pkg)?));
        }
        text.push_str(&format!("\npackages {{\n    fedora{listed}\n}}\n"));
    }
    Ok(text)
}

/// A value that would have to be escaped to survive being written into KDL.
fn quotable(value: &str) -> Result<&str, String> {
    match value.contains(['"', '\\', '\n']) {
        true => Err(format!(
            "`{value}` is not writable into a manifest as it is"
        )),
        false => Ok(value),
    }
}

/// Which image, or none: it asks even when there is one, because having a
/// module in the repository and listing it in an image are different decisions.
pub fn add_to_image(
    root: &Path,
    path: &str,
    given: Option<String>,
    prompt: &Prompt,
) -> Result<(), String> {
    let (list, _) = crate::model::image::List::load(root);
    let ids: Vec<String> = list.images.iter().map(|image| image.id.clone()).collect();
    let options: Vec<Choice> = list
        .images
        .iter()
        .map(|image| match image.name == image.id {
            true => Choice::new(&image.id, ""),
            false => Choice::new(&image.id, &image.name),
        })
        .collect();
    if ids.is_empty() {
        println!("no image lists it yet; `tect create image <name>` writes one");
        return Ok(());
    }

    let chosen = match given {
        Some(id) => Some(ids.iter().position(|known| *known == id).ok_or_else(|| {
            format!(
                "`{id}` is not a declared image; there is {}",
                ids.join(", ")
            )
        })?),
        None => prompt.choose("list it in an image", &options)?,
    };
    let Some(chosen) = chosen else {
        println!("next, to build it, list it in an image:\n\x20 module \"{path}\"");
        return Ok(());
    };

    let file = Path::new(list.images[chosen].src.name());
    append_module(file, path)?;
    println!("listed \"{path}\" in {}", file.display());
    Ok(())
}

/// One `module` line before the closing brace of the image's `modules` block.
/// Every other byte is left where it was: the tool creates whole files and
/// appends module lines, and never rewrites a value.
fn append_module(file: &Path, path: &str) -> Result<(), String> {
    let mut text =
        std::fs::read_to_string(file).map_err(|err| format!("{}: {err}", file.display()))?;
    let close = crate::parse::image::modules_close(&text)
        .ok_or_else(|| format!("{} has no `modules` block to add to", file.display()))?;

    let start = text[..close].rfind('\n').map_or(0, |at| at + 1);
    let indent = &text[start..close];
    let (at, line) = match indent.trim().is_empty() {
        true => (start, format!("{indent}    module \"{path}\"\n")),
        false => (close, format!("module \"{path}\" ")),
    };
    text.insert_str(at, &line);
    std::fs::write(file, text).map_err(|err| format!("{}: {err}", file.display()))
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

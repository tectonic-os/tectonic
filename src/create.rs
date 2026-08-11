//! `create repo`, `create image` and `create module`. Every step of a chain is
//! also a command: `create repo` calls `create image` in place rather than
//! writing an image of its own.
//!
//! Each of them collects every answer first and writes afterwards, which is why
//! no `apply` takes a `Prompt`.

use crate::model::image::REPO_FILE;
use crate::prompt::Prompt;
use crate::ui::Choice;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const OWNERSHIP: &str = "your account or org on github (not tectonic-os)";

/// The tree, an image in it, then the remote, which is optional and last: each
/// step after the first adds to what the one before it wrote.
pub struct Repo {
    name: String,
    id: String,
    root: PathBuf,
    owner: String,
    assets: PathBuf,
    image: Option<Image>,
    remote: bool,
}

impl Repo {
    pub fn collect(
        name: Option<String>,
        owner: Option<String>,
        image_name: Option<String>,
        base: Option<String>,
        root_arg: Option<PathBuf>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
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
        let assets = crate::init::assets()?;

        let owner = prompt.text(owner, &format!("owner, {OWNERSHIP}"), "`--owner`", None)?;
        let image = match image_name.is_some() || prompt.confirm("create an image in it now")? {
            true => Some(Image::collect(
                &root,
                image_name,
                base,
                Some(owner.clone()),
                "`--image`",
                prompt,
            )?),
            false => None,
        };
        let remote =
            gh_installed() && prompt.confirm(&format!("create {owner}/{id} on github now"))?;
        Ok(Self {
            name,
            id,
            root,
            owner,
            assets,
            image,
            remote,
        })
    }

    pub fn apply(&self) -> Result<(), String> {
        crate::init::write(&self.root, &self.name, &self.assets)?;
        println!("wrote {} into {}", self.name, self.root.display());
        if let Some(image) = &self.image {
            image.apply(&self.root)?;
        }
        if self.remote {
            create_remote(&self.owner, &self.id)?;
            println!("created {}/{} on github", self.owner, self.id);
        }

        let Self { owner, id, .. } = self;
        let next = match self.remote {
            true => format!(
                "git remote add origin https://github.com/{owner}/{id} && git push -u origin main"
            ),
            false => format!("gh repo create {owner}/{id} --source=. --push"),
        };
        println!(
            "\nnext, in {}:\n\
             \x20 git init && git add -A && git commit\n\
             \x20 {next}\n",
            self.root.display()
        );
        Ok(())
    }
}

/// One image, in `<image-id>.kdl` at the repository root.
pub struct Image {
    name: String,
    id: String,
    owner: Option<String>,
    base: String,
    family: String,
    file: PathBuf,
    /// The image a second one takes the fallback away from, named in repo.kdl
    /// so that a bare build still builds what it built before.
    names_default: Option<String>,
}

impl Image {
    pub fn collect(
        root: &Path,
        name: Option<String>,
        base: Option<String>,
        owner: Option<String>,
        flag: &str,
        prompt: &Prompt,
    ) -> Result<Self, String> {
        let name = prompt.text(name, "image name", flag, None)?;
        let id = crate::init::id(&name)?;
        let file = root.join(format!("{id}.kdl"));
        if file.exists() {
            return Err(format!("{} is already there", file.display()));
        }
        let names_default = implicit_default(root).filter(|was| *was != id);

        let base = match base {
            Some(given) => given,
            None => choose_base(prompt)?,
        };
        let family = match crate::base::find(&base) {
            Some(known) => known.family.to_string(),
            None => prompt.text(
                None,
                "base family",
                "`--base`, naming a base the catalog knows",
                Some(crate::base::DEFAULT.family),
            )?,
        };
        Ok(Self {
            name,
            id,
            owner,
            base,
            family,
            file,
            names_default,
        })
    }

    pub fn apply(&self, root: &Path) -> Result<(), String> {
        let text = image_kdl(
            &self.name,
            &self.id,
            self.owner.as_deref(),
            &self.base,
            &self.family,
            crate::base::find(&self.base),
        );
        crate::init::put(&self.file, &text)?;
        println!("wrote {}", self.file.display());
        if let Some(was) = &self.names_default {
            append_default_image(root, was)?;
            println!("named \"{was}\" the default image in {REPO_FILE}");
        }
        Ok(())
    }
}

/// The image a repository with one of them and no `default-image` falls back
/// to, which a second image takes away unless it is written down.
fn implicit_default(root: &Path) -> Option<String> {
    let (list, _) = crate::model::image::List::load(root);
    match (&list.default_image_id, list.images.as_slice()) {
        (None, [only]) => Some(only.id.clone()),
        _ => None,
    }
}

/// One `default-image` line at the end of repo.kdl, which does not carry the
/// node: an append, never a rewrite.
fn append_default_image(root: &Path, id: &str) -> Result<(), String> {
    let file = root.join(REPO_FILE);
    let mut text =
        std::fs::read_to_string(&file).map_err(|err| format!("{}: {err}", file.display()))?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("\ndefault-image \"{id}\"\n"));
    std::fs::write(&file, text).map_err(|err| format!("{}: {err}", file.display()))
}

/// One of the bases the catalog holds, or one typed in: an unknown base is not
/// an error, it is a base nothing can say anything about.
fn choose_base(prompt: &Prompt) -> Result<String, String> {
    let options: Vec<Choice> = crate::base::CATALOG
        .iter()
        .map(|base| Choice::new(base.image, base.about))
        .collect();
    match prompt.choose("which base, or none to name another", &options)? {
        Some(chosen) => Ok(crate::base::CATALOG[chosen].image.to_string()),
        None => prompt.text(
            None,
            "base image",
            "`--base`",
            Some(crate::base::DEFAULT.image),
        ),
    }
}

/// One module in the repository, and the offer to list it in an image, which
/// is a separate operation.
pub struct Module {
    path: String,
    file: PathBuf,
    text: String,
    listing: Listing,
}

impl Module {
    pub fn collect(
        root: &Path,
        name: Option<String>,
        pkgs: Vec<String>,
        with: Vec<(String, String)>,
        image_name: Option<String>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
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

        let text = module_kdl(&name, &family(root), &pkgs, &with)?;
        let listing = Listing::collect(root, image_name, prompt)?;
        Ok(Self {
            path,
            file,
            text,
            listing,
        })
    }

    pub fn apply(&self) -> Result<(), String> {
        crate::init::put(&self.file, &self.text)?;
        println!("wrote modules/{}/module.kdl", self.path);
        self.listing.apply(&self.path)
    }
}

/// The family the repository already builds on.
fn family(root: &Path) -> String {
    let (list, _) = crate::model::image::List::load(root);
    list.images
        .iter()
        .find_map(|image| image.base.as_ref().map(|base| base.family.clone()))
        .unwrap_or_else(|| crate::base::DEFAULT.family.to_string())
}

fn module_kdl(
    name: &str,
    family: &str,
    pkgs: &[String],
    with: &[(String, String)],
) -> Result<String, String> {
    let mut text = format!(
        "description \"{}\"\n\nsupports \"{family}\"\n",
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
        text.push_str(&format!("\npackages {{\n    {family}{listed}\n}}\n"));
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

/// Which image a module is listed in, or why none is. It asks even when there
/// is one image, because having a module in the repository and listing it in an
/// image are different decisions.
pub enum Listing {
    /// Nothing to list it in yet.
    NoImage,
    /// None of them, which is an answer.
    Declined,
    In(PathBuf),
}

impl Listing {
    pub fn collect(root: &Path, given: Option<String>, prompt: &Prompt) -> Result<Self, String> {
        let (list, _) = crate::model::image::List::load(root);
        let ids: Vec<String> = list.images.iter().map(|image| image.id.clone()).collect();
        if ids.is_empty() {
            return Ok(Self::NoImage);
        }
        let options: Vec<Choice> = list
            .images
            .iter()
            .map(|image| match image.name == image.id {
                true => Choice::new(&image.id, ""),
                false => Choice::new(&image.id, &image.name),
            })
            .collect();

        let chosen = match given {
            Some(id) => Some(ids.iter().position(|known| *known == id).ok_or_else(|| {
                format!(
                    "`{id}` is not a declared image; there is {}",
                    ids.join(", ")
                )
            })?),
            None => prompt.choose("list it in an image", &options)?,
        };
        Ok(match chosen {
            Some(chosen) => Self::In(PathBuf::from(list.images[chosen].src.name())),
            None => Self::Declined,
        })
    }

    pub fn apply(&self, path: &str) -> Result<(), String> {
        match self {
            Self::NoImage => {
                println!("no image lists it yet; `tect create image <name>` writes one")
            }
            Self::Declined => {
                println!("next, to build it, list it in an image:\n\x20 module \"{path}\"")
            }
            Self::In(file) => {
                append_module(file, path)?;
                println!("listed \"{path}\" in {}", file.display());
            }
        }
        Ok(())
    }
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

fn image_kdl(
    name: &str,
    id: &str,
    owner: Option<&str>,
    base: &str,
    family: &str,
    known: Option<&crate::base::Base>,
) -> String {
    let urls = match owner {
        Some(owner) => format!(
            "\x20   url \"https://github.com/{owner}/{id}\"\n\
             \x20   issues-url \"https://github.com/{owner}/{id}/issues\"\n"
        ),
        None => String::new(),
    };
    let mut ships = String::new();
    if let Some(known) = known {
        for (node, names) in [
            ("provides", known.provides),
            ("provides-file", known.provides_files),
        ] {
            if !names.is_empty() {
                let listed: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
                ships.push_str(&format!("\x20       {node} {}\n", listed.join(" ")));
            }
        }
    }
    format!(
        "image {{\n\
         \x20   name \"{name}\"\n\
         {urls}\n\
         \x20   base \"{base}\" {{\n\
         \x20       family \"{family}\"\n\
         {ships}\
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

/// The remote is offered only where `gh` is installed, which is a collect-time
/// read: whether it is created, committed and pushed is the user's.
fn gh_installed() -> bool {
    Command::new("gh")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn create_remote(owner: &str, id: &str) -> Result<(), String> {
    match Command::new("gh")
        .args(["repo", "create", &format!("{owner}/{id}"), "--public"])
        .status()
    {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!(
            "`gh repo create` exited {}",
            status.code().unwrap_or_default()
        )),
        Err(err) => Err(format!("gh: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_catalogued_base_writes_what_it_ships_and_an_unknown_one_writes_nothing() {
        let bazzite = "ghcr.io/ublue-os/bazzite:stable";
        let known = image_kdl(
            "Bazzite",
            "bazzite",
            None,
            bazzite,
            "fedora",
            crate::base::find(bazzite),
        );
        assert!(
            known.contains("        provides \"rechunking\" \"flatpak\"\n"),
            "{known}"
        );
        assert!(
            known.contains("        provides-file \"/usr/bin/flatpak\"\n"),
            "{known}"
        );

        let unknown = image_kdl("Own", "own", None, "example.invalid/own:1", "fedora", None);
        assert!(!unknown.contains("provides"), "{unknown}");
        assert!(
            unknown.contains("base \"example.invalid/own:1\" {\n        family \"fedora\"\n"),
            "{unknown}"
        );
    }

    /// A capability the catalog misspells is written into every image scaffolded
    /// on that base, where it suppresses nothing and satisfies nothing.
    #[test]
    fn every_catalogued_name_is_a_name() {
        use crate::model::image::is_name;
        for base in crate::base::CATALOG {
            assert!(is_name(base.family), "{}", base.image);
            for name in base.provides {
                assert!(is_name(name), "{} provides {name}", base.image);
            }
            for path in base.provides_files {
                assert!(path.starts_with('/'), "{} provides {path}", base.image);
            }
        }
    }
}

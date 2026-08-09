//! The tree a new repository starts as.

use crate::model::image::{is_name, REPO_FILE, SCHEMA_VERSION, TECT_VERSION};
use std::fs;
use std::path::{Path, PathBuf};

/// Where the release installs the scaffolding it ships.
const INSTALLED: &str = "/usr/share/tectonic/assets";

/// The scaffolding directory, looked for in this order: `TECT_ASSETS`, an
/// `assets` directory beside the binary, which is how the release tarball
/// unpacks, then the install path.
pub fn assets() -> Result<PathBuf, String> {
    let mut tried: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("TECT_ASSETS") {
        tried.push(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        tried.push(dir.join("assets"));
    }
    tried.push(PathBuf::from(INSTALLED));

    match tried.iter().find(|dir| dir.is_dir()) {
        Some(dir) => Ok(dir.clone()),
        None => Err(format!(
            "no assets directory; looked in {}",
            tried
                .iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The machine name `name` derives, which every generated reference uses.
pub fn id(name: &str) -> Result<String, String> {
    let id = name.to_lowercase().replace(' ', "-");
    if !is_name(&id) {
        return Err(format!(
            "`{name}` does not derive a usable name: lowercase letters, digits and \
             dashes, starting with a letter"
        ));
    }
    Ok(id)
}

/// Writes a repository into `root`: repo.kdl, the module directory, and
/// everything under `assets`, which is an image repository's root. The images
/// are `create image`'s.
pub fn write(root: &Path, name: &str, assets: &Path) -> Result<(), String> {
    if root.join(REPO_FILE).exists() {
        return Err(format!("{} is already a repository", root.display()));
    }

    copy_tree(assets, root)?;
    put(
        &root.join(REPO_FILE),
        &format!(
            "schema-version {SCHEMA_VERSION}\n\n\
             // renovate: datasource=github-releases depName=tectonic-os/tectonic\n\
             tect-version \"{TECT_VERSION}\"\n"
        ),
    )?;
    put(&root.join("README.md"), &format!("# {name}\n"))?;
    // A module directory that survives a commit: the build context mounts it.
    put(&root.join("modules/.gitkeep"), "")?;
    Ok(())
}

pub(crate) fn put(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    fs::write(path, text).map_err(|err| format!("{}: {err}", path.display()))
}

pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|err| format!("{}: {err}", to.display()))?;
    let entries = fs::read_dir(from).map_err(|err| format!("{}: {err}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("{}: {err}", from.display()))?;
        let source = entry.path();
        let dest = to.join(entry.file_name());
        if source.is_dir() {
            copy_tree(&source, &dest)?;
        } else {
            fs::copy(&source, &dest).map_err(|err| format!("{}: {err}", dest.display()))?;
        }
    }
    Ok(())
}

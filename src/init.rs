//! The tree a new repository starts as.

use crate::layout;
use crate::model::image::{is_name, SCHEMA_VERSION, TECT_VERSION};
use std::fs;
use std::path::{Path, PathBuf};

/// Where the release installs the scaffolding it ships, per-user first: on a
/// bootc or ostree host `/usr/share` is read-only.
const INSTALLED: [&str; 2] = [
    "/usr/local/share/tectonic/assets",
    "/usr/share/tectonic/assets",
];

/// The `sources` block a new repo.kdl is scaffolded with, which is one of the
/// assets rather than a value in here: editing it changes what every
/// repository created afterwards declares, and deleting it scaffolds none. It
/// is spliced into repo.kdl, so the copy that lands at the root is taken out
/// again.
pub const SOURCES_FILE: &str = "repo.sources.kdl";

pub fn sources(assets: &Path) -> String {
    fs::read_to_string(assets.join(SOURCES_FILE)).unwrap_or_default()
}

/// The scaffolding directory, looked for in this order: `TECT_ASSETS`, an
/// `assets` directory beside the binary, which is how the release tarball
/// unpacks, then the install paths. First match wins.
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
    if let Some(dir) = data_home() {
        tried.push(dir.join("tectonic/assets"));
    }
    tried.extend(INSTALLED.iter().map(PathBuf::from));

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

/// `$XDG_DATA_HOME`, else `~/.local/share`. `upgrade` reads it too, so the
/// per-user assets path has one definition rather than two that drift.
pub(crate) fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
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
/// are `create image`'s. Answers every path it wrote, relative to `root`.
pub fn write(root: &Path, name: &str, assets: &Path) -> Result<Vec<PathBuf>, String> {
    if root.join(layout::REPO_FILE).exists() {
        return Err(format!("{} is already a repository", root.display()));
    }

    let mut wrote = copy_tree(assets, root)?;
    // The workflows are generated from the declaration, not scaffolded.
    let shipped = root.join(layout::WORKFLOW_DIR);
    if shipped.is_dir() {
        fs::remove_dir_all(&shipped).map_err(|err| format!("{}: {err}", shipped.display()))?;
    }
    wrote.retain(|path| !path.starts_with(layout::WORKFLOW_DIR));

    let scaffold = root.join(SOURCES_FILE);
    let sources = match sources(assets) {
        block if block.is_empty() => block,
        block => format!("\n{block}"),
    };
    for control in [scaffold.as_path(), &root.join(crate::base::BASES_FILE)] {
        if control.is_file() {
            fs::remove_file(control).map_err(|err| format!("{}: {err}", control.display()))?;
        }
        wrote.retain(|path| root.join(path) != control);
    }
    put(
        &root.join(layout::REPO_FILE),
        &format!(
            "schema-version {SCHEMA_VERSION}\n\
             name \"{name}\"\n\n\
             // renovate: datasource=github-releases depName=tectonic-os/tectonic\n\
             tect-version \"{TECT_VERSION}\"\n\
             {sources}"
        ),
    )?;
    put(&root.join("README.md"), &format!("# {name}\n"))?;
    // A module directory that survives a commit: the build context mounts it.
    put(&root.join("modules/.gitkeep"), "")?;
    wrote.extend([layout::REPO_FILE, "README.md", "modules/.gitkeep"].map(PathBuf::from));
    Ok(wrote)
}

pub(crate) fn put(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    fs::write(path, text).map_err(|err| format!("{}: {err}", path.display()))
}

/// Answers every file it copied, relative to `to`.
pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(to).map_err(|err| format!("{}: {err}", to.display()))?;
    let entries = fs::read_dir(from).map_err(|err| format!("{}: {err}", from.display()))?;
    let mut wrote = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("{}: {err}", from.display()))?;
        let source = entry.path();
        let name = entry.file_name();
        let dest = to.join(&name);
        if source.is_dir() {
            wrote.extend(
                copy_tree(&source, &dest)?
                    .into_iter()
                    .map(|under| Path::new(&name).join(under)),
            );
        } else {
            fs::copy(&source, &dest).map_err(|err| format!("{}: {err}", dest.display()))?;
            wrote.push(PathBuf::from(name));
        }
    }
    Ok(wrote)
}

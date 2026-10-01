//! The tree a new repository starts as.

use crate::layout;
use crate::model::image::{is_name, SCHEMA_VERSION};
use std::fs;
use std::path::{Path, PathBuf};

/// Where the release installs the scaffolding it ships, per-user first: on a
/// bootc or ostree host `/usr/share` is read-only.
const INSTALLED: [&str; 2] = [
    "/usr/local/share/tectonic/assets",
    "/usr/share/tectonic/assets",
];

/// What `create repo` copies once and `generate` never writes again, as this
/// release ships it.
const SCAFFOLDED: [(&str, &str); 6] = [
    (".dockerignore", include_str!("../assets/.dockerignore")),
    (".gitattributes", include_str!("../assets/.gitattributes")),
    (".gitignore", include_str!("../assets/.gitignore")),
    (".shellcheckrc", include_str!("../assets/.shellcheckrc")),
    (
        ".github/renovate.json5",
        include_str!("../assets/.github/renovate.json5"),
    ),
    (
        "disk_config/disk.toml",
        include_str!("../assets/disk_config/disk.toml"),
    ),
];

/// This release ships this Containerfile skeleton.
pub(crate) const SKELETON: &str = include_str!("../assets/scripts/Containerfile.skeleton");

/// Each scaffolded or kept file that is not what this release supplies. A
/// repository may keep its own, so this is said and never refused.
pub fn drifted(root: &Path) -> Vec<String> {
    let differs = |path: &str, shipped: &str| {
        fs::read_to_string(root.join(path)).is_ok_and(|held| held != shipped)
    };
    let version = env!("CARGO_PKG_VERSION");
    let scaffolded = SCAFFOLDED
        .iter()
        .filter(|(path, shipped)| differs(path, shipped))
        .map(|(path, _)| {
            format!(
                "`{path}` is not what tectonic v{version} scaffolds, and nothing rewrites it; \
                 the current one is assets/{path} in that release"
            )
        });
    // `create scripts` writes these, and a kept script calls the other scripts
    // where they live.
    let skeleton = crate::emit::containerfile::SKELETON;
    let kept_skeleton = differs(skeleton, SKELETON).then(|| {
        format!(
            "`{skeleton}` is not what tectonic v{version} supplies; if it is deleted, then \
             `generate` uses the one tect supplies"
        )
    });
    let kept_scripts = crate::emit::SCRIPTS
        .iter()
        .map(|script| (format!("{}/{}", layout::SCRIPTS, script.name), script))
        .filter(|(path, script)| differs(path, &crate::emit::located(root, script.body)))
        .map(|(path, script)| {
            format!(
                "`{path}` is not what tectonic v{version} supplies, and `generate` does not \
                 replace it; if it is deleted, then `generate` writes the current one to \
                 generated/scripts/{}",
                script.name
            )
        });
    scaffolded
        .chain(kept_skeleton)
        .chain(kept_scripts)
        .collect()
}

/// The `sources` block a new repo.kdl is scaffolded with, which is one of the
/// assets: editing it changes what every repository created afterwards
/// declares, and deleting it scaffolds none. It is spliced into repo.kdl, so
/// the copy that lands at the root is taken out again.
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
/// per-user assets path has one definition.
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

    // `create scripts` writes the scripts the user keeps, and `generate`
    // writes the declared workflows. The sources block is spliced into
    // repo.kdl, and the base catalog is read from the assets in place.
    let mut wrote = copy_tree_except(
        assets,
        root,
        &[
            "lib",
            layout::SCRIPTS,
            layout::WORKFLOW_DIR,
            SOURCES_FILE,
            crate::base::BASES_FILE,
        ],
    )?;

    let sources = match sources(assets) {
        block if block.is_empty() => block,
        block => format!("\n{block}"),
    };
    put(
        &root.join(layout::REPO_FILE),
        &format!(
            "schema-version {SCHEMA_VERSION}\n\
             name \"{name}\"\n\
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
    copy_tree_except(from, to, &[])
}

/// Copies a tree without the paths in `skip`, which are relative to `from`.
/// Answers every file it copied, relative to `to`.
fn copy_tree_except(from: &Path, to: &Path, skip: &[&str]) -> Result<Vec<PathBuf>, String> {
    copy_below(from, to, Path::new(""), skip)
}

fn copy_below(from: &Path, to: &Path, under: &Path, skip: &[&str]) -> Result<Vec<PathBuf>, String> {
    let dest_dir = to.join(under);
    fs::create_dir_all(&dest_dir).map_err(|err| format!("{}: {err}", dest_dir.display()))?;
    let dir = from.join(under);
    let entries = fs::read_dir(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let mut wrote = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("{}: {err}", dir.display()))?;
        let rel = under.join(entry.file_name());
        if skip.iter().any(|skip| rel == Path::new(skip)) {
            continue;
        }
        let source = entry.path();
        if source.is_dir() {
            wrote.extend(copy_below(from, to, &rel, skip)?);
        } else {
            let dest = to.join(&rel);
            fs::copy(&source, &dest).map_err(|err| format!("{}: {err}", dest.display()))?;
            wrote.push(rel);
        }
    }
    Ok(wrote)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table is what `create repo` copies, and a copy edited since is said.
    #[test]
    fn the_scaffolded_table_is_what_create_repo_copies() {
        let root = std::env::temp_dir().join(format!("tect-scaffold-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let mut wrote: Vec<String> = write(&root, "Example", &assets)
            .unwrap()
            .iter()
            .map(|path| path.display().to_string())
            .filter(|path| {
                ![layout::REPO_FILE, "README.md", "modules/.gitkeep"].contains(&path.as_str())
            })
            .collect();
        wrote.sort();
        let mut table: Vec<&str> = SCAFFOLDED.iter().map(|(path, _)| *path).collect();
        table.sort();
        assert_eq!(wrote, table);
        assert!(drifted(&root).is_empty());

        fs::write(root.join(".gitignore"), "mine\n").unwrap();
        fs::remove_file(root.join(".shellcheckrc")).unwrap();
        let said = drifted(&root);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].starts_with("`.gitignore`"), "{said:?}");
        fs::remove_dir_all(root).unwrap();
    }

    /// `create repo --root` accepts a directory that holds files already, and
    /// those files are the user's.
    #[test]
    fn a_repository_written_into_a_full_directory_keeps_the_users_files() {
        let root = std::env::temp_dir().join(format!("tect-keep-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let mine = [
            "scripts/deploy.sh",
            ".github/workflows/mine.yml",
            "bases.kdl",
        ];
        for path in mine {
            put(&root.join(path), "mine\n").unwrap();
        }
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        write(&root, "Example", &assets).unwrap();
        for path in mine {
            assert_eq!(
                fs::read_to_string(root.join(path)).unwrap(),
                "mine\n",
                "{path}"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}

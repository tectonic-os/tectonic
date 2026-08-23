//! The build script one module's layer runs: every value the host resolved,
//! written out, then the steps that use them.

use crate::layout;
use crate::model::image::{Entry, Image};
use crate::model::module::Module;
use crate::model::options::{env_name, OptType};
use crate::resolve::collect::Collection;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const HEADER: &str = "\
#!/usr/bin/env bash
# GENERATED FILE, do not edit.
set -euxo pipefail
";

/// Where the scripts for one image live, beside the Containerfile named for it.
pub fn dir(image: &Image) -> PathBuf {
    PathBuf::from(layout::GENERATED).join(format!("{}.d", image.id))
}

/// The script for one entry, as the path it is written at. A module listed
/// both ungated and under a flavour is two entries and two scripts.
pub fn path(image: &Image, entry: &Entry) -> PathBuf {
    let name = match &entry.flavour {
        Some(flavour) => format!("{}@{flavour}.sh", entry.path),
        None => format!("{}.sh", entry.path),
    };
    dir(image).join(name)
}

/// One script per module layer the Containerfile emits.
pub fn scripts(image: &Image, collection: &Collection, root: &Path) -> Vec<(PathBuf, String)> {
    let base_family = image.base.as_ref().map_or("", |b| b.family.as_str());
    let mut out = Vec::new();
    for entry in &image.entries {
        let Some(module) = entry.module.as_ref().filter(|m| m.standard_layer) else {
            continue;
        };
        out.push((
            path(image, entry),
            script(entry, module, collection, base_family, root),
        ));
    }
    out
}

fn script(
    entry: &Entry,
    module: &Module,
    collection: &Collection,
    base_family: &str,
    root: &Path,
) -> String {
    let dir = format!("/ctx/modules/{}", entry.dir());
    let mut out = String::from(HEADER);
    let _ = write!(out, "\nMODDIR={dir}\nexport MODDIR\n");

    if let Some(flavour) = &entry.flavour {
        let _ = write!(
            out,
            "\nif [ \"${{FLAVOUR:-}}\" != \"{flavour}\" ]; then\n\
             \x20   echo \"skipping {}: not built for '${{FLAVOUR:-the ungated build}}'\"\n\
             \x20   exit 0\n\
             fi\n",
            entry.path
        );
    }

    let lists: Vec<String> = module
        .options
        .iter()
        .filter(|o| o.ty == OptType::List)
        .map(|o| env_name(&o.name))
        .collect();
    if !module.resolved.is_empty() || !module.assets.is_empty() {
        out.push('\n');
    }
    for (name, value) in &module.resolved {
        if lists.contains(name) {
            let _ = writeln!(out, "{name}=({value})");
        } else {
            let _ = writeln!(out, "export {name}=\"{value}\"");
        }
    }
    for asset in &module.assets {
        for (name, value) in asset.env() {
            let _ = writeln!(out, "export {name}=\"{value}\"");
        }
    }

    let packages = module.packages.iter().filter(|g| g.family == base_family);
    if packages.clone().next().is_some() {
        out.push_str("\nsource /ctx/lib/family.sh\n");
    }
    for group in packages {
        let packages = group
            .packages
            .iter()
            .map(|package| shell(package))
            .collect::<Vec<_>>()
            .join(" ");
        let repo = group
            .enablerepo
            .as_ref()
            .map(|repo| format!("TECT_ENABLE_REPO={} ", shell(repo)))
            .unwrap_or_default();
        let _ = write!(out, "\n{repo}install_packages {packages}\n");
    }

    // The guard is `dnf5 config-manager`'s layout. A deb family writes its
    // repo somewhere else, so the module's own `repo` file runs unguarded
    // there and is what has to be idempotent.
    let on_disk = layout::module(root, entry.dir());
    if on_disk.join("repo").is_file() {
        match repo_id(&on_disk.join("repo")) {
            Some(id) => {
                let _ = write!(
                    out,
                    "\nif [ -f /etc/yum.repos.d/{id}.repo ]; then\n\
                     \x20   echo \"repo {id} is already configured\"\n\
                     else\n\
                     \x20   source {dir}/repo\n\
                     fi\n"
                );
            }
            None => {
                let _ = write!(out, "\nsource {dir}/repo\n");
            }
        }
    }

    if on_disk.join("module.sh").is_file() {
        let _ = write!(out, "\nsource {dir}/module.sh\n");
    }

    let policy = te_files(&on_disk.join("selinux"));
    if !policy.is_empty() {
        let _ = write!(out, "\nsource /ctx/lib/selinux-helpers.sh\n");
        for file in policy {
            // semodule compiles in place, and the module directory is a
            // read-only bind mount.
            let _ = write!(
                out,
                "cp {dir}/selinux/{file} /tmp/{file}\n\
                 install_selinux_module /tmp/{file}\n"
            );
        }
    }

    if on_disk.join(layout::OVERLAY).is_dir() {
        let _ = write!(out, "\ncp -rT {dir}/files /\n");
    }

    for key in &module.keys {
        let from = shell(&format!("/ctx/keys{}", key.public));
        let to = shell(&key.public);
        let _ = writeln!(out, "\ninstall -D -m 0644 -- {from} {to}",);
    }

    if let Some(collected) = collection.by_module.get(&entry.path) {
        for one in collected {
            let staged = Path::new(&one.staged);
            let _ = write!(
                out,
                "\nmkdir -p {}\ncat {dir}/{} > {}\n",
                staged.parent().unwrap_or(staged).display(),
                one.file,
                one.staged
            );
        }
    }

    out
}

fn shell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// `REPO_ID="fedora-thing"` out of a module's repo file, which is what says
/// the repository is already configured.
fn repo_id(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    text.lines()
        .find_map(|line| line.trim().strip_prefix("REPO_ID=").map(str::trim))
        .map(|value| value.trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
}

/// The policy sources a module ships, sorted so two runs emit the same script.
fn te_files(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".te"))
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn shell_paths_survive_a_single_quote() {
        assert_eq!(super::shell("a'b"), "'a'\"'\"'b'");
    }
}

//! The build script one module's layer runs: every value the host resolved,
//! written out, then the steps that use them.

use crate::layout;
use crate::model::image::{Entry, Image};
use crate::model::module::{Copr, Module};
use crate::model::options::{env_name, OptType};
use crate::resolve::collect::Collection;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const HEADER: &str = "\
#!/usr/bin/env bash
# GENERATED FILE, do not edit.
set -euxo pipefail
";

/// Where the scripts for one image live, under that image's own directory.
/// Below `modules/` there, so a module called `finalize` cannot land on the
/// image's own `finalize.sh` beside it.
pub fn dir(image: &Image) -> PathBuf {
    layout::generated_image(&image.id).join(layout::MODULES)
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

/// Every capability the image has, whoever offers it: what a family-shaped
/// piece of a module is selected against, the way `base_family` selects a
/// `packages` batch.
fn provided(image: &Image) -> BTreeSet<&str> {
    image
        .base
        .iter()
        .flat_map(|base| base.provides.iter().chain(base.provides_files.iter()))
        .map(|decl| decl.name.as_str())
        .chain(
            image
                .modules()
                .flat_map(|m| m.provides.iter().chain(m.provides_files.iter()))
                .map(|decl| decl.name.as_str()),
        )
        .collect()
}

/// One script per module layer the Containerfile emits.
pub fn scripts(image: &Image, collection: &Collection, root: &Path) -> Vec<(PathBuf, String)> {
    let base_family = image.base.as_ref().map_or("", |b| b.family.as_str());
    let has = provided(image);
    let mut out = Vec::new();
    for entry in &image.entries {
        let Some(module) = entry.module.as_ref().filter(|m| m.standard_layer) else {
            continue;
        };
        out.push((
            path(image, entry),
            script(entry, module, collection, base_family, &has, root),
        ));
    }
    out
}

fn script(
    entry: &Entry,
    module: &Module,
    collection: &Collection,
    base_family: &str,
    has: &BTreeSet<&str>,
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

    // The repo a module declares is sourced before its packages, so an
    // `enablerepo` names a repository that exists by the time it is enabled.
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

    // COPR is a Fedora build service, so the declaration is only reachable on
    // a Fedora base. The layer is handed the derived names and derives none.
    let coprs: Vec<&Copr> = match base_family {
        "fedora" => module.coprs.iter().collect(),
        _ => Vec::new(),
    };
    if !coprs.is_empty()
        || module
            .packages
            .iter()
            .chain(&module.groups)
            .any(|group| group.family == base_family)
    {
        out.push_str("\nsource /ctx/lib/family.sh\n");
    }
    for copr in coprs {
        let _ = write!(
            out,
            "\nexport {}={}\nenable_copr {}\n",
            copr.env_name(),
            shell(&copr.selector()),
            shell(&copr.name())
        );
    }
    for (helper, declared) in [
        ("install_packages", &module.packages),
        ("install_groups", &module.groups),
    ] {
        for batch in declared.iter().filter(|g| g.family == base_family) {
            let names = batch
                .packages
                .iter()
                .map(|name| shell(name))
                .collect::<Vec<_>>()
                .join(" ");
            let repo = batch
                .enablerepo
                .as_ref()
                .map(|repo| format!("TECT_ENABLE_REPO={} ", shell(repo)))
                .unwrap_or_default();
            let _ = write!(out, "\n{repo}{helper} {names}\n");
        }
    }

    // Where this module's files are read from: its own directory, then the
    // family subtree. Ungated first, so a family adds to what runs everywhere
    // rather than replacing it, and a file it ships lands over the shared one.
    let mut roots = vec![(on_disk.clone(), dir.clone())];
    for gated in layout::family_dirs(&on_disk, base_family) {
        roots.push((on_disk.join(gated), format!("{dir}/{gated}")));
    }

    for (at, ctx) in &roots {
        if at.join("module.sh").is_file() {
            let _ = write!(out, "\nsource {ctx}/module.sh\n");
        }
    }

    // Both directories are taken the way a `packages` batch is: only the kind
    // this image has a MAC for. A module shipping for both is built for
    // whichever the base turned out to carry.
    let policy = match has.contains(layout::SELINUX.capability) {
        true => layout::SELINUX.files(&on_disk),
        false => Vec::new(),
    };
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

    // AppArmor validates and places: the parser only reads, so nothing is
    // copied out of the read-only mount first.
    let profiles = match has.contains(layout::APPARMOR.capability) {
        true => layout::APPARMOR.files(&on_disk),
        false => Vec::new(),
    };
    if !profiles.is_empty() {
        let _ = write!(out, "\nsource /ctx/lib/apparmor-helpers.sh\n");
        for file in profiles {
            let _ = write!(out, "install_apparmor_profile {dir}/apparmor/{file}\n");
        }
    }

    let overlays: Vec<&String> = roots
        .iter()
        .filter(|(at, _)| at.join(layout::OVERLAY).is_dir())
        .map(|(_, ctx)| ctx)
        .collect();
    for ctx in &overlays {
        let _ = write!(out, "\ncp -rT {ctx}/files /\n");
    }
    // A mode is declared once and applies to the path whichever overlay put it
    // there, so the chmods run after the last copy rather than after each.
    if !overlays.is_empty() {
        for declared in &module.modes {
            let _ = writeln!(
                out,
                "chmod '{}' -- {}",
                format_args!("{:04o}", declared.mode),
                shell(&declared.path)
            );
        }
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

#[cfg(test)]
mod tests {
    #[test]
    fn shell_paths_survive_a_single_quote() {
        assert_eq!(super::shell("a'b"), "'a'\"'\"'b'");
    }
}

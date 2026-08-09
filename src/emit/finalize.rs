//! The finalize step: the collected files assembled from their parts, then
//! every module's finalize hook, both named here in the order the resolved
//! plan put them in.

use crate::emit::module_build;
use crate::model::image::{Entry, Image};
use crate::resolve::collect::Collection;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const HEADER: &str = "\
#!/usr/bin/env bash
# GENERATED FILE, do not edit.
set -euxo pipefail
";

/// Where the script for one image is written.
pub fn path(image: &Image) -> PathBuf {
    module_build::dir(image).join("finalize.sh")
}

/// Every module directory whose finalize hook this image runs, in build order
/// and without the repeat a module listed under two flavours would produce.
pub fn hooks<'a>(image: &'a Image, root: &Path) -> Vec<&'a Entry> {
    let mut out: Vec<&Entry> = Vec::new();
    for entry in &image.entries {
        if entry.module.is_none()
            || !root
                .join("modules")
                .join(entry.dir())
                .join("finalize.sh")
                .is_file()
        {
            continue;
        }
        out.push(entry);
    }
    out
}

/// The script, or None when there is nothing to assemble and no hook to run.
pub fn script(image: &Image, collection: &Collection, root: &Path) -> Option<(PathBuf, String)> {
    let hooks = hooks(image, root);
    if collection.assembled.is_empty() && hooks.is_empty() {
        return None;
    }

    let mut out = String::from(HEADER);
    for (dest, parts) in &collection.assembled {
        // A part whose contributor is gated out of this build is not there,
        // which is why the list is filtered rather than passed straight to cat.
        let _ = write!(
            out,
            "\n# ---- {dest} ----\n\
             parts=()\n\
             for part in {}; do\n\
             \x20   [ -f \"$part\" ] || continue\n\
             \x20   parts+=(\"$part\")\n\
             done\n\
             if [ ${{#parts[@]}} -gt 0 ]; then\n\
             \x20   cat \"${{parts[@]}}\" > {dest}\n\
             fi\n\
             rm -rf {dest}.d\n",
            parts.join(" ")
        );
    }

    for entry in hooks {
        let dir = format!("/ctx/modules/{}", entry.dir());
        let mut body = format!("MODDIR={dir}\nexport MODDIR\nsource {dir}/finalize.sh\n");
        if let Some(flavour) = &entry.flavour {
            body = body
                .lines()
                .map(|line| format!("    {line}\n"))
                .collect::<String>();
            body = format!("if [ \"${{FLAVOUR:-}}\" = \"{flavour}\" ]; then\n{body}fi\n");
        }
        let _ = write!(out, "\n# ---- {} ----\n{body}", entry.path);
    }

    Some((path(image), out))
}

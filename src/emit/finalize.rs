//! The finalize step: the collected files assembled from their parts, then
//! every module's finalize hook, both named here in the order the resolved
//! plan put them in.

use crate::layout;
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
    layout::generated_image(&image.id).join("finalize.sh")
}

/// Every module directory whose finalize hook this image runs, in build order
/// and without the repeat a module listed under two flavours would produce.
pub fn hooks<'a>(image: &'a Image, root: &Path) -> Vec<&'a Entry> {
    image
        .entries
        .iter()
        .filter(|entry| entry.module.is_some())
        .filter(|entry| {
            layout::modules(root)
                .join(entry.dir())
                .join("finalize.sh")
                .is_file()
        })
        .collect()
}

/// The script assembling collected files, running hooks, and finalizing the image.
pub fn script(image: &Image, collection: &Collection, root: &Path) -> (PathBuf, String) {
    let hooks = hooks(image, root);
    let mut out = String::from(HEADER);
    for (dest, parts) in &collection.assembled {
        // A part whose contributor is gated out of this build is not there,
        // which is why the list is filtered rather than passed straight to cat.
        let _ = write!(
            out,
            r#"
# ---- {dest} ----
parts=()
for part in {}; do
    [ -f "$part" ] || continue
    parts+=("$part")
done
if [ ${{#parts[@]}} -gt 0 ]; then
    cat "${{parts[@]}}" > {dest}
fi
rm -rf {dest}.d
"#,
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

    // The tail interpolates nothing, so it lives beside this file where
    // `./lint.sh` reads it. Compiled in, never read at runtime: `scripts/tect.sh`
    // unpacks the binary alone.
    out.push('\n');
    out.push_str(include_str!("finalize.sh"));

    (path(image), out)
}

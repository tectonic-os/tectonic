//! The finalize step: the collected files assembled from their parts, then
//! every module's finalize hook, both named here in the order the resolved
//! plan put them in.

use crate::emit::module_build;
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
    module_build::dir(image).join("finalize.sh")
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
pub fn script(image: &Image, collection: &Collection, root: &Path) -> Option<(PathBuf, String)> {
    let hooks = hooks(image, root);
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

    out.push_str(
        "\n# ---- /opt relocation ----\n\
         mkdir -p /usr/lib/opt\n\
         tmpfiles=/usr/lib/tmpfiles.d/zz-opt-symlinks.conf\n\
         printf 'd /var/opt 0755 root root -\\n' > \"$tmpfiles\"\n\
         for d in /opt/*/; do\n\
         \x20   [ -d \"$d\" ] || continue\n\
         \x20   name=\"$(basename \"$d\")\"\n\
         \x20   cp -a \"$d\" \"/usr/lib/opt/${name}\"\n\
         \x20   esc=\"${name// /\\\\x20}\"\n\
         \x20   printf 'L+ /var/opt/%s - - - - /usr/lib/opt/%s\\n' \"$esc\" \"$esc\" >> \"$tmpfiles\"\n\
         done\n\
         rm -rf /opt\n\
         mv /opt.bak /opt\n\
         \n\
         # ---- module presets ----\n\
         apply_module_presets() {\n\
         \x20   local scope=\"$1\" dir=\"$2\" flag=() f verb unit\n\
         \x20   [ \"$scope\" = user ] && flag=(--global)\n\
         \x20   for f in \"$dir\"/45-module-*.preset; do\n\
         \x20       [ -f \"$f\" ] || continue\n\
         \x20       while read -r verb unit; do\n\
         \x20           case \"$verb\" in\n\
         \x20               enable) systemctl \"${flag[@]}\" enable \"$unit\" ;;\n\
         \x20               disable) systemctl \"${flag[@]}\" disable \"$unit\" ;;\n\
         \x20               *) ;;\n\
         \x20           esac\n\
         \x20       done < \"$f\"\n\
         \x20   done\n\
         }\n\
         apply_module_presets system /usr/lib/systemd/system-preset\n\
         apply_module_presets user /usr/lib/systemd/user-preset\n",
    );

    Some((path(image), out))
}

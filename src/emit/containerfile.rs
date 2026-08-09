//! The generated Containerfile: the skeleton the repository keeps, with the
//! build phases and module layers spliced between its markers.

use crate::emit::module_build;
use crate::model::image::{Entry, Image};
use crate::model::module::Module;
use crate::parse::disk::MODULE_SLOT;
use crate::resolve::collect::Collection;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The hand-written half, which a repository owns and this splices into.
pub const SKELETON: &str = "scripts/Containerfile.skeleton";
pub const BEGIN: &str = "# ---- BEGIN GENERATED (build phases and modules) ----";
pub const END: &str = "# ---- END GENERATED ----";

/// Where the assembled Containerfile for one image is written.
pub fn path(image: &Image) -> PathBuf {
    PathBuf::from("generated").join(&image.id)
}

/// The skeleton with `section` between its markers, the syntax directive kept
/// first and the header under it.
pub fn file(skeleton: &str, image: &Image, section: &str) -> String {
    let mut out = String::new();
    let mut lines = skeleton.lines().peekable();
    if lines.peek().is_some_and(|l| l.starts_with("# syntax=")) {
        let _ = writeln!(out, "{}", lines.next().unwrap_or_default());
    }
    let _ = write!(
        out,
        "# GENERATED FILE, do not edit. Produced by `tect generate` from\n\
         # {SKELETON} and the {} image definition.\n\n",
        image.id
    );

    let mut generated = false;
    for line in lines {
        if line == END {
            generated = false;
        }
        if !generated {
            let _ = writeln!(out, "{line}");
        }
        if line == BEGIN {
            let _ = write!(out, "\n{section}");
            generated = true;
        }
    }
    out
}

/// Declared immediately above the first layer that can read it, never at the
/// top.
const FLAVOUR_ARG: &str = "\
# ---- flavour gate ----
ARG FLAVOUR";

/// The binary, so anything running in the image can call the tool.
const TECT_MOUNT: &str = "--mount=type=bind,from=tect,source=/tect,target=/ctx/tect \\\n    ";

/// CI passes the build date, so this changes every day.
const IMAGE_VERSION_ARG: &str = "\
# ---- image version ----
ARG IMAGE_VERSION=dev";

/// What the image calls itself, from its own file, plus the registry the build
/// was pointed at, as the ARGs the phases below the modules read.
fn identity(image: &Image) -> Vec<(&'static str, String)> {
    let mut vars: Vec<(&'static str, String)> = Vec::new();
    for (name, value) in [
        ("IMAGE_ID", &image.id),
        ("IMAGE_NAME", &image.name),
        ("IMAGE_PRETTY_NAME", &image.pretty_name),
        ("IMAGE_URL", &image.url),
        ("IMAGE_ISSUES_URL", &image.issues_url),
    ] {
        if !value.is_empty() {
            vars.push((name, value.clone()));
        }
    }
    vars.push(("IMAGE_REGISTRY", String::new()));
    vars
}

pub fn section(
    image: &Image,
    collection: &Collection,
    phases: &[(u32, String)],
    root: &Path,
) -> String {
    let mut out = String::new();
    let mut flavour_arg_emitted = false;
    let mut finalize: Vec<String> = Vec::new();

    if let Some(base) = &image.base {
        let _ = write!(
            out,
            "### Base Image\n\
             FROM {}\n\n",
            base.image
        );
    }

    let _ = write!(out, "## Build phases and modules\n\n");

    for (_, file) in phases.iter().filter(|(number, _)| *number < MODULE_SLOT) {
        let _ = write!(out, "{}\n\n", phase(file, false, ""));
    }

    for entry in &image.entries {
        // A module that could not be read is already an issue, and there is
        // nothing to generate a layer from.
        let Some(module) = &entry.module else {
            continue;
        };

        if entry.flavour.is_some() && !flavour_arg_emitted {
            let _ = write!(out, "{FLAVOUR_ARG}\n\n");
            flavour_arg_emitted = true;
        }

        let mut blocks: Vec<String> = Vec::new();
        if let Some(body) = module.fragment.as_ref().filter(|_| !module.fragment_after) {
            blocks.push(fragment(entry, body));
        }
        if module.standard_layer {
            blocks.push(standard(entry, module, &module_build::path(image, entry)));
        }
        if let Some(body) = module.fragment.as_ref().filter(|_| module.fragment_after) {
            blocks.push(fragment(entry, body));
        }

        if let Some(flavour) = &entry.flavour {
            let _ = writeln!(out, "# ---- [{flavour}] ----");
        }
        let _ = writeln!(out, "# ---- {} ----", entry.path);
        let _ = write!(out, "{}\n\n", blocks.join("\n\n"));

        let dir = root.join("modules").join(entry.dir());
        if dir.join("finalize.sh").is_file() {
            finalize.push(match &entry.flavour {
                Some(f) => format!("{}:{f}", entry.dir()),
                None => entry.dir(),
            });
        }
    }

    if !flavour_arg_emitted {
        let _ = write!(out, "{FLAVOUR_ARG}\n\n");
    }

    let _ = write!(
        out,
        "# ---- finalize hook order ----\n\
         ARG FINALIZE_ORDER=\"{}\"\n\n",
        finalize.join(" ")
    );

    let _ = write!(
        out,
        "# ---- collected file destinations ----\n\
         ARG COLLECT_TARGETS=\"{}\"\n\n",
        collection.destinations.join(" ")
    );

    let _ = write!(out, "{IMAGE_VERSION_ARG}\n\n");

    let identity = identity(image);
    let _ = writeln!(out, "# ---- image identity ----");
    for (name, value) in &identity {
        let _ = writeln!(out, "ARG {name}=\"{value}\"");
    }
    out.push('\n');

    let identity_env: String = identity
        .iter()
        .map(|(name, _)| format!("{name}=\"${{{name}}}\" "))
        .collect();

    for (_, file) in phases.iter().filter(|(number, _)| *number >= MODULE_SLOT) {
        let _ = write!(out, "{}\n\n", phase(file, true, &identity_env));
    }

    out
}

/// One phase layer.
fn phase(file: &str, below_modules: bool, identity_env: &str) -> String {
    let mut out = format!(
        "# ---- phase {file} ----\n\
         RUN --mount=type=bind,from=ctx,source=/{file},target=/ctx/{file} \\\n    "
    );
    if below_modules {
        out.push_str("--mount=type=bind,from=ctx,source=/lib,target=/ctx/lib \\\n    ");
        out.push_str("--mount=type=bind,from=ctx,source=/modules,target=/ctx/modules \\\n    ");
    }
    out.push_str(TECT_MOUNT);
    out.push_str(
        "--mount=type=cache,target=/var/cache \\\n    \
         --mount=type=cache,target=/var/log \\\n    \
         --mount=type=tmpfs,target=/tmp \\\n    ",
    );
    if below_modules {
        out.push_str(
            "FLAVOUR=${FLAVOUR} IMAGE_VERSION=${IMAGE_VERSION} FINALIZE_ORDER=\"${FINALIZE_ORDER}\" \\\n    ",
        );
        out.push_str("COLLECT_TARGETS=\"${COLLECT_TARGETS}\" \\\n    ");
        out.push_str(identity_env);
    }
    let _ = write!(out, "/ctx/{file}");
    out
}

/// The layer is the mounts and the build inputs only a Containerfile can
/// name; everything the host resolved is in the script it runs.
fn standard(entry: &Entry, module: &Module, script: &Path) -> String {
    let mut env = String::new();

    let mut secrets = String::new();
    for decl in &module.secrets {
        let id = &decl.name;
        let _ = write!(
            secrets,
            "--mount=type=secret,id={id},target=/run/secrets/{id},required=false \\\n    "
        );
    }
    for decl in module.args.iter().rev() {
        let name = &decl.name;
        env.insert_str(0, &format!("{name}=${{{name}}} "));
    }

    let path = entry.dir();
    let script = script.display();
    let mut out = String::new();
    let _ = write!(
        out,
        "RUN --mount=type=bind,from=ctx,source=/modules/{path},target=/ctx/modules/{path} \\\n    \
         --mount=type=bind,from=ctx,source=/lib,target=/ctx/lib \\\n    \
         --mount=type=bind,from=ctx,source=/{script},target=/ctx/module.sh \\\n    \
         {TECT_MOUNT}\
         --mount=type=cache,target=/var/cache \\\n    \
         --mount=type=cache,target=/var/log \\\n    \
         --mount=type=tmpfs,target=/tmp \\\n    \
         {secrets}{env}bash /ctx/module.sh"
    );
    out
}

/// A module whose needs the field sets cannot express ships a fragment,
/// inlined verbatim above the standard block, or below it when the manifest
/// says `position "after"`.
fn fragment(entry: &Entry, body: &str) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "# verbatim from modules/{}/Containerfile.inc:\n{}",
        entry.dir(),
        body.trim_end_matches('\n')
    );
    out
}

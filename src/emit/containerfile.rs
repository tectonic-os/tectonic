//! The generated Containerfile: the skeleton the repository keeps, with the
//! module layers and the finalize layer spliced between its markers.

use crate::emit::{finalize, module_build};
use crate::layout;
use crate::model::image::{Entry, Image};
use crate::model::module::Module;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The hand-written half, which a repository owns and this splices into.
pub const SKELETON: &str = "scripts/Containerfile.skeleton";
pub const BEGIN: &str = "# ---- BEGIN GENERATED (module layers) ----";
pub const END: &str = "# ---- END GENERATED ----";

/// Where the assembled Containerfile for one image is written.
pub fn path(image: &Image) -> PathBuf {
    layout::generated_image(&image.id).join(layout::CONTAINERFILE)
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

/// The base's family, for the layers that cannot derive it from the filesystem:
/// what a package is queried with differs per family and nothing in the image
/// says which one built it.
const FAMILY_ARG: &str = "\
# ---- base family ----
ARG FAMILY";

/// What a fragment writes where a mount writes `source=/modules/<dir>`. It is
/// inlined verbatim, so it cannot name its own directory, and `${...}` would be
/// read as a build arg.
const MODULE_DIR: &str = "@MODULE@";

/// The binary, so anything running in the image can call the tool.
const TECT_MOUNT: &str = "--mount=type=bind,from=tect,source=/tect,target=/ctx/tect \\\n    ";

/// CI passes the build date, so this changes every day.
const IMAGE_VERSION_ARG: &str = "\
# ---- image version ----
ARG IMAGE_VERSION=dev";

/// What the image calls itself, from its own file, plus the registry the build
/// was pointed at, as the ARGs the layers below the modules read.
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

pub fn section(image: &Image, root: &Path) -> String {
    let mut out = String::new();
    let mut flavour_arg_emitted = false;

    if image
        .entries
        .iter()
        .filter_map(|entry| entry.module.as_ref())
        .any(|module| !module.keys.is_empty())
    {
        let _ = write!(
            out,
            "### Public keys\nFROM ctx AS public-keys\nCOPY keys/public/ /public/\n\n"
        );
    }

    // The declared tag is in plan.json; what the build actually resolved it to
    // is what `tect build` passes down, and what the build record keeps.
    if image.base.is_some() {
        let _ = write!(out, "### Base Image\nFROM ${{BASE}}\n\n");
    }

    let _ = write!(out, "## Module layers\n\n");
    let _ = write!(
        out,
        "# ---- /opt build root ----\nRUN mv /opt /opt.bak && mkdir /opt\n\n"
    );

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
            blocks.push(standard(
                entry,
                module,
                image,
                &module_build::path(image, entry),
            ));
        }
        if let Some(body) = module.fragment.as_ref().filter(|_| module.fragment_after) {
            blocks.push(fragment(entry, body));
        }

        if let Some(flavour) = &entry.flavour {
            let _ = writeln!(out, "# ---- [{flavour}] ----");
        }
        let _ = writeln!(out, "# ---- {} ----", entry.path);
        let _ = write!(out, "{}\n\n", blocks.join("\n\n"));
    }

    if !flavour_arg_emitted {
        let _ = write!(out, "{FLAVOUR_ARG}\n\n");
    }

    let _ = write!(out, "{IMAGE_VERSION_ARG}\n\n");
    let _ = write!(out, "{FAMILY_ARG}\n\n");

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

    let _ = write!(
        out,
        "# ---- os-release ----\n\
         RUN {TECT_MOUNT}IMAGE_VERSION=\"${{IMAGE_VERSION}}\" {identity_env}/ctx/tect os-release\n\n"
    );

    let _ = write!(out, "{}\n\n", finalize_layer(image, &identity_env, root));

    out
}

/// The one layer below the modules: the collected files assembled, then every
/// finalize hook, with only the module directories those hooks read mounted.
fn finalize_layer(image: &Image, identity_env: &str, root: &Path) -> String {
    let hooks = finalize::hooks(image, root);
    let script = finalize::path(image).display().to_string();
    let mut out = format!(
        "# ---- finalize ----\n\
         RUN --mount=type=bind,from=ctx,source=/{script},target=/ctx/finalize.sh \\\n    \
         --mount=type=bind,from=ctx,source=/generated/lib,target=/ctx/lib \\\n    "
    );

    let mut mounted: Vec<String> = Vec::new();
    for entry in hooks {
        let dir = entry.dir();
        if mounted.contains(&dir) {
            continue;
        }
        let _ = write!(
            out,
            "--mount=type=bind,from=ctx,source=/modules/{dir},target=/ctx/modules/{dir} \\\n    "
        );
        mounted.push(dir);
    }

    out.push_str(TECT_MOUNT);
    out.push_str(
        "--mount=type=cache,target=/var/cache \\\n    \
         --mount=type=cache,target=/var/log \\\n    \
         --mount=type=tmpfs,target=/tmp \\\n    \
         FLAVOUR=${FLAVOUR} IMAGE_VERSION=\"${IMAGE_VERSION}\" \\\n    ",
    );
    out.push_str(identity_env);
    out.push_str("bash /ctx/finalize.sh");
    out
}

/// The layer is the mounts and the build inputs only a Containerfile can
/// name; everything the host resolved is in the script it runs.
fn standard(entry: &Entry, module: &Module, image: &Image, script: &Path) -> String {
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
    let keys = match module.keys.is_empty() {
        true => String::new(),
        false => "--mount=type=bind,from=public-keys,source=/public,target=/ctx/keys \\\n    "
            .to_string(),
    };
    let mut out = String::new();
    let helpers: String = image
        .modules()
        .flat_map(|owner| owner.helpers.iter().map(move |helper| (owner, helper)))
        .map(|(owner, helper)| {
            let name = Path::new(&helper.name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            format!(
                "--mount=type=bind,from=ctx,source=/modules/{}/{},target=/ctx/lib/{name} \\\n    ",
                owner.dir, helper.name
            )
        })
        .collect();
    let rw = if helpers.is_empty() { "" } else { ",rw" };
    let _ = write!(
        out,
        "RUN --mount=type=bind,from=ctx,source=/modules/{path},target=/ctx/modules/{path} \\\n    \
         --mount=type=bind,from=ctx,source=/generated/lib,target=/ctx/lib{rw} \\\n    \
         {helpers}\
         --mount=type=bind,from=ctx,source=/{script},target=/ctx/module.sh \\\n    \
         {keys}{TECT_MOUNT}\
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
    let dir = entry.dir();
    let mut out = String::new();
    let _ = write!(
        out,
        "# verbatim from modules/{dir}/Containerfile.inc:\n{}",
        body.trim_end_matches('\n')
            .replace(MODULE_DIR, &format!("/modules/{dir}"))
    );
    out
}

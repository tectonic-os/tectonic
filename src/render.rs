//! The generated Containerfile section.

use crate::diag::{Issue, Issues};
use crate::list::{Entry, Image};
use crate::module::{Collected, Collection, Module};
use std::fmt::Write as _;
use std::path::Path;

/// Declared immediately above the first layer that can read it, never at the
/// top.
const FLAVOUR_ARG: &str = "\
# ---- flavour gate ----
ARG FLAVOUR";

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

/// Where the module layers sit among the build phases.
const MODULE_SLOT: u32 = 50;

pub fn section(
    image: &Image,
    collection: &Collection,
    root: &Path,
    issues: &mut Issues,
) -> String {
    let mut out = String::new();
    let mut flavour_arg_emitted = false;
    let mut finalize: Vec<String> = Vec::new();

    let base_family = image.base.as_ref().map_or("", |b| b.family.as_str());

    if let Some(base) = &image.base {
        let _ = write!(
            out,
            "### Base Image\n\
             FROM {}\n\n",
            base.image
        );
    }

    let _ = write!(
        out,
        "## Build phases and modules\n\n"
    );

    let phases = phases(root, issues);
    for (_, file) in phases.iter().filter(|(number, _)| *number < MODULE_SLOT) {
        let _ = write!(out, "{}\n\n", phase(file, false, ""));
    }

    for entry in &image.entries {
        let dir = root.join("modules").join(entry.dir());
        if !dir.is_dir() {
            issues.push(
                Issue::new(
                    format!("`{}` does not resolve to a module directory", entry.path),
                    &image.file,
                    &image.text,
                )
                .at(entry.span, "no such module")
                .help(format!("expected {}", dir.display())),
            );
            continue;
        }

        if entry.flavour.is_some() && !flavour_arg_emitted {
            let _ = write!(out, "{FLAVOUR_ARG}\n\n");
            flavour_arg_emitted = true;
        }

        let module = entry.module.as_ref();

        let inc = dir.join("Containerfile.inc");
        let mut blocks: Vec<String> = Vec::new();
        let fragment_after = module.is_some_and(|m| m.fragment_after);
        if inc.is_file() && !fragment_after {
            blocks.push(fragment(entry, &inc, flavour_arg_emitted, image, issues));
        }
        if module.is_none_or(|m| m.standard_layer) {
            blocks.push(standard(
                entry,
                module,
                collection.by_module.get(&entry.path),
                base_family,
            ));
        }
        if inc.is_file() && fragment_after {
            blocks.push(fragment(entry, &inc, flavour_arg_emitted, image, issues));
        }

        if let Some(flavour) = &entry.flavour {
            let _ = writeln!(out, "# ---- [{flavour}] ----");
        }
        let _ = writeln!(out, "# ---- {} ----", entry.path);
        let _ = write!(out, "{}\n\n", blocks.join("\n\n"));

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
    let _ = write!(
        out,
        "# ---- image identity ----\n",
    );
    for (name, value) in &identity {
        let _ = write!(out, "ARG {name}=\"{value}\"\n");
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

/// Every build-phases/*.sh, as its number and filename, in build order.
fn phases(root: &Path, issues: &mut Issues) -> Vec<(u32, String)> {
    let dir = root.join("build-phases");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut out: Vec<(u32, String)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".sh") || !entry.path().is_file() {
            continue;
        }
        let number = name
            .split_once('-')
            .and_then(|(prefix, _)| prefix.parse::<u32>().ok());
        match number {
            Some(number) => out.push((number, name)),
            None => {
                let file = dir.join(&name).display().to_string();
                issues.push(
                    Issue::new(format!("`{name}` has no phase number"), &file, "")
                        .help(format!(
                            "name it <number>-{name}: below {MODULE_SLOT} to run before the module layers, {MODULE_SLOT} or above to run after"
                        )),
                );
            }
        }
    }
    out.sort();
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

fn standard(
    entry: &Entry,
    module: Option<&Module>,
    collected: Option<&Vec<Collected>>,
    base_family: &str,
) -> String {
    let mut env = String::new();
    if let Some(flavour) = &entry.flavour {
        let _ = write!(env, "FLAVOUR_GATE={flavour} ");
    }
    for (name, value) in module.map(|m| m.resolved.as_slice()).unwrap_or_default() {
        let _ = write!(env, "{name}=\"{value}\" ");
    }
    if let Some(collected) = collected.filter(|c| !c.is_empty()) {
        let pairs: Vec<String> = collected
            .iter()
            .map(|c| format!("{}={}", c.file, c.staged))
            .collect();
        let _ = write!(env, "MODULE_COLLECT=\"{}\" ", pairs.join(" "));
    }

    let mut assets = String::new();
    for asset in module.map(|m| m.assets.as_slice()).unwrap_or_default() {
        for (name, value) in asset.env() {
            let _ = write!(assets, "{name}=\"{value}\" \\\n    ");
        }
    }

    let mut secrets = String::new();
    for decl in module.map(|m| m.secrets.as_slice()).unwrap_or_default() {
        let id = &decl.name;
        let _ = write!(
            secrets,
            "--mount=type=secret,id={id},target=/run/secrets/{id},required=false \\\n    "
        );
    }
    for decl in module
        .map(|m| m.args.as_slice())
        .unwrap_or_default()
        .iter()
        .rev()
    {
        let name = &decl.name;
        env.insert_str(0, &format!("{name}=${{{name}}} "));
    }

    let packages_cmd = packages_install(module, base_family);

    let path = entry.dir();
    let mut out = String::new();
    let _ = write!(
        out,
        "RUN --mount=type=bind,from=ctx,source=/modules/{path},target=/ctx/modules/{path} \\\n    \
         --mount=type=bind,from=ctx,source=/lib,target=/ctx/lib \\\n    \
         --mount=type=cache,target=/var/cache \\\n    \
         --mount=type=cache,target=/var/log \\\n    \
         --mount=type=tmpfs,target=/tmp \\\n    \
         {secrets}{packages_cmd}{assets}{env}bash /ctx/lib/run-module.sh /ctx/modules/{path}"
    );
    out
}

/// The install commands for declared packages, if any.
fn packages_install(module: Option<&Module>, base_family: &str) -> String {
    let groups = match module {
        Some(m) if !m.packages.is_empty() => m.packages.as_slice(),
        _ => return String::new(),
    };
    let mut out = String::new();
    for group in groups.iter().filter(|g| g.family == base_family) {
        let pkgs = group.packages.join(" ");
        match &group.enablerepo {
            Some(repo) => {
                let _ = write!(
                    out,
                    "dnf5 install -y --enablerepo='{repo}' {pkgs} && \\\n    "
                );
            }
            None => {
                let _ = write!(out, "dnf5 install -y {pkgs} && \\\n    ");
            }
        }
    }
    out
}

/// A module whose needs the field sets cannot express ships a fragment,
/// inlined verbatim above the standard block, or below it when the manifest
/// says `position "after"`.
fn fragment(
    entry: &Entry,
    inc: &Path,
    flavour_arg_emitted: bool,
    image: &Image,
    issues: &mut Issues,
) -> String {
    let body = std::fs::read_to_string(inc).unwrap_or_default();
    let path = &entry.path;

    if !flavour_arg_emitted && (body.contains("${FLAVOUR}") || body.contains("$FLAVOUR")) {
        issues.push(
            Issue::new(
                format!("`{path}` expands FLAVOUR above the flavour gate"),
                &image.file,
                &image.text,
            )
            .at(entry.span, "listed above the first flavour-gated module")
            .help("ARG FLAVOUR is declared directly above the first gated entry, so a fragment before it would expand to an empty string"),
        );
    }

    let runs = body.lines().any(|l| l.trim_start().starts_with("RUN "));
    if let Some(flavour) = entry.flavour.as_ref().filter(|_| runs) {
        let declared = body
            .split("FLAVOUR_GATE=")
            .nth(1)
            .map(|rest| rest.split_whitespace().next().unwrap_or_default());
        match declared {
            Some(d) if d == flavour => {}
            Some(d) => issues.push(
                Issue::new(
                    format!("`{path}` is listed under `{flavour}` but its fragment gates on `{d}`"),
                    &image.file,
                    &image.text,
                )
                .at(entry.span, "listed here"),
            ),
            None => issues.push(
                Issue::new(
                    format!(
                        "`{path}` is listed under `{flavour}` but its fragment sets no FLAVOUR_GATE"
                    ),
                    &image.file,
                    &image.text,
                )
                .at(entry.span, "the flavour gate would be silently ignored")
                .help(
                    "a fragment is emitted unconditionally, so anything it runs has to carry the gate itself",
                ),
            ),
        }
    }

    let mut out = String::new();
    let _ = write!(
        out,
        "# verbatim from modules/{}/Containerfile.inc:\n{}",
        entry.dir(),
        body.trim_end_matches('\n')
    );
    out
}

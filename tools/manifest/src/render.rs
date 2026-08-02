//! The generated Containerfile section.

use crate::diag::{Issue, Issues};
use crate::list::{Entry, Image, List, NO_FLAVOUR};
use crate::module::Module;
use std::collections::BTreeMap;
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
        ("IMAGE_LOGO", &image.logo),
        ("IMAGE_WATERMARK", &image.watermark),
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
    modules: &[Module],
    collected: &BTreeMap<String, Vec<(String, String)>>,
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

        let module = modules
            .iter()
            .find(|m| m.path == entry.path && m.flavour == entry.flavour);

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
                collected.get(&entry.path),
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

    let _ = write!(out, "{IMAGE_VERSION_ARG}\n\n");

    for path in [&image.logo, &image.watermark] {
        if !path.is_empty() && !root.join(path).is_file() {
            issues.push(
                Issue::new(format!("`{path}` does not exist"), &image.file, &image.text)
                    .at(image.span, "declared brand asset")
                    .help("the path is relative to the repository root"),
            );
        }
    }

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
        out.push_str("--mount=type=bind,from=ctx,source=/brand,target=/ctx/brand \\\n    ");
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
        out.push_str(identity_env);
    }
    let _ = write!(out, "/ctx/{file}");
    out
}

/// What a target is made of, as markdown, in the order the layers build.
pub fn summary(image: &Image, modules: &[Module], target: Option<&str>) -> String {
    let included: Vec<&Entry> = image.entries.iter().filter(|e| in_target(e, target)).collect();
    let gated = included.iter().filter(|e| e.flavour.is_some()).count();

    let mut out = String::new();
    let count = included.len();
    let _ = match target {
        Some(NO_FLAVOUR) => writeln!(out, "{count} modules, the ungated set."),
        Some(target) => writeln!(out, "{count} modules, {gated} of them gated to `{target}`."),
        None => writeln!(out, "{count} modules, {gated} of them gated to a flavour."),
    };
    let _ = write!(
        out,
        "\n| Module | Description | Options |\n| --- | --- | --- |\n"
    );

    for entry in included {
        let module = modules
            .iter()
            .find(|m| m.path == entry.path && m.flavour == entry.flavour);
        let mut name = format!("`{}`", entry.path);
        if let Some(flavour) = &entry.flavour {
            let _ = write!(name, " `[{flavour}]`");
        }
        if let Some(variant) = &entry.variant {
            let _ = write!(name, " `variant={variant}`");
        }
        if let Some(remote) = &entry.remote {
            let _ = write!(name, " `remote={}`", remote.git_ref);
        }
        let options: Vec<String> = module
            .map(|m| m.resolved.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|(name, value)| format!("`{name}=\"{}\"`", cell(value)))
            .collect();
        let _ = writeln!(
            out,
            "| {name} | {} | {} |",
            cell(module.map(|m| m.description.as_str()).unwrap_or_default()),
            options.join(" ")
        );
    }
    out
}

/// A pipe would end the column, and neither a description nor an option value
/// is stopped from holding one.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// Whether an entry lands in a target's image.
fn in_target(entry: &Entry, target: Option<&str>) -> bool {
    match (&entry.flavour, target) {
        (None, _) => true,
        (Some(_), None) => true,
        (Some(gate), Some(target)) => gate == target,
    }
}

/// Every pinned asset, pipe separated, one per line:
/// <module>|<name>|<manifest>|<version>|<sha256>|<from>|<url> Two consumers,
/// neither of which should be carrying a table of its own: the checksum
/// workflow, which recomputes a stale hash and needs the manifest to rewrite,
/// and the SBOM supplement, which needs the payloads an RPM inventory cannot
/// see.
pub fn assets(image: &Image, modules: &[Module], target: Option<&str>) -> String {
    let mut out = String::new();
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for entry in image.entries.iter().filter(|e| in_target(e, target)) {
        let Some(module) = modules
            .iter()
            .find(|m| m.path == entry.path && m.flavour == entry.flavour)
        else {
            continue;
        };
        for asset in &module.assets {
            if seen.contains(&(module.path.as_str(), asset.name.as_str())) {
                continue;
            }
            seen.push((module.path.as_str(), asset.name.as_str()));
            let _ = writeln!(
                out,
                "{}|{}|modules/{}/module.kdl|{}|{}|{}|{}",
                module.path,
                asset.name,
                module.dir,
                asset.version.as_deref().unwrap_or_default(),
                asset.sha256.as_deref().unwrap_or_default(),
                asset.from.as_str(),
                asset.url_resolved().unwrap_or_default(),
            );
        }
    }
    out
}

/// Every out-of-tree pin, pipe separated, one per line:
/// <name>|<dir>|<ref>|<sha256>|<url>|<subtree path>|<file> Two consumers: the
/// fetch, which needs somewhere to put the archive it verifies, and the
/// checksum workflow, which recomputes a hash a bumped ref made stale.
pub fn remotes(list: &List) -> String {
    let mut out = String::new();
    let mut seen: Vec<&str> = Vec::new();
    for image in &list.images {
        for entry in &image.entries {
            let Some(remote) = &entry.remote else {
                continue;
            };
            if seen.contains(&entry.path.as_str()) {
                continue;
            }
            seen.push(&entry.path);
            let _ = writeln!(
                out,
                "{}|modules/{}|{}|{}|{}|{}|{}",
                entry.path,
                entry.dir(),
                remote.git_ref,
                remote.sha256,
                remote.url_resolved(),
                remote.path.clone().unwrap_or_default(),
                image.file,
            );
        }
    }
    out
}

/// The module that provides a contract file path.
pub fn find_provider(
    image: &Image,
    modules: &[Module],
    file_path: &str,
    target: Option<&str>,
) -> String {
    for entry in image.entries.iter().filter(|e| in_target(e, target)) {
        let Some(module) = modules
            .iter()
            .find(|m| m.path == entry.path && m.flavour == entry.flavour)
        else {
            continue;
        };
        if module.provides_files.iter().any(|d| d.name == file_path) {
            return format!("{}\n", module.path);
        }
    }
    String::new()
}

/// Unique secret IDs the enabled modules declare, one per line.
pub fn secrets(image: &Image, modules: &[Module], target: Option<&str>) -> String {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = String::new();
    for entry in image.entries.iter().filter(|e| in_target(e, target)) {
        let Some(module) = modules
            .iter()
            .find(|m| m.path == entry.path && m.flavour == entry.flavour)
        else {
            continue;
        };
        for decl in &module.secrets {
            if seen.contains(&decl.name.as_str()) {
                continue;
            }
            seen.push(&decl.name);
            let _ = writeln!(out, "{}", decl.name);
        }
    }
    out
}

/// Contract file paths the finished image still carries, one per line: what
/// the base image guarantees, then what the enabled modules declare.
pub fn contract_files(image: &Image, modules: &[Module], target: Option<&str>) -> String {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = String::new();
    for decl in image.base.iter().flat_map(|b| b.provides_files.iter()) {
        if seen.contains(&decl.name.as_str()) {
            continue;
        }
        seen.push(&decl.name);
        let _ = writeln!(out, "{}", decl.name);
    }
    for entry in image.entries.iter().filter(|e| in_target(e, target)) {
        let Some(module) = modules
            .iter()
            .find(|m| m.path == entry.path && m.flavour == entry.flavour)
        else {
            continue;
        };
        for decl in &module.provides_files {
            if module.provides_files_build_only.contains(&decl.name) {
                continue;
            }
            if seen.contains(&decl.name.as_str()) {
                continue;
            }
            seen.push(&decl.name);
            let _ = writeln!(out, "{}", decl.name);
        }
    }
    out
}

/// Every verify diagnostic the enabled modules accept, unique, one per line,
/// pipe separated: <class>|<unit> Resolved per target for the same reason
/// `contract-files` is: an exception belongs to the module that ships the
/// unit, so an image without that module must not carry it.
pub fn verify_exceptions(image: &Image, modules: &[Module], target: Option<&str>) -> String {
    let mut seen: Vec<(&str, &str)> = Vec::new();
    let mut out = String::new();
    for entry in image.entries.iter().filter(|e| in_target(e, target)) {
        let Some(module) = modules
            .iter()
            .find(|m| m.path == entry.path && m.flavour == entry.flavour)
        else {
            continue;
        };
        for exception in &module.verify_exceptions {
            if seen.contains(&(exception.class.as_str(), exception.unit.as_str())) {
                continue;
            }
            seen.push((exception.class.as_str(), exception.unit.as_str()));
            let _ = writeln!(out, "{}|{}", exception.class, exception.unit);
        }
    }
    out
}

fn standard(
    entry: &Entry,
    module: Option<&Module>,
    collected: Option<&Vec<(String, String)>>,
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
            .map(|(file, into)| format!("{file}={into}"))
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

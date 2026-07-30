//! The generated Containerfile section.

use crate::diag::{Issue, Issues};
use crate::list::{Entry, List, NO_FLAVOUR};
use crate::module::Module;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// Declared immediately above the first layer that can read it, never at the
/// top.
const FLAVOUR_ARG: &str = "\
# ---- flavour gate ----
ARG FLAVOUR";

pub fn section(
    list: &List,
    modules: &[Module],
    collected: &BTreeMap<String, Vec<(String, String)>>,
    root: &Path,
    issues: &mut Issues,
) -> String {
    let mut out = String::new();
    let mut flavour_arg_emitted = false;
    let mut finalize: Vec<String> = Vec::new();

    for entry in &list.entries {
        let dir = root.join("modules").join(&entry.path);
        if !dir.is_dir() {
            issues.push(
                Issue::new(
                    format!("`{}` does not resolve to a module directory", entry.path),
                    &list.file,
                    &list.text,
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
            blocks.push(fragment(entry, &inc, flavour_arg_emitted, list, issues));
        }
        if module.is_none_or(|m| m.standard_layer) {
            blocks.push(standard(entry, module, collected.get(&entry.path)));
        }
        if inc.is_file() && fragment_after {
            blocks.push(fragment(entry, &inc, flavour_arg_emitted, list, issues));
        }

        if let Some(flavour) = &entry.flavour {
            let _ = writeln!(out, "# ---- [{flavour}] ----");
        }
        let _ = write!(out, "{}\n\n", blocks.join("\n\n"));

        if dir.join("finalize.sh").is_file() {
            finalize.push(match &entry.flavour {
                Some(f) => format!("{}:{f}", entry.path),
                None => entry.path.clone(),
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

    out
}

/// What a target is made of, as markdown, in the order the layers build.
pub fn summary(list: &List, modules: &[Module], target: Option<&str>) -> String {
    let included: Vec<&Entry> = list
        .entries
        .iter()
        .filter(|e| match (&e.flavour, target) {
            (None, _) => true,
            (Some(_), None) => true,
            (Some(gate), Some(target)) => gate == target,
        })
        .collect();
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

fn standard(
    entry: &Entry,
    module: Option<&Module>,
    collected: Option<&Vec<(String, String)>>,
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

    let path = &entry.path;
    let mut out = String::new();
    let _ = write!(
        out,
        "# ---- {path} ----\n\
         RUN --mount=type=bind,from=ctx,source=/modules/{path},target=/ctx/modules/{path} \\\n    \
         --mount=type=bind,from=ctx,source=/lib,target=/ctx/lib \\\n    \
         --mount=type=cache,target=/var/cache \\\n    \
         --mount=type=cache,target=/var/log \\\n    \
         --mount=type=tmpfs,target=/tmp \\\n    \
         {secrets}{env}bash /ctx/lib/run-module.sh /ctx/modules/{path}"
    );
    out
}

/// A module whose needs the field sets cannot express ships a fragment,
/// inlined verbatim above the standard block, or below it when the manifest
/// says `position "after"`.
fn fragment(
    entry: &Entry,
    inc: &Path,
    flavour_arg_emitted: bool,
    list: &List,
    issues: &mut Issues,
) -> String {
    let body = std::fs::read_to_string(inc).unwrap_or_default();
    let path = &entry.path;

    if !flavour_arg_emitted && (body.contains("${FLAVOUR}") || body.contains("$FLAVOUR")) {
        issues.push(
            Issue::new(
                format!("`{path}` expands FLAVOUR above the flavour gate"),
                &list.file,
                &list.text,
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
                    &list.file,
                    &list.text,
                )
                .at(entry.span, "listed here"),
            ),
            None => issues.push(
                Issue::new(
                    format!(
                        "`{path}` is listed under `{flavour}` but its fragment sets no FLAVOUR_GATE"
                    ),
                    &list.file,
                    &list.text,
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
        "# ---- {path} (verbatim from modules/{path}/Containerfile.inc) ----\n{}",
        body.trim_end_matches('\n')
    );
    out
}

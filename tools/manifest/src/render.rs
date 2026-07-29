//! The generated Containerfile section.

use crate::diag::{Issue, Issues};
use crate::list::{Entry, List};
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
    sinks: &BTreeMap<String, Vec<(String, String)>>,
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
        let block = if inc.is_file() {
            verbatim(entry, &inc, flavour_arg_emitted, list, issues)
        } else {
            standard(entry, module, sinks.get(&entry.path))
        };
        let _ = write!(out, "{block}\n\n");

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

fn standard(
    entry: &Entry,
    module: Option<&Module>,
    sinks: Option<&Vec<(String, String)>>,
) -> String {
    let mut env = String::new();
    if let Some(flavour) = &entry.flavour {
        let _ = write!(env, "FLAVOUR_GATE={flavour} ");
    }
    for (name, value) in module.map(|m| m.resolved.as_slice()).unwrap_or_default() {
        let _ = write!(env, "{name}=\"{value}\" ");
    }
    if let Some(sinks) = sinks.filter(|s| !s.is_empty()) {
        let pairs: Vec<String> = sinks
            .iter()
            .map(|(file, path)| format!("{file}={path}"))
            .collect();
        let _ = write!(env, "MODULE_SINKS=\"{}\" ", pairs.join(" "));
    }

    let path = &entry.path;
    let mut out = String::new();
    if let Some(flavour) = &entry.flavour {
        let _ = writeln!(out, "# ---- [{flavour}] ----");
    }
    let _ = write!(
        out,
        "# ---- {path} ----\n\
         RUN --mount=type=bind,from=ctx,source=/modules/{path},target=/ctx/modules/{path} \\\n    \
         --mount=type=bind,from=ctx,source=/lib,target=/ctx/lib \\\n    \
         --mount=type=cache,target=/var/cache \\\n    \
         --mount=type=cache,target=/var/log \\\n    \
         --mount=type=tmpfs,target=/tmp \\\n    \
         {env}bash /ctx/lib/run-module.sh /ctx/modules/{path}"
    );
    out
}

/// A module whose needs the field sets cannot express ships a fragment, which
/// replaces the standard block rather than adding to it.
fn verbatim(
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

    if let Some(flavour) = &entry.flavour {
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
                    "a fragment replaces the generated block, so it has to carry the gate itself",
                ),
            ),
        }
    }

    let mut out = String::new();
    if let Some(flavour) = &entry.flavour {
        let _ = writeln!(out, "# ---- [{flavour}] ----");
    }
    let _ = write!(
        out,
        "# ---- {path} (verbatim from modules/{path}/Containerfile.inc) ----\n{}",
        body.trim_end_matches('\n')
    );
    out
}

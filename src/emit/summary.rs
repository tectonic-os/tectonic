//! What one target is made of, as the markdown a build summary shows.

use crate::emit::json::{field, items, strings, text, Json};
use crate::emit::plan::of_target;
use crate::model::image::List;
use std::fmt::Write as _;

/// None when nothing publishes under that name.
pub fn render(list: &List, target: &str) -> Option<String> {
    let (_, flavour, entries) = of_target(list, target)?;

    let mut out = match &flavour {
        None => format!("{} modules, the ungated set.\n", entries.len()),
        Some(flavour) => format!(
            "{} modules, {} of them gated to `{flavour}`.\n",
            entries.len(),
            entries.iter().filter(|e| e.flavour.is_some()).count()
        ),
    };
    out.push_str("\n| Module | Description | Options | Satisfies |\n| --- | --- | --- | --- |\n");

    for entry in entries {
        let module = entry.module.as_ref();
        let _ = write!(out, "| `{}`", entry.path);
        if let Some(flavour) = &entry.flavour {
            let _ = write!(out, " `[{flavour}]`");
        }
        if let Some(variant) = &entry.variant {
            let _ = write!(out, " `variant={variant}`");
        }
        if let Some(remote) = entry.pin(&list.sources) {
            let _ = write!(
                out,
                " `remote={}`",
                remote.version.clone().unwrap_or_default()
            );
        }
        let _ = write!(
            out,
            " | {}",
            cell(module.map(|m| m.description.as_str()).unwrap_or_default())
        );
        let options: Vec<String> = module
            .map(|m| m.resolved.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|(name, value)| format!("`{name}=\"{}\"`", cell(value)))
            .collect();
        let satisfies: Vec<String> = module
            .map(|m| m.satisfies.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|coverage| {
                format!(
                    "`{}: {}`",
                    cell(&coverage.benchmark),
                    coverage
                        .rules
                        .iter()
                        .map(|rule| cell(rule))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        let _ = writeln!(out, " | {} | {} |", options.join(" "), satisfies.join(" "));
    }
    Some(out)
}

/// The same table with no repository at all, off one target of the manifest a
/// build baked. The caller scopes `target` to the image that is running; this
/// renders what it is handed and nothing else.
pub fn on_host(target: &Json) -> String {
    let modules = items(target, "modules");
    let gated = |module: &Json| text(module, "flavour");
    let mut out = match text(target, "flavour") {
        None => format!("{} modules, the ungated set.\n", modules.len()),
        Some(flavour) => format!(
            "{} modules, {} of them gated to `{flavour}`.\n",
            modules.len(),
            modules.iter().filter(|m| gated(m).is_some()).count()
        ),
    };
    out.push_str("\n| Module | Description | Options | Satisfies |\n| --- | --- | --- | --- |\n");

    for module in modules {
        let _ = write!(out, "| `{}`", text(module, "path").unwrap_or_default());
        for (word, value) in [
            ("", gated(module)),
            ("variant=", text(module, "variant")),
            ("remote=", text(module, "remote")),
        ] {
            if let Some(value) = value {
                let _ = match word.is_empty() {
                    true => write!(out, " `[{value}]`"),
                    false => write!(out, " `{word}{value}`"),
                };
            }
        }
        let _ = write!(
            out,
            " | {}",
            cell(&text(module, "description").unwrap_or_default())
        );
        let options: Vec<String> = match field(module, "options") {
            Some(Json::Object(fields)) => fields
                .iter()
                .map(|(name, value)| match value {
                    Json::String(value) => format!("`{name}=\"{}\"`", cell(value)),
                    other => format!("`{name}={}`", other.render()),
                })
                .collect(),
            _ => Vec::new(),
        };
        let satisfies: Vec<String> = items(module, "satisfies")
            .iter()
            .map(|claim| {
                format!(
                    "`{}: {}`",
                    cell(&text(claim, "benchmark").unwrap_or_default()),
                    strings(claim, "rules")
                        .iter()
                        .map(|rule| cell(rule))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        let _ = writeln!(out, " | {} | {} |", options.join(" "), satisfies.join(" "));
    }
    out
}

/// A `|` would end the cell it stands in.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

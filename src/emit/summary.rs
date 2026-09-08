//! What one target is made of, as the markdown a build summary shows.

use crate::emit::json::{field, items, strings, text, Json};
use crate::emit::plan::of_target;
use crate::model::image::List;
use std::fmt::Write as _;

/// One module's line, filled by either reading. `options` holds each value as
/// it is written after the `=`, which is the one thing the two readings render
/// differently.
struct Row {
    path: String,
    flavour: Option<String>,
    variant: Option<String>,
    remote: Option<String>,
    description: String,
    options: Vec<(String, String)>,
    satisfies: Vec<(String, Vec<String>)>,
}

/// None when nothing publishes under that name.
pub fn render(list: &List, target: &str) -> Option<String> {
    let (_, flavour, entries) = of_target(list, target)?;
    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            let module = entry.module.as_ref();
            Row {
                path: entry.path.clone(),
                flavour: entry.flavour.clone(),
                variant: entry.variant.clone(),
                remote: entry
                    .pin(&list.sources)
                    .map(|remote| remote.version.clone().unwrap_or_default()),
                description: module.map(|m| m.description.clone()).unwrap_or_default(),
                options: module
                    .map(|m| m.resolved.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|(name, value)| (name.clone(), format!("\"{}\"", cell(value))))
                    .collect(),
                satisfies: module
                    .map(|m| m.satisfies.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|coverage| (coverage.benchmark.clone(), coverage.rules.clone()))
                    .collect(),
            }
        })
        .collect();
    Some(table(flavour.as_deref(), &rows))
}

/// The same table with no repository at all, off one target of the manifest a
/// build baked. The caller scopes `target` to the image that is running; this
/// renders what it is handed and nothing else.
pub fn on_host(target: &Json) -> String {
    let rows: Vec<Row> = items(target, "modules")
        .iter()
        .map(|module| Row {
            path: text(module, "path").unwrap_or_default(),
            flavour: text(module, "flavour"),
            variant: text(module, "variant"),
            remote: text(module, "remote"),
            description: text(module, "description").unwrap_or_default(),
            options: match field(module, "options") {
                Some(Json::Object(fields)) => fields
                    .iter()
                    .map(|(name, value)| {
                        let written = match value {
                            Json::String(value) => format!("\"{}\"", cell(value)),
                            other => other.render(),
                        };
                        (name.clone(), written)
                    })
                    .collect(),
                _ => Vec::new(),
            },
            satisfies: items(module, "satisfies")
                .iter()
                .map(|claim| {
                    (
                        text(claim, "benchmark").unwrap_or_default(),
                        strings(claim, "rules"),
                    )
                })
                .collect(),
        })
        .collect();
    table(text(target, "flavour").as_deref(), &rows)
}

fn table(flavour: Option<&str>, rows: &[Row]) -> String {
    let mut out = match flavour {
        None => format!("{} modules, the ungated set.\n", rows.len()),
        Some(flavour) => format!(
            "{} modules, {} of them gated to `{flavour}`.\n",
            rows.len(),
            rows.iter().filter(|row| row.flavour.is_some()).count()
        ),
    };
    out.push_str("\n| Module | Description | Options | Satisfies |\n| --- | --- | --- | --- |\n");
    for row in rows {
        let _ = write!(out, "| `{}`", row.path);
        if let Some(flavour) = &row.flavour {
            let _ = write!(out, " `[{flavour}]`");
        }
        for (word, value) in [("variant", &row.variant), ("remote", &row.remote)] {
            if let Some(value) = value {
                let _ = write!(out, " `{word}={value}`");
            }
        }
        let options: Vec<String> = row
            .options
            .iter()
            .map(|(name, value)| format!("`{name}={value}`"))
            .collect();
        let satisfies: Vec<String> = row
            .satisfies
            .iter()
            .map(|(benchmark, rules)| {
                format!(
                    "`{}: {}`",
                    cell(benchmark),
                    rules
                        .iter()
                        .map(|rule| cell(rule))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        let _ = writeln!(
            out,
            " | {} | {} | {} |",
            cell(&row.description),
            options.join(" "),
            satisfies.join(" ")
        );
    }
    out
}

/// A `|` would end the cell it stands in.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

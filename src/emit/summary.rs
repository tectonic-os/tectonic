//! What one target is made of, as the markdown a build summary shows.

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

/// A `|` would end the cell it stands in.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

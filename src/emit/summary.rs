//! Repository and baked-manifest readings share one Markdown renderer.

use crate::emit::plan::of_target;
use crate::model::image::{List, NO_FLAVOUR};
use common::json::{field, items, strings, text, Json};
use std::fmt::Write as _;

/// A shared row model keeps repository and baked-manifest output aligned.
/// Options retain rendered values because the two sources represent them
/// differently.
struct Row {
    path: String,
    flavour: Option<String>,
    variant: Option<String>,
    remote: Option<String>,
    description: String,
    options: Vec<(String, String)>,
    satisfies: Vec<(String, Vec<String>)>,
    /// Rule, and whether the image lifted it. A refusal leaves a benchmark
    /// rule failing, which is at least as load-bearing as a claim.
    refuses: Vec<(String, bool)>,
}

/// Missing targets produce no summary.
pub fn render(list: &List, target: &str) -> Option<String> {
    let (image, flavour, entries) = of_target(list, target)?;
    let lifted: Vec<&str> = image.allows.iter().map(|a| a.rule.as_str()).collect();
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
                refuses: module
                    .map(|m| m.refuses.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|refusal| {
                        (
                            refusal.rule.clone(),
                            lifted.contains(&refusal.rule.as_str()),
                        )
                    })
                    .collect(),
            }
        })
        .collect();
    Some(table(
        flavour.as_deref(),
        image.conforms_of(flavour.as_deref().unwrap_or(NO_FLAVOUR)),
        &rows,
    ))
}

/// The host command passes only the running target, which prevents
/// repository-wide data from entering the table.
pub fn on_host(target: &Json) -> String {
    let lifted: Vec<String> = items(target, "allows")
        .iter()
        .filter_map(|allow| text(allow, "rule"))
        .collect();
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
                            // JSON layout whitespace must be folded into one
                            // Markdown table cell.
                            other => cell(
                                other
                                    .render()
                                    .lines()
                                    .map(str::trim_start)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                                    .trim(),
                            ),
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
            refuses: items(module, "refuses")
                .iter()
                .filter_map(|refusal| text(refusal, "rule"))
                .map(|rule| {
                    let lifted = lifted.contains(&rule);
                    (rule, lifted)
                })
                .collect(),
        })
        .collect();
    table(
        text(target, "flavour").as_deref(),
        text(target, "conforms").as_deref().unwrap_or_default(),
        &rows,
    )
}

fn table(flavour: Option<&str>, conforms: &str, rows: &[Row]) -> String {
    let noun = match rows.len() {
        1 => "module",
        _ => "modules",
    };
    let mut out = match flavour {
        None => format!("{} {noun}, the ungated set.\n", rows.len()),
        Some(flavour) => format!(
            "{} {noun}, {} gated to `{flavour}`.\n",
            rows.len(),
            rows.iter().filter(|row| row.flavour.is_some()).count()
        ),
    };
    // The benchmark the image declares, which the claims below are read
    // against. A machine with no repository has this document and no other.
    if !conforms.is_empty() {
        let _ = write!(out, "\nIt declares `conforms \"{}\"`.\n", cell(conforms));
    }
    out.push_str(
        "\n| Module | Description | Options | Satisfies | Refuses |\n| --- | --- | --- | --- | \
         --- |\n",
    );
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
        // A lifted refusal stays beside its module because the declaration
        // remains even when the image overrides it.
        let refuses: Vec<String> = row
            .refuses
            .iter()
            .map(|(rule, lifted)| match lifted {
                true => format!("`{}` (lifted)", cell(rule)),
                false => format!("`{}`", cell(rule)),
            })
            .collect();
        let _ = writeln!(
            out,
            " | {} | {} | {} | {} |",
            cell(&row.description),
            options.join(" "),
            satisfies.join(" "),
            refuses.join(" ")
        );
    }
    out
}

/// A `|` would end the cell it stands in.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A foreign manifest can contain a non-string option. JSON rendering must
    /// not split its Markdown row.
    #[test]
    fn an_option_that_is_not_a_string_stays_on_its_row() {
        let module = Json::object([
            ("path", Json::string("one/hello")),
            (
                "options",
                Json::map([
                    ("count".to_string(), Json::number(3)),
                    ("on".to_string(), Json::Bool(true)),
                    ("missing".to_string(), Json::Null),
                    ("names".to_string(), Json::strings(["a|b", "two  spaces"])),
                ]),
            ),
        ]);
        let out = on_host(&Json::object([
            ("flavour", Json::Null),
            ("modules", Json::array([module])),
        ]));
        assert_eq!(
            out.lines().last(),
            Some(
                "| `one/hello` |  | `count=3` `on=true` `missing=null` \
                 `names=[ \"a\\|b\", \"two  spaces\" ]` |  |  |"
            ),
            "{out}"
        );
        assert_eq!(out.lines().count(), 5, "{out}");
    }
}

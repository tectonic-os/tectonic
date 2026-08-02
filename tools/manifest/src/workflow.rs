//! Which workflows run.

use crate::diag::{Issue, Issues};
use crate::list::List;
use std::fmt::Write as _;
use std::path::Path;

/// GitHub's path, not this repository's choice, which is why it is written
/// here rather than declared anywhere.
const WORKFLOW_DIR: &str = ".github/workflows";

/// Every workflow file and whether the declaration says it runs.
pub fn resolve(list: &List, root: &Path, issues: &mut Issues) -> Vec<(String, bool)> {
    let files = files(root);

    for toggle in &list.workflows {
        if files.iter().any(|(_, stem)| *stem == toggle.name) {
            continue;
        }
        let known: Vec<&str> = files.iter().map(|(_, stem)| stem.as_str()).collect();
        issues.push(
            Issue::new(
                format!("`{}` is not a workflow", toggle.name),
                &list.repo_file,
                &list.repo_text,
            )
            .at(toggle.span, format!("no such file under {WORKFLOW_DIR}/"))
            .help(if known.is_empty() {
                format!("{WORKFLOW_DIR}/ holds no workflows")
            } else {
                format!("workflows: {}", known.join(", "))
            }),
        );
    }

    files
        .into_iter()
        .map(|(file, stem)| {
            let enabled = list
                .workflows
                .iter()
                .find(|w| w.name == stem)
                .is_none_or(|w| w.enabled);
            (file, enabled)
        })
        .collect()
}

/// One line per workflow file, pipe separated: <file>|<enabled> The file name
/// rather than the stem, because that is what the API takes as its
/// `workflow_id`, and `true`/`false` rather than the API's own
/// `active`/`disabled_manually` because this says what should be, not what is.
pub fn render(workflows: &[(String, bool)]) -> String {
    let mut out = String::new();
    for (file, enabled) in workflows {
        let _ = writeln!(out, "{file}|{enabled}");
    }
    out
}

/// Every workflow file, as its name and its stem, sorted by name so two runs
/// on the same tree answer identically.
fn files(root: &Path) -> Vec<(String, String)> {
    let dir = root.join(WORKFLOW_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut out: Vec<(String, String)> = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name
            .strip_suffix(".yml")
            .or_else(|| name.strip_suffix(".yaml"))
        else {
            continue;
        };
        let stem = stem.to_string();
        out.push((name, stem));
    }
    out.sort();
    out
}

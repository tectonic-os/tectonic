//! `generate` writes `.github/renovate.json5` from three parts. The
//! `renovate` block in repo.kdl gives the policy. The template gives the
//! managers that the tool's own files need. Each image entry that sets a
//! tracked `version` gives one manager.

use crate::model::image::List;
use common::json::Json;
use std::path::{Path, PathBuf};

const TEMPLATE: &str = include_str!("renovate.json5");
const POLICY: &str = "  // tect:policy\n\n";
const MANAGERS: &str = "    // tect:managers\n";

pub fn path() -> PathBuf {
    PathBuf::from(".github/renovate.json5")
}

/// Answers the file, or nothing if repo.kdl declares no `renovate` block.
pub fn file(list: &List) -> Option<String> {
    let policy: String = list
        .renovate
        .as_ref()?
        .iter()
        .map(|(key, value)| format!("  {}: {},\n", quoted(key), indented(value, "  ")))
        .collect();
    let policy = match policy.is_empty() {
        true => policy,
        false => format!("{policy}\n"),
    };
    let managers: String = managers(list)
        .iter()
        .map(|manager| format!("    {},\n", indented(manager, "    ")))
        .collect();
    Some(
        TEMPLATE
            .replace(POLICY, &policy)
            .replace(MANAGERS, &managers),
    )
}

/// One manager for each image file and module whose entry sets `version`,
/// where the module declares what tracks it. Every entry of the module in that
/// file carries the same tracker, so one manager matches them all.
fn managers(list: &List) -> Vec<Json> {
    let mut seen: Vec<(String, &str)> = Vec::new();
    let mut managers = Vec::new();
    for image in &list.images {
        let file = Path::new(image.src.name())
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .expect("an image source is named by its file");
        for entry in &image.entries {
            if !entry.options.iter().any(|(name, ..)| name == "version") {
                continue;
            }
            let Some(tracker) = entry
                .module
                .as_ref()
                .and_then(|module| module.options.iter().find(|opt| opt.name == "version"))
                .and_then(|opt| opt.renovate.as_ref())
            else {
                continue;
            };
            if seen.iter().any(|(f, p)| *f == file && *p == entry.path) {
                continue;
            }
            seen.push((file.clone(), &entry.path));
            let mut fields = vec![
                ("customType", Json::string("regex")),
                (
                    "description",
                    Json::string(format!("`version` of {} in {file}", entry.path)),
                ),
                (
                    "managerFilePatterns",
                    Json::strings([format!("/^{}$/", escaped(&file))]),
                ),
                ("matchStrings", Json::strings([entry_match(&entry.path)])),
                ("datasourceTemplate", Json::string(&tracker.datasource)),
                ("depNameTemplate", Json::string(&tracker.dep_name)),
            ];
            if let Some(extract) = &tracker.extract_version {
                fields.push(("extractVersionTemplate", Json::string(extract)));
            }
            managers.push(Json::object(fields));
        }
    }
    managers
}

/// Builds the pattern that finds the `version` value of the entry for `path`.
/// The entry name may be quoted or bare. The value must open a line or follow
/// `{` or `;`, so `kernel-version` and a commented line never match.
fn entry_match(path: &str) -> String {
    let path = escaped(path);
    format!(
        "module\\s+(?:\"{path}\"|{path}\\s)[^{{\\n;]*\\{{(?:[^}}]*?[;\\n])?\\s*version\\s+\"(?<currentValue>[^\"]+)\""
    )
}

/// Escapes each regular expression metacharacter. Renovate may compile the
/// pattern as a JavaScript expression with the `u` flag. That flag refuses an
/// escaped character that is not a metacharacter, such as `\@`.
fn escaped(text: &str) -> String {
    text.chars()
        .map(|c| match "\\.^$|?*+()[]{}".contains(c) {
            true => format!("\\{c}"),
            false => c.to_string(),
        })
        .collect()
}

fn quoted(key: &str) -> String {
    Json::string(key).render().trim_end().to_string()
}

/// Renders the value as pretty JSON. Each line after the first is indented to
/// sit under its key.
fn indented(value: &Json, indent: &str) -> String {
    value
        .render()
        .trim_end()
        .lines()
        .collect::<Vec<_>>()
        .join(&format!("\n{indent}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_holds_both_markers() {
        assert!(TEMPLATE.contains(POLICY));
        assert!(TEMPLATE.contains(MANAGERS));
    }

    #[test]
    fn only_a_metacharacter_is_escaped() {
        assert_eq!(
            escaped("core/image-viewer_2@1.x"),
            "core/image-viewer_2@1\\.x"
        );
    }
}

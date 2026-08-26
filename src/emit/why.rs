//! `why <module>`: the per-module trust read-out.
//!
//! One renderer, two readings. In a repository it comes off the resolved plan;
//! on a live host with no `repo.kdl` it comes off the manifest and the build
//! record baked into the image. What it answers is the same either way: what
//! lists this module, what it exchanges with the rest of them, what it claims,
//! and where every byte of it came from.

use crate::emit::json::Json;
use crate::emit::{Part, Table};
use crate::layout;
use crate::model::image::{Image, List};
use crate::model::module::Module;
use crate::provenance::Evidence;
use std::fmt::Write as _;

/// Something the module pulls in from outside the repository.
pub struct Fetch {
    pub name: String,
    pub locator: Option<String>,
    pub selector: Option<String>,
    pub verifier: Option<String>,
    pub tracker: String,
}

impl Fetch {
    fn of(name: &str, pin: &Evidence) -> Self {
        Fetch {
            name: name.to_string(),
            locator: pin.url.clone(),
            selector: pin.version.clone(),
            verifier: pin.sha256.clone(),
            tracker: pin.tracker.as_str().to_string(),
        }
    }

    fn json(&self) -> Json {
        Json::object([
            ("name", Json::string(&self.name)),
            ("locator", Json::optional(self.locator.clone())),
            ("selector", Json::optional(self.selector.clone())),
            ("verifier", Json::optional(self.verifier.clone())),
            ("tracker", Json::string(&self.tracker)),
        ])
    }
}

/// Everything the read-out says, gathered before anything is rendered so the
/// two readings meet here rather than in the output.
#[derive(Default)]
pub struct Why {
    pub path: String,
    pub description: String,
    /// The targets that build it, as they are named.
    pub images: Vec<String>,
    /// A capability it provides, and every module in the same image that
    /// requires it.
    pub provides: Vec<(String, Vec<String>)>,
    /// A capability it requires, and what provides it.
    pub requires: Vec<(String, Option<String>)>,
    pub satisfies: Vec<(String, Vec<String>)>,
    pub content: Option<String>,
    /// The collection it was imported from, and the pin that collection had.
    pub imported: Option<(String, Fetch)>,
    pub modified: bool,
    /// What the build observed the directory hashing to, where that was
    /// recorded. `content` is what the repository declared.
    pub built: Option<String>,
    pub fetches: Vec<Fetch>,
    /// It enables a third-party package repository, and the URLs its `repo`
    /// file names.
    pub repo: Option<Vec<String>>,
    /// Whether the `repo` file itself was there to read. A finished image does
    /// not carry the module tree, so a host knows only that there is one.
    pub repo_read: bool,
}

fn named(path: &str, given: &str) -> bool {
    path == given
        || (!given.is_empty()
            && path
                .strip_suffix(given)
                .is_some_and(|prefix| prefix.ends_with('/')))
}

pub(crate) fn matching(paths: &[String], given: &str) -> Vec<String> {
    paths
        .iter()
        .filter(|path| named(path, given))
        .cloned()
        .collect()
}

pub fn display(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| {
            let name = path.rsplit('/').next().unwrap_or(path);
            match paths
                .iter()
                .filter(|other| other.rsplit('/').next() == Some(name))
                .count()
            {
                1 => name.to_string(),
                _ => path.clone(),
            }
        })
        .collect()
}

/// What the repository says about one module. None when nothing declares it.
pub fn of(list: &List, path: &str, root: &std::path::Path) -> Option<Why> {
    let module = list
        .images
        .iter()
        .flat_map(Image::modules)
        .find(|m| named(&m.path, path))?;

    let mut why = Why {
        path: module.path.clone(),
        description: module.description.clone(),
        content: module.content.clone(),
        modified: matches!((&module.imported, &module.content),
            (Some(record), Some(content)) if record.content != *content),
        repo: module.repo.then(|| repo_urls(root, &module.dir)),
        repo_read: true,
        ..Why::default()
    };

    if let Some(record) = &module.imported {
        why.imported = Some((
            record.collection.clone(),
            Fetch::of("collection", &record.pin),
        ));
    }
    for asset in &module.assets {
        why.fetches.push(Fetch::of(&asset.name, &asset.pin));
    }
    for coverage in &module.satisfies {
        why.satisfies
            .push((coverage.benchmark.clone(), coverage.rules.clone()));
    }

    for image in &list.images {
        for entry in &image.entries {
            if entry.path != path {
                continue;
            }
            why.images.push(match &entry.flavour {
                None => image.id.clone(),
                Some(flavour) => format!("{}-{flavour}", image.id),
            });
        }
    }

    // Who trades with it, which is the half a manifest cannot answer alone.
    let peers: Vec<&Module> = list.images.iter().flat_map(Image::modules).collect();
    for decl in &module.provides {
        let wanted: Vec<String> = peers
            .iter()
            .filter(|other| other.path != path)
            .filter(|other| other.requires.iter().any(|r| r.name == decl.name))
            .map(|other| other.path.clone())
            .collect();
        why.provides.push((decl.name.clone(), wanted));
    }
    for decl in &module.requires {
        let from = peers
            .iter()
            .find(|other| other.provides.iter().any(|p| p.name == decl.name))
            .map(|other| other.path.clone())
            .or_else(|| {
                list.images
                    .iter()
                    .filter_map(|i| i.base.as_ref())
                    .any(|b| b.provides.iter().any(|p| p.name == decl.name))
                    .then(|| "base".to_string())
            });
        why.requires.push((decl.name.clone(), from));
    }

    Some(why)
}

/// Every URL a `repo` file names. A pointer, not a parsing contract: the file
/// is shell calling the family's config manager, so this reads what a person
/// would look for rather than claiming to understand it.
fn repo_urls(root: &std::path::Path, dir: &str) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(layout::module(root, dir).join("repo")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for word in text.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
        // A URL is as likely to be the value of a flag as a word of its own.
        let Some(at) = word.find("https://").or_else(|| word.find("http://")) else {
            continue;
        };
        let url = word[at..].trim_end_matches(['\\', ',', ';']).to_string();
        if !out.contains(&url) {
            out.push(url);
        }
    }
    out
}

/// The names a repository declares, for a `why` that was given none of them.
pub fn known(list: &List) -> Vec<String> {
    let mut out: Vec<String> = list
        .images
        .iter()
        .flat_map(|image| image.entries.iter().chain(&image.suppressed))
        .map(|entry| entry.path.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

fn listed(names: &[String]) -> String {
    match names.is_empty() {
        true => "nothing".to_string(),
        false => names.join(", "),
    }
}

impl Why {
    pub fn parts(&self, terminal: bool) -> Vec<Part> {
        let mut out = vec![Part::Heading(self.path.clone())];
        if !self.description.is_empty() {
            out.push(Part::Text(self.description.clone()));
        }

        out.push(Part::Heading("Where it is built".into()));
        out.push(Part::Text(listed(&self.images)));

        out.push(Part::Heading("What it exchanges".into()));
        if self.provides.is_empty() && self.requires.is_empty() {
            out.push(Part::Text(
                "Nothing: it neither provides nor requires.".into(),
            ));
        } else {
            let rows = self
                .provides
                .iter()
                .map(|(name, wanted)| {
                    (
                        vec!["provides".into(), format!("`{name}`"), listed(wanted)],
                        false,
                    )
                })
                .chain(self.requires.iter().map(|(name, from)| {
                    (
                        vec![
                            "requires".into(),
                            format!("`{name}`"),
                            from.clone().unwrap_or_else(|| "nothing".into()),
                        ],
                        false,
                    )
                }))
                .collect();
            out.push(Part::Table(Table {
                title: String::new(),
                header: &["Direction", "Capability", "With"],
                rows,
            }));
        }

        out.push(Part::Heading("What it claims".into()));
        match self.satisfies.is_empty() {
            true => out.push(Part::Text("Nothing. It declares no `satisfies`.".into())),
            false => {
                out.push(Part::Table(Table {
                    title: String::new(),
                    header: &["Benchmark", "Rules"],
                    rows: self
                        .satisfies
                        .iter()
                        .map(|(benchmark, rules)| {
                            (vec![format!("`{benchmark}`"), rules.join(", ")], false)
                        })
                        .collect(),
                }));
                out.push(Part::Text(
                    "A claim the tool records rather than certifies. The scan is what confirms it."
                        .into(),
                ));
            }
        }

        out.push(Part::Heading("Where it came from".into()));
        match (&self.content, &self.built) {
            (None, None) => out.push(Part::Text("Nothing hashed it.".into())),
            (declared, built) => {
                let mut rows = vec![(vec!["declared".into(), hash(declared)], false)];
                if let Some(built) = built {
                    rows.push((
                        vec!["observed by the build".into(), format!("`{built}`")],
                        false,
                    ));
                }
                out.push(Part::Table(Table {
                    title: String::new(),
                    header: &["Content", "Hash"],
                    rows,
                }));
                if built.is_some() && declared != built {
                    out.push(Part::Text(
                        "**The two documents disagree**, which they cannot if both came from this build."
                            .into(),
                    ));
                }
            }
        }
        match &self.imported {
            None => out.push(Part::Text(
                "It was written in this repository rather than imported, so nothing upstream to compare it against."
                    .into(),
            )),
            Some((collection, pin)) => {
                out.extend(evidence(
                    "Collection",
                    &[(collection.as_str(), pin)],
                    terminal,
                ));
                out.push(Part::Text(match self.modified {
                    true => "**It has been edited since it was imported.** Forking a module is legitimate; what the record buys is that the fork is visible.".into(),
                    false => "Its content still matches what was imported.".into(),
                }));
            }
        }

        out.push(Part::Heading("What it pulls in".into()));
        match self.fetches.is_empty() {
            true => out.push(Part::Text("Nothing. It declares no `asset`.".into())),
            false => {
                let rows: Vec<(&str, &Fetch)> = self
                    .fetches
                    .iter()
                    .map(|pin| (pin.name.as_str(), pin))
                    .collect();
                out.extend(evidence("Asset", &rows, terminal));
            }
        }

        out.push(Part::Heading("Third-party repositories".into()));
        match &self.repo {
            None => out.push(Part::Text("None. It ships no `repo` file.".into())),
            Some(urls) => {
                out.push(Part::Text(format!(
                    "It enables one, in `modules/{}/repo`. There is no grammar for that file, so read it: it is shell calling the family's config manager.",
                    self.path
                )));
                match urls.is_empty() {
                    true => out.push(Part::Text(match self.repo_read {
                        true => "No URL in it.".into(),
                        false => "Not readable from here: a finished image carries the manifest, not the module tree.".into(),
                    })),
                    false => out.push(Part::Table(Table {
                        title: String::new(),
                        header: &["URL it names"],
                        rows: urls.iter().map(|url| (vec![url.clone()], false)).collect(),
                    })),
                }
            }
        }
        out
    }

    pub fn markdown(&self) -> String {
        let mut out = String::new();
        for (at, part) in self.parts(false).iter().enumerate() {
            match part {
                Part::Heading(text) => {
                    let _ = writeln!(out, "{} {text}\n", if at == 0 { "#" } else { "##" });
                }
                Part::Text(text) => {
                    let _ = writeln!(out, "{text}\n");
                }
                Part::Table(table) => {
                    let _ = writeln!(out, "| {} |", table.header.join(" | "));
                    let _ = writeln!(out, "| {} |", vec!["---"; table.header.len()].join(" | "));
                    for (cells, _) in &table.rows {
                        let _ = writeln!(out, "| {} |", cells.join(" | "));
                    }
                    out.push('\n');
                }
            }
        }
        out
    }

    pub fn json(&self) -> Json {
        Json::object([
            ("module", Json::string(&self.path)),
            ("description", Json::string(&self.description)),
            ("images", Json::strings(self.images.clone())),
            (
                "provides",
                Json::array(self.provides.iter().map(|(name, wanted)| {
                    Json::object([
                        ("capability", Json::string(name)),
                        ("required_by", Json::strings(wanted.clone())),
                    ])
                })),
            ),
            (
                "requires",
                Json::array(self.requires.iter().map(|(name, from)| {
                    Json::object([
                        ("capability", Json::string(name)),
                        ("provided_by", Json::optional(from.clone())),
                    ])
                })),
            ),
            (
                "satisfies",
                Json::array(self.satisfies.iter().map(|(benchmark, rules)| {
                    Json::object([
                        ("benchmark", Json::string(benchmark)),
                        ("rules", Json::strings(rules.clone())),
                    ])
                })),
            ),
            ("content", Json::optional(self.content.clone())),
            ("built", Json::optional(self.built.clone())),
            (
                "imported",
                match &self.imported {
                    None => Json::Null,
                    Some((collection, pin)) => Json::object([
                        ("collection", Json::string(collection)),
                        ("pin", pin.json()),
                    ]),
                },
            ),
            ("modified", Json::Bool(self.modified)),
            ("fetches", Json::array(self.fetches.iter().map(Fetch::json))),
            (
                "repo",
                match &self.repo {
                    None => Json::Null,
                    Some(urls) => Json::strings(urls.clone()),
                },
            ),
        ])
    }
}

/// The four evidence slots across a row in markdown, or down rows at a terminal.
fn evidence(first: &'static str, pins: &[(&str, &Fetch)], terminal: bool) -> Vec<Part> {
    let say = |value: &Option<String>| value.clone().unwrap_or_else(|| "not declared".into());
    if terminal {
        return pins
            .iter()
            .map(|(name, pin)| {
                Part::Table(Table {
                    title: format!("{first} {name}"),
                    header: &["Field", "Value"],
                    rows: vec![
                        (vec!["Locator".into(), say(&pin.locator)], false),
                        (vec!["Selector".into(), say(&pin.selector)], false),
                        (vec!["Verifier".into(), say(&pin.verifier)], false),
                        (vec!["Tracker".into(), pin.tracker.clone()], false),
                    ],
                })
            })
            .collect();
    }
    vec![Part::Table(Table {
        title: String::new(),
        header: match first {
            "Collection" => &["Collection", "Locator", "Selector", "Verifier", "Tracker"],
            _ => &["Asset", "Locator", "Selector", "Verifier", "Tracker"],
        },
        rows: pins
            .iter()
            .map(|(name, pin)| {
                (
                    vec![
                        format!("`{name}`"),
                        say(&pin.locator),
                        say(&pin.selector),
                        say(&pin.verifier),
                        pin.tracker.clone(),
                    ],
                    false,
                )
            })
            .collect(),
    })]
}

fn hash(value: &Option<String>) -> String {
    match value {
        Some(hash) => format!("`{hash}`"),
        None => "nothing".to_string(),
    }
}

// ---- on a live host ------------------------------------------------------

fn field<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    match value {
        Json::Object(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, v)| v),
        _ => None,
    }
}

fn text(value: &Json, key: &str) -> Option<String> {
    match field(value, key) {
        Some(Json::String(found)) => Some(found.clone()),
        _ => None,
    }
}

fn items<'a>(value: &'a Json, key: &str) -> &'a [Json] {
    match field(value, key) {
        Some(Json::Array(found)) => found,
        _ => &[],
    }
}

fn strings(value: &Json, key: &str) -> Vec<String> {
    items(value, key)
        .iter()
        .filter_map(|item| match item {
            Json::String(found) => Some(found.clone()),
            _ => None,
        })
        .collect()
}

fn pin_of(value: &Json, name: &str) -> Fetch {
    Fetch {
        name: name.to_string(),
        locator: text(value, "locator"),
        selector: text(value, "selector"),
        verifier: text(value, "verifier"),
        tracker: text(value, "tracker").unwrap_or_else(|| "none".into()),
    }
}

/// Every target in the baked manifest, as the target object and its name.
fn targets(manifest: &Json) -> Vec<&Json> {
    items(manifest, "images")
        .iter()
        .flat_map(|image| items(image, "targets"))
        .collect()
}

/// The same read-out with no repository at all: the manifest is what the image
/// declares it is made of, the build record what the build resolved. Both are
/// baked, so an image answers for itself.
pub fn on_host(manifest: &Json, record: Option<&Json>, path: &str) -> Option<Why> {
    let mut why = Why {
        path: path.to_string(),
        ..Why::default()
    };
    let mut found = false;

    for target in targets(manifest) {
        let Some(module) = items(target, "modules").iter().find(|module| {
            text(module, "path")
                .as_deref()
                .is_some_and(|found| named(found, path))
        }) else {
            continue;
        };
        found = true;
        if let Some(name) = text(target, "published").or_else(|| text(target, "name")) {
            if !why.images.contains(&name) {
                why.images.push(name);
            }
        }
        why.description = text(module, "description").unwrap_or_default();
        for name in strings(module, "provides") {
            if !why.provides.iter().any(|(have, _)| *have == name) {
                let wanted = items(target, "modules")
                    .iter()
                    .filter(|other| text(other, "path").as_deref() != Some(path))
                    .filter(|other| strings(other, "requires").contains(&name))
                    .filter_map(|other| text(other, "path"))
                    .collect();
                why.provides.push((name, wanted));
            }
        }
        for name in strings(module, "requires") {
            if !why.requires.iter().any(|(have, _)| *have == name) {
                let from = items(target, "modules")
                    .iter()
                    .find(|other| strings(other, "provides").contains(&name))
                    .and_then(|other| text(other, "path"));
                why.requires.push((name, from));
            }
        }
        for claim in items(module, "satisfies") {
            let Some(benchmark) = text(claim, "benchmark") else {
                continue;
            };
            if !why.satisfies.iter().any(|(have, _)| *have == benchmark) {
                why.satisfies.push((benchmark, strings(claim, "rules")));
            }
        }

        if let Some(provenance) = field(module, "provenance") {
            why.content = text(provenance, "content");
            if let Some(imported) = field(provenance, "imported") {
                if let (Some(collection), Some(pin)) =
                    (text(imported, "collection"), field(imported, "pin"))
                {
                    why.modified = text(imported, "content") != why.content;
                    why.imported = Some((collection, pin_of(pin, "collection")));
                }
            }
            if matches!(field(provenance, "repo"), Some(Json::Bool(true))) {
                // The module tree is not in the finished image, so the file
                // itself cannot be read here; that it exists is the fact.
                why.repo = Some(Vec::new());
            }
        }

        for asset in items(target, "assets") {
            if text(asset, "module").as_deref() != Some(path) {
                continue;
            }
            let Some(name) = text(asset, "name") else {
                continue;
            };
            if why.fetches.iter().any(|have| have.name == name) {
                continue;
            }
            let pin = field(asset, "pin").unwrap_or(&Json::Null);
            why.fetches.push(pin_of(pin, &name));
        }
    }

    if !found {
        return None;
    }

    // What the build observed, where the manifest only says what was declared.
    if let Some(record) = record {
        why.built = items(record, "modules")
            .iter()
            .find(|module| text(module, "path").as_deref() == Some(path))
            .and_then(|module| text(module, "content"));
    }
    Some(why)
}

/// Every module the baked manifest names, for a `why` given one it does not.
pub fn known_on_host(manifest: &Json) -> Vec<String> {
    let mut out: Vec<String> = targets(manifest)
        .iter()
        .flat_map(|target| items(target, "modules"))
        .filter_map(|module| text(module, "path"))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The two documents an image carries, read from where the build put them.
pub fn baked(
    manifest: &std::path::Path,
    record: &std::path::Path,
) -> Result<(Json, Option<Json>), String> {
    let raw = std::fs::read_to_string(manifest)
        .map_err(|err| format!("{}: {err}", manifest.display()))?;
    let declared = Json::parse(&raw).map_err(|err| format!("{}: {err}", manifest.display()))?;
    let resolved = match std::fs::read_to_string(record) {
        Ok(raw) => Some(Json::parse(&raw).map_err(|err| format!("{}: {err}", record.display()))?),
        Err(_) => None,
    };
    Ok((declared, resolved))
}

#[cfg(test)]
mod tests {
    use super::{display, matching, Fetch, Why};
    use crate::emit::Part;

    #[test]
    fn module_names_are_unambiguous_suffixes() {
        let paths = vec![
            "one/hardening/coredumps".to_string(),
            "two/coredumps".to_string(),
            "one/updates".to_string(),
        ];

        assert_eq!(matching(&paths, "updates"), ["one/updates"]);
        assert_eq!(
            matching(&paths, "coredumps"),
            ["one/hardening/coredumps", "two/coredumps"]
        );
        assert_eq!(
            display(&paths),
            ["one/hardening/coredumps", "two/coredumps", "updates"]
        );
    }

    #[test]
    fn terminal_evidence_runs_down_two_columns() {
        let why = Why {
            fetches: vec![Fetch {
                name: "archive".into(),
                locator: Some("https://example.com/archive".into()),
                selector: Some("v1".into()),
                verifier: Some("abc".into()),
                tracker: "renovate".into(),
            }],
            ..Why::default()
        };
        let table = why
            .parts(true)
            .into_iter()
            .find_map(|part| match part {
                Part::Table(table) if table.title == "Asset archive" => Some(table),
                _ => None,
            })
            .expect("the asset has an evidence table");

        assert_eq!(table.header, ["Field", "Value"]);
        assert_eq!(table.rows.len(), 4);
    }
}

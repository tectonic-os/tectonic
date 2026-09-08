//! Every rule the profile an image declares selects, what the image claims it
//! with, and which module elsewhere would. The claim resolves forward
//! through `Content::rules`, the search for who would help runs backward
//! through `Content::numbering`, and the two are not interchangeable.

use crate::emit::json::Json;
use crate::emit::{Part, Table};
use crate::model::image::{Entry, Image};
use crate::provider::Index;
use crate::scap::{ordinal, reached, Content, Profile};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// One selected rule, as everything the read-out says about it.
struct Row {
    rule: String,
    /// Every number that reaches this rule, so a claim can be written against
    /// it. Empty for a rule no number reaches, which nothing can claim.
    numbers: Vec<String>,
    title: String,
    /// What the image claims this rule with: a module it installs, or the base
    /// it builds on, named by the reference.
    claimed: Vec<String>,
    /// Modules elsewhere that would, for a rule nothing listed claims.
    would: Vec<String>,
}

pub struct Coverage<'a> {
    image: &'a Image,
    profile: &'a Profile,
    rows: Vec<Row>,
}

/// The read-out, or nothing where the datastream carries no profile the image's
/// `conforms` names, which is the caller's diagnostic to make.
pub fn of<'a>(image: &'a Image, content: &'a Content, index: &Index) -> Option<Coverage<'a>> {
    let profile = content.profiles.iter().find(|p| p.is(&image.conforms))?;
    let selected = content.selected(&profile.id);

    // One walk of the image's modules, then one of the index for the whole
    // open set: both are keyed by rule afterwards, so neither is repeated per
    // rule.
    let mut claimed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for module in image.modules() {
        let numbers = module
            .satisfies
            .iter()
            .flat_map(|coverage| coverage.rules.iter());
        for rule in reached(content, numbers) {
            claimed.entry(rule).or_default().insert(module.path.clone());
        }
    }
    // The base claims rules the way a module it installs does, and the
    // reference every layer builds on is what names it in the row.
    if let Some(base) = &image.base {
        let numbers = base
            .satisfies
            .iter()
            .flat_map(|coverage| coverage.rules.iter());
        for rule in reached(content, numbers) {
            claimed.entry(rule).or_default().insert(base.image.clone());
        }
    }
    let open: BTreeSet<String> = selected
        .iter()
        .filter(|rule| !claimed.contains_key(*rule))
        .cloned()
        .collect();
    // A module the base suppressed is still one the image lists, so it is
    // still not an answer to what the image is missing.
    let listed: BTreeSet<String> = image
        .entries
        .iter()
        .chain(&image.suppressed)
        .map(Entry::dir)
        .collect();
    let mut would: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for provider in index.claiming(&content.numbering(&open)) {
        if listed.contains(&provider.dir()) {
            continue;
        }
        for rule in reached(content, provider.declares.satisfies.iter()) {
            if open.contains(&rule) {
                would.entry(rule).or_default().push(provider.qualified());
            }
        }
    }

    let mut rows: Vec<Row> = selected
        .iter()
        .map(|rule| Row {
            numbers: content
                .numbers
                .get(rule)
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
            title: content.titles.get(rule).unwrap_or(rule).clone(),
            claimed: claimed.get(rule).into_iter().flatten().cloned().collect(),
            would: would.remove(rule).unwrap_or_default(),
            rule: rule.clone(),
        })
        .collect();
    // A benchmark reads in its own order, which is not the rule ids' and not
    // a string's: 1.9 comes before 1.10, and a rule no number reaches last.
    rows.sort_by(|a, b| {
        let key = |row: &Row| {
            (
                row.numbers.is_empty(),
                row.numbers.first().map(|n| ordinal(n)).unwrap_or_default(),
                row.rule.clone(),
            )
        };
        key(a).cmp(&key(b))
    });
    Some(Coverage {
        image,
        profile,
        rows,
    })
}

const HEADER: &[&str] = &["Number", "Rule", "Claimed by", "Would claim"];

impl Coverage<'_> {
    fn claimed(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| !row.claimed.is_empty())
            .count()
    }

    pub fn markdown(&self) -> String {
        let named = match self.profile.title.is_empty() {
            true => format!("`{}`", self.profile.name()),
            false => format!("`{}` ({})", self.profile.name(), self.profile.title),
        };
        let unnamed = match self
            .rows
            .iter()
            .filter(|row| row.numbers.is_empty())
            .count()
        {
            0 => String::new(),
            1 => " One of them carries no number, so no `satisfies` can name it.".into(),
            n => format!(" {n} of them carry no number, so no `satisfies` can name them."),
        };
        let mut out = format!(
            "# {} coverage of `{}`\n\n\
             {named} selects {} rules, and what `{}` installs or builds on claims {} of \
             them.{unnamed}\n\n\
             | Number | Rule | Claimed by | Would claim |\n|---|---|---|---|\n",
            self.image.name,
            self.profile.name(),
            self.rows.len(),
            self.image.id,
            self.claimed(),
        );
        for row in &self.rows {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |",
                code(&row.numbers),
                row.title,
                code(&row.claimed),
                code(&row.would),
            );
        }
        out
    }

    pub fn parts(&self) -> Vec<Part> {
        vec![Part::Table(Table {
            title: format!("{} coverage of {}", self.image.id, self.profile.name()),
            header: HEADER,
            rows: self
                .rows
                .iter()
                .map(|row| {
                    (
                        vec![
                            row.numbers.join(", "),
                            row.title.clone(),
                            row.claimed.join(", "),
                            row.would.join(", "),
                        ],
                        row.claimed.is_empty(),
                    )
                })
                .collect(),
        })]
    }

    pub fn json(&self) -> Json {
        Json::object([
            ("image", Json::string(&self.image.id)),
            ("profile", Json::string(self.profile.name())),
            ("title", Json::string(&self.profile.title)),
            (
                "rules",
                Json::array(self.rows.iter().map(|row| {
                    Json::object([
                        ("rule", Json::string(&row.rule)),
                        ("numbers", Json::strings(row.numbers.iter().cloned())),
                        ("title", Json::string(&row.title)),
                        ("claimed_by", Json::strings(row.claimed.iter().cloned())),
                        ("would_claim", Json::strings(row.would.iter().cloned())),
                    ])
                })),
            ),
        ])
    }
}

fn code(names: &[String]) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<String>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
    }

    /// The base claims rules and the row names it, so this read-out and
    /// `tect check` count the same rules open. A rule the base claims is also
    /// one no module elsewhere is offered for.
    #[test]
    fn a_rule_the_base_claims_is_claimed_here_and_offered_to_nobody() {
        let root = fixture("tests/scap/claiming-base");
        let loaded = crate::load(&root);
        let content = crate::scap::content_of(&fixture("tests/scap/datastream.xml"))
            .expect("the fixture datastream reads");
        let image = loaded.list.images.first().expect("one image");
        let read = of(image, &content, &loaded.index).expect("the datastream carries `standard`");
        // The same two rules `tect check` counts open, and the base named in
        // the rows it closed.
        assert_eq!(read.claimed(), 2);
        let out = read.markdown();
        assert!(
            out.contains("installs or builds on claims 2 of them"),
            "{out}"
        );
        assert!(
            out.contains("| `1.1.1.1` | Install AIDE | `quay.io/fedora/fedora-bootc:44` |  |"),
            "{out}"
        );
    }

    /// The index reads a claim the resolved module dropped, so a module the
    /// image lists reaches the open rule it names. The row offers nothing.
    #[test]
    fn a_module_the_image_lists_is_not_offered_as_one_that_would_claim() {
        let root = fixture("tests/scap/listed-claimant");
        let loaded = crate::load(&root);
        let content = crate::scap::content_of(&fixture("tests/scap/datastream.xml"))
            .expect("the fixture datastream reads");
        let image = loaded.list.images.first().expect("one image");
        let out = of(image, &content, &loaded.index)
            .expect("the datastream carries `standard`")
            .markdown();
        assert!(
            out.contains("| `4.1.3.1` | Record Attempts to Alter Logon and Logout Events |  |  |"),
            "{out}"
        );
        assert!(!out.contains("one/broken"), "{out}");
    }
}

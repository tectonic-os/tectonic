//! Every rule the profile an image declares selects, which of its modules
//! claims it, and which module elsewhere would. The claim resolves forward
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
    /// Modules the image lists whose claim reaches this rule.
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
    let open: BTreeSet<String> = selected
        .iter()
        .filter(|rule| !claimed.contains_key(*rule))
        .cloned()
        .collect();
    let listed: BTreeSet<String> = image.entries.iter().map(Entry::dir).collect();
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
             {named} selects {} rules, and what `{}` lists claims {} of them.{unnamed}\n\n\
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

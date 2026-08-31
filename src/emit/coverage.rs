//! Every rule the profile an image declares selects, which of its modules
//! claims it, and which module elsewhere would. The claim resolves forward
//! through `Content::rules`, the search for who would help runs backward
//! through `Content::numbering`, and the two are not interchangeable.

use crate::emit::json::Json;
use crate::emit::{Part, Table};
use crate::model::image::{Entry, Image};
use crate::provider::Index;
use crate::scap::{ordinal, Content, Profile};
use std::collections::BTreeSet;
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
    let listed: BTreeSet<String> = image.entries.iter().map(Entry::dir).collect();
    let mut rows: Vec<Row> = content
        .selected(&profile.id)
        .iter()
        .map(|rule| {
            let numbers: Vec<String> = content
                .numbers
                .get(rule)
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            let claimed: BTreeSet<String> = image
                .modules()
                .filter(|module| {
                    module
                        .satisfies
                        .iter()
                        .flat_map(|coverage| coverage.rules.iter())
                        .any(|number| content.rules.get(number) == Some(rule))
                })
                .map(|module| module.path.clone())
                .collect();
            let would = match claimed.is_empty() {
                false => Vec::new(),
                true => index
                    .claiming(&numbers.iter().cloned().collect())
                    .into_iter()
                    .filter(|provider| !listed.contains(&provider.dir()))
                    .map(|provider| provider.qualified())
                    .collect(),
            };
            Row {
                rule: rule.clone(),
                numbers,
                title: content.titles.get(rule).unwrap_or(rule).clone(),
                claimed: claimed.into_iter().collect(),
                would,
            }
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

//! `tect set`, the config family. A question answered into a declaration file,
//! which `generate` then acts on: nothing here writes an artifact.
//!
//! Collect-then-apply like every other flow, so an apply takes no prompt.

use crate::conformance::profile_options;
use crate::copy;
use crate::declaration::{line_indent, splice};
use crate::error::Error;
use crate::scap::{ordinal, rule_name, Content};
use crate::{layout, parse};
use common::prompt::Prompt;
use common::ui::tree::Change;
use common::ui::{Answer, Choice};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A library added to `repo.kdl`'s `sources`: the default library of its kind
/// or a source the user types. An existing source under the same kind and
/// alias is replaced, so the declaration moves to what the user chose.
pub struct Library {
    kind: crate::model::remote::Kind,
    alias: String,
    at: LibraryAt,
}

enum LibraryAt {
    Default(&'static crate::collection::DefaultLibrary),
    Typed { url: String, path: String },
}

impl Library {
    /// Which library of `kind` to add, then the alias, URL and directory where
    /// the user typed one. Leaving the picker writes nothing. With nobody to
    /// ask, the default library of the kind is added.
    pub fn adding(
        kind: crate::model::remote::Kind,
        prompt: &Prompt,
    ) -> Result<Option<Self>, String> {
        let default = crate::collection::default_library(kind);
        if !prompt.asks() {
            return Ok(Some(Self {
                kind,
                alias: default.alias.to_string(),
                at: LibraryAt::Default(default),
            }));
        }
        let options = [
            Choice::new(
                format!("{} \"{}\"", kind.as_str(), default.alias),
                format!(
                    "{} ({}/{})",
                    default.about,
                    crate::collection::LIBRARY_URL,
                    default.path
                ),
            ),
            Choice::new(copy::OTHER_SOURCE, copy::OTHER_SOURCE_ABOUT),
        ];
        let Some(chosen) = prompt.choose(copy::WHICH_LIBRARY, &options)? else {
            return Ok(None);
        };
        if chosen == 0 {
            return Ok(Some(Self {
                kind,
                alias: default.alias.to_string(),
                at: LibraryAt::Default(default),
            }));
        }
        let alias = prompt.line(None, copy::SOURCE_ALIAS, "a source alias", "", None)?;
        if !crate::model::image::is_name(&alias) {
            return Err(format!(
                "`{alias}` is not a source name: lowercase, digits and dashes, starting with a \
                 letter"
            ));
        }
        let url = prompt.line(
            None,
            copy::SOURCE_URL,
            "the repository URL",
            "",
            Some(crate::collection::LIBRARY_URL),
        )?;
        let path = prompt.line(None, copy::SOURCE_PATH, "a directory inside it", "", None)?;
        Ok(Some(Self {
            kind,
            alias,
            at: LibraryAt::Typed { url, path },
        }))
    }

    /// The block written into `repo.kdl`, replacing the source of the same
    /// kind and alias where one exists.
    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        let file = root.join(layout::REPO_FILE);
        let text =
            std::fs::read_to_string(&file).map_err(|err| format!("{}: {err}", file.display()))?;
        let out = self.spliced(&text)?;
        std::fs::write(&file, out).map_err(|err| format!("{}: {err}", file.display()))?;
        Ok(vec![(
            PathBuf::from(layout::REPO_FILE),
            Change::Updated(format!(
                "{} \"{}\" added to sources",
                self.kind.as_str(),
                self.alias
            )),
        )])
    }

    fn spliced(&self, text: &str) -> Result<String, String> {
        if let Some(span) = parse::repo::source_span(text, self.kind.as_str(), &self.alias) {
            let indent = line_indent(text, span.offset);
            let mut out = text.to_string();
            out.replace_range(span.offset..span.offset + span.len, &self.render(indent));
            return Ok(out);
        }
        match parse::repo::sources_insert(text) {
            Some(close) => {
                let child = format!("{}    ", line_indent(text, close));
                let block = self.render(&child);
                let mut out = text.to_string();
                // A node may not follow a closing brace on the same line, so a
                // one-line block takes a newline before the new node.
                let lead = match out[..close].ends_with('\n') {
                    true => "",
                    false => "\n",
                };
                out.insert_str(close, &format!("{lead}{child}{block}\n"));
                Ok(out)
            }
            None => Ok(splice(
                text,
                None,
                &format!("sources {{\n    {}\n}}", self.render("    ")),
            )),
        }
    }

    /// One source node, with the first line unindented: a caller inserting it
    /// writes the line's own indent, and a caller replacing it leaves the
    /// indent it stands in.
    fn render(&self, indent: &str) -> String {
        let (url, path) = match &self.at {
            LibraryAt::Default(library) => (crate::collection::LIBRARY_URL, library.path),
            LibraryAt::Typed { url, path } => (url.as_str(), path.as_str()),
        };
        format!(
            "{} \"{}\" {{\n{indent}    url \"{url}\"\n{indent}    path \"{path}\"\n{indent}}}",
            self.kind.as_str(),
            self.alias
        )
    }
}

/// The branch a benchmark number sits in: its dotted prefix, and one flat
/// branch for a number with no dot to group it by, since a STIG ident nests
/// nowhere.
fn group(number: &str) -> &str {
    number.rsplit_once('.').map_or("other", |(head, _)| head)
}

fn claimable_rules<'a>(
    content: &'a Content,
    selected: &'a BTreeSet<String>,
) -> Vec<(&'a str, &'a BTreeSet<String>)> {
    let mut rules: Vec<_> = selected
        .iter()
        .filter_map(|rule| {
            content
                .numbers
                .get(rule)
                .map(|numbers| (rule.as_str(), numbers))
        })
        .collect();
    rules.sort_by_key(|(rule, numbers)| {
        (
            numbers
                .iter()
                .next()
                .map(|number| ordinal(number))
                .unwrap_or_default(),
            rule.to_string(),
        )
    });
    rules
}

fn claim_options(content: &Content, rules: &[(&str, &BTreeSet<String>)]) -> Vec<Choice> {
    rules
        .iter()
        .map(|(rule, numbers)| {
            let first = numbers.iter().next().expect("a number reaches the rule");
            Choice::new(
                format!(
                    "{}  {}  {}",
                    first,
                    content
                        .titles
                        .get(*rule)
                        .map(String::as_str)
                        .unwrap_or(*rule),
                    rule_name(rule)
                ),
                content.descriptions.get(*rule).cloned().unwrap_or_default(),
            )
            .within(group(first))
        })
        .collect()
}

/// The `set conforms` flow: the image, the profile, and the components the
/// offer brings to claim the profile's rules.
pub struct Conforms {
    declaration: crate::conformance::Conforms,
    bring: Option<crate::import::Component>,
}

impl Conforms {
    /// Which image, then which profile out of the content a scan of it would
    /// read, then whether to bring the components claiming its rules. Leaving
    /// either picker is `None` and writes nothing.
    pub fn collect(
        root: &Path,
        named: Option<String>,
        datastream: Option<PathBuf>,
        prompt: &Prompt,
        say: crate::report::Say<'_>,
    ) -> Result<Option<Self>, Error> {
        let crate::Loaded { list, .. } = crate::load(root);
        // The offer may bring a library component, so it reads the libraries
        // already on this machine. Resolution reads none.
        let disk = crate::parse::disk::Disk::scan(root);
        let index = crate::provider::Index::scan(root, &list.sources, &disk, false);
        if list.images.is_empty() {
            return Err(
                "`set conforms` needs an image; run `tect create image <name>` first"
                    .to_string()
                    .into(),
            );
        }
        let ids: Vec<&str> = list.images.iter().map(|image| image.id.as_str()).collect();
        let at = match named {
            Some(name) => list
                .images
                .iter()
                .position(|image| image.id == name)
                .ok_or_else(|| {
                    Error::Invocation(format!(
                        "`{name}` is not a declared image; there is {}",
                        ids.join(", ")
                    ))
                })?,
            None if list.images.len() == 1 => 0,
            None => {
                let options: Vec<Choice> = list
                    .images
                    .iter()
                    .map(|image| match image.name == image.id {
                        true => Choice::new(&image.id, ""),
                        false => Choice::new(&image.id, &image.name),
                    })
                    .collect();
                match prompt.choose(copy::MEASURED_IMAGE, &options)? {
                    Some(at) => at,
                    None => return Ok(None),
                }
            }
        };
        let image = &list.images[at];

        let (named, family) = crate::scap::named_by(image.base.as_ref());
        let path = crate::scap::content_path(named, family, datastream.as_deref())?;
        let content = crate::scap::content_of(&path)?;
        if content.profiles.is_empty() {
            return Err(format!(
                "{} carries no profile to be measured against",
                path.display()
            )
            .into());
        }

        say(&format!(
            "{}\n\n",
            crate::conformance::cost(&format!("`{}`", image.id), list.audit_enforce)
        ));
        let options = profile_options(&content);
        let Some(chosen) = prompt.choose(copy::WHICH_PROFILE, &options)? else {
            return Ok(None);
        };
        let profile = &content.profiles[chosen];

        let bring = offer(image, &content, profile, &index, prompt)?;
        let bring = match bring.is_empty() {
            true => None,
            false => Some(crate::import::bring(
                root,
                &list.sources,
                list.audit_enforce,
                &bring,
                &image.id,
            )?),
        };
        let brought = bring
            .as_ref()
            .map(crate::import::Component::brought)
            .unwrap_or_default();
        Ok(Some(Self {
            declaration: crate::conformance::Conforms::declaring(image, profile.name(), brought),
            bring,
        }))
    }

    /// The import first, since it writes into the same file, then the
    /// declaration over whatever is there now.
    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        let mut wrote = match &self.bring {
            Some(bring) => bring.write(root, &crate::model::image::List::load(root).0.sources)?,
            None => Vec::new(),
        };
        wrote.extend(self.declaration.apply(root)?);
        Ok(wrote)
    }
}

/// Which components elsewhere claim rules the profile selects that the image does
/// not have, as the question whether to bring them. A claimant the repository
/// owns needs a line, which `check`'s conformance notice already says.
fn offer(
    image: &crate::model::image::Image,
    content: &Content,
    profile: &crate::scap::Profile,
    index: &crate::provider::Index,
    prompt: &Prompt,
) -> Result<Vec<String>, String> {
    let owed = crate::scap::owed(image, content, profile, index);
    let named: Vec<String> = owed
        .helping
        .iter()
        .filter(|provider| provider.owner.is_some())
        .map(|provider| provider.qualified())
        .collect();
    if named.is_empty() {
        return Ok(Vec::new());
    }
    match prompt.confirm(copy::IMPORT_CLAIMING, copy::YES, copy::NO)? {
        true => Ok(named),
        false => Ok(Vec::new()),
    }
}

/// The same, for the rules a component claims: the manifest was always the
/// interface, and a claim is a number read out of a datastream or nothing.
pub const CLAIMS_BY_HAND: &str = "nothing can be asked here; write `satisfies { <benchmark> \
                                  \"<number>\" }` into the component, and `tect scap content` \
                                  prints the datastream those numbers are read out of";

/// The benchmark numbers one component claims, chosen out of the rules a profile
/// selects. A claim is recorded here and measured by `tect scap`; nothing in
/// this command reads a scan.
pub struct Claims {
    /// The manifest, from the repository root.
    file: PathBuf,
    /// What the numbers are written under, which is the profile they were
    /// chosen out of and is decorative: a number resolves against the content,
    /// never against this.
    profile: String,
    /// Every number the block will hold, chosen or kept.
    numbers: Vec<String>,
    /// How many of the profile's rules were chosen, which is what the tree
    /// reads out.
    claimed: usize,
}

impl Claims {
    /// Which profile the rules come out of, then which of them this component
    /// claims, opening on what it already declares. Leaving either picker is
    /// `None` and writes nothing.
    pub fn collect(
        root: &Path,
        named: &str,
        datastream: Option<PathBuf>,
        prompt: &Prompt,
        say: crate::report::Say<'_>,
    ) -> Result<Option<Self>, Error> {
        let file = Path::new(layout::COMPONENTS)
            .join(named)
            .join(layout::MANIFEST_FILE);
        if !root.join(&file).is_file() {
            return Err(Error::Invocation(format!(
                "`{named}` is not a component this repository holds; there is no {}",
                file.display()
            )));
        }
        let summary = parse::component::summary(&root.join(&file));
        let family = match summary.targets.first() {
            Some(family) => family.as_str(),
            None if datastream.is_some() => "",
            None => {
                return Err(Error::Invocation(format!(
                    "`{named}` declares no `targets` restriction, so it has no default SCAP \
                     content; give `--datastream <file>`"
                )))
            }
        };
        let path = crate::scap::content_path("", family, datastream.as_deref())?;
        let content = crate::scap::content_of(&path)?;
        if content.profiles.is_empty() {
            return Err(format!("{} carries no profile to claim against", path.display()).into());
        }

        let options = profile_options(&content);
        let Some(chosen) = prompt.choose(copy::WHICH_PROFILE, &options)? else {
            return Ok(None);
        };
        let profile = &content.profiles[chosen];

        // A rule no number reaches is one nothing can claim, so it is left out.
        // A row drawn for it would write nothing.
        let selected = content.selected(&profile.id);
        let rules = claimable_rules(&content, &selected);
        if rules.is_empty() {
            return Err(format!(
                "no rule `{}` selects carries a number, so nothing can be claimed against it",
                profile.name()
            )
            .into());
        }
        match selected.len() - rules.len() {
            0 => {}
            1 => say(&format!(
                "One of the rules `{}` selects is reached by no number of its own, so no \
                 `satisfies` can name it.\n\n",
                profile.name()
            )),
            missed => say(&format!(
                "{missed} of the rules `{}` selects are reached by no number of their own, so no \
                 `satisfies` can name them.\n\n",
                profile.name()
            )),
        }

        let first = |at: usize| rules[at].1.iter().next().expect("a number reaches it");
        let options = claim_options(&content, &rules);
        let held = crate::scap::reached(&content, summary.satisfies.iter());
        let on: Vec<usize> = (0..rules.len())
            .filter(|at| held.contains(rules[*at].0))
            .collect();

        let Answer::Chosen(chosen) =
            prompt.choose_many(&copy::claimed_rules(named), &options, &on)?
        else {
            return Ok(None);
        };

        // A claim the picker never offered is one about another profile, and
        // replacing the block wholesale would drop it.
        let offered: BTreeSet<&String> = rules
            .iter()
            .flat_map(|(_, numbers)| numbers.iter())
            .collect();
        let mut numbers: Vec<String> = summary
            .satisfies
            .iter()
            .filter(|number| !offered.contains(number))
            .cloned()
            .collect();
        numbers.extend(chosen.iter().map(|at| first(*at).clone()));
        numbers.sort_by_key(|number| (ordinal(number), number.clone()));
        numbers.dedup();
        Ok(Some(Self {
            file,
            profile: profile.name().to_string(),
            numbers,
            claimed: chosen.len(),
        }))
    }

    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        let file = root.join(&self.file);
        let text =
            std::fs::read_to_string(&file).map_err(|err| format!("{}: {err}", file.display()))?;
        std::fs::write(&file, self.spliced(&text))
            .map_err(|err| format!("{}: {err}", file.display()))?;
        Ok(vec![(
            self.file.clone(),
            Change::Updated(match self.claimed {
                0 => format!("claiming no rule of `{}`", self.profile),
                1 => format!("claiming one rule of `{}`", self.profile),
                many => format!("claiming {many} rules of `{}`", self.profile),
            }),
        )])
    }

    fn spliced(&self, text: &str) -> String {
        splice(text, parse::component::satisfies_span(text), &self.block())
    }

    /// One benchmark node, since declaring one twice is a diagnostic, and one
    /// number per line through KDL's own continuation. Claiming nothing declares
    /// no block.
    fn block(&self) -> String {
        match self.numbers.is_empty() {
            true => String::new(),
            false => format!(
                "satisfies {{\n    {} {}\n}}",
                self.profile,
                self.numbers
                    .iter()
                    .map(|number| format!("\"{number}\""))
                    .collect::<Vec<String>>()
                    .join(" \\\n        ")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-line `sources` block is valid KDL, and a source added to it stays
    /// inside the block instead of duplicating the line.
    #[test]
    fn a_one_line_sources_block_takes_a_source() {
        let library = Library {
            kind: crate::model::remote::Kind::Components,
            alias: "core".to_string(),
            at: LibraryAt::Default(crate::collection::default_library(
                crate::model::remote::Kind::Components,
            )),
        };
        let out = library
            .spliced("schema-version 1\n\nsources { components \"one\" { dir \"../one\" } }\n")
            .expect("the splice is text");
        let issues = crate::parse::schema::check_text(&out, &crate::parse::repo::REPO, false)
            .unwrap_or_else(|err| panic!("{err}: {out:?}"));
        assert!(issues.is_empty(), "{out}\n{}", issues.plain());
    }

    /// A number with no dot has no prefix to nest under, so every one of them
    /// shares the one flat branch.
    #[test]
    fn a_number_that_does_not_nest_goes_in_the_one_flat_branch() {
        assert_eq!(group("1.1.1.1"), "1.1.1");
        assert_eq!(group("5.2"), "5");
        assert_eq!(group("RHEL-09-232010"), "other");
    }

    fn claims(profile: &str, numbers: &[&str]) -> Claims {
        Claims {
            file: PathBuf::from("components/one/manifest.kdl"),
            profile: profile.to_string(),
            numbers: numbers.iter().map(|n| n.to_string()).collect(),
            claimed: numbers.len(),
        }
    }

    /// SCAP content is family-specific, so a base-agnostic component has no
    /// installed default.
    #[test]
    fn base_agnostic_claims_need_a_datastream() {
        let root = std::env::temp_dir().join(format!("tect-claims-any-{}", std::process::id()));
        let component = root.join("components/editor");
        std::fs::create_dir_all(&component).unwrap();
        std::fs::write(
            component.join(layout::MANIFEST_FILE),
            "schema-version 1\n\npackages \"nano\"\n",
        )
        .unwrap();

        let result = Claims::collect(&root, "editor", None, &Prompt::silent(), &mut |_| {});
        let Err(err) = result else {
            panic!("base-agnostic claims unexpectedly chose content");
        };
        assert!(
            err.message().contains("has no default SCAP content")
                && err.message().contains("--datastream <file>"),
            "{}",
            err.message()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The block replaces the one that was there, and every number goes on its
    /// own continued line under the one benchmark node.
    #[test]
    fn the_claims_replace_the_block_that_was_there() {
        assert_eq!(
            claims("ospp", &["1.1.1.1", "1.1.1.2"]).spliced(
                "description \"one\"\n\nsatisfies {\n    ospp \"9.9\"\n}\n\npackages \"curl\"\n"
            ),
            "description \"one\"\n\nsatisfies {\n    ospp \"1.1.1.1\" \\\n        \"1.1.1.2\"\n}\n\npackages \"curl\"\n"
        );
    }

    /// A component that declared none gets the block at the end.
    #[test]
    fn a_component_with_no_block_gets_one_at_the_end() {
        assert_eq!(
            claims("ospp", &["1.1.1.1"]).spliced("description \"one\"\n"),
            "description \"one\"\n\nsatisfies {\n    ospp \"1.1.1.1\"\n}\n"
        );
    }

    /// Claiming nothing takes the block away, and the blank line above it with
    /// it.
    #[test]
    fn claiming_nothing_takes_the_block_and_its_blank_line() {
        assert_eq!(
            claims("ospp", &[])
                .spliced("description \"one\"\n\nsatisfies {\n    ospp \"9.9\"\n}\n"),
            "description \"one\"\n"
        );
    }

    /// `coverages` refuses a benchmark with no name and `summary` keeps
    /// its numbers, so a number carried over from one comes back under the
    /// profile as a claim the picker never showed. The two halves `collect`
    /// runs between are the read and the write, and this is both of them.
    #[test]
    fn an_unnamed_benchmark_comes_back_named_after_the_profile() {
        let manifest = "description \"one\"\n\nsatisfies {\n    \"\" \"1.1.1.1\"\n}\n";
        let dir = std::env::temp_dir().join(format!("tect-claims-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(layout::MANIFEST_FILE);
        std::fs::write(&file, manifest).unwrap();

        // What `collect` carries over: the lenient read keeps a number the
        // strict one refused, and no picker ever showed it.
        let held = parse::component::summary(&file).satisfies;
        assert_eq!(held, ["1.1.1.1"]);
        std::fs::remove_dir_all(dir).unwrap();

        let carried: Vec<&str> = held.iter().map(String::as_str).collect();
        assert_eq!(
            claims("ospp", &carried).spliced(manifest),
            "description \"one\"\n\nsatisfies {\n    ospp \"1.1.1.1\"\n}\n"
        );
    }

    #[test]
    fn profile_and_filtered_claim_screens_snapshot_datastream_content() {
        use crate::screen_tests::{assert_screen, screen};

        let content = crate::scap::content_of(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scap/datastream.xml"),
        )
        .unwrap();
        let profiles = profile_options(&content);
        assert_screen(
            "profile-picker",
            screen().picker(copy::WHICH_PROFILE, &profiles, None, "enter confirms", 0),
        );

        let profile = content
            .profiles
            .iter()
            .find(|profile| profile.name() == "standard")
            .expect("the fixture carries the standard profile");
        let selected = content.selected(&profile.id);
        let rules = claimable_rules(&content, &selected);
        let options = claim_options(&content, &rules);
        let filtered: Vec<Choice> = options
            .into_iter()
            .filter(|option| option.label.contains("aide"))
            .collect();
        assert_eq!(filtered.len(), 1, "the aide filter identifies one rule");
        assert_screen(
            "claims-filtered-tree",
            screen().picker(
                &copy::claimed_rules("sshd"),
                &filtered,
                Some(&[0]),
                "filter, space toggles, enter confirms",
                0,
            ),
        );
    }
}

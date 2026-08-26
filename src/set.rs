//! `tect set`, the config family. A question answered into a declaration file,
//! which `generate` then acts on: nothing here writes an artifact.
//!
//! Collect-then-apply like every other flow, so `create repo` holds one of
//! these rather than calling the command.

use crate::dispatch::Error;
use crate::prompt::Prompt;
use crate::resolve::workflow::{Basis, Shipped, DEFAULT_AT, SHIPPED};
use crate::scap::{ordinal, rule_name, Content};
use crate::ui::tree::Change;
use crate::ui::{Answer, Choice};
use crate::{layout, parse};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// What to do instead, where there is nobody to ask. The declaration file was
/// always the interface, so a flag writing the same line would be a second one.
pub const BY_HAND: &str = "nothing can be asked here; edit the `workflows` block in repo.kdl, \
                           then run `tect generate`";

/// The same, for the profile an image is measured against: the image file was
/// always the interface, and there is a datastream to pick out of or there is
/// not.
pub const CONFORMS_BY_HAND: &str = "nothing can be asked here; write `conforms \"<profile>\"` \
                                    into the image, and `tect scap content` prints the \
                                    datastream it is then measured with";

/// The same, for the rules a module claims: the manifest was always the
/// interface, and a claim is a number read out of a datastream or nothing.
pub const CLAIMS_BY_HAND: &str = "nothing can be asked here; write `satisfies { <benchmark> \
                                  \"<number>\" }` into the module, and `tect scap content` \
                                  prints the datastream those numbers are read out of";

/// A top-level block put back where it was, else at the end of the file.
/// Taking one away takes the blank line above it with it.
fn splice(text: &str, was: Option<crate::diag::Span>, block: &str) -> String {
    let mut out = text.to_string();
    match was {
        Some(was) => {
            let end = was.offset + was.len;
            let start = match block.is_empty() {
                true => text[..was.offset].trim_end_matches(['\n', ' ']).len(),
                false => was.offset,
            };
            out.replace_range(start..end, block);
        }
        None if block.is_empty() => {}
        None => {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&format!("\n{block}\n"));
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Which CI the repository generates, and the one time the schedules hang off.
pub struct Workflows {
    chosen: Vec<&'static str>,
    at: (u32, u32),
    scans_scheduled: bool,
}

impl Workflows {
    /// Every workflow the repository can run, which is what a scaffold opens
    /// with and what nobody to ask is answered with.
    pub fn every(basis: &Basis) -> Vec<&'static str> {
        SHIPPED
            .iter()
            .filter(|shipped| shipped.met(basis))
            .map(|shipped| shipped.stem)
            .collect()
    }

    /// What the repository already declares, with `more` turned on as well:
    /// the block a question answered somewhere else edits.
    pub fn adding(list: &crate::model::image::List, more: &[&'static str]) -> Self {
        Self {
            chosen: SHIPPED
                .iter()
                .filter(|shipped| {
                    more.contains(&shipped.stem)
                        || list.workflows.iter().any(|w| w.name == shipped.stem)
                })
                .map(|shipped| shipped.stem)
                .collect(),
            at: list.workflows_at,
            scans_scheduled: list.scans_scheduled,
        }
    }

    /// `on` is what is already true. Answering with nothing is a repository
    /// that generates no CI, and leaving is `None`.
    pub fn collect(
        basis: &Basis,
        on: &[&str],
        at: (u32, u32),
        prompt: &Prompt,
    ) -> Result<Option<Self>, String> {
        let options: Vec<Choice> = SHIPPED
            .iter()
            .map(|shipped| match shipped.met(basis) {
                true => Choice::new(shipped.stem, shipped.about),
                false => Choice::new(shipped.stem, shipped.needs.unmet()),
            })
            .collect();
        let held: Vec<usize> = SHIPPED
            .iter()
            .enumerate()
            .filter(|(_, shipped)| on.contains(&shipped.stem))
            .map(|(at, _)| at)
            .collect();

        let Answer::Chosen(chosen) = prompt.choose_many("the CI to generate", &options, &held)?
        else {
            return Ok(None);
        };
        let chosen: Vec<&'static Shipped> = chosen.iter().map(|at| &SHIPPED[*at]).collect();
        if let Some(short) = chosen.iter().find(|shipped| !shipped.met(basis)) {
            return Err(format!("`{}` {}", short.stem, short.needs.unmet()));
        }

        let scans_scheduled = chosen.iter().any(|shipped| shipped.stem == "build")
            && prompt.confirm("run image scans only on scheduled builds", "Yes", "No")?;

        let at = match chosen.iter().any(|shipped| shipped.at.is_some()) {
            false => at,
            true => {
                let asked = prompt.text(
                    None,
                    "what time the daily build runs, UTC",
                    BY_HAND,
                    Some(&parse::repo::at_text(at)),
                )?;
                parse::repo::time(&asked)
                    .ok_or_else(|| format!("`{asked}` is not a time of day: `HH:MM`"))?
            }
        };
        Ok(Some(Self {
            chosen: chosen.iter().map(|shipped| shipped.stem).collect(),
            at,
            scans_scheduled,
        }))
    }

    /// The block written into repo.kdl, replacing the one that was there.
    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        let file = root.join(layout::REPO_FILE);
        let text =
            std::fs::read_to_string(&file).map_err(|err| format!("{}: {err}", file.display()))?;
        std::fs::write(&file, self.spliced(&text))
            .map_err(|err| format!("{}: {err}", file.display()))?;
        Ok(vec![(
            PathBuf::from(layout::REPO_FILE),
            Change::Updated("the workflows it generates".to_string()),
        )])
    }

    /// In place of the block that was there, else at the end. A repository
    /// generating nothing declares no block, since an empty one is refused.
    fn spliced(&self, text: &str) -> String {
        let block = match self.chosen.is_empty() {
            true => String::new(),
            false => {
                let named: String = self
                    .chosen
                    .iter()
                    .map(|stem| format!("    {stem}\n"))
                    .collect();
                let at = match self.at == DEFAULT_AT {
                    true => String::new(),
                    false => format!(" at=\"{}\"", parse::repo::at_text(self.at)),
                };
                let scan = match self.scans_scheduled {
                    true => " scan=\"scheduled\"",
                    false => "",
                };
                format!("workflows{at}{scan} {{\n{named}}}")
            }
        };
        splice(text, parse::repo::workflows_span(text), &block)
    }
}

/// A `conforms` is the scan gate, so writing one costs a scan on every build,
/// and in an enforcing repository it costs the build itself. `measured` is the
/// subject already quoted, since an offer elsewhere may name more than one
/// image.
pub(crate) fn cost(measured: &str, enforce: bool) -> String {
    match enforce {
        false => format!(
            "A `conforms` turns the scan on: every build measures {measured} against the profile \
             and publishes the score."
        ),
        true => format!(
            "A `conforms` turns the scan on, and this repository enforces: every build measures \
             {measured} against the profile, and a rule it fails fails the build."
        ),
    }
}

/// The benchmark profile one image is measured against, and whatever claiming
/// modules the offer alongside it brought.
pub struct Conforms {
    /// The image file, as the image was read from.
    file: PathBuf,
    /// The image's `name`, which is what the writer walks to.
    image: String,
    profile: String,
    brought: Vec<String>,
    bring: Option<crate::import::Module>,
}

impl Conforms {
    /// The same declaration made out of an offer somewhere else: one image, one
    /// profile already chosen, and the names the tree says arrived with it.
    pub(crate) fn declaring(
        image: &crate::model::image::Image,
        profile: &str,
        brought: Vec<String>,
    ) -> Self {
        Self {
            file: PathBuf::from(image.src.name()),
            image: image.name.clone(),
            profile: profile.to_string(),
            brought,
            bring: None,
        }
    }

    /// Which image, then which profile out of the content a scan of it would
    /// read, then whether to bring the modules claiming its rules. Leaving
    /// either picker is `None` and writes nothing.
    pub fn collect(
        root: &Path,
        named: Option<String>,
        datastream: Option<PathBuf>,
        prompt: &Prompt,
    ) -> Result<Option<Self>, Error> {
        let crate::Loaded { list, index, .. } = crate::load(root);
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
                match prompt.choose("which image is measured", &options)? {
                    Some(at) => at,
                    None => return Ok(None),
                }
            }
        };
        let image = &list.images[at];

        let family = image.base.as_ref().map_or("", |base| base.family.as_str());
        let path = crate::scap::content_path(family, datastream.as_deref())?;
        let content = crate::scap::content_of(&path)?;
        if content.profiles.is_empty() {
            return Err(format!(
                "{} carries no profile to be measured against",
                path.display()
            )
            .into());
        }

        println!("{}\n", cost(&format!("`{}`", image.id), list.audit_enforce));
        let options: Vec<Choice> = content
            .profiles
            .iter()
            .map(|profile| Choice::new(profile.name(), &profile.title))
            .collect();
        let Some(chosen) = prompt.choose("which profile", &options)? else {
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
        Ok(Some(Self {
            file: PathBuf::from(image.src.name()),
            image: image.name.clone(),
            profile: profile.name().to_string(),
            brought: bring
                .as_ref()
                .map(crate::import::Module::brought)
                .unwrap_or_default(),
            bring,
        }))
    }

    /// The import first, since it writes into the same file, then the
    /// declaration over whatever is there now.
    pub fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        let mut wrote = match &self.bring {
            Some(bring) => bring.write(root)?,
            None => Vec::new(),
        };
        let text = std::fs::read_to_string(&self.file)
            .map_err(|err| format!("{}: {err}", self.file.display()))?;
        let spliced = self.spliced(&text)?;
        std::fs::write(&self.file, spliced)
            .map_err(|err| format!("{}: {err}", self.file.display()))?;
        // Last, so the one line the tree draws for the file says both edits.
        wrote.push((
            self.file.clone(),
            Change::Updated(match self.brought.is_empty() {
                true => format!("measured against `{}`", self.profile),
                false => format!(
                    "measured against `{}`, and {} added to modules",
                    self.profile,
                    crate::import::said(&self.brought)
                ),
            }),
        ));
        Ok(wrote)
    }

    /// In place of the declaration that was there, else in front of `base`,
    /// which is where the schema lists it.
    fn spliced(&self, text: &str) -> Result<String, String> {
        let was = parse::image::conforms_span(text, &self.image)
            .ok_or_else(|| format!("{} declares no image `{}`", self.file.display(), self.image))?;
        let indent = &text[text[..was.offset].rfind('\n').map_or(0, |at| at + 1)..was.offset];
        let mut out = text.to_string();
        out.replace_range(
            was.offset..was.offset + was.len,
            &match was.len {
                0 => format!("conforms \"{}\"\n\n{indent}", self.profile),
                _ => format!("conforms \"{}\"", self.profile),
            },
        );
        Ok(out)
    }
}

/// The branch a benchmark number sits in: its dotted prefix, and one flat
/// branch for a number with no dot to group it by, since a STIG ident nests
/// nowhere.
fn group(number: &str) -> &str {
    number.rsplit_once('.').map_or("other", |(head, _)| head)
}

/// The benchmark numbers one module claims, chosen out of the rules a profile
/// selects rather than typed. A claim is recorded here and measured by `tect
/// scap`; nothing in this command reads a scan.
pub struct Claims {
    /// The manifest, from the repository root.
    file: PathBuf,
    /// What the numbers are written under, which is the profile they were
    /// chosen out of and is decorative: a number resolves against the content,
    /// never against this. The family is not folded in, because `supports`
    /// already declares it and a second spelling can disagree with the first.
    profile: String,
    /// Every number the block will hold, chosen or kept.
    numbers: Vec<String>,
    /// How many of the profile's rules were chosen, which is what the tree
    /// reads out.
    claimed: usize,
}

impl Claims {
    /// Which profile the rules come out of, then which of them this module
    /// claims, opening on what it already declares. Leaving either picker is
    /// `None` and writes nothing.
    pub fn collect(
        root: &Path,
        named: &str,
        datastream: Option<PathBuf>,
        prompt: &Prompt,
    ) -> Result<Option<Self>, Error> {
        let file = Path::new(layout::MODULES)
            .join(named)
            .join(layout::MODULE_FILE);
        if !root.join(&file).is_file() {
            return Err(Error::Invocation(format!(
                "`{named}` is not a module this repository holds; there is no {}",
                file.display()
            )));
        }
        let summary = parse::module::summary(&root.join(&file));
        let family = summary.supports.first().map_or("", String::as_str);
        let path = crate::scap::content_path(family, datastream.as_deref())?;
        let content = crate::scap::content_of(&path)?;
        if content.profiles.is_empty() {
            return Err(format!("{} carries no profile to claim against", path.display()).into());
        }

        let options: Vec<Choice> = content
            .profiles
            .iter()
            .map(|profile| Choice::new(profile.name(), &profile.title))
            .collect();
        let Some(chosen) = prompt.choose("which profile", &options)? else {
            return Ok(None);
        };
        let profile = &content.profiles[chosen];

        // A rule no number reaches is one nothing can claim, so it is left out
        // rather than drawn as a row that would write nothing.
        let selected = content.selected(&profile.id);
        let mut rules: Vec<(&String, &BTreeSet<String>)> = selected
            .iter()
            .filter_map(|rule| content.numbers.get(rule).map(|numbers| (rule, numbers)))
            .collect();
        rules.sort_by_key(|(rule, numbers)| {
            (
                numbers
                    .iter()
                    .next()
                    .map(|n| ordinal(n))
                    .unwrap_or_default(),
                (*rule).clone(),
            )
        });
        if rules.is_empty() {
            return Err(format!(
                "no rule `{}` selects carries a number, so nothing can be claimed against it",
                profile.name()
            )
            .into());
        }
        match selected.len() - rules.len() {
            0 => {}
            missed => println!(
                "{missed} of the rules `{}` selects are reached by no number of their own, so no \
                 `satisfies` can name them.\n",
                profile.name()
            ),
        }

        let first = |at: usize| rules[at].1.iter().next().expect("a number reaches it");
        let options: Vec<Choice> = (0..rules.len())
            .map(|at| {
                let (rule, _) = rules[at];
                Choice::new(
                    format!(
                        "{}  {}  {}",
                        first(at),
                        content.titles.get(rule).unwrap_or(rule),
                        rule_name(rule)
                    ),
                    content.descriptions.get(rule).cloned().unwrap_or_default(),
                )
                .within(group(first(at)))
            })
            .collect();
        let held = crate::scap::reached(&content, summary.satisfies.iter());
        let on: Vec<usize> = (0..rules.len())
            .filter(|at| held.contains(rules[*at].0))
            .collect();

        let Answer::Chosen(chosen) =
            prompt.choose_many(&format!("which rules `{named}` claims"), &options, &on)?
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
        let spliced = splice(&text, parse::module::satisfies_span(&text), &self.block());
        std::fs::write(&file, spliced).map_err(|err| format!("{}: {err}", file.display()))?;
        Ok(vec![(
            self.file.clone(),
            Change::Updated(match self.claimed {
                0 => format!("claiming no rule of `{}`", self.profile),
                1 => format!("claiming one rule of `{}`", self.profile),
                many => format!("claiming {many} rules of `{}`", self.profile),
            }),
        )])
    }

    /// One benchmark node, since declaring one twice is a diagnostic, and one
    /// number per line through KDL's own continuation, since a claim is read
    /// and reviewed a number at a time. Claiming nothing declares no block, the
    /// way generating no CI declares none.
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

/// Which modules elsewhere claim rules the profile selects that the image does
/// not have, as the question whether to bring them. A claimant the repository
/// owns needs a line rather than an import, which is what `check`'s own
/// conformance notice already says.
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
    let question = format!(
        "{} {} {} of the {} rules `{}` selects that nothing `{}` lists claims.\nImport {} as well?",
        crate::import::said(&named.iter().map(|at| format!("`{at}`")).collect::<Vec<_>>()),
        match named.len() {
            1 => "claims",
            _ => "claim",
        },
        owed.covered.len(),
        owed.open.len(),
        profile.name(),
        image.id,
        match named.len() {
            1 => "it",
            _ => "them",
        },
    );
    match prompt.confirm(&question, "Yes", "No")? {
        true => Ok(named),
        false => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conforms(profile: &str) -> Conforms {
        Conforms {
            file: PathBuf::from("example.image.kdl"),
            image: "Example".to_string(),
            profile: profile.to_string(),
            brought: Vec::new(),
            bring: None,
        }
    }

    /// The declaration replaces the one that was there and goes in front of
    /// `base` otherwise, which is where the schema lists it.
    #[test]
    fn the_profile_replaces_the_one_that_was_there() {
        let text =
            "image {\n    name \"Example\"\n\n    conforms \"standard\"\n\n    base \"x\" {\n    }\n}\n";
        assert_eq!(
            conforms("ospp").spliced(text).unwrap(),
            text.replace("\"standard\"", "\"ospp\"")
        );
    }

    /// An image with no `base` for it to stand in front of still gets one.
    #[test]
    fn an_image_with_nothing_to_stand_in_front_of_still_takes_one() {
        assert_eq!(
            conforms("standard")
                .spliced("image {\n    name \"Example\"\n}\n")
                .unwrap(),
            "image {\n    name \"Example\"\nconforms \"standard\"\n\n}\n"
        );
    }

    /// A number with no dot has no prefix to nest under, so every one of them
    /// shares the one flat branch.
    #[test]
    fn a_number_that_does_not_nest_goes_in_the_one_flat_branch() {
        assert_eq!(group("1.1.1.1"), "1.1.1");
        assert_eq!(group("5.2"), "5");
        assert_eq!(group("RHEL-09-232010"), "other");
    }

    fn set(chosen: &[&'static str], at: (u32, u32)) -> Workflows {
        Workflows {
            chosen: chosen.to_vec(),
            at,
            scans_scheduled: false,
        }
    }

    #[test]
    fn the_block_replaces_the_one_that_was_there_and_leaves_the_rest() {
        let text = "name \"Example\"\n\nworkflows {\n    build\n}\n\nsources {\n}\n";
        let out = set(&["build", "smoke-test"], DEFAULT_AT).spliced(text);
        assert_eq!(
            out,
            "name \"Example\"\n\nworkflows {\n    build\n    smoke-test\n}\n\nsources {\n}\n"
        );
    }

    #[test]
    fn a_repository_with_no_block_gets_one_at_the_end() {
        let out = set(&["build"], (6, 0)).spliced("name \"Example\"\n");
        assert_eq!(
            out,
            "name \"Example\"\n\nworkflows at=\"06:00\" {\n    build\n}\n"
        );
    }

    /// Generating nothing is an absent block: an empty one is a diagnostic.
    #[test]
    fn choosing_nothing_takes_the_block_away() {
        let text = "name \"Example\"\n\nworkflows {\n    build\n}\n";
        assert_eq!(set(&[], DEFAULT_AT).spliced(text), "name \"Example\"\n");
    }
}

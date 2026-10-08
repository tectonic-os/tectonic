//! `tect set`, the config family. A question answered into a declaration file,
//! which `generate` then acts on: nothing here writes an artifact.
//!
//! Collect-then-apply like every other flow, so `create repo` holds one of
//! these.

use crate::copy;
use crate::create::Field;
use crate::dispatch::Error;
use crate::resolve::workflow::{Basis, Shipped, DEFAULT_AT, SHIPPED};
use crate::scap::{ordinal, rule_name, Content};
use crate::{layout, parse};
use common::prompt::Prompt;
use common::ui::tree::Change;
use common::ui::{Answer, Choice};
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

/// The whitespace before `at`, which is the line's indent where `at` stands on
/// that line's own node and empty where the line holds code before it.
fn line_indent(text: &str, at: usize) -> &str {
    let start = text[..at].rfind('\n').map_or(0, |found| found + 1);
    let line = &text[start..at];
    &line[..line.len() - line.trim_start().len()]
}

/// Which CI the repository generates, and the one time the schedules hang off.
pub struct Workflows {
    chosen: Vec<&'static str>,
    at: (u32, u32),
    publishes_scheduled: bool,
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
            publishes_scheduled: list.publishes_scheduled,
            scans_scheduled: list.scans_scheduled,
        }
    }

    /// `on` is what is already true. Answering with nothing is a repository
    /// that generates no CI, and leaving is `None`.
    /// `from` is the review screen's re-entry point, which a caller with no
    /// review screen answers with `Field::Workflows`: ask all three.
    pub fn collect(
        basis: &Basis,
        on: &[&str],
        at: (u32, u32),
        publishes_current: bool,
        scans_current: bool,
        from: Field,
        prompt: &Prompt,
    ) -> Result<Option<Self>, String> {
        let chosen: Vec<&'static Shipped> = match from > Field::Workflows {
            true => SHIPPED
                .iter()
                .filter(|shipped| on.contains(&shipped.stem))
                .collect(),
            false => {
                let options = workflow_options(basis);
                // An edited base, or a declaration written before one changed,
                // can put a chosen workflow out of reach. It is cleared, and the
                // clearing is said aloud.
                for shipped in SHIPPED
                    .iter()
                    .filter(|shipped| on.contains(&shipped.stem) && !shipped.met(basis))
                {
                    println!("{}", copy::cleared(shipped.stem, shipped.needs.unmet()));
                }
                let held: Vec<usize> = SHIPPED
                    .iter()
                    .enumerate()
                    .filter(|(_, shipped)| on.contains(&shipped.stem) && shipped.met(basis))
                    .map(|(at, _)| at)
                    .collect();

                let Answer::Chosen(chosen) =
                    prompt.choose_many(copy::WORKFLOWS, &options, &held)?
                else {
                    return Ok(None);
                };
                chosen.iter().map(|at| &SHIPPED[*at]).collect()
            }
        };
        if let Some(short) = chosen.iter().find(|shipped| !shipped.met(basis)) {
            return Err(format!("`{}` {}", short.stem, short.needs.unmet()));
        }

        let builds = chosen.iter().any(|shipped| shipped.stem == "build");
        let publishes_scheduled = builds
            && match from > Field::Publish {
                true => publishes_current,
                false => prompt.confirm_current(
                    copy::PUBLISH_SCHEDULED,
                    copy::YES,
                    copy::NO,
                    publishes_current,
                )?,
            };
        let scans_scheduled = match (builds, publishes_scheduled) {
            (false, _) => false,
            (true, true) => scans_current,
            (true, false) => match from > Field::Scans {
                true => scans_current,
                false => prompt.confirm_current(
                    copy::SCAN_SCHEDULED,
                    copy::YES,
                    copy::NO,
                    scans_current,
                )?,
            },
        };

        let at = match chosen.iter().any(|shipped| shipped.at.is_some()) {
            false => at,
            true => {
                let asked = prompt.text(
                    None,
                    copy::DAILY_AT,
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
            publishes_scheduled,
            scans_scheduled,
        }))
    }

    /// The same questions asked again, opening on the answers already held.
    /// `basis` is passed on every call, because an edited base changes the
    /// family and so changes which workflows are reachable at all.
    pub fn again(
        &self,
        basis: &Basis,
        from: Field,
        prompt: &Prompt,
    ) -> Result<Option<Self>, String> {
        Self::collect(
            basis,
            &self.chosen,
            self.at,
            self.publishes_scheduled,
            self.scans_scheduled,
            from,
            prompt,
        )
    }

    /// The review screen's rows for the CI, said as the settings repo.kdl will
    /// hold. A row is here only where the question behind it was reachable: no
    /// `build`, no cadence.
    pub fn rows(&self) -> Vec<(Field, &'static str, String)> {
        let mut rows = vec![(
            Field::Workflows,
            copy::ROW_WORKFLOWS,
            match self.chosen.is_empty() {
                true => copy::NONE.to_string(),
                false => self.chosen.join(", "),
            },
        )];
        if !self.chosen.contains(&"build") {
            return rows;
        }
        rows.push((
            Field::Publish,
            copy::ROW_PUBLISH,
            match self.publishes_scheduled {
                true => copy::ON_SCHEDULED,
                false => copy::ON_EVERY_PUSH,
            }
            .to_string(),
        ));
        if !self.publishes_scheduled {
            rows.push((
                Field::Scans,
                copy::ROW_SCANS,
                match self.scans_scheduled {
                    true => copy::ON_SCHEDULED,
                    false => copy::ON_EVERY_BUILD,
                }
                .to_string(),
            ));
        }
        if self
            .chosen
            .iter()
            .any(|stem| SHIPPED.iter().any(|s| s.stem == *stem && s.at.is_some()))
        {
            rows.push((Field::Daily, copy::ROW_DAILY, parse::repo::at_text(self.at)));
        }
        rows
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
                let publish = match self.publishes_scheduled {
                    true => " publish=\"scheduled\"",
                    false => "",
                };
                let scan = match self.scans_scheduled {
                    true => " scan=\"scheduled\"",
                    false => "",
                };
                format!("workflows{at}{publish}{scan} {{\n{named}}}")
            }
        };
        splice(text, parse::repo::workflows_span(text), &block)
    }
}

fn workflow_options(basis: &Basis) -> Vec<Choice> {
    SHIPPED
        .iter()
        .map(|shipped| match shipped.met(basis) {
            true => Choice::new(shipped.stem, shipped.about),
            false => Choice::new(shipped.stem, shipped.needs.unmet()).unavailable(),
        })
        .collect()
}

/// A library added to `repo.kdl`'s `sources`: the default library of its kind
/// or a source the user types. An existing source under the same kind and
/// alias is replaced, so the declaration moves to what the user chose.
pub struct Library {
    kind: crate::model::remote::Kind,
    alias: String,
    at: LibraryAt,
}

enum LibraryAt {
    Default(&'static crate::base::DefaultLibrary),
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
        let default = crate::base::default_library(kind);
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
                    crate::base::LIBRARY_URL,
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
            Some(crate::base::LIBRARY_URL),
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
            LibraryAt::Default(library) => (crate::base::LIBRARY_URL, library.path),
            LibraryAt::Typed { url, path } => (url.as_str(), path.as_str()),
        };
        format!(
            "{} \"{}\" {{\n{indent}    url \"{url}\"\n{indent}    path \"{path}\"\n{indent}}}",
            self.kind.as_str(),
            self.alias
        )
    }
}

/// A `conforms` is the scan gate, so writing one costs a scan on every build,
/// and in an enforcing repository it costs the build itself. `measured` is the
/// subject already quoted, since an offer elsewhere may name more than one image.
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
                match prompt.choose(copy::MEASURED_IMAGE, &options)? {
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
            Some(bring) => bring.write(root, &crate::model::image::List::load(root).0.sources)?,
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

fn profile_options(content: &Content) -> Vec<Choice> {
    content
        .profiles
        .iter()
        .map(|profile| Choice::new(profile.name(), &profile.title))
        .collect()
}

fn claimable_rules<'a>(
    content: &'a Content,
    selected: &'a BTreeSet<String>,
) -> Vec<(&'a String, &'a BTreeSet<String>)> {
    let mut rules: Vec<_> = selected
        .iter()
        .filter_map(|rule| content.numbers.get(rule).map(|numbers| (rule, numbers)))
        .collect();
    rules.sort_by_key(|(rule, numbers)| {
        (
            numbers
                .iter()
                .next()
                .map(|number| ordinal(number))
                .unwrap_or_default(),
            (*rule).clone(),
        )
    });
    rules
}

fn claim_options(content: &Content, rules: &[(&String, &BTreeSet<String>)]) -> Vec<Choice> {
    rules
        .iter()
        .map(|(rule, numbers)| {
            let first = numbers.iter().next().expect("a number reaches the rule");
            Choice::new(
                format!(
                    "{}  {}  {}",
                    first,
                    content.titles.get(*rule).unwrap_or(*rule),
                    rule_name(rule)
                ),
                content.descriptions.get(*rule).cloned().unwrap_or_default(),
            )
            .within(group(first))
        })
        .collect()
}

/// The benchmark numbers one module claims, chosen out of the rules a profile
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
    /// Which profile the rules come out of, then which of them this module
    /// claims, opening on what it already declares. Leaving either picker is
    /// `None` and writes nothing.
    pub fn collect(
        root: &Path,
        named: &str,
        datastream: Option<PathBuf>,
        prompt: &Prompt,
    ) -> Result<Option<Self>, Error> {
        let remote = format!("{}/", crate::model::remote::REMOTE_DIR);
        if let Some(source) = named.strip_prefix(&remote) {
            return Err(Error::Invocation(format!(
                "nothing can be changed here; `{named}` is a fetched module; `tect copy module \
                 {source}` makes a copy this repository owns"
            )));
        }
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
        let family = match summary.supports.first() {
            Some(family) => family.as_str(),
            None if datastream.is_some() => "",
            None => {
                return Err(Error::Invocation(format!(
                    "`{named}` declares no `supports` restriction, so it has no default SCAP \
                     content; give `--datastream <file>`"
                )))
            }
        };
        let path = crate::scap::content_path(family, datastream.as_deref())?;
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
            1 => println!(
                "One of the rules `{}` selects is reached by no number of its own, so no \
                 `satisfies` can name it.\n",
                profile.name()
            ),
            missed => println!(
                "{missed} of the rules `{}` selects are reached by no number of their own, so no \
                 `satisfies` can name them.\n",
                profile.name()
            ),
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
        splice(text, parse::module::satisfies_span(text), &self.block())
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

/// Which modules elsewhere claim rules the profile selects that the image does
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

    /// A one-line `sources` block is valid KDL, and a source added to it stays
    /// inside the block instead of duplicating the line.
    #[test]
    fn a_one_line_sources_block_takes_a_source() {
        let library = Library {
            kind: crate::model::remote::Kind::BaseImages,
            alias: "core".to_string(),
            at: LibraryAt::Default(crate::base::default_library(
                crate::model::remote::Kind::BaseImages,
            )),
        };
        let out = library
            .spliced("schema-version 1\n\nsources { modules \"one\" { dir \"../one\" } }\n")
            .expect("the splice is text");
        let issues = crate::parse::schema::check_text(&out, &crate::parse::repo::REPO, false)
            .unwrap_or_else(|err| panic!("{err}: {out:?}"));
        assert!(issues.is_empty(), "{out}\n{}", issues.plain());
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

    fn claims(profile: &str, numbers: &[&str]) -> Claims {
        Claims {
            file: PathBuf::from("modules/one/module.kdl"),
            profile: profile.to_string(),
            numbers: numbers.iter().map(|n| n.to_string()).collect(),
            claimed: numbers.len(),
        }
    }

    /// SCAP content is family-specific, so a base-agnostic module has no
    /// installed default.
    #[test]
    fn base_agnostic_claims_need_a_datastream() {
        let root = std::env::temp_dir().join(format!("tect-claims-any-{}", std::process::id()));
        let module = root.join("modules/editor");
        std::fs::create_dir_all(&module).unwrap();
        std::fs::write(
            module.join(layout::MODULE_FILE),
            "schema-version 1\n\npackages \"nano\"\n",
        )
        .unwrap();

        let result = Claims::collect(&root, "editor", None, &Prompt::silent());
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

    /// A module that declared none gets the block at the end.
    #[test]
    fn a_module_with_no_block_gets_one_at_the_end() {
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
        let file = dir.join(layout::MODULE_FILE);
        std::fs::write(&file, manifest).unwrap();

        // What `collect` carries over: the lenient read keeps a number the
        // strict one refused, and no picker ever showed it.
        let held = parse::module::summary(&file).satisfies;
        assert_eq!(held, ["1.1.1.1"]);
        std::fs::remove_dir_all(dir).unwrap();

        let carried: Vec<&str> = held.iter().map(String::as_str).collect();
        assert_eq!(
            claims("ospp", &carried).spliced(manifest),
            "description \"one\"\n\nsatisfies {\n    ospp \"1.1.1.1\"\n}\n"
        );
    }

    fn set(chosen: &[&'static str], at: (u32, u32)) -> Workflows {
        Workflows {
            chosen: chosen.to_vec(),
            at,
            publishes_scheduled: false,
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

    #[test]
    fn cadence_properties_follow_declaration_order() {
        let mut workflows = set(&["build"], (6, 0));
        workflows.publishes_scheduled = true;
        workflows.scans_scheduled = true;
        assert_eq!(
            workflows.spliced("name \"Example\"\n"),
            "name \"Example\"\n\nworkflows at=\"06:00\" publish=\"scheduled\" scan=\"scheduled\" {\n    build\n}\n"
        );
    }

    /// The one thing that is wrong if `collect` is re-entered at the wrong
    /// point: a question above the edited row asked again spends the answer
    /// meant for the one below it, and this run has exactly one to spend.
    #[test]
    fn re_entering_at_a_field_asks_it_and_nothing_above_it() {
        let mut held = set(&["build"], DEFAULT_AT);
        held.scans_scheduled = true;
        let prompt = Prompt::scripted(["07:30"].map(str::to_string).to_vec());
        let again = held
            .again(&Basis::scaffolding(""), Field::Daily, &prompt)
            .unwrap()
            .unwrap();
        assert_eq!(again.chosen, ["build"]);
        assert!(!again.publishes_scheduled);
        assert!(again.scans_scheduled);
        assert_eq!(again.at, (7, 30));
    }

    /// An edited base can put a workflow out of reach, and so can a declaration
    /// written before one changed. It opens cleared, dim and unpickable, with a
    /// line above saying so.
    #[test]
    fn a_workflow_the_basis_cannot_run_opens_cleared_rather_than_refused() {
        let held = set(&["build", "build-disk"], DEFAULT_AT);
        // The list kept as offered, both cadences, and the time.
        let prompt = Prompt::scripted(["", "No", "No", "06:00"].map(str::to_string).to_vec());
        let again = held
            .again(&Basis::scaffolding(""), Field::Workflows, &prompt)
            .unwrap()
            .unwrap();
        assert_eq!(again.chosen, ["build"]);
    }

    /// A row for a question that was never reachable has nothing to re-enter.
    #[test]
    fn a_cadence_is_a_row_only_where_its_question_was_asked() {
        let labels = |workflows: &Workflows| -> Vec<&str> {
            workflows
                .rows()
                .iter()
                .map(|(_, label, _)| *label)
                .collect()
        };
        let mut workflows = set(&["build"], DEFAULT_AT);
        assert_eq!(
            labels(&workflows),
            [
                copy::ROW_WORKFLOWS,
                copy::ROW_PUBLISH,
                copy::ROW_SCANS,
                copy::ROW_DAILY
            ]
        );
        workflows.publishes_scheduled = true;
        assert!(!labels(&workflows).contains(&copy::ROW_SCANS));
        assert_eq!(labels(&set(&[], DEFAULT_AT)), [copy::ROW_WORKFLOWS]);
        assert_eq!(
            labels(&set(&["build-disk"], DEFAULT_AT)),
            [copy::ROW_WORKFLOWS]
        );
    }

    #[test]
    fn scheduled_publishing_makes_the_scan_question_redundant() {
        let prompt = Prompt::scripted(["1", "Yes", "06:00"].map(str::to_string).to_vec());
        let workflows = Workflows::collect(
            &Basis::scaffolding(""),
            &[],
            DEFAULT_AT,
            false,
            false,
            Field::Workflows,
            &prompt,
        )
        .unwrap()
        .unwrap();
        assert!(workflows.publishes_scheduled);
        assert!(!workflows.scans_scheduled);
        assert_eq!(workflows.at, (6, 0));
    }

    #[test]
    fn existing_scheduled_cadences_survive_accepting_scheduled_publishing() {
        let prompt = Prompt::scripted(["", "Yes", "06:00"].map(str::to_string).to_vec());
        let workflows = Workflows::collect(
            &Basis::scaffolding(""),
            &["build"],
            DEFAULT_AT,
            true,
            true,
            Field::Workflows,
            &prompt,
        )
        .unwrap()
        .unwrap();
        assert!(workflows.publishes_scheduled);
        assert!(workflows.scans_scheduled);
    }

    #[test]
    fn declining_scheduled_publishing_preserves_the_current_scan_cadence() {
        let prompt = Prompt::scripted(["", "No", "", "06:00"].map(str::to_string).to_vec());
        let workflows = Workflows::collect(
            &Basis::scaffolding(""),
            &["build"],
            DEFAULT_AT,
            true,
            true,
            Field::Workflows,
            &prompt,
        )
        .unwrap()
        .unwrap();
        assert!(!workflows.publishes_scheduled);
        assert!(workflows.scans_scheduled);
    }

    /// Generating nothing is an absent block: an empty one is a diagnostic.
    #[test]
    fn choosing_nothing_takes_the_block_away() {
        let text = "name \"Example\"\n\nworkflows {\n    build\n}\n";
        assert_eq!(set(&[], DEFAULT_AT).spliced(text), "name \"Example\"\n");
    }

    #[test]
    fn workflow_and_cadence_screens_snapshot_production_rows() {
        use crate::screen_tests::{assert_screen, assert_style_at, screen};

        let basis = Basis::scaffolding("");
        let options = workflow_options(&basis);
        let selected = [0, 2, 3, 5];
        let rendered = assert_screen(
            "workflow-picker",
            screen().picker(
                copy::WORKFLOWS,
                &options,
                Some(&selected),
                "space toggles",
                0,
            ),
        );
        assert_style_at(
            &rendered,
            0,
            2,
            "modifier: DIM",
            "the unavailable build-disk workflow",
        );
        let yes_no = [Choice::new(copy::YES, ""), Choice::new(copy::NO, "")];
        assert_screen(
            "workflow-publish-cadence",
            screen().picker(copy::PUBLISH_SCHEDULED, &yes_no, None, "enter confirms", 1),
        );
        assert_screen(
            "workflow-scan-cadence",
            screen().picker(copy::SCAN_SCHEDULED, &yes_no, None, "enter confirms", 1),
        );
        assert_screen(
            "workflow-daily-at",
            screen().line(
                copy::DAILY_AT,
                "",
                "",
                Some(&parse::repo::at_text(DEFAULT_AT)),
            ),
        );

        let mut workflows = set(&["build", "base-sig-probe"], DEFAULT_AT);
        workflows.scans_scheduled = true;
        let mut rows: Vec<Choice> = workflows
            .rows()
            .into_iter()
            .map(|(_, label, value)| Choice::new(format!("{label}  {value}"), ""))
            .collect();
        rows.push(Choice::new(copy::CREATE, ""));
        assert_screen(
            "workflow-cadences",
            screen().picker(copy::REVIEW, &rows, None, copy::REVIEW_KEYS, rows.len() - 1),
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

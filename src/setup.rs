//! This module holds the setup questions and answers that `create`, `import`
//! and `set` share.
//!
//! A flow collects every answer before it writes, so no apply takes a prompt.

use crate::copy;
use crate::declaration::{append, placed, splice, wrap};
use crate::layout;
use crate::parse;
use crate::resolve::workflow::{Basis, Shipped, DEFAULT_AT, SHIPPED};
use common::prompt::Prompt;
use common::ui::tree::Change;
use common::ui::{Answer, Choice};
use std::path::{Path, PathBuf};

/// Review rows double as re-entry points. Re-entry asks that field and every
/// later field. A gated row must re-ask whether its value exists so the user
/// can change `none` into a value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Name,
    Provider,
    Remote,
    Libraries,
    Image,
    Base,
    Workflows,
    Publish,
    Scans,
    Daily,
    Scripts,
}

/// The libraries a fresh repository reads, which is the one choice every
/// default library is offered under.
#[derive(Clone, Default)]
pub struct Libraries {
    pub(crate) chosen: Vec<&'static crate::collection::DefaultLibrary>,
}

impl Libraries {
    /// Every default library, in the order the scaffolded block lists them.
    pub fn every() -> Vec<&'static crate::collection::DefaultLibrary> {
        crate::collection::DEFAULT_LIBRARIES.iter().collect()
    }

    /// A first run opens on every default. A second opens on the answer held,
    /// so a re-entry at this row does not silently take the defaults back.
    pub fn collect(held: Option<&Self>, prompt: &Prompt) -> Result<Self, String> {
        let every = Self::every();
        let options: Vec<Choice> = every
            .iter()
            .map(|library| {
                Choice::new(
                    format!("{} \"{}\"", library.kind.as_str(), library.alias),
                    library.about,
                )
            })
            .collect();
        let on: Vec<usize> = match held {
            Some(held) => every
                .iter()
                .enumerate()
                .filter(|(_, library)| {
                    held.chosen
                        .iter()
                        .any(|held| held.alias == library.alias && held.kind == library.kind)
                })
                .map(|(at, _)| at)
                .collect(),
            None => (0..every.len()).collect(),
        };
        let Answer::Chosen(chosen) = prompt.choose_many(copy::LIBRARIES, &options, &on)? else {
            unreachable!("choose_many answers with a choice or fails")
        };
        Ok(Self {
            chosen: chosen.into_iter().map(|at| every[at]).collect(),
        })
    }

    pub(crate) fn row(&self) -> (Field, &'static str, String) {
        let mut aliases: Vec<&str> = Vec::new();
        for library in &self.chosen {
            if !aliases.contains(&library.alias) {
                aliases.push(library.alias);
            }
        }
        (
            Field::Libraries,
            copy::ROW_LIBRARIES,
            match aliases.is_empty() {
                true => copy::NONE.to_string(),
                false => aliases.join(", "),
            },
        )
    }
}

/// Image listing remains a user decision even when the repository has one
/// image.
pub enum Listing {
    Cancelled,
    NoImage,
    /// Declining every image is a valid answer. `asked` is false when no image
    /// exists and the response must name the flag.
    Declined {
        asked: bool,
    },
    In(Vec<Listed>),
}

pub struct Listed {
    file: PathBuf,
    image: String,
    flavour: Option<String>,
}

impl Listed {
    fn of(list: &crate::model::image::List, target: &crate::model::image::Target) -> Self {
        let image = list
            .images
            .iter()
            .find(|image| image.id == target.image)
            .expect("a target names an image the list holds");
        Self {
            file: PathBuf::from(image.src.name()),
            image: image.name.clone(),
            flavour: match target.flavour == crate::model::image::NO_FLAVOUR {
                true => None,
                false => Some(target.flavour.clone()),
            },
        }
    }
}

impl Listing {
    /// Duplicate checks run per component because one answer can select several
    /// images.
    pub fn collect(root: &Path, given: Vec<String>, prompt: &Prompt) -> Result<Self, String> {
        let list = crate::model::image::List::editable(root).map_err(|issues| issues.plain())?;
        Self::collect_from(&list, given, prompt)
    }

    /// Asks which one image or flavour an import is for. The last row answers
    /// none, which leaves the import in the repository and lists it nowhere.
    pub fn target(root: &Path, given: Option<String>, prompt: &Prompt) -> Result<Self, String> {
        let list = crate::model::image::List::editable(root).map_err(|issues| issues.plain())?;
        if list.images.is_empty() {
            return Ok(Self::NoImage);
        }
        let targets = list.targets();
        let at = match given {
            Some(name) => targets
                .iter()
                .position(|known| known.to_string() == name)
                .ok_or_else(|| {
                    let named: Vec<String> = targets.iter().map(ToString::to_string).collect();
                    format!(
                        "`{name}` is not a declared image; there is {}",
                        named.join(", ")
                    )
                })?,
            None => {
                let mut rows = target_rows(&list, &targets);
                rows.push(Choice::new(copy::TARGET_NONE, ""));
                match prompt.choose(copy::TARGET_IMAGE, &rows)? {
                    None => return Ok(Self::Declined { asked: false }),
                    Some(at) if at == targets.len() => return Ok(Self::Declined { asked: true }),
                    Some(at) => at,
                }
            }
        };
        Ok(Self::In(vec![Listed::of(&list, &targets[at])]))
    }

    pub(crate) fn collect_from(
        list: &crate::model::image::List,
        given: Vec<String>,
        prompt: &Prompt,
    ) -> Result<Self, String> {
        if list.images.is_empty() {
            return Ok(Self::NoImage);
        }
        let targets = list.targets();
        let named: Vec<String> = targets.iter().map(ToString::to_string).collect();

        let chosen: Vec<usize> = match given.is_empty() {
            true => match ask(list, &targets, prompt)? {
                common::ui::Answer::Cancelled => return Ok(Self::Cancelled),
                common::ui::Answer::Chosen(chosen) => chosen,
            },
            false => given
                .iter()
                .map(|name| {
                    named.iter().position(|known| known == name).ok_or_else(|| {
                        format!(
                            "`{name}` is not a declared image; there is {}",
                            named.join(", ")
                        )
                    })
                })
                .collect::<Result<_, _>>()?,
        };
        let mut unique = Vec::new();
        for at in chosen {
            if !unique.contains(&at) {
                unique.push(at);
            }
        }
        let chosen = unique;
        // The ungated entry is already in every flavour. The widget makes the
        // pair unreachable. A flag and the numbered list do not.
        let ungated = |target: &crate::model::image::Target| {
            chosen.iter().any(|at| {
                targets[*at].image == target.image
                    && targets[*at].flavour == crate::model::image::NO_FLAVOUR
            })
        };
        if let Some(gated) = chosen
            .iter()
            .map(|at| &targets[*at])
            .find(|target| target.flavour != crate::model::image::NO_FLAVOUR && ungated(target))
        {
            return Err(format!(
                "`{gated}` is inside `{}`, so listing it in both lists it twice",
                gated.image
            ));
        }

        Ok(match chosen.is_empty() {
            true => Self::Declined {
                asked: prompt.asks(),
            },
            false => Self::In(
                chosen
                    .iter()
                    .map(|at| Listed::of(list, &targets[*at]))
                    .collect(),
            ),
        })
    }

    /// Duplicate checks compare `name` within each selected image. A component
    /// gated to two flavours conflicts only where the flavours overlap.
    pub fn refuse_duplicate(
        &self,
        list: &crate::model::image::List,
        name: &str,
    ) -> Result<(), String> {
        let Self::In(listed) = self else {
            return Ok(());
        };
        let target = |image: &crate::model::image::Image, flavour: &Option<String>| {
            crate::model::image::Target {
                image: image.id.clone(),
                flavour: flavour
                    .clone()
                    .unwrap_or_else(|| crate::model::image::NO_FLAVOUR.to_string()),
            }
        };
        for into in listed {
            let Some((image, held)) = holds(list, into, name) else {
                continue;
            };
            let (at, into) = (target(image, &held.flavour), target(image, &into.flavour));
            return Err(match at.to_string() == into.to_string() {
                true => format!("`{at}` already lists `{name}`"),
                false => format!("`{at}` already lists `{name}`, so `{into}` lists it twice"),
            });
        }
        Ok(())
    }

    /// Existing declarations are skipped per image, so every returned image
    /// file was updated by this application. A declaration in one image does
    /// not suppress insertion into another image.
    pub fn apply(
        &self,
        list: &crate::model::image::List,
        path: &str,
    ) -> Result<Vec<(PathBuf, Change)>, String> {
        self.apply_declaration(list, &[(path, "")])
    }

    pub fn cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let Self::In(listed) = self else {
            return Ok(());
        };
        for target in listed {
            let text = std::fs::read_to_string(&target.file)
                .map_err(|err| format!("{}: {err}", target.file.display()))?;
            if crate::parse::image::block_close(&text, &target.image, &[]).is_none() {
                return Err(format!(
                    "{} declares no image `{}`",
                    target.file.display(),
                    target.image
                ));
            }
        }
        Ok(())
    }

    pub fn images(&self) -> Vec<&str> {
        let Self::In(listed) = self else {
            return Vec::new();
        };
        let mut out: Vec<&str> = Vec::new();
        for target in listed {
            if !out.contains(&target.image.as_str()) {
                out.push(&target.image);
            }
        }
        out
    }

    pub(crate) fn targets(&self) -> Vec<(&str, Option<&str>)> {
        let Self::In(listed) = self else {
            return Vec::new();
        };
        listed
            .iter()
            .map(|target| (target.image.as_str(), target.flavour.as_deref()))
            .collect()
    }

    /// Batch application preserves member order. An existing member is
    /// skipped only in the image that already declares it.
    pub(crate) fn apply_declaration(
        &self,
        list: &crate::model::image::List,
        declarations: &[(&str, &str)],
    ) -> Result<Vec<(PathBuf, Change)>, String> {
        match self {
            Self::Cancelled => Ok(Vec::new()),
            Self::NoImage => {
                println!("no image lists it yet; `tect create image <name>` writes one");
                Ok(Vec::new())
            }
            Self::Declined { .. } => {
                for (name, block) in declarations {
                    let (chain, line) = placed(None, name, block);
                    let shown = wrap(&chain, &line);
                    println!(
                        "next, to build it, list it in an image:\n {}",
                        shown.replace('\n', "\n ")
                    );
                }
                Ok(Vec::new())
            }
            Self::In(listed) => {
                let mut wrote: Vec<(PathBuf, Vec<String>)> = Vec::new();
                for target in listed {
                    let mut taken: Vec<String> = Vec::new();
                    for (name, block) in declarations {
                        if holds(list, target, name).is_some() {
                            continue;
                        }
                        let (chain, line) = placed(target.flavour.as_deref(), name, block);
                        append(&target.file, &target.image, &chain, &line)?;
                        taken.push((*name).to_string());
                    }
                    if taken.is_empty() {
                        continue;
                    }
                    let Some((_, added)) = wrote.iter_mut().find(|(file, _)| *file == target.file)
                    else {
                        wrote.push((target.file.clone(), taken));
                        continue;
                    };
                    for name in taken {
                        if !added.contains(&name) {
                            added.push(name);
                        }
                    }
                }
                Ok(wrote
                    .into_iter()
                    .map(|(file, added)| {
                        (
                            file,
                            Change::Updated(format!("{} added to components", crate::said(&added))),
                        )
                    })
                    .collect())
            }
        }
    }
}

/// An ungated entry spans every flavour, so an existing entry conflicts only
/// where its target overlaps `into`.
fn holds<'a>(
    list: &'a crate::model::image::List,
    into: &Listed,
    name: &str,
) -> Option<(
    &'a crate::model::image::Image,
    &'a crate::model::image::Entry,
)> {
    let image = list.images.iter().find(|image| image.name == into.image)?;
    let entry = image.entries.iter().find(|entry| {
        entry.path == name
            && crate::resolve::meet(into.flavour.as_deref(), entry.flavour.as_deref())
    })?;
    Some((image, entry))
}

/// Listing and flavour gating are one question at two depths. A single image
/// without flavours therefore uses a yes-or-no question.
fn ask(
    list: &crate::model::image::List,
    targets: &[crate::model::image::Target],
    prompt: &Prompt,
) -> Result<common::ui::Answer, String> {
    if let [only] = targets {
        let listed = prompt.confirm(&copy::list_in(&only.to_string()), copy::YES, copy::NO)?;
        return Ok(common::ui::Answer::Chosen(match listed {
            true => vec![0],
            false => Vec::new(),
        }));
    }
    prompt.choose_many(copy::LIST_IN_IMAGES, &target_rows(list, targets), &[])
}

/// Draws a flavour under its image, and an image beside its display name where
/// that differs from its id.
fn target_rows(
    list: &crate::model::image::List,
    targets: &[crate::model::image::Target],
) -> Vec<Choice> {
    let mut ungated = 0;
    let mut rows: Vec<Choice> = Vec::new();
    for (at, target) in targets.iter().enumerate() {
        let label = target.to_string();
        if target.flavour != crate::model::image::NO_FLAVOUR {
            rows.push(Choice::new(label, "").under(ungated));
            continue;
        }
        ungated = at;
        let named = list
            .images
            .iter()
            .find(|image| image.id == target.image)
            .map_or("", |image| match image.name == image.id {
                true => "",
                false => image.name.as_str(),
            });
        rows.push(Choice::new(label, named));
    }
    rows
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
    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is one answer or one current value the picker reads"
    )]
    pub fn collect(
        basis: &Basis,
        on: &[&str],
        at: (u32, u32),
        publishes_current: bool,
        scans_current: bool,
        from: Field,
        prompt: &Prompt,
        say: crate::report::Say<'_>,
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
                    say(&format!(
                        "{}\n",
                        copy::cleared(shipped.stem, shipped.needs.unmet())
                    ));
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
        say: crate::report::Say<'_>,
    ) -> Result<Option<Self>, String> {
        Self::collect(
            basis,
            &self.chosen,
            self.at,
            self.publishes_scheduled,
            self.scans_scheduled,
            from,
            prompt,
            say,
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
    pub(crate) fn spliced(&self, text: &str) -> String {
        let block = match self.chosen.is_empty() {
            true => String::new(),
            false => {
                let held = parse::repo::workflow_schedules(text);
                let named: String = self
                    .chosen
                    .iter()
                    .map(|stem| match held.iter().find(|(name, _)| name == stem) {
                        Some((_, line)) => format!("    {stem} schedule=\"{line}\"\n"),
                        None => format!("    {stem}\n"),
                    })
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

pub(crate) fn workflow_options(basis: &Basis) -> Vec<Choice> {
    SHIPPED
        .iter()
        .map(|shipped| match shipped.met(basis) {
            true => Choice::new(shipped.stem, shipped.about),
            false => Choice::new(shipped.stem, shipped.needs.unmet()).unavailable(),
        })
        .collect()
}

/// What to do instead, where there is nobody to ask. The declaration file was
/// always the interface, so a flag writing the same line would be a second one.
pub const BY_HAND: &str = "nothing can be asked here; edit the `workflows` block in repo.kdl, \
                           then run `tect generate`";

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_rewritten_block_keeps_each_declared_schedule() {
        let text = "workflows {\n    build\n    update-components schedule=\"0 3 * * 0\"\n}\n";
        assert_eq!(
            set(&["build", "update-components"], DEFAULT_AT).spliced(text),
            text
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
            .again(&Basis::scaffolding(""), Field::Daily, &prompt, &mut |_| {})
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
        let mut said = String::new();
        let again = held
            .again(
                &Basis::scaffolding(""),
                Field::Workflows,
                &prompt,
                &mut |text| said.push_str(text),
            )
            .unwrap()
            .unwrap();
        assert_eq!(again.chosen, ["build"]);
        assert!(said.contains("build-disk"), "{said}");
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
            &mut |_| {},
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
            &mut |_| {},
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
            &mut |_| {},
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
}

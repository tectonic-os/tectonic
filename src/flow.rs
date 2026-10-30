//! The setup questions and the written-tree report that `create`, `import` and
//! `set` share.
//!
//! A flow collects every answer before it writes, so no apply takes a prompt,
//! and it renders the tree line that names what it wrote.

use crate::copy;
use crate::layout;
use crate::parse;
use crate::resolve::workflow::{Basis, Shipped, DEFAULT_AT, SHIPPED};
use crate::scap::{ordinal, rule_name, Content};
use common::prompt::Prompt;
use common::ui::tree::Change;
use common::ui::{Answer, Choice};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub fn named_after_root(root: &Path) -> Option<String> {
    std::fs::canonicalize(root)
        .ok()
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
}

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
    pub(crate) chosen: Vec<&'static crate::base::DefaultLibrary>,
}

impl Libraries {
    /// Every default library, in the order the scaffolded block lists them.
    pub fn every() -> Vec<&'static crate::base::DefaultLibrary> {
        crate::base::DEFAULT_LIBRARIES.iter().collect()
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

pub fn report(root: &Path, wrote: &[(PathBuf, Change)]) {
    let declared = crate::model::image::List::load(root).0.id;
    let id = match declared.is_empty() {
        true => named_after_root(root).unwrap_or_default(),
        false => declared,
    };
    common::ui::tree::print(&id, wrote, describe);
}

/// A tree line says what a later step added to an existing file. File-purpose
/// descriptions stay in documentation so every path does not gain a column.
fn describe(_path: &Path, change: Option<&Change>) -> String {
    match change {
        Some(Change::Updated(edit)) => edit.clone(),
        _ => String::new(),
    }
}

pub(crate) fn under(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
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

/// The flavour block an entry is listed in. An ungated entry sits in the
/// image block itself.
fn listed_in(flavour: Option<&str>) -> Vec<(&str, Option<&str>)> {
    flavour
        .map(|flavour| ("flavour", Some(flavour)))
        .into_iter()
        .collect()
}

/// Places a component path: the blocks that hold its last segment, and the
/// line that lists it. `block` holds the option values the entry sets, or is
/// empty.
pub(crate) fn placed<'a>(
    flavour: Option<&'a str>,
    name: &'a str,
    block: &str,
) -> (Vec<(&'a str, Option<&'a str>)>, String) {
    let mut chain = listed_in(flavour);
    let (parents, last) = name.rsplit_once('/').unwrap_or(("", name));
    chain.extend(
        parents
            .split('/')
            .filter(|part| !part.is_empty())
            .map(|part| (part, None)),
    );
    (chain, format!("{last}{block}"))
}

fn wrap(blocks: &[(&str, Option<&str>)], leaf: &str) -> String {
    blocks
        .iter()
        .rev()
        .fold(leaf.to_string(), |inner, (node, arg)| {
            let arg = arg.map_or(String::new(), |arg| format!(" \"{arg}\""));
            let inner: String = inner.lines().map(|line| format!("    {line}\n")).collect();
            format!("{node}{arg} {{\n{inner}}}")
        })
}

/// Append inserts one declaration at the deepest existing block and wraps the
/// missing blocks. Every other byte stays in place.
pub(crate) fn append(
    file: &Path,
    image: &str,
    chain: &[(&str, Option<&str>)],
    leaf: &str,
) -> Result<(), String> {
    let mut text =
        std::fs::read_to_string(file).map_err(|err| format!("{}: {err}", file.display()))?;
    let (kept, close) = (0..=chain.len())
        .rev()
        .find_map(|kept| {
            crate::parse::image::block_close(&text, image, &chain[..kept]).map(|at| (kept, at))
        })
        .ok_or_else(|| format!("{} declares no image `{image}`", file.display()))?;
    let declaration = wrap(&chain[kept..], leaf);

    let start = text[..close].rfind('\n').map_or(0, |at| at + 1);
    let indent = &text[start..close];
    let (at, line) = match indent.trim().is_empty() {
        true => (
            start,
            declaration
                .lines()
                .map(|line| format!("{indent}    {line}\n"))
                .collect(),
        ),
        // A node that shares the block's line ends at a `;`, or the new node
        // would read as its argument.
        false => {
            let end = text[..close].trim_end().len();
            let gap = match text[..end].ends_with(['{', ';']) {
                true => " ",
                false => "; ",
            };
            (end, format!("{gap}{}", declaration.replace('\n', " ")))
        }
    };
    text.insert_str(at, &line);
    std::fs::write(file, text).map_err(|err| format!("{}: {err}", file.display()))
}

/// Removes the segment that lists `name` in one image of `file`, whose text
/// is `text`, and keeps every other byte in place. The calling command writes
/// the result, so a refusal leaves the file as it was.
pub(crate) fn unlisting(
    file: &Path,
    text: &str,
    image: &str,
    flavour: Option<&str>,
    name: &str,
) -> Result<String, String> {
    let root = file.parent().unwrap_or(Path::new("."));
    let mut chain = listed_in(flavour);
    chain.extend(crate::parse::image::segments(name));
    crate::parse::image::unlisted(text, image, &chain, |path| {
        layout::is_component(&layout::component(root, path))
    })
    .map_err(|err| format!("{} {err}", file.display()))
}

/// A top-level block put back where it was, else at the end of the file.
/// Taking one away takes the blank line above it with it.
pub(crate) fn splice(text: &str, was: Option<crate::diag::Span>, block: &str) -> String {
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

#[cfg(feature = "fuzzing")]
pub fn fuzz_rewrites(text: &str) {
    let spans = [
        crate::parse::repo::workflows_span(text),
        crate::parse::image::conforms_span(text, "fuzz"),
        crate::parse::component::satisfies_span(text),
    ];
    for span in spans {
        let _ = splice(text, span, "fuzz \"value\"");
        let _ = splice(text, span, "");
    }
}

/// The whitespace before `at`, which is the line's indent where `at` stands on
/// that line's own node and empty where the line holds code before it.
pub(crate) fn line_indent(text: &str, at: usize) -> &str {
    let start = text[..at].rfind('\n').map_or(0, |found| found + 1);
    let line = &text[start..at];
    &line[..line.len() - line.trim_start().len()]
}

/// Which CI the repository generates, and the one time the schedules hang off.
pub struct Workflows {
    pub(crate) chosen: Vec<&'static str>,
    pub(crate) at: (u32, u32),
    pub(crate) publishes_scheduled: bool,
    pub(crate) scans_scheduled: bool,
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

/// The branch a benchmark number sits in: its dotted prefix, and one flat
/// branch for a number with no dot to group it by, since a STIG ident nests
/// nowhere.
pub(crate) fn group(number: &str) -> &str {
    number.rsplit_once('.').map_or("other", |(head, _)| head)
}

pub(crate) fn profile_options(content: &Content) -> Vec<Choice> {
    content
        .profiles
        .iter()
        .map(|profile| Choice::new(profile.name(), &profile.title))
        .collect()
}

pub(crate) fn claimable_rules<'a>(
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

pub(crate) fn claim_options(
    content: &Content,
    rules: &[(&String, &BTreeSet<String>)],
) -> Vec<Choice> {
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

/// The same, for the rules a component claims: the manifest was always the
/// interface, and a claim is a number read out of a datastream or nothing.
pub const CLAIMS_BY_HAND: &str = "nothing can be asked here; write `satisfies { <benchmark> \
                                  \"<number>\" }` into the component, and `tect scap content` \
                                  prints the datastream those numbers are read out of";

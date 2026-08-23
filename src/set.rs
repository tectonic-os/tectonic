//! `tect set`, the config family. A question answered into repo.kdl, which
//! `generate` then acts on: nothing here writes an artifact.
//!
//! Collect-then-apply like every other flow, so `create repo` holds one of
//! these rather than calling the command.

use crate::prompt::Prompt;
use crate::resolve::workflow::{Basis, Shipped, DEFAULT_AT, SHIPPED};
use crate::ui::tree::Change;
use crate::ui::{Answer, Choice};
use crate::{layout, parse};
use std::path::{Path, PathBuf};

/// What to do instead, where there is nobody to ask. The declaration file was
/// always the interface, so a flag writing the same line would be a second one.
pub const BY_HAND: &str = "nothing can be asked here; edit the `workflows` block in repo.kdl, \
                           then run `tect generate`";

/// Which CI the repository generates, and the one time the schedules hang off.
pub struct Workflows {
    chosen: Vec<&'static str>,
    at: (u32, u32),
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
                format!("workflows{at} {{\n{named}}}")
            }
        };
        let mut out = text.to_string();
        match parse::repo::workflows_span(text) {
            Some(was) => {
                let end = was.offset + was.len;
                // Taking the block away takes the blank line above it too.
                let start = match block.is_empty() {
                    true => text[..was.offset].trim_end_matches(['\n', ' ']).len(),
                    false => was.offset,
                };
                out.replace_range(start..end, &block);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(chosen: &[&'static str], at: (u32, u32)) -> Workflows {
        Workflows {
            chosen: chosen.to_vec(),
            at,
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

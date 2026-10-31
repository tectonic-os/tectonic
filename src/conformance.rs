//! This module holds the benchmark profile one image is measured against.
//! It states the cost of declaring that profile.

use crate::parse;
use crate::scap::Content;
use common::ui::tree::Change;
use common::ui::Choice;
use std::path::{Path, PathBuf};

pub(crate) fn profile_options(content: &Content) -> Vec<Choice> {
    content
        .profiles
        .iter()
        .map(|profile| Choice::new(profile.name(), &profile.title))
        .collect()
}

/// What to do instead, where there is nobody to ask. The image file was
/// always the interface, and there is a datastream to pick out of or there is
/// not.
pub const CONFORMS_BY_HAND: &str = "nothing can be asked here; write `conforms \"<profile>\"` \
                                    into the image, and `tect scap content` prints the \
                                    datastream it is then measured with";

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
/// components the offer alongside it brought.
pub struct Conforms {
    /// The image file, as the image was read from.
    file: PathBuf,
    /// The image's `name`, which is what the writer walks to.
    image: String,
    profile: String,
    brought: Vec<String>,
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
        }
    }

    /// Writes the declaration over whatever is there now.
    pub(crate) fn apply(&self, root: &Path) -> Result<Vec<(PathBuf, Change)>, String> {
        let file = root.join(&self.file);
        let text =
            std::fs::read_to_string(&file).map_err(|err| format!("{}: {err}", file.display()))?;
        let spliced = self.spliced(&text)?;
        std::fs::write(&file, spliced).map_err(|err| format!("{}: {err}", file.display()))?;
        Ok(vec![(
            self.file.clone(),
            Change::Updated(match self.brought.is_empty() {
                true => format!("measured against `{}`", self.profile),
                false => format!(
                    "measured against `{}`, and {} added to components",
                    self.profile,
                    crate::said(&self.brought)
                ),
            }),
        )])
    }

    /// In place of the declaration that was there, else in front of the first
    /// path segment.
    pub(crate) fn spliced(&self, text: &str) -> Result<String, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn conforms(profile: &str) -> Conforms {
        Conforms {
            file: PathBuf::from("example.image.kdl"),
            image: "Example".to_string(),
            profile: profile.to_string(),
            brought: Vec::new(),
        }
    }

    /// The declaration replaces the one that was there and goes in front of
    /// the first path segment otherwise.
    #[test]
    fn the_profile_replaces_the_one_that_was_there() {
        let text =
            "image {\n    name \"Example\"\n\n    conforms \"standard\"\n\n    x {\n    }\n}\n";
        assert_eq!(
            conforms("ospp").spliced(text).unwrap(),
            text.replace("\"standard\"", "\"ospp\"")
        );
    }

    /// An image with no segment for it to stand in front of still gets one.
    #[test]
    fn an_image_with_nothing_to_stand_in_front_of_still_takes_one() {
        assert_eq!(
            conforms("standard")
                .spliced("image {\n    name \"Example\"\n}\n")
                .unwrap(),
            "image {\n    name \"Example\"\nconforms \"standard\"\n\n}\n"
        );
    }
}

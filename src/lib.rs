//! The only reader of the image files and the per-component manifest.kdl files.

pub mod bases;
pub mod build;
pub mod collection;
pub mod command;
pub mod conformance;
pub mod copy;
pub mod create;
pub mod declaration;
pub mod diag;
pub mod dispatch;
pub mod emit;
pub mod error;
pub mod fetch;
pub mod forge;
#[cfg(feature = "fuzzing")]
pub mod fuzzing;
pub mod import;
pub mod init;
pub mod key;
pub mod layout;
pub mod model;
pub mod parse;
pub mod provenance;
pub mod provider;
pub mod registry;
pub mod report;
pub mod resolve;
pub mod run;
pub mod runtime;
pub mod scap;
pub mod set;
pub mod setup;
pub mod sys;
pub mod upgrade;
pub mod vm;

#[cfg(test)]
mod screen_tests;

pub use command::{Arg, Command};
use diag::Issue;
use diag::Issues;
use diag::Source;
use model::component::Component;
use model::image::{Entry, Image, List};
pub use parse::repo::compatible;
use resolve::Resolved;
pub(crate) use run::run_loaded;
pub use run::{run, Run};
use std::path::{Path, PathBuf};

/// The nearest directory at or above `from` holding a repo.kdl.
pub fn find_root(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .find(|dir| dir.join(layout::REPO_FILE).is_file())
        .map(Path::to_path_buf)
}

/// What the repository declares, with each image's base component loaded and
/// no other component manifest: enough for anything that only needs a name.
pub fn declarations(root: &Path) -> (List, Issues, String) {
    let (list, issues) = List::load(root);
    let context = context(&list, root);
    (list, issues, context)
}

/// The files read, for the line a failure ends with.
fn context(list: &List, root: &Path) -> String {
    match list.files.is_empty() {
        true => root.display().to_string(),
        false => list.files.join(", "),
    }
}

/// The Containerfile skeleton to splice into. If the repository has none, then
/// this release's skeleton is used.
pub(crate) fn skeleton(root: &Path, issues: &mut Issues) -> Option<String> {
    use emit::containerfile::{
        BEGIN, END, RECORD_BEGIN, RECORD_END, SKELETON, TAIL_BEGIN, TAIL_END, VALIDATE_BEGIN,
        VALIDATE_END,
    };

    let text = match std::fs::read_to_string(root.join(SKELETON)) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => init::SKELETON.to_string(),
        Err(err) => {
            issues.push(Issue::new(
                format!("`{SKELETON}` cannot be read: {err}"),
                &Source::new(SKELETON, String::new()),
            ));
            return None;
        }
    };
    let src = Source::new(SKELETON, text.clone());
    // A skeleton that still carries the record or validate steps would run
    // each step twice.
    let stale = old_spine(&text);
    if !stale.is_empty() {
        let ranges: Vec<String> = stale
            .iter()
            .map(|(first, last)| match first == last {
                true => format!("{first}"),
                false => format!("{first} to {last}"),
            })
            .collect();
        issues.push(
            Issue::new(
                format!("`{SKELETON}` still carries the generated record or validate steps"),
                &src,
            )
            .help(format!(
                "the old steps are at lines {}; delete `{SKELETON}` and run `tect generate` \
                 to use this release's skeleton",
                ranges.join(" and ")
            )),
        );
        return None;
    }

    let expected = [
        BEGIN,
        END,
        RECORD_BEGIN,
        RECORD_END,
        TAIL_BEGIN,
        TAIL_END,
        VALIDATE_BEGIN,
        VALIDATE_END,
    ];
    let mut found = true;
    for marker in expected {
        if !text.lines().any(|line| line == marker) {
            issues.push(
                Issue::new(format!("`{SKELETON}` has no `{marker}` line"), &src)
                    .help("the generated regions go between their marker pairs"),
            );
            found = false;
        }
    }
    // A region in the wrong place runs its step in the wrong stage.
    // The seal pass stops at the tail. A step before the tail runs again in
    // that pass and can move the digest the UKI embedded.
    let at: Option<Vec<usize>> = expected
        .iter()
        .map(|marker| text.lines().position(|line| line == *marker))
        .collect();
    if let Some(at) = at {
        if !at.windows(2).all(|pair| pair[0] < pair[1]) {
            issues.push(
                Issue::new(
                    format!("`{SKELETON}` puts its marker pairs out of order"),
                    &src,
                )
                .help(
                    "order the regions: component layers, build record, tail fragments, validate",
                ),
            );
            found = false;
        }
    }
    found.then_some(text)
}

/// Returns the line ranges of a skeleton that still holds the generated
/// record or validate steps, as 1-based inclusive numbers. A range is the
/// stretch of lines outside the marker regions. Its ARG lines go with the
/// RUN they feed.
fn old_spine(text: &str) -> Vec<(usize, usize)> {
    use emit::containerfile::{
        BEGIN, END, RECORD_BEGIN, RECORD_END, TAIL_BEGIN, TAIL_END, VALIDATE_BEGIN, VALIDATE_END,
    };

    let begins = [BEGIN, RECORD_BEGIN, TAIL_BEGIN, VALIDATE_BEGIN];
    let ends = [END, RECORD_END, TAIL_END, VALIDATE_END];
    let mut inside = false;
    let mut stretches: Vec<(usize, usize)> = Vec::new();
    let mut open: Option<usize> = None;
    for (index, line) in text.lines().enumerate() {
        if begins.contains(&line) || ends.contains(&line) {
            if let Some(first) = open.take() {
                stretches.push((first, index));
            }
            inside = begins.contains(&line);
        } else if inside {
            if let Some(first) = open.take() {
                stretches.push((first, index));
            }
        } else if open.is_none() {
            open = Some(index);
        }
    }
    if let Some(first) = open {
        stretches.push((first, text.lines().count()));
    }

    let keys = [
        "/generated/plan.json",
        "/ctx/tect build-record",
        "bootc container lint",
        "/ctx/tect validate-image",
    ];
    stretches
        .into_iter()
        .filter(|(first, last)| {
            text.lines()
                .skip(*first)
                .take(last - first)
                .any(|line| keys.iter().any(|key| line.contains(key)))
        })
        .filter_map(|(first, last)| {
            let mut from = first;
            let mut to = last.saturating_sub(1);
            while from <= to && text.lines().nth(from).is_some_and(|l| l.trim().is_empty()) {
                from += 1;
            }
            while to >= from && text.lines().nth(to).is_some_and(|l| l.trim().is_empty()) {
                to = to.saturating_sub(1);
            }
            (from <= to).then_some((from + 1, to + 1))
        })
        .collect()
}

/// Every manifest read and every image resolved, which is what any command
/// beyond a name needs.
pub(crate) struct Loaded {
    list: List,
    resolved: Vec<Resolved>,
    workflows: Vec<resolve::workflow::Declared>,
    /// What a workflow body's guarded regions are kept by.
    facts: Vec<&'static str>,
    index: provider::Index,
    issues: Issues,
    context: String,
    /// Components a closed script rule does not reach, gathered as each image
    /// resolved.
    ungoverned: Vec<String>,
}

pub(crate) fn load(root: &Path) -> Loaded {
    let (mut list, mut issues) = List::load(root);
    let mut ungoverned: Vec<String> = Vec::new();
    let context = context(&list, root);

    let disk = parse::disk::Disk::scan(root);
    let index = provider::Index::local(root, &list.sources, &disk);
    provenance::lock::check(root, &mut issues);

    let network = list.network;
    let schema_version = list.schema_version;
    let repo_src = list.repo_src.clone();
    let mut resolved: Vec<Resolved> = Vec::new();
    for image in &mut list.images {
        let mut load = |entry: &Entry, image: &Image| {
            let mut component = Component::load(
                entry,
                image,
                root,
                &disk,
                schema_version,
                &repo_src,
                &mut issues,
            )?;
            component.access = network.access(&component);
            Some(component)
        };
        // Taken out so a diagnostic can still read the image it was declared in.
        let mut entries = std::mem::take(&mut image.entries);
        for entry in &mut entries {
            entry.component = load(entry, image);
        }
        image.entries = entries;
        for issue in resolve::imports::expand(image, &disk, load) {
            issues.push(issue);
        }

        resolve::graph::suppress(image);
        let order = resolve::order::sort(image, &mut issues);
        resolve::order::apply(image, &order);
        resolve::graph::check_graph(image, &index, &mut issues);
        resolve::graph::check_fragments(image, &mut issues);
        provenance::check_network(image, network.scripts, root, &mut issues);
        ungoverned.extend(provenance::ungoverned(image, network.scripts));
        let shipped = resolve::overlay::index(image, &disk);
        resolve::overlay::check(image, &shipped, &mut issues);
        let collected = resolve::collect::resolve_collects(image, root, &disk, &mut issues);

        resolved.push(Resolved { shipped, collected });
    }

    // This runs after the imports, so an imported component counts as listed.
    parse::component::check_unlisted(&list, root, &disk, &mut issues);

    // A component listed by two images is one component to say this about.
    ungoverned.sort();
    ungoverned.dedup();

    // After the components, since what a workflow needs is a fact about them.
    let basis = resolve::workflow::Basis::of(&list);
    let workflows = resolve::workflow::resolve(&list, &basis, &mut issues);
    let facts = resolve::workflow::facts(
        &basis,
        list.scans_scheduled,
        list.publishes_scheduled,
        list.backend,
    );

    Loaded {
        list,
        resolved,
        workflows,
        facts,
        index,
        issues,
        context,
        ungoverned,
    }
}

/// What `generate` produced, against what is committed under `generated/`: an
/// artifact that differs, one that is not there, and one nothing generates.
pub(crate) fn verify(root: &Path, files: &[(PathBuf, String)], issues: &mut Issues) {
    const HELP: &str = "run `tect generate` and commit what it writes";

    // A repository that has never generated is missing every one of them, and
    // twenty-one copies of one sentence is not twenty-one findings. One says
    // it, and `tect generate` is the answer to all of them.
    let mut absent: Vec<String> = Vec::new();
    for (path, generated) in files {
        let name = path.display().to_string();
        let Ok(found) = std::fs::read_to_string(root.join(path)) else {
            absent.push(name);
            continue;
        };
        if found == *generated {
            continue;
        }
        let (span, line) = difference(&found, generated);
        issues.push(
            Issue::new(
                format!("`{name}` is not what this repository generates"),
                &Source::new(&name, found),
            )
            .at(span, line)
            .help(HELP),
        );
    }

    match absent.as_slice() {
        [] => {}
        [name] => issues
            .push(Issue::new(format!("`{name}` is not there"), &Source::new(name, "")).help(HELP)),
        // Named for the tree: the finding is about all of them, and pointing
        // the summary line at one arbitrary file reads as that file being the
        // problem.
        many => issues.push(
            Issue::new(
                format!("{} generated files are not there", many.len()),
                &Source::new(layout::GENERATED, ""),
            )
            .help(format!("{HELP}; it writes all {} of them", many.len())),
        ),
    }

    let stale = tracked(&layout::generated(root))
        .into_iter()
        .filter_map(|path| path.strip_prefix(root).map(Path::to_path_buf).ok())
        .filter(|path| !files.iter().any(|(emitted, _)| emitted == path))
        .chain(resolve::workflow::orphans(root, files));
    for path in stale {
        let name = path.display().to_string();
        issues.push(
            Issue::new(
                format!("nothing generates `{name}`"),
                &Source::new(&name, ""),
            )
            .help("delete it, or declare what it belongs to"),
        );
    }
}

/// Writes what `generate` produced, after clearing the directory so an image or
/// a component that is gone leaves with its files. Everything under `generated/`
/// is written from here, and so is every workflow the repository declares. A
/// workflow it no longer declares is removed by name: one the tool does not
/// ship is the repository's own.
pub fn write_generated(root: &Path, files: &[(PathBuf, String)]) -> Result<(), String> {
    let dir = layout::generated(root);
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    for path in resolve::workflow::orphans(root, files) {
        let path = root.join(path);
        std::fs::remove_file(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    }
    for (path, text) in files {
        let path = root.join(path);
        let dir = path.parent().unwrap_or(&path);
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
        std::fs::write(&path, text).map_err(|err| format!("{}: {err}", path.display()))?;
    }
    // CI calls the scripts directly, whatever mode the repository committed.
    let scripts = layout::generated(root).join(layout::SCRIPTS);
    for script in emit::SCRIPTS {
        use std::os::unix::fs::PermissionsExt;
        let path = scripts.join(script.name);
        if !path.is_file() {
            continue;
        }
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .map_err(|err| format!("{}: {err}", path.display()))?;
    }
    Ok(())
}

/// The first line two texts differ on, as its span in `found` and what was
/// generated in its place.
fn difference(found: &str, generated: &str) -> (diag::Span, String) {
    let mut offset = 0;
    let mut theirs = found.lines();
    let mut ours = generated.lines();
    loop {
        match (theirs.next(), ours.next()) {
            (Some(a), Some(b)) if a == b => offset += a.len() + 1,
            (a, b) => {
                let span = diag::Span {
                    offset,
                    len: a.map_or(0, str::len),
                };
                let label = match b {
                    Some(line) => format!("generated: {line}"),
                    None => "generated: nothing, the file ends above".to_string(),
                };
                return (span, label);
            }
        }
    }
}

/// Every file under `dir`, deepest last and sorted, so a run reports them in
/// the same order twice.
/// Joins a list the way a sentence reads it, as `a, b and c`.
pub(crate) fn said(items: &[String]) -> String {
    match items.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

pub(crate) fn tracked(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            out.extend(tracked(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skeleton_that_still_carries_the_old_steps_is_refused() {
        let root = std::env::temp_dir().join(format!("tect-skeleton-{}", std::process::id()));
        let path = root.join(emit::containerfile::SKELETON);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, init::SKELETON).unwrap();

        let mut issues = Issues::default();
        assert!(skeleton(&root, &mut issues).is_some());
        assert!(issues.findings().is_empty());

        let stale = "\
ARG BASE
FROM scratch AS ctx

# ---- BEGIN GENERATED (component layers) ----
# ---- END GENERATED ----

COPY --from=ctx /generated/plan.json /usr/share/tectonic/manifest.json

ARG BASE
RUN /ctx/tect build-record

RUN --network=none bootc container lint

# ---- BEGIN GENERATED (tail fragments) ----
# ---- END GENERATED (tail fragments) ----

ARG FAMILY
ARG CONTRACTS
RUN /ctx/tect validate-image
";
        std::fs::write(&path, stale).unwrap();
        let mut issues = Issues::default();
        assert!(skeleton(&root, &mut issues).is_none());
        let plain = issues.plain();
        assert!(
            plain.contains("still carries the generated"),
            "no refusal in {plain}"
        );
        assert!(plain.contains("at lines 7 to 12"), "{plain}");
        assert!(plain.contains("and 17 to 19"), "{plain}");

        let unordered = "\
# ---- BEGIN GENERATED (validate) ----
# ---- END GENERATED (validate) ----

# ---- BEGIN GENERATED (component layers) ----
# ---- END GENERATED ----

# ---- BEGIN GENERATED (build record) ----
# ---- END GENERATED (build record) ----

# ---- BEGIN GENERATED (tail fragments) ----
# ---- END GENERATED (tail fragments) ----
";
        std::fs::write(&path, unordered).unwrap();
        let mut issues = Issues::default();
        assert!(skeleton(&root, &mut issues).is_none());
        assert!(
            issues.plain().contains("out of order"),
            "{}",
            issues.plain()
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}

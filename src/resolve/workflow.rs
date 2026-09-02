//! The CI the tool ships, and what a repository's declaration makes of it. A
//! workflow it does not name is not written, so absent and present are the only
//! two states there are.

use crate::diag::{Issue, Issues};
use crate::layout;
use crate::model::image::List;
use std::path::{Path, PathBuf};

/// What a workflow cannot run without, which is shown beside it and refused
/// when it is missing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    Nothing,
    /// Both halves of the disk workflow, measured 2026-08-31 and failing for
    /// two different reasons. The iso is **Anaconda's**, which is Fedora's
    /// installer with no deb equivalent to point the builder at — say Anaconda
    /// and not *installer iso*, because an Anaconda-free one is a different
    /// question and `BACKLOG.md` holds it. The qcow2 dies in the **SELinux
    /// relabel**, `setfiles` against a policy a deb image does not carry.
    Fedora,
    /// A module taking a `KERNEL` build arg, which is what it tracks.
    Kernel,
}

impl Needs {
    /// What a row says instead of what the workflow is for, when it cannot run
    /// here.
    pub fn unmet(self) -> &'static str {
        match self {
            Self::Nothing => "",
            Self::Fedora => "needs a fedora image",
            Self::Kernel => "needs a module taking a KERNEL arg",
        }
    }
}

/// One workflow the tool ships: the body compiled in, what a person needs to
/// know to pick it, and where its schedule sits against the daily build.
pub struct Shipped {
    pub stem: &'static str,
    pub body: &'static str,
    pub about: &'static str,
    pub needs: Needs,
    /// Hours from the daily build, and the day of the week cron runs it on,
    /// for the ones that run on a schedule at all.
    pub at: Option<(i64, &'static str)>,
}

impl Shipped {
    pub fn file(&self) -> String {
        format!("{}.yml", self.stem)
    }

    /// Whether this repository can run it.
    pub fn met(&self, basis: &Basis) -> bool {
        match self.needs {
            Needs::Nothing => true,
            Needs::Fedora => basis.fedora,
            Needs::Kernel => basis.kernel,
        }
    }
}

macro_rules! shipped {
    ($stem:literal, $about:literal, $needs:expr, $at:expr) => {
        Shipped {
            stem: $stem,
            body: include_str!(concat!("../../assets/.github/workflows/", $stem, ".yml")),
            about: $about,
            needs: $needs,
            at: $at,
        }
    };
}

/// Every workflow a repository may ask for, which is every workflow there is.
/// The order is the order a picker draws them in.
pub const SHIPPED: &[Shipped] = &[
    shipped!(
        "build",
        "builds, scans, signs and publishes every image",
        Needs::Nothing,
        Some((0, "*"))
    ),
    shipped!(
        "build-disk",
        "builds a disk image and an installer iso",
        Needs::Fedora,
        None
    ),
    shipped!(
        "base-sig-probe",
        "checks each base for a signature and corrects it",
        Needs::Nothing,
        Some((-6, "*"))
    ),
    shipped!(
        "cleanup-registry",
        "prunes old versions of the images off the registry",
        Needs::Nothing,
        Some((2, "*"))
    ),
    shipped!(
        "kernel-freshness",
        "tracks the kernel a module pins, and proposes a bump",
        Needs::Kernel,
        Some((-3, "*"))
    ),
    shipped!(
        "smoke-test",
        "installs the image to a disk and boots it under qemu",
        // Nothing in its body is a family: it installs out of the published
        // image with `bootc install to-disk` and boots that, with no builder,
        // no Anaconda and no relabel. Whether a base can install itself is the
        // base's fact and the run is what measures it.
        Needs::Nothing,
        Some((-9, "1"))
    ),
];

pub fn find(stem: &str) -> Option<&'static Shipped> {
    SHIPPED.iter().find(|shipped| shipped.stem == stem)
}

/// The daily build time every other schedule hangs off, UTC, as cron reads it.
pub const DEFAULT_AT: (u32, u32) = (12, 30);

/// What the repository can run, which is what decides whether a workflow may be
/// asked for at all.
pub struct Basis {
    pub fedora: bool,
    pub kernel: bool,
}

impl Basis {
    /// A repository already declared. `kernel` is a module taking the arg the
    /// freshness workflow flips, which is the fact rather than the preference.
    pub fn of(list: &List) -> Self {
        Self {
            fedora: list.images.iter().any(|image| {
                image
                    .base
                    .as_ref()
                    .is_some_and(|base| base.family == FEDORA)
            }),
            kernel: list.images.iter().any(|image| {
                image
                    .modules()
                    .any(|module| module.args.iter().any(|arg| arg.name == KERNEL_ARG))
            }),
        }
    }

    /// The one being scaffolded, which has a base and no modules yet.
    pub fn scaffolding(family: &str) -> Self {
        Self {
            fedora: family == FEDORA,
            kernel: false,
        }
    }
}

/// The one family a disk is built from: `bootc-image-builder` installs with
/// Anaconda and relabels its buildroot with SELinux, and no deb image carries
/// either. `build-disk.yml` gates on it, and so does `tect vm`.
pub(crate) const FEDORA: &str = "fedora";
const KERNEL_ARG: &str = "KERNEL";

/// The workflows a module declaring `args` would make runnable that the
/// repository does not already declare. `Basis::of` derives the fact; this is
/// what asks about it. Nothing where the block names a workflow the tool does
/// not ship, since it cannot be rewritten without dropping that line.
pub fn unlocked(list: &List, args: &[String]) -> Vec<&'static Shipped> {
    if list.workflows.is_empty() || list.workflows.iter().any(|w| find(&w.name).is_none()) {
        return Vec::new();
    }
    let was = Basis::of(list);
    let now = Basis {
        fedora: was.fedora,
        kernel: was.kernel || args.iter().any(|arg| arg == KERNEL_ARG),
    };
    SHIPPED
        .iter()
        .filter(|shipped| shipped.met(&now) && !shipped.met(&was))
        .filter(|shipped| !list.workflows.iter().any(|w| w.name == shipped.stem))
        .collect()
}

/// The facts a body's guarded regions are kept by.
pub fn facts(basis: &Basis, scans_scheduled: bool, publishes_scheduled: bool) -> Vec<&'static str> {
    let mut facts = match basis.kernel {
        true => vec!["kernel"],
        false => vec!["no-kernel"],
    };
    facts.push(match publishes_scheduled {
        true => "scheduled-publish",
        false => "push-publish",
    });
    facts.push(match scans_scheduled || publishes_scheduled {
        true => "scheduled-scan",
        false => "push-scan",
    });
    facts
}

/// One workflow the repository asked for, ready to be written.
pub struct Declared {
    pub file: String,
    /// The cron line the emitter substitutes, for the scheduled ones.
    pub schedule: Option<String>,
    pub body: &'static str,
}

/// What the declaration names, in catalog order, with everything it names that
/// the tool does not ship or the repository cannot run reported.
pub fn resolve(list: &List, basis: &Basis, issues: &mut Issues) -> Vec<Declared> {
    for declared in &list.workflows {
        let Some(shipped) = find(&declared.name) else {
            let known: Vec<&str> = SHIPPED.iter().map(|s| s.stem).collect();
            issues.push(
                Issue::new(
                    format!("`{}` is not a workflow", declared.name),
                    &list.repo_src,
                )
                .at(declared.span, "nothing ships one by that name")
                .help(format!("workflows: {}", known.join(", "))),
            );
            continue;
        };
        if !shipped.met(basis) {
            issues.push(
                Issue::new(
                    format!("`{}` cannot run in this repository", declared.name),
                    &list.repo_src,
                )
                .at(declared.span, shipped.needs.unmet())
                .help("drop it from `workflows`, or declare what it needs"),
            );
        }
    }

    SHIPPED
        .iter()
        .filter(|shipped| list.workflows.iter().any(|w| w.name == shipped.stem))
        .map(|shipped| Declared {
            file: shipped.file(),
            schedule: shipped
                .at
                .map(|(offset, day)| cron(list.workflows_at, offset, day)),
            body: shipped.body,
        })
        .collect()
}

/// A schedule as its offset from the daily build, so a repository moves all of
/// them by moving one value.
fn cron((hour, minute): (u32, u32), offset: i64, day: &str) -> String {
    let hour = (i64::from(hour) + offset).rem_euclid(24);
    format!("{minute} {hour} * * {day}")
}

/// Workflow files the tool owns that nothing generates any more. One it does
/// not ship is the repository's own and is left where it is.
pub fn orphans(root: &Path, generated: &[(PathBuf, String)]) -> Vec<PathBuf> {
    SHIPPED
        .iter()
        .map(|shipped| PathBuf::from(layout::WORKFLOW_DIR).join(shipped.file()))
        .filter(|path| root.join(path).is_file())
        .filter(|path| !generated.iter().any(|(written, _)| written == path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_body_carries_the_markers_its_row_claims() {
        for shipped in SHIPPED {
            let scheduled = shipped.body.contains("# tect:schedule");
            assert_eq!(
                scheduled,
                shipped.at.is_some(),
                "{} disagrees about having a schedule",
                shipped.stem
            );
            assert!(
                shipped.about.len() <= 55,
                "{} does not fit a picker row",
                shipped.stem
            );
        }
    }

    #[test]
    fn a_deb_repository_may_smoke_test_and_may_not_build_a_disk() {
        let deb = Basis {
            fedora: false,
            kernel: false,
        };
        let met = |stem: &str| find(stem).expect(stem).met(&deb);
        assert!(met("build"));
        assert!(
            met("smoke-test"),
            "it installs out of the image, not a family"
        );
        assert!(!met("build-disk"), "the installer iso is Anaconda's");
    }

    #[test]
    fn a_schedule_is_the_build_time_plus_its_offset() {
        assert_eq!(cron((12, 30), 0, "*"), "30 12 * * *");
        assert_eq!(cron((12, 30), 2, "*"), "30 14 * * *");
        assert_eq!(cron((12, 30), -6, "*"), "30 6 * * *");
        // Before midnight rather than a negative hour.
        assert_eq!(cron((3, 0), -9, "1"), "0 18 * * 1");
    }

    #[test]
    fn publishing_on_schedule_also_schedules_scanning() {
        let basis = Basis {
            fedora: true,
            kernel: false,
        };
        assert_eq!(
            facts(&basis, false, false),
            ["no-kernel", "push-publish", "push-scan"]
        );
        assert_eq!(
            facts(&basis, false, true),
            ["no-kernel", "scheduled-publish", "scheduled-scan"]
        );
        assert_eq!(
            facts(&basis, true, false),
            ["no-kernel", "push-publish", "scheduled-scan"]
        );
        assert_eq!(
            facts(&basis, true, true),
            ["no-kernel", "scheduled-publish", "scheduled-scan"]
        );
    }
}

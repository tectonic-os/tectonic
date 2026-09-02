//! The disk the repository's own script builds and boots, run as it stands.
//!
//! Nothing here reimplements it: rootful podman, `podman image scp`,
//! bootc-image-builder under `--privileged` and qemu port selection are
//! shell-shaped work, and the script is interactive, so this execs it the way
//! `build` execs the container backend.

use crate::command::Spec;
use crate::copy;
use crate::prompt::Prompt;
use crate::ui::Choice;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::Command;

/// Generated in place by `emit::SCRIPTS`, which is what lets this call it: a
/// scaffolded copy would be the repository's to edit, and then `tect vm`
/// would do whatever that edit says.
const SCRIPT: &str = "scripts/vm.sh";

/// Measured 2026-08-31 against both deb bases: an Ubuntu image generates a
/// manifest and then dies relabelling its buildroot, `setfiles` against an
/// SELinux policy the image does not carry, and a Debian one is refused before
/// that for omitting `VERSION_ID` from os-release. The generated
/// `build-disk.yml` skips the job on the same field, and it is the same
/// constant, so this is that gate where a person hits it.
use crate::resolve::workflow::FEDORA;

/// What the container image is converted into, and what each one is.
const TYPES: [(&str, &str); 3] = [
    ("qcow2", copy::DISK_QCOW2),
    ("raw", copy::DISK_RAW),
    ("iso", copy::DISK_ISO),
];

/// What the flags gave, in the script's own spelling.
pub struct Options {
    pub target: Option<String>,
    pub image: Option<String>,
    pub tag: Option<String>,
    pub ram: Option<String>,
    pub rebuild: bool,
}

/// Replaces this process with the script. Returns `Ok` only when the picker
/// was left, which writes nothing and exits 0.
pub fn run(
    root: &Path,
    spec: &Spec,
    given: Option<&str>,
    opts: &Options,
    prompt: &Prompt,
) -> Result<(), String> {
    let Some(kind) = kind(spec, given, prompt)? else {
        return Ok(());
    };
    // Only where a disk would actually be converted. The script boots what is
    // already there, so refusing `spawn` or `run` against an existing disk
    // would refuse the one thing that does work on this family.
    if converts(root, spec, kind, opts) {
        if let Some(family) = family(root, opts).filter(|family| family != FEDORA) {
            return Err(format!(
                "the `{family}` family builds no disk here: bootc-image-builder installs with \
                 Anaconda and relabels its buildroot with SELinux, and a `{family}` image carries \
                 neither.\n\nhelp: push the image and `bootc install to-disk --via-loopback \
                 --composefs-backend --filesystem ext4` writes one from the pushed ref, and \
                 `tect vm spawn` boots what that leaves"
            ));
        }
    }
    let script = root.join(SCRIPT);
    if !script.is_file() {
        return Err(format!(
            "{} is not there; `tect generate` writes it",
            script.display()
        ));
    }
    Err(format!(
        "{}: {}",
        script.display(),
        Command::new(&script).args(argv(spec, kind, opts)).exec()
    ))
}

/// Whether this run would convert the container image into a disk, which is
/// the only thing the family decides. Mirrors `vm.sh`: it converts for `build`,
/// for `--rebuild`, and when the disk is not there yet.
fn converts(root: &Path, spec: &Spec, kind: &str, opts: &Options) -> bool {
    if spec.noun == "build" || opts.rebuild {
        return true;
    }
    let at = match kind {
        "iso" => "out/bootiso/install.iso".to_string(),
        kind => format!("out/{kind}/disk.{kind}"),
    };
    !root.join(at).is_file()
}

/// The family of what would be converted, as the repository declares it, or
/// nothing where this cannot know: an `--image` names a ref that need not be
/// one of this repository's at all, and a target it does not declare is the
/// script's own to refuse.
fn family(root: &Path, opts: &Options) -> Option<String> {
    if opts.image.is_some() {
        return None;
    }
    let list = crate::model::image::List::load(root).0;
    let target = match &opts.target {
        Some(named) => list.find_target(named).ok()?,
        None => list.ungated_target()?,
    };
    let image = list.images.iter().find(|image| image.id == target.image)?;
    image
        .base
        .as_ref()
        .map(|base| base.family.clone())
        .filter(|family| !family.is_empty())
}

/// The whole command line the script is given.
fn argv(spec: &Spec, kind: &str, opts: &Options) -> Vec<String> {
    let mut args = vec![spec.noun.to_string(), kind.to_string()];
    for (flag, value) in [
        ("--target", &opts.target),
        ("--image", &opts.image),
        ("--tag", &opts.tag),
        ("--ram", &opts.ram),
    ] {
        if let Some(value) = value {
            args.extend([flag.to_string(), value.clone()]);
        }
    }
    if opts.rebuild {
        args.push("--rebuild".to_string());
    }
    args
}

/// The type named, else the one picked, else a refusal naming them.
fn kind(spec: &Spec, given: Option<&str>, prompt: &Prompt) -> Result<Option<&'static str>, String> {
    let names = || match TYPES
        .iter()
        .map(|(name, _)| format!("`{name}`"))
        .collect::<Vec<_>>()
        .split_last()
    {
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    };
    if let Some(named) = given {
        return match TYPES.iter().find(|(name, _)| *name == named) {
            Some((name, _)) => Ok(Some(name)),
            None => Err(format!(
                "`{}` takes {}, not `{named}`",
                spec.name(),
                names()
            )),
        };
    }
    if !prompt.draws() {
        return Err(format!("`{}` takes {}", spec.name(), names()));
    }
    Ok(prompt
        .choose(copy::WHICH_DISK, &rows(spec))?
        .map(|at| TYPES[at].0))
}

/// One row per type, in the order `TYPES` holds them, so an answer indexes
/// straight back into it. `spawn` boots a disk rather than an installer, so
/// its iso row is drawn and refused rather than left out.
fn rows(spec: &Spec) -> Vec<Choice> {
    TYPES
        .iter()
        .map(|(name, about)| match (*name, spec.noun) {
            ("iso", "spawn") => Choice::new(*name, copy::NO_ISO_SPAWN).unavailable(),
            _ => Choice::new(*name, *about),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Verb;

    #[test]
    fn the_script_it_execs_is_one_the_repository_generates() {
        assert!(crate::emit::SCRIPTS.iter().any(|(path, _)| *path == SCRIPT));
    }

    #[test]
    fn the_noun_is_the_scripts_command_and_the_flags_are_its_own() {
        let opts = Options {
            target: Some("desktop/dx".into()),
            image: None,
            tag: Some("42".into()),
            ram: None,
            rebuild: true,
        };
        assert_eq!(
            argv(Verb::VmRun.spec(), "qcow2", &opts),
            [
                "run",
                "qcow2",
                "--target",
                "desktop/dx",
                "--tag",
                "42",
                "--rebuild"
            ]
        );
    }

    #[test]
    fn spawn_shows_the_iso_it_cannot_boot_rather_than_hiding_it() {
        let iso = |spec| rows(spec).pop().expect("iso is the last row");
        assert!(!iso(Verb::VmSpawn.spec()).available);
        assert!(iso(Verb::VmRun.spec()).available);
    }

    /// A `debian` image is said no to before anything is pulled and before
    /// sudo is asked for, rather than ten minutes in, inside osbuild. The disk
    /// type is settled first, since the guard is about converting one and a
    /// type is free to resolve.
    #[test]
    fn a_family_the_builder_cannot_convert_is_refused_before_anything_is_pulled() {
        let root = std::env::temp_dir().join(format!("tect-vm-family.{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        crate::init::put(&root.join("repo.kdl"), "schema-version 1\nname \"Deb\"\n").unwrap();
        crate::init::put(
            &root.join("deb.image.kdl"),
            "image {\n    name \"deb\"\n\n    base \"docker.io/library/debian:forky\" {\n\
             \x20       family \"debian\"\n    }\n\n    modules {\n    }\n}\n",
        )
        .unwrap();
        let opts = |image: Option<&str>| Options {
            target: None,
            image: image.map(str::to_string),
            tag: None,
            ram: None,
            rebuild: false,
        };
        assert_eq!(family(&root, &opts(None)).as_deref(), Some("debian"));
        let refuse = |kind, opts: &Options| {
            run(
                &root,
                Verb::VmRun.spec(),
                Some(kind),
                opts,
                &Prompt::silent(),
            )
            .unwrap_err()
        };

        // The refusal itself: no sudo, no pull, no osbuild.
        let err = refuse("qcow2", &opts(None));
        assert!(
            err.starts_with("the `debian` family builds no disk here"),
            "{err}"
        );

        // With no type named and nobody to ask, the type is what is missing,
        // and that is what it says rather than reaching for the family.
        let unasked = run(
            &root,
            Verb::VmRun.spec(),
            None,
            &opts(None),
            &Prompt::silent(),
        )
        .unwrap_err();
        assert!(
            unasked.contains("takes `qcow2`, `raw` or `iso`"),
            "{unasked}"
        );

        // A disk that is already there is booted rather than rebuilt, so the
        // family has nothing to say about it and the guard stands aside. This
        // is what lets a deb disk built by hand be booted by the tool.
        crate::init::put(&root.join("out/raw/disk.raw"), "").unwrap();
        assert!(!converts(&root, Verb::VmSpawn.spec(), "raw", &opts(None)));
        assert!(!converts(&root, Verb::VmRun.spec(), "raw", &opts(None)));
        let booted = refuse("raw", &opts(None));
        assert!(booted.contains("vm.sh is not there"), "{booted}");

        // `build` converts always, `--rebuild` converts always, and a type with
        // no disk yet converts.
        assert!(converts(&root, Verb::VmBuild.spec(), "raw", &opts(None)));
        assert!(converts(&root, Verb::VmRun.spec(), "qcow2", &opts(None)));
        let rebuilding = Options {
            rebuild: true,
            ..opts(None)
        };
        assert!(converts(&root, Verb::VmSpawn.spec(), "raw", &rebuilding));

        // An `--image` names a ref this repository need not describe at all.
        assert_eq!(family(&root, &opts(Some("localhost/other"))), None);

        // And a fedora target is never refused: a wrong constant here would
        // refuse every repository and nothing else would catch it.
        crate::init::put(
            &root.join("fed.image.kdl"),
            "image {\n    name \"fed\"\n\n    base \"quay.io/fedora/fedora-bootc:44\" {\n\
             \x20       family \"fedora\"\n    }\n\n    modules {\n    }\n}\n",
        )
        .unwrap();
        crate::init::put(
            &root.join("repo.kdl"),
            "schema-version 1\nname \"Deb\"\ndefault-image \"fed\"\n",
        )
        .unwrap();
        assert_eq!(family(&root, &opts(None)).as_deref(), Some("fedora"));
        let fedora = refuse("qcow2", &opts(None));
        assert!(fedora.contains("vm.sh is not there"), "{fedora}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_type_that_is_not_one_of_the_three_is_refused_by_name() {
        let silent = Prompt::silent();
        let spec = Verb::VmBuild.spec();
        assert!(kind(spec, Some("vmdk"), &silent).is_err());
        assert_eq!(kind(spec, Some("raw"), &silent).unwrap(), Some("raw"));
        // Nobody to ask, and nothing named: the refusal names the three.
        assert!(kind(spec, None, &silent).is_err());
    }
}

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

#[derive(Default)]
struct Access {
    login: bool,
    ssh_key: Option<String>,
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
    let installer = installer(root, spec, kind, opts);
    let live = match kind == "iso" && converts(root, spec, kind, opts) {
        true => Some(stage(root, opts)?),
        false => None,
    };
    let access = access(root, kind, opts);
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
        Command::new(&script)
            .args(argv(spec, kind, opts, installer, live.as_deref(), &access))
            .exec()
    ))
}

/// Where the media is assembled, which is also where the disk it writes lands.
const BOOTISO: &str = "out/bootiso";

/// The build context `vm.sh` hands `podman build`, and the recipe tacklebox
/// assembles the media from. Neither is a `generate` output: both depend on
/// `--target`, `--tag` and `$IMAGE_REGISTRY`, which are build-time and not
/// commit-time, so they are staged here the way the installer and the login
/// access already are.
fn documents(
    list: &crate::model::image::List,
    name: &str,
    image: &str,
    imgref: &str,
) -> Result<Vec<(&'static str, String)>, String> {
    use crate::emit::recipe;
    // `targetImgref` is the installed machine's update origin, so a medium
    // built against a local namespace would write `localhost/...` into a
    // machine. A disk records nothing and gets away with it; an iso does not.
    if imgref.starts_with("localhost/") {
        return Err(format!(
            "`{imgref}` is where the installed machine would look for its updates, \
             and nothing serves it.\n\nhelp: set $IMAGE_REGISTRY to where `{name}` \
             publishes"
        ));
    }
    let refuse = || {
        format!(
            "there is no measured install recipe for `{name}`: its base family is \
             none of `fedora`, `debian` or `ubuntu`, and guessing one erases a disk \
             before it fails to boot"
        )
    };
    // Both references fisherman is given are the published one, and that is
    // not a lost split — it is where the split already happened. Tacklebox
    // embeds the local bytes *under* the published name, so on this medium
    // that name is what the bytes are called; asking fisherman for
    // `localhost/...` would miss the store and reach for a registry. The local
    // reference is the media recipe's `source` and appears nowhere else.
    let build = recipe::build(list, name, imgref, imgref, &[recipe::STORE.to_string()])
        .ok_or_else(refuse)?;
    let media = recipe::media(list, name, image, imgref).ok_or_else(refuse)?;
    Ok(vec![
        ("recipe.json", build.render()),
        ("media.json", media.render()),
        ("Containerfile", recipe::LIVE_ENV.to_string()),
        ("efi-from-image.patch", recipe::EFI_PATCH.to_string()),
    ])
}

/// Writes them, and answers the live environment's own reference: the script
/// builds and boots that image and has no JSON reader to find it with.
fn stage(root: &Path, opts: &Options) -> Result<String, String> {
    let list = crate::model::image::List::load(root).0;
    let target = target(&list, opts)
        .ok_or("no target to build an installer iso for; name one with `--target`")?;
    let published = target.published();
    let tag = crate::registry::tag(opts.tag.as_ref());
    let imgref =
        crate::registry::reference(&list, root, opts.target.as_deref(), opts.tag.as_ref())?;
    // What the script computes for itself on every other path: an `--image`
    // names the bytes, and the published reference above stays the origin.
    let image = match &opts.image {
        Some(named) => format!("{named}:{tag}"),
        None => crate::registry::at("localhost", &published, &tag),
    };
    for (name, body) in documents(&list, &target.to_string(), &image, &imgref)? {
        crate::init::put(&root.join(BOOTISO).join(name), &body)?;
    }
    frontend(&root.join(BOOTISO).join("tect"))?;
    Ok(crate::emit::recipe::live(&published))
}

/// The frontend the media autostarts is *this* binary, copied into the build
/// context beside the recipes. Not the published release: media assembled from
/// a tree exists to prove that tree, and fetching a different `tect` would
/// install with an installer nobody here is looking at.
///
/// A binary that cannot run in the live environment — a glibc build carried
/// onto another distribution's libc — is caught by the `--version` the
/// Containerfile runs, so the failure is an ISO build and not a boot.
fn frontend(to: &Path) -> Result<(), String> {
    let from = std::env::current_exe().map_err(|err| format!("this binary: {err}"))?;
    std::fs::copy(&from, to)
        .map(|_| ())
        .map_err(|err| format!("{} -> {}: {err}", from.display(), to.display()))
}

/// Whether this run would convert the container image into a disk, which is
/// the only thing the family decides. Mirrors `vm.sh`: it converts for `build`,
/// for `--rebuild`, and when the disk is not there yet.
fn converts(root: &Path, spec: &Spec, kind: &str, opts: &Options) -> bool {
    if spec.noun == "build" || opts.rebuild {
        return true;
    }
    let at = match kind {
        "iso" => format!("{BOOTISO}/install.iso"),
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
    let target = target(&list, opts)?;
    let image = list.images.iter().find(|image| image.id == target.image)?;
    image
        .base
        .as_ref()
        .map(|base| base.family.clone())
        .filter(|family| !family.is_empty())
}

fn target(list: &crate::model::image::List, opts: &Options) -> Option<crate::model::image::Target> {
    match &opts.target {
        Some(named) => list.find_target(named).ok(),
        None => list.ungated_target(),
    }
}

/// Whether the selected target imports non-root password credentials, and the
/// SSH public key `bootc install` can provision. An arbitrary image ref is
/// deliberately not assumed to match this source.
///
/// Neither half applies to an installer iso: it boots a live environment that
/// autologins root and creates the installed machine's account itself, so
/// asking for a VM password there would provision an unprivileged account
/// nobody needs and block a run with no terminal to ask on.
fn access(root: &Path, kind: &str, opts: &Options) -> Access {
    if kind == "iso" || opts.image.is_some() {
        return Access::default();
    }
    let list = crate::model::image::List::load(root).0;
    let Some(target) = target(&list, opts) else {
        return Access::default();
    };
    let Some((_, _, entries)) = crate::emit::plan::of_target(&list, &target.to_string()) else {
        return Access::default();
    };
    let dirs: Vec<_> = entries.iter().map(|entry| entry.dir()).collect();
    let disk = crate::parse::disk::Disk::scan(root);
    let keys: Vec<_> = disk
        .keys
        .get("ssh")
        .into_iter()
        .flatten()
        .filter(|(dir, _)| dirs.contains(dir))
        .map(|(_, key)| key)
        .collect();
    let existing: Vec<_> = keys
        .iter()
        .map(|key| crate::layout::public_key(root, &key.public))
        .filter(|path| crate::layout::nonempty(path))
        .collect();
    Access {
        login: dirs.iter().any(|dir| imports_passwords(root, dir)),
        ssh_key: match existing.as_slice() {
            [path] => Some(path.display().to_string()),
            _ => None,
        },
    }
}

fn imports_passwords(root: &Path, module: &str) -> bool {
    let mut dirs = vec![crate::layout::module(root, module).join(crate::layout::OVERLAY)];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if std::fs::read_to_string(path).is_ok_and(|text| {
                text.lines()
                    .any(|line| line.trim() == "ImportCredential=passwd.hashed-password.*")
            }) {
                return true;
            }
        }
    }
    false
}

/// Which installer converts this image, where one would be converted at all:
/// the script boots what is already there, and cannot read the family for
/// itself. `None` is the script's own default, `bib`. An `iso` reaches
/// neither converter — fisherman installs the target through podman, so one
/// live environment installs every family — and so names no installer.
///
/// Lifted out of `run`, which `exec`s and so cannot be tested past this point:
/// this is the whole of what the change decides, and it is worth asserting.
fn installer(root: &Path, spec: &Spec, kind: &str, opts: &Options) -> Option<&'static str> {
    if kind == "iso" || !converts(root, spec, kind, opts) {
        return None;
    }
    family(root, opts)
        .filter(|family| family != FEDORA)
        .map(|_| BOOTC)
}

/// What `vm.sh` calls `bootc install to-disk`. The script cannot read the base
/// family — it has no JSON reader and `plan.json` is where the family is — so
/// the tool, which already reads it for the refusal above, names the installer
/// instead. `bib` is the script's own default and is never passed.
const BOOTC: &str = "bootc";

/// The whole command line the script is given.
fn argv(
    spec: &Spec,
    kind: &str,
    opts: &Options,
    installer: Option<&str>,
    live: Option<&str>,
    access: &Access,
) -> Vec<String> {
    let mut args = vec![spec.noun.to_string(), kind.to_string()];
    if let Some(installer) = installer {
        args.extend(["--installer".to_string(), installer.to_string()]);
    }
    if let Some(live) = live {
        args.extend(["--live-image".to_string(), live.to_string()]);
    }
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
    if access.login {
        args.push("--login".to_string());
    }
    if let Some(key) = &access.ssh_key {
        args.extend(["--ssh-key".to_string(), key.clone()]);
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
            argv(
                Verb::VmRun.spec(),
                "qcow2",
                &opts,
                None,
                None,
                &Access::default()
            ),
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
        // The installer is the tool's to name, since the script cannot read the
        // family. `bib` is the script's default and is never passed.
        assert_eq!(
            argv(
                Verb::VmBuild.spec(),
                "raw",
                &opts,
                Some(BOOTC),
                None,
                &Access::default()
            )[..4],
            ["build", "raw", "--installer", "bootc"]
        );
    }

    #[test]
    fn spawn_shows_the_iso_it_cannot_boot_rather_than_hiding_it() {
        let iso = |spec| rows(spec).pop().expect("iso is the last row");
        assert!(!iso(Verb::VmSpawn.spec()).available);
        assert!(iso(Verb::VmRun.spec()).available);
    }

    /// A `debian` image is converted by bootc rather than by the builder that
    /// cannot convert it, decided before anything is pulled and before sudo is
    /// asked for rather than ten minutes in, inside osbuild. The disk type is
    /// settled first, since the choice is about converting one and a type is
    /// free to resolve.
    #[test]
    fn the_family_names_the_converter_before_anything_is_pulled() {
        let root = std::env::temp_dir().join(format!("tect-vm-family.{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        crate::init::put(&root.join("repo.kdl"), "schema-version 1\nname \"Deb\"\n").unwrap();
        crate::init::put(
            &root.join("deb.image.kdl"),
            "image {\n    name \"deb\"\n\n    base \"docker.io/library/debian:forky\" {\n\
             \x20       family \"debian\"\n    }\n\n    modules {\n        module \"login\"\n    }\n}\n",
        )
        .unwrap();
        crate::init::put(
            &root.join("modules/login/module.kdl"),
            "description \"login\"\nsupports \"debian\"\nkey \"ssh\" {\n    generator \"ssh-keygen\"\n    public \"/usr/lib/tectonic/authorized_keys\"\n    private \"id_ed25519\"\n}\n",
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
        let declared = access(&root, "raw", &opts(None));
        assert!(!declared.login);
        assert!(declared.ssh_key.is_none());
        crate::init::put(
            &root.join(
                "modules/login/files/usr/lib/systemd/system/systemd-sysusers.service.d/login.conf",
            ),
            "[Service]\nImportCredential=passwd.hashed-password.*\n",
        )
        .unwrap();
        assert!(access(&root, "raw", &opts(None)).login);
        crate::init::put(
            &crate::layout::public_key(&root, "/usr/lib/tectonic/authorized_keys"),
            "ssh-ed25519 AAAA test\n",
        )
        .unwrap();
        let recorded = access(&root, "raw", &opts(None));
        assert!(recorded.login);
        assert!(recorded.ssh_key.is_some());
        let args = argv(
            Verb::VmRun.spec(),
            "raw",
            &opts(None),
            Some(BOOTC),
            None,
            &recorded,
        );
        assert!(args.contains(&"--login".to_string()));
        assert!(args.contains(&"--ssh-key".to_string()));
        assert!(!access(&root, "raw", &opts(Some("localhost/other"))).login);
        // An iso asks for no password: it autologins root and the installer
        // creates the account on the machine it installs.
        assert!(!access(&root, "iso", &opts(None)).login);
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

        // The whole of what this decides: a deb target that would convert a
        // disk is installed with bootc rather than refused, and the script is
        // told so, because it cannot read the family for itself.
        let build = Verb::VmBuild.spec();
        for kind in ["qcow2", "raw"] {
            assert_eq!(installer(&root, build, kind, &opts(None)), Some(BOOTC));
        }
        // So what a deb target hits is the missing script rather than a wall.
        let installed = refuse("qcow2", &opts(None));
        assert!(installed.contains("vm.sh is not there"), "{installed}");
        // An iso reaches neither converter: the media boots a live environment
        // that installs the target through podman, whatever family it is.
        assert_eq!(installer(&root, build, "iso", &opts(None)), None);

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
        // And nothing is named for it, so the script boots what is there.
        assert_eq!(
            installer(&root, Verb::VmRun.spec(), "raw", &opts(None)),
            None
        );
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
        // Fedora keeps the script's own default, and its iso is not refused.
        for kind in ["qcow2", "raw", "iso"] {
            assert_eq!(installer(&root, build, kind, &opts(None)), None);
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    /// What an iso stages, and the two refusals that are the whole point of
    /// staging it in the tool rather than in the script. Off the real
    /// `deb-families` fixture, so the recipe is a declaration's and not a
    /// constructed one, and off `documents` rather than `stage` because the
    /// references are the caller's — resolving them reads $IMAGE_REGISTRY and
    /// a git remote, and neither belongs in a unit test.
    #[test]
    fn an_iso_stages_both_recipes_and_refuses_a_local_update_origin() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/repos/deb-families");
        let list = crate::model::image::List::load(&root).0;
        let staged = documents(
            &list,
            "forky",
            "localhost/forky:latest",
            "ghcr.io/someone/forky:latest",
        )
        .expect("a debian target has a measured recipe");
        let named: Vec<_> = staged.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            named,
            [
                "recipe.json",
                "media.json",
                "Containerfile",
                "efi-from-image.patch"
            ]
        );
        let at = |want| staged.iter().find(|(name, _)| *name == want).unwrap();
        // The store is named in both halves it has to be named in, and they
        // are the halves nothing upstream connects: the recipe reaches the
        // install container, the storage.conf reaches the pull before it.
        assert!(at("recipe.json").1.contains(crate::emit::recipe::STORE));
        // What fisherman installs is the name the media holds the bytes under,
        // which is the published one: the local build appears only as the
        // media recipe's source, and naming it here reaches for a registry.
        assert!(at("recipe.json")
            .1
            .contains("\"image\": \"ghcr.io/someone/forky:latest\""));
        assert!(!at("recipe.json").1.contains("localhost/"));
        assert!(at("Containerfile").1.contains(crate::emit::recipe::STORE));
        // The media embeds the local bytes under the published name, which is
        // what makes the install offline and the update origin right at once.
        assert!(at("media.json")
            .1
            .contains("\"source\": \"localhost/forky:latest\""));
        assert!(at("media.json")
            .1
            .contains("\"ref\": \"ghcr.io/someone/forky:latest\""));

        // A local namespace is where a disk gets away with recording nothing
        // and a machine does not: refuse it by name rather than install one.
        let local = documents(
            &list,
            "forky",
            "localhost/forky:latest",
            "localhost/forky:latest",
        )
        .unwrap_err();
        assert!(local.contains("IMAGE_REGISTRY"), "{local}");
        // And a family with no measured answer is refused before a disk is
        // erased, not after.
        let unknown =
            documents(&list, "not-a-target", "image", "ghcr.io/someone/x:latest").unwrap_err();
        assert!(unknown.contains("no measured install recipe"), "{unknown}");
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

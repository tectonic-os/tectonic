//! The whole argv a container build runs with, derived from the plan, and then
//! the backend that runs it.

use crate::emit::plan::{contract_files, of_target, pinned, preset_files, provides, unique_pairs};
use crate::layout;
use crate::model::image::{List, NO_FLAVOUR};
use crate::model::module::Module;
use crate::provenance::build as record;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::Command;

#[derive(Default)]
pub struct Options {
    pub target: Option<String>,
    pub kernel: Option<String>,
    pub tags: Vec<String>,
    pub secrets: Vec<String>,
    pub backend: Option<String>,
    pub oci_output: Option<String>,
    pub no_cache_from: bool,
    pub cache_to: bool,
}

/// Why a build came back without becoming the backend.
pub enum Stopped {
    /// The repository is wrong, and every problem was printed.
    Repository,
    /// A UKI target's two passes ran to completion; the single pass would have
    /// replaced this process instead.
    Built,
}

/// Fetches, verifies, then builds. A single-pass build replaces this process
/// with the backend and returns only when the repository is wrong; a UKI
/// target's two passes run to completion and return [`Stopped::Built`].
pub fn run(root: &Path, opts: &Options) -> Result<Stopped, String> {
    let backend = match opts.backend.clone().or_else(|| env("BUILD_BACKEND")) {
        None => "buildah".to_string(),
        Some(name) if name == "buildx" || name == "buildah" => name,
        Some(name) => return Err(format!("unknown backend `{name}` (buildx or buildah)")),
    };

    let (list, mut issues, context) = crate::declarations(root);
    if opts.target.is_none() {
        if let Some(issue) = list.no_default() {
            issues.push(issue);
        }
    }
    if issues.report(&context) {
        return Ok(Stopped::Repository);
    }
    let target = target(&list, opts.target.as_deref())?;

    // Nothing is fetched or regenerated here: a build proves the committed
    // files are current. `tect fetch modules` and `tect generate` are what
    // change the repository, and `vm.sh --rebuild` runs both before this. The
    // rest of the build works off what this read, never a second reading.
    let gate = crate::run(crate::Command::Verify, None, root);
    if gate.issues.report(&gate.context) {
        return Ok(Stopped::Repository);
    }
    eprintln!(
        "tect: {} generated files match the manifests",
        gate.files.len()
    );

    let (list, resolved) = (gate.list, gate.resolved);
    let (image, flavour, entries) =
        of_target(&list, &target).ok_or_else(|| format!("`{target}` has no image"))?;
    let modules: Vec<&Module> = entries.iter().filter_map(|e| e.module.as_ref()).collect();
    let presets = list
        .images
        .iter()
        .position(|i| i.id == image.id)
        .map(|i| {
            preset_files(
                image,
                &resolved[i].shipped,
                flavour.as_deref().unwrap_or(NO_FLAVOUR),
            )
        })
        .unwrap_or_default();
    let published = list.find_target(&target)?.published();

    let version = match env("IMAGE_VERSION") {
        Some(named) => named,
        None => today()?,
    };
    // One resolution, not two: `BASE` in the environment is what CI already
    // resolved for the image label, so the label and the record agree.
    let declared_base = image.base.as_ref().map(|b| b.image.clone());
    let resolved_base = match (env("BASE"), &declared_base) {
        (Some(given), _) => Some(given),
        (None, Some(declared)) => record::base(declared),
        (None, None) => None,
    };
    let source_commit = record::source_commit(root);
    crate::provenance::enforce_build(
        list.audit_enforce,
        resolved_base.as_deref(),
        source_commit.as_deref(),
    )?;
    if let Some(declared) = &declared_base {
        match &resolved_base {
            Some(resolved) => eprintln!("tect: base {declared} -> {resolved}"),
            None => {
                eprintln!("tect: {declared} did not resolve to a digest; the build record says so")
            }
        }
    }
    let resolved_base = resolved_base.unwrap_or_else(|| declared_base.clone().unwrap_or_default());
    let source_commit = source_commit.unwrap_or_default();

    // A cloned asset's verifier is the commit its selector names, which only
    // the remote can answer.
    let resolutions: Vec<String> = pinned(&modules)
        .into_iter()
        .filter(|(_, asset)| asset.pin.cloned())
        .filter_map(|(module, asset)| {
            let url = asset.pin.url.as_deref()?;
            let version = asset.pin.version.as_deref()?;
            let commit = record::clone_commit(url, version)?;
            Some(format!("{}|{}|{version}|{commit}", module.path, asset.name))
        })
        .collect();

    let namespace = crate::registry::namespace(root);

    // Resolved here rather than emitted as an image identity ARG: a flavour may
    // declare its own, and the generated file is one per image.
    let conforms =
        crate::scap::profile_name(image.conforms_of(flavour.as_deref().unwrap_or(NO_FLAVOUR)))
            .to_string();
    // Refused rather than guessed: a base measured against the wrong benchmark
    // returns numbers, and the image is remediated to a profile nobody chose.
    let scap_content = match conforms.is_empty() {
        true => String::new(),
        false => crate::scap::content_for(root, &list, Some(&target))?,
    };
    // Both lists include the base, which claims the way a module does.
    let (scap_claimed, scap_refused) = crate::scap::exclusions(image);
    let mut build_args = vec![
        format!("FLAVOUR={}", flavour.unwrap_or_default()),
        format!("IMAGE_VERSION={version}"),
        format!("IMAGE_REGISTRY={}", namespace.clone().unwrap_or_default()),
        format!(
            "CONTRACT_FILES={}",
            contract_files(image, &modules, &list.capabilities).join(" ")
        ),
        format!(
            "VERIFY_EXCEPTIONS={}",
            unique_pairs(&modules, |m| m
                .verify_exceptions
                .iter()
                .map(|e| (e.class.clone(), e.unit.clone()))
                .collect())
            .iter()
            .map(|(class, unit)| format!("{class}|{unit}"))
            .collect::<Vec<_>>()
            .join(" ")
        ),
        format!("MODULE_PRESETS={}", presets.join(" ")),
        format!(
            "LUKS_INITRAMFS={}",
            provides(image, &entries, crate::emit::recipe::LUKS_INITRAMFS)
        ),
        format!(
            "FAMILY={}",
            image
                .base
                .as_ref()
                .map(|b| b.family.as_str())
                .unwrap_or_default()
        ),
        format!("CONFORMS={conforms}"),
        format!("SCAP_CONTENT={scap_content}"),
        // Two vocabularies on purpose. A claim is written as a benchmark
        // number and the hook resolves it against the content it installs; a
        // refusal is written as a rule ID, because 710 of the 994 rules carry
        // no number that reaches them and a rule worth refusing is often one.
        format!("SCAP_CLAIMED={}", scap_claimed.join(" ")),
        format!("SCAP_REFUSED={}", scap_refused.join(" ")),
        format!("TARGET={target}"),
        format!(
            "MODULE_HASHES={}",
            modules
                .iter()
                .filter_map(|m| m.content.as_ref().map(|c| format!("{}|{c}", m.path)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        format!("ASSET_RESOLUTIONS={}", resolutions.join(" ")),
        format!("SOURCE_COMMIT={source_commit}"),
        format!("BUILD_BACKEND={backend}"),
        format!("AUDIT_ENFORCE={}", list.audit_enforce),
    ];
    if let Some(base) = &image.base {
        build_args.push(format!("BASE_DECLARED={}", base.image));
        build_args.push(format!("BASE={resolved_base}"));
    }
    if let Some(kernel) = &opts.kernel {
        build_args.push(format!("KERNEL={kernel}"));
    }

    let mut tags = opts.tags.clone();
    tags.extend(lines("TAGS"));
    if tags.is_empty() {
        tags.push(format!(
            "{}:{}",
            env("IMAGE_NAME").unwrap_or_else(|| published.clone()),
            env("DEFAULT_TAG").unwrap_or_else(|| "latest".to_string())
        ));
    }

    let secrets = secrets(opts)?;
    let (import, export) = cache(&list, &target, &published, opts, namespace.ok())?;

    install(root)?;

    eprintln!(
        "tect: {backend} target={target} version={version}{}",
        match &opts.kernel {
            Some(kernel) => format!(" kernel={kernel}"),
            None => String::new(),
        }
    );
    eprintln!("tect: tags {}", tags.join(" "));
    if !import.is_empty() {
        eprintln!("tect: importing cache from {}", import.join(" "));
    }
    if let Some(export) = &export {
        eprintln!("tect: exporting cache to {export}");
    }

    let containerfile = crate::emit::containerfile::path(image);
    let program = match backend.as_str() {
        "buildx" => "docker",
        _ => "podman",
    };
    let prefix = build_prefix(&backend, &containerfile, &build_args, &secrets);
    let mut labels: Vec<String> = lines("LABELS");
    if list.manifest_label {
        labels.push(format!("org.tectonic.manifest={}", record::MANIFEST));
    }

    // buildah has no registry layer cache and writes no OCI archive, and that
    // is about the backend rather than the pass, so both paths answer it
    // instead of dropping an answer on the way to one of them.
    if backend != "buildx" {
        if export.is_some() {
            return Err("buildah cannot export a BuildKit layer cache".into());
        }
        if opts.oci_output.is_some() {
            return Err("the buildah backend cannot write an OCI archive".into());
        }
        if !import.is_empty() {
            eprintln!("tect: buildah ignores the registry layer cache");
        }
    }

    // A sealed UKI carries the split image's storage digest, and a single build
    // has no storage to read it from. The generated tail provides the `split`
    // stage pass one stops at.
    if !image.boot.is_empty() {
        if backend == "buildx" {
            return Err(
                "`--backend buildx` cannot build a UKI target: the composefs digest is read \
                 from the local buildah store, and buildx has none"
                    .into(),
            );
        }
        build_uki(
            root,
            program,
            &containerfile,
            &build_args,
            &secrets,
            &target,
            &tags,
            &labels,
        )?;
        return Ok(Stopped::Built);
    }

    let mut args = prefix;
    for tag in &tags {
        args.extend(["--tag".to_string(), tag.clone()]);
    }
    for label in &labels {
        args.extend(["--label".to_string(), label.clone()]);
    }
    if backend == "buildx" {
        for reference in &import {
            args.extend([
                "--cache-from".to_string(),
                format!("type=registry,ref={reference}"),
            ]);
        }
        if let Some(export) = &export {
            args.extend([
                "--cache-to".to_string(),
                format!("type=registry,ref={export}"),
            ]);
        }
        args.push("--provenance=false".into());
        if let Some(path) = &opts.oci_output {
            args.extend(["--output".to_string(), format!("type=oci,dest={path}")]);
        }
    } else {
        args.push("--pull=newer".into());
    }
    args.push(".".into());

    Err(format!(
        "{program}: {}",
        Command::new(program).args(&args).current_dir(root).exec()
    ))
}

/// The argv every pass shares: the backend, the generated file, every build
/// argument and every secret.
fn build_prefix(
    backend: &str,
    containerfile: &Path,
    build_args: &[String],
    secrets: &[(String, String)],
) -> Vec<String> {
    let mut args = match backend {
        "buildx" => vec!["buildx".to_string(), "build".to_string()],
        _ => vec!["build".to_string()],
    };
    args.push("--file".to_string());
    args.push(containerfile.display().to_string());
    for arg in build_args {
        args.extend(["--build-arg".to_string(), arg.clone()]);
    }
    for (id, path) in secrets {
        args.extend(["--secret".to_string(), format!("id={id},src={path}")]);
    }
    args
}

/// One build's whole argv: what it starts from, the build arguments this pass
/// adds, the stage it stops at, its tags and labels, and the context last.
fn build_argv(
    prefix: &[String],
    extra: &[(&str, &str)],
    target: Option<&str>,
    tags: &[String],
    labels: &[String],
    pull: bool,
) -> Vec<String> {
    let mut args = prefix.to_vec();
    for (name, value) in extra {
        args.extend(["--build-arg".to_string(), format!("{name}={value}")]);
    }
    if let Some(target) = target {
        args.extend(["--target".to_string(), target.to_string()]);
    }
    for tag in tags {
        args.extend(["--tag".to_string(), tag.clone()]);
    }
    for label in labels {
        args.extend(["--label".to_string(), label.clone()]);
    }
    if pull {
        args.push("--pull=newer".to_string());
    }
    args.push(".".to_string());
    args
}

/// The scratch tag one pass writes and the next builds on. A target names a
/// flavour with a `/`, which no tag may hold, so anything but a reference
/// character becomes a `-`, and the process id keeps two concurrent builds of
/// one target from sealing each other's layers.
fn scratch_tag(kind: &str, target: &str) -> String {
    let tag: String = target
        .chars()
        .map(
            |c| match c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                true => c,
                false => '-',
            },
        )
        .collect();
    format!("localhost/tect-uki-{kind}:{tag}-{}", std::process::id())
}

/// A UKI target builds twice: the split image, the storage digest `bootc
/// install` will compare, and the image sealed with it. The generated tail's
/// `ARG COMPOSEFS_DIGEST` is what carries the digest into the seal. Nothing
/// reaches the user's tags until the sealed image's own digest, its embedded
/// cmdline and the skeleton's validation have all passed.
fn build_uki(
    root: &Path,
    program: &str,
    containerfile: &Path,
    build_args: &[String],
    secrets: &[(String, String)],
    target: &str,
    tags: &[String],
    labels: &[String],
) -> Result<(), String> {
    // The generated file is repository-relative and `buildah` resolves it
    // under the process's own directory, which need not be the repository.
    let root = fs::canonicalize(root).map_err(|err| format!("{}: {err}", root.display()))?;
    let containerfile = root.join(containerfile);
    let root = root.as_path();
    if tags.is_empty() {
        return Err("a UKI build has no tag to publish".into());
    }
    let text = fs::read_to_string(&containerfile).map_err(|err| {
        format!(
            "{}: {err}; run `tect generate` to write the boot chain's tail",
            containerfile.display()
        )
    })?;
    if !text.contains("${SPLIT_BASE}") || !text.contains("ARG COMPOSEFS_DIGEST") {
        return Err(format!(
            "{} carries no two-pass seal (`FROM ${{SPLIT_BASE}}` and `ARG COMPOSEFS_DIGEST`); \
             the module collection providing its boot chain predates this tool, so update it, \
             then run `tect generate`",
            containerfile.display()
        ));
    }
    let seal = seal_containerfile(&text).ok_or_else(|| {
        format!(
            "{} has no tail section to seal; run `tect generate` to write it",
            containerfile.display()
        )
    })?;
    // Per build: two builds of one repository must not seal each other's
    // generated file, and one must not delete the other's mid-pass.
    let seal_file = root
        .join(layout::OUT)
        .join(format!("uki-seal-{}.Containerfile", std::process::id()));
    if let Some(dir) = seal_file.parent() {
        fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    fs::write(&seal_file, seal).map_err(|err| format!("{}: {err}", seal_file.display()))?;

    let split_tag = scratch_tag("split", target);
    let sealed_tag = scratch_tag("sealed", target);
    let sealed = (|| -> Result<String, String> {
        let prefix = build_prefix("buildah", &containerfile, build_args, secrets);
        let split = build_argv(
            &prefix,
            &[],
            Some("split"),
            std::slice::from_ref(&split_tag),
            &[],
            true,
        );
        run_build(
            program,
            root,
            &split,
            &format!("building the split image {split_tag}"),
        )?;
        let store = graph_root()?;
        let digest = composefs_digest(&store, &split_tag)?;
        let prefix = build_prefix("buildah", &seal_file, build_args, secrets);
        let argv = build_argv(
            &prefix,
            &[
                ("SPLIT_BASE", split_tag.as_str()),
                ("COMPOSEFS_DIGEST", digest.as_str()),
            ],
            None,
            std::slice::from_ref(&sealed_tag),
            labels,
            true,
        );
        run_build(
            program,
            root,
            &argv,
            &format!("sealing with composefs digest {digest}"),
        )?;
        Ok(digest)
    })();
    let _ = fs::remove_file(&seal_file);
    let digest = match sealed {
        Ok(digest) => {
            remove_image(program, &split_tag);
            digest
        }
        Err(err) => {
            remove_image(program, &split_tag);
            remove_image(program, &sealed_tag);
            return Err(err);
        }
    };

    // The storage digest of what was published, not the one it was given: a
    // tail fragment or a rebuild after the read would move it, and the check
    // that only read the cmdline back could not see that.
    let checked = (|| -> Result<(), String> {
        let store = graph_root()?;
        let published = composefs_digest(&store, &sealed_tag)?;
        if published != digest {
            return Err(format!(
                "the sealed image digests {published}, not {digest} the UKI embeds; a step \
                 after the seal changed the image, so `bootc install` would refuse it"
            ));
        }
        let out = Command::new(program)
            .args(verify_argv(&sealed_tag))
            .output()
            .map_err(|err| format!("{program}: {err}"))?;
        let cmdline = String::from_utf8_lossy(&out.stdout);
        if !(out.status.success() && carries_digest(&cmdline, &digest)) {
            return Err(format!(
                "{sealed_tag} does not carry composefs={digest} on its UKI cmdline"
            ));
        }
        // The skeleton's validation step cannot run in the published stage: it
        // writes (the rpm database's shared-memory file, directory mtimes) and
        // moves the digest the UKI embedded. It runs here instead, against the
        // sealed image, and its writes land in a discarded container.
        validate_sealed(root, &sealed_tag, build_args)
    })();
    if let Err(err) = checked {
        remove_image(program, &sealed_tag);
        return Err(err);
    }

    for tag in tags {
        let status = match Command::new(program)
            .args(["tag", &sealed_tag, tag])
            .status()
        {
            Ok(status) => status,
            Err(err) => {
                remove_image(program, &sealed_tag);
                return Err(format!("{program}: {err}"));
            }
        };
        if !status.success() {
            remove_image(program, &sealed_tag);
            return Err(format!("tagging {tag}: {program} exited {status}"));
        }
    }
    remove_image(program, &sealed_tag);
    Ok(())
}

/// Everything up to and including the tail section's end: the split base, the
/// seal and the copy into `/boot`, with the skeleton's trailing steps left
/// out. A step that runs after the seal writes to the published image and
/// moves the digest the UKI embedded.
fn seal_containerfile(text: &str) -> Option<String> {
    let at = text.rfind(crate::emit::containerfile::TAIL_END)?;
    let end = at + crate::emit::containerfile::TAIL_END.len();
    Some(text[..end].to_string())
}

/// The skeleton's checks, run against the sealed image outside the build.
/// `validate_argv` is what the podman command is built from.
fn validate_sealed(root: &Path, tag: &str, build_args: &[String]) -> Result<(), String> {
    let argv = validate_argv(root, tag, build_args);
    let status = Command::new("podman")
        .args(&argv)
        .status()
        .map_err(|err| format!("podman: {err}"))?;
    match status.success() {
        true => Ok(()),
        false => Err(format!("validating {tag}: podman exited {status}")),
    }
}

/// `podman run` of the image's own validation, with the tool the build pins
/// and the environment the skeleton's step passes it.
fn validate_argv(root: &Path, tag: &str, build_args: &[String]) -> Vec<String> {
    let tool = root.join(layout::MOUNTED);
    let mut argv: Vec<String> = vec![
        "run".to_string(),
        "--rm".to_string(),
        "--security-opt".to_string(),
        "label=disable".to_string(),
        "--tmpfs".to_string(),
        "/tmp".to_string(),
        "--tmpfs".to_string(),
        "/run".to_string(),
        "-v".to_string(),
        format!("{}:/ctx/tect:ro", tool.display()),
        "--entrypoint".to_string(),
        "/ctx/tect".to_string(),
    ];
    for name in [
        "CONTRACT_FILES",
        "VERIFY_EXCEPTIONS",
        "MODULE_PRESETS",
        "LUKS_INITRAMFS",
        "FAMILY",
    ] {
        if let Some(value) = build_args
            .iter()
            .find_map(|arg| arg.strip_prefix(&format!("{name}=")))
        {
            argv.extend(["-e".to_string(), format!("{name}={value}")]);
        }
    }
    argv.push(tag.to_string());
    argv.push("validate-image".to_string());
    argv
}

/// Runs one pass to completion. The single-pass path execs instead, so only
/// the two passes report through here.
fn run_build(program: &str, root: &Path, argv: &[String], step: &str) -> Result<(), String> {
    eprintln!("tect: {step}");
    let status = Command::new(program)
        .args(argv)
        .current_dir(root)
        .status()
        .map_err(|err| format!("{program}: {err}"))?;
    match status.success() {
        true => Ok(()),
        false => Err(format!("{step}: {program} exited {status}")),
    }
}

/// The store the split image landed in, which the digest container mounts.
fn graph_root() -> Result<String, String> {
    let out = Command::new("podman")
        .args(["info", "--format", "{{.Store.GraphRoot}}"])
        .output()
        .map_err(|err| format!("podman info: {err}"))?;
    let store = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match out.status.success() && !store.is_empty() {
        true => Ok(store),
        false => Err("`podman info --format '{{.Store.GraphRoot}}'` named no store".into()),
    }
}

/// The digest command exactly as measured working rootless, so a change to it
/// is a change to what was proved rather than a drift.
fn digest_argv(store: &str, image: &str) -> Vec<String> {
    vec![
        "run".to_string(),
        "--rm".to_string(),
        "--privileged".to_string(),
        "--security-opt".to_string(),
        "label=disable".to_string(),
        "--tmpfs".to_string(),
        "/var/tmp:size=8g".to_string(),
        "-v".to_string(),
        format!("{store}:/var/lib/containers/storage"),
        image.to_string(),
        "bootc".to_string(),
        "container".to_string(),
        "compute-composefs-digest-from-storage".to_string(),
        image.to_string(),
    ]
}

/// The digest `bootc install` computes from the storage the split image landed
/// in, measured working rootless 2026-09-16.
fn composefs_digest(store: &str, image: &str) -> Result<String, String> {
    let argv = digest_argv(store, image);
    let out = Command::new("podman")
        .args(&argv)
        .output()
        .map_err(|err| format!("podman: {err}"))?;
    let digest = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match out.status.success() && digest.len() == 128 && digest.bytes().all(lower_hex) {
        true => Ok(digest),
        false => Err(format!(
            "`podman {}` did not print a composefs digest: `{digest}`",
            argv.join(" ")
        )),
    }
}

fn lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

/// Best effort: the split image is scratch, and pass two has everything it
/// needs out of it.
fn remove_image(program: &str, tag: &str) {
    let _ = Command::new(program).args(["rmi", "--force", tag]).output();
}

/// Whether the embedded cmdline carries the digest pass two sealed.
fn carries_digest(cmdline: &str, digest: &str) -> bool {
    cmdline.contains(&format!("composefs={digest}"))
}

/// The UKI cmdline read back out of the image the first tag names.
fn verify_argv(tag: &str) -> Vec<String> {
    vec![
        "run".to_string(),
        "--rm".to_string(),
        "--entrypoint".to_string(),
        "/bin/sh".to_string(),
        tag.to_string(),
        "-c".to_string(),
        "uki=$(find /boot/EFI/Linux -name \"*.efi\" -print -quit); \
         objcopy -O binary --only-section=.cmdline \"$uki\" /dev/stdout | tr -d \"\\0\""
            .to_string(),
    ]
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// A newline-separated environment value, as the metadata action emits tags and
/// labels.
fn lines(name: &str) -> Vec<String> {
    env(name)
        .iter()
        .flat_map(|value| value.lines())
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
}

fn target(list: &List, named: Option<&str>) -> Result<String, String> {
    match named {
        Some(name) => list.find_target(name).map(|target| target.to_string()),
        None => Ok(list
            .default_target()
            .ok_or("no default image to build; name a target with `--target`")?
            .to_string()),
    }
}

/// `--secret <id>=<path>`, and `MOK_KEY_PATH` for the one a local build is
/// likely to have.
fn secrets(opts: &Options) -> Result<Vec<(String, String)>, String> {
    let mut out: Vec<(String, String)> = Vec::new();
    for pair in &opts.secrets {
        match pair.split_once('=') {
            Some((id, path)) if !id.is_empty() && !path.is_empty() => {
                out.push((id.to_string(), path.to_string()))
            }
            _ => return Err(format!("`--secret` takes `<id>=<path>`, not `{pair}`")),
        }
    }
    if let Some(path) = env("MOK_KEY_PATH") {
        if out.iter().any(|(id, _)| id == "mok_privkey") {
            return Err("MOK_KEY_PATH and `--secret mok_privkey=` both set; use one".into());
        }
        out.push(("mok_privkey".to_string(), path));
    }
    for (id, path) in &out {
        if !Path::new(path).is_file() {
            return Err(format!(
                "secret `{id}` points at `{path}`, which is not there"
            ));
        }
    }
    Ok(out)
}

/// What the layer cache is imported from and exported to, which is the one
/// place a build reaches a registry the plan does not name.
fn cache(
    list: &List,
    target: &str,
    published: &str,
    opts: &Options,
    namespace: Option<String>,
) -> Result<(Vec<String>, Option<String>), String> {
    if opts.no_cache_from && !opts.cache_to {
        return Ok((Vec::new(), None));
    }
    let (Some(namespace), Some(image)) = (namespace, list.cache_image()) else {
        if opts.cache_to {
            return Err("`--cache-to` needs a registry namespace".into());
        }
        eprintln!("tect: skipping the registry layer cache");
        return Ok((Vec::new(), None));
    };
    let repo = format!("{namespace}/{image}");

    let mut import = Vec::new();
    if !opts.no_cache_from {
        import.push(format!("{repo}:{published}"));
        let this = list.targets().into_iter().find(|t| t.to_string() == target);
        for sibling in list.targets() {
            let same = this
                .as_ref()
                .is_some_and(|t| t.image == sibling.image && t.flavour != sibling.flavour);
            if same {
                import.push(format!("{repo}:{}", sibling.published()));
            }
        }
    }
    let export = opts
        .cache_to
        .then(|| format!("{repo}:{published},mode=max"));
    Ok((import, export))
}

/// The binary the `tect` stage copies, which is this one: a build runs the
/// release the repository is pinned to, not whatever is on the machine.
fn install(root: &Path) -> Result<(), String> {
    let from = std::env::current_exe().map_err(|err| format!("this binary: {err}"))?;
    let to = root.join(layout::MOUNTED);
    if from.canonicalize().ok() == to.canonicalize().ok() {
        return Ok(());
    }
    let dir = to.parent().unwrap_or(Path::new(layout::OUT));
    fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    fs::copy(&from, &to).map_err(|err| format!("{}: {err}", to.display()))?;
    fs::set_permissions(&to, fs::Permissions::from_mode(0o755))
        .map_err(|err| format!("{}: {err}", to.display()))
}

/// Today in UTC, as the version an image is stamped with when nothing names
/// one.
fn today() -> Result<String, String> {
    let out = Command::new("date")
        .args(["-u", "+%Y%m%d"])
        .output()
        .map_err(|err| format!("date: {err}"))?;
    let stamp = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match out.status.success() && stamp.len() == 8 {
        true => Ok(stamp),
        false => Err(format!("date -u +%Y%m%d said `{stamp}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A build's shared prefix, as one target's build derives it.
    fn prefix() -> Vec<String> {
        build_prefix(
            "buildah",
            Path::new("generated/next44/Containerfile"),
            &["FLAVOUR=".to_string(), "BASE=example:1".to_string()],
            &[("mok_privkey".to_string(), "/keys/mok.key".to_string())],
        )
    }

    /// The scratch tag a pass writes and the next builds on. A target names a
    /// flavour with a `/`, which no tag may hold, and the two passes and two
    /// concurrent builds must not share one.
    #[test]
    fn the_scratch_tags_are_references_and_apart() {
        assert_eq!(
            scratch_tag("split", "next44"),
            format!("localhost/tect-uki-split:next44-{}", std::process::id())
        );
        let flavoured = scratch_tag("split", "next44/dev");
        let (_, tag) = flavoured.rsplit_once(':').expect("a tag");
        assert!(!tag.is_empty() && !tag.contains('/'), "{flavoured}");
        assert_ne!(
            scratch_tag("split", "next44"),
            scratch_tag("sealed", "next44")
        );
    }

    /// Pass one stops at the split stage and names it with the temporary tag,
    /// so no user tag can point at an image pass two has not sealed.
    #[test]
    fn pass_one_builds_the_split_stage_only() {
        let temp = scratch_tag("split", "next44");
        let args = build_argv(
            &prefix(),
            &[],
            Some("split"),
            std::slice::from_ref(&temp),
            &[],
            true,
        );
        assert!(args
            .windows(2)
            .any(|window| window[0] == "--target" && window[1] == "split"));
        assert!(args.contains(&temp));
        assert!(args.contains(&"--pull=newer".to_string()));
        assert!(!args.contains(&"example/next44:latest".to_string()));
    }

    /// The digest command is the measured one, unchanged.
    #[test]
    fn the_digest_command_is_the_measured_shape() {
        assert_eq!(
            digest_argv(
                "/var/lib/containers/storage",
                "localhost/tect-uki-split:next44"
            ),
            vec![
                "run",
                "--rm",
                "--privileged",
                "--security-opt",
                "label=disable",
                "--tmpfs",
                "/var/tmp:size=8g",
                "-v",
                "/var/lib/containers/storage:/var/lib/containers/storage",
                "localhost/tect-uki-split:next44",
                "bootc",
                "container",
                "compute-composefs-digest-from-storage",
                "localhost/tect-uki-split:next44",
            ]
        );
    }

    /// The seal pass stops at the tail: the skeleton's trailing steps would
    /// run in the published image and move the digest the UKI embedded.
    #[test]
    fn the_seal_file_stops_at_the_tail() {
        let text = format!(
            "ARG BASE\nFROM scratch AS rootfs\nRUN echo build\n{}\nARG FAMILY\nRUN /ctx/tect validate-image\n",
            crate::emit::containerfile::TAIL_END
        );
        let seal = seal_containerfile(&text).expect("a tail section");
        assert!(seal.contains("RUN echo build"));
        assert!(seal.ends_with(crate::emit::containerfile::TAIL_END));
        assert!(!seal.contains("validate-image"));
    }

    /// Validation runs the sealed image with the tool the build pinned, and
    /// nothing but the tail runs in the published stage.
    #[test]
    fn validation_runs_the_sealed_image() {
        let root = Path::new("/repo");
        let args = vec![
            "FAMILY=fedora".to_string(),
            "CONTRACT_FILES=a b".to_string(),
        ];
        let argv = validate_argv(root, "example:latest", &args);
        assert!(argv
            .windows(2)
            .any(|w| w[0] == "--entrypoint" && w[1] == "/ctx/tect"));
        assert!(argv
            .windows(2)
            .any(|w| w[0] == "-e" && w[1] == "FAMILY=fedora"));
        assert!(argv
            .windows(2)
            .any(|w| w[0] == "-e" && w[1] == "CONTRACT_FILES=a b"));
        assert_eq!(argv[argv.len() - 2], "example:latest");
        assert_eq!(argv[argv.len() - 1], "validate-image");
        assert!(argv.iter().any(|arg| arg == "/repo/out/tect:/ctx/tect:ro"));
    }

    /// Pass two carries the split image and the digest beyond the ordinary
    /// build and stops at a scratch tag: the user's tags are applied only
    /// after the sealed digest, the cmdline and the validation have passed, so
    /// a failed check leaves nothing published.
    #[test]
    fn pass_two_carries_the_split_base_and_the_digest() {
        let split = scratch_tag("split", "next44");
        let sealed = scratch_tag("sealed", "next44");
        let digest = "a1".repeat(64);
        let labels = vec!["org.tectonic.manifest=/usr/share/tectonic/manifest.json".to_string()];
        let args = build_argv(
            &prefix(),
            &[
                ("SPLIT_BASE", split.as_str()),
                ("COMPOSEFS_DIGEST", digest.as_str()),
            ],
            None,
            std::slice::from_ref(&sealed),
            &labels,
            true,
        );
        assert!(args.contains(&format!("SPLIT_BASE={split}")));
        assert!(args.contains(&format!("COMPOSEFS_DIGEST={digest}")));
        assert!(args.contains(&sealed));
        assert!(!args.contains(&"example/next44:latest".to_string()));
        assert!(!args.iter().any(|arg| arg == "--target"));
        assert!(carries_digest(
            &format!("rw composefs={digest} quiet"),
            &digest
        ));
        assert!(!carries_digest("rw composefs=deadbeef quiet", &digest));
    }
}

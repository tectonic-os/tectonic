//! The whole argv a container build runs with, derived from the plan, and then
//! the backend that runs it.

use crate::emit::plan::{contract_files, of_target, unique_pairs};
use crate::model::image::{List, Target};
use crate::model::module::Module;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::Command;

/// Where the `tect` build stage copies the binary from, so every layer mounts
/// the release the repository is pinned to.
const MOUNTED: &str = "out/tect";

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

/// Why a build came back rather than becoming the backend.
pub enum Stopped {
    /// The repository is wrong, and every problem was printed.
    Repository,
}

/// Fetches, verifies, then replaces this process with the backend. Returns only
/// when the repository is wrong, having reported why.
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

    for line in crate::fetch::modules(root, &list)? {
        eprintln!("tect: {line}");
    }
    // Never regenerated here: a build proves the committed files are current.
    // What it read is what the rest of this builds from, so the build cannot
    // act on a second reading of the same files.
    let gate = crate::run(crate::Command::Verify, None, root);
    if gate.issues.report(&gate.context) {
        return Ok(Stopped::Repository);
    }
    eprintln!(
        "tect: {} generated files match the manifests",
        gate.files.len()
    );

    let list = gate.list;
    let (image, flavour, entries) =
        of_target(&list, &target).ok_or_else(|| format!("`{target}` has no image"))?;
    let modules: Vec<&Module> = entries.iter().filter_map(|e| e.module.as_ref()).collect();
    let published = published(&list, &target);

    let version = match env("IMAGE_VERSION") {
        Some(named) => named,
        None => today()?,
    };
    let namespace = crate::registry::namespace(root);
    let mut build_args = vec![
        format!("FLAVOUR={}", flavour.unwrap_or_default()),
        format!("IMAGE_VERSION={version}"),
        format!("IMAGE_REGISTRY={}", namespace.clone().unwrap_or_default()),
        format!(
            "CONTRACT_FILES={}",
            contract_files(image, &modules).join(" ")
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
    ];
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
    let mut args: Vec<String> = match backend.as_str() {
        "buildx" => vec!["buildx".into(), "build".into()],
        _ => vec!["build".into()],
    };
    args.push("--file".into());
    args.push(containerfile.display().to_string());
    for arg in &build_args {
        args.extend(["--build-arg".to_string(), arg.clone()]);
    }
    for tag in &tags {
        args.extend(["--tag".to_string(), tag.clone()]);
    }
    for label in lines("LABELS") {
        args.extend(["--label".to_string(), label]);
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
    } else {
        if !import.is_empty() {
            eprintln!("tect: buildah ignores the registry layer cache");
        }
        if export.is_some() {
            return Err("buildah cannot export a BuildKit layer cache".into());
        }
    }
    for (id, path) in &secrets {
        args.extend(["--secret".to_string(), format!("id={id},src={path}")]);
    }
    match (backend.as_str(), &opts.oci_output) {
        ("buildx", output) => {
            args.push("--provenance=false".into());
            if let Some(path) = output {
                args.extend(["--output".to_string(), format!("type=oci,dest={path}")]);
            }
        }
        (_, Some(_)) => return Err("the buildah backend cannot write an OCI archive".into()),
        (_, None) => args.push("--pull=newer".into()),
    }
    args.push(".".into());

    let program = match backend.as_str() {
        "buildx" => "docker",
        _ => "podman",
    };
    Err(format!(
        "{program}: {}",
        Command::new(program).args(&args).current_dir(root).exec()
    ))
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
    let known: Vec<String> = list.targets().iter().map(Target::to_string).collect();
    let target = match named {
        Some(name) => name.to_string(),
        None => list
            .default_target()
            .ok_or("no default image to build; name a target with `--target`")?
            .to_string(),
    };
    match known.contains(&target) {
        true => Ok(target),
        false => Err(format!(
            "`{target}` is not a build target (have: {})",
            known.join(" ")
        )),
    }
}

fn published(list: &List, target: &str) -> String {
    list.targets()
        .iter()
        .find(|have| have.to_string() == target)
        .map(Target::published)
        .unwrap_or_default()
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
    let to = root.join(MOUNTED);
    if from.canonicalize().ok() == to.canonicalize().ok() {
        return Ok(());
    }
    let dir = to.parent().unwrap_or(Path::new("out"));
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

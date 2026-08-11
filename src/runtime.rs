//! What the tool does inside a build layer, where it is mounted as /ctx/tect.

use std::fmt::Write as _;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Whether one line of `systemd-analyze verify` output is of a class.
type Matches = fn(&str) -> bool;

/// The diagnostic classes `allow-verify` may name, and what each one matches in
/// `systemd-analyze verify` output.
pub const VERIFY_CLASSES: [(&str, Matches); 2] = [
    ("mount-not-found", mount_not_found),
    ("man-page-missing", man_page_missing),
];

/// `Failed to create ...: Unit <name>.mount not found.`
fn mount_not_found(line: &str) -> bool {
    line.contains("Failed to create ")
        && line.contains(": Unit ")
        && (line.ends_with(".mount not found.") || line.ends_with(".swap not found."))
}

/// `Command 'man <page>' failed with code <n>`
fn man_page_missing(line: &str) -> bool {
    let Some((_, rest)) = line.split_once("Command 'man ") else {
        return false;
    };
    let Some((page, code)) = rest.split_once("' failed with code ") else {
        return false;
    };
    !page.contains('\'') && !code.is_empty() && code.chars().all(|c| c.is_ascii_digit())
}

pub fn class_names() -> String {
    VERIFY_CLASSES
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

fn classify(line: &str) -> Option<&'static str> {
    VERIFY_CLASSES
        .iter()
        .find(|(_, matches)| matches(line))
        .map(|(name, _)| *name)
}

// ---- os-release ----------------------------------------------------------

const OS_RELEASE: &str = "/usr/lib/os-release";

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Writes the image's declared identity into os-release, from the ARGs the
/// generated Containerfile passes down.
pub fn os_release() -> Result<(), String> {
    let name = env("IMAGE_NAME").ok_or("IMAGE_NAME is unset: the image declares no name")?;
    let version = env("IMAGE_VERSION").unwrap_or_else(|| "dev".to_string());
    let pretty = env("IMAGE_PRETTY_NAME").unwrap_or_else(|| format!("{name} {version}"));
    let hostname = env("IMAGE_ID").unwrap_or_else(|| name.to_lowercase());

    let mut set: Vec<(&str, String)> = vec![
        ("NAME", name),
        ("PRETTY_NAME", pretty),
        ("DEFAULT_HOSTNAME", hostname),
        ("IMAGE_VERSION", version),
    ];
    if let Some(url) = env("IMAGE_URL") {
        set.push(("HOME_URL", url.clone()));
        set.push(("DOCUMENTATION_URL", url));
    }
    if let Some(url) = env("IMAGE_ISSUES_URL") {
        set.push(("SUPPORT_URL", url.clone()));
        set.push(("BUG_REPORT_URL", url));
    }

    let text = fs::read_to_string(OS_RELEASE).map_err(|err| format!("{OS_RELEASE}: {err}"))?;
    fs::write(OS_RELEASE, assign(&text, &set)).map_err(|err| format!("{OS_RELEASE}: {err}"))?;

    let link = Path::new("/etc/os-release");
    let _ = fs::remove_file(link);
    std::os::unix::fs::symlink("../usr/lib/os-release", link)
        .map_err(|err| format!("{}: {err}", link.display()))
}

/// Each key replaced where it already is, appended where it is not.
fn assign(text: &str, set: &[(&str, String)]) -> String {
    let mut out = String::new();
    for line in text.lines() {
        match line
            .split_once('=')
            .and_then(|(key, _)| set.iter().find(|(name, _)| *name == key))
        {
            Some((name, value)) => {
                let _ = writeln!(out, "{name}=\"{value}\"");
            }
            None => {
                let _ = writeln!(out, "{line}");
            }
        }
    }
    for (name, value) in set {
        if !text
            .lines()
            .any(|line| line.split_once('=').is_some_and(|(key, _)| key == *name))
        {
            let _ = writeln!(out, "{name}=\"{value}\"");
        }
    }
    out
}

// ---- fetch ---------------------------------------------------------------

const FETCH_USAGE: &str = "\
usage: tect fetch <what> <url> <sha256> [target] [extra...]

  file    <url> <sha256> <path>          verify and keep the download
  tree    <url> <sha256> <dir> [args..]  unpack it, extra args reaching tar
  bin     <url> <sha256> <name> [inner]  install one executable to /usr/bin
  rpm     <url> <sha256>                 install the package";

pub fn fetch(args: &[&str]) -> Result<(), String> {
    let (what, url, sha256, rest) = match args {
        [what, url, sha256, rest @ ..] => (*what, *url, *sha256, rest),
        _ => return Err(FETCH_USAGE.to_string()),
    };

    match (what, rest) {
        ("file", [path]) => verified(url, sha256, Path::new(path)),
        ("tree", [dir, extra @ ..]) => extract(url, sha256, Path::new(dir), extra),
        ("bin", [name]) => install_bin(url, sha256, name, name),
        ("bin", [name, inner]) => install_bin(url, sha256, name, inner),
        ("rpm", []) => {
            let rpm = scratch(url);
            verified(url, sha256, &rpm)?;
            let status = run("dnf5", &["install", "-y", &rpm.to_string_lossy()])?;
            let _ = fs::remove_file(&rpm);
            status
        }
        _ => Err(FETCH_USAGE.to_string()),
    }
}

/// A working path in /tmp, named for what is being downloaded.
fn scratch(url: &str) -> PathBuf {
    let name = url.rsplit('/').next().unwrap_or("download");
    PathBuf::from(format!("/tmp/fetch.{}.{name}", std::process::id()))
}

fn run(program: &str, args: &[&str]) -> Result<Result<(), String>, String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|err| format!("{program}: {err}"))?;
    Ok(if status.success() {
        Ok(())
    } else {
        Err(format!("{program} {}: {status}", args.join(" ")))
    })
}

/// Downloads `url` and refuses it unless it hashes to `sha256`.
fn verified(url: &str, sha256: &str, dest: &Path) -> Result<(), String> {
    if let Some(dir) = dest.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    run(
        "curl",
        &["--retry", "3", "-fsSLo", &dest.to_string_lossy(), url],
    )??;

    let got = sha256_file(dest)?;
    if got != sha256.to_lowercase() {
        let _ = fs::remove_file(dest);
        return Err(format!(
            "{url}\n  expected sha256 {sha256}\n  got      sha256 {got}"
        ));
    }
    Ok(())
}

pub(crate) fn extract(url: &str, sha256: &str, dir: &Path, extra: &[&str]) -> Result<(), String> {
    let archive = scratch(url);
    verified(url, sha256, &archive)?;
    fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;

    let path = archive.to_string_lossy().into_owned();
    let target = dir.to_string_lossy().into_owned();
    let (program, mut args) = if url.ends_with(".zip") || url.ends_with(".plasmoid") {
        ("unzip", vec!["-q", &path, "-d", &target])
    } else if url.ends_with(".tar.zst") {
        (
            "tar",
            vec!["--use-compress-program=zstd", "-xf", &path, "-C", &target],
        )
    } else {
        ("tar", vec!["-xf", &path, "-C", &target])
    };
    args.extend_from_slice(extra);

    let status = run(program, &args)?;
    let _ = fs::remove_file(&archive);
    status
}

fn install_bin(url: &str, sha256: &str, name: &str, inner: &str) -> Result<(), String> {
    let dest = PathBuf::from("/usr/bin").join(name);
    let tmp = PathBuf::from(format!("/tmp/fetch-bin.{}.{name}", std::process::id()));

    if [".zip", ".tar.gz", ".tgz", ".tar.xz", ".tar.zst"]
        .iter()
        .any(|ext| url.ends_with(ext))
    {
        extract(url, sha256, &tmp, &[])?;
        install(&tmp.join(inner), &dest)?;
        let _ = fs::remove_dir_all(&tmp);
    } else {
        verified(url, sha256, &tmp)?;
        install(&tmp, &dest)?;
        let _ = fs::remove_file(&tmp);
    }
    Ok(())
}

fn install(from: &Path, to: &Path) -> Result<(), String> {
    fs::copy(from, to).map_err(|err| format!("{} to {}: {err}", from.display(), to.display()))?;
    fs::set_permissions(to, fs::Permissions::from_mode(0o755))
        .map_err(|err| format!("{}: {err}", to.display()))
}

// ---- sha256 --------------------------------------------------------------

/// What `path` hashes to, by coreutils, which every build layer and every host
/// running this already has.
fn sha256_file(path: &Path) -> Result<String, String> {
    let out = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|err| format!("sha256sum: {err}"))?;
    if !out.status.success() {
        return Err(format!(
            "sha256sum {}: {}{}",
            path.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .filter(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_lowercase)
        .ok_or_else(|| format!("sha256sum {}: no hash in its output", path.display()))
}

// ---- validate-image ------------------------------------------------------

/// Presets a module ships, which is the only enablement this checks.
const MODULE_PRESET: &str = "45-module-";

struct Report {
    failures: usize,
}

impl Report {
    fn fail(&mut self, message: impl AsRef<str>) {
        eprintln!("FAIL: {}", message.as_ref());
        self.failures += 1;
    }
}

/// Combined output of a command, and whether it succeeded.
fn output(program: &str, args: &[&str]) -> Result<(bool, String), String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("{program}: {err}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), text))
}

/// Every check a built image has to pass before it is published.
pub fn validate_image() -> Result<(), String> {
    let mut report = Report { failures: 0 };

    println!("==> bootc install print-configuration");
    match output("bootc", &["install", "print-configuration"]) {
        Ok((true, _)) => println!("    ok"),
        _ => report.fail("bootc install print-configuration failed to parse"),
    }

    println!("==> initramfs");
    let package = fs::read_to_string("/usr/lib/kernel-build/kernel-package")
        .map(|text| text.trim().to_string())
        .unwrap_or_else(|_| "kernel-core".to_string());
    match output(
        "rpm",
        &["-q", "--qf", "%{VERSION}-%{RELEASE}.%{ARCH}", &package],
    ) {
        Ok((true, version)) if !version.trim().is_empty() => {
            let initramfs = format!("/usr/lib/modules/{}/initramfs.img", version.trim());
            if Path::new(&initramfs).is_file() {
                println!("    {initramfs} present");
            } else {
                report.fail(format!("initramfs missing at {initramfs}"));
            }
        }
        _ => report.fail(format!(
            "cannot determine kernel version from package {package}"
        )),
    }

    println!("==> /usr/lib/opt symlinks");
    let tmpfiles = "/usr/lib/tmpfiles.d/zz-opt-symlinks.conf";
    match fs::read_to_string(tmpfiles) {
        Ok(text) => {
            for line in text.lines() {
                let mut fields = line.split_whitespace();
                let (Some(kind), Some(path)) = (fields.next(), fields.next()) else {
                    continue;
                };
                if kind != "L+" && kind != "L" {
                    continue;
                }
                let target = fields
                    .skip(4)
                    .collect::<Vec<_>>()
                    .join(" ")
                    .replace("\\x20", " ");
                if Path::new(&target).exists() {
                    println!("    {path} -> {target} ok");
                } else {
                    report.fail(format!("{path} -> {target}: target does not exist"));
                }
            }
        }
        Err(_) => println!("    (no /usr/lib/opt symlinks declared)"),
    }

    println!("==> contract files");
    let contracts = env("CONTRACT_FILES").unwrap_or_default();
    if contracts.trim().is_empty() {
        println!("    (none declared)");
    } else {
        for path in contracts.split_whitespace() {
            if Path::new(path).exists() {
                println!("    {path} ok");
            } else {
                report.fail(format!(
                    "{path}: the manifest declares it, the image does not have it"
                ));
            }
        }
    }

    let mut exceptions: Vec<(String, String)> = Vec::new();
    for token in env("VERIFY_EXCEPTIONS")
        .unwrap_or_default()
        .split_whitespace()
    {
        let (class, unit) = token.split_once('|').unwrap_or((token, ""));
        if VERIFY_CLASSES.iter().any(|(name, _)| *name == class) {
            exceptions.push((class.to_string(), unit.to_string()));
        } else {
            report.fail(format!(
                "allow-verify \"{class}\": not a diagnostic class this image knows; known: {}",
                class_names()
            ));
        }
    }

    println!("==> systemd unit verification");
    let mut checked = 0usize;
    for scope in ["system", "user"] {
        let unit_dirs = [
            PathBuf::from(format!("/usr/lib/systemd/{scope}")),
            PathBuf::from(format!("/etc/systemd/{scope}")),
        ];
        for preset in presets(scope) {
            println!("    {}", preset.display());
            let Ok(text) = fs::read_to_string(&preset) else {
                continue;
            };
            for line in text.lines() {
                let mut fields = line.split_whitespace();
                let (Some(verb), Some(unit)) = (fields.next(), fields.next()) else {
                    continue;
                };
                if verb != "enable" && verb != "disable" {
                    continue;
                }
                checked += 1;

                if !unit_dirs.iter().any(|dir| find_unit(dir, unit)) {
                    if verb == "enable" {
                        report.fail(format!(
                            "{unit}: unit file not found in {} {}",
                            unit_dirs[0].display(),
                            unit_dirs[1].display()
                        ));
                    } else {
                        println!("        {unit} (not present, {verb}d)");
                    }
                    continue;
                }

                let config_root = PathBuf::from(format!("/etc/systemd/{scope}"));
                let links = enablement_links(&config_root, unit);
                if verb == "enable" && links.is_empty() {
                    report.fail(format!(
                        "{unit}: preset enables it, but nothing under {} does",
                        config_root.display()
                    ));
                } else if verb == "disable" && !links.is_empty() {
                    report.fail(format!(
                        "{unit}: preset disables it, but {} still enables it: {}",
                        config_root.display(),
                        links.join(" ")
                    ));
                }

                if scope != "system" || verb != "enable" {
                    println!("        {unit} {verb}d");
                    continue;
                }

                let verified = output("systemd-analyze", &["verify", "--no-pager", unit]);
                let (ok, text) = match verified {
                    Ok(result) => result,
                    Err(message) => {
                        report.fail(message);
                        continue;
                    }
                };
                if ok {
                    println!("        {unit} enabled");
                    continue;
                }
                if text.trim().is_empty() {
                    report.fail(format!(
                        "{unit}: systemd-analyze verify failed without saying why"
                    ));
                    continue;
                }

                let allowed: Vec<&str> = exceptions
                    .iter()
                    .filter(|(_, for_unit)| for_unit == unit)
                    .map(|(class, _)| class.as_str())
                    .collect();
                let unexpected: Vec<&str> = text
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .filter(|line| !allowed.iter().any(|class| classify(line) == Some(*class)))
                    .collect();

                if unexpected.is_empty() {
                    println!("        {unit} enabled (verify: declared exceptions only)");
                    continue;
                }
                report.fail(format!("{unit}: systemd-analyze verify"));
                for line in unexpected {
                    eprintln!("          {line}");
                    if let Some(class) = classify(line) {
                        eprintln!(
                            "            this is the known class '{class}'. If it is expected here,"
                        );
                        eprintln!(
                            "            declare it in the module shipping {}:",
                            preset.file_name().unwrap_or_default().to_string_lossy()
                        );
                        eprintln!("              allow-verify \"{class}\" unit=\"{unit}\"");
                    }
                }
            }
        }
    }

    if checked == 0 {
        report.fail("no module preset files found");
    }

    println!();
    if report.failures == 0 {
        println!("All validation checks passed.");
        Ok(())
    } else {
        Err(format!("{} validation check(s) failed.", report.failures))
    }
}

fn presets(scope: &str) -> Vec<PathBuf> {
    let dir = PathBuf::from(format!("/usr/lib/systemd/{scope}-preset"));
    let mut found: Vec<PathBuf> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            path.is_file() && name.starts_with(MODULE_PRESET) && name.ends_with(".preset")
        })
        .collect();
    found.sort();
    found
}

/// Whether `dir` holds a file named `unit`, at any depth.
fn find_unit(dir: &Path, unit: &str) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            if find_unit(&path, unit) {
                return true;
            }
        } else if path.file_name().is_some_and(|name| name == unit) && path.is_file() {
            return true;
        }
    }
    false
}

/// The symlinks at most two levels under `root` that enable `unit`, masks
/// excluded.
fn enablement_links(root: &Path, unit: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut dirs = vec![(root.to_path_buf(), 1u32)];
    while let Some((dir, depth)) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                if depth < 2 {
                    dirs.push((path, depth + 1));
                }
                continue;
            }
            if !kind.is_symlink() {
                continue;
            }
            let Ok(target) = fs::read_link(&path) else {
                continue;
            };
            if target == Path::new("/dev/null") {
                continue;
            }
            let named = |p: &Path| p.file_name().is_some_and(|name| name == unit);
            if named(&path) || named(&target) {
                links.push(path.display().to_string());
            }
        }
    }
    links.sort();
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hash that comes back wrong, or does not come back at all, is what a
    /// pinned archive is refused on.
    #[test]
    fn a_file_hashes_to_what_the_vector_says_or_fails() {
        let path = std::env::temp_dir().join(format!("tect-sha256.{}", std::process::id()));
        fs::write(&path, "abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        fs::remove_file(&path).unwrap();
        assert!(sha256_file(&path).is_err());
    }

    #[test]
    fn os_release_keys_are_replaced_in_place_or_appended() {
        let set = [
            ("NAME", "Example".to_string()),
            ("IMAGE_VERSION", "20260809".to_string()),
        ];
        assert_eq!(
            assign("NAME=Fedora\nVERSION_ID=44\nID=fedora\n", &set),
            "NAME=\"Example\"\nVERSION_ID=44\nID=fedora\nIMAGE_VERSION=\"20260809\"\n"
        );
        assert_eq!(
            assign("IMAGE_VERSION=\"old\"\nNAME=Fedora\n", &set),
            "IMAGE_VERSION=\"20260809\"\nNAME=\"Example\"\n"
        );
    }

    #[test]
    fn verify_classes_match_what_they_name() {
        assert_eq!(
            classify("Failed to create var-home.mount: Unit var-home.mount not found."),
            Some("mount-not-found")
        );
        assert_eq!(
            classify("systemd-analyze[1]: Command 'man foo.service(8)' failed with code 16"),
            Some("man-page-missing")
        );
        assert_eq!(classify("Unit is bad in some other way"), None);
    }
}

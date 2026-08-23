//! Replacing this release with the published one. The binary and the assets
//! move together or not at all: `init::assets` falls through to whatever stale
//! copy the host already has, so a binary that arrives alone scaffolds from it
//! with no diagnostic. The shell statement of the same thing is `install.sh`,
//! and the two agree by reading `init::data_home` rather than by copying it.

use crate::init;
use std::cmp::Ordering;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const REPO: &str = "tectonic-os/tectonic";

/// Where the two halves come from and where they go, decided before anything
/// is fetched.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub version: String,
    pub url: String,
    /// The binary itself, so its parent is the directory on the path.
    pub bin: PathBuf,
    pub assets: PathBuf,
}

/// The pair euid chooses, and the download that fills it. There is no fallback
/// between the pairs: an unwritable destination names the other rather than
/// guessing, which is the guess this command exists not to make.
fn plan(
    version: &str,
    euid: u32,
    home: Option<PathBuf>,
    data: Option<PathBuf>,
) -> Result<Plan, String> {
    let unset = "HOME is not set, so there is no per-user pair to move to";
    let (bindir, assets) = match euid {
        0 => (
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/local/share/tectonic/assets"),
        ),
        _ => (
            home.ok_or(unset)?.join(".local/bin"),
            data.ok_or(unset)?.join("tectonic/assets"),
        ),
    };
    Ok(Plan {
        version: version.to_string(),
        url: format!(
            "https://github.com/{REPO}/releases/download/v{version}/\
             tect-v{version}-x86_64-linux-musl.tar.gz"
        ),
        bin: bindir.join("tect"),
        assets,
    })
}

/// What a refusal names instead of falling back.
fn instead(euid: u32) -> &'static str {
    match euid {
        0 => "the per-user pair is ~/.local/bin with $XDG_DATA_HOME/tectonic/assets",
        _ => "run as root to move /usr/local/bin and /usr/local/share/tectonic/assets",
    }
}

/// The tag out of the URL `releases/latest` lands on. The whole input is the
/// guard: a split that found nothing answers nothing, rather than handing a
/// failed redirect on as a version.
fn version_of(url: &str) -> Result<String, String> {
    let url = url.trim();
    let version = match url.rsplit_once("/tag/v") {
        Some((_, version)) => version,
        None => return Err(format!("cannot read a version out of {url}")),
    };
    let usable = !version.is_empty()
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    match usable {
        true => Ok(version.to_string()),
        false => Err(format!("cannot read a version out of {url}")),
    }
}

/// A version as numbers, so `0.3.10` is ahead of `0.3.9` where string order
/// has it behind. A part that is not a number drops out, which is what makes a
/// pre-release compare as the version it is a pre-release of.
fn parts(version: &str) -> Vec<u64> {
    version
        .split(['.', '-'])
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// curl's stdout, or what it said instead. Every network call here shells out;
/// the crate has no HTTP client and needs none.
fn curl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("curl")
        .args(args)
        .output()
        .map_err(|err| format!("curl: {err}"))?;
    match out.status.success() {
        true => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        false => Err(format!(
            "curl {}: {}{}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )),
    }
}

/// The release `releases/latest` redirects to. Reading the tag out of the
/// redirect costs no rate-limited API call and drags in no `jq`.
fn latest() -> Result<String, String> {
    let url = format!("https://github.com/{REPO}/releases/latest");
    let landed = curl(&[
        "-fsSL",
        "--retry",
        "3",
        "-I",
        "-o",
        "/dev/null",
        "-w",
        "%{url_effective}",
        &url,
    ])?;
    version_of(&landed)
}

/// The digest published beside the tarball, which holds the bare hex and a
/// newline and nothing else.
fn digest(url: &str) -> Result<String, String> {
    let hex = curl(&["-fsSL", "--retry", "3", url])?.trim().to_lowercase();
    match hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        true => Ok(hex),
        false => Err(format!("{url} does not hold a sha256")),
    }
}

/// The staging path, removed however this ends, which is the trap `install.sh`
/// sets. It stands beside its destination so placing the download is a rename
/// and not a copy begun after the old copy is already gone.
struct Staged(PathBuf);

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
        let _ = fs::remove_file(&self.0);
    }
}

pub fn run() -> Result<(), String> {
    let arch = std::env::consts::ARCH;
    if arch != "x86_64" {
        return Err(format!(
            "only x86_64 Linux is published, and this is {arch}"
        ));
    }

    let running = crate::model::image::TECT_VERSION;
    let here = std::env::current_exe().map_err(|err| format!("this binary: {err}"))?;
    println!("tect: running {running}, at {}", here.display());

    let latest = latest()?;
    println!("tect: the latest release is {latest}");
    match parts(running).cmp(&parts(&latest)) {
        Ordering::Equal => {
            println!("tect: already current, so nothing moves");
            return Ok(());
        }
        Ordering::Greater => {
            println!("tect: this build is ahead of the latest release, so nothing moves");
            return Ok(());
        }
        Ordering::Less => {}
    }

    let euid = unsafe { libc::geteuid() };
    let plan = plan(
        &latest,
        euid,
        std::env::var_os("HOME").map(PathBuf::from),
        init::data_home(),
    )?;
    let bindir = plan
        .bin
        .parent()
        .ok_or_else(|| format!("{} has no directory", plan.bin.display()))?;
    let parent = plan
        .assets
        .parent()
        .ok_or_else(|| format!("{} has no directory", plan.assets.display()))?;

    // Second in init::assets()'s order, ahead of the pair this places, so it
    // would shadow what just arrived with no diagnostic.
    let beside = bindir.join("assets");
    if beside.exists() {
        return Err(format!(
            "{} outranks {}; remove it first",
            beside.display(),
            plan.assets.display()
        ));
    }
    if let Some(set) = std::env::var_os("TECT_ASSETS") {
        eprintln!(
            "tect: warning: TECT_ASSETS={} outranks {}",
            Path::new(&set).display(),
            plan.assets.display()
        );
    }
    if !plan.assets.ends_with("tectonic/assets") {
        return Err(format!("refusing to remove {}", plan.assets.display()));
    }

    for dir in [bindir, parent] {
        fs::create_dir_all(dir)
            .map_err(|err| format!("{}: {err}\n\n{}", dir.display(), instead(euid)))?;
    }

    let work = Staged(parent.join(format!(".upgrade.{}", std::process::id())));
    fs::create_dir(&work.0)
        .map_err(|err| format!("{}: {err}\n\n{}", work.0.display(), instead(euid)))?;
    let sha256 = digest(&format!("{}.sha256", plan.url))?;
    crate::runtime::extract(&plan.url, Some(&sha256), &work.0, &[])?;

    let (came, assets) = (work.0.join("tect"), work.0.join("assets"));
    if !came.is_file() {
        return Err(format!("{} holds no tect", plan.url));
    }
    if !assets.is_dir() {
        return Err(format!("{} holds no assets directory", plan.url));
    }

    // Everything that can fail is done before anything is replaced: the
    // download is verified, and the new binary is already beside the old one.
    let ready = Staged(bindir.join(format!(".tect.{}", std::process::id())));
    fs::copy(&came, &ready.0)
        .map_err(|err| format!("{}: {err}\n\n{}", ready.0.display(), instead(euid)))?;
    fs::set_permissions(&ready.0, fs::Permissions::from_mode(0o755))
        .map_err(|err| format!("{}: {err}", ready.0.display()))?;

    // Swapped rather than merged: an asset a release dropped must not survive
    // into every repository created afterwards.
    let _ = fs::remove_dir_all(&plan.assets);
    fs::rename(&assets, &plan.assets).map_err(|err| format!("{}: {err}", plan.assets.display()))?;
    // rename(2), never fs::copy: a running executable is ETXTBSY.
    fs::rename(&ready.0, &plan.bin).map_err(|err| format!("{}: {err}", plan.bin.display()))?;

    println!(
        "tect: moved tect {latest} from {running} to {}",
        plan.bin.display()
    );
    println!("tect: moved its assets to {}", plan.assets.display());
    if here != plan.bin {
        println!(
            "tect: note: {} is what you ran, and it is not what this replaced",
            here.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_the_tag_the_redirect_landed_on() {
        assert_eq!(
            version_of("https://github.com/tectonic-os/tectonic/releases/tag/v0.3.7").unwrap(),
            "0.3.7"
        );
        assert_eq!(version_of("  .../tag/v1.0.0-rc1\n").unwrap(), "1.0.0-rc1");
    }

    /// A redirect that did not land on a tag is refused rather than becoming a
    /// download URL built out of a whole URL.
    #[test]
    fn a_url_holding_no_tag_is_no_version() {
        for url in [
            "https://github.com/tectonic-os/tectonic/releases/latest",
            "https://github.com/login?return_to=/tectonic-os/tectonic",
            "https://github.com/tectonic-os/tectonic/releases/tag/v",
            "https://github.com/tectonic-os/tectonic/releases/tag/v0.3.7/../..",
            "",
        ] {
            assert!(version_of(url).is_err(), "{url}");
        }
    }

    #[test]
    fn root_and_a_person_take_different_pairs_and_never_each_others() {
        let root = plan("0.3.8", 0, None, None).unwrap();
        assert_eq!(root.bin, PathBuf::from("/usr/local/bin/tect"));
        assert_eq!(
            root.assets,
            PathBuf::from("/usr/local/share/tectonic/assets")
        );
        assert_eq!(
            root.url,
            "https://github.com/tectonic-os/tectonic/releases/download/v0.3.8/\
             tect-v0.3.8-x86_64-linux-musl.tar.gz"
        );

        // The assets half is the data home's, never a hardcoded ~/.local/share:
        // run() hands it init::data_home(), which is the line the two halves
        // agree on.
        let home = PathBuf::from("/home/someone");
        let user = plan(
            "0.3.8",
            1000,
            Some(home.clone()),
            Some(PathBuf::from("/elsewhere/data")),
        )
        .unwrap();
        assert_eq!(user.bin, home.join(".local/bin/tect"));
        assert_eq!(
            user.assets,
            PathBuf::from("/elsewhere/data/tectonic/assets")
        );
    }

    #[test]
    fn a_person_with_no_home_is_refused_rather_than_sent_to_usr_local() {
        assert!(plan("0.3.8", 1000, None, None).is_err());
        assert!(plan("0.3.8", 1000, Some(PathBuf::from("/home/x")), None).is_err());
    }

    #[test]
    fn versions_compare_as_numbers_rather_than_as_strings() {
        assert!(parts("0.3.10") > parts("0.3.9"));
        assert_eq!(parts("0.3.7"), parts("0.3.7"));
        assert!(parts("0.4.0") > parts("0.3.99"));
        // A pre-release compares as the version it precedes, which makes
        // `upgrade` a no-op on a build of it rather than a downgrade.
        assert_eq!(parts("0.4.0-rc1"), parts("0.4.0"));
    }
}

//! What the tool does inside a build layer, where it is mounted as /ctx/tect.

use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The diagnostic classes `allow-verify` may name, and what each one matches in
/// `systemd-analyze verify` output.
pub const VERIFY_CLASSES: [(&str, fn(&str) -> bool); 2] = [
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

fn matches_class(class: &str, line: &str) -> bool {
    VERIFY_CLASSES
        .iter()
        .any(|(name, matches)| *name == class && matches(line))
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
    let mut out = String::new();
    for line in text.lines() {
        let key = line.split_once('=').map(|(k, _)| k).unwrap_or("");
        match set.iter().find(|(name, _)| *name == key) {
            Some((name, value)) => {
                let _ = writeln!(out, "{name}=\"{value}\"");
            }
            None => {
                let _ = writeln!(out, "{line}");
            }
        }
    }
    for (name, value) in &set {
        if !text.lines().any(|l| l.starts_with(&format!("{name}="))) {
            let _ = writeln!(out, "{name}=\"{value}\"");
        }
    }
    fs::write(OS_RELEASE, out).map_err(|err| format!("{OS_RELEASE}: {err}"))?;

    let link = Path::new("/etc/os-release");
    let _ = fs::remove_file(link);
    std::os::unix::fs::symlink("../usr/lib/os-release", link)
        .map_err(|err| format!("{}: {err}", link.display()))
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
        return Err(format!("{url}\n  expected sha256 {sha256}\n  got      sha256 {got}"));
    }
    Ok(())
}

fn extract(url: &str, sha256: &str, dir: &Path, extra: &[&str]) -> Result<(), String> {
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

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    filled: usize,
    total: u64,
}

impl Sha256 {
    fn new() -> Self {
        Sha256 {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            filled: 0,
            total: 0,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.total += data.len() as u64;
        while !data.is_empty() {
            let take = (64 - self.filled).min(data.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled == 64 {
                compress(&mut self.state, &self.block);
                self.filled = 0;
            }
        }
    }

    fn hex(mut self) -> String {
        let bits = self.total * 8;
        self.update(&[0x80]);
        while self.filled != 56 {
            self.update(&[0]);
        }
        self.total = 0;
        self.update(&bits.to_be_bytes());
        self.state.iter().fold(String::new(), |mut out, word| {
            let _ = write!(out, "{word:08x}");
            out
        })
    }
}

fn compress(state: &mut [u32; 8], chunk: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (i, word) in chunk.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| format!("{}: {err}", path.display()))?;
        if read == 0 {
            return Ok(hash.hex());
        }
        hash.update(&buffer[..read]);
    }
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
                let target = fields.skip(4).collect::<Vec<_>>().join(" ").replace("\\x20", " ");
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
    for token in env("VERIFY_EXCEPTIONS").unwrap_or_default().split_whitespace() {
        let (class, unit) = token.split_once('|').unwrap_or((token, ""));
        if classify_known(class) {
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
                    .filter(|line| !allowed.iter().any(|class| matches_class(class, line)))
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
        Err(format!(
            "{} validation check(s) failed.",
            report.failures
        ))
    }
}

fn classify_known(class: &str) -> bool {
    VERIFY_CLASSES.iter().any(|(name, _)| *name == class)
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
            let Ok(kind) = entry.file_type() else { continue };
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

    #[test]
    fn sha256_matches_the_published_vectors() {
        for (input, want) in [
            (
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
        ] {
            let mut hash = Sha256::new();
            hash.update(input.as_bytes());
            assert_eq!(hash.hex(), want);
        }

        // A million 'a', which is what exercises the length encoding.
        let mut hash = Sha256::new();
        for _ in 0..1000 {
            hash.update(&[b'a'; 1000]);
        }
        assert_eq!(
            hash.hex(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
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

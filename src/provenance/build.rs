//! The build record: what was **resolved**, where `plan.json` is what was
//! **declared**.
//!
//! A moving reference with a recorded resolution is fully auditable and fully
//! fresh, which is the whole reason to be on bootc. There are exactly three of
//! them and they get one treatment: a base image tag resolves to a manifest
//! digest, a cloned asset's ref to a commit, and an unpinned collection's ref
//! to what the tarball actually hashed to.
//!
//! The resolution cannot go in `plan.json`: that file is committed and byte
//! compared, so a daily-changing value would fail `verify` every morning. This
//! document is written in-layer from ARGs instead, exactly the way `tect
//! os-release` writes `/usr/lib/os-release`, so nothing under `generated/` ever
//! holds it and `verify` never sees it.

use crate::emit::json::Json;
use std::path::Path;
use std::process::Command;

/// Where the record lands in the image, beside the baked manifest.
pub const RECORD: &str = "/usr/share/tectonic/build.json";

/// Where the generated plan is baked, which is what the image declares it is
/// made of.
pub const MANIFEST: &str = "/usr/share/tectonic/manifest.json";

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// What `<program> <args>` printed, or None when it is not installed or said
/// nothing. A resolution that fails is recorded as absent rather than fatal;
/// `audit { enforce }` is what makes it an error.
fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// A base image tag with the manifest digest it names right now appended, so
/// the result records what was asked for and pulls what answered.
pub fn base(reference: &str) -> Option<String> {
    if reference.contains('@') {
        return Some(reference.to_string());
    }
    let digest = output(
        "skopeo",
        &[
            "inspect",
            "--format",
            "{{.Digest}}",
            &format!("docker://{reference}"),
        ],
    )?;
    // The tag stays on: `repo:tag@sha256:...` is what was asked for and what
    // answered, and the digest is what a pull honours.
    digest
        .starts_with("sha256:")
        .then(|| format!("{reference}@{digest}"))
}

/// A cloned asset's selector as the commit it points at. A selector that is
/// already a commit resolves to itself; a tag is asked of the remote, which is
/// what makes a tag that moved later detectable.
pub fn clone_commit(url: &str, version: &str) -> Option<String> {
    if version.len() == 40 && version.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(version.to_lowercase());
    }
    let listed = output("git", &["ls-remote", url, version])?;
    let sha = listed.split_whitespace().next()?;
    (sha.len() == 40).then(|| sha.to_lowercase())
}

/// The commit the repository is at, which is what binds this image to a tree
/// someone can read.
pub fn source_commit(root: &Path) -> Option<String> {
    output(
        "git",
        &["-C", &root.display().to_string(), "rev-parse", "HEAD"],
    )
}

/// `a|b c|d` as pairs, which is how every list-shaped build argument already
/// reaches a layer.
fn pairs(raw: &str) -> Vec<Vec<&str>> {
    raw.split_whitespace()
        .map(|field| field.split('|').collect())
        .collect()
}

/// The record, from the arguments the generated Containerfile passed down.
/// Nothing here reads the repository: the layer has none.
pub fn build() -> Json {
    record(&env)
}

/// The shape of the build record. A host binary reads it back, and is pinned
/// independently of the one that wrote it, so the number is the whole of what
/// says the two agree.
pub const SCHEMA_VERSION: u32 = 1;

/// The record from any reading of the arguments, so what it says can be
/// checked without a container and without touching this process's environment.
fn record(env: &dyn Fn(&str) -> Option<String>) -> Json {
    let declared = env("BASE_DECLARED");
    let resolved = env("BASE");
    Json::object([
        ("schema_version", Json::Number(SCHEMA_VERSION)),
        ("target", Json::optional(env("TARGET"))),
        ("image", Json::optional(env("IMAGE_ID"))),
        ("version", Json::optional(env("IMAGE_VERSION"))),
        ("tect", Json::string(env!("CARGO_PKG_VERSION"))),
        // Which of buildx and buildah produced this. Nothing else in the
        // image can say, and the two do not always agree on what they make.
        ("backend", Json::optional(env("BUILD_BACKEND"))),
        ("source_commit", Json::optional(env("SOURCE_COMMIT"))),
        (
            "base",
            Json::object([
                ("declared", Json::optional(declared)),
                ("resolved", Json::optional(resolved)),
            ]),
        ),
        (
            "modules",
            Json::array(
                pairs(&env("MODULE_HASHES").unwrap_or_default())
                    .iter()
                    .filter(|f| f.len() == 2)
                    .map(|f| {
                        Json::object([
                            ("path", Json::string(f[0])),
                            ("content", Json::string(f[1])),
                        ])
                    }),
            ),
        ),
        (
            "assets",
            Json::array(
                pairs(&env("ASSET_RESOLUTIONS").unwrap_or_default())
                    .iter()
                    .filter(|f| f.len() == 4)
                    .map(|f| {
                        Json::object([
                            ("module", Json::string(f[0])),
                            ("name", Json::string(f[1])),
                            ("selector", Json::string(f[2])),
                            ("resolved", Json::string(f[3])),
                        ])
                    }),
            ),
        ),
        (
            "audit",
            Json::object([(
                "enforce",
                Json::Bool(env("AUDIT_ENFORCE").as_deref() == Some("true")),
            )]),
        ),
        // Nothing has checked the claims yet. The scan is what fills it in, and
        // saying so plainly is what stops the artifact implying an audit it did
        // not get.
        ("verified", Json::Null),
    ])
}

/// Writes the record into the image being built.
pub fn write() -> Result<(), String> {
    let path = Path::new(RECORD);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    std::fs::write(path, format!("{}\n", build().render()))
        .map_err(|err| format!("{RECORD}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a build stamps, from the arguments the Containerfile passes down.
    #[test]
    fn the_record_names_the_digest_and_says_whether_anything_was_enforced() {
        let given = |name: &str| {
            Some(match name {
                "BASE" => "quay.io/fedora/fedora-bootc:44@sha256:abc",
                "BASE_DECLARED" => "quay.io/fedora/fedora-bootc:44",
                "TARGET" => "example-desktop",
                "IMAGE_ID" => "example",
                "SOURCE_COMMIT" => "deadbeef",
                "BUILD_BACKEND" => "buildah",
                "MODULE_HASHES" => "core/hello|aaa apps/browser|bbb",
                "ASSET_RESOLUTIONS" => "mods/xone|xone|B7|3484f60",
                _ => return None,
            })
            .map(str::to_string)
        };
        let out = record(&given).render();

        assert!(out.contains("\"resolved\": \"quay.io/fedora/fedora-bootc:44@sha256:abc\""));
        assert!(out.contains("\"declared\": \"quay.io/fedora/fedora-bootc:44\""));
        assert!(out.contains("\"target\": \"example-desktop\""));
        assert!(out.contains("\"backend\": \"buildah\""));
        assert!(out.contains("\"path\": \"apps/browser\""));
        assert!(out.contains("\"selector\": \"B7\""));
        // Nothing was declared, so the record says so rather than staying silent.
        assert!(out.contains("\"enforce\": false"));
        assert!(out.contains("\"verified\": null"));
    }

    /// A reference that already names a digest is not asked about again, which
    /// is what makes `BASE` in the environment one resolution rather than two.
    #[test]
    fn a_reference_that_already_carries_a_digest_resolves_to_itself() {
        let pinned = "quay.io/fedora/fedora-bootc:44@sha256:abc";
        assert_eq!(base(pinned).as_deref(), Some(pinned));
        assert_eq!(
            clone_commit("", "3484f603484782dd7551c64e5a33fc602b127051").as_deref(),
            Some("3484f603484782dd7551c64e5a33fc602b127051")
        );
    }
}

//! The cache and tree of a declared collection.
//!
//! A collection names a local directory or a Git URL and a selector. A fetch
//! keeps the Git tree under the sources cache, with a stamp beside it that
//! names the hash the tree holds. A pinned collection reads only a tree whose
//! stamp matches.

use crate::layout;
use crate::model::remote::{At, Collection};
use std::path::{Path, PathBuf};

/// Answers every member of a collection by its path in the tree.
pub(crate) fn members(tree: &Path) -> Vec<String> {
    layout::member_dirs(tree)
        .iter()
        .map(|path| {
            path.strip_prefix(tree)
                .unwrap_or(path)
                .display()
                .to_string()
        })
        .collect()
}

/// The source's tree where it is already on this machine: the directory it
/// is, a repository fetched at the hash it is still pinned to, or whatever the
/// last import of an unpinned one left. Nothing is fetched.
pub fn cached(root: &Path, collection: &Collection) -> Option<PathBuf> {
    let dir = match &collection.at {
        At::Dir(dir) => root.join(dir),
        At::Git(pin) => {
            let key = source_key(pin).ok()?;
            let dir = root.join(layout::SOURCES_CACHE).join(&key);
            if pin.sha256.is_none() {
                return dir.is_dir().then_some(dir);
            }
            let stamp = root.join(layout::SOURCES_CACHE).join(format!("{key}.pin"));
            match std::fs::read_to_string(&stamp).ok().as_deref() == pin.sha256.as_deref() {
                true => dir,
                false => return None,
            }
        }
    };
    dir.is_dir().then_some(dir)
}

/// The source's tree on this machine: the directory it already is, or the
/// fetched repository, verified once and kept for the next lookup. An unpinned
/// one is fetched once per process, since the ref it follows may have moved.
pub(crate) fn tree(root: &Path, collection: &Collection) -> Result<PathBuf, String> {
    let remote = match &collection.at {
        At::Dir(dir) => {
            let dir = root.join(dir);
            if dir.is_dir() {
                return Ok(dir);
            }
            return Err(format!(
                "`{}` is {}, which is not a directory on this machine",
                collection.name,
                dir.display()
            ));
        }
        At::Git(remote) => remote,
    };

    let key = source_key(remote)?;
    let cache = root.join(layout::SOURCES_CACHE);
    let dir = cache.join(&key);
    let pin = cache.join(format!("{key}.pin"));
    let run = cache.join(format!("{key}.run"));
    let this_run = std::process::id().to_string();
    if remote.sha256.is_some() {
        if let Some(current) = cached(root, collection) {
            return Ok(current);
        }
    } else if dir.is_dir()
        && std::fs::read_to_string(&run).is_ok_and(|found| found.trim() == this_run)
    {
        return Ok(dir);
    }

    std::fs::create_dir_all(&cache).map_err(|err| format!("{}: {err}", cache.display()))?;
    let work = cache.join(format!(".{key}.fetching.{}", std::process::id()));
    crate::init::remove(&work)?;
    let work = crate::init::Scratch(work);
    fetch_git(remote, &work.0).map_err(|err| format!("`{}`: {err}", collection.name))?;
    crate::init::remove(&dir)?;
    let fetched = work.0.join("tree");
    std::fs::rename(&fetched, &dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    match &remote.sha256 {
        Some(sha256) => {
            crate::init::put(&pin, &format!("{sha256}\n"))?;
            crate::init::remove(&run)?;
        }
        None => {
            crate::init::put(&run, &format!("{this_run}\n"))?;
            crate::init::remove(&pin)?;
        }
    }
    Ok(dir)
}

fn source_key(remote: &crate::provenance::Evidence) -> Result<String, String> {
    let url = remote.url.as_deref().ok_or("source has no Git URL")?;
    let selector = remote.selector();
    cache_key(url, selector)
}

/// The key a Git URL and selector share, which is what makes two sources
/// naming one library fetch one tree.
pub fn cache_key(url: &str, selector: &str) -> Result<String, String> {
    crate::runtime::sha256_stdin(format!("{url}\0{selector}").as_bytes())
}

/// The directory a fetched source's tree is cached under, for a caller that
/// seeds or inspects the cache.
pub fn cache_path(root: &Path, url: &str, selector: &str) -> Result<PathBuf, String> {
    Ok(root
        .join(layout::SOURCES_CACHE)
        .join(cache_key(url, selector)?))
}

fn git(repo: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "/bin/false")
        .env("SSH_ASKPASS", "/bin/false")
        .output()
        .map_err(|err| format!("git {}: {err}", args.join(" ")))
}

fn git_ok(repo: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let out = git(repo, args)?;
    if out.status.success() {
        return Ok(out);
    }
    Err(format!(
        "git {}: {}\n{}",
        args.join(" "),
        out.status,
        String::from_utf8_lossy(&out.stderr).trim_end()
    ))
}

fn fetch_git(remote: &crate::provenance::Evidence, work: &Path) -> Result<(), String> {
    let url = remote.url.as_deref().ok_or("source has no Git URL")?;
    let selector = remote.selector();
    // `git archive` resolves a relative `--output` under the bare repository it
    // runs in, so every path here has to be absolute first.
    let work = {
        let repo = work.join("repo.git");
        std::fs::create_dir_all(&repo).map_err(|err| format!("{}: {err}", repo.display()))?;
        std::fs::canonicalize(work).map_err(|err| format!("{}: {err}", work.display()))?
    };
    let repo = work.join("repo.git");
    let tree = work.join("tree");
    let archive = work.join("source.tar");
    git_ok(&repo, &["init", "--bare", "--quiet"])?;
    git_ok(
        &repo,
        &[
            "-c",
            "protocol.version=2",
            "fetch",
            "--quiet",
            "--depth=1",
            "--no-tags",
            // `--` ends the options, so a URL is data even when it starts
            // with a dash.
            "--",
            url,
            selector,
        ],
    )?;
    let commit = git_ok(&repo, &["rev-parse", "FETCH_HEAD^{commit}"])?;
    let commit = String::from_utf8_lossy(&commit.stdout).trim().to_string();
    let archive_arg = archive.to_string_lossy();
    git_ok(
        &repo,
        &[
            "archive",
            "--format=tar",
            &format!("--output={archive_arg}"),
            &commit,
        ],
    )?;
    if let Some(expected) = &remote.sha256 {
        crate::runtime::match_sha256(&archive, expected, &format!("{url} at {selector}"))?;
    }
    std::fs::create_dir_all(&tree).map_err(|err| format!("{}: {err}", tree.display()))?;
    let out = std::process::Command::new("tar")
        .args([
            "-xf",
            &archive.to_string_lossy(),
            "-C",
            &tree.to_string_lossy(),
        ])
        .output()
        .map_err(|err| format!("tar: {err}"))?;
    if !out.status.success() {
        return Err(format!(
            "tar: {}{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

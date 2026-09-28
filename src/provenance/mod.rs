//! Where everything in a repository came from, in one shape.
//!
//! Every pin in the tree fills the same four slots: the locator says where it
//! comes from, the selector which version, the verifier what proves you got
//! that one, and the tracker who keeps the selector current. `asset`, an
//! out-of-tree module and a collection each hold the one `pin` table; the base
//! carries locator and selector joined in the image reference, and `signed` as
//! its verifier.

pub mod build;
pub mod evidence;
pub mod record;

use crate::diag::Span;
use common::json::Json;

/// Where the expected hash comes from when a version bump makes the pinned one
/// stale.
#[derive(Clone, Copy, PartialEq)]
pub enum ShaFrom {
    /// Hash the payload at the locator.
    Asset,
    /// Upstream publishes `<url>.sha256` beside it.
    Sidecar,
    /// Nothing derives it.
    Manual,
}

impl ShaFrom {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "asset" => Some(Self::Asset),
            "sidecar" => Some(Self::Sidecar),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Sidecar => "sidecar",
            Self::Manual => "manual",
        }
    }
}

/// Who keeps the selector current.
pub enum Tracker {
    /// Renovate, through the custom manager matching this datasource, against
    /// what that datasource calls the thing.
    Renovate {
        datasource: String,
        dep_name: String,
    },
    /// Nothing does, and why.
    Manual(String),
    /// Nothing does, the ref moves, and there is no verifier: every fetch takes
    /// whatever it held then.
    Unpinned(String),
    /// Nothing was declared, which is a diagnostic everywhere a pin is read.
    None,
}

impl Tracker {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Renovate { .. } => "renovate",
            Self::Manual(_) => "manual",
            Self::Unpinned(_) => "unpinned",
            Self::None => "none",
        }
    }

    /// What the author said about it, which only the two untracked answers
    /// carry.
    pub fn reason(&self) -> Option<String> {
        match self {
            Self::Manual(why) | Self::Unpinned(why) => Some(why.clone()),
            _ => None,
        }
    }
}

/// One pin, filled the same way whatever declared it.
pub struct Evidence {
    /// Unexpanded, `{version}` included, because this is what a reviewer reads
    /// and what the checksum workflow rewrites around.
    pub url: Option<String>,
    pub version: Option<String>,
    pub sha256: Option<String>,
    pub from: ShaFrom,
    /// The directory inside the archive the content sits in.
    pub path: Option<String>,
    pub tracker: Tracker,
    pub span: Span,
}

impl Evidence {
    pub fn new(span: Span) -> Self {
        Evidence {
            url: None,
            version: None,
            sha256: None,
            from: ShaFrom::Asset,
            path: None,
            tracker: Tracker::None,
            span,
        }
    }

    /// The URL a fetch actually requests.
    pub fn url_resolved(&self) -> Option<String> {
        let url = self.url.as_ref()?;
        Some(match &self.version {
            Some(version) => url.replace("{version}", version),
            None => url.clone(),
        })
    }

    /// Whether the content is a git clone, which is what makes the commit the
    /// selector names the verifier. A download is verified by a hash.
    pub fn cloned(&self) -> bool {
        self.url.as_deref().is_some_and(|url| url.ends_with(".git"))
    }

    /// Whether it follows a moving ref, which is what makes every fetch of it a
    /// different tree with nothing to verify it against.
    pub fn unpinned(&self) -> bool {
        matches!(self.tracker, Tracker::Unpinned(_))
    }

    /// The four slots, then what each of them was spelled with. Everything that
    /// records a provenance fact goes through here, so one shape reaches
    /// `plan.json`, the build record and `tect why`.
    pub fn json(&self) -> Json {
        Json::object([
            ("locator", Json::optional(self.url.clone())),
            ("selector", Json::optional(self.version.clone())),
            ("verifier", Json::optional(self.sha256.clone())),
            ("tracker", Json::string(self.tracker.as_str())),
            ("reason", Json::optional(self.tracker.reason())),
            ("from", Json::string(self.from.as_str())),
            ("path", Json::optional(self.path.clone())),
            ("resolved", Json::optional(self.url_resolved())),
        ])
    }
}

// ---- policy --------------------------------------------------------------

/// The network verbs a build layer reaches the outside world with. Closed, and
/// held as data, so what counts as a fetch is one list to read.
const FETCHES: [&str; 8] = [
    "curl ",
    "wget ",
    "git clone",
    "pip install",
    "pip3 install",
    "npm install",
    "cargo install",
    "go install",
];

/// The scripts a module runs, which are the two places it can fetch from --
/// each also under a family directory, where the same script runs on one family
/// alone and can fetch just as undeclared.
const SCRIPTS: [&str; 2] = ["module.sh", "finalize.sh"];

fn scripts() -> Vec<String> {
    SCRIPTS
        .iter()
        .map(|name| name.to_string())
        .chain(
            crate::layout::FAMILY_DIRS
                .iter()
                .flat_map(|(gated, _)| SCRIPTS.iter().map(move |name| format!("{gated}/{name}"))),
        )
        .collect()
}

/// A module that reaches the network with nothing declaring what it pulls.
/// Always on, whatever the posture: no record can describe an undeclared fetch
/// after the fact.
pub fn check_fetch(
    module: &crate::model::module::Module,
    dir: &std::path::Path,
    issues: &mut crate::diag::Issues,
) {
    if !module.assets.is_empty() {
        return;
    }
    for script in scripts() {
        let Ok(text) = std::fs::read_to_string(dir.join(&script)) else {
            continue;
        };
        let Some(verb) = FETCHES.into_iter().find(|verb| {
            text.lines()
                .map(str::trim_start)
                .any(|line| !line.starts_with('#') && line.contains(verb))
        }) else {
            continue;
        };
        issues.push(
            crate::diag::Issue::new(
                format!(
                    "`{}` fetches in {script} with nothing declaring what",
                    module.path
                ),
                &module.src,
            )
            .at(
                Span::default(),
                format!("`{}` reaches the network", verb.trim()),
            )
            .help(
                "declare an `asset` with the url, the version and the sha256, and let the layer \
                 read it out of ASSET_*; an undeclared fetch is the one build input no record \
                 can describe after the fact",
            ),
        );
        return;
    }
}

/// Reports a module whose declarations need a network step that repo.kdl
/// closes. The build would fail on its first lookup, so the check names the
/// rule that closed the step.
pub fn check_network(
    image: &crate::model::image::Image,
    scripts_rule: crate::model::image::NetRule,
    root: &std::path::Path,
    issues: &mut crate::diag::Issues,
) {
    // A `deny` rule closes the step whatever the module declares, so the
    // declaration is no way out of it.
    let reopen = match scripts_rule {
        crate::model::image::NetRule::Deny => {
            "set `scripts \"strict\"` in repo.kdl's `security-policy { network }` and declare \
             `network \"scripts\"`, or leave this module out of the image"
        }
        crate::model::image::NetRule::Allow | crate::model::image::NetRule::Strict => {
            "declare `network \"scripts\"`, which a `strict` rule in repo.kdl opens the \
             script step for"
        }
    };
    let base_family = image.base.as_ref().map_or("", |b| b.family.as_str());
    use crate::diag::Issue;
    for entry in &image.entries {
        let Some(module) = entry.module.as_ref() else {
            continue;
        };
        if !module.standard_layer {
            if let Some(span) = module.network {
                issues.push(
                    Issue::new(
                        format!("`{}` declares a script step it has not got", module.path),
                        &module.src,
                    )
                    .at(span, "no standard layer")
                    .help(
                        "`standard-layer #false` leaves the fragment the only thing this \
                           module emits, and a fragment's own RUN lines are emitted as written",
                    ),
                );
            }
            continue;
        }
        if !module.access.packages
            && crate::emit::module_build::installs_packages(image, entry, module, root)
        {
            issues.push(
                Issue::new(
                    format!(
                        "`{}` installs packages, and repo.kdl denies the package step the network",
                        module.path
                    ),
                    &module.src,
                )
                .help(
                    "set `packages \"allow\"` in repo.kdl's `security-policy { network }`, or leave this \
                       module out of the image",
                ),
            );
        }
        if module.access.scripts {
            continue;
        }
        match module.network {
            Some(span) => issues.push(
                Issue::new(
                    format!(
                        "`{}` declares that its scripts reach the network, and repo.kdl denies it",
                        module.path
                    ),
                    &module.src,
                )
                .at(span, "denied by `scripts \"deny\"`")
                .help(
                    "set `scripts \"strict\"` in repo.kdl's `security-policy { network }` to honour this \
                       declaration, or leave this module out of the image",
                ),
            ),
            None => {
                for asset in &module.assets {
                    issues.push(
                        Issue::new(
                            format!(
                                "`{}` declares an asset its script step cannot fetch",
                                module.path
                            ),
                            &module.src,
                        )
                        .at(asset.span, "fetched with the network closed")
                        .help(reopen),
                    );
                }
                let dir = crate::layout::module(root, entry.dir());
                if let Some((script, verb)) = installer(&dir, base_family) {
                    issues.push(
                        Issue::new(
                            format!(
                                "`{}` installs packages in {script}, and its script step has \
                                 no network",
                                module.path
                            ),
                            &module.src,
                        )
                        .at(
                            Span::default(),
                            format!("`{}` needs the network", verb.trim()),
                        )
                        .help(reopen),
                    );
                }
            }
        }
    }
}

/// The package managers. A call to one reaches the network unless its
/// subcommand works from the installed package database alone.
const MANAGERS: [&str; 8] = [
    "dnf ",
    "dnf5 ",
    "yum ",
    "microdnf ",
    "rpm-ostree ",
    "apt ",
    "apt-get ",
    "zypper ",
];

/// These subcommands work from the installed package database alone, so they
/// run under `--network=none`.
const OFFLINE: [&str; 5] = ["remove", "erase", "purge", "autoremove", "clean"];

/// The layer helpers that call a package manager whatever their arguments.
const HELPERS: [&str; 6] = [
    "install_packages ",
    "install_groups ",
    "enable_copr ",
    "add_disabled_repo ",
    "fetch_install_rpm ",
    "ensure_checkpolicy",
];

/// Returns the first script of the module that calls a package manager, and
/// the call. The family directories this base does not select never run, so
/// they are not read.
fn installer(dir: &std::path::Path, base_family: &str) -> Option<(String, &'static str)> {
    let roots = std::iter::once(String::new()).chain(
        crate::layout::family_dirs(dir, base_family)
            .into_iter()
            .map(|gated| format!("{gated}/")),
    );
    for root in roots {
        for name in SCRIPTS {
            let script = format!("{root}{name}");
            let Ok(text) = std::fs::read_to_string(dir.join(&script)) else {
                continue;
            };
            let found = MANAGERS.into_iter().chain(HELPERS).find(|verb| {
                text.lines()
                    .map(str::trim_start)
                    .any(|line| !line.starts_with('#') && reaches(line, verb))
            });
            if let Some(verb) = found {
                return Some((script, verb));
            }
        }
    }
    None
}

/// Whether the line calls the verb in a way that reaches the network. The verb
/// starts a command, so `apt ` inside `adapt ` or `/etc/apt ` is no call.
fn reaches(line: &str, verb: &str) -> bool {
    let mut rests = line
        .match_indices(verb)
        .filter(|(at, _)| {
            line[..*at]
                .chars()
                .next_back()
                .is_none_or(|c| c.is_whitespace() || "(;&|`".contains(c))
        })
        .map(|(at, _)| &line[at + verb.len()..]);
    match HELPERS.contains(&verb) {
        true => rests.next().is_some(),
        false => rests.any(|rest| {
            rest.split_whitespace()
                .find(|word| !word.starts_with('-'))
                .is_none_or(|subcommand| !OFFLINE.contains(&subcommand))
        }),
    }
}

/// The modules whose Containerfile fragment a closed script rule does not
/// reach. A fragment's own `RUN` lines are emitted as written, so the rule
/// cannot govern them. That is a limit, and a silent limit is what makes a
/// rule read wider than it is, so `check` names each one.
pub fn ungoverned(
    image: &crate::model::image::Image,
    scripts_rule: crate::model::image::NetRule,
) -> Vec<String> {
    if scripts_rule == crate::model::image::NetRule::Allow {
        return Vec::new();
    }
    image
        .entries
        .iter()
        .filter_map(|entry| entry.module.as_ref())
        .filter(|module| module.fragment.is_some())
        .map(|module| module.path.clone())
        .collect()
}

/// What `audit { enforce }` refuses at build time: a record that would not name
/// the digest it built on, or would not bind the image to a tree anyone can
/// read. Apart from the build, so the posture is checkable without running one.
pub fn enforce_build(
    enforce: bool,
    resolved_base: Option<&str>,
    source_commit: Option<&str>,
) -> Result<(), String> {
    if !enforce {
        return Ok(());
    }
    if resolved_base.is_none() {
        return Err(
            "the base did not resolve to a manifest digest, so the build record would \
                    not name what it built on; `audit { enforce #true }` makes that an error"
                .into(),
        );
    }
    if source_commit.is_none() {
        return Err(
            "the repository is at no commit, so the build record would not bind this \
                    image to a tree anyone can read; `audit { enforce #true }` makes that an \
                    error"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installer_starts_a_command() {
        assert!(reaches("apt-get install -y vim", "apt-get "));
        assert!(reaches("x && dnf5 install -y vim", "dnf5 "));
        assert!(!reaches("we adapt the config", "apt "));
        assert!(!reaches("mkdir -p /etc/apt tect-build", "apt "));
        assert!(!reaches("fetch_install_packages foo", "install_packages "));
    }

    #[test]
    fn a_removal_needs_no_network() {
        assert!(!reaches("dnf5 -y remove --noautoremove bootupd", "dnf5 "));
        assert!(!reaches("dnf -y clean all", "dnf "));
        assert!(reaches("dnf5 distro-sync -y --repo=x pkg", "dnf5 "));
        assert!(reaches("install_packages vim", "install_packages "));
    }

    /// Unenforced records whatever it has; enforced refuses what it cannot
    /// record. Same facts, one lever.
    #[test]
    fn enforcement_is_a_lever_over_a_record_that_always_exists() {
        assert!(enforce_build(false, None, None).is_ok());
        assert!(enforce_build(true, Some("repo@sha256:abc"), Some("deadbeef")).is_ok());

        let no_base = enforce_build(true, None, Some("deadbeef")).unwrap_err();
        assert!(no_base.contains("did not resolve"), "{no_base}");
        let no_commit = enforce_build(true, Some("repo@sha256:abc"), None).unwrap_err();
        assert!(no_commit.contains("no commit"), "{no_commit}");
    }
}

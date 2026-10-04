//! Where everything sits in a repository, stated once. The shape is the tool's
//! own and is not configurable: every repository looking alike is what lets a
//! stranger read one, and what lets a diagnostic name a path outright.

use std::path::{Path, PathBuf};

/// Vendored and authored modules, one directory apiece.
pub const MODULES: &str = "modules";
/// Everything the tool writes and `verify` byte-compares. Tracked.
pub const GENERATED: &str = "generated";
/// Local exports, caches and scratch. Ignored, and nothing here is read back
/// as a declaration.
pub const OUT: &str = "out";
/// Repository key material: committed public halves and ignored private halves.
pub const KEYS: &str = "keys";
const PUBLIC_KEYS: &str = "public";
const PRIVATE_KEYS: &str = "private";

/// The install logic of a module, which the build sources in its layer.
pub const SCRIPT: &str = "module.sh";
/// Containerfile lines that a module ships for the build to add verbatim.
pub const FRAGMENT: &str = "Containerfile.inc";

/// A module's overlay tree, staged into the image by the collector.
pub const OVERLAY: &str = "files";

/// Mandatory access control policy a module ships: which directory holds it,
/// what marks a file in there as policy, and the capability that says the
/// image has that MAC. A module may ship for more than one and is built for
/// whichever the image has.
pub struct Policy {
    pub dir: &'static str,
    /// The suffix a policy source carries, or nothing where the filename is
    /// itself the identifier.
    pub ext: Option<&'static str>,
    pub capability: &'static str,
}

pub const SELINUX: Policy = Policy {
    dir: "selinux",
    ext: Some("te"),
    capability: "selinux-policy",
};

/// AppArmor names a profile by its filename, so nothing is stripped and no
/// suffix is expected: `apparmor/usr.bin.foo` is placed as that name.
pub const APPARMOR: Policy = Policy {
    dir: "apparmor",
    ext: None,
    capability: "apparmor-policy",
};

impl Policy {
    /// The policy sources one module ships, sorted so two runs emit the same
    /// script. A subdirectory is not a profile: AppArmor abstractions and
    /// tunables are includes, and a module placing one uses `files/`.
    pub fn files(&self, module_dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(module_dir.join(self.dir)) else {
            return Vec::new();
        };
        let mut out: Vec<String> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|ty| ty.is_file()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| match self.ext {
                Some(ext) => name.ends_with(&format!(".{ext}")),
                None => true,
            })
            .collect();
        out.sort();
        out
    }
}

/// Every directory name a module may gate files behind, and the families each
/// is taken on. `deb` and `rpm` each name the two families that share a
/// package manager and an installer. Ordered widest first: a `debian/` beside
/// a `deb/` is taken after it.
pub const FAMILY_DIRS: [(&str, &[&str]); 6] = [
    ("deb", &["debian", "ubuntu"]),
    ("rpm", &["fedora", "rhel"]),
    ("fedora", &["fedora"]),
    ("rhel", &["rhel"]),
    ("debian", &["debian"]),
    ("ubuntu", &["ubuntu"]),
];

/// Where a file a module ships is read from, most specific first and the module
/// root last: what a lookup that picks one copy walks. The reverse of
/// `family_dirs`, whose order is what a layering emitter wants.
pub fn family_first(module_dir: &Path, family: &str) -> Vec<String> {
    family_dirs(module_dir, family)
        .into_iter()
        .rev()
        .map(|gated| format!("{gated}/"))
        .chain(std::iter::once(String::new()))
        .collect()
}

/// A file one module ships, as the path inside its directory that a build
/// reads it at: the family copy where there is one, and the ungated copy
/// otherwise. `None` where the module ships it nowhere.
pub fn shipped(module_dir: &Path, family: &str, file: &str) -> Option<String> {
    family_first(module_dir, family)
        .into_iter()
        .map(|at| format!("{at}{file}"))
        .find(|at| module_dir.join(at).is_file())
}

/// The ones this module ships for this family, in the order they are taken.
/// Additive: an ungated `module.sh` and `files/` at the module root still run
/// everywhere. Empty where the image declares no family.
pub fn family_dirs(module_dir: &Path, family: &str) -> Vec<&'static str> {
    FAMILY_DIRS
        .iter()
        .filter(|(_, taken_on)| taken_on.contains(&family))
        .map(|(name, _)| *name)
        .filter(|name| module_dir.join(name).is_dir())
        .collect()
}

/// Whether a directory inside a module is one that the module's own build
/// reads. The walk for nested modules passes over it.
pub fn module_owned(name: &str) -> bool {
    name == OVERLAY
        || name == SELINUX.dir
        || name == APPARMOR.dir
        || FAMILY_DIRS.iter().any(|(dir, _)| *dir == name)
}

/// Every module directory below `tree`, sorted. `marks` decides whether a
/// directory is a module. The walk never enters a directory that `skip` names,
/// and inside a module it passes over what `module_owned` names, because a
/// module may hold modules of its own. A link to a directory is read as a
/// module and never entered, so a link back to a parent cannot loop.
pub fn module_dirs(
    tree: &Path,
    marks: impl Fn(&Path) -> bool,
    skip: impl Fn(&str) -> bool,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // Each directory is paired with whether it is a module.
    let mut dirs = vec![(tree.to_path_buf(), false)];
    while let Some((dir, in_module)) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if !path.is_dir() || skip(&name) || (in_module && module_owned(&name)) {
                continue;
            }
            let module = marks(&path);
            if module {
                out.push(path.clone());
            }
            if !entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
                dirs.push((path, module));
            }
        }
    }
    out.sort();
    out
}

/// The scripts that a repository keeps in place of the ones `generate` writes.
pub const SCRIPTS: &str = "scripts";

/// Where a tool-owned script lives, relative to the root. If the repository
/// keeps its own copy in `scripts/`, then `generate` writes none.
pub fn script(root: &Path, name: &str) -> String {
    match root.join(SCRIPTS).join(name).is_file() {
        true => format!("{SCRIPTS}/{name}"),
        false => format!("{GENERATED}/{SCRIPTS}/{name}"),
    }
}

/// Whether a directory under `modules/` is a module. A module may ship no
/// module.kdl, so the files that a build reads at its root also mark it. A
/// family directory does not, because a group of modules may carry a family's
/// name.
pub fn is_module(dir: &Path) -> bool {
    dir.join(MODULE_FILE).is_file()
        || dir.join(SCRIPT).is_file()
        || dir.join(FRAGMENT).is_file()
        || dir.join(OVERLAY).is_dir()
}

/// GitHub's path, not this repository's choice, which is why it is written
/// here.
pub const WORKFLOW_DIR: &str = ".github/workflows";

/// Repo context, not an image, and the file whose presence marks a root.
pub const REPO_FILE: &str = "repo.kdl";
/// An image file with no name in front of it, which is what a repository
/// holding one image wants to call it.
pub const IMAGE_FILE: &str = "image.kdl";
/// What names the rest. The name in front is decorative: an image is called
/// what it declares, and a file may hold as many as it likes.
pub const IMAGE_SUFFIX: &str = ".image.kdl";
/// What a module declares about itself.
pub const MODULE_FILE: &str = "module.kdl";
/// What `copy module` writes beside a vendored module.kdl, never into it.
pub const RECORD_FILE: &str = "provenance.kdl";

/// Where the `tect` build stage copies the binary from, so every layer mounts
/// the release the repository is pinned to.
pub const MOUNTED: &str = "out/tect";
/// Where a fetched collection is unpacked. Under `out/`, which is ignored: the
/// selected member is referenced or copied from here.
pub const SOURCES_CACHE: &str = "out/sources";
/// What a fetched tree hashed to, kept out of the tree it describes so nothing
/// under `modules/` is tool-written state.
pub const STAMPS: &str = "out/remote-modules";

/// Whether a root file holds images. An allowlist, so a root `.kdl` that is
/// neither is reported. "Everything but repo.kdl" would parse it as an image.
pub fn is_image_file(name: &str) -> bool {
    name == IMAGE_FILE || name.ends_with(IMAGE_SUFFIX)
}

/// What a misnamed root `.kdl` would have to be called, for the diagnostic that
/// reports one.
pub fn as_image_file(name: &str) -> String {
    format!("{}{IMAGE_SUFFIX}", name.trim_end_matches(".kdl"))
}

pub fn modules(root: &Path) -> PathBuf {
    root.join(MODULES)
}

/// One module's directory, by its path relative to `modules/`.
pub fn module(root: &Path, dir: impl AsRef<Path>) -> PathBuf {
    modules(root).join(dir)
}

pub fn public_key(root: &Path, path: &str) -> PathBuf {
    root.join(KEYS)
        .join(PUBLIC_KEYS)
        .join(path.trim_start_matches('/'))
}

pub fn private_key(root: &Path, name: &str) -> PathBuf {
    root.join(KEYS).join(PRIVATE_KEYS).join(name)
}

pub fn nonempty(path: &Path) -> bool {
    path.metadata().is_ok_and(|meta| meta.len() > 0)
}

/// The module.kdl inside it, which is the file that declares it exists.
pub fn manifest(root: &Path, dir: impl AsRef<Path>) -> PathBuf {
    module(root, dir).join(MODULE_FILE)
}

pub fn generated(root: &Path) -> PathBuf {
    root.join(GENERATED)
}

/// Everything `generate` writes for one image, under a directory of its own.
pub fn generated_image(id: &str) -> PathBuf {
    PathBuf::from(GENERATED).join(id)
}

/// What the generated Containerfile is called.
pub const CONTAINERFILE: &str = "Containerfile";

/// The one directory under `generated/` that belongs to no image: the helper
/// library every module build mounts at `/ctx/lib`. It is a sibling of
/// `generated_image`, so no image may be called this.
pub const LIB: &str = "lib";

pub fn out(root: &Path) -> PathBuf {
    root.join(OUT)
}

/// One path that the schema reference lists. `path` is joined with nothing
/// between the parts, so a part carries its own `/`.
pub struct Place {
    pub path: &'static [&'static str],
    /// The command or the person that writes the path. If it is empty, then
    /// the reference leaves the column out.
    pub writer: &'static str,
    /// Whether git tracks the path in a repository that `tect create repo`
    /// scaffolds.
    pub tracked: bool,
    pub what: &'static str,
}

/// A page of the schema reference that describes where things sit, and not
/// the grammar of one file.
pub struct Tree {
    /// The directory that the drawn tree hangs off.
    pub root: &'static str,
    pub intro: &'static str,
    pub places: &'static [Place],
    pub lists: &'static [(&'static str, &'static [&'static str])],
    pub notes: &'static [&'static str],
}

/// Where everything sits in a repository.
#[rustfmt::skip]
pub const REPOSITORY: Tree = Tree {
    root: "<repository>/",
    intro: "A repository is a git repository with `repo.kdl` at its root, which is how `tect` \
        finds the root. `tect create repo` scaffolds the layout below. Every path that `tect` \
        reads or writes has a fixed place, so every repository looks alike.",
    places: &[
        Place { path: &[REPO_FILE], writer: "`tect create repo`, then the user", tracked: true,
            what: "The settings of the whole repository. See [`repo.kdl`](repo.md)." },
        Place { path: &[IMAGE_FILE], writer: "`tect create image`, then the user", tracked: true,
            what: "The images of the repository. See [`image.kdl`](image.md)." },
        Place { path: &["<name>", IMAGE_SUFFIX], writer: "`tect create image`, then the user", tracked: true,
            what: "More images. The name in front of the suffix is decorative." },
        Place { path: &[MODULES, "/"], writer: "the user, `tect create module` or `tect copy module`", tracked: true,
            what: "The modules of the repository, one directory each. See [Modules](modules.md)." },
        Place { path: &[MODULES, "/", crate::model::remote::REMOTE_DIR, "/"], writer: "`tect`", tracked: false,
            what: "The modules that an image pins from another repository, fetched and verified \
                against their hash." },
        Place { path: &[KEYS, "/", PUBLIC_KEYS, "/"], writer: "`tect create key` or `tect set key`", tracked: true,
            what: "The public half of each key, at the path it has in the image." },
        Place { path: &[KEYS, "/", PRIVATE_KEYS, "/"], writer: "`tect create key`", tracked: false,
            what: "The private half of each key. The build never copies it into the image." },
        Place { path: &[GENERATED, "/"], writer: "`tect generate`", tracked: true,
            what: "Everything that `tect generate` writes. `tect verify` compares it byte for \
                byte, so the user does not edit it." },
        Place { path: &[SCRIPTS, "/"], writer: "`tect create scripts`, then the user", tracked: true,
            what: "The scripts that the repository keeps in place of the ones `tect generate` \
                writes into `generated/scripts/`." },
        Place { path: &[WORKFLOW_DIR, "/"], writer: "`tect generate`", tracked: true,
            what: "The CI workflows that `workflows` in `repo.kdl` names." },
        Place { path: &[OUT, "/"], writer: "`tect`", tracked: false,
            what: "Local exports, caches and scratch. `tect` reads nothing here as a declaration." },
        Place { path: &[SOURCES_CACHE, "/"], writer: "`tect`", tracked: false,
            what: "The collections that `tect` fetches. An imported module is read from here." },
        Place { path: &["disk_config/"], writer: "`tect create repo`, then the user", tracked: true,
            what: "The settings that bootc-image-builder reads when it builds a Fedora or RHEL disk \
                image. `disk.toml` sets the smallest size of `/`." },
        Place { path: &[".github/renovate.json5"], writer: "`tect create repo`, then the user", tracked: true,
            what: "The Renovate settings. Its custom managers keep the `tect` release in `repo.kdl` \
                and each pin that declares `renovate` current." },
        Place { path: &[".gitignore"], writer: "`tect create repo`", tracked: true,
            what: "Keeps `keys/private/`, `out/` and `modules/.remote/` out of git." },
    ],
    lists: &[],
    notes: &[],
};

/// Where everything sits in one module directory.
#[rustfmt::skip]
pub const MODULE_DIR: Tree = Tree {
    root: "modules/<module-name>/",
    intro: "A module is a directory under `modules/`, at whatever depth groups it, and its path \
        names the module. The build reads the files below by convention, so a module can be as \
        small as one `module.sh`. A module can also hold other modules. The walk that finds \
        modules passes over the directories that the build of a module reads.",
    places: &[
        Place { path: &[MODULE_FILE], writer: "", tracked: true,
            what: "What the module declares about itself. A module with only a script and \
                `files/` can leave it out. A module fetched from another repository needs one. \
                See [`module.kdl`](module.md)." },
        Place { path: &[SCRIPT], writer: "", tracked: true,
            what: "The install logic. The build sources it in the layer of the module." },
        Place { path: &["finalize.sh"], writer: "", tracked: true,
            what: "The finalize phase sources it, in resolved order." },
        Place { path: &[OVERLAY, "/"], writer: "", tracked: true,
            what: "An overlay that the build copies over `/`." },
        Place { path: &["repo"], writer: "", tracked: true,
            what: "A package repository file. The build sources it first. On Fedora, if the \
                `REPO_ID` of `repo` is already configured, then the build skips it." },
        Place { path: &[FRAGMENT], writer: "", tracked: true,
            what: "Containerfile lines that the build adds verbatim, for a need that the \
                fields cannot express. `fragment` in `module.kdl` places them." },
        Place { path: &[SELINUX.dir, "/*.te"], writer: "", tracked: true,
            what: "SELinux policy. If the image provides `selinux-policy`, then the build \
                compiles and installs it." },
        Place { path: &[APPARMOR.dir, "/"], writer: "", tracked: true,
            what: "AppArmor profiles. If the image provides `apparmor-policy`, then the build \
                validates them and places them in `/etc/apparmor.d`." },
        Place { path: &["<family>/"], writer: "", tracked: true,
            what: "The files of the module that apply to one base family." },
        Place { path: &["<collected file>"], writer: "", tracked: true,
            what: "A part of a file that another module collects. The build stages it for that \
                module." },
        Place { path: &[RECORD_FILE], writer: "", tracked: true,
            what: "Where a copied module came from, which `tect copy module` writes. See \
                [`provenance.kdl`](provenance.md)." },
    ],
    lists: &[
        ("A policy directory follows these rules:", &[
            "A module that ships `selinux/` or `apparmor/` builds after the module that provides \
             that MAC, as if it declared `after`.",
            "A module can ship both policy directories. The build takes each directory only where \
             the image has that MAC. An image with no MAC installs no policy and needs no provider.",
            "A policy directory declares no requirement. A module that needs a MAC declares \
             `requires \"selinux-policy\"` or `requires \"apparmor-policy\"`, and `tect` refuses \
             it on an image without that MAC.",
        ]),
        ("A `<family>/` directory gates `module.sh`, `finalize.sh`, `files/`, `repo` and a \
          collected file:", &[
            "The six directory names are `fedora/`, `rhel/`, `debian/`, `ubuntu/`, `rpm/` and \
             `deb/`. The build reads no other directory in a module.",
            "The build takes `deb/` on Debian and Ubuntu, and `rpm/` on Fedora and RHEL. It takes \
             `debian/` on Debian only, after `deb/`.",
            "A gated half runs after the ungated half. The family `files/` is copied over the \
             shared `files/`, and the family `module.sh` is sourced after the shared `module.sh`.",
            "A family `repo` replaces the other copies. `debian/repo` wins over `deb/repo`, which \
             wins over the `repo` at the module root, and the build sources only the winner.",
            "If `supports` is declared and the directory names no supported family, then `tect` \
             refuses it. With no `supports` declaration, every family is supported.",
        ]),
    ],
    notes: &[
        "A module with no `<family>/` directory gates nothing. Its `module.sh` and `files/` run on \
         every family it supports; with no `supports` declaration, that is every family.",
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A link back to a parent would loop, and a module's `files/` could hold a
    /// file named module.kdl that is overlay content and not a module. A link
    /// to a module is how a user shares one module between two trees.
    #[test]
    fn the_walk_finds_nested_modules_and_passes_over_module_content() {
        let root = std::env::temp_dir().join(format!("tect-module-dirs-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        for dir in ["a/b", "a/files/c", "a/fedora/d", ".git/e", "group/f"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join(MODULE_FILE), "").unwrap();
        }
        std::fs::write(root.join("a").join(MODULE_FILE), "").unwrap();
        std::os::unix::fs::symlink(&root, root.join("a/b/loop")).unwrap();
        std::os::unix::fs::symlink(root.join("group/f"), root.join("linked")).unwrap();

        let found: Vec<String> = module_dirs(
            &root,
            |path| path.join(MODULE_FILE).is_file(),
            |name| name.starts_with('.'),
        )
        .iter()
        .map(|path| path.strip_prefix(&root).unwrap().display().to_string())
        .collect();
        assert_eq!(found, ["a", "a/b", "group/f", "linked"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The shared directory is taken before the family's own, so a `rhel/`
    /// file lands over the `rpm/` one it replaces.
    #[test]
    fn a_family_reads_the_shared_directory_first() {
        let root = std::env::temp_dir().join(format!("tect-family-dirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for name in ["rpm", "rhel", "fedora", "deb"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }

        assert_eq!(family_dirs(&root, "rhel"), ["rpm", "rhel"]);
        assert_eq!(family_dirs(&root, "fedora"), ["rpm", "fedora"]);
        assert_eq!(family_first(&root, "rhel"), ["rhel/", "rpm/", ""]);
        std::fs::remove_dir_all(root).unwrap();
    }
}

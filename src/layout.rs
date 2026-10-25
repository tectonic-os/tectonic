//! Where everything sits in a repository, stated once. The shape is the tool's
//! own and is not configurable: every repository looking alike is what lets a
//! stranger read one, and what lets a diagnostic name a path outright.

use std::path::{Path, PathBuf};

/// Vendored and authored components, one directory apiece.
pub const COMPONENTS: &str = "components";
/// Everything the tool writes and `verify` byte-compares. Tracked.
pub const GENERATED: &str = "generated";
/// Local exports, caches and scratch. Ignored, and nothing here is read back
/// as a declaration.
pub const OUT: &str = "out";
/// Repository key material: committed public halves and ignored private halves.
pub const KEYS: &str = "keys";
const PUBLIC_KEYS: &str = "public";
const PRIVATE_KEYS: &str = "private";

/// The install logic of a component, which the build sources in its layer.
pub const SCRIPT: &str = "component.sh";
/// A component's sourced repository setup, which the package step runs first.
pub const REPO_SCRIPT: &str = "repo.sh";
/// A component's sourced package work, which the package step runs after the
/// declared packages.
pub const PACKAGES_SCRIPT: &str = "packages.sh";
/// A component's hook over the completed image, which the finalize step sources.
pub const FINALIZE_SCRIPT: &str = "finalize.sh";
/// Containerfile lines that a component ships for the build to add verbatim.
pub const FRAGMENT: &str = "Containerfile.inc";

/// A component's overlay tree, staged into the image by the collector.
pub const OVERLAY: &str = "files";

/// Mandatory access control policy a component ships: which directory holds it,
/// what marks a file in there as policy, and the capability that says the
/// image has that MAC. A component may ship for more than one and is built for
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
    /// The policy sources one component ships, sorted so two runs emit the same
    /// script. A subdirectory is not a profile: AppArmor abstractions and
    /// tunables are includes, and a component placing one uses `files/`.
    pub fn files(&self, component_dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(component_dir.join(self.dir)) else {
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

/// Every directory name a component may gate files behind, and the families each
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

/// Where a file a component ships is read from, most specific first and the component
/// root last: what a lookup that picks one copy walks. The reverse of
/// `family_dirs`, whose order is what a layering emitter wants.
pub fn family_first(component_dir: &Path, family: &str) -> Vec<String> {
    family_dirs(component_dir, family)
        .into_iter()
        .rev()
        .map(|gated| format!("{gated}/"))
        .chain(std::iter::once(String::new()))
        .collect()
}

/// A file one component ships, as the path inside its directory that a build
/// reads it at: the family copy where there is one, and the ungated copy
/// otherwise. `None` where the component ships it nowhere.
pub fn shipped(component_dir: &Path, family: &str, file: &str) -> Option<String> {
    family_first(component_dir, family)
        .into_iter()
        .map(|at| format!("{at}{file}"))
        .find(|at| component_dir.join(at).is_file())
}

/// The ones this component ships for this family, in the order they are taken.
/// Additive: an ungated `component.sh` and `files/` at the component root still run
/// everywhere. Empty where the image declares no family.
pub fn family_dirs(component_dir: &Path, family: &str) -> Vec<&'static str> {
    FAMILY_DIRS
        .iter()
        .filter(|(_, taken_on)| taken_on.contains(&family))
        .map(|(name, _)| *name)
        .filter(|name| component_dir.join(name).is_dir())
        .collect()
}

/// Lists each copy of one script the component ships for this family, as a path
/// inside the component. The root copy runs first. The family copies follow from
/// the broadest to the most specific.
pub fn layered(on_disk: &Path, family: &str, file: &str) -> Vec<String> {
    std::iter::once(String::new())
        .chain(
            family_dirs(on_disk, family)
                .into_iter()
                .map(|gated| format!("{gated}/")),
        )
        .map(|at| format!("{at}{file}"))
        .filter(|at| on_disk.join(at).is_file())
        .collect()
}

/// Whether a directory inside a component is one that the component's own build
/// reads. The walk for nested components passes over it.
pub fn component_owned(name: &str) -> bool {
    name == OVERLAY
        || name == SELINUX.dir
        || name == APPARMOR.dir
        || FAMILY_DIRS.iter().any(|(dir, _)| *dir == name)
}

/// Every component directory below `tree`, sorted. `marks` decides whether a
/// directory is a component. The walk never enters a directory that `skip` names,
/// and inside a component it passes over what `component_owned` names, because a
/// component may hold components of its own. A link to a directory is read as a
/// component and never entered, so a link back to a parent cannot loop.
pub fn component_dirs(
    tree: &Path,
    marks: impl Fn(&Path) -> bool,
    skip: impl Fn(&str) -> bool,
) -> Vec<PathBuf> {
    walk_components(tree, false, marks, skip)
}

/// Walks as `component_dirs` does. `in_component` says whether `tree` is a component.
fn walk_components(
    tree: &Path,
    in_component: bool,
    marks: impl Fn(&Path) -> bool,
    skip: impl Fn(&str) -> bool,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // Each directory is paired with whether it is a component.
    let mut dirs = vec![(tree.to_path_buf(), in_component)];
    while let Some((dir, in_component)) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if !path.is_dir() || skip(&name) || (in_component && component_owned(&name)) {
                continue;
            }
            let component = marks(&path);
            if component {
                out.push(path.clone());
            }
            if !entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
                dirs.push((path, component));
            }
        }
    }
    out.sort();
    out
}

/// Answers every member of a component library below `tree`. A member is a
/// directory with a `manifest.kdl`. The walk passes over dot directories,
/// because a library read out of a working tree carries `.git`.
pub fn member_dirs(tree: &Path) -> Vec<PathBuf> {
    walk_components(tree, false, is_member, is_dot)
}

/// Answers every member that a component directory holds of its own, relative to
/// that directory. The copy and the hash of a component leave these out, because
/// each one is imported on its own.
pub fn held_components(dir: &Path) -> Vec<PathBuf> {
    walk_components(dir, true, is_member, is_dot)
        .into_iter()
        .filter_map(|path| path.strip_prefix(dir).ok().map(Path::to_path_buf))
        .collect()
}

fn is_member(path: &Path) -> bool {
    path.join(MANIFEST_FILE).is_file()
}

fn is_dot(name: &str) -> bool {
    name.starts_with('.')
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

/// Whether a directory under `components/` is a component. A component may ship no
/// manifest.kdl, so the files that a build reads at its root also mark it. A
/// family directory does not, because a group of components may carry a family's
/// name.
pub fn is_component(dir: &Path) -> bool {
    [
        MANIFEST_FILE,
        SCRIPT,
        REPO_SCRIPT,
        PACKAGES_SCRIPT,
        FINALIZE_SCRIPT,
        FRAGMENT,
    ]
    .iter()
    .any(|file| dir.join(file).is_file())
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
/// What a component declares about itself.
pub const MANIFEST_FILE: &str = "manifest.kdl";
/// Holds what `import component` records of each component it copied in. It sits at
/// the root of `components/`, so no component directory holds a file outside its own
/// hash.
pub const LOCK_FILE: &str = "components.lock.kdl";

/// Where the `tect` build stage copies the binary from, so every layer mounts
/// the release the repository is pinned to.
pub const MOUNTED: &str = "out/tect";
/// Holds each fetched library, unpacked. `import component` copies the chosen
/// member from here. `.gitignore` ignores `.cache/`.
pub const SOURCES_CACHE: &str = ".cache/sources";

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

pub fn components(root: &Path) -> PathBuf {
    root.join(COMPONENTS)
}

/// Locates the lock of every imported component.
pub fn lock(root: &Path) -> PathBuf {
    components(root).join(LOCK_FILE)
}

/// One component's directory, by its path relative to `components/`.
pub fn component(root: &Path, dir: impl AsRef<Path>) -> PathBuf {
    components(root).join(dir)
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

/// The manifest.kdl inside it, which is the file that declares it exists.
pub fn manifest(root: &Path, dir: impl AsRef<Path>) -> PathBuf {
    component(root, dir).join(MANIFEST_FILE)
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
/// library every component build mounts at `/ctx/lib`. It is a sibling of
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
        Place { path: &[COMPONENTS, "/"], writer: "the user, `tect create component` or `tect import component`", tracked: true,
            what: "The components of the repository, one directory each. See [Components](components.md)." },
        Place { path: &[COMPONENTS, "/", LOCK_FILE], writer: "`tect import component`, `tect fetch components` and `tect set component`", tracked: true,
            what: "Where each imported component came from, and what its content hashed to. See \
                [`components.lock.kdl`](lock.md)." },
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
            what: "The libraries that `tect import component` fetches and copies a component from. \
                `tect check`, `tect generate` and `tect build` read nothing here." },
        Place { path: &["disk_config/"], writer: "`tect create repo`, then the user", tracked: true,
            what: "The settings that bootc-image-builder reads when it builds a Fedora or RHEL disk \
                image. `disk.toml` sets the smallest size of `/`." },
        Place { path: &[".github/renovate.json5"], writer: "`tect generate`", tracked: true,
            what: "The Renovate settings, written from the `renovate` block in `repo.kdl`. Its custom \
                managers keep the `tect` release, the base images, each pin that declares \
                `renovate` and each image entry's tracked `version` current." },
        Place { path: &[".gitignore"], writer: "`tect create repo`", tracked: true,
            what: "Keeps `keys/private/`, `out/` and `.cache/` out of git." },
    ],
    lists: &[],
    notes: &[],
};

/// Where everything sits in one component directory.
#[rustfmt::skip]
pub const COMPONENT_DIR: Tree = Tree {
    root: "components/<component-name>/",
    intro: "A component is a directory under `components/`, at whatever depth groups it, and its path \
        names the component. The build reads the files below by convention, so a component can be as \
        small as one `component.sh`. Any one of `manifest.kdl`, `component.sh`, `repo.sh`, `packages.sh`, \
        `finalize.sh`, `Containerfile.inc` and `files/` marks a directory as a component. A component \
        can also hold other components. An image that lists a nested component also gets the nearest \
        component that holds it, and an image that lists the outer component gets none of the components \
        inside it. The walk that finds \
        components passes over the directories that the build of a component reads.",
    places: &[
        Place { path: &[MANIFEST_FILE], writer: "", tracked: true,
            what: "What the component declares about itself. A component with only a script and \
                `files/` can leave it out. A component fetched from another repository needs one. \
                See [`manifest.kdl`](component.md)." },
        Place { path: &[SCRIPT], writer: "", tracked: true,
            what: "The install logic. The build sources it in the layer of the component, after the \
                keys and before the policies and `files/`." },
        Place { path: &[PACKAGES_SCRIPT], writer: "", tracked: true,
            what: "Package work that the declarations cannot express. The package step sources it \
                after the declared packages, with the network that `packages` in `repo.kdl` \
                allows. It can call the family's `install_packages`." },
        Place { path: &[FINALIZE_SCRIPT], writer: "", tracked: true,
            what: "The finalize phase sources it, in resolved order." },
        Place { path: &[OVERLAY, "/"], writer: "", tracked: true,
            what: "An overlay that the build copies over `/`." },
        Place { path: &[REPO_SCRIPT], writer: "", tracked: true,
            what: "A package repository file. The build sources it first. On Fedora, if the \
                `REPO_ID` of `repo.sh` is already configured, then the build skips it." },
        Place { path: &[FRAGMENT], writer: "", tracked: true,
            what: "Containerfile lines that the build adds verbatim, for a need that the \
                fields cannot express. `fragment` in `manifest.kdl` places them." },
        Place { path: &[SELINUX.dir, "/*.te"], writer: "", tracked: true,
            what: "SELinux policy. If the image provides `selinux-policy`, then the build \
                compiles and installs it." },
        Place { path: &[APPARMOR.dir, "/"], writer: "", tracked: true,
            what: "AppArmor profiles. If the image provides `apparmor-policy`, then the build \
                validates them and places them in `/etc/apparmor.d`." },
        Place { path: &["<family>/"], writer: "", tracked: true,
            what: "The files of the component that apply to one base family." },
        Place { path: &["<collected file>"], writer: "", tracked: true,
            what: "A part of a file that another component collects. The build stages it for that \
                component." },
    ],
    lists: &[
        ("A policy directory follows these rules:", &[
            "A component that ships `selinux/` or `apparmor/` builds after the component that provides \
             that MAC, as if it declared `after`.",
            "A component can ship both policy directories. The build takes each directory only where \
             the image has that MAC. An image with no MAC installs no policy and needs no provider.",
            "A policy directory declares no requirement. A component that needs a MAC declares \
             `requires \"selinux-policy\"` or `requires \"apparmor-policy\"`, and `tect` refuses \
             it on an image without that MAC.",
        ]),
        ("A `<family>/` directory gates `component.sh`, `packages.sh`, `finalize.sh`, `files/`, \
          `repo.sh` and a collected file:", &[
            "The six directory names are `fedora/`, `rhel/`, `debian/`, `ubuntu/`, `rpm/` and \
             `deb/`. The build reads no other directory in a component.",
            "The build takes `deb/` on Debian and Ubuntu, and `rpm/` on Fedora and RHEL. It takes \
             `debian/` on Debian only, after `deb/`.",
            "A gated half runs after the ungated half. The family `files/` is copied over the \
             shared `files/`. The family `component.sh`, `packages.sh` and `finalize.sh` are each \
             sourced after the shared copy, from `deb/` or `rpm/` to the single family.",
            "A family `repo.sh` replaces the other copies. `debian/repo.sh` wins over `deb/repo.sh`, which \
             wins over the `repo.sh` at the component root, and the build sources only the winner.",
            "If `targets` is declared and the directory names no targeted family, then `tect` \
             refuses it. With no `targets` declaration, every family is targeted.",
        ]),
    ],
    notes: &[
        "A component with no `<family>/` directory gates nothing. Its `component.sh` and `files/` run on \
         every family it targets; with no `targets` declaration, that is every family.",
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_component_is_what_the_library_lists_inside_the_component() {
        let root = std::env::temp_dir().join(format!("tect-held-dirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["a", "a/b", "a/files/c", "a/group/files/d", "a/.x/e"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join(MANIFEST_FILE), "").unwrap();
        }
        let listed = member_dirs(&root);
        let held = held_components(&root.join("a"));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            listed,
            ["a", "a/b", "a/group/files/d"].map(|dir| root.join(dir))
        );
        assert_eq!(held, [PathBuf::from("b"), PathBuf::from("group/files/d")]);
    }

    /// A link back to a parent would loop, and a component's `files/` could hold a
    /// file named manifest.kdl that is overlay content and not a component. A link
    /// to a component is how a user shares one component between two trees.
    #[test]
    fn the_walk_finds_nested_components_and_passes_over_component_content() {
        let root = std::env::temp_dir().join(format!("tect-component-dirs-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        for dir in ["a/b", "a/files/c", "a/fedora/d", ".git/e", "group/f"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join(MANIFEST_FILE), "").unwrap();
        }
        std::fs::write(root.join("a").join(MANIFEST_FILE), "").unwrap();
        std::os::unix::fs::symlink(&root, root.join("a/b/loop")).unwrap();
        std::os::unix::fs::symlink(root.join("group/f"), root.join("linked")).unwrap();

        let found: Vec<String> = component_dirs(
            &root,
            |path| path.join(MANIFEST_FILE).is_file(),
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

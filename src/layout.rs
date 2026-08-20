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

/// A module's overlay tree, staged into the image by the collector.
pub const OVERLAY: &str = "files";

/// GitHub's path, not this repository's choice, which is why it is written
/// here rather than declared anywhere.
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
/// What `import module` writes beside a vendored module.kdl, never into it.
pub const RECORD_FILE: &str = "provenance.kdl";

/// Where the `tect` build stage copies the binary from, so every layer mounts
/// the release the repository is pinned to.
pub const MOUNTED: &str = "out/tect";
/// Where a fetched collection is unpacked. Under `out/`, which is ignored: the
/// copy that gets committed is the one under `modules/`.
pub const SOURCES_CACHE: &str = "out/sources";
/// What a fetched tree hashed to, kept out of the tree it describes so nothing
/// under `modules/` is tool-written state.
pub const STAMPS: &str = "out/remote-modules";

/// Whether a root file holds images. An allowlist rather than "everything but
/// repo.kdl", so a root `.kdl` that is neither is reported instead of parsed.
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

/// The module.kdl inside it, which is the file that declares it exists.
pub fn manifest(root: &Path, dir: impl AsRef<Path>) -> PathBuf {
    module(root, dir).join(MODULE_FILE)
}

pub fn generated(root: &Path) -> PathBuf {
    root.join(GENERATED)
}

pub fn out(root: &Path) -> PathBuf {
    root.join(OUT)
}

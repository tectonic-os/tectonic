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

/// Repo context, not an image: the file every image file is not, and the one
/// whose presence marks a root.
pub const REPO_FILE: &str = "repo.kdl";
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

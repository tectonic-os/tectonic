//! One walk of the repository's trees, for everything that asks what is on
//! disk rather than what an image enables.

use kdl::KdlDocument;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Default)]
pub struct Disk {
    /// Capability to every module declaring it, whether or not a list enables
    /// it, so an unsatisfied requirement can name what would satisfy it.
    pub providers: BTreeMap<String, Vec<String>>,
    /// Collected filename to the module that collects it, so a contribution
    /// whose consumer is not enabled can name what to enable.
    pub collectors: BTreeMap<String, String>,
    /// Module directory to every path its files/ overlay puts in the image.
    pub overlays: BTreeMap<String, Vec<String>>,
}

impl Disk {
    /// Every module directory on disk, whether or not an image lists it.
    pub fn modules(&self) -> impl Iterator<Item = &String> {
        self.overlays.keys()
    }

    pub fn scan(root: &Path) -> Self {
        let mut out = Disk::default();

        let modules = root.join("modules");
        let mut dirs = vec![modules.clone()];
        while let Some(dir) = dirs.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() || path.file_name().is_some_and(|n| n == "_template") {
                    continue;
                }
                let manifest = path.join("module.kdl");
                if !manifest.is_file() {
                    dirs.push(path);
                    continue;
                }
                let name = path
                    .strip_prefix(&modules)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                out.overlays
                    .insert(name.clone(), overlay_paths(&path.join("files")));

                let Ok(text) = std::fs::read_to_string(&manifest) else {
                    continue;
                };
                let Ok(doc) = text.parse::<KdlDocument>() else {
                    continue;
                };
                for node in doc.nodes() {
                    let args = || {
                        node.entries()
                            .iter()
                            .filter(|e| e.name().is_none())
                            .filter_map(|e| e.value().as_string())
                    };
                    match node.name().value() {
                        "provides" | "provides-file" => {
                            for cap in args() {
                                out.providers
                                    .entry(cap.to_string())
                                    .or_default()
                                    .push(name.clone());
                            }
                        }
                        "collects" => {
                            if let Some(file) = args().next() {
                                out.collectors.insert(file.to_string(), name.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // read_dir order is the filesystem's, and a help line lists these.
        for candidates in out.providers.values_mut() {
            candidates.sort();
        }
        out
    }
}

/// Every file in an overlay, as the absolute path it becomes in the image.
fn overlay_paths(overlay: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut dirs = vec![overlay.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = path.symlink_metadata() else {
                continue;
            };
            if meta.is_dir() {
                dirs.push(path);
            } else if let Ok(rel) = path.strip_prefix(overlay) {
                out.push(format!("/{}", rel.display()));
            }
        }
    }
    out
}

//! One walk of the repository's trees, for everything that asks what is on
//! disk.

use crate::diag::{Issues, Source};
use crate::layout;
use crate::model::component::Key;
use kdl::KdlDocument;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Default)]
pub struct Disk {
    /// Key kind to every component declaring it and what each declaration says,
    /// which is where `create key` learns everything but the kind.
    pub keys: BTreeMap<String, Vec<(String, Key)>>,
    /// Key kind to the component whose declaration of it the manifest refuses, and
    /// the refusals rendered. `create key` then names the value to fix
    /// instead of a component to import.
    pub refused_keys: BTreeMap<String, (String, String)>,
    /// Collected filename to the component that collects it, so a contribution
    /// whose consumer is not enabled can name what to enable.
    pub collectors: BTreeMap<String, String>,
    /// Component directory to every path its files/ overlay puts in the image.
    pub overlays: BTreeMap<String, Vec<String>>,
}

impl Disk {
    /// Every component directory on disk, whether or not an image lists it.
    pub fn components(&self) -> impl Iterator<Item = &String> {
        self.overlays.keys()
    }

    /// Whether the directory under `components/` is a component.
    pub fn is_component(&self, dir: &str) -> bool {
        self.overlays.contains_key(dir)
    }

    /// A component may ship only a file that another component collects, and the
    /// collected names come from the manifests. If any component collects a file,
    /// then a second walk counts that file as the mark of a component.
    pub fn scan(root: &Path) -> Self {
        let first = Self::walk(root, &[]);
        if first.collectors.is_empty() {
            return first;
        }
        let collected: Vec<String> = first.collectors.keys().cloned().collect();
        Self::walk(root, &collected)
    }

    fn walk(root: &Path, collected: &[String]) -> Self {
        let mut out = Self::default();

        let components = layout::components(root);
        let found = layout::component_dirs(
            &components,
            |path| {
                layout::is_component(path) || collected.iter().any(|file| path.join(file).is_file())
            },
            |name| name == "_template",
        );
        for path in found {
            let manifest = path.join(layout::MANIFEST_FILE);
            let name = path
                .strip_prefix(&components)
                .unwrap_or(&path)
                .display()
                .to_string();
            // A family overlay puts paths in the image too, so an override
            // one component gates and another does not is still two components
            // writing one path.
            let mut paths = overlay_paths(&path.join(layout::OVERLAY));
            for (gated, _) in layout::FAMILY_DIRS {
                paths.extend(overlay_paths(&path.join(gated).join(layout::OVERLAY)));
            }
            out.overlays.insert(name.clone(), paths);

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
                    "collects" => {
                        if let Some(file) = args().next() {
                            out.collectors.insert(file.to_string(), name.clone());
                        }
                    }
                    // A malformed one is reported where the manifest is
                    // checked, not here.
                    "key" => {
                        let src = Source::new(manifest.display().to_string(), text.clone());
                        let mut refused = Issues::default();
                        let Some(key) =
                            crate::parse::component::parse_key(node, &src, &mut refused)
                        else {
                            out.refuse_key(args().next(), &name, &doc, &src, refused);
                            continue;
                        };
                        out.keys
                            .entry(key.kind.clone())
                            .or_default()
                            .push((name.clone(), key));
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

impl Disk {
    /// Keeps the first component that declares `kind` with a refused key. A key
    /// with no kind has nothing `create key` can name.
    fn refuse_key(
        &mut self,
        kind: Option<&str>,
        component: &str,
        doc: &KdlDocument,
        src: &Source,
        mut refused: Issues,
    ) {
        let Some(kind) = kind else {
            return;
        };
        crate::parse::schema::check_doc(
            doc,
            &crate::parse::component::COMPONENT,
            src,
            &mut refused,
        );
        self.refused_keys
            .entry(kind.to_string())
            .or_insert_with(|| (component.to_string(), refused.plain()));
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

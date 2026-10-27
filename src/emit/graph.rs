//! What one image's components promise each other, as a diagram and as data.

use crate::emit::{Part, Table};
use crate::layout;
use crate::model::component::Component;
use crate::model::image::Image;
use common::json::Json;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

/// The one node in the graph that is not a component.
const BASE: &str = "base";

/// One capability name or contract path, and everything the image says about
/// it. Node indices, so a rendering never looks a name up twice.
#[derive(Default)]
struct Cap {
    file: bool,
    build_only: bool,
    /// None when nothing enabled provides it. `check` refuses that for a
    /// `requires` and accepts it for an `after`.
    provider: Option<usize>,
    required_by: Vec<usize>,
    after: Vec<usize>,
}

struct Node {
    /// Mermaid identity, which cannot hold a slash.
    id: String,
    /// The component path, or `base`.
    name: String,
    /// What the diagram prints, which adds the flavour gate.
    label: String,
}

/// One walk over a resolved image: the components in build order, and every
/// capability sorted by name.
pub struct Graph<'a> {
    image: &'a Image,
    nodes: Vec<Node>,
    caps: BTreeMap<&'a str, Cap>,
    overrides: Vec<(&'a str, &'a str)>,
    /// Holds each imported component and the component that imports it.
    imports: Vec<(&'a str, &'a str)>,
    /// Components the base covers entirely, and what each of them declares. Not
    /// nodes: nothing builds them, so they have no edges.
    suppressed: Vec<(&'a str, Vec<&'a str>)>,
}

pub fn of(image: &Image) -> Graph<'_> {
    let mut graph = Graph {
        image,
        nodes: Vec::new(),
        caps: BTreeMap::new(),
        overrides: Vec::new(),
        imports: Vec::new(),
        suppressed: Vec::new(),
    };

    if let Some(base) = &image.base {
        graph.nodes.push(Node {
            id: BASE.to_string(),
            name: BASE.to_string(),
            label: base.image.clone(),
        });
        for decl in base.shipped() {
            graph.cap(&decl.name).provider.get_or_insert(0);
        }
    }

    for component in image.components() {
        let node = graph.node(component);
        for decl in &component.provides {
            let located = component.files.iter().find(|f| f.name == decl.name);
            let cap = graph.cap(&decl.name);
            if cap.provider.is_none() {
                cap.provider = Some(node);
                cap.file = located.is_some();
                cap.build_only = located.is_some_and(|f| f.build_only);
            }
        }
        for decl in &component.requires {
            graph.cap(&decl.name).required_by.push(node);
        }
        // The policy a component ships orders it the same way an `after` does,
        // so the graph draws that edge too: an ordering the build enforces and
        // the graph does not show is an ordering nothing can be read off.
        let after = component
            .after
            .iter()
            .map(|decl| decl.name.as_str())
            .chain(component.policies.iter().copied());
        for name in after {
            let held = &mut graph.cap(name).after;
            if !held.contains(&node) {
                held.push(node);
            }
        }
        for decl in &component.overrides {
            graph.overrides.push((&component.path, &decl.name));
        }
    }

    // A requirement that a descendant name or a component's own path meets has no
    // `provides` row of its own, so it takes the first node that answers it.
    let unmet: Vec<&str> = graph
        .caps
        .iter()
        .filter(|(_, cap)| cap.provider.is_none())
        .map(|(name, _)| *name)
        .collect();
    for name in unmet {
        let provider = match image.base_answers(name) {
            true => Some(0),
            false => image.components().find(|m| m.answers(name)).and_then(|m| {
                graph
                    .nodes
                    .iter()
                    .position(|node| node.id != BASE && node.name == m.path)
            }),
        };
        graph.cap(name).provider = provider;
    }

    // Every importer draws its edge. An entry records only its first importer.
    let importers = image
        .base
        .iter()
        .map(|base| (base.path.as_str(), &base.imports))
        .chain(image.components().map(|m| (m.path.as_str(), &m.imports)));
    for (by, imports) in importers {
        for import in imports {
            let edge = (import.path.as_str(), by);
            let built = image.entries.iter().any(|entry| entry.path == import.path);
            if built && !graph.imports.contains(&edge) {
                graph.imports.push(edge);
            }
        }
    }

    for component in image.suppressed.iter().filter_map(|e| e.component.as_ref()) {
        graph.suppressed.push((
            &component.path,
            component
                .provides
                .iter()
                .map(|decl| decl.name.as_str())
                .collect(),
        ));
    }

    graph
}

/// Both renderings of one image's graph, for `generate` to write.
pub fn files(image: &Image) -> Vec<(PathBuf, String)> {
    let graph = of(image);
    vec![
        (path(image, "md"), graph.markdown()),
        (path(image, "json"), graph.json().render()),
    ]
}

pub fn path(image: &Image, format: &str) -> PathBuf {
    layout::generated_image(&image.id).join(format!("graph.{format}"))
}

impl<'a> Graph<'a> {
    fn cap(&mut self, name: &'a str) -> &mut Cap {
        self.caps.entry(name).or_default()
    }

    /// A component listed both ungated and under a flavour is two entries and one
    /// node: what it promises does not change with the gate.
    fn node(&mut self, component: &'a Component) -> usize {
        if let Some(index) = self
            .nodes
            .iter()
            .position(|n| n.id != BASE && n.name == component.path)
        {
            return index;
        }
        self.nodes.push(Node {
            id: format!(
                "m{}",
                self.nodes.len() - usize::from(self.image.base.is_some())
            ),
            name: component.path.clone(),
            label: match &component.flavour {
                Some(flavour) => format!("{} [{flavour}]", component.path),
                None => component.path.clone(),
            },
        });
        self.nodes.len() - 1
    }

    /// Every capability between one pair of nodes is one arrow, since a wall of
    /// parallel edges says nothing the label cannot. Keyed by node index, so
    /// the arrows come out grouped by provider on any machine.
    fn edges(&self) -> BTreeMap<(usize, usize, bool), Vec<&'a str>> {
        let mut edges: BTreeMap<(usize, usize, bool), Vec<&str>> = BTreeMap::new();
        for (name, cap) in &self.caps {
            let Some(provider) = cap.provider else {
                continue;
            };
            let hard = cap.required_by.iter().map(|to| (to, false));
            for (&to, soft) in hard.chain(cap.after.iter().map(|to| (to, true))) {
                if to != provider {
                    edges.entry((provider, to, soft)).or_default().push(name);
                }
            }
        }
        edges
    }

    pub fn json(&self) -> Json {
        Json::object([
            ("image", Json::string(&self.image.id)),
            (
                "capabilities",
                Json::array(self.caps.iter().map(|(name, cap)| {
                    Json::object([
                        ("name", Json::string(*name)),
                        (
                            "kind",
                            Json::string(if cap.file { "file" } else { "capability" }),
                        ),
                        ("build_only", Json::Bool(cap.build_only)),
                        (
                            "provider",
                            Json::optional(cap.provider.map(|node| self.nodes[node].name.as_str())),
                        ),
                        ("required_by", self.names(&cap.required_by)),
                        ("after", self.names(&cap.after)),
                    ])
                })),
            ),
            (
                "overrides",
                Json::array(self.overrides.iter().map(|(component, path)| {
                    Json::object([
                        ("component", Json::string(*component)),
                        ("path", Json::string(*path)),
                    ])
                })),
            ),
            (
                "imports",
                Json::array(self.imports.iter().map(|(component, by)| {
                    Json::object([
                        ("component", Json::string(*component)),
                        ("by", Json::string(*by)),
                    ])
                })),
            ),
            (
                "suppressed",
                Json::array(self.suppressed.iter().map(|(component, provides)| {
                    Json::object([
                        ("component", Json::string(*component)),
                        ("provides", Json::strings(provides.iter().copied())),
                    ])
                })),
            ),
        ])
    }

    fn names(&self, nodes: &[usize]) -> Json {
        Json::strings(nodes.iter().map(|&node| self.nodes[node].name.as_str()))
    }

    /// The tables that `markdown()` writes, as data and without the mermaid
    /// block, which is source for a renderer a terminal does not have. Empty
    /// tables are left out, the way the markdown leaves out their headings.
    pub fn parts(&self) -> Vec<Part> {
        let mut tables = Vec::new();
        if !self.caps.is_empty() {
            tables.push(Part::Table(Table {
                title: "Capabilities".to_string(),
                header: &["Name", "Kind", "Provided by", "Required by", "After"],
                rows: self
                    .caps
                    .iter()
                    .map(|(name, cap)| {
                        (
                            vec![
                                (*name).to_string(),
                                kind(cap).to_string(),
                                cap.provider
                                    .map(|node| self.nodes[node].name.clone())
                                    .unwrap_or_default(),
                                self.plain(&cap.required_by),
                                self.plain(&cap.after),
                            ],
                            cap.provider.is_none() && !cap.required_by.is_empty(),
                        )
                    })
                    .collect(),
            }));
        }
        if !self.imports.is_empty() {
            tables.push(Part::Table(Table {
                title: "Imports".to_string(),
                header: &["Component", "Imported by"],
                rows: self
                    .imports
                    .iter()
                    .map(|(component, by)| {
                        (vec![(*component).to_string(), (*by).to_string()], false)
                    })
                    .collect(),
            }));
        }
        if !self.suppressed.is_empty() {
            tables.push(Part::Table(Table {
                title: "Suppressed".to_string(),
                header: &["Component", "Provides"],
                rows: self
                    .suppressed
                    .iter()
                    .map(|(component, provides)| {
                        (vec![(*component).to_string(), provides.join(", ")], false)
                    })
                    .collect(),
            }));
        }
        if !self.overrides.is_empty() {
            tables.push(Part::Table(Table {
                title: "Overrides".to_string(),
                header: &["Component", "Path"],
                rows: self
                    .overrides
                    .iter()
                    .map(|(component, path)| {
                        (vec![(*component).to_string(), (*path).to_string()], false)
                    })
                    .collect(),
            }));
        }
        tables
    }

    pub fn markdown(&self) -> String {
        let imports = match self.imports.is_empty() {
            true => "",
            false => "A thick arrow points from a component to what it imports.\n",
        };
        let mut out = format!(
            "# {} capability graph\n\n\
             GENERATED FILE, do not edit.\n\n\
             An arrow points from a provider to what needs it, dotted for `after`,\n\
             which orders the build without requiring anything. Layers build left to\n\
             right.\n{imports}\n\
             ```mermaid\ngraph LR\n",
            self.image.name
        );
        for node in &self.nodes {
            let _ = writeln!(out, "    {}[\"{}\"]", node.id, node.label);
        }
        for ((from, to, soft), caps) in self.edges() {
            let _ = writeln!(
                out,
                "    {} {}|\"{}\"| {}",
                self.nodes[from].id,
                if soft { "-.->" } else { "-->" },
                caps.join(", "),
                self.nodes[to].id
            );
        }
        for (component, by) in &self.imports {
            let (Some(from), Some(to)) = (self.at(by), self.at(component)) else {
                continue;
            };
            let _ = writeln!(
                out,
                "    {} ==>|\"imports\"| {}",
                self.nodes[from].id, self.nodes[to].id
            );
        }
        out.push_str("```\n");

        if !self.caps.is_empty() {
            out.push_str(
                "\n## Capabilities\n\n\
                 | Name | Kind | Provided by | Required by | After |\n\
                 |---|---|---|---|---|\n",
            );
            for (name, cap) in &self.caps {
                let _ = writeln!(
                    out,
                    "| `{name}` | {} | {} | {} | {} |",
                    kind(cap),
                    cap.provider
                        .map(|node| code(&self.nodes[node].name))
                        .unwrap_or_default(),
                    self.list(&cap.required_by),
                    self.list(&cap.after),
                );
            }
        }

        if !self.suppressed.is_empty() {
            out.push_str(
                "\n## Suppressed\n\n\
                 The base already provides everything these declare, so nothing builds\n\
                 them.\n\n\
                 | Component | Provides |\n|---|---|\n",
            );
            for (component, provides) in &self.suppressed {
                let provides: Vec<String> = provides.iter().map(|name| code(name)).collect();
                let _ = writeln!(out, "| {} | {} |", code(component), provides.join(", "));
            }
        }

        if !self.imports.is_empty() {
            out.push_str("\n## Imports\n\n| Component | Imported by |\n|---|---|\n");
            for (component, by) in &self.imports {
                let _ = writeln!(out, "| {} | {} |", code(component), code(by));
            }
        }

        if !self.overrides.is_empty() {
            out.push_str("\n## Overrides\n\n| Component | Path |\n|---|---|\n");
            for (component, path) in &self.overrides {
                let _ = writeln!(out, "| {} | {} |", code(component), code(path));
            }
        }

        out
    }

    /// Finds the node of a component path. The base's path is the `base` node.
    fn at(&self, path: &str) -> Option<usize> {
        if self
            .image
            .base
            .as_ref()
            .is_some_and(|base| base.path == path)
        {
            return Some(0);
        }
        self.nodes
            .iter()
            .position(|node| node.id != BASE && node.name == path)
    }

    fn plain(&self, nodes: &[usize]) -> String {
        nodes
            .iter()
            .map(|&node| self.nodes[node].name.as_str())
            .collect::<Vec<&str>>()
            .join(", ")
    }

    fn list(&self, nodes: &[usize]) -> String {
        nodes
            .iter()
            .map(|&node| code(&self.nodes[node].name))
            .collect::<Vec<String>>()
            .join(", ")
    }
}

fn kind(cap: &Cap) -> &'static str {
    match (cap.file, cap.build_only) {
        (false, _) => "capability",
        (true, false) => "file",
        (true, true) => "file, build only",
    }
}

fn code(text: &str) -> String {
    format!("`{text}`")
}

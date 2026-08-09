//! What one image's modules promise each other, as a diagram and as data.

use crate::emit::json::Json;
use crate::model::image::Image;
use crate::model::module::Module;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

/// The one node in the graph that is not a module.
const BASE: &str = "base";

/// One capability name or contract path, and everything the image says about
/// it. Node indices, so a rendering never looks a name up twice.
#[derive(Default)]
struct Cap {
    file: bool,
    build_only: bool,
    /// None when nothing enabled provides it, which is already a diagnostic.
    provider: Option<usize>,
    required_by: Vec<usize>,
    after: Vec<usize>,
}

struct Node {
    /// Mermaid identity, which cannot hold a slash.
    id: String,
    /// The module path, or `base`.
    name: String,
    /// What the diagram prints, which adds the flavour gate.
    label: String,
}

/// One walk over a resolved image: the modules in build order, and every
/// capability sorted by name.
pub struct Graph<'a> {
    image: &'a Image,
    nodes: Vec<Node>,
    caps: BTreeMap<&'a str, Cap>,
    overrides: Vec<(&'a str, &'a str)>,
}

pub fn of(image: &Image) -> Graph<'_> {
    let mut graph = Graph {
        image,
        nodes: Vec::new(),
        caps: BTreeMap::new(),
        overrides: Vec::new(),
    };

    if let Some(base) = &image.base {
        graph.nodes.push(Node {
            id: BASE.to_string(),
            name: BASE.to_string(),
            label: base.image.clone(),
        });
        for (decls, file) in [(&base.provides, false), (&base.provides_files, true)] {
            for decl in decls {
                graph.cap(&decl.name, file).provider.get_or_insert(0);
            }
        }
    }

    for module in image.modules() {
        let node = graph.node(module);
        for (decls, file) in [(&module.provides, false), (&module.provides_files, true)] {
            for decl in decls {
                let build_only = module.provides_files_build_only.contains(&decl.name);
                let cap = graph.cap(&decl.name, file);
                if cap.provider.is_none() {
                    cap.provider = Some(node);
                    cap.build_only = build_only;
                }
            }
        }
        for (decls, file) in [(&module.requires, false), (&module.requires_files, true)] {
            for decl in decls {
                graph.cap(&decl.name, file).required_by.push(node);
            }
        }
        for decl in &module.after {
            graph.cap(&decl.name, false).after.push(node);
        }
        for decl in &module.overrides {
            graph.overrides.push((&module.path, &decl.name));
        }
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
    PathBuf::from("generated").join(format!("{}.graph.{format}", image.id))
}

impl<'a> Graph<'a> {
    fn cap(&mut self, name: &'a str, file: bool) -> &mut Cap {
        self.caps.entry(name).or_insert(Cap {
            file,
            ..Cap::default()
        })
    }

    /// A module listed both ungated and under a flavour is two entries and one
    /// node: what it promises does not change with the gate.
    fn node(&mut self, module: &'a Module) -> usize {
        if let Some(index) = self
            .nodes
            .iter()
            .position(|n| n.id != BASE && n.name == module.path)
        {
            return index;
        }
        self.nodes.push(Node {
            id: format!(
                "m{}",
                self.nodes.len() - usize::from(self.image.base.is_some())
            ),
            name: module.path.clone(),
            label: match &module.flavour {
                Some(flavour) => format!("{} [{flavour}]", module.path),
                None => module.path.clone(),
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
                Json::array(self.overrides.iter().map(|(module, path)| {
                    Json::object([
                        ("module", Json::string(*module)),
                        ("path", Json::string(*path)),
                    ])
                })),
            ),
        ])
    }

    fn names(&self, nodes: &[usize]) -> Json {
        Json::strings(nodes.iter().map(|&node| self.nodes[node].name.as_str()))
    }

    pub fn markdown(&self) -> String {
        let mut out = format!(
            "# {} capability graph\n\n\
             GENERATED FILE, do not edit.\n\n\
             An arrow points from a provider to what needs it, dotted for `after`,\n\
             which orders the build without requiring anything. Layers build left to\n\
             right.\n\n\
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
                    match (cap.file, cap.build_only) {
                        (false, _) => "capability",
                        (true, false) => "file",
                        (true, true) => "file, build only",
                    },
                    cap.provider
                        .map(|node| code(&self.nodes[node].name))
                        .unwrap_or_default(),
                    self.list(&cap.required_by),
                    self.list(&cap.after),
                );
            }
        }

        if !self.overrides.is_empty() {
            out.push_str("\n## Overrides\n\n| Module | Path |\n|---|---|\n");
            for (module, path) in &self.overrides {
                let _ = writeln!(out, "| {} | {} |", code(module), code(path));
            }
        }

        out
    }

    fn list(&self, nodes: &[usize]) -> String {
        nodes
            .iter()
            .map(|&node| code(&self.nodes[node].name))
            .collect::<Vec<String>>()
            .join(", ")
    }
}

fn code(text: &str) -> String {
    format!("`{text}`")
}

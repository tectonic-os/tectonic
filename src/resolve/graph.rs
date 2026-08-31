//! The capability graph, and the fragments nothing generated can agree with.

use crate::diag::{Issue, Issues};
use crate::layout;
use crate::model::image::{Entry, Image};
use crate::model::module::{Decl, Module};
use crate::provider::Index;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// A module the base covers entirely provisions nothing, so it comes off the
/// entry list before anything orders, checks or emits it. Runs first: the base
/// wins as provider everywhere, and building what it already ships is the
/// duplicate layer `base { provides }` exists to prevent.
pub fn suppress(image: &mut Image) {
    let base: BTreeSet<String> = image
        .base
        .iter()
        .flat_map(|b| b.provides.iter().chain(b.provides_files.iter()))
        .map(|decl| decl.name.clone())
        .collect();
    if base.is_empty() {
        return;
    }
    let (suppressed, kept): (Vec<Entry>, Vec<Entry>) = image
        .entries
        .drain(..)
        .partition(|entry| covered(entry, &base));
    image.entries = kept;
    image.suppressed = suppressed;
}

/// Provides something, and the base provides all of it.
fn covered(entry: &Entry, base: &BTreeSet<String>) -> bool {
    let Some(module) = &entry.module else {
        return false;
    };
    let mut decls = provided(module).peekable();
    decls.peek().is_some() && decls.all(|decl| base.contains(&decl.name))
}

/// Capabilities and contract paths together: a base covers both the same way.
fn provided(module: &Module) -> impl Iterator<Item = &Decl> {
    module.provides.iter().chain(module.provides_files.iter())
}

fn names(decls: &[&Decl]) -> String {
    decls
        .iter()
        .map(|decl| format!("`{}`", decl.name))
        .collect::<Vec<String>>()
        .join(", ")
}

/// The help a requirement nothing anywhere provides gets: what the index
/// searched, and — the point of it — what it did not. Resolution never
/// fetches, so in a fresh clone every declared collection is unread, and
/// concluding that nobody provides the capability would assert a search that
/// never ran.
fn nowhere(index: &Index, capability: &str, src: &str) -> String {
    let (searched, unsearched) = match (index.unread().is_empty(), index.sourced()) {
        (true, true) => ("the repository or its collections", String::new()),
        (true, false) => (
            "the repository",
            ". repo.kdl declares no `sources`, so there is no collection to import one from"
                .to_string(),
        ),
        (false, _) => ("the repository", format!(". {}", index.unsearched())),
    };
    format!(
        "no module in {searched} declares `provides {capability:?}`, and neither does the `base` node in {src}{unsearched}"
    )
}

/// Single pass over the resolved graph.
pub fn check_graph(image: &Image, root: &Path, index: &Index, issues: &mut Issues) {
    let mut offered: BTreeMap<&str, Vec<&Module>> = BTreeMap::new();
    for module in image.modules() {
        for decl in module.provides.iter().chain(module.provides_files.iter()) {
            offered.entry(decl.name.as_str()).or_default().push(module);
        }
    }

    let base_caps: BTreeMap<&str, &crate::model::image::Decl> = image
        .base
        .iter()
        .flat_map(|b| b.provides.iter().chain(b.provides_files.iter()))
        .map(|decl| (decl.name.as_str(), decl))
        .collect();

    let mut helpers: BTreeMap<&str, &Module> = BTreeMap::new();
    for module in image.modules() {
        for helper in &module.helpers {
            let name = Path::new(&helper.name)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if let Some(first) = helpers.insert(name, module) {
                issues.push(
                    Issue::new(
                        format!("two enabled modules declare helper `{name}`"),
                        &module.src,
                    )
                    .at(helper.span, "mounted to the same path")
                    .help(format!(
                        "also declared by `{}`; helper basenames must be unique across an image",
                        first.path
                    )),
                );
            }
        }
    }

    // Anything the base covers entirely is already suppressed, so what reaches
    // here is a module the base covers in part: its layer still builds, and
    // would provision what the base ships a second time.
    for module in image.modules() {
        let (covered, rest): (Vec<&Decl>, Vec<&Decl>) =
            provided(module).partition(|decl| base_caps.contains_key(decl.name.as_str()));
        let (Some(first), Some(_)) = (covered.first(), rest.first()) else {
            continue;
        };
        issues.push(
            Issue::new(
                format!(
                    "`{}` provides {}, which the base image already provides",
                    module.path,
                    names(&covered)
                ),
                &module.src,
            )
            .at(first.span, "already provided by the base")
            .help(format!(
                "a module the base covers entirely is suppressed instead. This one also provides {}, so its layer still builds and would provision {} a second time: split it, or drop the declaration",
                names(&rest),
                names(&covered)
            )),
        );
    }

    let base_family = image
        .base
        .as_ref()
        .map(|b| b.family.as_str())
        .filter(|f| !f.is_empty());
    if let Some(base_family) = base_family {
        let needs_adapter = image.modules().any(|module| {
            (base_family == "fedora" && !module.coprs.is_empty())
                || module
                    .packages
                    .iter()
                    .chain(&module.groups)
                    .any(|group| group.family == base_family)
        });
        let has_adapter = offered.get("build-environment").is_some_and(|providers| {
            providers
                .iter()
                .any(|module| module.supports.iter().any(|family| family == base_family))
        });
        if needs_adapter && !has_adapter {
            issues.push(
                Issue::new(
                    format!(
                        "image `{}` installs packages for `{base_family}`, but no enabled module provides `build-environment` for that family",
                        image.id
                    ),
                    &image.src,
                )
                .at(image.span, "missing family adapter"),
            );
        }
    }
    for module in image.modules() {
        let Some(base_family) = base_family else {
            break;
        };
        if !module.supports.iter().any(|f| f == base_family) {
            let supported = module.supports.join(", ");
            issues.push(
                Issue::new(
                    format!(
                        "`{}` does not support the `{base_family}` base family",
                        module.path
                    ),
                    &module.src,
                )
                .help(if supported.is_empty() {
                    "add `supports \"fedora\"` to the manifest".to_string()
                } else {
                    format!("it declares support for: {supported}")
                }),
            );
        }
    }

    for (capability, providers) in &offered {
        if providers.len() > 1 {
            let names: Vec<&str> = providers.iter().map(|m| m.path.as_str()).collect();
            let first = providers[0];
            issues.push(
                Issue::new(
                    format!("`{capability}` is provided by more than one enabled module"),
                    &first.src,
                )
                .at(
                    first.provides.iter().chain(first.provides_files.iter())
                        .find(|d| d.name == **capability)
                        .map(|d| d.span)
                        .unwrap_or_default(),
                    "also provided elsewhere",
                )
                .help(format!(
                    "provided by: {}. Enable one provider, so that what satisfies a requirement is never ambiguous",
                    names.join(", ")
                )),
            );
        }
    }

    const MAC_POLICY: &str = "mac-policy";
    for module in image.modules() {
        let dir = layout::module(root, &module.dir);
        let has_policy = std::fs::read_dir(dir.join("selinux"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| e.path().extension().is_some_and(|ext| ext == "te"));
        if !has_policy || module.requires.iter().any(|d| d.name == MAC_POLICY) {
            continue;
        }
        issues.push(
            Issue::new(
                format!(
                    "`{}` ships SELinux policy without requiring `{MAC_POLICY}`",
                    module.path
                ),
                &module.src,
            )
            .help(format!(
                "add `requires \"{MAC_POLICY}\"`; the generated build script compiles selinux/*.te against the base image's policy store"
            )),
        );
    }

    for module in image.modules() {
        let hard = module
            .requires
            .iter()
            .map(|d| (d, "requires"))
            .chain(module.requires_files.iter().map(|d| (d, "requires-file")));

        for (decl, kind) in hard {
            if base_caps.contains_key(decl.name.as_str()) {
                continue;
            }

            let Some(providers) = offered.get(decl.name.as_str()) else {
                // The help names what could satisfy it, and a provider for
                // another family could not. Falling back to every provider
                // keeps a diagnostic that names something over one that names
                // nothing when no module supports this family at all.
                let family = image.base.as_ref().map_or("", |base| base.family.as_str());
                let fits = index.fitting(&decl.name, family);
                let candidates = match fits.is_empty() {
                    true => index.of(&decl.name),
                    false => fits,
                };
                let named: Vec<String> = candidates.iter().map(|p| p.qualified()).collect();
                let help = match candidates.iter().find(|p| p.here) {
                    Some(_) => format!(
                        "{} would satisfy it; add it to this image. Nothing is included automatically, so the list stays the complete statement of what is in the image",
                        named.join(" or ")
                    ),
                    None => match candidates.first() {
                        Some(first) => format!(
                            "{} would satisfy it; `tect import module {}` brings it in and lists it",
                            named.join(" or "),
                            first.qualified()
                        ),
                        None => nowhere(index, &decl.name, &image.src.name()),
                    },
                };
                issues.push(
                    Issue::new(
                        format!(
                            "`{}` {kind} `{}`, which nothing enabled provides",
                            module.path, decl.name
                        ),
                        &module.src,
                    )
                    .at(decl.span, "unsatisfied")
                    .help(help),
                );
                continue;
            };

            if let Some(provider) = providers.first() {
                if let Some(provider_flavour) = &provider.flavour {
                    if module.flavour.as_ref() != Some(provider_flavour) {
                        issues.push(
                            Issue::new(
                                format!(
                                    "`{}` {kind} `{}`, which only `{}` provides and only on the `{provider_flavour}` flavour",
                                    module.path, decl.name, provider.path
                                ),
                                &module.src,
                            )
                            .at(decl.span, "unsatisfied on every other target")
                            .help("either gate this module to the same flavour, or move the provider out of the flavour block"),
                        );
                    }
                }
            }
        }
    }

    // Nothing else validates `after`, so a dangling one is said here.
    for module in image.modules() {
        for decl in &module.after {
            if base_caps.contains_key(decl.name.as_str()) {
                continue;
            }
            if let Some(providers) = offered.get(decl.name.as_str()) {
                if let Some(provider) = providers.first() {
                    if let Some(provider_flavour) = &provider.flavour {
                        if module.flavour.as_ref() != Some(provider_flavour) {
                            issues.push(
                                Issue::new(
                                    format!(
                                        "`{}` builds after `{}`, which only `{}` provides and only on the `{provider_flavour}` flavour",
                                        module.path, decl.name, provider.path
                                    ),
                                    &module.src,
                                )
                                .at(decl.span, "ineffective on every other target")
                                .help("either gate this module to the same flavour, or move the provider out of the flavour block"),
                            );
                        }
                    }
                }
                continue;
            }
            issues.push(
                Issue::new(
                    format!(
                        "`{}` builds after `{}`, which nothing enabled provides",
                        module.path, decl.name
                    ),
                    &module.src,
                )
                .at(decl.span, "nothing to order after")
                .help("an `after` orders the build without requiring anything; name a capability the base or an enabled module provides, or make it a `requires` so the missing edge is a requirement rather than an ordering"),
            );
        }
    }
}

/// A fragment is inlined verbatim, so nothing the generator does can make it
/// agree with the entry that carries it. Walks the entries in build order,
/// which is where `ARG FLAVOUR` lands: directly above the first gated module.
pub fn check_fragments(image: &Image, issues: &mut Issues) {
    let mut gated = false;
    for entry in &image.entries {
        let Some(module) = &entry.module else {
            continue;
        };
        gated |= entry.flavour.is_some();
        let Some(body) = &module.fragment else {
            continue;
        };
        let path = &entry.path;

        if !gated && (body.contains("${FLAVOUR}") || body.contains("$FLAVOUR")) {
            issues.push(
                Issue::new(
                    format!("`{path}` expands FLAVOUR above the flavour gate"),
                    &image.src,
                )
                .at(entry.span, "listed above the first flavour-gated module")
                .help("ARG FLAVOUR is declared directly above the first gated entry, so a fragment before it would expand to an empty string"),
            );
        }

        let runs = body.lines().any(|l| l.trim_start().starts_with("RUN "));
        let Some(flavour) = entry.flavour.as_ref().filter(|_| runs) else {
            continue;
        };
        let declared = body
            .split("FLAVOUR_GATE=")
            .nth(1)
            .map(|rest| rest.split_whitespace().next().unwrap_or_default());
        match declared {
            Some(d) if d == flavour => {}
            Some(d) => issues.push(
                Issue::new(
                    format!("`{path}` is listed under `{flavour}` but its fragment gates on `{d}`"),
                    &image.src,
                )
                .at(entry.span, "listed here"),
            ),
            None => issues.push(
                Issue::new(
                    format!(
                        "`{path}` is listed under `{flavour}` but its fragment sets no FLAVOUR_GATE"
                    ),
                    &image.src,
                )
                .at(entry.span, "the flavour gate would be silently ignored")
                .help(
                    "a fragment is emitted unconditionally, so anything it runs has to carry the gate itself",
                ),
            ),
        }
    }
}

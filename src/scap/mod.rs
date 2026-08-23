//! What the modules claimed, what the scan measured, and what stopped passing
//! since the last one. The claims come off the resolved plan; the mapping from
//! a benchmark number to a rule, and the rule to a result, come off the two
//! XML documents a scan run produces.

mod xml;

use crate::diag::{Issue, Issues, Source, Span};
use crate::emit::json::Json;
use crate::emit::plan::of_target;
use crate::model::image::{Entry, Image, List, NO_FLAVOUR};
use crate::resolve::overlay;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Where a runner installs the SSG content.
const CONTENT: &str = "/usr/share/xml/scap/ssg/content";

#[derive(Default)]
pub struct Options {
    pub target: Option<String>,
    pub datastream: Option<PathBuf>,
    pub baseline: Option<PathBuf>,
}

/// What the scan came back with; the report is on stdout either way.
pub enum Verdict {
    Clean,
    /// Findings, in a repository that enforces. Every one of them was printed.
    Wrong,
}

/// Prints the datastream one target is measured with, and nothing at all when
/// the image declares no `conforms`.
pub fn content(root: &Path, named: Option<&str>) -> Result<Verdict, String> {
    let Some((list, _)) = open(root, named) else {
        return Ok(Verdict::Wrong);
    };
    println!("{}", datastream(&list, named)?);
    Ok(Verdict::Clean)
}

/// Every manifest read and every image resolved, or nothing where the
/// repository is wrong and has been told so. A claim lives in a module, so
/// neither command here can work off the declarations alone.
fn open(root: &Path, named: Option<&str>) -> Option<(List, Vec<crate::resolve::Resolved>)> {
    let crate::Loaded {
        list,
        resolved,
        mut issues,
        context,
        ..
    } = crate::load(root);
    if named.is_none() {
        if let Some(issue) = list.no_default() {
            issues.push(issue);
        }
    }
    match issues.report(&context) {
        true => None,
        false => Some((list, resolved)),
    }
}

fn datastream(list: &List, named: Option<&str>) -> Result<String, String> {
    let name = target(list, named)?;
    let (image, _, _) = of_target(list, &name).ok_or(unknown(&name))?;
    if image.conforms.is_empty() {
        return Ok(String::new());
    }
    let family = image.base.as_ref().map_or("", |base| base.family.as_str());
    let file = match family {
        "fedora" => "ssg-fedora-ds.xml",
        "debian" => "ssg-debian12-ds.xml",
        "ubuntu" => "ssg-ubuntu2404-ds.xml",
        _ => return Err(format!("no SSG content is known for the `{family}` family")),
    };
    Ok(format!("{CONTENT}/{file}"))
}

/// Reads the repository, the datastream and the report, prints what the three
/// of them say together, and holds the claims to it.
pub fn run(root: &Path, arf: &Path, opts: &Options) -> Result<Verdict, String> {
    let named = opts.target.as_deref();
    let Some((list, resolved)) = open(root, named) else {
        return Ok(Verdict::Wrong);
    };
    let name = target(&list, named)?;
    let (image, flavour, entries) = of_target(&list, &name).ok_or(unknown(&name))?;
    let at = list.images.iter().position(|have| have.id == image.id);
    let shipped = &resolved[at.unwrap_or_default()].shipped;
    let gate = flavour.as_deref().unwrap_or(NO_FLAVOUR);

    let datastream = match &opts.datastream {
        Some(path) => path.clone(),
        None => match datastream(&list, Some(&name))? {
            empty if empty.is_empty() => {
                return Err(format!(
                    "`{name}` declares no `conforms`, so there is nothing to measure it against; \
                     name a datastream to scan it anyway"
                ))
            }
            path => PathBuf::from(path),
        },
    };
    let content = Content::read(&read(&datastream)?);
    let measured = results(&read(arf)?);

    let mut found: Vec<Finding> = Vec::new();
    let mut out = String::new();
    let passed: BTreeSet<String> = measured
        .iter()
        .filter(|(_, result)| *result == "pass")
        .map(|(rule, _)| rule.clone())
        .collect();
    let failed = measured.values().filter(|r| *r == "fail").count();
    let _ = writeln!(
        out,
        "The scan measured {} rules: {} pass, {failed} fail.\n",
        measured.len(),
        passed.len()
    );

    claims(
        &mut out,
        &mut found,
        Measured {
            image,
            entries: &entries,
            shipped,
            gate,
            content: &content,
            measured: &measured,
        },
    );
    profiles(&mut out, &mut found, image, &content, &measured);
    if let Some(path) = &opts.baseline {
        ratchet(&mut out, &mut found, path, &name, &passed, &measured)?;
    }
    print!("{out}");

    if found.is_empty() {
        return Ok(Verdict::Clean);
    }
    if !list.audit_enforce {
        for finding in &found {
            eprintln!("tect: {}", finding.message);
        }
        eprintln!(
            "tect: {} finding{}, and the repository does not enforce",
            found.len(),
            if found.len() == 1 { "" } else { "s" }
        );
        return Ok(Verdict::Clean);
    }
    let mut issues = Issues::default();
    for finding in found {
        let issue = match &finding.at {
            Some((src, span)) => {
                Issue::new(finding.message, src).at(*span, "this is what was measured")
            }
            None => Issue::new(finding.message, &list.repo_src),
        };
        issues.push(match finding.help {
            Some(help) => issue.help(help),
            None => issue,
        });
    }
    issues.report(&crate::context(&list, root));
    Ok(Verdict::Wrong)
}

/// One thing the measurement says is wrong, pointed at the declaration that
/// says it, which is a module manifest for a claim and nothing for a rule that
/// merely stopped passing.
struct Finding {
    message: String,
    at: Option<(Source, Span)>,
    help: Option<String>,
}

/// Everything the declared half of the comparison reads.
struct Measured<'a> {
    image: &'a Image,
    entries: &'a [&'a Entry],
    shipped: &'a overlay::Index,
    gate: &'a str,
    content: &'a Content,
    measured: &'a BTreeMap<String, String>,
}

/// Every rule the target's modules claim, against what the scan measured.
fn claims(out: &mut String, found: &mut Vec<Finding>, m: Measured) {
    let mut rows = String::new();
    for entry in m.entries {
        let Some(module) = entry.module.as_ref() else {
            continue;
        };
        for coverage in &module.satisfies {
            for rule in &coverage.rules {
                let at = Some((module.src.clone(), coverage.span));
                let Some(id) = m.content.rules.get(rule) else {
                    let _ = writeln!(
                        rows,
                        "| {} | {} | {rule} | **maps to nothing** |",
                        entry.path, coverage.benchmark
                    );
                    found.push(Finding {
                        message: format!(
                            "{} claims {} {rule}, which names no rule in the datastream",
                            entry.path, coverage.benchmark
                        ),
                        at,
                        help: Some(
                            "the declaration is wrong rather than the image: a number is only \
                             usable where the scanned family's content carries it"
                                .into(),
                        ),
                    });
                    continue;
                };
                let result = m.measured.get(id).map_or("notselected", String::as_str);
                let _ = writeln!(
                    rows,
                    "| {} | {} | {rule} | {result} |",
                    entry.path, coverage.benchmark
                );
                if result != "fail" {
                    continue;
                }
                // A claim whose files another module replaced is a composition
                // failure: the claimant is honest and the image is not hardened.
                let lost = overridden(m.image, m.shipped, m.gate, &entry.path);
                let help = lost.first().map(|(path, by)| {
                    format!(
                        "{by} owns the final {path}, so this claim is not contradicted; the \
                         composition defeats it"
                    )
                });
                found.push(Finding {
                    message: format!(
                        "{} claims {} {rule} and the image fails it",
                        entry.path, coverage.benchmark
                    ),
                    at,
                    help,
                });
            }
        }
    }
    if rows.is_empty() {
        let _ = writeln!(out, "No module here declares `satisfies`.\n");
        return;
    }
    let _ = write!(
        out,
        "## Declared coverage, measured\n\n\
         | Module | Benchmark | Rule | Result |\n| --- | --- | --- | --- |\n{rows}\n"
    );
}

/// What the image scores against every profile the datastream carries, which
/// is what makes choosing one to declare honest.
fn profiles(
    out: &mut String,
    found: &mut Vec<Finding>,
    image: &Image,
    content: &Content,
    measured: &BTreeMap<String, String>,
) {
    let declared = content
        .profiles
        .iter()
        .find(|profile| profile.is(&image.conforms));
    if !image.conforms.is_empty() && declared.is_none() {
        let names: Vec<&str> = content.profiles.iter().map(|p| p.name()).collect();
        found.push(Finding {
            message: format!(
                "this image conforms to `{}`, which is none of the profiles the datastream \
                 carries: {}",
                image.conforms,
                names.join(", ")
            ),
            at: Some((image.src.clone(), image.span)),
            help: Some(
                "the benchmark set is open, so only a scan catches a name nothing measures".into(),
            ),
        });
    }
    if content.profiles.is_empty() {
        return;
    }
    let _ = write!(
        out,
        "## Measured against every profile\n\n\
         | Profile | Selected | Pass | Fail | Other |\n| --- | --- | --- | --- | --- |\n"
    );
    for profile in &content.profiles {
        let selected = content.selected(&profile.id);
        let count = |want: &str| {
            selected
                .iter()
                .filter(|rule| measured.get(*rule).map(String::as_str) == Some(want))
                .count()
        };
        let (pass, fail) = (count("pass"), count("fail"));
        let _ = writeln!(
            out,
            "| `{}`{} | {} | {pass} | {fail} | {} |",
            profile.name(),
            match profile.is(&image.conforms) {
                true => " (declared)",
                false => "",
            },
            selected.len(),
            selected.len() - pass - fail
        );
    }
    out.push('\n');
}

/// What passed last time and does not now. The baseline is read before it is
/// written, so one deliberate regression is one red run and then the new floor.
fn ratchet(
    out: &mut String,
    found: &mut Vec<Finding>,
    path: &Path,
    target: &str,
    passed: &BTreeSet<String>,
    measured: &BTreeMap<String, String>,
) -> Result<(), String> {
    let before = match std::fs::read_to_string(path) {
        Ok(text) => {
            baseline(&Json::parse(&text).map_err(|err| format!("{}: {err}", path.display()))?)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    let now = Json::object([
        ("target", Json::string(target)),
        ("passed", Json::strings(passed.iter().cloned())),
    ]);
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    std::fs::write(path, now.render()).map_err(|err| format!("{}: {err}", path.display()))?;

    let Some(before) = before else {
        let _ = writeln!(
            out,
            "## Since the last scan\n\nNothing to compare against; this run is the baseline.\n"
        );
        return Ok(());
    };
    let lost: Vec<&String> = before.difference(passed).collect();
    if lost.is_empty() {
        let _ = writeln!(
            out,
            "## Since the last scan\n\nNothing that passed before stopped passing.\n"
        );
        return Ok(());
    }
    let _ = write!(
        out,
        "## Since the last scan\n\n| Rule | Now |\n| --- | --- |\n"
    );
    for rule in &lost {
        let _ = writeln!(
            out,
            "| {rule} | {} |",
            measured.get(*rule).map_or("notselected", String::as_str)
        );
    }
    out.push('\n');
    found.push(Finding {
        message: format!(
            "{} passed the last scan and does not now: {}",
            match lost.len() {
                1 => "one rule".to_string(),
                many => format!("{many} rules"),
            },
            lost.iter()
                .map(|rule| rule.as_str())
                .collect::<Vec<&str>>()
                .join(", ")
        ),
        at: None,
        help: Some(
            "nothing here declares them, so this is the image being un-hardened by something \
             it did not choose, most often a base that moved"
                .into(),
        ),
    });
    Ok(())
}

/// The pass set a previous run wrote.
fn baseline(doc: &Json) -> Option<BTreeSet<String>> {
    let Json::Object(fields) = doc else {
        return None;
    };
    let (_, passed) = fields.iter().find(|(name, _)| name == "passed")?;
    let Json::Array(items) = passed else {
        return None;
    };
    Some(
        items
            .iter()
            .filter_map(|item| match item {
                Json::String(rule) => Some(rule.clone()),
                _ => None,
            })
            .collect(),
    )
}

/// Every overlay path this module ships that another one in the same target
/// replaced, as the path and who took it.
fn overridden(
    image: &Image,
    shipped: &overlay::Index,
    gate: &str,
    module: &str,
) -> Vec<(String, String)> {
    crate::emit::plan::overrides(image, shipped, gate)
        .into_iter()
        .filter(|(_, loser, _)| loser == module)
        .map(|(path, _, by)| (path, by))
        .collect()
}

fn target(list: &List, named: Option<&str>) -> Result<String, String> {
    match named {
        Some(name) => list.find_target(name).map(|target| target.to_string()),
        None => list
            .default_target()
            .map(|target| target.to_string())
            .ok_or_else(|| "no default image to take a target from".to_string()),
    }
}

fn unknown(name: &str) -> String {
    format!("`{name}` is not a build target")
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))
}

// ---- the two documents a scan run produces --------------------------------

/// What the datastream knows: which rule a benchmark number names, and which
/// rules each profile selects.
#[derive(Default)]
struct Content {
    /// A number to the first rule in document order carrying it, matched
    /// against every reference, ident and version the way the datastream's own
    /// numbering is written.
    rules: BTreeMap<String, String>,
    ids: BTreeSet<String>,
    profiles: Vec<Profile>,
}

#[derive(Default)]
struct Profile {
    id: String,
    extends: String,
    /// Each `select`, in document order, and whether it selects or deselects.
    selects: Vec<(String, bool)>,
}

impl Profile {
    /// The tail of the id, which is what a `conforms` names.
    fn name(&self) -> &str {
        match self.id.rsplit_once("_profile_") {
            Some((_, name)) => name,
            None => &self.id,
        }
    }

    fn is(&self, declared: &str) -> bool {
        !declared.is_empty() && (self.id == declared || self.name() == declared)
    }
}

impl Content {
    fn read(text: &str) -> Self {
        let mut content = Content::default();
        let mut rule = String::new();
        let mut numbered = false;
        let mut profile: Option<Profile> = None;
        for event in xml::scan(text) {
            match event {
                xml::Event::Open {
                    name: "Rule",
                    attrs,
                } => {
                    rule = xml::attr(attrs, "id").unwrap_or_default().to_string();
                    content.ids.insert(rule.clone());
                }
                xml::Event::Close { name: "Rule" } => rule.clear(),
                xml::Event::Open {
                    name: "reference" | "ident" | "version",
                    ..
                } => numbered = !rule.is_empty(),
                xml::Event::Text(text) if numbered => {
                    content
                        .rules
                        .entry(text.to_string())
                        .or_insert_with(|| rule.clone());
                    numbered = false;
                }
                xml::Event::Open {
                    name: "Profile",
                    attrs,
                } => {
                    profile = Some(Profile {
                        id: xml::attr(attrs, "id").unwrap_or_default().to_string(),
                        extends: xml::attr(attrs, "extends").unwrap_or_default().to_string(),
                        selects: Vec::new(),
                    })
                }
                xml::Event::Close { name: "Profile" } => {
                    content.profiles.extend(profile.take());
                }
                xml::Event::Open {
                    name: "select",
                    attrs,
                } => {
                    if let (Some(profile), Some(idref)) =
                        (profile.as_mut(), xml::attr(attrs, "idref"))
                    {
                        let on = xml::attr(attrs, "selected") != Some("false");
                        profile.selects.push((idref.to_string(), on));
                    }
                }
                _ => numbered = false,
            }
        }
        content
    }

    /// Every rule one profile selects, following what it extends first, since
    /// a profile that extends another may deselect part of it.
    fn selected(&self, id: &str) -> BTreeSet<String> {
        self.select(id, 0)
    }

    fn select(&self, id: &str, depth: usize) -> BTreeSet<String> {
        let Some(profile) = self.profiles.iter().find(|have| have.id == id) else {
            return BTreeSet::new();
        };
        let mut out = match profile.extends.is_empty() || depth > 8 {
            true => BTreeSet::new(),
            false => self.select(&profile.extends, depth + 1),
        };
        for (rule, on) in &profile.selects {
            if !self.ids.contains(rule) {
                continue;
            }
            match on {
                true => out.insert(rule.clone()),
                false => out.remove(rule),
            };
        }
        out
    }
}

/// What the report measured, as a rule id to its outcome.
fn results(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut rule = String::new();
    let mut wanted = false;
    for event in xml::scan(text) {
        match event {
            xml::Event::Open {
                name: "rule-result",
                attrs,
            } => rule = xml::attr(attrs, "idref").unwrap_or_default().to_string(),
            xml::Event::Close {
                name: "rule-result",
            } => rule.clear(),
            xml::Event::Open { name: "result", .. } => wanted = !rule.is_empty(),
            xml::Event::Text(text) if wanted => {
                out.entry(rule.clone()).or_insert_with(|| text.to_string());
                wanted = false;
            }
            _ => wanted = false,
        }
    }
    out
}

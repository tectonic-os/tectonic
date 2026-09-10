//! What the modules claimed, what the scan measured, and what stopped passing
//! since the last one. The claims come off the resolved plan; the mapping from
//! a benchmark number to a rule, and the rule to a result, come off the two
//! XML documents a scan run produces.

mod xml;

use crate::diag::{Issue, Issues, Source, Span};
use crate::emit::json::{self, Json};
use crate::emit::plan::of_target;
use crate::model::image::{Entry, Image, List, NO_FLAVOUR};
use crate::provider::Index;
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
    /// What the bare base passed on its own, as a scan of an image listing no
    /// module wrote it. Read with `baseline()`, because it is that document.
    pub base: Option<PathBuf>,
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
    println!("{}", datastream(root, &list, named)?);
    Ok(Verdict::Clean)
}

/// The same answer with no repository at all: the `conforms` comes off the
/// running target and the base family off the image it belongs to. Same gate
/// as above — a target measured against nothing says so with a blank line.
pub fn content_on_host(image: &Json, target: &Json) -> Result<Verdict, String> {
    let path = match json::text(target, "conforms")
        .unwrap_or_default()
        .is_empty()
    {
        true => String::new(),
        false => installed(
            CONTENT,
            &json::field(image, "base")
                .and_then(|base| json::text(base, "family"))
                .unwrap_or_default(),
        )?
        .display()
        .to_string(),
    };
    println!("{path}");
    Ok(Verdict::Clean)
}

/// What `check` says about an image declaring `conforms`, in two tiers. A
/// notice, never an error: declaring a profile before reaching it is the point.
/// Without a datastream nothing knows which rules a profile selects, so it can
/// only report an image that declares one and claims nothing; with one it names
/// the unclaimed rules and the modules that would claim them.
pub fn conformance(
    list: &List,
    index: &Index,
    datastream: Option<&Path>,
) -> Result<Vec<String>, String> {
    let content = match datastream {
        Some(path) => Some(content_of(path)?),
        None => None,
    };
    let mut out = Vec::new();
    // A flavour may declare a `conforms` the image does not, and `tect build`
    // remediates on that answer, so a check reading the ungated one alone would
    // review every target except the ones it acts on.
    let measured = list
        .images
        .iter()
        .filter(|i| !i.conforms.is_empty() || i.flavours.iter().any(|f| !f.conforms.is_empty()));
    for image in measured {
        out.extend(unremediated(image, index));
        out.extend(allows_nothing(image));
        if let Some(content) = &content {
            out.extend(claimed_and_refused(image, content));
            out.extend(claims_nothing(image, content));
            out.extend(refuses_nothing(image, content));
        }
        match &content {
            Some(content) => out.extend(unclaimed(image, content, index)),
            None if image.modules().any(|m| !m.satisfies.is_empty())
                || image.base.iter().any(|base| !base.satisfies.is_empty()) => {}
            None => out.push(format!(
                "`{}` conforms to `{}` and nothing it installs or builds on declares `satisfies`, \
                 so nothing here claims a rule of it. Nothing read a datastream, so that is a \
                 count of declarations: `tect check --datastream <file>` says which of the \
                 profile's rules are unclaimed",
                image.id, image.conforms
            )),
        }
    }
    Ok(out)
}

/// Every benchmark number an image claims and every rule it refuses: the two
/// lists that keep remediation off what a module owns.
///
/// **The base is a claimant.** It declares `satisfies` the way a module does,
/// and a claim missing from this list is one remediation can silently make
/// true, which leaves the claim standing and unproved. `owed` and
/// `conformance` both chain the base already; this is the third reader that
/// has to agree with them.
///
/// Claims are numbers and refusals are rule IDs, which is the vocabularies
/// staying apart on purpose: 710 of 994 rules carry no number that reaches
/// them, and a rule worth refusing is often one.
///
/// **An image may lift a refusal.** A module's refusal is its judgement about
/// a deployment, and the deployment is the image's. A lifted rule is
/// remediated and then measured, so the scan says which way it went either
/// way — which is where the honesty lives.
pub fn exclusions(image: &Image) -> (Vec<String>, Vec<String>) {
    let claimed = crate::emit::plan::distinct(
        image
            .base
            .iter()
            .flat_map(|base| base.satisfies.iter())
            .chain(image.modules().flat_map(|module| module.satisfies.iter()))
            .flat_map(|coverage| coverage.rules.iter().cloned()),
    );
    let lifted: BTreeSet<&str> = image
        .allows
        .iter()
        .map(|allow| rule_name(&allow.rule))
        .collect();
    let refused = crate::emit::plan::distinct(
        image
            .modules()
            .flat_map(|module| module.refuses.iter().map(|r| rule_name(&r.rule)))
            .filter(|rule| !lifted.contains(rule))
            .map(str::to_string),
    );
    (claimed, refused)
}

/// The capability the remediation step provides, which is how an image says it
/// runs one without the tool naming a module.
const REMEDIATION: &str = "scap-remediation";

/// An image measured against a profile that nothing in it will remediate. Needs
/// no datastream: it is a question about the module list alone. A notice rather
/// than an error, because measuring without remediating is a real thing to want
/// — it is measuring and *silently* not remediating that is the accident.
///
/// Silent until something on this machine offers the capability. A notice
/// telling a reader to add a module that exists nowhere is one they cannot act
/// on, and every image declaring `conforms` would carry it.
fn unremediated(image: &Image, index: &Index) -> Option<String> {
    if index.providing(REMEDIATION).is_empty() {
        return None;
    }
    let has = image
        .modules()
        .any(|m| m.provides.iter().any(|p| p.name == REMEDIATION))
        || image
            .base
            .iter()
            .any(|base| base.provides.iter().any(|p| p.name == REMEDIATION));
    (!has).then(|| {
        format!(
            "`{}` conforms to `{}` and nothing it installs provides `{REMEDIATION}`, so the \
             profile is measured and nothing acts on it. That is a scan report rather than a \
             hardened image; add a module providing `{REMEDIATION}`, or drop `conforms`",
            image.id, image.conforms
        )
    })
}

/// A refusal naming a rule the content does not carry. It reaches the build as
/// one more `-u` and protects nothing, and a misspelling looks exactly like a
/// rule that was left alone — which is the whole thing a refusal is for.
fn refuses_nothing(image: &Image, content: &Content) -> Vec<String> {
    let known: BTreeSet<&str> = content.ids.iter().map(|id| rule_name(id)).collect();
    image
        .modules()
        .flat_map(|module| {
            module
                .refuses
                .iter()
                .map(move |refusal| (module.path.as_str(), refusal))
        })
        .filter(|(_, refusal)| !known.contains(rule_name(&refusal.rule)))
        .map(|(path, refusal)| {
            format!(
                "`{path}` refuses `{}`, which the content it is measured against carries no rule \
                 by. A refusal that reaches no rule keeps nothing out of remediation and reads \
                 exactly like one that does",
                refusal.rule
            )
        })
        .collect()
}

/// A claim whose number reaches no rule in the content this image is measured
/// against. The finalize layer resolves those numbers itself — a claim is
/// written as a benchmark number and a tailoring wants a rule — so a number
/// that reaches nothing fails the build inside the remediation hook, where the
/// diagnostic is a `RUN` that stopped. Named here instead.
///
/// This is not always a typo. A whole vocabulary can be missing: measured
/// 2026-09-09, `ssg-cs10-ds.xml` carries no `CCI-` number and no `BP28(` at
/// all, so `hardening/login-defaults`, gated to `family "fedora"` and correct
/// on `fedora-bootc`, names nothing on the EL base that declares the same
/// family. The gate a claim needs is the content, and `family` is not it.
fn claims_nothing(image: &Image, content: &Content) -> Vec<String> {
    let claimants = image
        .base
        .iter()
        .map(|base| ("its base", &base.satisfies))
        .chain(image.modules().map(|m| (m.path.as_str(), &m.satisfies)));
    claimants
        .flat_map(|(who, satisfies)| satisfies.iter().map(move |coverage| (who, coverage)))
        .filter_map(|(who, coverage)| {
            let lost: Vec<&str> = coverage
                .rules
                .iter()
                .filter(|number| !content.rules.contains_key(*number))
                .map(String::as_str)
                .collect();
            (!lost.is_empty()).then(|| {
                format!(
                    "`{who}` claims {} under `{}`, which the content `{}` is measured against \
                     carries no rule by. The remediation hook resolves a claim to the rule it \
                     protects, so this stops the build in the finalize layer",
                    lost.join(", "),
                    coverage.benchmark,
                    image.id
                )
            })
        })
        .collect()
}

/// A rule one image both claims and refuses. The two are written in different
/// vocabularies — a claim in a benchmark number, a refusal in a rule ID — so
/// only a datastream can tell that they meet, which is what puts this in the
/// second tier.
fn claimed_and_refused(image: &Image, content: &Content) -> Vec<String> {
    let (claimed, refused) = exclusions(image);
    let refused: BTreeSet<&str> = refused.iter().map(String::as_str).collect();
    claimed
        .iter()
        .filter_map(|number| Some((number, content.rules.get(number)?)))
        .filter(|(_, rule)| refused.contains(rule_name(rule)))
        .map(|(number, rule)| {
            format!(
                "`{}` claims `{number}` and something it installs refuses `{}`, which is the \
                 same rule. A claim says the image passes it and a refusal says nothing may set \
                 it, so one of the two is wrong",
                image.id,
                rule_name(rule)
            )
        })
        .collect()
}

/// What an image still owes the profile it is measured against.
pub struct Owed<'a> {
    /// How many rules the profile selects, which is what `open` is out of.
    pub selects: usize,
    /// The selected rules nothing the image installs and no base it builds on
    /// claims. A module the base suppressed is installed by nothing, so its
    /// claims are not here.
    pub open: BTreeSet<String>,
    /// Modules elsewhere claiming one of them, already minus what the image
    /// lists: an offer is only worth making about what it does not have.
    pub helping: Vec<&'a crate::provider::Provider>,
    /// The open rules `helping` would close, which is fewer than `open` where
    /// nothing claims the rest.
    pub covered: BTreeSet<String>,
}

/// Which of a profile's rules an image is still missing, and who would claim
/// them. The claim resolves forward through `Content::rules`; the search for
/// who would help runs backward through `Content::numbering`.
pub fn owed<'a>(image: &Image, content: &Content, profile: &Profile, index: &'a Index) -> Owed<'a> {
    let selected = content.selected(&profile.id);
    // The base claims rules the way a listed module does, and a module the base
    // suppressed left `entries` with its claims, so a base that covers a module
    // says for itself what that module was claiming.
    let claimed = reached(
        content,
        image
            .modules()
            .flat_map(|module| module.satisfies.iter())
            .chain(image.base.iter().flat_map(|base| base.satisfies.iter()))
            .flat_map(|coverage| coverage.rules.iter()),
    );
    let open: BTreeSet<String> = selected.difference(&claimed).cloned().collect();
    // A module the image already lists is not an answer to what it is missing.
    // A module the base suppressed is still one the image lists, so it is
    // still not an answer to what the image is missing.
    let listed: BTreeSet<String> = image
        .entries
        .iter()
        .chain(&image.suppressed)
        .map(Entry::dir)
        .collect();
    let helping: Vec<&crate::provider::Provider> = match open.is_empty() {
        true => Vec::new(),
        false => index
            .claiming(&content.numbering(&open))
            .into_iter()
            .filter(|provider| !listed.contains(&provider.dir()))
            .collect(),
    };
    let covered: BTreeSet<String> = helping
        .iter()
        .flat_map(|provider| reached(content, provider.declares.satisfies.iter()))
        .filter(|rule| open.contains(rule))
        .collect();
    Owed {
        selects: selected.len(),
        open,
        helping,
        covered,
    }
}

/// The rules `numbers` reach: a claim names the first rule in document order
/// carrying it, which is the direction a claim resolves in.
pub fn reached<'a>(
    content: &Content,
    numbers: impl Iterator<Item = &'a String>,
) -> BTreeSet<String> {
    numbers
        .filter_map(|number| content.rules.get(number))
        .cloned()
        .collect()
}

pub(crate) fn profile_names(content: &Content) -> String {
    let names: Vec<&str> = content.profiles.iter().map(Profile::name).collect();
    match names.is_empty() {
        true => "no profile at all".into(),
        false => names.join(", "),
    }
}

/// The datastream-backed tier: of the rules the declared profile selects, the
/// ones nothing it installs and no base it builds on claims, and what
/// elsewhere would claim them.
fn unclaimed(image: &Image, content: &Content, index: &Index) -> Option<String> {
    let Some(profile) = content.profiles.iter().find(|p| p.is(&image.conforms)) else {
        return Some(format!(
            "`{}` conforms to `{}`, which is none of the profiles the datastream carries: {}",
            image.id,
            image.conforms,
            profile_names(content)
        ));
    };
    let owed = owed(image, content, profile, index);
    if owed.open.is_empty() {
        return None;
    }
    let named: Vec<String> = owed
        .helping
        .iter()
        .map(|provider| format!("`{}`", provider.qualified()))
        .collect();

    let searched = match index.unread().is_empty() && index.sourced() {
        true => "the repository or its collections",
        false => "the repository",
    };
    let found = match named.is_empty() {
        true => format!("nothing else in {searched} claims them"),
        false => format!(
            "{} would claim {} of them",
            named.join(", "),
            owed.covered.len()
        ),
    };
    let unsearched = match index.unsearched() {
        clause if clause.is_empty() => String::new(),
        clause => format!(". {clause}"),
    };
    Some(format!(
        "`{}` conforms to `{}`, and nothing it installs or builds on claims {} of the {} rules it \
         selects; {found}{unsearched}",
        image.id,
        image.conforms,
        owed.open.len(),
        owed.selects
    ))
}

/// An `allow-remediation` naming a rule nothing this image installs refuses. It
/// changes no build arg and reads exactly like one that lifted something, which
/// is the same failure a misspelled refusal has. Needs no datastream: it is a
/// question about the module list.
fn allows_nothing(image: &Image) -> Vec<String> {
    let refused: BTreeSet<&str> = image
        .modules()
        .flat_map(|module| module.refuses.iter().map(|r| rule_name(&r.rule)))
        .collect();
    image
        .allows
        .iter()
        .filter(|allow| !refused.contains(rule_name(&allow.rule)))
        .map(|allow| {
            format!(
                "`{}` allows remediation of `{}` and nothing it installs refuses that rule, so \
                 the allowance lifts nothing. A rule no module refuses is remediated already",
                image.id, allow.rule
            )
        })
        .collect()
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

/// The content one target is measured against, as `tect build` hands it to the
/// finalize layer. Empty where the target declares no `conforms`, which is the
/// same gate the layer's own `CONFORMS` carries.
pub fn content_for(root: &Path, list: &List, named: Option<&str>) -> Result<String, String> {
    datastream(root, list, named)
}

/// The profile a scan names to run the tailoring below.
const MEASURED: &str = "xccdf_tect_profile_measured";

/// Prints the tailoring one target is scanned with, and nothing where it
/// declares no `conforms`. `(all)` scores every variable at its default, so a
/// rule remediated to the declared profile's value reads there as failing.
pub fn tailoring_for(
    root: &Path,
    named: Option<&str>,
    given: Option<&Path>,
) -> Result<Verdict, String> {
    let Some((list, _)) = open(root, named) else {
        return Ok(Verdict::Wrong);
    };
    let name = target(&list, named)?;
    let (image, flavour, _) = of_target(&list, &name).ok_or(unknown(&name))?;
    let conforms = image.conforms_of(flavour.as_deref().unwrap_or(NO_FLAVOUR));
    if conforms.is_empty() {
        return Ok(Verdict::Clean);
    }
    let path = match given {
        Some(path) => path.to_path_buf(),
        None => PathBuf::from(datastream(root, &list, Some(&name))?),
    };
    let content = content_of(&path)?;
    let profile = content
        .profiles
        .iter()
        .find(|p| p.is(conforms))
        .ok_or_else(|| {
            format!(
                "`{name}` conforms to `{conforms}`, which is none of the profiles {} carries: {}",
                path.display(),
                profile_names(&content)
            )
        })?;
    print!("{}", tailoring(&content, profile));
    Ok(Verdict::Clean)
}

/// The declared profile with every rule selected, so one scan measures the
/// profile at its own values and every claim outside it besides.
/// XCCDF requires the version's `time`; a fixed one keeps the output a golden.
fn tailoring(content: &Content, profile: &Profile) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <xccdf-1.2:Tailoring xmlns:xccdf-1.2=\"http://checklists.nist.gov/xccdf/1.2\" \
         id=\"xccdf_tect_tailoring_measured\">\n  \
         <xccdf-1.2:version time=\"1970-01-01T00:00:00\">1</xccdf-1.2:version>\n  \
         <xccdf-1.2:Profile id=\"{MEASURED}\" extends=\"{}\">\n    \
         <xccdf-1.2:title>{}, and every other rule</xccdf-1.2:title>\n",
        profile.id,
        profile.name()
    );
    // A rule under a group the profile deselects stays unselected however it
    // is selected itself.
    for id in content.groups.iter().chain(&content.ids) {
        let _ = writeln!(
            out,
            "    <xccdf-1.2:select idref=\"{id}\" selected=\"true\"/>"
        );
    }
    out.push_str("  </xccdf-1.2:Profile>\n</xccdf-1.2:Tailoring>\n");
    out
}

fn datastream(root: &Path, list: &List, named: Option<&str>) -> Result<String, String> {
    let name = target(list, named)?;
    let (image, flavour, _) = of_target(list, &name).ok_or(unknown(&name))?;
    if image
        .conforms_of(flavour.as_deref().unwrap_or(NO_FLAVOUR))
        .is_empty()
    {
        return Ok(String::new());
    }
    let base = image.base.as_ref();
    let mut issues = crate::diag::Issues::default();
    let (bases, _) = crate::base::catalog(root, &list.sources, &mut issues);
    Ok(measured_by(
        CONTENT,
        &bases,
        base.map_or("", |base| base.image.as_str()),
        base.map_or("", |base| base.family.as_str()),
    )?
    .display()
    .to_string())
}

/// The content one base is measured against: the file its catalog row names,
/// and the family default for a base the catalog does not describe. A row
/// declaring none falls through to that default, which is what refuses the deb
/// families by name.
fn measured_by(
    dir: &str,
    bases: &[crate::base::Base],
    image: &str,
    family: &str,
) -> Result<PathBuf, String> {
    match crate::base::find(bases, image) {
        Some(base) if !base.scap_content.is_empty() => Ok(Path::new(dir).join(&base.scap_content)),
        // The catalog describes this base and names no content for it. Falling
        // back to the family is how an EL base — which declares `family
        // "fedora"` because every other family-gated behaviour matches — gets
        // measured against Fedora's benchmark and comes back with numbers
        // rather than an error.
        Some(base) => match installed(dir, family) {
            // The family has no content either, which is the deb case and
            // carries its own written reason.
            Err(refusal) => Err(refusal),
            Ok(_) => Err(format!(
                "`{}` names no `scap-content`, so nothing says which benchmark it is measured \
                 against\n\nhelp: add `scap-content \"ssg-<os>-ds.xml\"` to its row. A row that \
                 carries one in the source tree and not here is a stale installed catalog, which \
                 `TECT_ASSETS=<tree>/assets` points past",
                base.image
            )),
        },
        // A base the catalog does not describe: the family is all there is.
        None => installed(dir, family),
    }
}

/// Where a family's SCAP content is. Both deb families refuse: SSG writes one
/// `<xccdf-1.2:platform>` per benchmark and every rule inherits it, so against
/// bases of `debian:forky` and `ubuntu:26.04` the nearest content marks every
/// rule `notapplicable` — a scan that measures nothing, silently.
fn installed(dir: &str, family: &str) -> Result<PathBuf, String> {
    let file = match family {
        "fedora" => "ssg-fedora-ds.xml",
        "debian" | "ubuntu" => {
            return Err(format!(
                "SSG publishes no content a `{family}` image can be measured against\n\nhelp: \
                 every SSG benchmark names the one release it applies to, and there is no Debian \
                 14 benchmark and no usable Ubuntu 26.04 one; drop `conforms` from this image, or \
                 name content of your own with `--datastream <file>`"
            ))
        }
        _ => return Err(format!("no SSG content is known for the `{family}` family")),
    };
    Ok(Path::new(dir).join(file))
}

/// The content a profile is chosen out of: what was named, else the copy this
/// machine has for the family. This probes the host, so no command whose output
/// is a golden may reach it.
pub fn content_path(family: &str, given: Option<&Path>) -> Result<PathBuf, String> {
    content_at(CONTENT, family, given)
}

/// Reads and recognizes a SCAP datastream.
pub fn content_of(path: &Path) -> Result<Content, String> {
    let content = Content::read(&read(path)?);
    if content.ids.is_empty() && content.profiles.is_empty() {
        return Err(format!(
            "{} carries no XCCDF rule and no profile, so it is not SCAP content\n\nhelp: use an \
             `ssg-<os>-ds.xml` file; `tect scap content` prints the path a scan uses",
            path.display()
        ));
    }
    Ok(content)
}

fn content_at(dir: &str, family: &str, given: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = given {
        return Ok(path.to_path_buf());
    }
    let path = installed(dir, family)?;
    match path.is_file() {
        true => Ok(path),
        false => Err(format!(
            "{} is not there, so there is no content to choose a profile out of; install \
             `scap-security-guide`, or name one with `--datastream <file>`",
            path.display()
        )),
    }
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
        None => match datastream(root, &list, Some(&name))? {
            empty if empty.is_empty() => {
                return Err(format!(
                    "`{name}` declares no `conforms`, so there is nothing to measure it against; \
                     name a datastream to scan it anyway"
                ))
            }
            path => PathBuf::from(path),
        },
    };
    let content = content_of(&datastream)?;
    let measured = results(&read(arf)?);
    let base = match &opts.base {
        None => None,
        Some(path) => {
            let doc =
                Json::parse(&read(path)?).map_err(|err| format!("{}: {err}", path.display()))?;
            Some(baseline(&doc).ok_or_else(|| {
                format!(
                    "{}: no `passed` list, so it is not a scan's pass set",
                    path.display()
                )
            })?)
        }
    };

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
            base: base.as_ref(),
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
    /// What the bare base passed alone. It holds passes and nothing else, so a
    /// rule missing from it may have failed there or never been measured.
    base: Option<&'a BTreeSet<String>>,
}

/// Every rule the target's modules claim, against what the scan measured, and
/// where a base scan was read, against what the bare base passed alone.
fn claims(out: &mut String, found: &mut Vec<Finding>, m: Measured) {
    let mut rows = String::new();
    let mut redundant: Vec<String> = Vec::new();
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
                        "| {} | {} | {rule} |{} **maps to nothing** |",
                        entry.path,
                        coverage.benchmark,
                        base_cell(m.base, None)
                    );
                    found.push(Finding {
                        message: format!(
                            "{} claims {} {rule}, which names no rule in the datastream",
                            entry.path, coverage.benchmark
                        ),
                        at,
                        help: Some(
                            "a number is only usable where the scanned family's content \
                             carries it, so the declaration is what has to change"
                                .into(),
                        ),
                    });
                    continue;
                };
                let over = m.base.is_some_and(|passed| passed.contains(id));
                let result = m.measured.get(id).map_or("notselected", String::as_str);
                let _ = writeln!(
                    rows,
                    "| {} | {} | {rule} |{} {result} |",
                    entry.path,
                    coverage.benchmark,
                    base_cell(m.base, Some(id))
                );
                if over && result == "pass" {
                    redundant.push(format!("{} {} {rule}", entry.path, coverage.benchmark));
                }
                if result != "fail" {
                    continue;
                }
                // A claim whose files another module replaced is a composition
                // failure: the claimant is honest and the image is not hardened.
                let lost = overridden(m.image, m.shipped, m.gate, &entry.path);
                let help = lost
                    .first()
                    .map(|(path, by)| {
                        format!(
                            "{by} owns the final {path}, so this claim is not contradicted; the \
                             composition defeats it"
                        )
                    })
                    .or_else(|| over.then(|| regressed(m.image)));
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
    let head = match m.base.is_some() {
        true => {
            "| Module | Benchmark | Rule | Base alone | Result |\n| --- | --- | --- | --- | \
                 --- |\n"
        }
        false => "| Module | Benchmark | Rule | Result |\n| --- | --- | --- | --- |\n",
    };
    let _ = write!(out, "## Declared coverage, measured\n\n{head}{rows}\n");
    if m.base.is_none() {
        return;
    }
    let _ = writeln!(
        out,
        "The base column is the bare base's own pass set. It records what passed and nothing \
         else, so `-` is not a failure: the rule may have failed on the base, or may never have \
         been measured there.\n"
    );
    if !redundant.is_empty() {
        let _ = writeln!(
            out,
            "Not load-bearing: {}. The base alone already passes {}, so the image passes with \
             the module and without it. Whether the module implements it too is not something \
             this scan can say, and it applies its settings either way.\n",
            redundant.join(", "),
            match redundant.len() {
                1 => "it",
                _ => "them",
            }
        );
    }
}

/// The `base alone` cell, absent where no base scan was read. The document
/// holds a pass set and nothing else, so the only two answers it has are that
/// the base passed the rule and that it does not say.
fn base_cell(base: Option<&BTreeSet<String>>, id: Option<&str>) -> String {
    match base {
        None => String::new(),
        Some(passed) => match id.is_some_and(|id| passed.contains(id)) {
            true => " pass |".into(),
            false => " - |".into(),
        },
    }
}

/// A claim the base alone passed and the built image now fails, which is the
/// base moving under the image.
fn regressed(image: &Image) -> String {
    format!(
        "{} alone passed this rule when the base scan was taken, so the image lost it rather \
         than never having had it: a base that moved, or a layer over it",
        image
            .base
            .as_ref()
            .map_or("the base".to_string(), |base| format!("`{}`", base.image))
    )
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
    let _ = writeln!(
        out,
        "\nEvery row is scored at the values the scan ran with, which are the named profile's \
         under `tect scap tailoring` and every default under `(all)`.\n"
    );
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

/// What the datastream knows: which rule a benchmark number names, which
/// numbers name it back, what each rule and profile is called, and which rules
/// each profile selects.
#[derive(Default)]
pub struct Content {
    /// A number to the first rule in document order carrying it, matched
    /// against every reference, ident and version the way the datastream's own
    /// numbering is written.
    pub rules: BTreeMap<String, String>,
    /// The inverse of `rules`: a rule to every number that resolves to it, so a
    /// number a later rule also carries is absent here. Kept, it would point at
    /// a rule a claim would not reach.
    pub numbers: BTreeMap<String, BTreeSet<String>>,
    /// A rule to its title, which is the one line naming it a person reads.
    pub titles: BTreeMap<String, String>,
    /// A rule to the head of its description, which is the only prose the
    /// content carries about it. Markup between the open and the text drops it,
    /// the way a title's does.
    pub descriptions: BTreeMap<String, String>,
    ids: BTreeSet<String>,
    /// Every group, which a profile may deselect with the rules under it.
    groups: BTreeSet<String>,
    pub profiles: Vec<Profile>,
}

#[derive(Default)]
pub struct Profile {
    pub id: String,
    pub title: String,
    extends: String,
    /// Each `select`, in document order, and whether it selects or deselects.
    selects: Vec<(String, bool)>,
}

impl Profile {
    /// The tail of the id, which is what a `conforms` names.
    pub fn name(&self) -> &str {
        match self.id.rsplit_once("_profile_") {
            Some((_, name)) => name,
            None => &self.id,
        }
    }

    pub fn is(&self, declared: &str) -> bool {
        !declared.is_empty() && (self.id == declared || self.name() == declared)
    }
}

/// The tail of a profile id, which is the form `autotailor` and `oscap
/// --profile` both take. `conforms` accepts either spelling, and the full one
/// would be namespaced a second time on the way into a tailoring file.
pub fn profile_name(id: &str) -> &str {
    id.rsplit_once("_profile_").map_or(id, |(_, tail)| tail)
}

/// The tail of a rule id, which names it without the datastream's own prefix.
pub fn rule_name(id: &str) -> &str {
    id.rsplit_once("_rule_").map_or(id, |(_, tail)| tail)
}

/// A dotted benchmark number as what it sorts by, with a part that is not one
/// sorting after every part that is, so 1.9 reads before 1.10.
pub fn ordinal(number: &str) -> Vec<u64> {
    number
        .split('.')
        .map(|part| part.parse().unwrap_or(u64::MAX))
        .collect()
}

impl Content {
    pub fn read(text: &str) -> Self {
        let mut content = Content::default();
        let mut rule = String::new();
        // The element whose text is wanted, taken every event so that anything
        // between the open and the text drops it.
        let mut wanted: Option<&str> = None;
        let mut profile: Option<Profile> = None;
        for event in xml::scan(text) {
            let want = wanted.take();
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
                    name: "Group",
                    attrs,
                } => {
                    content
                        .groups
                        .insert(xml::attr(attrs, "id").unwrap_or_default().to_string());
                }
                xml::Event::Open {
                    name: name @ ("reference" | "ident" | "version" | "title" | "description"),
                    ..
                } => wanted = Some(name),
                xml::Event::Text(text) => match want {
                    Some("title") if !rule.is_empty() => {
                        content
                            .titles
                            .entry(rule.clone())
                            .or_insert_with(|| text.to_string());
                    }
                    Some("title") => {
                        if let Some(profile) = profile.as_mut() {
                            if profile.title.is_empty() {
                                profile.title = text.to_string();
                            }
                        }
                    }
                    Some("description") if !rule.is_empty() => {
                        content
                            .descriptions
                            .entry(rule.clone())
                            .or_insert_with(|| text.to_string());
                    }
                    Some(_) if !rule.is_empty() => {
                        content
                            .rules
                            .entry(text.to_string())
                            .or_insert_with(|| rule.clone());
                    }
                    _ => {}
                },
                xml::Event::Open {
                    name: "Profile",
                    attrs,
                } => {
                    profile = Some(Profile {
                        id: xml::attr(attrs, "id").unwrap_or_default().to_string(),
                        extends: xml::attr(attrs, "extends").unwrap_or_default().to_string(),
                        ..Profile::default()
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
                _ => {}
            }
        }
        let mut numbers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (number, rule) in &content.rules {
            numbers
                .entry(rule.clone())
                .or_default()
                .insert(number.clone());
        }
        content.numbers = numbers;
        content
    }

    /// Every benchmark number that would reach these rules, which is what a
    /// module's claim is written in.
    pub fn numbering(&self, rules: &BTreeSet<String>) -> BTreeSet<String> {
        rules
            .iter()
            .filter_map(|rule| self.numbers.get(rule))
            .flatten()
            .cloned()
            .collect()
    }

    /// Every rule one profile selects, following what it extends first, since
    /// a profile that extends another may deselect part of it.
    pub fn selected(&self, id: &str) -> BTreeSet<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::disk::Disk;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
    }

    /// The one derivation this adds: a profile selects rules, each rule is
    /// reached by the numbers that resolve to it, and the index says which
    /// modules claim those numbers.
    #[test]
    fn the_index_answers_which_modules_claim_a_profile_s_rules() {
        let content = Content::read(&read(&fixture("tests/scap/datastream.xml")).unwrap());
        let profile = content
            .profiles
            .iter()
            .find(|profile| profile.is("standard"))
            .expect("the fixture carries `standard`");
        assert_eq!(profile.title, "Standard System Security Profile for Fedora");
        assert_eq!(
            content
                .titles
                .get("xccdf_org.ssgproject.content_rule_package_aide_installed")
                .map(String::as_str),
            Some("Install AIDE")
        );

        // Both the CIS number and the STIG ident reach the one rule, and the
        // group the profile selects but no `Rule` defines reaches nothing.
        let numbers = content.numbering(&content.selected(&profile.id));
        assert!(numbers.contains("5.2.20") && numbers.contains("RHEL-09-232010"));
        assert!(!numbers.contains("5.5.2"), "`standard` does not select it");

        let root = fixture("tests/repos/enforced");
        let index = Index::scan(&root, &[], &Disk::scan(&root), false);
        let claiming: Vec<String> = index
            .claiming(&numbers)
            .iter()
            .map(|held| held.qualified())
            .collect();
        assert_eq!(claiming, ["one/auditing", "one/hello"]);
        assert!(index.claiming(&BTreeSet::new()).is_empty());
    }

    /// The flag wins; with none, the family's installed copy, and its absence
    /// names both ways to answer it.
    #[test]
    fn a_named_datastream_is_taken_and_a_missing_one_names_both_remedies() {
        let given = fixture("tests/scap/datastream.xml");
        assert_eq!(
            content_at("/nowhere", "fedora", Some(&given)).unwrap(),
            given
        );
        assert_eq!(
            content_at(
                given
                    .parent()
                    .expect("a fixture directory")
                    .to_str()
                    .unwrap(),
                "fedora",
                None
            )
            .unwrap_err(),
            format!(
                "{} is not there, so there is no content to choose a profile out of; install \
                 `scap-security-guide`, or name one with `--datastream <file>`",
                given.with_file_name("ssg-fedora-ds.xml").display()
            )
        );
        assert!(content_at("/nowhere", "arch", None)
            .unwrap_err()
            .contains("no SSG content is known"));
    }

    /// A deb image declaring `conforms` is refused: a scan reporting nothing
    /// measured looks exactly like a scan reporting nothing wrong.
    #[test]
    fn a_deb_family_is_refused_rather_than_given_content_that_measures_nothing() {
        for family in ["debian", "ubuntu"] {
            let err = content_at("/nowhere", family, None).unwrap_err();
            assert!(
                err.contains(&format!("no content a `{family}` image")),
                "{err}"
            );
            assert!(err.contains("drop `conforms`"), "{err}");
        }
        // A named datastream still wins: the refusal is about what is published,
        // not about the family being unmeasurable by anything.
        let given = fixture("tests/scap/datastream.xml");
        assert_eq!(
            content_at("/nowhere", "debian", Some(&given)).unwrap(),
            given
        );
    }

    /// The bug the `scap-content` row exists to close. Every family-gated
    /// behaviour an EL base has matches Fedora's — dnf, rpm, SELinux, bootupd —
    /// so it declares `family "fedora"`, and the family default would measure it
    /// against the wrong benchmark and return numbers rather than an error.
    #[test]
    fn a_base_row_names_content_the_family_default_would_get_wrong() {
        let row = |image: &str, content: &str| crate::base::Base {
            image: image.to_string(),
            family: "fedora".to_string(),
            provides: Vec::new(),
            provides_files: Vec::new(),
            requires: Vec::new(),
            about: String::new(),
            signed: false,
            scap_content: content.to_string(),
            span: crate::diag::Span::default(),
        };
        let bases = [
            row(
                "quay.io/centos-bootc/centos-bootc:stream10",
                "ssg-cs10-ds.xml",
            ),
            row("quay.io/fedora/fedora-bootc:44", "ssg-fedora-ds.xml"),
            row("example.invalid/silent:1", ""),
        ];
        let of = |image, family| measured_by("/nowhere", &bases, image, family);

        let el = of("quay.io/centos-bootc/centos-bootc:stream10", "fedora").unwrap();
        assert!(el.ends_with("ssg-cs10-ds.xml"), "{}", el.display());

        let fedora = of("quay.io/fedora/fedora-bootc:44", "fedora").unwrap();
        assert!(
            fedora.ends_with("ssg-fedora-ds.xml"),
            "{}",
            fedora.display()
        );

        // A digest pins a catalogued tag rather than naming a second base.
        let pinned = of(
            "quay.io/centos-bootc/centos-bootc:stream10@sha256:0000",
            "fedora",
        )
        .unwrap();
        assert!(pinned.ends_with("ssg-cs10-ds.xml"), "{}", pinned.display());

        // A base the catalog does not describe has only the family to go on.
        let fell = of("example.invalid/unknown:1", "fedora").unwrap();
        assert!(fell.ends_with("ssg-fedora-ds.xml"), "{}", fell.display());

        // A row that names none is refused rather than guessed at. This is the
        // stale-catalog case: the installed `bases.kdl` predates the field, and
        // guessing hands an EL image Fedora's benchmark without a word.
        let err = of("example.invalid/silent:1", "fedora").unwrap_err();
        assert!(err.contains("names no `scap-content`"), "{err}");
        assert!(err.contains("stale installed catalog"), "{err}");

        // Where the family has no content either, its own reason wins.
        let err = of("example.invalid/silent:1", "debian").unwrap_err();
        assert!(err.contains("no content a `debian` image"), "{err}");
    }

    #[test]
    fn a_file_without_rules_or_profiles_is_not_scap_content() {
        let path = fixture("tests/repos/enforced/example.image.kdl");
        assert_eq!(
            content_of(&path)
                .err()
                .expect("the image file is not SCAP content"),
            format!(
                "{} carries no XCCDF rule and no profile, so it is not SCAP content\n\nhelp: use \
                 an `ssg-<os>-ds.xml` file; `tect scap content` prints the path a scan uses",
                path.display()
            )
        );
    }

    /// The datastream-backed tier over a repository whose every source was
    /// read: the claims the image lists come off the profile's rules, what is
    /// left is named, and nothing is said about a collection.
    /// Both lists feed the tailoring that keeps remediation off what a module
    /// owns, so anything missing here is a rule SSG sets on a claimant's
    /// behalf — leaving the claim standing and nothing proving it.
    #[test]
    fn the_exclusions_carry_every_claim_including_the_bases_and_every_refusal() {
        let root = fixture("tests/repos/refused");
        let loaded = crate::load(&root);
        let image = loaded
            .list
            .images
            .first()
            .expect("the fixture declares one image");
        let (claimed, refused) = exclusions(image);

        // The module's claim, and the base's, which is the one a walk over
        // `entries` alone would drop.
        assert!(claimed.contains(&"1.1.1.1".to_string()), "{claimed:?}");
        assert!(
            claimed.contains(&"RHEL-09-232010".to_string()),
            "a base claims the way a module does: {claimed:?}"
        );
        assert!(
            refused.contains(&"package_aide_installed".to_string()),
            "{refused:?}"
        );
    }

    /// Two checks that need no scan. One is a manifest arguing with itself: a
    /// claim says the image passes a rule and a refusal says nothing may set
    /// it. The other is an image measured against a profile nothing will act
    /// on, which is a report rather than a hardened image.
    #[test]
    fn check_catches_a_rule_both_claimed_and_refused_and_an_image_nothing_remediates() {
        let root = fixture("tests/repos/refused");
        let loaded = crate::load(&root);
        let said = conformance(
            &loaded.list,
            &loaded.index,
            Some(&fixture("tests/scap/datastream.xml")),
        )
        .expect("the fixture datastream reads");

        assert!(
            said.iter().any(|m| m.contains("claims `1.1.1.1`")
                && m.contains("refuses `package_aide_installed`")
                && m.contains("one of the two is wrong")),
            "{said:#?}"
        );
        assert!(
            said.iter()
                .any(|m| m.contains("nothing it installs provides `scap-remediation`")),
            "{said:#?}"
        );
        // A claim the content carries no rule by fails inside the finalize
        // layer, where the diagnostic is a `RUN` that stopped, so it is named
        // here. This is the EL case: a number correct on one benchmark and
        // absent from another.
        assert!(
            said.iter().any(|m| m.contains("one/typo")
                && m.contains("claims CCI-000198 under `stig`")
                && m.contains("stops the build in the finalize layer")),
            "{said:#?}"
        );

        // A refusal that reaches no rule protects nothing and looks identical
        // to one that does, so it is named too.
        assert!(
            said.iter().any(|m| m.contains("one/typo")
                && m.contains("refuses `not_a_rule_this_content_has`")
                && m.contains("keeps nothing out of remediation")),
            "{said:#?}"
        );

        // An allowance answering no refusal lifts nothing, and reads exactly
        // like one that did.
        assert!(
            said.iter()
                .any(|m| m.contains("`no_module_here_refuses_this`")
                    && m.contains("the allowance lifts nothing")),
            "{said:#?}"
        );
        assert!(
            !said
                .iter()
                .any(|m| m.contains("`sshd_disable_root_login`") && m.contains("lifts nothing")),
            "an allowance that answers a real refusal is silent: {said:#?}"
        );

        // And the lift reaches the build: the rule the image allowed is out of
        // the refused set the finalize layer tailors with, while the other
        // module's refusal stands.
        let image = &loaded.list.images[0];
        let (_, refused) = exclusions(image);
        assert!(
            !refused.contains(&"sshd_disable_root_login".to_string()),
            "{refused:?}"
        );
        assert!(
            refused.contains(&"package_aide_installed".to_string()),
            "another module's refusal is untouched: {refused:?}"
        );

        // Silent where nothing on this machine offers the capability: a notice
        // naming a module that exists nowhere cannot be acted on.
        let bare = fixture("tests/scap");
        let nothing = Index::scan(&bare, &[], &Disk::scan(&bare), false);
        assert!(
            !conformance(&loaded.list, &nothing, None)
                .expect("no datastream is the first tier")
                .iter()
                .any(|m| m.contains("scap-remediation")),
            "an unavailable capability is not offered"
        );
    }

    #[test]
    fn the_notice_names_the_rules_no_listed_module_claims() {
        let root = fixture("tests/repos/enforced");
        let loaded = crate::load(&root);
        let said = conformance(
            &loaded.list,
            &loaded.index,
            Some(&fixture("tests/scap/datastream.xml")),
        )
        .expect("the fixture datastream reads");
        // `9.9.9.9` is the fixture's number that reaches nothing, and it is
        // named twice over: the count leaves it out, and the claim itself is
        // one the remediation hook would fail the build resolving.
        assert_eq!(
            said,
            [
                "`one/hello` claims 9.9.9.9 under `cis-fedora`, which the content `enforced` is \
              measured against carries no rule by. The remediation hook resolves a claim to the \
              rule it protects, so this stops the build in the finalize layer",
                "`enforced` conforms to `standard`, and nothing it installs or builds on claims 2 \
              of the 4 rules it selects; `one/auditing` would claim 1 of them"
            ]
        );
        // The other arm, over an index holding no module at all: what is left
        // open is the image's own, so only the offer goes away. The claim that
        // reaches nothing is the image's own too, and stays.
        let bare = fixture("tests/scap");
        let nothing = Index::scan(&bare, &[], &Disk::scan(&bare), false);
        assert_eq!(
            conformance(
                &loaded.list,
                &nothing,
                Some(&fixture("tests/scap/datastream.xml"))
            )
            .expect("the fixture datastream reads"),
            [
                "`one/hello` claims 9.9.9.9 under `cis-fedora`, which the content `enforced` is \
              measured against carries no rule by. The remediation hook resolves a claim to the \
              rule it protects, so this stops the build in the finalize layer",
                "`enforced` conforms to `standard`, and nothing it installs or builds on claims 2 \
              of the 4 rules it selects; nothing else in the repository claims them"
            ]
        );
        // No datastream, and a listed module does declare `satisfies`: there
        // is nothing the manifests alone can say.
        assert!(conformance(&loaded.list, &loaded.index, None)
            .unwrap()
            .is_empty());
    }

    /// A module the base already provides leaves `entries` for `suppressed`,
    /// and the image lists it either way. This repository has no diagnostic to
    /// its name, so the offer would be the only thing wrong with it.
    #[test]
    fn a_module_the_base_suppressed_is_not_offered_back() {
        let root = fixture("tests/scap/suppressed-claimant");
        let loaded = crate::load(&root);
        let image = loaded.list.images.first().expect("one image");
        assert!(image.entries.is_empty(), "the base covers the only module");
        assert_eq!(
            image.suppressed.iter().map(Entry::dir).collect::<Vec<_>>(),
            ["one/aide"]
        );
        assert_eq!(
            conformance(
                &loaded.list,
                &loaded.index,
                Some(&fixture("tests/scap/datastream.xml"))
            )
            .expect("the fixture datastream reads"),
            [
                "`suppressed` conforms to `standard`, and nothing it installs or builds on claims \
              4 of the 4 rules it selects; nothing else in the repository claims them"
            ]
        );
    }

    /// Making a conformance claim is the job of an image, and a base declaring
    /// `satisfies` is a fact about that base. The base here covers the only
    /// module and says for itself the rule that module was claiming, so the
    /// rule resolves where suppression alone would have left it open.
    #[test]
    fn a_base_claims_the_rules_it_says_it_satisfies() {
        let root = fixture("tests/scap/claiming-base");
        let loaded = crate::load(&root);
        assert!(
            loaded.issues.plain().is_empty(),
            "{}",
            loaded.issues.plain()
        );
        let image = loaded.list.images.first().expect("one image");
        assert!(image.entries.is_empty(), "the base covers the only module");
        assert_eq!(
            image
                .base
                .as_ref()
                .expect("a base")
                .satisfies
                .iter()
                .map(|coverage| (coverage.benchmark.as_str(), coverage.rules.len()))
                .collect::<Vec<_>>(),
            [("cis-fedora", 2)]
        );
        assert_eq!(
            conformance(
                &loaded.list,
                &loaded.index,
                Some(&fixture("tests/scap/datastream.xml"))
            )
            .expect("the fixture datastream reads"),
            [
                "`claiming` conforms to `standard`, and nothing it installs or builds on claims 2 of \
              the 4 rules it selects; nothing else in the repository claims them"
            ]
        );
        // The declaration tier reads the same way: the suppressed module is not
        // among the ones the image lists, so the base is what keeps this quiet.
        assert!(conformance(&loaded.list, &loaded.index, None)
            .unwrap()
            .is_empty());
    }

    /// A collection declared and cached is one the search reached, so the
    /// sentence says what was searched. The collection holds a module and it
    /// claims nothing, which is the only way to be sure the walk read it.
    #[test]
    fn a_search_that_reached_the_collections_says_so() {
        let root = fixture("tests/scap/sourced-claimant");
        let loaded = crate::load(&root);
        assert!(
            loaded.issues.plain().is_empty(),
            "{}",
            loaded.issues.plain()
        );
        assert!(loaded.index.sourced() && loaded.index.unread().is_empty());
        assert!(
            loaded.index.at(".remote/held/one/plain").is_some(),
            "the collection was walked"
        );
        assert_eq!(
            conformance(
                &loaded.list,
                &loaded.index,
                Some(&fixture("tests/scap/datastream.xml"))
            )
            .expect("the fixture datastream reads"),
            [
                "`sourced` conforms to `standard`, and nothing it installs or builds on claims 4 \
              of the 4 rules it selects; nothing else in the repository or its collections claims them"
            ]
        );
    }

    /// `coverages` drops a claim whose benchmark name is empty and
    /// `parse::module::summary` keeps it, so the index offers a module the
    /// image already lists. Offering it back is what the `listed` filter stops.
    #[test]
    fn a_module_the_image_lists_is_not_offered_as_one_that_would_help() {
        let root = fixture("tests/scap/listed-claimant");
        let loaded = crate::load(&root);
        assert_eq!(
            loaded
                .index
                .at("one/broken")
                .expect("the index reads the manifest leniently")
                .declares
                .satisfies,
            ["4.1.3.1"],
            "the index has the claim the resolved module dropped"
        );
        assert_eq!(
            conformance(
                &loaded.list,
                &loaded.index,
                Some(&fixture("tests/scap/datastream.xml"))
            )
            .expect("the fixture datastream reads"),
            [
                "`listed` conforms to `standard`, and nothing it installs or builds on claims 3 \
              of the 4 rules it selects; nothing else in the repository claims them"
            ]
        );
    }
}

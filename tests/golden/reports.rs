//! The SCAP reports: coverage and the scan ratchet.

use crate::harness::{assert_golden, copy, crate_dir, tmp};

use std::path::Path;

/// One `scap` run: the report, what it said about it, and the exit code the
/// scan job branches on.
fn scap_run(root: &Path, args: &[&str]) -> (String, String, i32) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tect"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or_default(),
    )
}

/// `coverage`, over the fixture datastream and no scan at all: the read-out
/// both ways, and everything that stops it being answerable.
#[test]
fn coverage() {
    let enforced = crate_dir().join("tests/repos/enforced");
    let stream = crate_dir()
        .join("tests/scap/datastream.xml")
        .display()
        .to_string();
    let read_out = |at: &Path, args: &[&str]| {
        let mut all = vec!["--root", ".", "coverage"];
        all.extend_from_slice(args);
        all.extend(["--datastream", &stream]);
        scap_run(at, &all)
    };

    let mut out = String::new();
    for (heading, args) in [
        ("the read-out, with no scan behind it", &[][..]),
        ("and as json", &["--format", "json"][..]),
    ] {
        let (report, said, code) = read_out(&enforced, args);
        out.push_str(&format!("==== {heading}\n{report}{said}==== exit {code}\n"));
    }

    // A profile the content does not carry, an image measured against nothing,
    // and a named file that is not a datastream: every unanswerable input.
    let root = tmp().join("coverage");
    let _ = std::fs::remove_dir_all(&root);
    copy(&enforced, &root);
    let image = root.join("example.image.kdl");
    let text = std::fs::read_to_string(&image).unwrap();
    std::fs::write(
        &image,
        text.replace("conforms \"standard\"", "conforms \"nosuch\""),
    )
    .unwrap();
    for (heading, at) in [
        ("conforming to a profile the content does not carry", &root),
        (
            "and measured against nothing at all",
            &crate_dir().join("tests/repos/minimal"),
        ),
    ] {
        let (report, said, code) = read_out(at, &[]);
        out.push_str(&format!("==== {heading}\n{report}{said}==== exit {code}\n"));
    }
    let (report, said, code) = scap_run(
        &enforced,
        &[
            "--root",
            ".",
            "coverage",
            "--datastream",
            "example.image.kdl",
        ],
    );
    out.push_str(&format!(
        "==== and a datastream that is not one\n{report}{said}==== exit {code}\n"
    ));

    let (_, said, code) = scap_run(&enforced, &["--root", ".", "coverage"]);
    out.push_str(&format!(
        "==== and with nothing to measure it against\n{said}==== exit {code}\n"
    ));

    assert_golden("coverage", "report.txt", &out);
}

/// `scap`, over a fixture report and datastream: what the modules claimed
/// against what was measured, what the image scores against every profile the
/// datastream carries, and what the ratchet catches once a rule that passed
/// stops. The claims and the numbers are the `enforced` fixture's own.
#[test]
fn scap() {
    let enforced = crate_dir().join("tests/repos/enforced");
    let fixtures = crate_dir().join("tests/scap");
    let root = tmp().join("scap");
    let _ = std::fs::remove_dir_all(&root);
    copy(&enforced, &root);

    // Unenforced, so a finding is the plain line. A rendering depends on the
    // terminal it is read on.
    let repo = root.join("repo.kdl");
    let text = std::fs::read_to_string(&repo).unwrap();
    let (before, rest) = text.split_once("audit {").expect("the fixture enforces");
    let after = rest.split_once('}').expect("a closed block").1;
    std::fs::write(&repo, format!("{before}{after}")).unwrap();

    let baseline = tmp().join("scap-baseline.json");
    let _ = std::fs::remove_file(&baseline);
    let datastream = fixtures.join("datastream.xml");
    let claim_only = tmp().join("scap-claim-only");
    let _ = std::fs::remove_dir_all(&claim_only);
    copy(&enforced, &claim_only);
    let image = claim_only.join("example.image.kdl");
    let text = std::fs::read_to_string(&image).unwrap();
    std::fs::write(&image, text.replace("    conforms \"standard\"\n", "")).unwrap();

    let (report, said, code) = scap_run(
        &claim_only,
        &[
            "--root",
            ".",
            "scap",
            &fixtures.join("arf.xml").display().to_string(),
            "--datastream",
            &datastream.display().to_string(),
        ],
    );
    assert!(
        code == 2,
        "explicit datastream did not evaluate claims: {said}"
    );
    assert!(report.contains("| one/hello | cis-fedora | 1.1.1.1 | pass |"));

    let mut out = String::new();
    for (heading, arf) in [
        ("the first scan, which is the baseline", "arf.xml"),
        ("and one where a rule stopped passing", "arf-regressed.xml"),
    ] {
        let (report, said, code) = scap_run(
            &root,
            &[
                "--root",
                ".",
                "scap",
                &fixtures.join(arf).display().to_string(),
                "--datastream",
                &datastream.display().to_string(),
                "--baseline",
                &baseline.display().to_string(),
            ],
        );
        out.push_str(&format!("==== {heading}\n{report}{said}==== exit {code}\n"));
    }

    // What CI scans with: the declared profile, and every group and rule it
    // leaves out selected besides.
    let (tailoring, said, code) = scap_run(
        &root,
        &[
            "--root",
            ".",
            "scap",
            "tailoring",
            "--datastream",
            &datastream.display().to_string(),
        ],
    );
    out.push_str(&format!(
        "==== the tailoring\n{tailoring}{said}==== exit {code}\n"
    ));

    // The bare base's own pass set beside the image's. A claim the base already
    // passes is a notice and never a finding, and one the base passed that the
    // image now fails names the base.
    let base = fixtures.join("base.json");
    let against_base = [
        "--root",
        ".",
        "scap",
        &fixtures.join("arf.xml").display().to_string(),
        "--datastream",
        &datastream.display().to_string(),
        "--base-scan",
        &base.display().to_string(),
    ]
    .map(String::from);
    let borrowed: Vec<&str> = against_base.iter().map(String::as_str).collect();
    let (report, said, code) = scap_run(&root, &borrowed);
    out.push_str(&format!(
        "==== and against what the bare base passes alone\n{report}{said}==== exit {code}\n"
    ));
    let (_, said, code) = scap_run(&enforced, &borrowed);
    assert!(code == 2, "enforcement did not fail the scan: {said}");
    assert!(
        said.contains("quay.io/fedora/fedora-bootc:44` alone passed this rule"),
        "the base-regression help did not name the base: {said}"
    );

    // A profile the datastream does not carry: the open vocabulary means only
    // the scan can catch it, and what it is worth is the list it comes back
    // with.
    let image = root.join("example.image.kdl");
    let text = std::fs::read_to_string(&image).unwrap();
    std::fs::write(
        &image,
        text.replace("conforms \"standard\"", "conforms \"nosuch\""),
    )
    .unwrap();
    let (report, said, code) = scap_run(
        &root,
        &[
            "--root",
            ".",
            "scap",
            &fixtures.join("arf.xml").display().to_string(),
            "--datastream",
            &datastream.display().to_string(),
        ],
    );
    out.push_str(&format!(
        "==== conforming to a profile nothing carries\n{}{said}==== exit {code}\n",
        report.split("## Measured").nth(1).unwrap_or_default()
    ));

    // The same repository enforcing: the same report, and the findings become
    // what fails it. What it says them with is a rendering that depends on the
    // terminal, so the exit code is the assertion and only the report is
    // golden.
    let (report, said, code) = scap_run(
        &enforced,
        &[
            "--root",
            ".",
            "scap",
            &fixtures.join("arf.xml").display().to_string(),
            "--datastream",
            &datastream.display().to_string(),
        ],
    );
    assert!(code == 2, "enforcement did not fail the scan: {said}");
    assert!(said.contains("cis-fedora 5.2.20"), "{said}");
    out.push_str(&format!(
        "==== enforced, and the report is the same\n{report}"
    ));

    // The content one target is measured with, and nothing at all for one that
    // asks to be measured against nothing.
    for (heading, at) in [
        ("the content it is measured with", &enforced),
        (
            "and for an image declaring claims but no profile",
            &claim_only,
        ),
        (
            "and for an image declaring neither a claim nor a profile",
            &crate_dir().join("tests/repos/minimal"),
        ),
    ] {
        let (path, _, code) = scap_run(at, &["--root", ".", "scap", "content"]);
        out.push_str(&format!("==== {heading}\n{path}==== exit {code}\n"));
    }

    assert_golden("scap", "report.txt", &out);
}

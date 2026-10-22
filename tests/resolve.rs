use std::path::PathBuf;
use tect::Command;

#[test]
fn after_rejects_a_provider_on_another_target() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("after-gated-provider");
    let _ = std::fs::remove_dir_all(&root);
    for module in ["consumer", "provider", "coinstalled", "separate"] {
        std::fs::create_dir_all(root.join("modules").join(module)).unwrap();
    }
    std::fs::write(root.join("repo.kdl"), "schema-version 1\nname \"After\"\n").unwrap();
    std::fs::write(
        root.join("image.kdl"),
        r#"schema-version 1

image {
    name "After"
    base "example" { family "fedora" }
    flavours {
        dev
        separate
    }
    modules {
        module "consumer"
        flavour "dev" {
            module "provider"
            module "coinstalled"
        }
        flavour "separate" {
            module "separate"
        }
    }
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("modules/consumer/module.kdl"),
        "schema-version 1\n\ndescription \"Consumes everywhere\"\nsupports \"fedora\"\nafter \"tool\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("modules/provider/module.kdl"),
        "schema-version 1\n\ndescription \"Provides tools\"\nsupports \"fedora\"\nprovides \"tool\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("modules/coinstalled/module.kdl"),
        "schema-version 1\n\ndescription \"Consumes with provider\"\nsupports \"fedora\"\nafter \"tool\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("modules/separate/module.kdl"),
        "schema-version 1\n\ndescription \"Consumes elsewhere\"\nsupports \"fedora\"\nafter \"tool\"\n",
    )
    .unwrap();

    let issues = tect::run(Command::Check, None, &root).issues.plain();
    assert!(
        issues.contains("`consumer` builds after `tool`, which only `provider` on the `dev`"),
        "{issues}"
    );
    assert!(
        issues.contains("`separate` builds after `tool`, which only `provider` on the `dev`"),
        "{issues}"
    );
    assert!(!issues.contains("`coinstalled` builds after"), "{issues}");
    let _ = std::fs::remove_dir_all(root);
}

/// A module `signed` that declares `line` for `secureboot-key`, which nothing in
/// the image provides, listed before a module `plain`.
fn unprovided(dir: &str, line: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(dir);
    let _ = std::fs::remove_dir_all(&root);
    for module in ["signed", "plain"] {
        std::fs::create_dir_all(root.join("modules").join(module))
            .expect("the test target directory is writable");
    }
    std::fs::write(root.join("repo.kdl"), "schema-version 1\nname \"After\"\n")
        .expect("the test target directory is writable");
    std::fs::write(
        root.join("image.kdl"),
        r#"schema-version 1

image {
    name "After"
    base "example" { family "fedora" }
    modules {
        module "signed"
        module "plain"
    }
}
"#,
    )
    .expect("the test target directory is writable");
    std::fs::write(
        root.join("modules/signed/module.kdl"),
        format!(
            "schema-version 1\n\ndescription \"Signs when a key exists\"\nsupports \"fedora\"\n{line} \"secureboot-key\"\n"
        ),
    )
    .expect("the test target directory is writable");
    std::fs::write(
        root.join("modules/plain/module.kdl"),
        "schema-version 1\n\ndescription \"Orders nothing\"\nsupports \"fedora\"\n",
    )
    .expect("the test target directory is writable");
    root
}

/// Whether the graph's capability table marks the `secureboot-key` row as a
/// defect.
fn key_row_is_a_defect(root: &std::path::Path) -> bool {
    tect::run(Command::Graph, None, root)
        .parts
        .into_iter()
        .find_map(|part| match part {
            tect::emit::Part::Table(table) if table.title == "Capabilities" => table
                .rows
                .into_iter()
                .find(|(cells, _)| cells[0] == "secureboot-key")
                .map(|(_, defect)| defect),
            _ => None,
        })
        .expect("the capability table holds a secureboot-key row")
}

/// A module that signs with a key when one exists declares the order without
/// requiring the key's provider.
#[test]
fn after_with_no_provider_orders_nothing() {
    let root = unprovided("after-no-provider", "after");
    assert_eq!(tect::run(Command::Check, None, &root).issues.plain(), "");
    let plan = tect::run(Command::Plan, None, &root).stdout;
    let at = |name: &str| plan.find(&format!("\"path\": \"{name}\"")).unwrap();
    assert!(at("signed") < at("plain"), "{plan}");
    let _ = std::fs::remove_dir_all(root);
}

/// The graph agrees with `check`, which accepts the same `after`.
#[test]
fn graph_does_not_flag_an_after_with_no_provider() {
    let root = unprovided("after-no-provider-graph", "after");
    assert!(!key_row_is_a_defect(&root));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn graph_flags_a_requires_with_no_provider() {
    let root = unprovided("requires-no-provider-graph", "requires");
    assert!(key_row_is_a_defect(&root));
    let _ = std::fs::remove_dir_all(root);
}

/// A module shipping a MAC profile builds after whoever provides the MAC, even
/// though it declares no `requires` for it: the profile is emitted against
/// whatever the image carries, so the module supports families with a different
/// MAC and cannot name one.
#[test]
fn shipped_policy_orders_a_module_after_the_mac_that_installs_it() {
    // `check` reads the catalog, and the one installed on this host may be older.
    std::env::set_var(
        "TECT_ASSETS",
        concat!(env!("CARGO_MANIFEST_DIR"), "/assets"),
    );
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("policy-ordering");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("modules/apparmor")).unwrap();
    std::fs::create_dir_all(root.join("modules/yubikey/apparmor")).unwrap();
    std::fs::write(root.join("repo.kdl"), "schema-version 1\nname \"Mac\"\n").unwrap();
    std::fs::write(
        root.join("image.kdl"),
        r#"schema-version 1

image {
    name "Mac"
    base "example" { family "debian" }
    modules {
        module "yubikey"
        module "apparmor"
    }
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("modules/apparmor/module.kdl"),
        "schema-version 1\n\ndescription \"The MAC\"\nsupports \"debian\"\nprovides \"apparmor-policy\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("modules/yubikey/module.kdl"),
        "schema-version 1\n\ndescription \"Ships a profile and requires nothing\"\nsupports \"debian\"\n",
    )
    .unwrap();
    std::fs::write(root.join("modules/yubikey/apparmor/usr.sbin.pcscd"), "").unwrap();

    let plan = tect::run(Command::Plan, None, &root).stdout;
    let at = |name: &str| plan.find(&format!("\"path\": \"{name}\"")).unwrap();
    assert!(at("apparmor") < at("yubikey"), "{plan}");
    // An ordering edge is not a requirement, so nothing is owed and nothing is
    // said.
    assert_eq!(tect::run(Command::Check, None, &root).issues.plain(), "");

    // And with no MAC in the image at all, the profile is never installed, so
    // there is nothing to order after and still nothing to report.
    std::fs::write(
        root.join("image.kdl"),
        std::fs::read_to_string(root.join("image.kdl"))
            .unwrap()
            .replace("        module \"apparmor\"\n", ""),
    )
    .unwrap();
    assert_eq!(tect::run(Command::Check, None, &root).issues.plain(), "");
    let _ = std::fs::remove_dir_all(root);
}

/// Two providers of one capability coexist, and one provider that declares it
/// exclusive makes the second a conflict.
#[test]
fn a_capability_conflicts_only_where_a_provider_declares_it_exclusive() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("exclusive-provider");
    let _ = std::fs::remove_dir_all(&root);
    for module in ["one", "two"] {
        std::fs::create_dir_all(root.join("modules").join(module)).unwrap();
    }
    std::fs::write(root.join("repo.kdl"), "schema-version 1\nname \"Two\"\n").unwrap();
    std::fs::write(
        root.join("image.kdl"),
        "schema-version 1\n\nimage {\n    name \"Two\"\n    base \"example\" { family \"fedora\" }\n    modules {\n        module \"one\"\n        module \"two\"\n    }\n}\n",
    )
    .unwrap();
    let provider = |name: &str, provides: &str| {
        std::fs::write(
            root.join("modules").join(name).join("module.kdl"),
            format!(
                "schema-version 1\n\ndescription \"Provides\"\nsupports \"fedora\"\n{provides}\n"
            ),
        )
        .unwrap();
    };
    provider("one", "provides \"tool\" file=\"/usr/share/tool\"");
    provider("two", "provides \"tool\" file=\"/usr/share/tool\"");
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    assert!(!issues.contains("more than one enabled module"), "{issues}");

    provider(
        "two",
        "provides \"tool\" file=\"/usr/share/tool\" exclusive=#true",
    );
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    assert!(
        issues.contains("`tool` is provided by more than one enabled module"),
        "{issues}"
    );
    assert!(issues.contains("`two` declares it `exclusive`"), "{issues}");
    let _ = std::fs::remove_dir_all(root);
}

/// Writes a repository whose one image body is `modules`, and one module per
/// `(name, manifest)` pair.
fn written(name: &str, modules: &str, manifests: &[(&str, &str)]) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("the test repository is writable");
    std::fs::write(
        root.join("repo.kdl"),
        "schema-version 1\nname \"Checked\"\n",
    )
    .expect("the test repository is writable");
    std::fs::write(
        root.join("image.kdl"),
        format!(
            "schema-version 1\n\nimage {{\n    name \"Checked\"\n    base \"example\" {{ family \"fedora\" }}\n    flavours {{\n        dev\n        prod\n    }}\n    modules {{\n{modules}\n    }}\n}}\n"
        ),
    )
    .expect("the test repository is writable");
    for (module, manifest) in manifests {
        std::fs::create_dir_all(root.join("modules").join(module))
            .expect("the test repository is writable");
        std::fs::write(
            root.join("modules").join(module).join("module.kdl"),
            format!(
                "schema-version 1\n\ndescription \"Checked\"\nsupports \"fedora\"\n{manifest}\n"
            ),
        )
        .expect("the test repository is writable");
    }
    root
}

/// Answers what `check` says of the repository `written` makes.
fn checked(name: &str, modules: &str, manifests: &[(&str, &str)]) -> String {
    let root = written(name, modules, manifests);
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(root);
    issues
}

#[test]
fn a_module_builds_after_every_provider_of_what_it_requires() {
    let provider = "provides \"tool\" file=\"/usr/share/tool\"";
    let root = written(
        "order-every-provider",
        "        module \"consumer\"\n        module \"one\"\n        module \"two\"",
        &[
            ("consumer", "requires \"tool\""),
            ("one", provider),
            ("two", provider),
        ],
    );
    let plan = tect::run(Command::Plan, None, &root).stdout;
    let at = |name: &str| plan.find(&format!("\"path\": \"{name}\"")).unwrap();
    assert!(at("one") < at("consumer"), "{plan}");
    assert!(at("two") < at("consumer"), "{plan}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_lone_provider_that_declares_exclusive_is_no_conflict() {
    let issues = checked(
        "exclusive-alone",
        "        module \"solo\"",
        &[(
            "solo",
            "provides \"tool\" file=\"/usr/share/tool\" exclusive=#true",
        )],
    );
    assert!(!issues.contains("more than one enabled module"), "{issues}");
}

#[test]
fn exclusive_providers_in_sibling_flavours_never_conflict() {
    let exclusive = "provides \"tool\" file=\"/usr/share/tool\" exclusive=#true";
    let issues = checked(
        "exclusive-siblings",
        "        flavour \"dev\" { module \"one\" }\n        flavour \"prod\" { module \"two\" }",
        &[("one", exclusive), ("two", exclusive)],
    );
    assert!(!issues.contains("more than one enabled module"), "{issues}");
}

#[test]
fn every_flavour_that_holds_a_conflict_is_named_at_once() {
    let provider = "provides \"tool\" file=\"/usr/share/tool\"";
    let exclusive = "provides \"tool\" file=\"/usr/share/tool\" exclusive=#true";
    let issues = checked(
        "exclusive-both-flavours",
        "        flavour \"dev\" {\n            module \"a\"\n            module \"b\"\n        }\n        flavour \"prod\" {\n            module \"c\"\n            module \"d\"\n        }",
        &[("a", exclusive), ("b", provider), ("c", exclusive), ("d", provider)],
    );
    assert!(issues.contains("provided by: a, b, c, d."), "{issues}");
}

/// The ungated `one` builds into the `dev` target beside `two`. The `prod`
/// provider `three` never builds beside the exclusive `two`, so the refusal
/// leaves it out.
#[test]
fn an_exclusive_provider_conflicts_with_an_ungated_one_in_its_flavour() {
    let provider = "provides \"tool\" file=\"/usr/share/tool\"";
    let issues = checked(
        "exclusive-ungated",
        "        module \"one\"\n        flavour \"dev\" { module \"two\" }\n        flavour \"prod\" { module \"three\" }",
        &[
            ("one", provider),
            ("two", "provides \"tool\" file=\"/usr/share/tool\" exclusive=#true"),
            ("three", provider),
        ],
    );
    assert!(
        issues.contains("provided by: one, two. `two` declares it `exclusive`"),
        "{issues}"
    );
}

/// A key kind names one key in the image, so two modules that declare it
/// conflict unless both declare the same key.
#[test]
fn two_keys_of_one_kind_conflict_unless_they_are_the_same_key() {
    let key = |bits: u32| {
        format!(
            "key \"secureboot\" {{\n    generator \"openssl\" profile=\"module-signing\" bits={bits}\n    public \"/usr/share/secureboot/sb_cert.der\" format=\"der\"\n    private \"MOK.priv\"\n}}"
        )
    };
    let modules = "        module \"one\"\n        module \"two\"";
    let same = checked(
        "same-key",
        modules,
        &[("one", &key(4096)), ("two", &key(4096))],
    );
    assert!(!same.contains("more than one enabled module"), "{same}");
    let differ = checked(
        "different-key",
        modules,
        &[("one", &key(4096)), ("two", &key(2048))],
    );
    assert!(
        differ.contains("`secureboot-key` is provided by more than one enabled module"),
        "{differ}"
    );
}

/// Many providers may fill one requirement, and any one that reaches the
/// requiring module's targets satisfies it.
#[test]
fn a_requirement_is_met_by_any_provider_that_reaches_it() {
    let provider = "provides \"tool\" file=\"/usr/share/tool\"";
    let met = checked(
        "requires-any",
        "        module \"consumer\"\n        module \"ungated\"\n        flavour \"dev\" { module \"gated\" }",
        &[
            ("consumer", "requires \"tool\""),
            ("ungated", provider),
            ("gated", provider),
        ],
    );
    assert!(!met.contains("requires `tool`"), "{met}");

    let unmet = checked(
        "requires-elsewhere",
        "        module \"consumer\"\n        flavour \"dev\" { module \"one\" }\n        flavour \"prod\" { module \"two\" }",
        &[
            ("consumer", "requires \"tool\""),
            ("one", provider),
            ("two", provider),
        ],
    );
    assert!(
        unmet.contains("`consumer` requires `tool`, which only `one` on the `dev` flavour and"),
        "{unmet}"
    );
    assert!(
        unmet.contains("`two` on the `prod` flavour provides"),
        "{unmet}"
    );
}

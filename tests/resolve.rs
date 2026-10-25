use std::path::PathBuf;
use tect::Command;

#[test]
fn after_rejects_a_provider_on_another_target() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("after-gated-provider");
    let _ = std::fs::remove_dir_all(&root);
    for component in ["consumer", "provider", "coinstalled", "separate"] {
        std::fs::create_dir_all(root.join("components").join(component)).unwrap();
    }
    std::fs::write(root.join("repo.kdl"), "schema-version 1\nname \"After\"\n").unwrap();
    write_base(&root, "fedora", "");
    std::fs::write(
        root.join("image.kdl"),
        r#"schema-version 1

image {
    name "After"
    base-image {
        example
    }
    flavours {
        dev
        separate
    }
    consumer
    flavour "dev" {
        provider
        coinstalled
    }
    flavour "separate" {
        separate
    }
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("components/consumer/manifest.kdl"),
        "schema-version 1\n\ndescription \"Consumes everywhere\"\ntargets \"fedora\"\nafter \"tool\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("components/provider/manifest.kdl"),
        "schema-version 1\n\ndescription \"Provides tools\"\ntargets \"fedora\"\nprovides \"tool\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("components/coinstalled/manifest.kdl"),
        "schema-version 1\n\ndescription \"Consumes with provider\"\ntargets \"fedora\"\nafter \"tool\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("components/separate/manifest.kdl"),
        "schema-version 1\n\ndescription \"Consumes elsewhere\"\ntargets \"fedora\"\nafter \"tool\"\n",
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

/// A component `signed` that declares `line` for `secureboot-key`, which nothing in
/// the image provides, listed before a component `plain`.
fn unprovided(dir: &str, line: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(dir);
    let _ = std::fs::remove_dir_all(&root);
    for component in ["signed", "plain"] {
        std::fs::create_dir_all(root.join("components").join(component))
            .expect("the test target directory is writable");
    }
    std::fs::write(root.join("repo.kdl"), "schema-version 1\nname \"After\"\n")
        .expect("the test target directory is writable");
    write_base(&root, "fedora", "");
    std::fs::write(
        root.join("image.kdl"),
        r#"schema-version 1

image {
    name "After"
    base-image {
        example
    }
    signed
    plain
}
"#,
    )
    .expect("the test target directory is writable");
    std::fs::write(
        root.join("components/signed/manifest.kdl"),
        format!(
            "schema-version 1\n\ndescription \"Signs when a key exists\"\ntargets \"fedora\"\n{line} \"secureboot-key\"\n"
        ),
    )
    .expect("the test target directory is writable");
    std::fs::write(
        root.join("components/plain/manifest.kdl"),
        "schema-version 1\n\ndescription \"Orders nothing\"\ntargets \"fedora\"\n",
    )
    .expect("the test target directory is writable");
    root
}

/// Answers the cells of the graph's capability table row for `name`, and
/// whether the table marks that row as a defect.
fn capability_row(root: &std::path::Path, name: &str) -> (Vec<String>, bool) {
    tect::run(Command::Graph, None, root)
        .parts
        .into_iter()
        .find_map(|part| match part {
            tect::emit::Part::Table(table) if table.title == "Capabilities" => {
                table.rows.into_iter().find(|(cells, _)| cells[0] == name)
            }
            _ => None,
        })
        .expect("the capability table holds the row")
}

/// Whether the graph's capability table marks the `secureboot-key` row as a
/// defect.
fn key_row_is_a_defect(root: &std::path::Path) -> bool {
    capability_row(root, "secureboot-key").1
}

/// A component that signs with a key when one exists declares the order without
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

/// A component shipping a MAC profile builds after whoever provides the MAC, even
/// though it declares no `requires` for it: the profile is emitted against
/// whatever the image carries, so the component targets families with a different
/// MAC and cannot name one.
#[test]
fn shipped_policy_orders_a_component_after_the_mac_that_installs_it() {
    // `check` reads the catalog, and the one installed on this host may be older.
    std::env::set_var(
        "TECT_ASSETS",
        concat!(env!("CARGO_MANIFEST_DIR"), "/assets"),
    );
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("policy-ordering");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("components/apparmor")).unwrap();
    std::fs::create_dir_all(root.join("components/yubikey/apparmor")).unwrap();
    std::fs::write(root.join("repo.kdl"), "schema-version 1\nname \"Mac\"\n").unwrap();
    write_base(&root, "debian", "");
    std::fs::write(
        root.join("image.kdl"),
        r#"schema-version 1

image {
    name "Mac"
    base-image {
        example
    }
    yubikey
    apparmor
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("components/apparmor/manifest.kdl"),
        "schema-version 1\n\ndescription \"The MAC\"\ntargets \"debian\"\nprovides \"apparmor-policy\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("components/yubikey/manifest.kdl"),
        "schema-version 1\n\ndescription \"Ships a profile and requires nothing\"\ntargets \"debian\"\n",
    )
    .unwrap();
    std::fs::write(root.join("components/yubikey/apparmor/usr.sbin.pcscd"), "").unwrap();

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
            .replace("        apparmor\n", ""),
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
    for component in ["one", "two"] {
        std::fs::create_dir_all(root.join("components").join(component)).unwrap();
    }
    std::fs::write(root.join("repo.kdl"), "schema-version 1\nname \"Two\"\n").unwrap();
    write_base(&root, "fedora", "");
    std::fs::write(
        root.join("image.kdl"),
        "schema-version 1\n\nimage {\n    name \"Two\"\n    base-image {\n        example\n    }\n    one\n    two\n}\n",
    )
    .unwrap();
    let provider = |name: &str, provides: &str| {
        std::fs::write(
            root.join("components").join(name).join("manifest.kdl"),
            format!(
                "schema-version 1\n\ndescription \"Provides\"\ntargets \"fedora\"\n{provides}\n"
            ),
        )
        .unwrap();
    };
    provider("one", "provides \"tool\" witness=\"/usr/share/tool\"");
    provider("two", "provides \"tool\" witness=\"/usr/share/tool\"");
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    assert!(
        !issues.contains("more than one enabled component"),
        "{issues}"
    );

    provider(
        "two",
        "provides \"tool\" witness=\"/usr/share/tool\" exclusive=#true",
    );
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    assert!(
        issues.contains("`tool` is provided by more than one enabled component"),
        "{issues}"
    );
    assert!(issues.contains("`two` declares it `exclusive`"), "{issues}");
    let _ = std::fs::remove_dir_all(root);
}

/// Writes a repository whose one image body is `components`, and one component per
/// `(name, manifest)` pair.
fn written(name: &str, components: &str, manifests: &[(&str, &str)]) -> PathBuf {
    written_on(name, "", components, manifests)
}

/// Writes the repository `written` makes, on a base whose block also holds
/// `base`.
fn written_on(name: &str, base: &str, components: &str, manifests: &[(&str, &str)]) -> PathBuf {
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
            "schema-version 1\n\nimage {{\n    name \"Checked\"\n    base-image {{ example }}\n    flavours {{\n        dev\n        prod\n    }}\n{components}\n}}\n"
        ),
    )
    .expect("the test repository is writable");
    write_base(&root, "fedora", &base.replace("; ", "\n"));
    for (component, manifest) in manifests {
        std::fs::create_dir_all(root.join("components").join(component))
            .expect("the test repository is writable");
        std::fs::write(
            root.join("components").join(component).join("manifest.kdl"),
            format!(
                "schema-version 1\n\ndescription \"Checked\"\ntargets \"fedora\"\n{manifest}\n"
            ),
        )
        .expect("the test repository is writable");
    }
    root
}

/// Answers what `check` says of the repository `written` makes.
fn checked(name: &str, components: &str, manifests: &[(&str, &str)]) -> String {
    let root = written(name, components, manifests);
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(root);
    issues
}

/// Answers what `check` says of the repository `written_on` makes.
fn checked_on(name: &str, base: &str, components: &str, manifests: &[(&str, &str)]) -> String {
    let root = written_on(name, base, components, manifests);
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(root);
    issues
}

#[test]
fn a_component_builds_after_every_provider_of_what_it_requires() {
    let provider = "provides \"tool\" witness=\"/usr/share/tool\"";
    let root = written(
        "order-every-provider",
        "        consumer\n        one\n        two",
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
        "        solo",
        &[(
            "solo",
            "provides \"tool\" witness=\"/usr/share/tool\" exclusive=#true",
        )],
    );
    assert!(
        !issues.contains("more than one enabled component"),
        "{issues}"
    );
}

#[test]
fn exclusive_providers_in_sibling_flavours_never_conflict() {
    let exclusive = "provides \"tool\" witness=\"/usr/share/tool\" exclusive=#true";
    let issues = checked(
        "exclusive-siblings",
        "        flavour \"dev\" { one }\n        flavour \"prod\" { two }",
        &[("one", exclusive), ("two", exclusive)],
    );
    assert!(
        !issues.contains("more than one enabled component"),
        "{issues}"
    );
}

#[test]
fn every_flavour_that_holds_a_conflict_is_named_at_once() {
    let provider = "provides \"tool\" witness=\"/usr/share/tool\"";
    let exclusive = "provides \"tool\" witness=\"/usr/share/tool\" exclusive=#true";
    let issues = checked(
        "exclusive-both-flavours",
        "        flavour \"dev\" {\n            a\n            b\n        }\n        flavour \"prod\" {\n            c\n            d\n        }",
        &[("a", exclusive), ("b", provider), ("c", exclusive), ("d", provider)],
    );
    assert!(issues.contains("provided by: a, b, c, d."), "{issues}");
}

/// The ungated `one` builds into the `dev` target beside `two`. The `prod`
/// provider `three` never builds beside the exclusive `two`, so the refusal
/// leaves it out.
#[test]
fn an_exclusive_provider_conflicts_with_an_ungated_one_in_its_flavour() {
    let provider = "provides \"tool\" witness=\"/usr/share/tool\"";
    let issues = checked(
        "exclusive-ungated",
        "        one\n        flavour \"dev\" { two }\n        flavour \"prod\" { three }",
        &[
            ("one", provider),
            (
                "two",
                "provides \"tool\" witness=\"/usr/share/tool\" exclusive=#true",
            ),
            ("three", provider),
        ],
    );
    assert!(
        issues.contains("provided by: one, two. `two` declares it `exclusive`"),
        "{issues}"
    );
}

/// A key kind names one key in the image, so two components that declare it
/// conflict unless both declare the same key.
#[test]
fn two_keys_of_one_kind_conflict_unless_they_are_the_same_key() {
    let key = |bits: u32| {
        format!(
            "key \"secureboot\" {{\n    generator \"openssl\" profile=\"module-signing\" bits={bits}\n    public \"/usr/share/secureboot/sb_cert.der\" format=\"der\"\n    private \"MOK.priv\"\n}}"
        )
    };
    let components = "        one\n        two";
    let same = checked(
        "same-key",
        components,
        &[("one", &key(4096)), ("two", &key(4096))],
    );
    assert!(!same.contains("more than one enabled component"), "{same}");
    let differ = checked(
        "different-key",
        components,
        &[("one", &key(4096)), ("two", &key(2048))],
    );
    assert!(
        differ.contains("`secureboot-key` is provided by more than one enabled component"),
        "{differ}"
    );
}

/// Many providers may fill one requirement, and any one that reaches the
/// requiring component's targets satisfies it.
#[test]
fn a_requirement_is_met_by_any_provider_that_reaches_it() {
    let provider = "provides \"tool\" witness=\"/usr/share/tool\"";
    let met = checked(
        "requires-any",
        "        consumer\n        ungated\n        flavour \"dev\" { gated }",
        &[
            ("consumer", "requires \"tool\""),
            ("ungated", provider),
            ("gated", provider),
        ],
    );
    assert!(!met.contains("requires `tool`"), "{met}");

    let unmet = checked(
        "requires-elsewhere",
        "        consumer\n        flavour \"dev\" { one }\n        flavour \"prod\" { two }",
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

/// Answers what `plan` prints for the repository `written_on` makes.
fn planned(name: &str, base: &str, components: &str, manifests: &[(&str, &str)]) -> String {
    let root = written_on(name, base, components, manifests);
    let plan = tect::run(Command::Plan, None, &root).stdout;
    let _ = std::fs::remove_dir_all(root);
    plan
}

#[test]
fn an_imported_component_builds_and_records_its_importer() {
    let root = written(
        "imports-builds",
        "        consumer",
        &[
            ("consumer", "requires \"tool\"\nimports \"provider\""),
            ("provider", "provides \"tool\" witness=\"/usr/share/tool\""),
        ],
    );
    let plan = tect::run(Command::Plan, None, &root).stdout;
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(root);
    let at = |name: &str| plan.find(&format!("\"path\": \"{name}\"")).unwrap();
    assert!(at("provider") < at("consumer"), "{plan}");
    let flat = squashed(&plan);
    assert!(
        flat.contains(
            r#""met_by":{"tool":{"how":"imported","provider":"provider","by":"consumer"}}"#
        ),
        "{plan}"
    );
    assert!(
        flat.contains(r#""origin":{"how":"imported","by":"consumer"}"#),
        "{plan}"
    );
    assert!(issues.is_empty(), "{issues}");
}

/// The provider sits under `components/`, and no entry and no import names it.
#[test]
fn an_unmet_requirement_fails_while_its_provider_sits_unlisted() {
    let root = written(
        "imports-unlisted",
        "        consumer",
        &[
            ("consumer", "requires \"tool\""),
            ("provider", "provides \"tool\" witness=\"/usr/share/tool\""),
        ],
    );
    let plan = tect::run(Command::Plan, None, &root).stdout;
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(root);
    assert!(!plan.contains("\"path\": \"provider\""), "{plan}");
    assert!(
        issues.contains("`consumer` requires `tool`, which nothing enabled provides"),
        "{issues}"
    );
    assert!(
        issues.contains("`provider` would satisfy it; add it to this image"),
        "{issues}"
    );
}

#[test]
fn a_fact_the_base_provides_meets_a_requirement() {
    let issues = checked_on(
        "imports-base-fact",
        "; provides \"tool\"",
        "        consumer",
        &[("consumer", "requires \"tool\"")],
    );
    assert!(!issues.contains("requires `tool`"), "{issues}");
}

/// Every manifest `written` makes declares `targets "fedora"`. The base
/// here names no family, so the import skips that check.
#[test]
fn a_base_with_no_family_imports_a_component_that_names_one() {
    let root = written_on(
        "imports-no-family",
        "; imports \"provider\"",
        "        consumer",
        &[
            ("consumer", "requires \"tool\""),
            ("provider", "provides \"tool\" witness=\"/usr/share/tool\""),
        ],
    );
    let base = root.join("components/base-image/example/manifest.kdl");
    let text = std::fs::read_to_string(&base).expect("the test repository is readable");
    assert!(text.contains(" { family \"fedora\"; }"), "{text}");
    std::fs::write(&base, text.replace(" { family \"fedora\"; }", ""))
        .expect("the test repository is writable");
    let plan = tect::run(Command::Plan, None, &root).stdout;
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(root);
    assert!(plan.contains("\"path\": \"provider\""), "{plan}");
    assert!(!issues.contains("requires `tool`"), "{issues}");
}

/// The base covers all that `helper` provides, which suppresses a component
/// that nothing names by path.
#[test]
fn a_component_required_by_its_path_is_not_suppressed() {
    let root = written_on(
        "suppress-by-path",
        "; provides \"flatpak\"",
        "        consumer\n        helper",
        &[
            ("consumer", "requires \"helper\""),
            ("helper", "provides \"flatpak\""),
        ],
    );
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(root);
    assert!(!issues.contains("requires `helper`"), "{issues}");
}

#[test]
fn a_component_meets_a_requirement_for_its_own_path() {
    let components = "        consumer\n        helper";
    let manifests = [("consumer", "requires \"helper\""), ("helper", "")];
    let issues = checked("closure-self-checked", components, &manifests);
    assert!(!issues.contains("requires `helper`"), "{issues}");
    let plan = planned("closure-self", "", components, &manifests);
    assert!(
        squashed(&plan).contains(r#""met_by":{"helper":{"how":"listed","provider":"helper"}}"#),
        "{plan}"
    );
}

/// The hierarchy belongs to declared names. The directory `rpm/` above the
/// component at `rpm/dnf5` organises the tree and names no capability.
#[test]
fn a_declared_name_meets_its_ancestor_and_a_component_path_does_not() {
    let declared = checked(
        "hierarchy-declared",
        "        consumer\n        manager",
        &[
            ("consumer", "requires \"rpm\""),
            ("manager", "provides \"rpm/dnf5\""),
        ],
    );
    assert!(!declared.contains("requires `rpm`"), "{declared}");

    let pathed = checked(
        "hierarchy-path",
        "        consumer\n        rpm {\n            dnf5\n        }",
        &[("consumer", "requires \"rpm\""), ("rpm/dnf5", "")],
    );
    assert!(
        pathed.contains("`consumer` requires `rpm`, which nothing enabled provides"),
        "{pathed}"
    );
}

#[test]
fn two_unlisted_candidates_fail_naming_both() {
    let provider = "provides \"tool\" witness=\"/usr/share/tool\"";
    let root = written(
        "closure-ambiguous",
        "        consumer",
        &[
            ("consumer", "requires \"tool\""),
            ("one", provider),
            ("two", provider),
        ],
    );
    let run = tect::run(Command::Generate, None, &root);
    let _ = std::fs::remove_dir_all(root);
    let issues = run.issues.plain();
    assert!(
        issues.contains("`consumer` requires `tool`, which nothing enabled provides"),
        "{issues}"
    );
    assert!(
        issues.contains("`one` and `two` each provide it"),
        "{issues}"
    );
}

/// The `dev` importer gates the provider to `dev`. The `prod` importer then
/// imports it too, so the provider builds into every target.
#[test]
fn a_component_imported_in_two_flavours_builds_ungated() {
    let plan = planned(
        "imports-widens",
        "",
        "        flavour \"dev\" { one }\n        flavour \"prod\" { two }",
        &[
            ("one", "imports \"provider\""),
            ("two", "imports \"provider\""),
            ("provider", "provides \"tool\" witness=\"/usr/share/tool\""),
        ],
    );
    let entry = &plan[plan.find("\"path\": \"provider\"").unwrap()..];
    let flavour = entry.lines().nth(1).unwrap();
    assert!(flavour.contains("\"flavour\": null"), "{plan}");
}

/// The `dev` import of `provider` imports `lib` for `dev` too. Widening
/// `provider` for `prod` must widen what it imports as well.
#[test]
fn a_widened_import_widens_what_it_imports() {
    let plan = planned(
        "imports-widens-chain",
        "",
        "        flavour \"dev\" { one }\n        flavour \"prod\" { two }",
        &[
            ("one", "imports \"provider\""),
            ("two", "imports \"provider\""),
            ("provider", "imports \"lib\""),
            ("lib", "provides \"lib\" witness=\"/usr/share/lib\""),
        ],
    );
    let entry = &plan[plan.find("\"path\": \"lib\"").unwrap()..];
    let flavour = entry.lines().nth(1).unwrap();
    assert!(flavour.contains("\"flavour\": null"), "{plan}");
}

#[test]
fn a_base_import_builds_in_the_image() {
    let plan = planned(
        "imports-base",
        "; imports \"provider\"",
        "        consumer",
        &[
            ("consumer", "requires \"tool\""),
            ("provider", "provides \"tool\" witness=\"/usr/share/tool\""),
        ],
    );
    assert!(
        squashed(&plan).contains(r#""origin":{"how":"imported","by":"base-image/example"}"#),
        "{plan}"
    );
}

#[test]
fn a_component_imported_twice_in_one_flavour_stays_gated() {
    let plan = planned(
        "imports-stays",
        "",
        "        flavour \"dev\" {\n            one\n            two\n        }",
        &[
            ("one", "imports \"provider\""),
            ("two", "imports \"provider\""),
            ("provider", "provides \"tool\" witness=\"/usr/share/tool\""),
        ],
    );
    let entry = &plan[plan.find("\"path\": \"provider\"").unwrap()..];
    let flavour = entry.lines().nth(1).unwrap();
    assert!(flavour.contains("\"flavour\": \"dev\""), "{plan}");
}

#[test]
fn an_entry_at_an_imported_path_fails_naming_the_importer() {
    let issues = checked(
        "imports-listed",
        "        consumer\n        provider",
        &[("consumer", "imports \"provider\""), ("provider", "")],
    );
    assert!(
        issues.contains("`provider` is listed in the image, and `consumer` imports it"),
        "{issues}"
    );
}

#[test]
fn an_imported_path_missing_from_components_fails_naming_the_importer() {
    let issues = checked(
        "imports-missing",
        "        consumer",
        &[("consumer", "imports \"absent\"")],
    );
    assert!(
        issues.contains("`consumer` imports `absent`, which is not under components/"),
        "{issues}"
    );
    assert!(
        issues.contains("`tect import component absent`"),
        "{issues}"
    );
}

#[test]
fn an_import_cycle_is_refused() {
    let issues = checked(
        "imports-cycle",
        "        one",
        &[("one", "imports \"two\""), ("two", "imports \"one\"")],
    );
    assert!(
        issues.contains("`one` and `two` import each other in a cycle"),
        "{issues}"
    );
    assert!(!issues.contains("is listed in the image"), "{issues}");
}

#[test]
fn a_self_import_is_named_as_one() {
    let issues = checked("imports-self", "        one", &[("one", "imports \"one\"")]);
    assert!(issues.contains("`one` imports itself"), "{issues}");
}

/// `consumer` requires nothing of `provider`, so only the entry position
/// orders the two layers.
#[test]
fn an_import_builds_before_the_component_that_imports_it() {
    let plan = planned(
        "imports-order-entry",
        "",
        "        consumer",
        &[("consumer", "imports \"provider\""), ("provider", "")],
    );
    let at = |name: &str| plan.find(&format!("\"path\": \"{name}\"")).unwrap();
    assert!(at("provider") < at("consumer"), "{plan}");
}

/// `c` arrives through `a` and `d` through `b`, so neither one's first
/// importer lies on the cycle between them.
#[test]
fn an_import_cycle_reached_through_a_second_importer_is_refused() {
    let issues = checked(
        "imports-cycle-second",
        "        a\n        b",
        &[
            ("a", "imports \"c\""),
            ("b", "imports \"d\""),
            ("c", "imports \"d\""),
            ("d", "imports \"c\""),
        ],
    );
    assert!(issues.contains("import each other in a cycle"), "{issues}");
}

#[test]
fn an_import_cycle_names_every_member() {
    let issues = checked(
        "imports-cycle-three",
        "        one",
        &[
            ("one", "imports \"two\""),
            ("two", "imports \"three\""),
            ("three", "imports \"one\""),
        ],
    );
    assert!(
        issues.contains("`one`, `two` and `three` import each other in a cycle"),
        "{issues}"
    );
}

/// `consumer` requires nothing of `provider`, so only the entry position
/// orders the two layers.
#[test]
fn a_base_import_builds_before_the_listed_components() {
    let plan = planned(
        "imports-order",
        "; imports \"provider\"",
        "        consumer",
        &[("consumer", ""), ("provider", "")],
    );
    let at = |name: &str| plan.find(&format!("\"path\": \"{name}\"")).unwrap();
    assert!(at("provider") < at("consumer"), "{plan}");
}

#[test]
fn an_option_import_left_empty_is_refused() {
    let issues = checked(
        "imports-empty",
        "        consumer",
        &[(
            "consumer",
            "imports \"$runtime\"\noption \"runtime\" type=\"string\"",
        )],
    );
    assert!(
        issues.contains("`consumer` imports a path that its option leaves empty"),
        "{issues}"
    );
}

#[test]
fn the_graph_draws_every_importer_and_met_by_names_one() {
    let root = written(
        "imports-graph",
        "        one\n        two",
        &[
            ("one", "requires \"driver\"\nimports \"runtime\""),
            ("two", "imports \"runtime\""),
            ("runtime", "provides \"driver\""),
        ],
    );
    let graph = squashed(&tect::run(Command::GraphJson, None, &root).stdout);
    let plan = squashed(&tect::run(Command::Plan, None, &root).stdout);
    let _ = std::fs::remove_dir_all(root);
    assert!(
        graph.contains(r#"{"component":"runtime","by":"one"}"#)
            && graph.contains(r#"{"component":"runtime","by":"two"}"#),
        "{graph}"
    );
    assert!(
        plan.contains(r#""met_by":{"driver":{"how":"imported","provider":"runtime","by":"one"}}"#),
        "{plan}"
    );
}

/// The base imports one build environment, so a second one that the image
/// lists is a second provider of an exclusive capability.
#[test]
fn a_listed_provider_clashes_with_an_exclusive_one_the_base_imports() {
    let exclusive = "provides \"build-environment\" exclusive=#true";
    let issues = checked_on(
        "imports-exclusive",
        "; imports \"fedora-env\"",
        "        deb-env",
        &[("fedora-env", exclusive), ("deb-env", exclusive)],
    );
    assert!(
        issues.contains("more than one enabled component"),
        "{issues}"
    );
}

/// `runtime` declares the option that both importers set.
const RUNTIME: &str = "option \"driver\" type=\"string\" { default \"overlay\"; }";

#[test]
fn an_option_an_importer_passes_reaches_the_imported_component() {
    let plan = planned(
        "imports-passes",
        "",
        "        consumer",
        &[
            ("consumer", "imports { \"runtime\" { driver \"vfs\"; }; }"),
            ("runtime", RUNTIME),
        ],
    );
    assert!(
        squashed(&plan).contains(r#""options":{"OPT_DRIVER":"vfs"}"#),
        "{plan}"
    );
}

#[test]
fn two_importers_passing_one_value_agree() {
    let issues = checked(
        "imports-agree",
        "        one\n        two",
        &[
            ("one", "imports { \"runtime\" { driver \"vfs\"; }; }"),
            ("two", "imports { \"runtime\" { driver \"vfs\"; }; }"),
            ("runtime", RUNTIME),
        ],
    );
    assert!(!issues.contains("pass different values"), "{issues}");
}

#[test]
fn an_option_block_after_paths_on_the_line_is_refused() {
    let issues = checked(
        "imports-block-several",
        "        consumer",
        &[
            ("consumer", "imports \"runtime\" { driver \"vfs\"; }"),
            ("runtime", RUNTIME),
        ],
    );
    assert!(
        issues.contains("`imports` writes names on its line and a block"),
        "{issues}"
    );
}

/// The importer targets fedora, and what it imports targets debian alone.
#[test]
fn an_imported_component_off_the_family_names_its_importer() {
    let root = written(
        "imports-off-family",
        "        consumer",
        &[("consumer", "imports \"deb-only\""), ("deb-only", "")],
    );
    let manifest = root.join("components/deb-only/manifest.kdl");
    let text = std::fs::read_to_string(&manifest).expect("the test repository is readable");
    std::fs::write(
        &manifest,
        text.replace("targets \"fedora\"", "targets \"debian\""),
    )
    .expect("the test repository is writable");
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(root);
    assert!(
        issues.contains("`deb-only` does not support the `fedora` base family"),
        "{issues}"
    );
    assert!(issues.contains("`consumer` imports it"), "{issues}");
}

#[test]
fn two_importers_passing_different_values_fail_naming_both() {
    let issues = checked(
        "imports-disagree",
        "        one\n        two",
        &[
            ("one", "imports { \"runtime\" { driver \"vfs\"; }; }"),
            ("two", "imports { \"runtime\" { driver \"btrfs\"; }; }"),
            ("runtime", RUNTIME),
        ],
    );
    assert!(
        issues.contains("`two` and `one` pass different values for option `driver` of `runtime`"),
        "{issues}"
    );
}

/// The asking component comes first, so a lookup that takes the wrong node
/// names the asking component.
#[test]
fn the_graph_names_the_path_or_descendant_that_meets_a_requirement() {
    let root = written(
        "graph-meets",
        "        consumer\n        helper\n        manager",
        &[
            ("consumer", "requires \"helper\"\nrequires \"rpm\""),
            ("helper", ""),
            ("manager", "provides \"rpm/dnf5\""),
        ],
    );
    let helper = capability_row(&root, "helper").0;
    let rpm = capability_row(&root, "rpm").0;
    let _ = std::fs::remove_dir_all(root);
    assert_eq!(helper[2], "helper");
    assert_eq!(rpm[2], "manager");
}

#[test]
fn a_requirement_cycle_is_refused() {
    let issues = checked(
        "requires-cycle",
        "        one\n        two",
        &[("one", "requires \"two\""), ("two", "requires \"one\"")],
    );
    assert!(
        issues.contains("the component graph has a cycle"),
        "{issues}"
    );
}

/// Drops the whitespace of a JSON read-out, so an assertion does not depend
/// on its indentation.
fn squashed(json: &str) -> String {
    json.split_whitespace().collect()
}

/// `consumer` imports the component its `runtime` option names, whose default
/// is `one`.
const CHOOSER: &str = "requires \"tool\"\nimports \"$runtime\"\noption \"runtime\" type=\"string\" { default \"one\"; }";

#[test]
fn an_option_import_takes_the_path_the_image_sets() {
    let provider = "provides \"tool\" witness=\"/usr/share/tool\"";
    let plan = planned(
        "imports-chosen",
        "",
        "        consumer { runtime \"two\"; }",
        &[("consumer", CHOOSER), ("one", provider), ("two", provider)],
    );
    assert!(!plan.contains("\"path\": \"one\""), "{plan}");
    assert!(
        squashed(&plan)
            .contains(r#""met_by":{"tool":{"how":"imported","provider":"two","by":"consumer"}}"#),
        "{plan}"
    );
}

#[test]
fn an_option_import_takes_its_default_where_the_image_sets_none() {
    let provider = "provides \"tool\" witness=\"/usr/share/tool\"";
    let plan = planned(
        "imports-chosen-default",
        "",
        "        consumer",
        &[("consumer", CHOOSER), ("one", provider), ("two", provider)],
    );
    assert!(plan.contains("\"path\": \"one\""), "{plan}");
    assert!(!plan.contains("\"path\": \"two\""), "{plan}");
}

#[test]
fn an_option_import_that_names_no_component_fails_naming_the_path() {
    let provider = "provides \"tool\" witness=\"/usr/share/tool\"";
    let issues = checked(
        "imports-chosen-missing",
        "        consumer { runtime \"three\"; }",
        &[("consumer", CHOOSER), ("one", provider), ("two", provider)],
    );
    assert!(
        issues.contains("`consumer` imports `three`, which is not under components/"),
        "{issues}"
    );
}

#[test]
fn a_reference_to_an_option_that_is_not_a_string_fails_at_its_node() {
    let issues = checked(
        "reference-not-string",
        "        consumer",
        &[(
            "consumer",
            "option \"fast\" type=\"bool\" { default #false; }\npackages \"tool=$fast\" \"other=$absent\"",
        )],
    );
    assert!(issues.contains("`$fast` reads a bool option"), "{issues}");
    assert!(
        issues.contains("`$absent` names no option of this component"),
        "{issues}"
    );
}

/// The image sets the version. The fedora family spells it with a dash.
#[test]
fn an_image_entry_version_reaches_the_package_line() {
    let root = written(
        "version-reaches-packages",
        "        browser { version \"130.0\"; }\n        adapter",
        &[
            (
                "browser",
                "option \"version\" type=\"string\" { default \"1\"; }\npackages \"firefox=$version\"",
            ),
            ("adapter", "provides \"build-environment\""),
        ],
    );
    let run = tect::run(Command::Generate, None, &root);
    let _ = std::fs::remove_dir_all(root);
    let script = run
        .files
        .iter()
        .find(|(path, _)| path.ends_with("components/browser.sh"))
        .map(|(_, text)| text.as_str())
        .unwrap_or_else(|| panic!("{}", run.issues.plain()));
    assert!(
        script.contains("install_packages 'firefox-130.0'"),
        "{script}"
    );
}

#[test]
fn an_option_value_that_spoils_a_package_name_fails_at_the_entry() {
    let issues = checked(
        "version-spoils-packages",
        "        browser { version \"1 2\"; }",
        &[(
            "browser",
            "option \"version\" type=\"string\" { default \"1\"; }\npackages \"firefox=$version\"",
        )],
    );
    assert!(
        issues.contains("`browser` installs `firefox=$version` as `firefox=1 2`"),
        "{issues}"
    );
}

/// `remover` takes away `tool`, which `provider` ships.
const PROVIDER: (&str, &str) = ("provider", "provides \"tool\" witness=\"/usr/share/tool\"");
const REMOVER: (&str, &str) = ("remover", "removes \"tool\"");

#[test]
fn a_component_that_requires_a_removed_name_fails_naming_the_remover() {
    for (name, components) in [
        (
            "removes-before",
            "        remover\n        provider\n        consumer",
        ),
        (
            "removes-after",
            "        provider\n        consumer\n        remover",
        ),
    ] {
        let issues = checked(
            name,
            components,
            &[PROVIDER, REMOVER, ("consumer", "requires \"tool\"")],
        );
        assert!(
            issues.contains("`consumer` requires `tool`, which `remover` removes"),
            "{issues}"
        );
    }
}

#[test]
fn a_removes_that_nothing_provides_fails() {
    let issues = checked("removes-nothing", "        remover", &[REMOVER]);
    assert!(
        issues.contains("`remover` removes `tool`, which nothing in the image provides"),
        "{issues}"
    );
}

#[test]
fn a_remover_builds_after_the_provider_and_its_witness_must_be_absent() {
    let plan = planned(
        "removes-order",
        "",
        "        remover\n        provider",
        &[PROVIDER, REMOVER],
    );
    let at = |name: &str| plan.find(&format!("\"path\": \"{name}\"")).unwrap();
    assert!(at("provider") < at("remover"), "{plan}");
    assert!(
        squashed(&plan).contains(r#""contract_files":["!/usr/share/tool"]"#),
        "{plan}"
    );
}

#[test]
fn a_removed_base_name_is_no_longer_claimed() {
    let plan = planned(
        "removes-base",
        "; provides \"bootupctl\" witness=\"/usr/bin/bootupctl\"",
        "        remover",
        &[("remover", "removes \"bootupctl\"")],
    );
    let root = written_on(
        "removes-base-check",
        "; provides \"bootupctl\" witness=\"/usr/bin/bootupctl\"",
        "        remover",
        &[("remover", "removes \"bootupctl\"")],
    );
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(root);
    assert!(
        !issues.contains("nothing in the image provides"),
        "{issues}"
    );
    assert!(
        squashed(&plan).contains(r#""contract_files":["!/usr/bin/bootupctl"]"#),
        "{plan}"
    );
}

/// Writes the base component an image here builds on. Its image reference is
/// `example`, and `extra` adds manifest lines.
fn write_base(root: &std::path::Path, family: &str, extra: &str) {
    let dir = root.join("components/base-image/example");
    std::fs::create_dir_all(&dir).expect("the test repository is writable");
    std::fs::write(
        dir.join("manifest.kdl"),
        format!("schema-version 1\n\nbase \"example\" {{ family \"{family}\"; }}\n{extra}\n"),
    )
    .expect("the test repository is writable");
}

/// Writes a repository whose one image lists `bare`, a component with no
/// manifest that holds `files`. `adapter` meets the family adapter
/// requirement that a package step raises.
fn bare_component(name: &str, files: &[&str]) -> PathBuf {
    let root = written(
        name,
        "        adapter\n        bare",
        &[("adapter", "provides \"build-environment\"")],
    );
    for file in files {
        let at = root.join("components/bare").join(file);
        let made = match file.strip_suffix('/') {
            Some(_) => std::fs::create_dir_all(at.join("etc")),
            None => at
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&at, "true\n")),
        };
        made.expect("the test repository is writable");
    }
    root
}

/// Generates the repository and returns every file it writes, joined.
fn generated(root: &std::path::Path) -> String {
    let run = tect::run(Command::Generate, None, root);
    assert!(run.issues.is_empty(), "{}", run.issues.plain());
    run.files.into_iter().map(|(_, text)| text).collect()
}

#[test]
fn a_component_with_one_script_and_no_manifest_builds() {
    for (file, line) in [
        ("files/", "cp -rT /ctx/components/bare/files /"),
        ("repo.sh", "source /ctx/components/bare/repo.sh"),
        (
            "packages.sh",
            "source /ctx/lib/family.sh\n\nsource /ctx/components/bare/packages.sh",
        ),
        ("component.sh", "source /ctx/components/bare/component.sh"),
        ("finalize.sh", "source /ctx/components/bare/finalize.sh"),
    ] {
        let root = bare_component(&format!("bare-{}", file.trim_end_matches('/')), &[file]);
        let out = generated(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert!(out.contains(line), "{file}: {out}");
    }
}

/// The package step installs the declared packages, then runs each
/// `packages.sh` from the component root to the most specific family.
#[test]
fn packages_sh_runs_after_the_declared_packages_from_broad_to_specific() {
    let root = bare_component(
        "packages-sh-order",
        &[
            "packages.sh",
            "rpm/packages.sh",
            "fedora/packages.sh",
            "debian/packages.sh",
        ],
    );
    std::fs::write(
        root.join("components/bare/manifest.kdl"),
        "schema-version 1

packages \"tool\"\n",
    )
    .unwrap();
    let out = generated(&root);
    let _ = std::fs::remove_dir_all(&root);
    let at = |line: &str| out.find(line).unwrap_or_else(|| panic!("{line}: {out}"));
    assert!(at("install_packages 'tool'") < at("source /ctx/components/bare/packages.sh"));
    assert!(
        at("source /ctx/components/bare/packages.sh")
            < at("source /ctx/components/bare/rpm/packages.sh")
    );
    assert!(
        at("source /ctx/components/bare/rpm/packages.sh")
            < at("source /ctx/components/bare/fedora/packages.sh")
    );
    assert!(!out.contains("debian/packages.sh"), "{out}");
}

#[test]
fn why_names_the_component_that_imported_this_one() {
    let root = written(
        "why-imported",
        "        consumer",
        &[("consumer", "imports \"provider\""), ("provider", "")],
    );
    let imported = tect::run(Command::Why, Some("provider"), &root).stdout;
    let listed = tect::run(Command::Why, Some("consumer"), &root).stdout;
    let _ = std::fs::remove_dir_all(root);
    assert!(
        imported.contains("`consumer` imports it into `checked`."),
        "{imported}"
    );
    assert!(!listed.contains("imports it into"), "{listed}");
}

/// `desktop` is a component and holds `desktop/kde`. `group` is only a directory.
#[test]
fn listing_a_child_includes_the_component_that_holds_it() {
    let root = written(
        "nesting",
        "        desktop { kde }\n        group {\n            tool\n        }",
        &[("desktop", ""), ("desktop/kde", ""), ("group/tool", "")],
    );
    let plan = tect::run(Command::Plan, None, &root).stdout;
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let parent = tect::run(Command::Plan, None, &{
        std::fs::write(
            root.join("image.kdl"),
            "schema-version 1\n\nimage {\n    name \"Checked\"\n    base-image {\n        example\n    }\n    desktop\n}\n",
        )
        .expect("the test repository is writable");
        root.clone()
    })
    .stdout;
    let _ = std::fs::remove_dir_all(&root);
    assert!(issues.is_empty(), "{issues}");
    let at = |name: &str| plan.find(&format!("\"path\": \"{name}\"")).unwrap();
    assert!(at("desktop") < at("desktop/kde"), "{plan}");
    assert!(!plan.contains("\"path\": \"group\""), "{plan}");
    assert!(!parent.contains("\"path\": \"desktop/kde\""), "{parent}");
}

/// The base entry sets the base's `version` option and pins a digest.
#[test]
fn a_base_component_resolves_its_reference_from_the_entry() {
    let root = written("base-component", "", &[]);
    std::fs::write(
        root.join("components/base-image/example/manifest.kdl"),
        "schema-version 1\n\nprovides \"bootc\"\nbase \"quay.io/fedora/fedora-bootc:$version\" { family \"fedora\"; }\noption \"version\" type=\"string\" { default \"44\"; }\n",
    )
    .expect("the test repository is writable");
    std::fs::write(
        root.join("image.kdl"),
        "schema-version 1\n\nimage {\n    name \"Checked\"\n    base-image {\n        example {\n            version \"45\"\n            digest \"sha256:abc\"\n            signed #true\n        }\n    }\n}\n",
    )
    .expect("the test repository is writable");
    let plan = tect::run(Command::Plan, None, &root).stdout;
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(&root);
    assert!(issues.is_empty(), "{issues}");
    let flat = squashed(&plan);
    assert!(
        flat.contains(r#""image":"quay.io/fedora/fedora-bootc:45@sha256:abc""#),
        "{plan}"
    );
    assert!(flat.contains(r#""signed":true"#), "{plan}");
}

#[test]
fn an_image_that_lists_no_component_declaring_base_fails() {
    let root = written("base-not-a-base", "", &[("plain", "")]);
    std::fs::write(
        root.join("image.kdl"),
        "schema-version 1\n\nimage {\n    name \"Checked\"\n    plain\n}\n",
    )
    .expect("the test repository is writable");
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(&root);
    assert!(issues.contains("`checked` lists no base"), "{issues}");
}

#[test]
fn a_family_outside_a_base_block_is_unknown() {
    let issues = checked(
        "family-not-a-base",
        "        plain",
        &[("plain", "family \"fedora\"")],
    );
    assert!(issues.contains("unknown node `family`"), "{issues}");
}

/// The identity component's layer renders the stable fields. The finalize
/// step renders the version.
#[test]
fn the_identity_component_renders_os_release_after_its_layer() {
    let root = written(
        "os-release-component",
        "        identity\n        later",
        &[("identity", "provides \"os-release\""), ("later", "")],
    );
    let out = generated(&root);
    let _ = std::fs::remove_dir_all(&root);
    let at = |line: &str| out.find(line).unwrap_or_else(|| panic!("{line}: {out}"));
    let render = at("/ctx/tect os-release");
    assert!(at("# ---- identity ----") < render, "{out}");
    assert!(render < at("# ---- later ----"), "{out}");
    let run = &out[render - 300..render];
    assert!(!run.contains("IMAGE_VERSION"), "{run}");
    assert!(
        out.contains("# ---- os-release ----\n/ctx/tect os-release"),
        "{out}"
    );
    assert_eq!(
        out.matches("# ---- image identity ----").count(),
        1,
        "{out}"
    );
    assert!(at("# ---- image identity ----") < render, "{out}");
}

#[test]
fn an_image_with_no_os_release_provider_renders_none() {
    let root = written("os-release-none", "        later", &[("later", "")]);
    let out = generated(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(!out.contains("tect os-release"), "{out}");
}

#[test]
fn a_base_that_ships_a_script_is_refused() {
    let root = written("base-ships", "", &[]);
    std::fs::write(
        root.join("components/base-image/example/component.sh"),
        "true\n",
    )
    .expect("the test repository is writable");
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        issues.contains("`base-image/example` is a base, and a base builds no layer"),
        "{issues}"
    );
}

/// The remover's own `provides` withdraws nothing, so it does not count.
#[test]
fn a_removes_of_a_name_only_the_remover_provides_fails() {
    let issues = checked(
        "removes-self",
        "        remover",
        &[("remover", "provides \"tool\"\nremoves \"tool\"")],
    );
    assert!(
        issues.contains("`remover` removes `tool`, which nothing in the image provides"),
        "{issues}"
    );
}

#[test]
fn a_base_witness_of_its_own_reaches_the_contract() {
    let plan = planned(
        "base-own-witness",
        "; provides \"bootc\"; provides \"tool\" witness=\"/opt/tool\"",
        "",
        &[],
    );
    // `bootc` declares no `witness=`, so nothing witnesses it.
    assert!(
        squashed(&plan).contains(r#""contract_files":["/opt/tool"]"#),
        "{plan}"
    );
}

#[test]
fn a_reference_is_checked_in_a_gate_an_image_and_an_import() {
    let issues = checked(
        "reference-places",
        "        consumer",
        &[(
            "consumer",
            "on \"fedora\" { packages \"a=$gated\"; }\nimports \"$chosen\"\nimports { \"tool\" { driver \"$passed\"; }; }",
        )],
    );
    assert!(issues.contains("`$gated` names no option"), "{issues}");
    assert!(issues.contains("`$chosen` names no option"), "{issues}");
    assert!(issues.contains("`$passed` names no option"), "{issues}");
    let root = written("reference-image", "", &[]);
    write_base(&root, "fedora", "base \"x:$tag\"");
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(&root);
    assert!(issues.contains("`$tag` names no option"), "{issues}");
}

/// `remover` builds on `dev` alone, so the `prod` consumer keeps `tool`.
#[test]
fn a_gated_remover_withdraws_on_its_own_targets_alone() {
    let prod = checked(
        "removes-flavour",
        "        provider\n        flavour \"dev\" { remover; }\n        flavour \"prod\" { consumer; }",
        &[PROVIDER, REMOVER, ("consumer", "requires \"tool\"")],
    );
    assert!(!prod.contains("which `remover` removes"), "{prod}");
    let dev = checked(
        "removes-flavour-same",
        "        provider\n        flavour \"dev\" {\n            remover\n            consumer\n        }",
        &[PROVIDER, REMOVER, ("consumer", "requires \"tool\"")],
    );
    assert!(
        dev.contains("`consumer` requires `tool`, which `remover` removes"),
        "{dev}"
    );
}

/// The base ships only `rpm/dnf5`, and `remover` takes it away, so nothing
/// meets the broader `rpm` any more.
#[test]
fn removing_the_only_lower_name_leaves_the_broader_requirement_unmet() {
    let root = written_on(
        "removes-hierarchy",
        "; provides \"rpm/dnf5\"",
        "        remover\n        consumer",
        &[
            ("remover", "removes \"rpm/dnf5\""),
            ("consumer", "requires \"rpm\""),
        ],
    );
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(root);
    assert!(
        issues.contains("`consumer` requires `rpm`, which nothing enabled provides"),
        "{issues}"
    );
}

/// The template already lists the base once, so a second listing in a
/// flavour is a second base entry.
#[test]
fn a_base_listed_twice_is_refused() {
    let issues = checked(
        "base-listed",
        "    flavour \"dev\" { base-image { example } }",
        &[],
    );
    assert!(issues.contains("`checked` lists two bases"), "{issues}");
}

#[test]
fn a_requirement_the_base_meets_is_recorded_as_the_base_s() {
    let plan = planned(
        "met-by-base",
        "; provides \"tool\"",
        "        consumer",
        &[("consumer", "requires \"tool\"")],
    );
    assert!(
        squashed(&plan).contains(r#""met_by":{"tool":{"how":"base"}}"#),
        "{plan}"
    );
}

/// `tool` is the control. No base requirement names it, so it keeps its
/// witness. The base requires `identity` by its capability, by its path and
/// by a name above the one it provides.
#[test]
fn a_component_the_base_requires_leaves_no_base_witness() {
    for (case, requires) in [
        ("capability", "os-release"),
        ("path", "identity"),
        ("broader", "os"),
    ] {
        let plan = planned(
            &format!("base-requires-witness-{case}"),
            &format!("; requires \"{requires}\""),
            "        identity\n        tool",
            &[
                (
                    "identity",
                    "provides \"os/release\" witness=\"/usr/lib/os-release\"\nprovides \"os-release\"",
                ),
                ("tool", "provides \"tool\" witness=\"/usr/bin/tool\""),
            ],
        );
        let squashed = squashed(&plan);
        assert!(
            squashed.contains(r#""witnesses":{"tool":["/usr/bin/tool"]}"#),
            "{case}: {plan}"
        );
        assert!(
            squashed.contains(r#""/usr/lib/os-release""#),
            "{case}: {plan}"
        );
    }
}

/// Removing the only `pm/dnf5` provider withdraws the broader `pm` with it,
/// because nothing else in the image answers `pm`.
#[test]
fn a_component_whose_lower_name_is_removed_no_longer_meets_the_broader_one() {
    let issues = checked(
        "component-lower-removed",
        "        consumer\n        manager\n        remover",
        &[
            ("consumer", "requires \"pm\""),
            ("manager", "provides \"pm/dnf5\""),
            ("remover", "removes \"pm/dnf5\""),
        ],
    );
    assert!(
        issues.contains("`consumer` requires `pm`, which nothing enabled provides"),
        "{issues}"
    );
}

/// The groups case targets fedora, because an ungated package group is refused
/// on a deb family before the base check reads it.
#[test]
fn a_base_that_ships_files_or_packages_is_refused() {
    for (case, extra) in [
        ("files", ""),
        ("packages", "packages \"vim\""),
        ("groups", "targets \"fedora\"\npackage-groups \"core\""),
    ] {
        let root = written(&format!("base-ships-{case}"), "", &[]);
        write_base(&root, "fedora", extra);
        if case == "files" {
            std::fs::create_dir_all(root.join("components/base-image/example/files/etc"))
                .expect("the test repository is writable");
        }
        let issues = tect::run(Command::Check, None, &root).issues.plain();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            issues.contains("`base-image/example` is a base, and a base builds no layer"),
            "{case}: {issues}"
        );
    }
}

/// The `version` option's own tracker wins over the tag a reference ends in.
#[test]
fn a_base_version_tracker_of_its_own_reaches_renovate() {
    let root = written("base-own-tracker", "", &[]);
    std::fs::write(
        root.join("components/base-image/example/manifest.kdl"),
        "schema-version 1\n\nbase \"example.invalid/base:$version\" { family \"fedora\"; }\n\
         option \"arch\" type=\"string\" { default \"x\"; }\n\
         option \"version\" type=\"string\" { default \"1\"; renovate datasource=\"github-releases\" depName=\"owner/base\"; }\n",
    )
    .expect("the test repository is writable");
    std::fs::write(
        root.join("repo.kdl"),
        "schema-version 1\nname \"Checked\"\nrenovate {\n    timezone \"UTC\"\n}\n",
    )
    .expect("the test repository is writable");
    let image = std::fs::read_to_string(root.join("image.kdl")).expect("the image file is written");
    std::fs::write(
        root.join("image.kdl"),
        image.replace(
            "base-image { example }\n",
            "base-image { example { version \"1\"; } }\n",
        ),
    )
    .expect("the test repository is writable");
    let out = generated(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(out.contains("owner/base"), "{out}");
    assert!(!out.contains("example.invalid/base\""), "{out}");
}

/// `tools` has no directory, so `plan.json` names the image file that defines
/// it. The base adapter is listed too, because `packages` needs one.
#[test]
fn an_inline_definition_records_the_image_file_as_its_origin() {
    let root = written(
        "inline-origin",
        "    tools {\n        packages \"vim\"\n    }\n    adapter",
        &[("adapter", "provides \"build-environment\"")],
    );
    let out = generated(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(out.contains(r#""defined": "./image.kdl""#), "{out}");
}

/// `tools` is defined inline in `a`, so `b`, which lists only its base and
/// `needs`, has no provider of `tool`.
#[test]
fn an_inline_provider_answers_only_its_own_image() {
    let root = written("inline-scope", "", &[("needs", "requires \"tool\"")]);
    std::fs::write(
        root.join("image.kdl"),
        "schema-version 1\n\nimage {\n    name \"a\"\n    base-image { example }\n    tools {\n        provides \"tool\"\n    }\n}\n\nimage {\n    name \"b\"\n    base-image { example }\n    needs\n}\n",
    )
    .expect("the test repository is writable");
    let issues = tect::run(Command::Check, None, &root).issues.plain();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        issues.contains("no component in the repository declares `provides \"tool\"`"),
        "{issues}"
    );
    assert_eq!(issues.matches("provides \"tool\"").count(), 1, "{issues}");
}

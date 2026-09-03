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
        r#"image {
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
        "description \"Consumes everywhere\"\nsupports \"fedora\"\nafter \"tool\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("modules/provider/module.kdl"),
        "description \"Provides tools\"\nsupports \"fedora\"\nprovides \"tool\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("modules/coinstalled/module.kdl"),
        "description \"Consumes with provider\"\nsupports \"fedora\"\nafter \"tool\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("modules/separate/module.kdl"),
        "description \"Consumes elsewhere\"\nsupports \"fedora\"\nafter \"tool\"\n",
    )
    .unwrap();

    let issues = tect::run(Command::Check, None, &root).issues.plain();
    assert!(
        issues.contains("`consumer` builds after `tool`, which only `provider` provides"),
        "{issues}"
    );
    assert!(
        issues.contains("`separate` builds after `tool`, which only `provider` provides"),
        "{issues}"
    );
    assert!(!issues.contains("`coinstalled` builds after"), "{issues}");
    let _ = std::fs::remove_dir_all(root);
}

/// A module shipping a MAC profile builds after whoever provides the MAC, even
/// though it declares no `requires` for it: the profile is emitted against
/// whatever the image carries, so the module supports families with a different
/// MAC and cannot name one. Measured 2026-09-02 — `yubikey` listed above
/// `deb-bootc-base/apparmor` dies on a missing `apparmor_parser`, and the
/// same build with the two the other way round passes.
#[test]
fn shipped_policy_orders_a_module_after_the_mac_that_installs_it() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("policy-ordering");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("modules/apparmor")).unwrap();
    std::fs::create_dir_all(root.join("modules/yubikey/apparmor")).unwrap();
    std::fs::write(root.join("repo.kdl"), "schema-version 1\nname \"Mac\"\n").unwrap();
    std::fs::write(
        root.join("image.kdl"),
        r#"image {
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
        "description \"The MAC\"\nsupports \"debian\"\nprovides \"apparmor-policy\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("modules/yubikey/module.kdl"),
        "description \"Ships a profile and requires nothing\"\nsupports \"debian\"\n",
    )
    .unwrap();
    std::fs::write(root.join("modules/yubikey/apparmor/usr.sbin.pcscd"), "").unwrap();

    let plan = tect::run(Command::Plan, None, &root).stdout;
    let at = |name: &str| plan.find(&format!("\"path\": \"{name}\"")).unwrap();
    assert!(at("apparmor") < at("yubikey"), "{plan}");
    // An ordering edge is not a requirement, so nothing is owed and nothing is
    // said: no `builds after ... which nothing enabled provides`.
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

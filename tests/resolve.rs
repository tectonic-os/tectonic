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

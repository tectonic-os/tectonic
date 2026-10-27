//! What a refused invocation looks like from outside the process.

use std::process::Command;

fn refused(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tect"))
        .args(args)
        .output()
        .expect("the binary runs");
    (
        out.status.code().unwrap_or_default(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn an_unknown_flag_is_an_invocation_error() {
    let (code, stderr) = refused(&["--definitely-not-a-flag"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("Error: "), "{stderr}");
    assert!(stderr.contains("--definitely-not-a-flag"), "{stderr}");
}

#[test]
fn an_unknown_command_is_an_invocation_error() {
    let (code, stderr) = refused(&["frobnicate"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("Error: "), "{stderr}");
}

/// The image sets the option that `consumer` imports by, so `two` builds. It
/// lands in `generated/`, and the image file stays as the user wrote it.
#[test]
fn generate_leaves_the_image_file_byte_identical() {
    let root = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli-generate");
    let _ = std::fs::remove_dir_all(&root);
    let files = [
        ("repo.kdl", "schema-version 1\nname \"Checked\"\n"),
        (
            "image.kdl",
            "schema-version 1\n\nimage {\n    name \"Checked\"\n    base-image {\n        example\n    }\n    consumer { runtime \"two\"; }\n}\n",
        ),
        (
            "components/consumer/manifest.kdl",
            "schema-version 1\nrequires \"tool\"\nimports \"$runtime\"\noption \"runtime\" type=\"string\" { default \"one\"; }\n",
        ),
        (
            "components/base-image/example/manifest.kdl",
            "schema-version 1\nbase \"example\" { family \"fedora\"; }\n",
        ),
        ("components/one/manifest.kdl", "schema-version 1\nprovides \"tool\"\n"),
        ("components/two/manifest.kdl", "schema-version 1\nprovides \"tool\"\n"),
    ];
    for (path, text) in files {
        let at = root.join(path);
        std::fs::create_dir_all(at.parent().expect("every path has a parent"))
            .expect("the test repository is writable");
        std::fs::write(at, text).expect("the test repository is writable");
    }
    let out = Command::new(env!("CARGO_BIN_EXE_tect"))
        .arg("generate")
        .current_dir(&root)
        .env(
            "TECT_ASSETS",
            concat!(env!("CARGO_MANIFEST_DIR"), "/assets"),
        )
        .output()
        .expect("the binary runs");
    let image = std::fs::read_to_string(root.join("image.kdl")).unwrap_or_default();
    let imported = root.join("generated/checked/components/two.sh").is_file();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(image, files[1].1);
    assert!(imported);
}

/// `fedora-botc` is one letter away from the library's `fedora-bootc`.
#[test]
fn create_image_refuses_a_base_path_no_component_names() {
    let root = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli-base-typo");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("components")).expect("the test repository is writable");
    std::fs::write(root.join("repo.kdl"), "schema-version 1\n")
        .expect("the test repository is writable");
    let (code, stderr) = refused(&[
        "create",
        "image",
        "typo",
        "--base",
        "base-image/fedora-botc",
        "--root",
        root.to_str().expect("the temporary path is UTF-8"),
        "--no-tui",
    ]);
    let wrote = root.join("typo.image.kdl").exists();
    let _ = std::fs::remove_dir_all(&root);
    assert_ne!(code, 0, "{stderr}");
    assert!(
        stderr.contains("`base-image/fedora-botc` names no base component"),
        "{stderr}"
    );
    assert!(!wrote, "the refused image was written");
}

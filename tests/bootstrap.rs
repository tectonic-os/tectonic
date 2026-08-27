use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn run(root: &Path, path: &str, asset: &Path) -> std::process::Output {
    Command::new(root.join("scripts/tect.sh"))
        .current_dir(root)
        .env("PATH", path)
        .env("MOCK_ASSET", asset)
        .output()
        .unwrap()
}

#[test]
fn bootstrap_holds_declared_hashes_to_their_cache_identity() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("bootstrap");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::create_dir(root.join("mock-bin")).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/scripts/tect.sh"),
        root.join("scripts/tect.sh"),
    )
    .unwrap();

    executable(
        &root.join("mock-bin/curl"),
        "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\n    case \"$1\" in\n        -o) out=$2; shift 2 ;;\n        *) shift ;;\n    esac\ndone\ncp \"$MOCK_ASSET\" \"$out\"\n",
    );
    executable(&root.join("tect"), "#!/bin/sh\nprintf 'cached\\n'\n");
    let asset = root.join("release.tar.gz");
    assert!(Command::new("tar")
        .args(["-czf"])
        .arg(&asset)
        .args(["-C"])
        .arg(&root)
        .arg("tect")
        .status()
        .unwrap()
        .success());
    let hash = Command::new("sha256sum").arg(&asset).output().unwrap();
    assert!(hash.status.success());
    let hash = String::from_utf8(hash.stdout).unwrap()[..64].to_string();
    let path = format!(
        "{}:{}",
        root.join("mock-bin").display(),
        std::env::var("PATH").unwrap()
    );

    fs::write(
        root.join("repo.kdl"),
        format!("  tect-version \"1.2.3\" sha256=\"{hash}\"   // pinned\n"),
    )
    .unwrap();
    let first = run(&root, &path, &asset);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, b"cached\n");

    let changed = if hash.starts_with('a') { "b" } else { "a" }.repeat(64);
    fs::write(
        root.join("repo.kdl"),
        format!("tect-version \"1.2.3\" sha256=\"{changed}\"\n"),
    )
    .unwrap();
    let second = run(&root, &path, &asset);
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("does not match"));

    for malformed in ["", "bad"] {
        fs::write(
            root.join("repo.kdl"),
            format!("tect-version \"1.2.3\" sha256=\"{malformed}\"\n"),
        )
        .unwrap();
        let output = run(&root, &path, &asset);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("malformed sha256"));
    }
}

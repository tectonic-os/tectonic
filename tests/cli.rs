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

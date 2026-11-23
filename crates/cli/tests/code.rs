#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Output};

fn od(args: &[&str], config: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opendesigner"))
        .args(args)
        .env("OPENDESIGNER_CONFIG_DIR", config)
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

#[test]
fn a_folder_starts_in_assets_mode_and_switches_to_code() {
    let repo = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let (r, c) = (
        repo.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    );
    let before: serde_json::Value =
        serde_json::from_str(&out(&od(&["code", "status", r], c))).unwrap();
    assert_eq!(before["mode"], "assets");
    assert!(od(&["code", "mode", "code", r], c).status.success());
    let after: serde_json::Value =
        serde_json::from_str(&out(&od(&["code", "status", r], c))).unwrap();
    assert_eq!(after["mode"], "code");
    assert!(after["stack_questions"].is_array());
}

#[test]
fn approve_without_a_plan_fails_and_without_yes_starts_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let (r, c) = (
        repo.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    );
    od(&["code", "mode", "code", r], c);
    let none = od(&["code", "approve", r], c);
    assert!(!none.status.success());

    std::fs::write(
        repo.path().join(".opendesigner/code.json"),
        r#"{"version":1,"mode":"code","plan":{"services":[{"name":"web","kind":"web","command":"echo hi","port":4321}]}}"#,
    )
    .unwrap();
    let preview = od(&["code", "approve", r], c);
    assert!(preview.status.success());
    let text = out(&preview);
    assert!(
        text.contains("echo hi") && text.contains("Nothing was started"),
        "{text}"
    );
    let status: serde_json::Value =
        serde_json::from_str(&out(&od(&["code", "status", r], c))).unwrap();
    assert_eq!(status["approved"], false);
}

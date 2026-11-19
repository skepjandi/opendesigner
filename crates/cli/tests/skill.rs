#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::process::{Command, Output};

fn od(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opendesigner"))
        .args(args)
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

#[test]
fn install_previews_by_default_then_writes_with_yes_and_uninstalls() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path().to_str().unwrap();
    fs::create_dir_all(home.path().join(".claude")).unwrap();

    let status = od(&["skill", "status", "--home", h]);
    assert!(status.status.success());
    let text = out(&status);
    assert!(
        text.contains("claude-code") && text.contains("agent found"),
        "{text}"
    );
    assert!(text.contains("not installed"));

    // preview: shows the paths, writes nothing
    let preview = od(&["skill", "install", "--home", h]);
    assert!(preview.status.success());
    let text = out(&preview);
    assert!(
        text.contains(".claude/skills/opendesigner/SKILL.md"),
        "{text}"
    );
    assert!(text.contains("Nothing written"));
    assert!(!home.path().join(".claude/skills").exists());

    let done = od(&["skill", "install", "--home", h, "--yes"]);
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
    let skill =
        fs::read_to_string(home.path().join(".claude/skills/opendesigner/SKILL.md")).unwrap();
    assert!(skill.starts_with("---\nname: opendesigner\n"));
    assert!(
        !home.path().join(".agents").exists(),
        "no Codex/Gemini/opencode here, so no ~/.agents"
    );

    assert!(out(&od(&["skill", "status", "--home", h])).contains("installed, version"));

    let preview = od(&["skill", "uninstall", "--home", h]);
    assert!(out(&preview).contains("Nothing removed"));
    assert!(
        home.path()
            .join(".claude/skills/opendesigner/SKILL.md")
            .exists()
    );
    let removed = od(&["skill", "uninstall", "--home", h, "--yes"]);
    assert!(removed.status.success());
    assert!(!home.path().join(".claude/skills/opendesigner").exists());
}

#[test]
fn a_foreign_folder_is_refused_and_unknown_targets_fail() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path().to_str().unwrap();
    let theirs = home.path().join(".claude/skills/opendesigner");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), "theirs").unwrap();

    let refused = od(&[
        "skill",
        "install",
        "--home",
        h,
        "--target",
        "claude-code",
        "--yes",
    ]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("not created by OpenDesigner"));
    assert_eq!(
        fs::read_to_string(theirs.join("SKILL.md")).unwrap(),
        "theirs"
    );

    let bad = od(&["skill", "install", "--home", h, "--target", "cursor"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("unknown target"));

    let none = tempfile::tempdir().unwrap();
    let text = out(&od(&[
        "skill",
        "install",
        "--home",
        none.path().to_str().unwrap(),
        "--yes",
    ]));
    assert!(text.contains("No supported agent found"), "{text}");
}

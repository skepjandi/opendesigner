#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use od_agent::skill::{self, Action, State};

/// Every file under `root` with its contents, relative paths.
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.insert(
                    p.strip_prefix(base).unwrap().display().to_string(),
                    fs::read(&p).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn home_with_agents() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join(".claude")).unwrap();
    fs::create_dir_all(home.path().join(".codex")).unwrap();
    // things the installer must never touch
    fs::write(home.path().join(".claude/settings.json"), "{\"keep\":true}").unwrap();
    fs::write(home.path().join(".codex/config.toml"), "keep = true").unwrap();
    home
}

#[test]
fn plan_detects_agents_and_reports_state_without_writing() {
    let home = home_with_agents();
    let before = snapshot(home.path());
    let plan = skill::plan(home.path()).unwrap();
    assert_eq!(snapshot(home.path()), before, "planning writes nothing");
    let by_id = |id: &str| plan.iter().find(|p| p.target.id == id).unwrap();
    assert!(by_id("claude-code").detected);
    assert!(
        by_id("agents").detected,
        ".codex implies a reader of ~/.agents/skills"
    );
    assert_eq!(by_id("claude-code").state, State::NotInstalled);
    assert_eq!(
        by_id("claude-code").dir,
        home.path().join(".claude/skills/opendesigner")
    );
    let empty = tempfile::tempdir().unwrap();
    assert!(
        skill::plan(empty.path())
            .unwrap()
            .iter()
            .all(|p| !p.detected)
    );
}

#[test]
fn install_writes_only_inside_the_skill_folder_and_is_idempotent() {
    let home = home_with_agents();
    let before = snapshot(home.path());
    let reports = skill::install(home.path(), &["claude-code", "agents"], false).unwrap();
    assert_eq!(reports.len(), 2);

    let after = snapshot(home.path());
    for (path, bytes) in &before {
        assert_eq!(after.get(path), Some(bytes), "{path} must be untouched");
    }
    let added: Vec<&String> = after.keys().filter(|k| !before.contains_key(*k)).collect();
    assert!(!added.is_empty());
    for path in &added {
        assert!(
            path.starts_with(".claude/skills/opendesigner/")
                || path.starts_with(".agents/skills/opendesigner/"),
            "unexpected write: {path}"
        );
    }
    for rel in [
        "SKILL.md",
        "references/tools.md",
        "references/store-specs.md",
        "references/style-guide.md",
        ".opendesigner-install.json",
    ] {
        assert!(
            home.path()
                .join(".claude/skills/opendesigner")
                .join(rel)
                .exists(),
            "{rel}"
        );
    }

    let again = skill::install(home.path(), &["claude-code"], false).unwrap();
    assert!(
        again[0]
            .actions
            .iter()
            .all(|a| matches!(a, Action::Unchanged(_))),
        "{:?}",
        again[0].actions
    );
    assert_eq!(snapshot(home.path()), after);
    let state = &skill::plan(home.path())
        .unwrap()
        .into_iter()
        .find(|p| p.target.id == "claude-code")
        .unwrap()
        .state;
    assert!(matches!(state, State::Installed { modified, .. } if modified.is_empty()));
}

#[test]
fn a_skill_folder_we_did_not_create_is_never_overwritten_without_consent() {
    let home = home_with_agents();
    let theirs = home.path().join(".claude/skills/opendesigner");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), "someone else's skill").unwrap();

    let err = skill::install(home.path(), &["claude-code"], false).unwrap_err();
    assert!(
        err.to_string().contains("not created by OpenDesigner"),
        "{err}"
    );
    assert_eq!(
        fs::read_to_string(theirs.join("SKILL.md")).unwrap(),
        "someone else's skill"
    );
    assert!(skill::uninstall(home.path(), &["claude-code"]).is_err());
    assert!(theirs.join("SKILL.md").exists());

    skill::install(home.path(), &["claude-code"], true).unwrap();
    assert!(
        fs::read_to_string(theirs.join("SKILL.md"))
            .unwrap()
            .contains("name: opendesigner")
    );
}

#[test]
fn files_the_user_edited_survive_updates_and_uninstall() {
    let home = home_with_agents();
    let dir = home.path().join(".claude/skills/opendesigner");
    skill::install(home.path(), &["claude-code"], false).unwrap();
    fs::write(dir.join("references/style-guide.md"), "my own house style").unwrap();

    let state = &skill::plan(home.path())
        .unwrap()
        .into_iter()
        .find(|p| p.target.id == "claude-code")
        .unwrap()
        .state;
    assert!(
        matches!(state, State::Installed { modified, .. } if modified == &["references/style-guide.md"])
    );

    let report = skill::install(home.path(), &["claude-code"], false).unwrap();
    assert!(
        report[0]
            .actions
            .contains(&Action::KeptEdited("references/style-guide.md".into()))
    );
    assert_eq!(
        fs::read_to_string(dir.join("references/style-guide.md")).unwrap(),
        "my own house style"
    );
    // still tracked as edited on the next update
    let report = skill::install(home.path(), &["claude-code"], false).unwrap();
    assert!(
        report[0]
            .actions
            .contains(&Action::KeptEdited("references/style-guide.md".into()))
    );

    let removed = skill::uninstall(home.path(), &["claude-code"]).unwrap();
    assert!(
        removed[0]
            .actions
            .contains(&Action::KeptEdited("references/style-guide.md".into()))
    );
    assert_eq!(
        fs::read_to_string(dir.join("references/style-guide.md")).unwrap(),
        "my own house style"
    );
    assert!(!dir.join("SKILL.md").exists());
    assert!(!dir.join(".opendesigner-install.json").exists());
}

#[test]
fn uninstall_removes_exactly_what_install_wrote_and_nothing_else() {
    let home = home_with_agents();
    let before = snapshot(home.path());
    skill::install(home.path(), &["claude-code", "agents"], false).unwrap();
    let reports = skill::uninstall(home.path(), &["claude-code", "agents"]).unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(snapshot(home.path()), before);
    assert!(
        !home.path().join(".claude/skills/opendesigner").exists(),
        "empty folder removed"
    );
    assert!(
        skill::uninstall(home.path(), &["claude-code"])
            .unwrap()
            .is_empty(),
        "nothing to do twice"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed_out_of_the_skill_folder() {
    use std::os::unix::fs::symlink;
    let home = home_with_agents();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("victim"), "precious").unwrap();

    // the skill folder itself is a symlink
    fs::create_dir_all(home.path().join(".claude/skills")).unwrap();
    symlink(
        outside.path(),
        home.path().join(".claude/skills/opendesigner"),
    )
    .unwrap();
    assert!(skill::install(home.path(), &["claude-code"], false).is_err());
    assert!(
        skill::install(home.path(), &["claude-code"], true).is_err(),
        "even with consent"
    );
    assert_eq!(
        fs::read_to_string(outside.path().join("victim")).unwrap(),
        "precious"
    );
    fs::remove_file(home.path().join(".claude/skills/opendesigner")).unwrap();

    // a file inside it is a symlink
    skill::install(home.path(), &["claude-code"], false).unwrap();
    let dir = home.path().join(".claude/skills/opendesigner");
    fs::remove_file(dir.join("SKILL.md")).unwrap();
    symlink(outside.path().join("victim"), dir.join("SKILL.md")).unwrap();
    let _ = skill::install(home.path(), &["claude-code"], false);
    assert_eq!(
        fs::read_to_string(outside.path().join("victim")).unwrap(),
        "precious"
    );
    let _ = skill::uninstall(home.path(), &["claude-code"]);
    assert_eq!(
        fs::read_to_string(outside.path().join("victim")).unwrap(),
        "precious"
    );

    // the user's skills root may legitimately be a symlink (dotfiles managers do this)
    let home2 = tempfile::tempdir().unwrap();
    let real = tempfile::tempdir().unwrap();
    fs::create_dir_all(home2.path().join(".claude")).unwrap();
    symlink(real.path(), home2.path().join(".claude/skills")).unwrap();
    skill::install(home2.path(), &["claude-code"], false).unwrap();
    assert!(real.path().join("opendesigner/SKILL.md").exists());
}

#[test]
fn unknown_targets_and_unsafe_names_are_rejected() {
    let home = home_with_agents();
    assert!(skill::install(home.path(), &["cursor"], false).is_err());
    assert!(skill::uninstall(home.path(), &["nope"]).is_err());
    assert_eq!(skill::TARGETS.len(), 2);
    for t in skill::TARGETS {
        assert!(!t.skills_dir.contains(".."));
        assert!(t.source.starts_with("https://"));
    }
}

#[test]
fn an_update_removes_files_an_older_version_wrote_and_the_current_one_does_not() {
    let home = home_with_agents();
    skill::install(home.path(), &["claude-code"], false).unwrap();
    let dir = home.path().join(".claude/skills/opendesigner");
    // pretend an older version also wrote references/old.md
    let old = dir.join("references/old.md");
    fs::write(&old, "old").unwrap();
    let manifest_path = dir.join(".opendesigner-install.json");
    let mut m: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    use sha2::{Digest, Sha256};
    let hash: String = Sha256::digest(b"old")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    m["files"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"path":"references/old.md","sha256":hash}));
    fs::write(&manifest_path, m.to_string()).unwrap();

    let report = skill::install(home.path(), &["claude-code"], false).unwrap();
    assert!(
        report[0]
            .actions
            .contains(&Action::Removed("references/old.md".into()))
    );
    assert!(!old.exists());
}

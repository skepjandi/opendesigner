//! `doctor`, `remove-background` and `transcribe` through the real binary, with fake programs
//! named by the override variables (a child process, so the environment is its own).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn od(args: &[&str], env: &[(&str, &Path)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_opendesigner"));
    cmd.args(args).env("PATH", "/bin:/usr/bin");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

fn stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

#[test]
fn doctor_lists_found_programs_with_versions_and_install_hints_for_the_rest() {
    let tmp = tempfile::tempdir().unwrap();
    let rembg = script(tmp.path(), "rembg", "echo 'rembg 9.9.9'");
    let none = tmp.path().join("none");

    let text = stdout(&od(
        ["doctor"].as_slice(),
        &[
            ("OPENDESIGNER_REMBG", &rembg),
            ("OPENDESIGNER_WHISPER", &rembg),
            ("OPENDESIGNER_FFMPEG", &rembg),
        ],
    ));
    assert!(text.contains("rembg 9.9.9"), "{text}");

    let json = stdout(&od(
        ["doctor", "--json"].as_slice(),
        &[
            ("OPENDESIGNER_REMBG", &rembg),
            ("OPENDESIGNER_FFMPEG", &none),
        ],
    ));
    let report: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(report["can"]["remove_background"], true);
    assert_eq!(report["can"]["export_video"], false);
    let ffmpeg = &report["programs"][0];
    assert_eq!(ffmpeg["name"], "ffmpeg");
    assert!(
        ffmpeg["problem"]
            .as_str()
            .unwrap()
            .contains("OPENDESIGNER_FFMPEG")
    );
}

#[test]
fn remove_background_writes_the_cutout_into_the_project_and_missing_rembg_is_a_clear_error() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("p.od");
    stdout(&od(&["new", project.to_str().unwrap()], &[]));
    fs::create_dir_all(project.join("media")).unwrap();
    let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];
    fs::write(project.join("media/pic.png"), png).unwrap();
    let args = [
        "remove-background",
        project.to_str().unwrap(),
        "--file",
        "media/pic.png",
    ];

    let missing = od(&args, &[]);
    assert!(!missing.status.success());
    let err = String::from_utf8_lossy(&missing.stderr);
    assert!(
        err.contains("rembg was not found") && err.contains("pip install"),
        "{err}"
    );

    let canned = tmp.path().join("canned.png");
    fs::write(&canned, png).unwrap();
    let rembg = script(
        tmp.path(),
        "rembg",
        &format!("for last; do :; done\ncp '{}' \"$last\"", canned.display()),
    );
    let out = stdout(&od(&args, &[("OPENDESIGNER_REMBG", &rembg)]));
    assert!(out.contains("media/pic-cutout.png"), "{out}");
    assert_eq!(fs::read(project.join("media/pic-cutout.png")).unwrap(), png);
    assert!(project.join("media/pic-cutout.rembg.json").is_file());
}

#[test]
fn transcribe_without_a_speech_program_says_what_to_install() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("p.od");
    stdout(&od(&["new", project.to_str().unwrap()], &[]));
    fs::create_dir_all(project.join("media")).unwrap();
    fs::write(project.join("media/a.wav"), b"RIFF").unwrap();
    let out = od(
        &[
            "transcribe",
            project.to_str().unwrap(),
            "--file",
            "media/a.wav",
        ],
        &[],
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("pip install openai-whisper") && err.contains("OPENDESIGNER_WHISPER"),
        "{err}"
    );
}

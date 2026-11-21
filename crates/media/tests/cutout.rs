//! Background removal and the capabilities report, with fake programs (`tests/fakes`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fakes;

use std::time::Duration;

use fakes::*;
use od_media::cutout::{self, Job};
use od_media::external::{Capabilities, ExternalError, Search};

fn project_with_image(tmp: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let project = tmp.join("project");
    std::fs::create_dir_all(project.join("media")).unwrap();
    let input = project.join("media/Hero shot.png");
    std::fs::write(&input, png_bytes()).unwrap();
    (project, input)
}

fn job<'a>(
    program: &'a std::path::Path,
    input: &'a std::path::Path,
    project: &'a std::path::Path,
) -> Job<'a> {
    Job {
        program,
        input,
        reference: "media/Hero shot.png",
        project,
        model: None,
        mask_only: false,
        timeout: Duration::from_secs(20),
        cancel: None,
    }
}

fn media_names(project: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(project.join("media"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_cutout_is_written_next_to_a_sidecar_and_never_over_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let (project, input) = project_with_image(tmp.path());
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let rembg = fake_rembg(&bin, &{
        let p = bin.join("canned.png");
        std::fs::write(&p, png_bytes()).unwrap();
        p
    });
    let mut j = job(&rembg, &input, &project);
    j.model = Some("isnet-general-use");

    let first = cutout::remove_background(&j).unwrap();
    assert_eq!(first.file, "media/Hero shot-cutout.png");
    assert_eq!(first.sidecar, "media/Hero shot-cutout.rembg.json");
    let second = cutout::remove_background(&j).unwrap();
    assert_eq!(second.file, "media/Hero shot-cutout-2.png");

    let sidecar: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(project.join(&first.sidecar)).unwrap())
            .unwrap();
    assert_eq!(sidecar["source"], "media/Hero shot.png");
    assert_eq!(sidecar["tool"], "rembg");
    assert_eq!(sidecar["model"], "isnet-general-use");
    assert!(sidecar["processed"].as_str().unwrap().contains("locally"));
    assert_eq!(
        std::fs::read(&input).unwrap(),
        png_bytes(),
        "the source is untouched"
    );

    let args = recorded(&bin, "rembg-args.txt");
    assert_eq!(&args[..3], ["i", "-m", "isnet-general-use"]);
    assert_eq!(args[3], input.to_str().unwrap());
    assert!(args[4].ends_with("out.png"));
    assert_eq!(args.len(), 5);
}

#[test]
fn mask_only_asks_for_the_mask_and_names_the_file_so() {
    let tmp = tempfile::tempdir().unwrap();
    let (project, input) = project_with_image(tmp.path());
    let canned = tmp.path().join("c.png");
    std::fs::write(&canned, png_bytes()).unwrap();
    let rembg = fake_rembg(tmp.path(), &canned);
    let mut j = job(&rembg, &input, &project);
    j.mask_only = true;
    let made = cutout::remove_background(&j).unwrap();
    assert_eq!(made.file, "media/Hero shot-mask.png");
    assert!(recorded(tmp.path(), "rembg-args.txt").contains(&"-om".to_string()));
}

#[test]
fn a_result_that_is_not_a_png_or_a_failed_run_leaves_media_as_it_was() {
    let tmp = tempfile::tempdir().unwrap();
    let (project, input) = project_with_image(tmp.path());
    let before = media_names(&project);

    let not_png = tmp.path().join("fake.png");
    std::fs::write(&not_png, b"GIF89a....").unwrap();
    let rembg = fake_rembg(tmp.path(), &not_png);
    let err = cutout::remove_background(&job(&rembg, &input, &project)).unwrap_err();
    assert!(err.to_string().contains("PNG"), "{err}");

    let failing = script(tmp.path(), "rembg-fail", "echo 'no model' >&2; exit 2");
    let err = cutout::remove_background(&job(&failing, &input, &project)).unwrap_err();
    assert!(err.to_string().contains("no model"), "{err}");

    let silent = script(tmp.path(), "rembg-silent", "exit 0");
    assert!(cutout::remove_background(&job(&silent, &input, &project)).is_err());

    let slow = script(tmp.path(), "rembg-slow", "sleep 30");
    let mut j = job(&slow, &input, &project);
    j.timeout = Duration::from_millis(300);
    assert!(matches!(
        cutout::remove_background(&j).unwrap_err(),
        ExternalError::Timeout { .. }
    ));
    assert_eq!(media_names(&project), before);
}

#[test]
fn model_names_are_plain() {
    assert!(cutout::model_name("u2net").is_ok());
    assert!(cutout::model_name("isnet-general-use").is_ok());
    for bad in [
        "",
        "U2NET",
        "../x",
        "a b",
        "-o",
        "u2net;ls",
        &"a".repeat(41),
    ] {
        assert!(cutout::model_name(bad).is_err(), "{bad}");
    }
}

fn search(dir: &std::path::Path) -> Search {
    Search {
        explicit: None,
        app_dirs: vec![],
        path_dirs: vec![dir.to_path_buf()],
    }
}

#[test]
fn the_report_lists_what_is_found_with_versions_and_what_is_missing_with_hints() {
    let tmp = tempfile::tempdir().unwrap();
    script(tmp.path(), "rembg", "echo 'rembg 2.0.5'");
    script(tmp.path(), "whisper-cli", "echo 'whisper.cpp 1.7' >&2");
    let s = search(tmp.path());
    let caps = Capabilities::probe_with(true, &search(&tmp.path().join("none")), &s, &s, None);

    let rembg = caps.program("rembg").unwrap();
    assert_eq!(
        (rembg.found.is_some(), rembg.version.as_str()),
        (true, "rembg 2.0.5")
    );
    let cpp = caps.program("whisper-cli").unwrap();
    assert_eq!(
        cpp.version, "whisper.cpp 1.7",
        "a version printed on stderr is read"
    );
    assert!(caps.program("whisper").unwrap().found.is_none());
    let ffmpeg = caps.program("ffmpeg").unwrap();
    assert!(ffmpeg.found.is_none() && ffmpeg.install_hint.contains("brew install ffmpeg"));
    assert_eq!(caps.missing(), vec!["ffmpeg"]);
}

#[test]
fn a_program_that_hangs_on_its_version_question_does_not_hang_the_report() {
    let tmp = tempfile::tempdir().unwrap();
    script(tmp.path(), "rembg", "echo ok; sleep 60");
    let s = search(tmp.path());
    let started = std::time::Instant::now();
    let caps = Capabilities::probe_with(true, &s, &s, &s, None);
    assert!(started.elapsed() < Duration::from_secs(40));
    assert_eq!(caps.program("rembg").unwrap().version, "");
    assert!(caps.program("rembg").unwrap().found.is_some());
}

#[test]
fn a_bad_override_shows_as_a_problem_not_as_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = search(tmp.path());
    s.explicit = Some(tmp.path().join("nope").into_os_string());
    let caps = Capabilities::probe_with(true, &search(tmp.path()), &search(tmp.path()), &s, None);
    let rembg = caps.program("rembg").unwrap();
    assert!(rembg.found.is_none());
    assert!(
        rembg
            .problem
            .as_deref()
            .unwrap()
            .contains("OPENDESIGNER_REMBG")
    );
}

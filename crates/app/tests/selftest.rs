#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Starts the real editor window offscreen and lets `qml/SelfTest.qml` drive it: create a project,
/// draw shapes, edit properties, undo, pin a note, start the agent connection, restore a
/// checkpoint, export against store presets, and grab the window. The test then checks the files
/// that session wrote.
#[test]
fn the_editor_window_runs_a_scripted_session() {
    // Referencing the library makes the test binary link the generated Qt glue it depends on.
    assert!(!opendesigner_app::state::EditorState::frame_presets().is_null());
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_opendesigner-app"))
        .arg(format!("--selftest={}", work.display()))
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("OPENDESIGNER_CONFIG_DIR", tmp.path().join("cfg"))
        .env("HOME", &home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            panic!("the editor did not finish its scripted session in 120 s");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let out = child.wait_with_output().unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(status.success(), "exit {status:?}\n{log}");
    assert!(log.contains("SELFTEST OK"), "{log}");
    assert!(!log.contains("SELFTEST FAIL"), "{log}");
    let qml_warnings: Vec<&str> = log.lines().filter(|l| l.contains("qrc:/qt/qml")).collect();
    assert!(
        qml_warnings.is_empty(),
        "QML warnings:\n{}",
        qml_warnings.join("\n")
    );
    assert!(
        !log.contains("ReferenceError")
            && !log.contains("TypeError")
            && !log.contains("is not a type"),
        "QML errors:\n{log}"
    );

    // the project the session built
    let project = work.join("p.od");
    let page = std::fs::read_to_string(project.join("pages/p_1.json")).unwrap();
    for name in ["\"rect\"", "\"ellipse\"", "\"text\""] {
        assert!(page.contains(name), "{name} missing from the saved page");
    }
    let notes = std::fs::read_to_string(project.join("annotations.json")).unwrap();
    assert!(notes.contains("make this bigger"));
    assert!(project.join("exports/shot.png").exists());
    assert!(
        !project.join("exports/bad.png").exists(),
        "a failed export writes nothing"
    );
    let log_file = std::fs::read_to_string(project.join(".od/history/log.jsonl")).unwrap();
    assert!(
        log_file.contains("\"type\":\"restore\""),
        "the restore is in the history"
    );
    assert!(
        tmp.path().join("cfg/mcp-token").exists(),
        "the agent connection created its token"
    );

    // the window really drew something: more than one color, and the editor's gold
    let img = png::Decoder::new(std::io::Cursor::new(
        std::fs::read(work.join("window.png")).unwrap(),
    ));
    let mut reader = img.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert!(
        info.width >= 900 && info.height >= 600,
        "{}x{}",
        info.width,
        info.height
    );
    let channels = info.color_type.samples();
    let colors: HashSet<[u8; 3]> = buf[..info.buffer_size()]
        .chunks_exact(channels)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    assert!(
        colors.len() > 20,
        "the window looks blank: {} colors",
        colors.len()
    );
}

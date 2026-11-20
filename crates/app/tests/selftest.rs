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

    // a magenta 64x32 picture for the session to import
    let mut fixture = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut fixture, 64, 32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[255, 0, 255].repeat(64 * 32)).unwrap();
    }
    std::fs::write(work.join("fixture.png"), &fixture).unwrap();

    // a scripted ACP agent for the chat steps of the session
    let exe = std::env::current_exe().unwrap();
    let mock = exe
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join(if cfg!(windows) {
            "mock_acp_agent.exe"
        } else {
            "mock_acp_agent"
        });
    if !mock.exists() {
        let built = Command::new(env!("CARGO"))
            .args([
                "build",
                "--example",
                "mock_acp_agent",
                "-p",
                "opendesigner-agent",
            ])
            .status()
            .unwrap();
        assert!(built.success());
    }
    let session_log = work.join("agent-session.json");

    let mut child = Command::new(env!("CARGO_BIN_EXE_opendesigner-app"))
        .arg(format!("--selftest={}", work.display()))
        .arg(format!(
            "--selftest-agent=\"{}\" normal \"{}\"",
            mock.display(),
            session_log.display()
        ))
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
    for name in ["\"rect\"", "\"ellipse\"", "\"text\"", "\"Screen\""] {
        assert!(page.contains(name), "{name} missing from the saved page");
    }
    assert!(
        page.contains("\"device3d\"") && page.contains("\"rotate_y\""),
        "the turned 3D device is saved in the page"
    );
    let notes = std::fs::read_to_string(project.join("annotations.json")).unwrap();
    assert!(notes.contains("make this bigger"));
    assert!(project.join("exports/shot.png").exists());
    assert!(
        project.join("exports/slide-01.png").exists(),
        "the set export wrote its slide"
    );
    assert!(
        project.join("exports/tr-ja-01.png").exists(),
        "the Japanese set was written under its language name"
    );
    assert!(
        project.join("exports/pano-01.png").exists()
            && project.join("exports/pano-02.png").exists(),
        "the panorama was cut into two files"
    );
    assert_eq!(
        std::fs::read(project.join("media/fixture.png")).unwrap(),
        fixture,
        "the imported image is copied into the project"
    );
    assert!(page.contains("media/fixture.png"));
    assert!(
        page.contains("{brand.gold}"),
        "the ellipse is filled with the token"
    );
    let manifest = std::fs::read_to_string(project.join("od.toml")).unwrap();
    assert!(
        manifest.contains("[locales.ja]") && manifest.contains("守る"),
        "the translation is saved in the manifest: {manifest}"
    );
    assert!(
        manifest.contains("brand.gold") && manifest.contains("#f2ca83"),
        "{manifest}"
    );
    // the exported frame contains the imported picture, centered
    let shot = png::Decoder::new(std::io::Cursor::new(
        std::fs::read(project.join("exports/shot.png")).unwrap(),
    ));
    let mut shot = shot.read_info().unwrap();
    let mut pixels = vec![0; shot.output_buffer_size().unwrap()];
    let info = shot.next_frame(&mut pixels).unwrap();
    let at = |x: usize, y: usize| {
        let i = (y * info.width as usize + x) * info.color_type.samples();
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    assert_eq!(
        at(660, 1434),
        [255, 0, 255],
        "the image is drawn into the export"
    );
    assert!(
        !project.join("exports/bad.png").exists(),
        "a failed export writes nothing"
    );
    let log_file = std::fs::read_to_string(project.join(".od/history/log.jsonl")).unwrap();
    assert!(
        log_file.contains("\"type\":\"restore\""),
        "the restore is in the history"
    );
    // the chat gave the scripted agent the editor's local MCP server and its token
    let given: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&session_log).unwrap()).unwrap();
    let token = std::fs::read_to_string(tmp.path().join("cfg/mcp-token")).unwrap();
    assert_eq!(
        given["mcpServers"][0]["headers"][0]["value"],
        format!("Bearer {}", token.trim())
    );
    // the MCP server the user added is given to the agent as well, in the ACP stdio form
    assert!(given["mcpServers"][1].get("type").is_none());
    assert_eq!(given["mcpServers"][1]["name"], "selftest-mcp");
    // the program is sent as the path to the executable, as the ACP stdio transport wants
    let command = given["mcpServers"][1]["command"].as_str().unwrap();
    assert!(
        command.ends_with("echo") || command.ends_with("echo.exe"),
        "{command}"
    );
    assert_eq!(given["mcpServers"][1]["args"][0], "hello");
    assert_eq!(given["mcpServers"][1]["env"][0]["name"], "SELFTEST_KEY");
    assert_eq!(given["mcpServers"][1]["env"][0]["value"], "value");
    assert_eq!(given["cwd"], project.display().to_string());
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

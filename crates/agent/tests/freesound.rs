//! The Freesound client and its tools against a local server. The tests never touch the real
//! Freesound.
//!
//! The client reads two things from the environment: the API address (so a test can point it
//! elsewhere) and the config folder (where the user's token lives). A test cannot set either
//! (edition 2024 makes `std::env::set_var` unsafe, and this crate forbids unsafe), so the test
//! runs itself again in a child process that has them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex};

use od_agent::{Handler, freesound};
use od_core::Workspace;
use serde_json::{Value, json};

/// The name of the variable that says "this is the child process".
const CHILD: &str = "OD_FREESOUND_TEST_CHILD";
/// The test's own name, so the child runs only this one.
const NAME: &str = "the_client_and_the_tools_work_against_a_local_server";

/// A server that answers like Freesound does, and remembers what it was asked.
struct Mock {
    base: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Mock {
    fn start() -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let base = format!("http://{address}/apiv2");
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let requests = seen.clone();
        let preview = format!("http://{address}/previews");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let requests = requests.clone();
                let preview = preview.clone();
                std::thread::spawn(move || serve(stream, requests, &preview));
            }
        });
        Mock { base, seen }
    }

    fn requests(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

/// Answers one request.
fn serve(mut stream: TcpStream, seen: Arc<Mutex<Vec<String>>>, preview: &str) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    reader.read_line(&mut request_line).unwrap();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" || line.is_empty() {
            break;
        }
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();
    seen.lock().unwrap().push(path.clone());
    let (status, content_type, body) = answer(&path, preview);
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
    let _ = stream.flush();
}

/// What the server answers for a path.
fn answer(path: &str, preview: &str) -> (&'static str, &'static str, Vec<u8>) {
    if path.starts_with("/apiv2/search/text/") {
        if path.contains("query=huge") {
            return ("200 OK", "application/json", vec![b'x'; 3 * 1024 * 1024]);
        }
        let body = json!({
            "count": 2,
            "next": null,
            "previous": null,
            "results": [
                {
                    "id": 7,
                    "name": "Bell",
                    "duration": 1.5,
                    "license": "Creative Commons 0",
                    "username": "someone",
                    "previews": {
                        "preview-hq-mp3": format!("{preview}/7_hq.mp3"),
                        "preview-lq-mp3": format!("{preview}/7_lq.mp3"),
                    },
                },
                {
                    "id": 8,
                    "name": "Drum loop",
                    "duration": 0.5,
                    "license": "Attribution",
                    "username": "other",
                    "previews": { "preview-hq-mp3": format!("{preview}/8_hq.mp3") },
                },
            ],
        });
        return (
            "200 OK",
            "application/json",
            serde_json::to_vec(&body).unwrap(),
        );
    }
    if path.starts_with("/apiv2/sounds/") {
        let id: u64 = path
            .trim_start_matches("/apiv2/sounds/")
            .split(['/', '?'])
            .next()
            .unwrap_or("")
            .parse()
            .unwrap_or(0);
        let body = match id {
            7 => json!({
                "id": 7,
                "name": "Bell",
                "duration": 1.5,
                "license": "Creative Commons 0",
                "username": "someone",
                "previews": { "preview-hq-mp3": format!("{preview}/7_hq.mp3") },
            }),
            8 => json!({
                "id": 8,
                "name": "Drum loop",
                "duration": 0.5,
                "license": "Attribution",
                "username": "other",
                "previews": { "preview-hq-mp3": format!("{preview}/8_hq.mp3") },
            }),
            _ => json!({ "detail": "Not found." }),
        };
        let status = if id == 7 || id == 8 {
            "200 OK"
        } else {
            "404 Not Found"
        };
        return (
            status,
            "application/json",
            serde_json::to_vec(&body).unwrap(),
        );
    }
    if path.starts_with("/apiv2/huge") {
        return ("200 OK", "application/json", vec![b'x'; 3 * 1024 * 1024]);
    }
    if path.starts_with("/previews/") {
        let mut body = b"ID3\x04\0\0\0\0\0\0".to_vec();
        body.extend_from_slice(&[b'I'; 56]);
        return ("200 OK", "audio/mpeg", body);
    }
    ("404 Not Found", "text/plain", b"nothing here".to_vec())
}

/// The parent process: it starts the server and runs the test again in a child that has the
/// environment the client reads.
#[test]
fn the_client_and_the_tools_work_against_a_local_server() {
    if std::env::var(CHILD).is_ok() {
        child();
        return;
    }
    let mock = Mock::start();
    let config = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(config.path().join("freesound")).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture"])
        .env(CHILD, "1")
        .env(freesound::URL_ENV, &mock.base)
        .env("OPENDESIGNER_CONFIG_DIR", config.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the test in the child process failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // The server saw the API's own paths, and the token only in the query string.
    let requests = mock.requests();
    assert!(
        requests
            .iter()
            .any(|r| r.starts_with("/apiv2/search/text/")),
        "the search was not asked for: {requests:?}"
    );
    assert!(
        requests.iter().any(|r| r.starts_with("/apiv2/sounds/8/")),
        "the sound was not asked for: {requests:?}"
    );
    assert!(
        requests.iter().any(|r| r.starts_with("/previews/")),
        "the preview was not downloaded: {requests:?}"
    );
    for request in &requests {
        assert!(
            !request.contains("test-token") || request.contains("token=test-token"),
            "the token should only travel in the query string: {request}"
        );
    }
    assert!(
        requests
            .iter()
            .filter(|r| r.starts_with("/apiv2/"))
            .all(|r| r.contains("token=test-token")),
        "every call to the API carries the token: {requests:?}"
    );
}

/// The child process: the client and the tools, against the local server.
fn child() {
    let config = std::path::PathBuf::from(std::env::var("OPENDESIGNER_CONFIG_DIR").unwrap());
    let token_file = config.join("freesound/token");

    // Without a token the tools say what to do.
    let _ = std::fs::remove_file(&token_file);
    let missing = freesound::search("bell", None, 1, true)
        .unwrap_err()
        .to_string();
    assert!(
        missing.contains("no Freesound token") && missing.contains("freesound.org/apiv2/apply/"),
        "the missing token should be explained: {missing}"
    );

    std::fs::write(&token_file, "test-token\n").unwrap();
    let (sounds, hidden) = freesound::search("bell", Some(2000), 1, true).unwrap();
    assert_eq!(sounds.len(), 2);
    assert_eq!(hidden, 0);
    assert_eq!(sounds[0].id, 7);
    assert_eq!(sounds[0].name, "Bell");
    assert_eq!(sounds[0].duration_ms, 1500);
    assert_eq!(sounds[0].license.spdx, "CC0-1.0");
    assert_eq!(
        sounds[0].license.url,
        "https://creativecommons.org/publicdomain/zero/1.0/"
    );
    assert_eq!(sounds[0].author, "someone");
    assert_eq!(
        sounds[0].attribution(),
        "\"Bell\" by someone (https://freesound.org/s/7/), licensed under CC0 1.0 (https://creativecommons.org/publicdomain/zero/1.0/)"
    );
    assert_eq!(sounds[1].license.spdx, "CC-BY-4.0");
    assert_eq!(
        sounds[1].license.url,
        "https://creativecommons.org/licenses/by/4.0/"
    );

    // An answer that is too big is refused.
    let huge = freesound::search("huge", None, 1, true)
        .unwrap_err()
        .to_string();
    assert!(
        huge.contains("larger than"),
        "a huge answer should be refused: {huge}"
    );

    // A sound that is not there is an error, not a silent nothing.
    let missing = freesound::sound(999).unwrap_err().to_string();
    assert!(
        missing.contains("404"),
        "a missing sound should be reported: {missing}"
    );

    // Importing puts the preview and what it is into the project.
    let project = tempfile::tempdir().unwrap();
    let imported = freesound::import(project.path(), 8, Some("drum"), true).unwrap();
    assert_eq!(imported.file, "media/drum.mp3");
    assert_eq!(imported.sidecar, "media/drum.license.json");
    assert_eq!(
        imported.attribution,
        "\"Drum loop\" by other (https://freesound.org/s/8/), licensed under CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/)"
    );
    assert!(project.path().join("media/drum.mp3").exists());
    let sidecar: Value = serde_json::from_str(
        &std::fs::read_to_string(project.path().join("media/drum.license.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(sidecar["schema"], 1);
    assert_eq!(sidecar["source"]["provider"], "freesound");
    assert_eq!(sidecar["source"]["item_id"], "8");
    assert_eq!(
        sidecar["source"]["landing_url"],
        "https://freesound.org/s/8/"
    );
    assert_eq!(sidecar["title"], "Drum loop");
    assert_eq!(sidecar["creator"]["name"], "other");
    assert_eq!(sidecar["license"]["spdx"], "CC-BY-4.0");
    assert_eq!(
        sidecar["license"]["url"],
        "https://creativecommons.org/licenses/by/4.0/"
    );
    assert!(
        sidecar["license"]["attribution_required"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(sidecar["credit_line"], imported.attribution);
    assert!(sidecar["sha256"].as_str().unwrap().len() == 64);

    // The same sound again gets its own name; nothing is overwritten.
    let again = freesound::import(project.path(), 8, Some("drum"), true).unwrap();
    assert_eq!(again.file, "media/drum-2.mp3");

    // The sounds in the project are listed with their attribution.
    let listed = od_agent::sound_license::list_records(project.path());
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].file, "media/drum-2.mp3");
    assert!(
        listed[0]
            .sidecar
            .as_ref()
            .unwrap()
            .credit_line
            .contains("Drum loop")
    );

    // The same through the tools.
    let ws = Workspace::create(&project.path().join("ws.od"), "Store").unwrap();
    let ws = Arc::new(Mutex::new(ws));
    let handler = Handler::new(ws.clone());
    let session = {
        let out = handler.handle(
            None,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-06-18","capabilities":{},
                "clientInfo":{"name":"claude-code","version":"1"}}}),
        );
        assert_eq!(out.status, 200);
        out.session_id.unwrap()
    };
    let call = |name: &str, arguments: Value| -> (bool, Value) {
        let out = handler.handle(
            Some(&session),
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
        );
        assert_eq!(out.status, 200, "{name}");
        let body = out.body.unwrap();
        let result = &body["result"];
        let text = result["content"][0]["text"].as_str().unwrap().to_string();
        let parsed = serde_json::from_str(&text).unwrap_or_else(|_| {
            panic!("{name} did not answer with JSON: {text}");
        });
        (result["isError"].as_bool().unwrap(), parsed)
    };

    let (is_error, found) = call(
        "search_sounds",
        json!({"query": "bell", "provider": "freesound"}),
    );
    assert!(!is_error, "the search failed: {found}");
    assert_eq!(found["provider"], "freesound");
    assert_eq!(found["sounds"].as_array().unwrap().len(), 2);
    assert_eq!(found["sounds"][0]["id"], "7");
    assert_eq!(found["sounds"][0]["license"], "CC0-1.0");
    assert!(
        found["sounds"][0]["attribution"]
            .as_str()
            .unwrap()
            .contains("Bell")
    );

    let (is_error, imported) = call(
        "import_sound",
        json!({"id": 7, "provider": "freesound", "name": "bell"}),
    );
    assert!(!is_error, "the import failed: {imported}");
    assert_eq!(imported["file"], "media/bell.mp3");
    assert_eq!(imported["sidecar"], "media/bell.license.json");
    assert_eq!(imported["license"], "CC0-1.0");
    assert_eq!(
        imported["attribution_required"], false,
        "CC0 asks for no credit"
    );
    assert_eq!(imported["attribution"], "");

    let (is_error, listed) = call("list_sounds", json!({}));
    assert!(!is_error, "the listing failed: {listed}");
    let rows = listed.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["file"], "media/bell.mp3");
    assert_eq!(rows[0]["source"], "freesound");
    assert_eq!(rows[0]["license"], "CC0-1.0");
    assert!(rows[0]["attribution"].as_str().is_none_or(str::is_empty));

    // The tool writes into the project and nowhere else.
    let dir = ws.lock().unwrap().dir().unwrap().to_path_buf();
    assert!(dir.join("media/bell.mp3").exists());
    assert!(dir.join("media/bell.license.json").exists());
}

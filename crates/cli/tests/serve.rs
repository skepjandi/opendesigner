#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

struct Kill(Child);
impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn post(port: u16, token: &str, session: Option<&str>, body: &str) -> (u16, String, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut req = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\
         Authorization: Bearer {token}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(id) = session {
        req.push_str(&format!("Mcp-Session-Id: {id}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, head.to_lowercase(), body.to_string())
}

/// The real binary, a real socket: start the server, connect as an agent, read the guidance and
/// apply an edit that lands on disk.
#[test]
fn serve_mcp_serves_a_project_to_an_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = env!("CARGO_BIN_EXE_opendesigner");
    let dir = tmp.path().join("p.od");
    assert!(
        Command::new(bin)
            .args(["new", dir.to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );

    let mut child = Kill(
        Command::new(bin)
            .args(["serve-mcp", dir.to_str().unwrap(), "--port", "0"])
            .env("OPENDESIGNER_CONFIG_DIR", tmp.path().join("cfg"))
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let out = child.0.stdout.take().unwrap();
    let mut lines = BufReader::new(out).lines();
    let first = lines.next().unwrap().unwrap();
    let url = first.split(" on ").nth(1).unwrap().to_string();
    let port: u16 = url
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches("/mcp")
        .parse()
        .unwrap();
    let token = std::fs::read_to_string(tmp.path().join("cfg/mcp-token")).unwrap();
    assert!(
        !first.contains(token.trim()),
        "the token is not printed by default"
    );

    let (status, head, body) = post(
        port,
        token.trim(),
        None,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"claude-code"}}}"#,
    );
    assert_eq!(status, 200, "{body}");
    let session = head
        .lines()
        .find_map(|l| l.strip_prefix("mcp-session-id: "))
        .unwrap()
        .to_string();

    let (_, _, guidance) = post(
        port,
        token.trim(),
        Some(&session),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_guidance","arguments":{}}}"#,
    );
    let g: serde_json::Value = serde_json::from_str(&guidance).unwrap();
    let text = g["result"]["content"][0]["text"].as_str().unwrap();
    let guidance: serde_json::Value = serde_json::from_str(text).unwrap();
    let root = guidance["pages"][0]["root"].as_str().unwrap().to_string();

    let call = serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
        "name":"apply_ops","arguments":{"summary":"add a card","ops":[
            {"op":"create","parent":root,"node":{"kind":"rect","name":"Card","width":10,"height":10}}]}}});
    let (status, _, applied) = post(port, token.trim(), Some(&session), &call.to_string());
    assert_eq!(status, 200);
    assert!(applied.contains("\\\"created\\\""), "{applied}");

    let page = std::fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    assert!(
        page.contains("\"Card\""),
        "the edit was saved to the project"
    );
    let log = std::fs::read_to_string(dir.join(".od/history/log.jsonl")).unwrap();
    assert!(log.contains("agent:claude-code"));

    let (status, _, _) = post(port, "wrong", Some(&session), &call.to_string());
    assert_eq!(status, 401);
}

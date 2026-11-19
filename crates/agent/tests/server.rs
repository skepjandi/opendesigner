#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use od_agent::{Handler, RunningServer, start};
use od_core::Workspace;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Response {
    status: u16,
    headers: String,
    body: String,
}

async fn boot() -> (RunningServer, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let ws = Workspace::create(&tmp.path().join("p.od"), "x").unwrap();
    let handler = Arc::new(Handler::new(Arc::new(Mutex::new(ws))));
    let server = start(handler, TOKEN.to_string(), 0).await.unwrap();
    (server, tmp)
}

/// One raw HTTP/1.1 request; `extra` are header lines, `host` overrides the Host header.
async fn request(
    server: &RunningServer,
    method: &str,
    extra: &[(&str, &str)],
    host: Option<&str>,
    body: &str,
) -> Response {
    let port = server.addr().port();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let host = host.map_or_else(|| format!("127.0.0.1:{port}"), str::to_string);
    let mut req = format!(
        "{method} /mcp HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    if !body.is_empty() {
        req.push_str(
            "Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\n",
        );
    }
    for (k, v) in extra {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    // the server may answer and close before an oversized body is fully sent
    let _ = stream.write_all(req.as_bytes()).await;
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw).await;
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    Response {
        status,
        headers: head.to_lowercase(),
        body: body.to_string(),
    }
}

fn auth() -> (&'static str, String) {
    ("Authorization", format!("Bearer {TOKEN}"))
}

fn initialize_body() -> String {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-06-18","clientInfo":{"name":"test-agent"}}})
    .to_string()
}

#[tokio::test]
async fn a_valid_request_gets_a_json_rpc_answer_and_a_session() {
    let (server, _tmp) = boot().await;
    let (k, v) = auth();
    let r = request(&server, "POST", &[(k, &v)], None, &initialize_body()).await;
    assert_eq!(r.status, 200);
    assert!(r.headers.contains("content-type: application/json"));
    assert!(r.headers.contains("mcp-session-id: "));
    let body: Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(body["result"]["serverInfo"]["name"], "opendesigner");
    server.stop().await;
}

#[tokio::test]
async fn requests_without_the_right_token_are_refused() {
    let (server, _tmp) = boot().await;
    let none = request(&server, "POST", &[], None, &initialize_body()).await;
    assert_eq!(none.status, 401);
    assert!(none.headers.contains("www-authenticate: bearer"));
    for bad in [
        "Bearer wrong".to_string(),
        format!("Bearer {}", &TOKEN[..63]),
        format!("Bearer {TOKEN}x"),
        TOKEN.to_string(),
        format!("Basic {TOKEN}"),
    ] {
        let r = request(
            &server,
            "POST",
            &[("Authorization", &bad)],
            None,
            &initialize_body(),
        )
        .await;
        assert_eq!(r.status, 401, "{bad}");
    }
    server.stop().await;
}

#[tokio::test]
async fn web_pages_cannot_reach_it_even_with_a_stolen_token() {
    let (server, _tmp) = boot().await;
    let (k, v) = auth();
    // a browser request carries Origin
    let r = request(
        &server,
        "POST",
        &[(k, &v), ("Origin", "https://evil.example")],
        None,
        &initialize_body(),
    )
    .await;
    assert_eq!(r.status, 403);
    // DNS rebinding: the page's own hostname arrives as Host
    for host in [
        "evil.example",
        "evil.example:80",
        "127.0.0.1",
        "localhost:1",
        "127.0.0.1:1",
    ] {
        let r = request(&server, "POST", &[(k, &v)], Some(host), &initialize_body()).await;
        assert_eq!(r.status, 403, "Host {host}");
    }
    let port = server.addr().port();
    let local = request(
        &server,
        "POST",
        &[(k, &v)],
        Some(&format!("localhost:{port}")),
        &initialize_body(),
    )
    .await;
    assert_eq!(local.status, 200, "localhost with the right port is fine");
    server.stop().await;
}

#[tokio::test]
async fn only_post_and_delete_exist_and_bodies_are_validated() {
    let (server, _tmp) = boot().await;
    let (k, v) = auth();
    let get = request(
        &server,
        "GET",
        &[(k, &v), ("Accept", "text/event-stream")],
        None,
        "",
    )
    .await;
    assert_eq!(get.status, 405);
    assert!(get.headers.contains("allow: post, delete"));
    let junk = request(&server, "POST", &[(k, &v)], None, "{not json").await;
    assert_eq!(junk.status, 400);
    assert!(junk.body.contains("-32700"));
    let huge = "x".repeat(5 * 1024 * 1024);
    let big = request(&server, "POST", &[(k, &v)], None, &huge).await;
    assert_eq!(big.status, 413);
    let bad_version = request(
        &server,
        "POST",
        &[(k, &v), ("MCP-Protocol-Version", "1999-01-01")],
        None,
        &initialize_body(),
    )
    .await;
    assert_eq!(bad_version.status, 400);
    server.stop().await;
}

#[tokio::test]
async fn sessions_notifications_and_delete_follow_the_transport_rules() {
    let (server, _tmp) = boot().await;
    let (k, v) = auth();
    let init = request(&server, "POST", &[(k, &v)], None, &initialize_body()).await;
    let sid = init
        .headers
        .lines()
        .find_map(|l| l.strip_prefix("mcp-session-id: "))
        .unwrap()
        .to_string();

    let note = json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string();
    let r = request(
        &server,
        "POST",
        &[(k, &v), ("Mcp-Session-Id", &sid)],
        None,
        &note,
    )
    .await;
    assert_eq!(r.status, 202);
    assert!(r.body.is_empty());

    let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string();
    let ok = request(
        &server,
        "POST",
        &[(k, &v), ("Mcp-Session-Id", &sid)],
        None,
        &list,
    )
    .await;
    assert_eq!(ok.status, 200);
    assert!(ok.body.contains("get_guidance"));
    let missing = request(&server, "POST", &[(k, &v)], None, &list).await;
    assert_eq!(missing.status, 400);

    let del = request(
        &server,
        "DELETE",
        &[(k, &v), ("Mcp-Session-Id", &sid)],
        None,
        "",
    )
    .await;
    assert_eq!(del.status, 200);
    let gone = request(
        &server,
        "POST",
        &[(k, &v), ("Mcp-Session-Id", &sid)],
        None,
        &list,
    )
    .await;
    assert_eq!(gone.status, 404);
    server.stop().await;
}

#[tokio::test]
async fn it_binds_loopback_only_and_falls_forward_when_the_port_is_taken() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = Arc::new(Mutex::new(
        Workspace::create(&tmp.path().join("p.od"), "x").unwrap(),
    ));
    let a = start(Arc::new(Handler::new(ws.clone())), TOKEN.into(), 0)
        .await
        .unwrap();
    assert!(a.addr().ip().is_loopback());
    let taken = a.addr().port();
    let b = start(Arc::new(Handler::new(ws)), TOKEN.into(), taken)
        .await
        .unwrap();
    assert!(
        b.addr().port() > taken,
        "{} then {}",
        taken,
        b.addr().port()
    );
    assert!(b.url().starts_with("http://127.0.0.1:"));
    a.stop().await;
    b.stop().await;
}

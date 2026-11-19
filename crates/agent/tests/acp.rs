#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use od_agent::acp::{AcpSession, AgentCommand, ChatEvent, McpEndpoint};
use serde_json::Value;
use tokio::sync::mpsc;

/// The scripted agent's executable. Cargo says where it put it (the layout of `target/` differs
/// between versions), so ask it to build the example and report the path.
fn mock() -> String {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        let out = std::process::Command::new(env!("CARGO"))
            .args([
                "build",
                "--example",
                "mock_acp_agent",
                "-p",
                "opendesigner-agent",
                "--message-format=json",
            ])
            .stderr(std::process::Stdio::inherit())
            .output()
            .unwrap();
        assert!(out.status.success(), "building the mock agent failed");
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|m| {
                m["reason"] == "compiler-artifact" && m["target"]["name"] == "mock_acp_agent"
            })
            .find_map(|m| m["executable"].as_str().map(str::to_string))
            .expect("cargo did not report the mock agent's executable")
    })
    .clone()
}

fn command(mode: &str, log: Option<&std::path::Path>) -> AgentCommand {
    let mut args = vec![mode.to_string()];
    args.extend(log.map(|p| p.display().to_string()));
    AgentCommand {
        program: mock(),
        args,
        cwd: std::env::temp_dir(),
    }
}

fn endpoint() -> McpEndpoint {
    McpEndpoint {
        url: "http://127.0.0.1:45790/mcp".into(),
        token: "secret-token".into(),
    }
}

async fn start(
    mode: &str,
    log: Option<&std::path::Path>,
) -> (
    Result<AcpSession, od_agent::AgentError>,
    mpsc::UnboundedSender<ChatEvent>,
    mpsc::UnboundedReceiver<ChatEvent>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    let session = AcpSession::start(&command(mode, log), &endpoint(), tx.clone()).await;
    (session, tx, rx)
}

async fn next(rx: &mut mpsc::UnboundedReceiver<ChatEvent>) -> ChatEvent {
    tokio::time::timeout(Duration::from_secs(20), rx.recv())
        .await
        .expect("an event in time")
        .expect("the channel is open")
}

#[tokio::test]
async fn a_turn_streams_asks_permission_refuses_files_and_ends() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("session-new.json");
    let (session, tx, mut rx) = start("", Some(&log)).await;
    let session = Arc::new(session.unwrap());

    // the agent was pointed at the local MCP server with the bearer token, and nothing else
    let params: Value = serde_json::from_str(&std::fs::read_to_string(&log).unwrap()).unwrap();
    assert_eq!(params["mcpServers"].as_array().unwrap().len(), 1);
    let server = &params["mcpServers"][0];
    assert_eq!(server["type"], "http");
    assert_eq!(server["url"], "http://127.0.0.1:45790/mcp");
    assert_eq!(server["headers"][0]["name"], "Authorization");
    assert_eq!(server["headers"][0]["value"], "Bearer secret-token");

    session.prompt("make it bigger", tx.clone());
    let mut text = String::new();
    let mut saw = Vec::new();
    loop {
        match next(&mut rx).await {
            ChatEvent::Text(t) => text.push_str(&t),
            ChatEvent::Thought(t) => saw.push(format!("thought:{t}")),
            ChatEvent::ToolCall { id, title, status } => {
                saw.push(format!("call:{id}:{title}:{status}"))
            }
            ChatEvent::ToolUpdate { id, status, .. } => {
                saw.push(format!("update:{id}:{}", status.unwrap()))
            }
            ChatEvent::Plan(entries) => {
                saw.push(format!(
                    "plan:{}",
                    entries
                        .iter()
                        .map(|e| format!("{}={}", e.content, e.status))
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
            ChatEvent::Permission {
                request,
                title,
                options,
            } => {
                assert_eq!(request, 900);
                assert_eq!(title, "Run a command");
                assert_eq!(options.len(), 2);
                assert_eq!(options[0].kind, "allow_once");
                session.answer_permission(request, Some(&options[0].id));
            }
            ChatEvent::TurnEnded(reason) => {
                assert_eq!(reason, "end_turn");
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(
        text, "Hello world [fs refused] [chose yes]",
        "files are not served"
    );
    assert_eq!(
        saw,
        [
            "thought:thinking",
            "plan:Read the guidance=in_progress,Make it bigger=pending",
            "call:t1:get_guidance:pending",
            "update:t1:completed",
            "plan:Read the guidance=completed,Make it bigger=in_progress"
        ]
    );
    session.shutdown().await;
}

#[tokio::test]
async fn permission_can_be_refused_or_cancelled_and_a_turn_can_be_cancelled() {
    let (session, tx, mut rx) = start("", None).await;
    let session = Arc::new(session.unwrap());

    for (answer, expected) in [(Some("no"), "[chose no]"), (None, "[permission cancelled]")] {
        session.prompt("go", tx.clone());
        let mut text = String::new();
        loop {
            match next(&mut rx).await {
                ChatEvent::Text(t) => text.push_str(&t),
                ChatEvent::Permission { request, .. } => session.answer_permission(request, answer),
                ChatEvent::TurnEnded(_) => break,
                _ => {}
            }
        }
        assert!(text.ends_with(expected), "{text}");
    }

    // a turn that keeps waiting ends as cancelled when the user cancels it
    session.prompt("wait for me", tx.clone());
    loop {
        match next(&mut rx).await {
            ChatEvent::Permission { request, .. } => {
                session.answer_permission(request, Some("yes"))
            }
            ChatEvent::Text(t) if t.contains("[chose yes]") => session.cancel(),
            ChatEvent::TurnEnded(reason) => {
                assert_eq!(reason, "cancelled");
                break;
            }
            _ => {}
        }
    }
    session.shutdown().await;
}

#[tokio::test]
async fn setup_failures_say_what_to_do() {
    for (mode, expected) in [
        ("version", "ACP version 2"),
        ("nohttp", "over HTTP"),
        ("auth", "sign in first"),
    ] {
        let (session, _tx, _rx) = start(mode, None).await;
        let message = session.err().expect("setup should fail").to_string();
        assert!(message.contains(expected), "{mode}: {message}");
        if mode == "auth" {
            assert!(
                message.contains("mock-agent login"),
                "the agent's own hint is included: {message}"
            );
        }
    }
    let (tx, _rx) = mpsc::unbounded_channel();
    let missing = AgentCommand {
        program: "od-no-such-agent-binary".into(),
        args: Vec::new(),
        cwd: std::env::temp_dir(),
    };
    let err = AcpSession::start(&missing, &endpoint(), tx)
        .await
        .err()
        .unwrap();
    assert!(err.to_string().contains("cannot start"), "{err}");
}

#[tokio::test]
async fn an_agent_that_dies_mid_turn_fails_the_turn_and_reports_the_exit() {
    let (session, tx, mut rx) = start("", None).await;
    let session = Arc::new(session.unwrap());
    session.prompt("crash now", tx.clone());
    let (mut failed, mut exited) = (false, false);
    while !(failed && exited) {
        match next(&mut rx).await {
            ChatEvent::Failed(m) => {
                assert!(m.contains("exited"), "{m}");
                failed = true;
            }
            ChatEvent::Exited(_) => exited = true,
            _ => {}
        }
    }
}

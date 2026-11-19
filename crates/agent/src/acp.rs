//! A client for the Agent Client Protocol (v1): the in-app chat starts the coding agent the user
//! already uses (an ACP adapter such as `gemini --acp`) as a child process, speaks newline-delimited
//! JSON-RPC 2.0 to it over stdio, and points it at OpenDesigner's local MCP server so what it does
//! goes through the same validated, attributed, reversible operations as any other agent.
//!
//! What this client does **not** do: it declares no file-system or terminal capability (agents do
//! their own file work in their own process, inside the session's working directory, and ask for
//! permission themselves), it never starts anything except the command the user configured, and it
//! holds no agent credentials: the agent signs in on its own.
//!
//! Spec: <https://agentclientprotocol.com/protocol/v1/overview>. The protocol is moving (a v2 is
//! being drafted); this speaks v1 and refuses an agent that answers with another version.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

use crate::error::AgentError;

const PROTOCOL_VERSION: u64 = 1;
/// Long enough for `npx` to download an adapter the first time (measured: over a minute).
const SETUP_TIMEOUT: Duration = Duration::from_secs(240);
const STDERR_LINES: usize = 40;
/// Longest line read from the agent (a guard against an agent that never ends one).
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;

/// What to start and where.
#[derive(Debug, Clone)]
pub struct AgentCommand {
    pub program: String,
    pub args: Vec<String>,
    /// The session's working directory: the project folder.
    pub cwd: PathBuf,
}

/// The local MCP server the agent should use.
#[derive(Debug, Clone)]
pub struct McpEndpoint {
    pub url: String,
    pub token: String,
}

/// One choice offered with a permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionOption {
    pub id: String,
    pub name: String,
    /// `allow_once`, `allow_always`, `reject_once` or `reject_always`.
    pub kind: String,
}

/// What the agent reports while a turn runs, in the order it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    Text(String),
    Thought(String),
    ToolCall {
        id: String,
        title: String,
        status: String,
    },
    ToolUpdate {
        id: String,
        title: Option<String>,
        status: Option<String>,
    },
    /// The agent wants permission; answer with [`AcpSession::answer_permission`].
    Permission {
        request: u64,
        title: String,
        options: Vec<PermissionOption>,
    },
    /// The turn finished (`end_turn`, `cancelled`, `max_tokens`, ...).
    TurnEnded(String),
    /// The agent refused or failed a request.
    Failed(String),
    /// The agent process ended.
    Exited(String),
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// A running agent and its one session.
pub struct AcpSession {
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicU64,
    session_id: String,
    child: Arc<tokio::sync::Mutex<Child>>,
    stderr: Arc<Mutex<VecDeque<String>>>,
}

fn rpc_error(v: &Value) -> String {
    let message = v["message"].as_str().unwrap_or("unknown error");
    match v["code"].as_i64() {
        Some(-32000) => {
            format!("the agent needs you to sign in first (run it once in a terminal): {message}")
        }
        Some(code) => format!("{message} (code {code})"),
        None => message.to_string(),
    }
}

fn text_of(content: &Value) -> Option<String> {
    (content["type"] == "text")
        .then(|| content["text"].as_str().map(str::to_string))
        .flatten()
}

fn event_for(update: &Value) -> Option<ChatEvent> {
    match update["sessionUpdate"].as_str()? {
        "agent_message_chunk" => text_of(&update["content"]).map(ChatEvent::Text),
        "agent_thought_chunk" => text_of(&update["content"]).map(ChatEvent::Thought),
        "tool_call" => Some(ChatEvent::ToolCall {
            id: update["toolCallId"].as_str()?.to_string(),
            title: update["title"].as_str().unwrap_or("tool call").to_string(),
            status: update["status"].as_str().unwrap_or("pending").to_string(),
        }),
        "tool_call_update" => Some(ChatEvent::ToolUpdate {
            id: update["toolCallId"].as_str()?.to_string(),
            title: update["title"].as_str().map(str::to_string),
            status: update["status"].as_str().map(str::to_string),
        }),
        _ => None,
    }
}

fn permission_event(id: u64, params: &Value) -> ChatEvent {
    let options = params["options"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|o| {
                    Some(PermissionOption {
                        id: o["optionId"].as_str()?.to_string(),
                        name: o["name"].as_str().unwrap_or("").to_string(),
                        kind: o["kind"].as_str().unwrap_or("").to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    ChatEvent::Permission {
        request: id,
        title: params["toolCall"]["title"]
            .as_str()
            .unwrap_or("the agent asks for permission")
            .to_string(),
        options,
    }
}

impl AcpSession {
    /// Starts the agent, negotiates the protocol and opens a session that uses `mcp`. Events of
    /// the session arrive on `events` until the agent exits.
    pub async fn start(
        command: &AgentCommand,
        mcp: &McpEndpoint,
        events: mpsc::UnboundedSender<ChatEvent>,
    ) -> Result<AcpSession, AgentError> {
        let mut child = Command::new(&command.program)
            .args(&command.args)
            .current_dir(&command.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                AgentError::Config(format!(
                    "cannot start {:?}: {e}. Is it installed and on your PATH?",
                    command.program
                ))
            })?;
        let (stdin, stdout, stderr) =
            match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
                (Some(i), Some(o), Some(e)) => (i, o, e),
                _ => {
                    return Err(AgentError::Config(
                        "the agent's pipes are unavailable".into(),
                    ));
                }
            };

        let (out, mut out_rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(line) = out_rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err()
                    || stdin.write_all(b"\n").await.is_err()
                    || stdin.flush().await.is_err()
                {
                    break;
                }
            }
        });

        let stderr_log: Arc<Mutex<VecDeque<String>>> = Arc::default();
        let log = Arc::clone(&stderr_log);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(mut l) = log.lock() {
                    if l.len() >= STDERR_LINES {
                        l.pop_front();
                    }
                    l.push_back(line);
                }
            }
        });

        let pending: Pending = Arc::default();
        let child = Arc::new(tokio::sync::Mutex::new(child));
        spawn_reader(
            stdout,
            Arc::clone(&pending),
            out.clone(),
            events,
            Arc::clone(&child),
            Arc::clone(&stderr_log),
        );

        let mut session = AcpSession {
            out,
            pending,
            next_id: AtomicU64::new(1),
            session_id: String::new(),
            child,
            stderr: stderr_log,
        };
        let setup = async {
            let init = session
                .request(
                    "initialize",
                    json!({
                        "protocolVersion": PROTOCOL_VERSION,
                        "clientCapabilities": {},
                        "clientInfo": {"name": "opendesigner", "title": "OpenDesigner",
                                       "version": env!("CARGO_PKG_VERSION")},
                    }),
                )
                .await?;
            if init["protocolVersion"].as_u64() != Some(PROTOCOL_VERSION) {
                return Err(format!(
                    "the agent speaks ACP version {}, this build speaks {PROTOCOL_VERSION}",
                    init["protocolVersion"]
                ));
            }
            if init["agentCapabilities"]["mcpCapabilities"]["http"] != true {
                return Err("this agent cannot connect to MCP servers over HTTP, which the editor's local server needs"
                    .to_string());
            }
            let created = session
                .request(
                    "session/new",
                    json!({
                        "cwd": command.cwd.display().to_string(),
                        "mcpServers": [{
                            "type": "http",
                            "name": "opendesigner",
                            "url": mcp.url,
                            "headers": [{"name": "Authorization",
                                         "value": format!("Bearer {}", mcp.token)}],
                        }],
                    }),
                )
                .await?;
            created["sessionId"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "the agent did not return a session id".to_string())
        };
        match tokio::time::timeout(SETUP_TIMEOUT, setup).await {
            Ok(Ok(id)) => {
                session.session_id = id;
                Ok(session)
            }
            Ok(Err(message)) => Err(session.failure(message).await),
            Err(_) => Err(session
                .failure("the agent did not answer in time".to_string())
                .await),
        }
    }

    /// A setup failure with the tail of the agent's own error output, which usually says why.
    async fn failure(&self, message: String) -> AgentError {
        // the agent's own explanation is often still in flight on stderr
        tokio::time::sleep(Duration::from_millis(200)).await;
        let tail = self
            .stderr
            .lock()
            .map(|l| {
                l.iter()
                    .rev()
                    .take(5)
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if tail.is_empty() {
            AgentError::Config(message)
        } else {
            AgentError::Config(format!("{message}\n{tail}"))
        }
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| "internal error".to_string())?
            .insert(id, tx);
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.out
            .send(line.to_string())
            .map_err(|_| "the agent has exited".to_string())?;
        rx.await.map_err(|_| "the agent has exited".to_string())?
    }

    /// Sends a user message. The turn runs in the background: progress arrives as events, ending
    /// with `TurnEnded` or `Failed`.
    pub fn prompt(self: &Arc<Self>, text: &str, events: mpsc::UnboundedSender<ChatEvent>) {
        let me = Arc::clone(self);
        let text = text.to_string();
        tokio::spawn(async move {
            let result = me
                .request(
                    "session/prompt",
                    json!({"sessionId": me.session_id,
                           "prompt": [{"type": "text", "text": text}]}),
                )
                .await;
            let event = match result {
                Ok(v) => {
                    ChatEvent::TurnEnded(v["stopReason"].as_str().unwrap_or("end_turn").to_string())
                }
                Err(e) => ChatEvent::Failed(e),
            };
            let _ = events.send(event);
        });
    }

    /// Asks the agent to stop the running turn.
    pub fn cancel(&self) {
        let line = json!({"jsonrpc": "2.0", "method": "session/cancel",
                          "params": {"sessionId": self.session_id}});
        let _ = self.out.send(line.to_string());
    }

    /// Answers a permission request: the option the user picked, or `None` to cancel.
    pub fn answer_permission(&self, request: u64, option: Option<&str>) {
        let outcome = match option {
            Some(id) => json!({"outcome": "selected", "optionId": id}),
            None => json!({"outcome": "cancelled"}),
        };
        let line = json!({"jsonrpc": "2.0", "id": request, "result": {"outcome": outcome}});
        let _ = self.out.send(line.to_string());
    }

    /// Ends the agent process.
    pub async fn shutdown(&self) {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
    }
}

fn spawn_reader(
    stdout: tokio::process::ChildStdout,
    pending: Pending,
    out: mpsc::UnboundedSender<String>,
    events: mpsc::UnboundedSender<ChatEvent>,
    child: Arc<tokio::sync::Mutex<Child>>,
    stderr: Arc<Mutex<VecDeque<String>>>,
) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) if line.len() > MAX_LINE_BYTES => {
                    let _ = events.send(ChatEvent::Failed(
                        "the agent sent an oversized message".into(),
                    ));
                    break;
                }
                Ok(_) => {}
            }
            // anything that is not a JSON-RPC message (a banner, a log line) is ignored
            let Ok(message) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            handle_message(&message, &pending, &out, &events);
        }
        // the agent is gone: fail whatever was waiting
        if let Ok(mut p) = pending.lock() {
            for (_, tx) in p.drain() {
                let _ = tx.send(Err("the agent has exited".to_string()));
            }
        }
        let code = child
            .lock()
            .await
            .try_wait()
            .ok()
            .flatten()
            .map_or("unknown".to_string(), |s| s.to_string());
        let tail = stderr
            .lock()
            .map(|l| {
                l.iter()
                    .rev()
                    .take(3)
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_default();
        let _ = events.send(ChatEvent::Exited(
            format!("{code} {tail}").trim().to_string(),
        ));
    });
}

fn handle_message(
    message: &Value,
    pending: &Pending,
    out: &mpsc::UnboundedSender<String>,
    events: &mpsc::UnboundedSender<ChatEvent>,
) {
    let id = message["id"].as_u64();
    match (message["method"].as_str(), id) {
        // a response to one of our requests
        (None, Some(id)) => {
            let result = if message["error"].is_null() {
                Ok(message["result"].clone())
            } else {
                Err(rpc_error(&message["error"]))
            };
            if let Ok(mut p) = pending.lock() {
                if let Some(tx) = p.remove(&id) {
                    let _ = tx.send(result);
                }
            }
        }
        (Some("session/update"), None) => {
            if let Some(event) = event_for(&message["params"]["update"]) {
                let _ = events.send(event);
            }
        }
        (Some("session/request_permission"), Some(id)) => {
            let _ = events.send(permission_event(id, &message["params"]));
        }
        // a request we do not serve (files, terminals): no such capability was declared
        (Some(_), Some(id)) => {
            let reply = json!({"jsonrpc": "2.0", "id": id,
                               "error": {"code": -32601, "message": "method not supported by this client"}});
            let _ = out.send(reply.to_string());
        }
        _ => {}
    }
}

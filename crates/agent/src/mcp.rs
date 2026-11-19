//! The MCP protocol layer: JSON-RPC 2.0 over Streamable HTTP, without the transport. A request
//! goes in, an `Outcome` (status, optional JSON body, optional new session id) comes out, so the
//! protocol is testable without a socket.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use od_core::{Actor, NodeId, Workspace};
use serde_json::{Value, json};

use crate::token;
use crate::tools::{self, CallCtx, Content, Turns};

/// Protocol versions we speak, newest first.
pub const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const MAX_SESSIONS: usize = 64;

const INSTRUCTIONS: &str = "OpenDesigner is a design editor. Call get_guidance first: it returns \
the user's selection, the notes they pinned and the edits they made after your last turn. Change \
the design only with apply_ops, look at the result with render_preview, resolve notes you acted \
on, and validate store exports with export or validate_export. Every change you make is \
attributed to you, checkpointed and reversible by the user.";

/// What changed, for front ends that show it (the editor outlines the touched nodes).
#[derive(Debug, Clone)]
pub struct ChangeEvent {
    pub actor: String,
    pub tool: String,
    pub touched: Vec<NodeId>,
}

pub type ChangeHook = Arc<dyn Fn(ChangeEvent) + Send + Sync>;

pub struct Outcome {
    pub status: u16,
    pub body: Option<Value>,
    pub session_id: Option<String>,
}

impl Outcome {
    fn json(body: Value) -> Outcome {
        Outcome {
            status: 200,
            body: Some(body),
            session_id: None,
        }
    }

    fn accepted() -> Outcome {
        Outcome {
            status: 202,
            body: None,
            session_id: None,
        }
    }

    fn status(status: u16) -> Outcome {
        Outcome {
            status,
            body: None,
            session_id: None,
        }
    }
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

struct ClientSession {
    actor: Actor,
}

struct State {
    sessions: HashMap<String, ClientSession>,
    turns: Turns,
}

pub struct Handler {
    ws: Arc<Mutex<Workspace>>,
    state: Mutex<State>,
    on_change: Option<ChangeHook>,
}

fn sanitize_client_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
        .take(64)
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        "mcp-client".to_string()
    } else {
        cleaned
    }
}

impl Handler {
    pub fn new(ws: Arc<Mutex<Workspace>>) -> Handler {
        Handler {
            ws,
            state: Mutex::new(State {
                sessions: HashMap::new(),
                turns: Turns::default(),
            }),
            on_change: None,
        }
    }

    pub fn with_change_hook(mut self, hook: ChangeHook) -> Handler {
        self.on_change = Some(hook);
        self
    }

    pub fn workspace(&self) -> &Arc<Mutex<Workspace>> {
        &self.ws
    }

    /// Ends a session; false when it did not exist.
    pub fn end_session(&self, id: &str) -> bool {
        self.state
            .lock()
            .map(|mut s| s.sessions.remove(id).is_some())
            .unwrap_or(false)
    }

    /// Handles one POST body. `session` is the `Mcp-Session-Id` header, if any.
    pub fn handle(&self, session: Option<&str>, msg: Value) -> Outcome {
        let Value::Object(obj) = &msg else {
            return Outcome {
                status: 400,
                body: Some(rpc_error(
                    Value::Null,
                    -32600,
                    "expected one JSON-RPC message",
                )),
                session_id: None,
            };
        };
        if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Outcome {
                status: 400,
                body: Some(rpc_error(Value::Null, -32600, "jsonrpc must be \"2.0\"")),
                session_id: None,
            };
        }
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            // a response to a server request: we send none, accept and ignore
            return Outcome::accepted();
        };
        let params = obj.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = obj.get("id").cloned() else {
            return self.notification(session, method);
        };
        if method == "initialize" {
            return self.initialize(id, &params);
        }
        let Some(actor) = self.session_actor(session) else {
            return self.missing_session(session, id);
        };
        match method {
            "ping" => Outcome::json(rpc_result(id, json!({}))),
            "tools/list" => Outcome::json(rpc_result(id, json!({"tools": tools::definitions()}))),
            "tools/call" => Outcome::json(self.tools_call(id, actor, &params)),
            _ => Outcome::json(rpc_error(
                id,
                -32601,
                &format!("method not found: {method}"),
            )),
        }
    }

    fn missing_session(&self, session: Option<&str>, id: Value) -> Outcome {
        match session {
            None => Outcome {
                status: 400,
                body: Some(rpc_error(id, -32600, "missing Mcp-Session-Id header")),
                session_id: None,
            },
            Some(_) => Outcome::status(404),
        }
    }

    fn notification(&self, session: Option<&str>, method: &str) -> Outcome {
        if method == "notifications/initialized" || method.starts_with("notifications/") {
            return match session {
                Some(s) if self.session_actor(Some(s)).is_none() => Outcome::status(404),
                _ => Outcome::accepted(),
            };
        }
        Outcome::accepted()
    }

    fn session_actor(&self, session: Option<&str>) -> Option<Actor> {
        let id = session?;
        self.state
            .lock()
            .ok()?
            .sessions
            .get(id)
            .map(|s| s.actor.clone())
    }

    fn initialize(&self, id: Value, params: &Value) -> Outcome {
        let requested = params.get("protocolVersion").and_then(Value::as_str);
        let version = requested
            .filter(|v| SUPPORTED_VERSIONS.contains(v))
            .unwrap_or(SUPPORTED_VERSIONS[0]);
        let client = params
            .pointer("/clientInfo/name")
            .and_then(Value::as_str)
            .map_or_else(|| "mcp-client".to_string(), sanitize_client_name);
        // never fall back to attributing an agent's edits to the user
        let Ok(actor) = Actor::agent(&client) else {
            return Outcome {
                status: 500,
                body: Some(rpc_error(id, -32603, "could not name the client")),
                session_id: None,
            };
        };
        let Ok(sid) = token::session_id() else {
            return Outcome {
                status: 500,
                body: Some(rpc_error(id, -32603, "could not create a session")),
                session_id: None,
            };
        };
        {
            let Ok(mut state) = self.state.lock() else {
                return Outcome::status(500);
            };
            if state.sessions.len() >= MAX_SESSIONS {
                return Outcome {
                    status: 503,
                    body: Some(rpc_error(id, -32000, "too many sessions; end one first")),
                    session_id: None,
                };
            }
            state.sessions.insert(sid.clone(), ClientSession { actor });
        }
        Outcome {
            status: 200,
            body: Some(rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "opendesigner", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": INSTRUCTIONS,
                }),
            )),
            session_id: Some(sid),
        }
    }

    fn tools_call(&self, id: Value, actor: Actor, params: &Value) -> Value {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return rpc_error(id, -32602, "tools/call needs a tool name");
        };
        let args = params.get("arguments").cloned().unwrap_or(Value::Null);
        let (Ok(mut ws), Ok(mut state)) = (self.ws.lock(), self.state.lock()) else {
            return rpc_error(id, -32603, "the editor state is unavailable");
        };
        let version_before = ws.session.log().len();
        let mut ctx = CallCtx {
            ws: &mut ws,
            actor: actor.clone(),
            turns: &mut state.turns,
        };
        let result = tools::call(&mut ctx, name, args);
        let touched = match &result {
            Ok(_) => ws.session.log()[version_before..]
                .iter()
                .flat_map(|tx| tx.ops.iter().flat_map(|a| a.touched.clone()))
                .collect(),
            Err(_) => Vec::new(),
        };
        drop(state);
        drop(ws);
        if !touched.is_empty() {
            if let Some(hook) = &self.on_change {
                hook(ChangeEvent {
                    actor: actor.to_string(),
                    tool: name.to_string(),
                    touched,
                });
            }
        }
        match result {
            Ok(out) => rpc_result(
                id,
                json!({"content": out.content.iter().map(content_json).collect::<Vec<_>>(),
                       "isError": false}),
            ),
            Err(e) => rpc_result(
                id,
                json!({"content": [{"type": "text", "text": e.0}], "isError": true}),
            ),
        }
    }
}

fn content_json(c: &Content) -> Value {
    match c {
        Content::Text(t) => json!({"type": "text", "text": t}),
        Content::Image { data, mime } => json!({"type": "image", "data": data, "mimeType": mime}),
    }
}

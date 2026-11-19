//! The in-app chat: a conversation with the coding agent the user already uses, over the Agent
//! Client Protocol (`od_agent::acp`). This holds the conversation and the connection; it has no Qt
//! in it, and the editor object forwards to it. The agent edits the design through the same MCP
//! server as any other agent, so its work is attributed, checkpointed and outlined as usual.

use std::sync::Arc;

use od_agent::acp::{AcpSession, AgentCommand, ChatEvent, McpEndpoint};
use serde_json::{Value, json};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

/// What reaches the editor from the connection.
pub enum ChatInput {
    /// The agent finished starting (or failed to).
    Started(Result<AcpSession, String>),
    Event(ChatEvent),
}

pub type ChatHook = Arc<dyn Fn(ChatInput) + Send + Sync>;

const MAX_MESSAGES: usize = 400;

const PREAMBLE: &str = "You are connected to OpenDesigner, a design editor, through its MCP \
server. Call get_guidance first: it returns what the user has selected, the notes they pinned \
and what they changed by hand since your last turn. Change the design with apply_ops, look at it \
with render_preview, and say in the user's terms what you changed.\n\n";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Off,
    Starting,
    Ready,
    Busy,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Status::Off => "off",
            Status::Starting => "starting",
            Status::Ready => "ready",
            Status::Busy => "busy",
        }
    }
}

pub struct Chat {
    runtime: Runtime,
    session: Option<Arc<AcpSession>>,
    events: Option<mpsc::UnboundedSender<ChatEvent>>,
    messages: Vec<Value>,
    status: Status,
    permission: Option<Value>,
    first_prompt: bool,
}

/// A coding agent the chat knows how to start.
struct Preset {
    label: &'static str,
    command: &'static str,
    /// Programs that must be on `PATH` for it to work; all of them.
    needs: &'static [&'static str],
}

/// Agents that speak ACP, most direct first. `omp`, `gemini` and `opencode` serve it themselves;
/// the other two run through the adapters their vendors publish.
const PRESETS: &[Preset] = &[
    Preset {
        label: "omp",
        command: "omp acp",
        needs: &["omp"],
    },
    Preset {
        label: "Claude Code",
        command: "npx -y @agentclientprotocol/claude-agent-acp",
        needs: &["npx", "claude"],
    },
    Preset {
        label: "Codex",
        command: "npx -y @agentclientprotocol/codex-acp",
        needs: &["npx", "codex"],
    },
    Preset {
        label: "Gemini CLI",
        command: "gemini --acp",
        needs: &["gemini"],
    },
    Preset {
        label: "opencode",
        command: "opencode acp",
        needs: &["opencode"],
    },
    Preset {
        label: "Other",
        command: "",
        needs: &[],
    },
];

/// Whether an executable called `program` is in one of the `PATH` directories.
fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let names: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::split_paths(&path).any(|dir| {
        names
            .iter()
            .any(|ext| dir.join(format!("{program}{ext}")).is_file())
    })
}

/// The chat's agent choices as JSON: `[{label, command, found}]`. `found` is whether the agent
/// is installed here, judged by the programs it needs being on `PATH`.
pub fn presets_json() -> String {
    let rows: Vec<serde_json::Value> = PRESETS
        .iter()
        .map(|p| {
            serde_json::json!({
                "label": p.label,
                "command": p.command,
                "found": !p.needs.is_empty() && p.needs.iter().all(|n| on_path(n)),
            })
        })
        .collect();
    serde_json::Value::Array(rows).to_string()
}

/// Splits a command line into program and arguments: spaces separate, single or double quotes
/// group. No shell is involved, so nothing is expanded or piped.
pub fn split_command(line: &str) -> Result<(String, Vec<String>), String> {
    let mut parts: Vec<String> = Vec::new();
    let (mut current, mut quote, mut started) = (String::new(), None::<char>, false);
    for c in line.chars() {
        match (c, quote) {
            ('\'' | '"', None) => {
                quote = Some(c);
                started = true;
            }
            (q, Some(open)) if q == open => quote = None,
            (c, None) if c.is_whitespace() => {
                if started {
                    parts.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (c, _) => {
                current.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err("the command has an unclosed quote".to_string());
    }
    if started {
        parts.push(current);
    }
    let mut parts = parts.into_iter();
    let program = parts
        .next()
        .ok_or("type the command that starts your agent")?;
    Ok((program, parts.collect()))
}

impl Chat {
    pub fn new() -> Result<Chat, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Chat {
            runtime,
            session: None,
            events: None,
            messages: Vec::new(),
            status: Status::Off,
            permission: None,
            first_prompt: true,
        })
    }

    pub fn is_running(&self) -> bool {
        self.status != Status::Off
    }

    fn push(&mut self, message: Value) {
        self.messages.push(message);
        if self.messages.len() > MAX_MESSAGES {
            self.messages.remove(0);
        }
    }

    /// Starts the agent in the background; `Started` arrives through `hook`.
    pub fn start(&mut self, command: AgentCommand, mcp: McpEndpoint, hook: ChatHook) {
        self.stop();
        self.messages.clear();
        self.permission = None;
        self.first_prompt = true;
        self.status = Status::Starting;
        let (tx, mut rx) = mpsc::unbounded_channel::<ChatEvent>();
        self.events = Some(tx.clone());
        let forward = Arc::clone(&hook);
        self.runtime.spawn(async move {
            while let Some(event) = rx.recv().await {
                forward(ChatInput::Event(event));
            }
        });
        self.runtime.spawn(async move {
            let started = AcpSession::start(&command, &mcp, tx)
                .await
                .map_err(|e| e.to_string());
            hook(ChatInput::Started(started));
        });
    }

    pub fn send(&mut self, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        let (session, events) = match (&self.session, &self.events) {
            (Some(s), Some(e)) if self.status == Status::Ready => (Arc::clone(s), e.clone()),
            _ if self.status == Status::Busy => {
                return Err("the agent is still working; cancel it or wait".into());
            }
            _ => return Err("the chat is not connected".into()),
        };
        self.push(json!({"role": "user", "text": text}));
        let prompt = if std::mem::take(&mut self.first_prompt) {
            format!("{PREAMBLE}{text}")
        } else {
            text.to_string()
        };
        self.status = Status::Busy;
        let _enter = self.runtime.enter();
        session.prompt(&prompt, events);
        Ok(())
    }

    /// Answers the pending permission request with `option` (`None` cancels it).
    pub fn answer(&mut self, request: u64, option: Option<&str>) -> Result<(), String> {
        let session = self.session.as_ref().ok_or("the chat is not connected")?;
        session.answer_permission(request, option);
        self.permission = None;
        Ok(())
    }

    pub fn cancel(&self) {
        if let Some(s) = &self.session {
            s.cancel();
        }
    }

    pub fn stop(&mut self) {
        if let Some(session) = self.session.take() {
            self.runtime.spawn(async move { session.shutdown().await });
        }
        self.events = None;
        self.permission = None;
        self.status = Status::Off;
    }

    /// Folds something the connection reported into the conversation.
    pub fn apply(&mut self, input: ChatInput) {
        match input {
            ChatInput::Started(Ok(session)) => {
                self.session = Some(Arc::new(session));
                self.status = Status::Ready;
                self.push(json!({"role": "note", "text": "Connected. Ask for a change."}));
            }
            ChatInput::Started(Err(message)) => {
                self.status = Status::Off;
                self.events = None;
                self.push(json!({"role": "error", "text": message}));
            }
            ChatInput::Event(event) => self.event(event),
        }
    }

    fn event(&mut self, event: ChatEvent) {
        match event {
            ChatEvent::Text(t) => self.append("agent", &t),
            ChatEvent::Thought(t) => self.append("thought", &t),
            ChatEvent::ToolCall { id, title, status } => {
                self.push(json!({"role": "tool", "id": id, "title": title, "status": status}));
            }
            ChatEvent::ToolUpdate { id, title, status } => {
                if let Some(m) = self
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m["role"] == "tool" && m["id"] == id.as_str())
                {
                    if let Some(t) = title {
                        m["title"] = json!(t);
                    }
                    if let Some(s) = status {
                        m["status"] = json!(s);
                    }
                }
            }
            ChatEvent::Permission {
                request,
                title,
                options,
            } => {
                let options: Vec<Value> = options
                    .iter()
                    .map(|o| json!({"id": o.id, "name": o.name, "kind": o.kind}))
                    .collect();
                self.permission = Some(json!({
                    "request": request.to_string(), "title": title, "options": options,
                }));
            }
            ChatEvent::TurnEnded(reason) => {
                if self.status == Status::Busy {
                    self.status = Status::Ready;
                }
                self.permission = None;
                if reason != "end_turn" {
                    self.push(json!({"role": "note", "text": format!("Stopped: {reason}")}));
                }
            }
            ChatEvent::Failed(message) => {
                if self.status == Status::Busy {
                    self.status = Status::Ready;
                }
                self.permission = None;
                self.push(json!({"role": "error", "text": message}));
            }
            ChatEvent::Exited(why) => {
                self.session = None;
                self.events = None;
                self.status = Status::Off;
                self.permission = None;
                self.push(json!({"role": "error", "text": format!("The agent exited. {why}")}));
            }
        }
    }

    /// Streamed text extends the last message of the same kind instead of starting a new one.
    fn append(&mut self, role: &str, text: &str) {
        if let Some(last) = self.messages.last_mut() {
            if last["role"] == role {
                let joined = format!("{}{text}", last["text"].as_str().unwrap_or(""));
                last["text"] = json!(joined);
                return;
            }
        }
        self.push(json!({"role": role, "text": text}));
    }

    /// What the panel shows before any chat was started.
    pub fn idle_view() -> Value {
        json!({"status": "off", "messages": [], "permission": null})
    }

    pub fn view(&self) -> Value {
        json!({
            "status": self.status.name(),
            "messages": self.messages,
            "permission": self.permission,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_agent::acp::PermissionOption;

    #[test]
    fn commands_split_on_spaces_and_respect_quotes_without_a_shell() {
        assert_eq!(
            split_command("npx -y @agentclientprotocol/claude-agent-acp").unwrap(),
            (
                "npx".to_string(),
                vec![
                    "-y".to_string(),
                    "@agentclientprotocol/claude-agent-acp".to_string()
                ]
            )
        );
        assert_eq!(
            split_command(r#"  "/Applications/My Agent/agent" --flag 'a b' "#).unwrap(),
            (
                "/Applications/My Agent/agent".to_string(),
                vec!["--flag".to_string(), "a b".to_string()]
            )
        );
        assert_eq!(split_command("agent ''").unwrap().1, vec![String::new()]);
        assert!(split_command("").is_err());
        assert!(split_command("   ").is_err());
        assert!(split_command("agent 'open").is_err());
        // no expansion: a pipe or a variable is just an argument
        assert_eq!(
            split_command("agent | rm $HOME").unwrap().1,
            ["|", "rm", "$HOME"]
        );
    }

    fn chat() -> Chat {
        Chat::new().unwrap()
    }

    #[test]
    fn streamed_text_joins_tool_calls_update_in_place_and_turns_return_to_ready() {
        let mut c = chat();
        c.status = Status::Busy;
        c.apply(ChatInput::Event(ChatEvent::Thought("hm".into())));
        c.apply(ChatInput::Event(ChatEvent::Text("Hel".into())));
        c.apply(ChatInput::Event(ChatEvent::Text("lo".into())));
        c.apply(ChatInput::Event(ChatEvent::ToolCall {
            id: "t1".into(),
            title: "get_guidance".into(),
            status: "pending".into(),
        }));
        c.apply(ChatInput::Event(ChatEvent::ToolUpdate {
            id: "t1".into(),
            title: None,
            status: Some("completed".into()),
        }));
        c.apply(ChatInput::Event(ChatEvent::Text("Done.".into())));
        c.apply(ChatInput::Event(ChatEvent::TurnEnded("end_turn".into())));
        let v = c.view();
        assert_eq!(v["status"], "ready");
        let roles: Vec<&str> = v["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, ["thought", "agent", "tool", "agent"]);
        assert_eq!(v["messages"][1]["text"], "Hello");
        assert_eq!(v["messages"][2]["status"], "completed");
    }

    #[test]
    fn a_permission_request_waits_for_the_user_and_failures_are_shown() {
        let mut c = chat();
        c.status = Status::Busy;
        c.apply(ChatInput::Event(ChatEvent::Permission {
            request: 7,
            title: "Run a command".into(),
            options: vec![PermissionOption {
                id: "yes".into(),
                name: "Allow".into(),
                kind: "allow_once".into(),
            }],
        }));
        let v = c.view();
        assert_eq!(v["permission"]["request"], "7");
        assert_eq!(v["permission"]["options"][0]["id"], "yes");
        assert!(
            c.send("more").is_err(),
            "no new message while the agent works"
        );

        c.apply(ChatInput::Event(ChatEvent::Failed("rate limited".into())));
        let v = c.view();
        assert_eq!(v["status"], "ready");
        assert!(v["permission"].is_null());
        assert_eq!(v["messages"][0]["role"], "error");

        c.apply(ChatInput::Event(ChatEvent::Exited("exit status: 1".into())));
        assert_eq!(c.view()["status"], "off");
        assert!(
            c.send("hello?").is_err(),
            "not connected after the agent exits"
        );
    }

    #[test]
    fn a_failed_start_leaves_the_chat_off_with_the_reason() {
        let mut c = chat();
        c.status = Status::Starting;
        c.apply(ChatInput::Started(Err("cannot start \"x\"".into())));
        let v = c.view();
        assert_eq!(v["status"], "off");
        assert!(
            v["messages"][0]["text"]
                .as_str()
                .unwrap()
                .contains("cannot start")
        );
    }
}

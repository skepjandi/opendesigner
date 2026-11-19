//! A scripted Agent Client Protocol agent for the tests of `od_agent::acp`. It prints a banner
//! (not JSON, as some real agents do), answers `initialize` and `session/new`, and for a prompt
//! streams text, a thought and a tool call, asks permission, asks the client for a file (which the
//! client must refuse) and ends the turn. the first argument picks a misbehavior (`version`, `nohttp`, `auth`,
//! `notice`) and the second names a file that receives the `session/new` parameters.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, Write};

use serde_json::{Value, json};

fn send(v: Value) {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{v}").unwrap();
    out.flush().unwrap();
}

fn update(session: &str, body: Value) {
    send(json!({"jsonrpc": "2.0", "method": "session/update",
                "params": {"sessionId": session, "update": body}}));
}

fn text(session: &str, kind: &str, t: &str) {
    update(
        session,
        json!({"sessionUpdate": kind, "content": {"type": "text", "text": t}}),
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).cloned().unwrap_or_default();
    let log_path = args.get(2).cloned();
    println!("mock agent starting (this line is not JSON-RPC)");
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines().map(|l| l.unwrap());
    let session = "sess-1";

    while let Some(line) = lines.next() {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = msg["id"].clone();
        match msg["method"].as_str() {
            Some("initialize") => {
                let version = if mode == "version" { 2 } else { 1 };
                let http = mode != "nohttp";
                send(json!({"jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": version,
                    "agentCapabilities": {"mcpCapabilities": {"http": http}}}}));
            }
            Some("session/new") => {
                if let Some(path) = &log_path {
                    std::fs::write(path, msg["params"].to_string()).unwrap();
                }
                if mode == "auth" {
                    send(json!({"jsonrpc": "2.0", "id": id,
                                "error": {"code": -32000, "message": "Authentication required"}}));
                    eprintln!("please run `mock-agent login`");
                } else if mode == "settings" {
                    send(json!({"jsonrpc": "2.0", "id": id, "result": {
                        "sessionId": session,
                        "configOptions": [
                            {"id": "model", "name": "Model", "category": "model", "type": "select",
                             "currentValue": "fast", "options": [
                                {"value": "fast", "name": "Fast"},
                                {"group": "g", "name": "Big", "options": [{"value": "big", "name": "Big one"}]}]},
                            {"id": "thinking", "name": "Thinking", "category": "thought_level",
                             "type": "select", "currentValue": "low", "options": [
                                {"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]},
                            {"id": "volume", "name": "Volume", "type": "number", "currentValue": "3"}]}}));
                    update(
                        session,
                        json!({"sessionUpdate": "available_commands_update",
                        "availableCommands": [
                            {"name": "compact", "description": "Shrink the context"},
                            {"name": "switch", "description": "Switch model", "input": {"hint": "[model]"}}]}),
                    );
                } else if mode == "legacy" {
                    send(json!({"jsonrpc": "2.0", "id": id, "result": {
                        "sessionId": session,
                        "modes": {"currentModeId": "ask", "availableModes": [
                            {"id": "ask", "name": "Ask"}, {"id": "code", "name": "Code"}]},
                        "models": {"currentModelId": "m1", "availableModels": [
                            {"modelId": "m1", "name": "One"}, {"modelId": "m2", "name": "Two"}]}}}));
                } else {
                    send(json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": session}}));
                }
                if mode == "notice" {
                    // an agent that could not start one of the MCP servers says so and goes on
                    update(
                        session,
                        json!({"sessionUpdate": "notice", "severity": "warning",
                               "title": "MCP server unavailable",
                               "description": "Continuing without it."}),
                    );
                }
            }
            Some("session/set_config_option") => {
                let (cfg, value) = (
                    msg["params"]["configId"].clone(),
                    msg["params"]["value"].clone(),
                );
                if value == "forbidden" {
                    send(json!({"jsonrpc": "2.0", "id": id,
                                "error": {"code": -32602, "message": "no such value"}}));
                } else {
                    send(
                        json!({"jsonrpc": "2.0", "id": id, "result": {"configOptions": [
                        {"id": "model", "name": "Model", "category": "model", "type": "select",
                         "currentValue": if cfg == "model" { value.clone() } else { json!("fast") },
                         "options": [{"value": "fast", "name": "Fast"}, {"value": "big", "name": "Big one"}]},
                        {"id": "thinking", "name": "Thinking", "category": "thought_level",
                         "type": "select",
                         "currentValue": if cfg == "thinking" { value } else { json!("low") },
                         "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]}]}}),
                    );
                }
            }
            Some("session/set_mode") | Some("session/set_model") => {
                send(json!({"jsonrpc": "2.0", "id": id, "result": {}}));
            }
            Some("session/prompt") => {
                let prompt = msg["params"]["prompt"][0]["text"].as_str().unwrap_or("");
                text(session, "agent_thought_chunk", "thinking");
                update(
                    session,
                    json!({"sessionUpdate": "plan", "entries": [
                        {"content": "Read the guidance", "priority": "high", "status": "in_progress"},
                        {"content": "Make it bigger", "priority": "medium", "status": "pending"}]}),
                );
                text(session, "agent_message_chunk", "Hello");
                text(session, "agent_message_chunk", " world");
                update(
                    session,
                    json!({"sessionUpdate": "tool_call", "toolCallId": "t1",
                           "title": "get_guidance", "status": "pending"}),
                );
                update(
                    session,
                    json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1",
                           "status": "completed"}),
                );
                update(
                    session,
                    json!({"sessionUpdate": "plan", "entries": [
                        {"content": "Read the guidance", "priority": "high", "status": "completed"},
                        {"content": "Make it bigger", "priority": "medium", "status": "in_progress"}]}),
                );
                if prompt.contains("crash") {
                    std::process::exit(3);
                }
                // a request the client does not serve
                send(
                    json!({"jsonrpc": "2.0", "id": 901, "method": "fs/read_text_file",
                            "params": {"sessionId": session, "path": "/etc/hosts"}}),
                );
                send(
                    json!({"jsonrpc": "2.0", "id": 900, "method": "session/request_permission",
                            "params": {"sessionId": session,
                                       "toolCall": {"toolCallId": "t2", "title": "Run a command"},
                                       "options": [
                                           {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                                           {"optionId": "no", "name": "Reject", "kind": "reject_once"}]}}),
                );
                let mut stop = "end_turn";
                for line in lines.by_ref() {
                    let Ok(reply) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    if reply["id"] == 901 {
                        let refused = reply["error"]["code"] == -32601;
                        text(
                            session,
                            "agent_message_chunk",
                            if refused {
                                " [fs refused]"
                            } else {
                                " [fs served]"
                            },
                        );
                    } else if reply["id"] == 900 {
                        let outcome = &reply["result"]["outcome"];
                        let said = if outcome["outcome"] == "cancelled" {
                            " [permission cancelled]".to_string()
                        } else {
                            format!(" [chose {}]", outcome["optionId"].as_str().unwrap_or("?"))
                        };
                        text(session, "agent_message_chunk", &said);
                        if prompt.contains("wait") {
                            continue; // keep waiting for a cancel
                        }
                        break;
                    } else if reply["method"] == "session/cancel" {
                        stop = "cancelled";
                        break;
                    }
                }
                send(json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": stop}}));
            }
            _ => {}
        }
    }
}

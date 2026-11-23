//! The one place a `tmux` program is started. Everything runs on a dedicated tmux server
//! (`-L opendesigner`), so the user's own tmux sessions and configuration are never touched, and
//! `tmux -L opendesigner attach` shows exactly what the app started.

use std::process::{Command, Stdio};

use crate::{CodeError, fail};

pub const SOCKET: &str = "opendesigner";

#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs tmux with the arguments given. A trait so the runner is tested without a tmux server.
pub trait Tmux: Send + Sync {
    fn run(&self, args: &[String]) -> Result<Reply, CodeError>;
}

pub struct SystemTmux {
    socket: String,
}

impl SystemTmux {
    pub fn new() -> SystemTmux {
        SystemTmux::with_socket(SOCKET)
    }

    pub fn with_socket(socket: &str) -> SystemTmux {
        SystemTmux {
            socket: socket.to_string(),
        }
    }

    pub fn socket(&self) -> &str {
        &self.socket
    }
}

impl Default for SystemTmux {
    fn default() -> Self {
        SystemTmux::new()
    }
}

impl Tmux for SystemTmux {
    fn run(&self, args: &[String]) -> Result<Reply, CodeError> {
        let output = Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    CodeError(
                        "tmux is not installed. Install it (macOS: brew install tmux; Linux: your package manager) and try again. On Windows, tmux runs inside WSL, which is not supported yet.".into(),
                    )
                } else {
                    CodeError(format!("cannot start tmux: {e}"))
                }
            })?;
        Ok(Reply {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// Fails with tmux's own message when the command did not succeed.
pub(crate) fn expect_success(reply: Reply, what: &str) -> Result<Reply, CodeError> {
    if reply.success {
        Ok(reply)
    } else {
        fail(format!("{what}: {}", reply.stderr.trim()))
    }
}

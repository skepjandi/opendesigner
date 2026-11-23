//! Starts, watches and stops the services of an approved run plan, each in its own tmux session
//! named `od-<project>-<service>`. A session stays on screen after its process exits, so a crash
//! and its error message remain readable in the app and with `tmux attach`.
//!
//! The runner never decides whether to run: callers pass a plan they got from
//! `approval::require`.

use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::plan::{RunPlan, Service, ServiceKind, valid_name};
use crate::tmux::{Tmux, expect_success};
use crate::{CodeError, fail};

pub const MAX_LOG_LINES: usize = 500;
const MAX_LOG_BYTES: usize = 64 * 1024;
const PORT_PROBE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum State {
    Stopped,
    Running,
    Exited { code: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceStatus {
    pub name: String,
    pub kind: ServiceKind,
    pub command: String,
    pub port: Option<u16>,
    #[serde(flatten)]
    pub state: State,
    /// A web service that is running and accepting connections on its port.
    pub ready: bool,
    pub session: String,
}

pub struct Runner<T: Tmux> {
    tmux: T,
    slug: String,
    socket: String,
}

fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// A short, readable, unique tag for a project: its folder name and four hex digits of its path.
fn slug_of(repo: &Path) -> String {
    let canonical = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    let digest = Sha256::digest(canonical.display().to_string().as_bytes());
    let tag: String = digest.iter().take(2).map(|b| format!("{b:02x}")).collect();
    let name: String = repo
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(12)
        .collect();
    format!("{}-{tag}", name.trim_matches('-'))
}

pub fn port_open(port: u16) -> bool {
    [
        SocketAddr::from(([127, 0, 0, 1], port)),
        SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port)),
    ]
    .iter()
    .any(|addr| TcpStream::connect_timeout(addr, PORT_PROBE).is_ok())
}

impl<T: Tmux> Runner<T> {
    pub fn new(tmux: T, repo: &Path, socket: &str) -> Runner<T> {
        Runner {
            tmux,
            slug: slug_of(repo),
            socket: socket.to_string(),
        }
    }

    pub fn session(&self, service: &str) -> String {
        format!("od-{}-{service}", self.slug)
    }

    /// What the user types in a terminal to watch a service.
    pub fn attach_command(&self, service: &str) -> String {
        format!(
            "tmux -L {} attach -t {}",
            self.socket,
            self.session(service)
        )
    }

    fn run(&self, args: &[&str]) -> Result<crate::tmux::Reply, CodeError> {
        let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
        self.tmux.run(&args)
    }

    pub fn available(&self) -> Result<(), CodeError> {
        expect_success(self.run(&["-V"])?, "tmux").map(|_| ())
    }

    /// (session name, state) of every session this project owns.
    fn sessions(&self) -> Result<Vec<(String, State)>, CodeError> {
        let reply = self.run(&[
            "list-panes",
            "-a",
            "-F",
            "#{session_name} #{pane_dead} #{pane_dead_status}",
        ])?;
        if !reply.success {
            let none = reply.stderr.contains("no server running")
                || reply.stderr.contains("error connecting");
            return if none {
                Ok(Vec::new())
            } else {
                fail(format!("tmux: {}", reply.stderr.trim()))
            };
        }
        let prefix = format!("od-{}-", self.slug);
        Ok(reply
            .stdout
            .lines()
            .filter_map(|line| {
                let mut parts = line.split(' ');
                let name = parts.next()?;
                name.starts_with(&prefix).then_some(())?;
                let dead = parts.next()? == "1";
                let code = parts.next().and_then(|c| c.parse().ok()).unwrap_or(0);
                let state = if dead {
                    State::Exited { code }
                } else {
                    State::Running
                };
                Some((name.to_string(), state))
            })
            .collect())
    }

    pub fn status(&self, plan: &RunPlan) -> Result<Vec<ServiceStatus>, CodeError> {
        let sessions = self.sessions()?;
        Ok(plan
            .services
            .iter()
            .map(|service| {
                let session = self.session(&service.name);
                let state = sessions
                    .iter()
                    .find(|(name, _)| *name == session)
                    .map_or(State::Stopped, |(_, state)| state.clone());
                let ready = state == State::Running
                    && service.kind == ServiceKind::Web
                    && service.port.is_some_and(port_open);
                ServiceStatus {
                    name: service.name.clone(),
                    kind: service.kind,
                    command: service.command.clone(),
                    port: service.port,
                    state,
                    ready,
                    session,
                }
            })
            .collect())
    }

    /// Starts one service of `plan`. A service that is running is left alone and reported; one
    /// that exited is replaced.
    pub fn start(&self, plan: &RunPlan, repo: &Path, name: &str) -> Result<(), CodeError> {
        let Some(service) = plan.service(name) else {
            return fail(format!("the run plan has no service {name:?}"));
        };
        let dir = service.working_dir(repo)?;
        let session = self.session(&service.name);
        match self.sessions()?.iter().find(|(n, _)| *n == session) {
            Some((_, State::Running)) => return fail(format!("{name} is already running")),
            Some(_) => self.kill(&session)?,
            None => {}
        }
        let shell_command = format!(
            "exec \"${{SHELL:-/bin/sh}}\" -lc {}",
            sh_quote(&service.command)
        );
        let dir = dir.display().to_string();
        let args = [
            "start-server",
            ";",
            "set-option",
            "-g",
            "default-shell",
            "/bin/sh",
            ";",
            "set-option",
            "-g",
            "remain-on-exit",
            "on",
            ";",
            "new-session",
            "-d",
            "-s",
            &session,
            "-c",
            &dir,
            "-x",
            "200",
            "-y",
            "50",
            &shell_command,
        ];
        expect_success(self.run(&args)?, &format!("starting {name}")).map(|_| ())
    }

    pub fn start_all(&self, plan: &RunPlan, repo: &Path) -> Result<Vec<String>, CodeError> {
        let mut skipped = Vec::new();
        for service in &plan.services {
            match self.start(plan, repo, &service.name) {
                Ok(()) => {}
                Err(e) if e.0.ends_with("is already running") => skipped.push(service.name.clone()),
                Err(e) => return Err(e),
            }
        }
        Ok(skipped)
    }

    fn kill(&self, session: &str) -> Result<(), CodeError> {
        let target = format!("={session}");
        let reply = self.run(&["kill-session", "-t", &target])?;
        let gone = reply.stderr.contains("can't find session")
            || reply.stderr.contains("no server running");
        if reply.success || gone {
            Ok(())
        } else {
            fail(format!("stopping {session}: {}", reply.stderr.trim()))
        }
    }

    pub fn stop(&self, name: &str) -> Result<(), CodeError> {
        if !valid_name(name) {
            return fail(format!("{name:?} is not a service name"));
        }
        self.kill(&self.session(name))
    }

    pub fn stop_all(&self, plan: &RunPlan) -> Result<(), CodeError> {
        plan.services.iter().try_for_each(|s| self.stop(&s.name))
    }

    pub fn restart(&self, plan: &RunPlan, repo: &Path, name: &str) -> Result<(), CodeError> {
        if plan.service(name).is_none() {
            return fail(format!("the run plan has no service {name:?}"));
        }
        self.stop(name)?;
        self.start(plan, repo, name)
    }

    /// The last `lines` lines of the service's screen and scrollback, plain text, at most 64 KB.
    pub fn logs(&self, name: &str, lines: usize) -> Result<String, CodeError> {
        if !valid_name(name) {
            return fail(format!("{name:?} is not a service name"));
        }
        let lines = lines.clamp(1, MAX_LOG_LINES);
        let target = format!("={}:", self.session(name));
        let from = format!("-{lines}");
        let reply = self.run(&["capture-pane", "-p", "-J", "-t", &target, "-S", &from])?;
        let text = expect_success(reply, &format!("reading {name}"))?.stdout;
        let text = text.trim_end();
        Ok(tail_bytes(text, MAX_LOG_BYTES).to_string())
    }
}

/// The last `max` bytes of `text`, cut at a character boundary.
fn tail_bytes(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// Where the web view loads a running web service from.
pub fn web_url(service: &Service) -> Option<String> {
    match (service.kind, service.port) {
        (ServiceKind::Web, Some(port)) => Some(format!("http://localhost:{port}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;
    use crate::tmux::Reply;

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<Vec<String>>>,
        replies: Mutex<VecDeque<Reply>>,
    }

    impl Fake {
        fn with(replies: Vec<Reply>) -> Fake {
            Fake {
                calls: Mutex::default(),
                replies: Mutex::new(replies.into()),
            }
        }
    }

    impl Tmux for &Fake {
        fn run(&self, args: &[String]) -> Result<Reply, CodeError> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(self.replies.lock().unwrap().pop_front().unwrap_or(Reply {
                success: true,
                ..Reply::default()
            }))
        }
    }

    fn ok(stdout: &str) -> Reply {
        Reply {
            success: true,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    fn failed(stderr: &str) -> Reply {
        Reply {
            success: false,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }

    fn plan() -> RunPlan {
        RunPlan {
            services: vec![Service {
                name: "web".into(),
                kind: ServiceKind::Web,
                command: "npm run dev -- --name 'it\\'s'".into(),
                cwd: ".".into(),
                port: Some(3999),
            }],
        }
    }

    #[test]
    fn start_runs_the_command_in_a_dedicated_login_shell_session() {
        let repo = tempfile::tempdir().unwrap();
        let fake = Fake::with(vec![failed("no server running on /tmp/x"), ok("")]);
        let runner = Runner::new(&fake, repo.path(), "opendesigner");
        runner.start(&plan(), repo.path(), "web").unwrap();
        let calls = fake.calls.lock().unwrap();
        let start = &calls[1];
        assert!(start.contains(&runner.session("web")));
        let shell = start.last().unwrap();
        assert!(shell.starts_with("exec \"${SHELL:-/bin/sh}\" -lc '"));
        assert!(shell.contains("it\\'\\''s"));
        assert!(start.windows(2).any(|w| w == ["remain-on-exit", "on"]));
    }

    #[test]
    fn a_running_service_is_not_started_twice_and_a_dead_one_is_replaced() {
        let repo = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        let runner = Runner::new(&fake, repo.path(), "opendesigner");
        let session = runner.session("web");
        fake.replies
            .lock()
            .unwrap()
            .push_back(ok(&format!("{session} 0 0\n")));
        let error = runner.start(&plan(), repo.path(), "web").unwrap_err();
        assert!(error.0.contains("already running"));

        fake.calls.lock().unwrap().clear();
        fake.replies
            .lock()
            .unwrap()
            .push_back(ok(&format!("{session} 1 2\n")));
        runner.start(&plan(), repo.path(), "web").unwrap();
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls[1][0], "kill-session");
        assert_eq!(calls[2][0], "start-server");
    }

    #[test]
    fn status_reads_running_exited_and_other_projects_are_ignored() {
        let repo = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        let runner = Runner::new(&fake, repo.path(), "opendesigner");
        let session = runner.session("web");
        fake.replies.lock().unwrap().push_back(ok(&format!(
            "{session} 1 3\nod-someone-else-web 0 0\nmine 0 0\n"
        )));
        let status = runner.status(&plan()).unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].state, State::Exited { code: 3 });
        assert!(!status[0].ready);

        fake.replies
            .lock()
            .unwrap()
            .push_back(failed("no server running on /tmp/x"));
        assert_eq!(runner.status(&plan()).unwrap()[0].state, State::Stopped);
    }

    #[test]
    fn stopping_a_missing_session_is_fine_and_bad_names_are_refused() {
        let repo = tempfile::tempdir().unwrap();
        let fake = Fake::with(vec![failed("can't find session: od-x-web")]);
        let runner = Runner::new(&fake, repo.path(), "opendesigner");
        assert!(runner.stop("web").is_ok());
        assert!(runner.stop("web; rm -rf").is_err());
        assert!(runner.logs("../x", 10).is_err());
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn logs_are_bounded() {
        let repo = tempfile::tempdir().unwrap();
        let big = "é".repeat(MAX_LOG_BYTES);
        let fake = Fake::with(vec![ok(&big)]);
        let runner = Runner::new(&fake, repo.path(), "opendesigner");
        let text = runner.logs("web", 100_000).unwrap();
        assert!(text.len() <= MAX_LOG_BYTES);
        let calls = fake.calls.lock().unwrap();
        assert!(calls[0].contains(&format!("-{MAX_LOG_LINES}")));
    }

    #[test]
    fn two_projects_never_share_session_names() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let fake = Fake::default();
        let first = Runner::new(&fake, a.path(), "opendesigner").session("web");
        let second = Runner::new(&fake, b.path(), "opendesigner").session("web");
        assert_ne!(first, second);
        assert!(first.starts_with("od-") && first.ends_with("-web"));
    }

    #[test]
    fn only_web_services_have_a_url() {
        let web = plan().services.remove(0);
        assert_eq!(web_url(&web).as_deref(), Some("http://localhost:3999"));
        let api = Service {
            kind: ServiceKind::Process,
            ..web
        };
        assert_eq!(web_url(&api), None);
    }
}

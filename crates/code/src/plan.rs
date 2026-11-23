//! The run plan: the processes a code project needs while it is built (a dev server, an API, a
//! database). An agent proposes it; nothing runs until the user approves exactly this text
//! (`approval`). Validation keeps a plan inside the project folder and inside sane limits; it
//! cannot make a command safe, which is why the user reads and approves every command.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CodeError, fail, io, plain_text};

pub const MAX_SERVICES: usize = 8;
pub const MAX_COMMAND_CHARS: usize = 500;
pub const MAX_CWD_CHARS: usize = 200;
const MAX_NAME_CHARS: usize = 24;
const MIN_PORT: u16 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    /// Serves a page on `port`; the app can show it in a web view.
    Web,
    /// Any other long-running process: an API, a database, a watcher.
    Process,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    /// `a-z`, `0-9`, `-`; names the tmux session.
    pub name: String,
    pub kind: ServiceKind,
    /// The shell command line, exactly as the user will read it.
    pub command: String,
    /// A folder inside the project, relative to its root; `.` is the root.
    #[serde(default = "root_dir")]
    pub cwd: String,
    /// Required for `web`: the port the command listens on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

fn root_dir() -> String {
    ".".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlan {
    pub services: Vec<Service>,
}

pub(crate) fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.chars().count() <= MAX_NAME_CHARS
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A relative path made of plain names only (or `.`): no `..`, no root, no drive.
fn relative_inside(cwd: &str) -> bool {
    !cwd.is_empty()
        && Path::new(cwd)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

impl Service {
    pub fn validate(&self) -> Result<(), CodeError> {
        if !valid_name(&self.name) {
            return fail(format!(
                "service name {:?}: use up to {MAX_NAME_CHARS} of a-z, 0-9 and - (for example web or api)",
                self.name
            ));
        }
        let command = self.command.trim();
        if command.is_empty() || command.starts_with('-') {
            return fail(format!("service {}: type the command to run", self.name));
        }
        plain_text("a command", &self.command, MAX_COMMAND_CHARS, false)?;
        plain_text("a folder", &self.cwd, MAX_CWD_CHARS, false)?;
        if !relative_inside(&self.cwd) {
            return fail(format!(
                "service {}: the folder {:?} must be inside the project (a relative path without ..)",
                self.name, self.cwd
            ));
        }
        match (self.kind, self.port) {
            (ServiceKind::Web, None) => fail(format!(
                "service {}: a web service needs its port",
                self.name
            )),
            (_, Some(port)) if port < MIN_PORT => fail(format!(
                "service {}: port {port} is below {MIN_PORT}",
                self.name
            )),
            _ => Ok(()),
        }
    }

    /// The service's folder on disk, with symlinks resolved and checked to stay inside `repo`.
    pub fn working_dir(&self, repo: &Path) -> Result<PathBuf, CodeError> {
        let root = repo
            .canonicalize()
            .map_err(|e| io(&repo.display().to_string(), e))?;
        let dir = root
            .join(&self.cwd)
            .canonicalize()
            .map_err(|e| io(&format!("service {}: folder {}", self.name, self.cwd), e))?;
        if !dir.starts_with(&root) || !dir.is_dir() {
            return fail(format!(
                "service {}: {:?} is not a folder inside the project",
                self.name, self.cwd
            ));
        }
        Ok(dir)
    }
}

impl RunPlan {
    pub fn validate(&self) -> Result<(), CodeError> {
        if self.services.is_empty() || self.services.len() > MAX_SERVICES {
            return fail(format!("a run plan has 1 to {MAX_SERVICES} services"));
        }
        for (i, service) in self.services.iter().enumerate() {
            service.validate()?;
            for other in &self.services[..i] {
                if other.name == service.name {
                    return fail(format!("two services are called {}", service.name));
                }
                if service.port.is_some() && other.port == service.port {
                    return fail(format!(
                        "services {} and {} both use port {}",
                        other.name,
                        service.name,
                        service.port.unwrap_or_default()
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn service(&self, name: &str) -> Option<&Service> {
        self.services.iter().find(|s| s.name == name)
    }

    /// What an approval is bound to: any change to any service changes this value.
    pub fn fingerprint(&self) -> String {
        let canonical = serde_json::to_vec(self).unwrap_or_default();
        let digest = Sha256::digest(canonical);
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web() -> Service {
        Service {
            name: "web".into(),
            kind: ServiceKind::Web,
            command: "npm run dev".into(),
            cwd: ".".into(),
            port: Some(3000),
        }
    }

    fn plan(services: Vec<Service>) -> RunPlan {
        RunPlan { services }
    }

    #[test]
    fn a_plain_web_service_is_valid() {
        assert!(plan(vec![web()]).validate().is_ok());
    }

    #[test]
    fn folders_cannot_leave_the_project() {
        for cwd in ["..", "../x", "/etc", "a/../..", ""] {
            let service = Service {
                cwd: cwd.into(),
                ..web()
            };
            assert!(service.validate().is_err(), "{cwd:?} must be refused");
        }
        let nested = Service {
            cwd: "apps/web".into(),
            ..web()
        };
        assert!(nested.validate().is_ok());
    }

    #[test]
    fn commands_are_one_printable_line() {
        for command in ["", "  ", "-rf", "a\nb", "a\u{0}b", &"x".repeat(501)] {
            let service = Service {
                command: command.into(),
                ..web()
            };
            assert!(service.validate().is_err(), "{command:?} must be refused");
        }
    }

    #[test]
    fn names_ports_and_counts_are_checked() {
        let bad_name = Service {
            name: "Web!".into(),
            ..web()
        };
        assert!(bad_name.validate().is_err());
        let no_port = Service {
            port: None,
            ..web()
        };
        assert!(no_port.validate().is_err());
        let low_port = Service {
            port: Some(80),
            ..web()
        };
        assert!(low_port.validate().is_err());
        assert!(plan(vec![web(), web()]).validate().is_err());
        let same_port = Service {
            name: "api".into(),
            kind: ServiceKind::Process,
            ..web()
        };
        assert!(plan(vec![web(), same_port]).validate().is_err());
        assert!(plan(Vec::new()).validate().is_err());
    }

    #[test]
    fn the_fingerprint_follows_every_field() {
        let base = plan(vec![web()]);
        let same = plan(vec![web()]);
        assert_eq!(base.fingerprint(), same.fingerprint());
        let other_command = plan(vec![Service {
            command: "npm start".into(),
            ..web()
        }]);
        let other_dir = plan(vec![Service {
            cwd: "app".into(),
            ..web()
        }]);
        let other_port = plan(vec![Service {
            port: Some(3001),
            ..web()
        }]);
        for changed in [other_command, other_dir, other_port] {
            assert_ne!(base.fingerprint(), changed.fingerprint());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_project_is_refused() {
        let outside = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), repo.path().join("link")).unwrap();
        let service = Service {
            cwd: "link".into(),
            ..web()
        };
        assert!(service.working_dir(repo.path()).is_err());
        let inside = Service {
            cwd: ".".into(),
            ..web()
        };
        assert!(inside.working_dir(repo.path()).is_ok());
    }
}

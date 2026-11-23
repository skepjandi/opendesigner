//! What a repository remembers about being a code project: its mode, the stack and the run plan.
//! Stored in `<repo>/.opendesigner/code.json`, next to the agent's notes, so it can be tracked in
//! git. It holds no approval: an approval lives only in the user's own config folder
//! (`approval`), so a cloned repository can never arrive pre-approved.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::plan::RunPlan;
use crate::stack::Stack;
use crate::{CodeError, fail, io};

pub const FOLDER: &str = ".opendesigner";
pub const FILE: &str = "code.json";
const VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Designs, store assets and video. The agent starts no processes.
    #[default]
    Assets,
    /// A codebase the agent builds and runs; designs and assets remain available.
    Code,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeProject {
    pub version: u32,
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<Stack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<RunPlan>,
}

impl Default for CodeProject {
    fn default() -> Self {
        CodeProject {
            version: VERSION,
            mode: Mode::Assets,
            stack: None,
            plan: None,
        }
    }
}

fn path_of(repo: &Path) -> PathBuf {
    repo.join(FOLDER).join(FILE)
}

impl CodeProject {
    /// The project at `repo`; a repository that never chose a mode is in assets mode.
    pub fn load(repo: &Path) -> Result<CodeProject, CodeError> {
        let path = path_of(repo);
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(CodeProject::default()),
            Err(e) => return Err(io(&path.display().to_string(), e)),
        };
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
            return fail(format!(
                "{} is not a plain file of a sane size",
                path.display()
            ));
        }
        let text = fs::read_to_string(&path).map_err(|e| io(&path.display().to_string(), e))?;
        let project: CodeProject = serde_json::from_str(&text)
            .map_err(|e| CodeError(format!("{}: {e}", path.display())))?;
        project.validate()?;
        Ok(project)
    }

    pub fn validate(&self) -> Result<(), CodeError> {
        if self.version != VERSION {
            return fail(format!(
                "code.json version {} is not supported",
                self.version
            ));
        }
        if let Some(stack) = &self.stack {
            stack.validate()?;
        }
        if let Some(plan) = &self.plan {
            plan.validate()?;
        }
        Ok(())
    }

    /// Writes the file atomically; refuses when `.opendesigner` or the file is a symlink.
    pub fn save(&self, repo: &Path) -> Result<(), CodeError> {
        self.validate()?;
        let folder = repo.join(FOLDER);
        if fs::symlink_metadata(&folder).is_ok_and(|m| m.file_type().is_symlink()) {
            return fail(format!(
                "{} is a link; not writing through it",
                folder.display()
            ));
        }
        fs::create_dir_all(&folder).map_err(|e| io(&folder.display().to_string(), e))?;
        let path = path_of(repo);
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return fail(format!(
                "{} is a link; not writing through it",
                path.display()
            ));
        }
        let mut text = serde_json::to_string_pretty(self).map_err(|e| CodeError(e.to_string()))?;
        text.push('\n');
        let tmp = path.with_extension("tmp");
        let mut file = fs::File::create(&tmp).map_err(|e| io(&tmp.display().to_string(), e))?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| io(&tmp.display().to_string(), e))?;
        fs::rename(&tmp, &path).map_err(|e| io(&path.display().to_string(), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Service, ServiceKind};
    use crate::stack::{DecidedBy, Target};

    fn sample() -> CodeProject {
        CodeProject {
            version: VERSION,
            mode: Mode::Code,
            stack: Some(Stack {
                name: "Vite React".into(),
                target: Target::Web,
                language: "TypeScript".into(),
                framework: "React".into(),
                notes: String::new(),
                decided_by: DecidedBy::User,
            }),
            plan: Some(RunPlan {
                services: vec![Service {
                    name: "web".into(),
                    kind: ServiceKind::Web,
                    command: "npm run dev".into(),
                    cwd: ".".into(),
                    port: Some(5173),
                }],
            }),
        }
    }

    #[test]
    fn a_missing_file_means_assets_mode() {
        let repo = tempfile::tempdir().unwrap();
        let loaded = CodeProject::load(repo.path()).unwrap();
        assert_eq!(loaded.mode, Mode::Assets);
        assert!(loaded.plan.is_none());
    }

    #[test]
    fn save_then_load_round_trips_and_is_stable() {
        let repo = tempfile::tempdir().unwrap();
        sample().save(repo.path()).unwrap();
        let first = fs::read(path_of(repo.path())).unwrap();
        let loaded = CodeProject::load(repo.path()).unwrap();
        assert_eq!(loaded, sample());
        loaded.save(repo.path()).unwrap();
        assert_eq!(first, fs::read(path_of(repo.path())).unwrap());
    }

    #[test]
    fn a_file_with_a_bad_plan_is_refused_on_load() {
        let repo = tempfile::tempdir().unwrap();
        let mut project = sample();
        project.plan.as_mut().unwrap().services[0].cwd = "../outside".into();
        fs::create_dir_all(repo.path().join(FOLDER)).unwrap();
        fs::write(
            path_of(repo.path()),
            serde_json::to_string(&project).unwrap(),
        )
        .unwrap();
        assert!(CodeProject::load(repo.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn saving_does_not_write_through_a_linked_folder() {
        let outside = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), repo.path().join(FOLDER)).unwrap();
        assert!(sample().save(repo.path()).is_err());
        assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
    }
}

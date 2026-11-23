//! The user's approval of a run plan. It is bound to one repository path and to the plan's
//! fingerprint, so changing any command, folder or port withdraws it. It is stored in the
//! per-user config folder and only the app writes it: no MCP tool, project file or agent can
//! approve a plan (`docs/wiki/security.md#running-processes`).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::plan::RunPlan;
use crate::{CodeError, fail, io};

const FILE: &str = "code-approvals.json";
const MAX_FILE_BYTES: u64 = 1024 * 1024;

type Approvals = BTreeMap<String, String>;

fn key_of(repo: &Path) -> Result<String, CodeError> {
    let canonical = repo
        .canonicalize()
        .map_err(|e| io(&repo.display().to_string(), e))?;
    Ok(canonical.display().to_string())
}

fn read(config: &Path) -> Approvals {
    let path = config.join(FILE);
    match fs::metadata(&path) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_FILE_BYTES => fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default(),
        _ => Approvals::new(),
    }
}

fn write(config: &Path, approvals: &Approvals) -> Result<(), CodeError> {
    fs::create_dir_all(config).map_err(|e| io(&config.display().to_string(), e))?;
    let path = config.join(FILE);
    let text = serde_json::to_string_pretty(approvals).map_err(|e| CodeError(e.to_string()))?;
    let tmp = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp).map_err(|e| io(&tmp.display().to_string(), e))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| io(&tmp.display().to_string(), e))?;
    fs::rename(&tmp, &path).map_err(|e| io(&path.display().to_string(), e))
}

/// Records that the user approved `plan` for `repo`. The plan is validated first.
pub fn approve(config: &Path, repo: &Path, plan: &RunPlan) -> Result<(), CodeError> {
    plan.validate()?;
    let mut approvals = read(config);
    approvals.insert(key_of(repo)?, plan.fingerprint());
    write(config, &approvals)
}

pub fn revoke(config: &Path, repo: &Path) -> Result<(), CodeError> {
    let mut approvals = read(config);
    if approvals.remove(&key_of(repo)?).is_some() {
        write(config, &approvals)?;
    }
    Ok(())
}

pub fn is_approved(config: &Path, repo: &Path, plan: &RunPlan) -> bool {
    key_of(repo)
        .ok()
        .is_some_and(|key| read(config).get(&key) == Some(&plan.fingerprint()))
}

/// The plan if the user approved exactly it, otherwise an error that says what to do.
pub fn require<'a>(
    config: &Path,
    repo: &Path,
    plan: Option<&'a RunPlan>,
) -> Result<&'a RunPlan, CodeError> {
    let Some(plan) = plan else {
        return fail("there is no run plan yet: call propose_run_plan first");
    };
    if is_approved(config, repo, plan) {
        Ok(plan)
    } else {
        fail("the user has not approved this run plan yet; they approve it in the Run panel")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Service, ServiceKind};

    fn plan(command: &str) -> RunPlan {
        RunPlan {
            services: vec![Service {
                name: "web".into(),
                kind: ServiceKind::Web,
                command: command.into(),
                cwd: ".".into(),
                port: Some(3000),
            }],
        }
    }

    #[test]
    fn approval_holds_for_the_same_plan_only() {
        let config = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let approved = plan("npm run dev");
        assert!(!is_approved(config.path(), repo.path(), &approved));
        approve(config.path(), repo.path(), &approved).unwrap();
        assert!(is_approved(config.path(), repo.path(), &approved));
        assert!(!is_approved(config.path(), repo.path(), &plan("rm -rf ~")));
    }

    #[test]
    fn approval_is_per_repository_and_can_be_revoked() {
        let config = tempfile::tempdir().unwrap();
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let shared = plan("npm run dev");
        approve(config.path(), a.path(), &shared).unwrap();
        assert!(!is_approved(config.path(), b.path(), &shared));
        revoke(config.path(), a.path()).unwrap();
        assert!(!is_approved(config.path(), a.path(), &shared));
    }

    #[test]
    fn require_explains_what_is_missing() {
        let config = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let missing = require(config.path(), repo.path(), None).unwrap_err();
        assert!(missing.0.contains("propose_run_plan"));
        let unapproved = plan("npm run dev");
        let refused = require(config.path(), repo.path(), Some(&unapproved)).unwrap_err();
        assert!(refused.0.contains("Run panel"));
    }

    #[test]
    fn an_invalid_plan_cannot_be_approved() {
        let config = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        assert!(approve(config.path(), repo.path(), &RunPlan::default()).is_err());
    }
}

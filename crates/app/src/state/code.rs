//! Code mode in the window: the mode the user chose, the Run panel's view of the stack, the run
//! plan and its services, and the user-only actions (approve, stop, change mode). The decisions
//! live in `od_code` and `od_agent::code_tools`; this resolves the repository folder and turns
//! errors into messages (`docs/wiki/code-mode.md`).

use od_agent::code_tools::{self, Env};
use od_code::approval;
use od_code::project::{CodeProject, Mode};
use od_code::runner;
use od_code::simulator::System;
use od_code::tmux::SystemTmux;

use super::*;
use std::path::PathBuf;

impl EditorState {
    fn code_env(&self) -> Res<Env<SystemTmux, System>> {
        let repo = self.repo.as_deref().ok_or("no project is open")?;
        let project = self.project_dir().unwrap_or_else(|| repo.to_path_buf());
        Env::system(repo, &project).map_err(|e| e.0)
    }

    /// The code side of the open project as the Run panel shows it. `{"open": false}` when no
    /// project is open.
    pub fn code_overview(&self) -> Value {
        let Ok(env) = self.code_env() else {
            return json!({"open": false});
        };
        match code_tools::overview(&env) {
            Ok(mut overview) => {
                overview["open"] = json!(true);
                if let Ok(plan) = env.saved_plan() {
                    overview["urls"] = json!(
                        plan.services
                            .iter()
                            .filter_map(|s| {
                                runner::web_url(s).map(|url| json!({"name": s.name, "url": url}))
                            })
                            .collect::<Vec<_>>()
                    );
                    overview["attach"] = json!(plan.services.iter().map(|s| {
                        json!({"name": s.name, "command": env.runner.attach_command(&s.name)})
                    }).collect::<Vec<_>>());
                }
                overview
            }
            Err(e) => json!({"open": true, "error": e.0}),
        }
    }

    /// Whether the open folder is in Code mode.
    pub fn code_mode(&self) -> bool {
        self.repo
            .as_deref()
            .and_then(|repo| CodeProject::load(repo).ok())
            .is_some_and(|p| p.mode == Mode::Code)
    }

    pub fn set_code_mode(&mut self, on: bool) -> Res<()> {
        let repo = self.repo.as_deref().ok_or("no project is open")?;
        let mode = if on { Mode::Code } else { Mode::Assets };
        code_tools::set_mode(repo, mode).map_err(|e| e.0)
    }

    /// The user pressed Approve and start: records the approval of exactly the plan on disk and
    /// starts every service in it.
    pub fn approve_run_plan(&mut self) -> Res<()> {
        let env = self.code_env()?;
        let plan = env.saved_plan()?;
        approval::approve(&env.config, &env.repo, &plan).map_err(|e| e.0)?;
        env.runner.available().map_err(|e| e.0)?;
        env.runner.start_all(&plan, &env.repo).map_err(|e| e.0)?;
        Ok(())
    }

    /// Withdraws the approval and stops everything the plan started.
    pub fn revoke_run_plan(&mut self) -> Res<()> {
        let env = self.code_env()?;
        let plan = env.saved_plan()?;
        env.runner.stop_all(&plan).map_err(|e| e.0)?;
        approval::revoke(&env.config, &env.repo).map_err(|e| e.0)
    }

    /// `action`: `start`, `stop` or `restart`, for one service of the approved plan.
    pub fn run_service(&mut self, name: &str, action: &str) -> Res<()> {
        let env = self.code_env()?;
        let plan = env.approved_plan().map_err(|e| e.0)?;
        match action {
            "start" => env.runner.start(&plan, &env.repo, name),
            "stop" => env.runner.stop(name),
            "restart" => env.runner.restart(&plan, &env.repo, name),
            other => return Err(format!("unknown action {other:?}")),
        }
        .map_err(|e| e.0)
    }

    pub fn code_paths(&self) -> Option<CodePaths> {
        let repo = self.repo.clone()?;
        let project = self.project_dir().unwrap_or_else(|| repo.clone());
        Some(CodePaths { repo, project })
    }

    pub fn service_log(&self, name: &str) -> Res<String> {
        let env = self.code_env()?;
        env.runner.logs(name, 200).map_err(|e| e.0)
    }
}

/// The simulator work runs off the UI thread, so it takes owned paths and builds its own `Env`.
#[derive(Clone)]
pub struct CodePaths {
    repo: PathBuf,
    project: PathBuf,
}

impl CodePaths {
    fn env(&self) -> Res<Env<SystemTmux, System>> {
        Env::system(&self.repo, &self.project).map_err(|e| e.0)
    }

    /// `{"simulators": [...]}` or `{"error": "..."}`; never blocks longer than the programs'
    /// own timeouts.
    pub fn list_simulators(&self) -> Value {
        match self.env().and_then(|env| env.sims.list().map_err(|e| e.0)) {
            Ok(found) => json!({"simulators": found}),
            Err(e) => json!({"error": e}),
        }
    }

    /// The user's click on Boot: no run-plan approval, because the user is the one asking.
    pub fn boot_simulator(&self, id: &str) -> Res<()> {
        let env = self.env()?;
        if let Some(avd) = id.strip_prefix("android-avd:") {
            env.runner.available().map_err(|e| e.0)?;
            let command = env.sims.emulator_command(id).map_err(|e| e.0)?;
            env.runner
                .start_emulator(avd, &command, &env.repo)
                .map_err(|e| e.0)
        } else {
            env.sims.boot_ios(id).map_err(|e| e.0)
        }
    }

    /// The simulator's screen as (width, height, straight RGBA; a screenshot is opaque).
    pub fn screen(&self, id: &str) -> Res<(u32, u32, Vec<u8>)> {
        let png = self.env()?.sims.screenshot(id).map_err(|e| e.0)?;
        decode_png(&png)
    }

    /// Saves the screen into the design project's `media/` and returns its reference.
    pub fn save_screen(&self, id: &str) -> Res<String> {
        let png = self.env()?.sims.screenshot(id).map_err(|e| e.0)?;
        code_tools::save_capture(&self.project, id, &png).map_err(|e| e.0)
    }
}

const MAX_SCREEN_PIXELS: u64 = 16_000_000;

fn decode_png(png: &[u8]) -> Res<(u32, u32, Vec<u8>)> {
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let name: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let file = std::env::temp_dir().join(format!("od-screen-{name}.png"));
    std::fs::write(&file, png).map_err(|e| e.to_string())?;
    let decoded = od_render::decode_rgba(&file, MAX_SCREEN_PIXELS);
    let _ = std::fs::remove_file(&file);
    decoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened() -> (EditorState, tempfile::TempDir) {
        let repo = tempfile::tempdir().unwrap();
        let mut state = EditorState::default();
        state
            .open_workspace_in(repo.path(), None, true, None)
            .unwrap();
        (state, repo)
    }

    #[test]
    fn a_folder_starts_in_assets_mode_and_the_user_can_switch_both_ways() {
        let (mut state, _repo) = opened();
        assert!(!state.code_mode());
        state.set_code_mode(true).unwrap();
        assert!(state.code_mode());
        assert_eq!(state.code_overview()["mode"], "code");
        state.set_code_mode(false).unwrap();
        assert!(!state.code_mode());
    }

    #[test]
    fn nothing_is_open_before_a_folder_is() {
        let state = EditorState::default();
        assert_eq!(state.code_overview()["open"], false);
        assert!(!state.code_mode());
    }
}

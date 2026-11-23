//! Code mode for agents (`docs/wiki/code-mode.md`): read what the project is, record the stack
//! after asking the user, propose the processes to run, and start, stop and read them once the
//! user has approved the plan in the app. There is no tool that approves a plan, switches the
//! mode, or runs a command that is not in the approved plan.

use std::path::{Path, PathBuf};

use od_code::approval;
use od_code::detect;
use od_code::plan::{RunPlan, Service};
use od_code::project::{CodeProject, Mode};
use od_code::runner::{self, Runner};
use od_code::simulator::{Exec, Programs, Simulators, System};
use od_code::stack::{self, Stack};
use od_code::tmux::{self, SystemTmux, Tmux};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{CallCtx, Output, ToolError, parse, tool};
use crate::{memory, token};

pub const NAMES: [&str; 12] = [
    "get_code_project",
    "set_stack",
    "propose_run_plan",
    "start_services",
    "stop_services",
    "restart_service",
    "service_logs",
    "list_simulators",
    "boot_simulator",
    "install_app",
    "launch_app",
    "capture_simulator",
];

pub fn definitions() -> Vec<Value> {
    let service = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string", "description": "a-z, 0-9 and -, up to 24 characters, for example web or api"},
            "kind": {"type": "string", "enum": ["web", "process"], "description": "web serves a page on `port`; process is anything else that keeps running"},
            "command": {"type": "string", "description": "one shell command line, shown to the user word for word"},
            "cwd": {"type": "string", "description": "folder inside the project, relative; default ."},
            "port": {"type": "integer", "description": "required for web; 1024 to 65535"}
        },
        "required": ["name", "kind", "command"],
        "additionalProperties": false
    });
    vec![
        tool(
            "get_code_project",
            "The code side of this project: its mode (assets or code), the recorded stack, what the \
             folder already is (detected files, package scripts, frameworks), the questions to ask \
             before choosing a stack, the run plan, whether the user approved it, and the status of \
             each service. Call it first in code mode; read get_playbook `code` before you build.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "set_stack",
            "Record the technology the project is built with. Ask the user the questions from \
             get_code_project first and propose two or three options with their trade-offs; call this \
             once they chose (decided_by agent) or when they simply told you what to use (decided_by \
             user). Code mode only.",
            json!({
                "name": {"type": "string", "description": "how the user would say it, for example Next.js with Postgres"},
                "target": {"type": "string", "enum": ["web", "ios", "android", "desktop", "cli", "other"]},
                "language": {"type": "string"},
                "framework": {"type": "string"},
                "notes": {"type": "string", "description": "why this stack, what the user answered, what was ruled out"},
                "decided_by": {"type": "string", "enum": ["user", "agent"]}
            }),
            &["name", "target", "language", "decided_by"],
            false,
        ),
        tool(
            "propose_run_plan",
            "Write down the processes the project needs while you build it (a dev server, an API, a \
             database), with the exact command, folder and port of each. Nothing runs yet: the user \
             reads the plan in the Run panel and approves it there. Changing the plan withdraws the \
             approval. Code mode only.",
            json!({"services": {"type": "array", "items": service, "minItems": 1, "maxItems": 8}}),
            &["services"],
            false,
        ),
        tool(
            "start_services",
            "Start the services of the approved run plan, each in its own tmux session so the user \
             can watch it. Omit `name` to start all. A service that is already running is left \
             alone. Fails until the user approves the plan. Code mode only.",
            json!({"name": {"type": "string"}}),
            &[],
            false,
        ),
        tool(
            "stop_services",
            "Stop services of the approved run plan. Omit `name` to stop all.",
            json!({"name": {"type": "string"}}),
            &[],
            false,
        ),
        tool(
            "restart_service",
            "Stop and start one service of the approved run plan, for example after you changed its \
             configuration.",
            json!({"name": {"type": "string"}}),
            &["name"],
            false,
        ),
        tool(
            "service_logs",
            "The last lines on a service's tmux screen, plain text: startup output, errors and the \
             message a crashed process left behind. Read it after you start a service and whenever \
             the page does not load.",
            json!({
                "name": {"type": "string"},
                "lines": {"type": "integer", "description": "default 80, at most 500"}
            }),
            &["name"],
            true,
        ),
        tool(
            "list_simulators",
            "The iOS Simulators and Android emulators on this computer (a connected phone is never \
             listed or touched): id, platform, name, OS and whether it is booted. The id goes into \
             the other simulator tools. Code mode only.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "boot_simulator",
            "Boot an iOS simulator (and open the Simulator app) or start an Android virtual device \
             in a tmux session. Needs the user's approval of the run plan. Wait until list_simulators \
             says booted.",
            json!({"id": {"type": "string", "description": "from list_simulators"}}),
            &["id"],
            false,
        ),
        tool(
            "install_app",
            "Install an app the project built into a booted simulator: a .app folder for iOS, an \
             .apk for Android, as a path inside the project. Needs the user's approval of the run plan.",
            json!({
                "id": {"type": "string"},
                "path": {"type": "string", "description": "relative to the project folder"}
            }),
            &["id", "path"],
            false,
        ),
        tool(
            "launch_app",
            "Launch an installed app by its application id (for example com.example.app) in a \
             booted simulator. Needs the user's approval of the run plan.",
            json!({"id": {"type": "string"}, "app_id": {"type": "string"}}),
            &["id", "app_id"],
            false,
        ),
        tool(
            "capture_simulator",
            "Take a screenshot of a booted simulator and save it in the design project's media/ \
             folder. The answer names the file; use it as an image node or as the capture of a 3D \
             device (get_playbook 3d) for store screenshots.",
            json!({"id": {"type": "string"}}),
            &["id"],
            false,
        ),
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackArgs {
    name: String,
    target: stack::Target,
    language: String,
    #[serde(default)]
    framework: String,
    #[serde(default)]
    notes: String,
    decided_by: stack::DecidedBy,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanArgs {
    services: Vec<Service>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameArgs {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogArgs {
    name: String,
    #[serde(default)]
    lines: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SimArgs {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallArgs {
    id: String,
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchArgs {
    id: String,
    app_id: String,
}

fn err(e: od_code::CodeError) -> ToolError {
    ToolError(e.0)
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    let dir = ctx
        .ws
        .dir()
        .map(Path::to_path_buf)
        .ok_or_else(|| ToolError("the project has no folder".into()))?;
    let repo = memory::repo_of(&dir).unwrap_or_else(|| dir.clone());
    let env = Env::system(&repo, &dir)?;
    call_with(&env, name, args)
}

/// Everything a code tool touches, passed in so tests use a temporary folder and a fake tmux.
pub struct Env<T: Tmux, E: Exec> {
    pub repo: PathBuf,
    /// The open design project (`media/` lives here); inside the repository.
    pub project: PathBuf,
    pub config: PathBuf,
    pub runner: Runner<T>,
    pub sims: Simulators<E>,
}

impl<T: Tmux, E: Exec> Env<T, E> {
    /// The plan in `code.json`, approved or not.
    pub fn saved_plan(&self) -> Result<RunPlan, String> {
        CodeProject::load(&self.repo)
            .map_err(|e| e.0)?
            .plan
            .ok_or_else(|| "there is no run plan yet; the agent proposes one".to_string())
    }

    /// The saved plan, only if the user approved exactly it.
    pub fn approved_plan(&self) -> Result<RunPlan, od_code::CodeError> {
        let project = CodeProject::load(&self.repo)?;
        approval::require(&self.config, &self.repo, project.plan.as_ref()).cloned()
    }
}

impl Env<SystemTmux, System> {
    /// The real thing: the user's config folder and the dedicated tmux server.
    pub fn system(repo: &Path, project: &Path) -> Result<Env<SystemTmux, System>, ToolError> {
        let config = token::config_dir().map_err(|e| ToolError(e.to_string()))?;
        Ok(Env {
            repo: repo.to_path_buf(),
            project: project.to_path_buf(),
            config,
            runner: Runner::new(SystemTmux::new(), repo, tmux::SOCKET),
            sims: Simulators::new(System, Programs::locate()),
        })
    }
}

pub fn call_with<T: Tmux, E: Exec>(
    env: &Env<T, E>,
    name: &str,
    args: Value,
) -> Result<Output, ToolError> {
    match name {
        "get_code_project" => get_code_project(env),
        "set_stack" => set_stack(env, parse(args)?),
        "propose_run_plan" => propose_run_plan(env, parse(args)?),
        "start_services" => start_services(env, parse(args)?),
        "stop_services" => stop_services(env, parse(args)?),
        "restart_service" => restart_service(env, parse(args)?),
        "service_logs" => service_logs(env, parse(args)?),
        "list_simulators" => list_simulators(env),
        "boot_simulator" => boot_simulator(env, parse(args)?),
        "install_app" => install_app(env, parse(args)?),
        "launch_app" => launch_app(env, parse(args)?),
        "capture_simulator" => capture_simulator(env, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

fn load_code_mode<T: Tmux, E: Exec>(env: &Env<T, E>) -> Result<CodeProject, ToolError> {
    let project = CodeProject::load(&env.repo).map_err(err)?;
    if project.mode == Mode::Code {
        Ok(project)
    } else {
        Err(ToolError(
            "this project is in Assets mode, so nothing is built or run. The user switches to Code \
             mode when they choose the folder; designs and assets work in both modes."
                .into(),
        ))
    }
}

fn approved_plan<'a, T: Tmux, E: Exec>(
    env: &Env<T, E>,
    project: &'a CodeProject,
) -> Result<&'a RunPlan, ToolError> {
    approval::require(&env.config, &env.repo, project.plan.as_ref()).map_err(err)
}

fn get_code_project<T: Tmux, E: Exec>(env: &Env<T, E>) -> Result<Output, ToolError> {
    Ok(Output::json(&overview(env)?))
}

/// The code side of the project as JSON: what `get_code_project` answers and the Run panel shows.
pub fn overview<T: Tmux, E: Exec>(env: &Env<T, E>) -> Result<Value, ToolError> {
    let project = CodeProject::load(&env.repo).map_err(err)?;
    let mut out = json!({
        "mode": project.mode,
        "stack": project.stack,
        "detected": detect::detect(&env.repo),
        "plan": project.plan,
    });
    if project.mode != Mode::Code {
        out["note"] =
            json!("Assets mode: the user has not asked for code. Make designs and assets.");
        return Ok(out);
    }
    if project.stack.is_none() {
        out["stack_questions"] = json!(stack::QUESTIONS);
        out["next"] = json!(
            "No stack is recorded. If the user already named one, call set_stack with decided_by \
             user. Otherwise ask the stack_questions (a few at a time, skip what the folder already \
             answers), propose two or three options, and call set_stack when they choose."
        );
    }
    if let Some(plan) = &project.plan {
        let approved = approval::is_approved(&env.config, &env.repo, plan);
        out["approved"] = json!(approved);
        out["services"] = match env.runner.status(plan) {
            Ok(status) => json!(status),
            Err(e) => json!({"error": e.0}),
        };
        out["watch_with"] = json!(
            plan.services
                .iter()
                .map(|s| env.runner.attach_command(&s.name))
                .collect::<Vec<_>>()
        );
        if !approved {
            out["next"] = json!(
                "The plan is waiting for the user: tell them to approve it in the Run panel."
            );
        }
    } else if project.stack.is_some() {
        out["next"] = json!(
            "Scaffold the project if it is empty, then call propose_run_plan with the dev server."
        );
    }
    Ok(out)
}

/// What `get_guidance` adds in Code mode, so an agent that only calls `get_guidance` still learns
/// that this is a codebase to build and run.
pub fn guidance_hint(project_dir: Option<&Path>) -> Option<Value> {
    let dir = project_dir?;
    let repo = memory::repo_of(dir).unwrap_or_else(|| dir.to_path_buf());
    let project = CodeProject::load(&repo).ok()?;
    (project.mode == Mode::Code).then(|| {
        json!(
            "This project is in Code mode: a real codebase you build and run, and designs and assets \
             stay available. Call get_code_project, read get_playbook code, and ask the user about \
             the stack before you choose one."
        )
    })
}

fn list_simulators<T: Tmux, E: Exec>(env: &Env<T, E>) -> Result<Output, ToolError> {
    load_code_mode(env)?;
    let found = env.sims.list().map_err(err)?;
    let note = found
        .is_empty()
        .then_some("None found: iOS Simulators need Xcode, Android emulators need the Android SDK (ANDROID_HOME).");
    Ok(Output::json(&json!({"simulators": found, "note": note})))
}

fn boot_simulator<T: Tmux, E: Exec>(env: &Env<T, E>, a: SimArgs) -> Result<Output, ToolError> {
    let project = load_code_mode(env)?;
    approved_plan(env, &project)?;
    if a.id.starts_with("android-avd:") {
        env.runner.available().map_err(err)?;
        let command = env.sims.emulator_command(&a.id).map_err(err)?;
        let avd = a.id.trim_start_matches("android-avd:");
        env.runner
            .start_emulator(avd, &command, &env.repo)
            .map_err(err)?;
    } else {
        env.sims.boot_ios(&a.id).map_err(err)?;
    }
    Ok(Output::json(&json!({
        "booting": a.id,
        "next": "Poll list_simulators until it is booted (an emulator takes a minute or more).",
    })))
}

/// A path the agent named, relative to the repository, resolved and kept inside it.
fn inside_repo<T: Tmux, E: Exec>(env: &Env<T, E>, relative: &str) -> Result<PathBuf, ToolError> {
    let root = env
        .repo
        .canonicalize()
        .map_err(|e| ToolError(e.to_string()))?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|_| ToolError(format!("{relative:?} does not exist in the project")))?;
    if path.starts_with(&root) && !Path::new(relative).is_absolute() {
        Ok(path)
    } else {
        Err(ToolError(format!("{relative:?} is outside the project")))
    }
}

fn install_app<T: Tmux, E: Exec>(env: &Env<T, E>, a: InstallArgs) -> Result<Output, ToolError> {
    let project = load_code_mode(env)?;
    approved_plan(env, &project)?;
    let path = inside_repo(env, &a.path)?;
    env.sims.install(&a.id, &path).map_err(err)?;
    Ok(Output::json(
        &json!({"installed": a.path, "next": "launch_app with the application id."}),
    ))
}

fn launch_app<T: Tmux, E: Exec>(env: &Env<T, E>, a: LaunchArgs) -> Result<Output, ToolError> {
    let project = load_code_mode(env)?;
    approved_plan(env, &project)?;
    env.sims.launch(&a.id, &a.app_id).map_err(err)?;
    Ok(Output::json(
        &json!({"launched": a.app_id, "next": "capture_simulator to look at it."}),
    ))
}

fn capture_simulator<T: Tmux, E: Exec>(env: &Env<T, E>, a: SimArgs) -> Result<Output, ToolError> {
    load_code_mode(env)?;
    let png = env.sims.screenshot(&a.id).map_err(err)?;
    let reference = save_capture(&env.project, &a.id, &png)?;
    Ok(Output::json(
        &json!({"image": reference, "bytes": png.len()}),
    ))
}

/// Writes the screenshot to a private temporary folder and imports it like any picture the user
/// adds, so the same checks and the same unused-name rule apply.
pub fn save_capture(project: &Path, id: &str, png: &[u8]) -> Result<String, ToolError> {
    let tag: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(40)
        .collect();
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).map_err(|e| ToolError(e.to_string()))?;
    let folder = std::env::temp_dir().join(format!(
        "od-capture-{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ));
    std::fs::create_dir(&folder).map_err(|e| ToolError(e.to_string()))?;
    let file = folder.join(format!("simulator-{tag}.png"));
    let result = std::fs::write(&file, png)
        .map_err(|e| ToolError(e.to_string()))
        .and_then(|()| od_core::media::import(project, &file).map_err(ToolError::from));
    let _ = std::fs::remove_dir_all(&folder);
    result
}

/// The user's own choice of mode, from the start screen. Not a tool: an agent cannot call it.
pub fn set_mode(repo: &Path, mode: Mode) -> Result<(), ToolError> {
    let mut project = CodeProject::load(repo).map_err(err)?;
    project.mode = mode;
    project.save(repo).map_err(err)
}

fn set_stack<T: Tmux, E: Exec>(env: &Env<T, E>, a: StackArgs) -> Result<Output, ToolError> {
    let mut project = load_code_mode(env)?;
    let stack = Stack {
        name: a.name,
        target: a.target,
        language: a.language,
        framework: a.framework,
        notes: a.notes,
        decided_by: a.decided_by,
    };
    stack.validate().map_err(err)?;
    project.stack = Some(stack);
    project.save(&env.repo).map_err(err)?;
    Ok(Output::json(&json!({"stack": project.stack})))
}

fn propose_run_plan<T: Tmux, E: Exec>(env: &Env<T, E>, a: PlanArgs) -> Result<Output, ToolError> {
    let mut project = load_code_mode(env)?;
    let plan = RunPlan {
        services: a.services,
    };
    plan.validate().map_err(err)?;
    for service in &plan.services {
        service.working_dir(&env.repo).map_err(err)?;
    }
    let unchanged = project.plan.as_ref() == Some(&plan);
    project.plan = Some(plan.clone());
    project.save(&env.repo).map_err(err)?;
    let approved = approval::is_approved(&env.config, &env.repo, &plan);
    Ok(Output::json(&json!({
        "saved": true,
        "unchanged": unchanged,
        "approved": approved,
        "next": if approved {
            "This plan is already approved: call start_services."
        } else {
            "Tell the user to read the plan in the Run panel and press Approve and start."
        },
    })))
}

fn start_services<T: Tmux, E: Exec>(env: &Env<T, E>, a: NameArgs) -> Result<Output, ToolError> {
    let project = load_code_mode(env)?;
    let plan = approved_plan(env, &project)?;
    env.runner.available().map_err(err)?;
    let skipped = match &a.name {
        Some(name) => {
            env.runner.start(plan, &env.repo, name).map_err(err)?;
            Vec::new()
        }
        None => env.runner.start_all(plan, &env.repo).map_err(err)?,
    };
    Ok(Output::json(&json!({
        "started": true,
        "already_running": skipped,
        "next": "Call service_logs to see the output, and check the status in get_code_project.",
        "services": env.runner.status(plan).map_err(err)?,
    })))
}

fn stop_services<T: Tmux, E: Exec>(env: &Env<T, E>, a: NameArgs) -> Result<Output, ToolError> {
    let project = load_code_mode(env)?;
    let plan = approved_plan(env, &project)?;
    match &a.name {
        Some(name) => env.runner.stop(name).map_err(err)?,
        None => env.runner.stop_all(plan).map_err(err)?,
    }
    Ok(Output::json(
        &json!({"services": env.runner.status(plan).map_err(err)?}),
    ))
}

fn restart_service<T: Tmux, E: Exec>(env: &Env<T, E>, a: NameArgs) -> Result<Output, ToolError> {
    let project = load_code_mode(env)?;
    let plan = approved_plan(env, &project)?;
    let name = a
        .name
        .ok_or_else(|| ToolError("name the service to restart".into()))?;
    env.runner.restart(plan, &env.repo, &name).map_err(err)?;
    Ok(Output::json(
        &json!({"services": env.runner.status(plan).map_err(err)?}),
    ))
}

fn service_logs<T: Tmux, E: Exec>(env: &Env<T, E>, a: LogArgs) -> Result<Output, ToolError> {
    let project = load_code_mode(env)?;
    let plan = project
        .plan
        .as_ref()
        .ok_or_else(|| ToolError("there is no run plan yet".into()))?;
    if plan.service(&a.name).is_none() {
        return Err(ToolError(format!(
            "the run plan has no service {:?}",
            a.name
        )));
    }
    let lines = a.lines.unwrap_or(80).min(runner::MAX_LOG_LINES);
    let text = env.runner.logs(&a.name, lines).map_err(err)?;
    Ok(Output::json(&json!({"service": a.name, "lines": text})))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use od_code::tmux::Reply;

    use super::*;

    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<VecDeque<Reply>>>, Arc<Mutex<Vec<Vec<String>>>>);

    impl Tmux for Fake {
        fn run(&self, args: &[String]) -> Result<Reply, od_code::CodeError> {
            self.1.lock().unwrap().push(args.to_vec());
            Ok(self.0.lock().unwrap().pop_front().unwrap_or(Reply {
                success: true,
                ..Reply::default()
            }))
        }
    }

    struct NoPrograms;

    impl Exec for NoPrograms {
        fn run(
            &self,
            _program: &Path,
            _args: &[&str],
        ) -> Result<od_code::simulator::Output, od_code::CodeError> {
            Err(od_code::CodeError("no programs in this test".into()))
        }
    }

    struct Fixture {
        _repo: tempfile::TempDir,
        _config: tempfile::TempDir,
        env: Env<Fake, NoPrograms>,
        fake: Fake,
    }

    fn fixture(mode: Mode) -> Fixture {
        let repo = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        let env = Env {
            repo: repo.path().to_path_buf(),
            project: repo.path().to_path_buf(),
            config: config.path().to_path_buf(),
            runner: Runner::new(fake.clone(), repo.path(), tmux::SOCKET),
            sims: Simulators::new(NoPrograms, Programs::default()),
        };
        CodeProject {
            mode,
            ..CodeProject::default()
        }
        .save(&env.repo)
        .unwrap();
        Fixture {
            _repo: repo,
            _config: config,
            env,
            fake,
        }
    }

    fn run(f: &Fixture, name: &str, args: Value) -> Result<Value, ToolError> {
        let out = call_with(&f.env, name, args)?;
        let crate::tools::Content::Text(text) = &out.content[0] else {
            panic!("a code tool answers with text");
        };
        Ok(serde_json::from_str(text).unwrap())
    }

    fn plan_args(command: &str) -> Value {
        json!({"services": [{"name": "web", "kind": "web", "command": command, "port": 5173}]})
    }

    #[test]
    fn assets_mode_refuses_every_code_tool_but_still_reports() {
        let f = fixture(Mode::Assets);
        assert_eq!(
            run(&f, "get_code_project", json!({})).unwrap()["mode"],
            "assets"
        );
        for (tool, args) in [
            (
                "set_stack",
                json!({"name": "x", "target": "web", "language": "x", "decided_by": "user"}),
            ),
            ("propose_run_plan", plan_args("npm run dev")),
            ("start_services", json!({})),
        ] {
            let error = run(&f, tool, args).unwrap_err();
            assert!(error.0.contains("Assets mode"), "{tool}: {}", error.0);
        }
        assert!(f.fake.1.lock().unwrap().is_empty());
    }

    #[test]
    fn nothing_starts_until_the_user_approves_and_a_changed_plan_needs_approval_again() {
        let f = fixture(Mode::Code);
        let proposed = run(&f, "propose_run_plan", plan_args("npm run dev")).unwrap();
        assert_eq!(proposed["approved"], false);
        let refused = run(&f, "start_services", json!({})).unwrap_err();
        assert!(refused.0.contains("Run panel"));
        assert!(
            f.fake.1.lock().unwrap().is_empty(),
            "tmux must not be touched"
        );

        let plan = CodeProject::load(&f.env.repo).unwrap().plan.unwrap();
        approval::approve(&f.env.config, &f.env.repo, &plan).unwrap();
        f.fake.0.lock().unwrap().extend([
            Reply {
                success: true,
                ..Reply::default()
            },
            Reply {
                success: false,
                stderr: "no server running on x".into(),
                ..Reply::default()
            },
            Reply {
                success: true,
                ..Reply::default()
            },
        ]);
        let started = run(&f, "start_services", json!({})).unwrap();
        assert_eq!(started["started"], true);

        let changed = run(&f, "propose_run_plan", plan_args("rm -rf ~")).unwrap();
        assert_eq!(changed["approved"], false);
        assert!(run(&f, "start_services", json!({})).is_err());
    }

    #[test]
    fn a_plan_that_leaves_the_project_is_refused() {
        let f = fixture(Mode::Code);
        let args = json!({"services": [{"name": "web", "kind": "process", "command": "ls", "cwd": "../.."}]});
        assert!(run(&f, "propose_run_plan", args).is_err());
        assert!(CodeProject::load(&f.env.repo).unwrap().plan.is_none());
    }

    #[test]
    fn a_new_project_is_told_to_ask_before_it_picks_a_stack() {
        let f = fixture(Mode::Code);
        let before = run(&f, "get_code_project", json!({})).unwrap();
        assert!(
            before["stack_questions"]
                .as_array()
                .is_some_and(|q| q.len() > 3)
        );
        run(
            &f,
            "set_stack",
            json!({"name": "Vite React", "target": "web", "language": "TypeScript", "decided_by": "agent"}),
        )
        .unwrap();
        let after = run(&f, "get_code_project", json!({})).unwrap();
        assert_eq!(after["stack"]["name"], "Vite React");
        assert!(after.get("stack_questions").is_none());
    }
}

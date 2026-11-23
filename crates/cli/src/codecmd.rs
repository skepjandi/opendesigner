//! `opendesigner code`: the same Code mode operations as the Run panel, for a terminal or CI
//! (docs/wiki/code-mode.md). Approving a plan prints every command and writes nothing without
//! `--yes`, like the skill installer.

use std::path::{Path, PathBuf};

use clap::{Subcommand, ValueEnum};
use od_agent::code_tools::{self, Env};
use od_code::approval;
use od_code::project::Mode;
use od_code::simulator::System;
use od_code::tmux::SystemTmux;

use crate::CliError;

#[derive(Clone, Copy, ValueEnum)]
pub enum ModeArg {
    Assets,
    Code,
}

#[derive(Subcommand)]
pub enum CodeAction {
    /// The mode, stack, run plan, whether it is approved, and the status of each service (JSON).
    Status {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Switch a folder between Assets and Code mode.
    Mode {
        mode: ModeArg,
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Show the run plan an agent proposed; with --yes, approve exactly it and start every
    /// service in tmux.
    Approve {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long)]
        yes: bool,
    },
    /// Stop every service of the plan and withdraw the approval.
    Revoke {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// The last lines on a service's tmux screen.
    Logs {
        service: String,
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long, default_value_t = 80)]
        lines: usize,
    },
    /// The iOS Simulators and Android emulators on this computer (JSON).
    Simulators {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
}

fn env(dir: &Path) -> Result<Env<SystemTmux, System>, CliError> {
    let repo = dir
        .canonicalize()
        .map_err(|e| CliError::Invalid(format!("{}: {e}", dir.display())))?;
    Env::system(&repo, &repo).map_err(|e| CliError::Invalid(e.0))
}

fn invalid(e: od_code::CodeError) -> CliError {
    CliError::Invalid(e.0)
}

pub fn run(action: CodeAction) -> Result<(), CliError> {
    match action {
        CodeAction::Status { dir } => {
            let overview = code_tools::overview(&env(&dir)?).map_err(|e| CliError::Invalid(e.0))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&overview).unwrap_or_default()
            );
        }
        CodeAction::Mode { mode, dir } => {
            let env = env(&dir)?;
            let mode = match mode {
                ModeArg::Assets => Mode::Assets,
                ModeArg::Code => Mode::Code,
            };
            code_tools::set_mode(&env.repo, mode).map_err(|e| CliError::Invalid(e.0))?;
            println!(
                "{} is now in {} mode",
                env.repo.display(),
                match mode {
                    Mode::Assets => "Assets",
                    Mode::Code => "Code",
                }
            );
        }
        CodeAction::Approve { dir, yes } => {
            let env = env(&dir)?;
            let plan = env.saved_plan().map_err(CliError::Invalid)?;
            println!("Run plan for {}:", env.repo.display());
            for s in &plan.services {
                println!(
                    "  {} ({:?}{}): {}   [in {}]",
                    s.name,
                    s.kind,
                    s.port.map_or(String::new(), |p| format!(", port {p}")),
                    s.command,
                    s.cwd
                );
            }
            if !yes {
                println!(
                    "\nThese commands run on this computer with your rights. Nothing was started; run again with --yes to approve exactly this plan."
                );
                return Ok(());
            }
            let config = env.config.clone();
            approval::approve(&config, &env.repo, &plan).map_err(invalid)?;
            env.runner.available().map_err(invalid)?;
            let skipped = env.runner.start_all(&plan, &env.repo).map_err(invalid)?;
            for s in &plan.services {
                let note = if skipped.contains(&s.name) {
                    " (already running)"
                } else {
                    ""
                };
                println!(
                    "started {}{note}: {}",
                    s.name,
                    env.runner.attach_command(&s.name)
                );
            }
        }
        CodeAction::Revoke { dir } => {
            let env = env(&dir)?;
            let plan = env.saved_plan().map_err(CliError::Invalid)?;
            env.runner.stop_all(&plan).map_err(invalid)?;
            approval::revoke(&env.config, &env.repo).map_err(invalid)?;
            println!("stopped every service and withdrew the approval");
        }
        CodeAction::Logs {
            service,
            dir,
            lines,
        } => {
            let env = env(&dir)?;
            print!("{}", env.runner.logs(&service, lines).map_err(invalid)?);
            println!();
        }
        CodeAction::Simulators { dir } => {
            let found = env(&dir)?.sims.list().map_err(invalid)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&found).unwrap_or_default()
            );
        }
    }
    Ok(())
}

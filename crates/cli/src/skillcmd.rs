use std::path::PathBuf;

use clap::Subcommand;
use od_agent::skill::{self, Action, PlannedTarget, State};

use crate::CliError;

#[derive(Subcommand)]
pub enum SkillAction {
    /// Show where the skill would go and whether it is installed.
    Status {
        /// Use this folder instead of your home directory (for testing).
        #[arg(long)]
        home: Option<PathBuf>,
    },
    /// Install or update the skill. Shows what it will write; writes only with --yes.
    Install {
        #[arg(long)]
        home: Option<PathBuf>,
        /// A target id (see `skill status`); repeatable. Default: every detected agent.
        #[arg(long = "target")]
        targets: Vec<String>,
        /// Actually write the files.
        #[arg(long)]
        yes: bool,
        /// Replace a folder named opendesigner that this app did not create.
        #[arg(long)]
        overwrite_foreign: bool,
    },
    /// Remove exactly the files an install wrote. Shows them; removes only with --yes.
    Uninstall {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long = "target")]
        targets: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
}

fn describe(state: &State) -> String {
    match state {
        State::NotInstalled => "not installed".to_string(),
        State::Foreign => "a folder not created by OpenDesigner is there".to_string(),
        State::Installed { version, modified } if modified.is_empty() => {
            format!("installed, version {version}")
        }
        State::Installed { version, modified } => format!(
            "installed, version {version}; you edited: {}",
            modified.join(", ")
        ),
    }
}

fn chosen<'a>(
    plan: &'a [PlannedTarget],
    targets: &[String],
) -> Result<Vec<&'a PlannedTarget>, CliError> {
    if targets.is_empty() {
        return Ok(plan.iter().filter(|p| p.detected).collect());
    }
    targets
        .iter()
        .map(|t| {
            plan.iter().find(|p| p.target.id == t).ok_or_else(|| {
                CliError::Invalid(format!("unknown target {t:?}; see `skill status`"))
            })
        })
        .collect()
}

pub fn run(action: SkillAction) -> Result<(), CliError> {
    match action {
        SkillAction::Status { home } => {
            let home = home.map_or_else(skill::home_dir, Ok)?;
            for p in skill::plan(&home)? {
                println!(
                    "{}\t{}\t{}\n  read by: {}\n  folder: {}\n  state: {}",
                    p.target.id,
                    p.target.label,
                    if p.detected {
                        "agent found"
                    } else {
                        "agent not found"
                    },
                    p.target.readers.join(", "),
                    p.dir.display(),
                    describe(&p.state)
                );
            }
        }
        SkillAction::Install {
            home,
            targets,
            yes,
            overwrite_foreign,
        } => {
            let home = home.map_or_else(skill::home_dir, Ok)?;
            let plan = skill::plan(&home)?;
            let picked = chosen(&plan, &targets)?;
            if picked.is_empty() {
                println!(
                    "No supported agent found. Targets: see `opendesigner skill status`; pick one with --target."
                );
                return Ok(());
            }
            for p in &picked {
                println!(
                    "{} ({}): {}",
                    p.target.id,
                    p.target.readers.join(", "),
                    p.dir.display()
                );
                for f in skill::generate() {
                    println!("  writes {}", p.dir.join(f.path).display());
                }
                println!(
                    "  writes {}",
                    p.dir.join(".opendesigner-install.json").display()
                );
                if p.state == State::Foreign {
                    println!("  NOTE: a folder not created by OpenDesigner is already there");
                }
            }
            if !yes {
                println!("\nNothing written. Re-run with --yes to install.");
                return Ok(());
            }
            let ids: Vec<&str> = picked.iter().map(|p| p.target.id).collect();
            for r in skill::install(&home, &ids, overwrite_foreign)? {
                println!("{}: {}", r.target, r.dir.display());
                print_actions(&r.actions);
            }
        }
        SkillAction::Uninstall { home, targets, yes } => {
            let home = home.map_or_else(skill::home_dir, Ok)?;
            let plan = skill::plan(&home)?;
            let picked: Vec<&PlannedTarget> = if targets.is_empty() {
                plan.iter()
                    .filter(|p| matches!(p.state, State::Installed { .. }))
                    .collect()
            } else {
                chosen(&plan, &targets)?
            };
            if picked.is_empty() {
                println!("Nothing to remove.");
                return Ok(());
            }
            for p in &picked {
                println!(
                    "{}: {} ({})",
                    p.target.id,
                    p.dir.display(),
                    describe(&p.state)
                );
            }
            if !yes {
                println!(
                    "\nNothing removed. Re-run with --yes to remove the files OpenDesigner wrote (files you edited are kept)."
                );
                return Ok(());
            }
            let ids: Vec<&str> = picked.iter().map(|p| p.target.id).collect();
            for r in skill::uninstall(&home, &ids)? {
                println!("{}: {}", r.target, r.dir.display());
                print_actions(&r.actions);
            }
        }
    }
    Ok(())
}

fn print_actions(actions: &[Action]) {
    for a in actions {
        match a {
            Action::Wrote(p) => println!("  wrote {p}"),
            Action::Unchanged(p) => println!("  unchanged {p}"),
            Action::KeptEdited(p) => println!("  kept {p} (you edited it)"),
            Action::Removed(p) => println!("  removed {p}"),
        }
    }
}

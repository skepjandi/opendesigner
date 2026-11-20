//! `lint` and `export --batch`: the document linter and template batch export
//! (`docs/wiki/lint-rules.md`, `docs/wiki/agent-integration.md#batch-export`).

use std::path::{Path, PathBuf};

use clap::ValueEnum;
use od_agent::batch::{self, BatchRequest, Target};
use od_agent::doctools;
use od_core::lint::{self, Finding, Severity};
use od_core::{Operation, Workspace};

use crate::CliError;

/// The severity at which `lint` fails (exit code 2).
#[derive(Clone, Copy, ValueEnum)]
pub enum FailOn {
    Warn,
    Error,
}

impl FailOn {
    fn severity(self) -> Severity {
        match self {
            FailOn::Warn => Severity::Warn,
            FailOn::Error => Severity::Error,
        }
    }
}

fn describe(f: &Finding) -> String {
    let at: Vec<String> = [
        f.node.as_ref().map(|n| format!("node {n}")),
        f.frame.as_ref().map(|n| format!("frame {n}")),
        f.video.as_ref().map(|n| format!("video {n}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    format!(
        "{} {}\t{}\t{}\n    hint: {}",
        f.code,
        f.severity.as_str(),
        at.join(", "),
        f.message,
        f.hint
    )
}

pub fn lint(dir: &Path, json: bool, fail_on: FailOn, presets: &[String]) -> Result<(), CliError> {
    let ws = Workspace::open(dir)?;
    let targets = presets
        .iter()
        .map(|id| crate::preset(id).map(doctools::preset_target))
        .collect::<Result<Vec<_>, _>>()?;
    let findings = doctools::lint_workspace(&ws, targets);
    if json {
        println!("{}", doctools::report_json(&findings, Severity::Info));
    } else {
        for f in &findings {
            println!("{}", describe(f));
        }
        let (errors, warnings, infos) = lint::counts(&findings);
        println!("{errors} error(s), {warnings} warning(s), {infos} note(s)");
    }
    match lint::worst(&findings) {
        Some(worst) if worst >= fail_on.severity() => Err(CliError::Violations(format!(
            "lint found {} finding(s) at or above {}",
            findings
                .iter()
                .filter(|f| f.severity >= fail_on.severity())
                .count(),
            fail_on.severity().as_str()
        ))),
        _ => Ok(()),
    }
}

pub struct BatchArgs {
    pub dir: PathBuf,
    pub rows: PathBuf,
    pub frame: Option<String>,
    pub video: Option<String>,
    pub preset: Option<String>,
    pub out_template: Option<String>,
    pub strict_variables: bool,
    pub scale: f32,
}

pub fn export_batch(a: BatchArgs) -> Result<(), CliError> {
    let target = match (a.frame, a.video) {
        (Some(f), None) => Target::Frame(parse_id(&f)?),
        (None, Some(v)) => Target::Video(parse_id(&v)?),
        _ => {
            return Err(CliError::Invalid(
                "--batch needs exactly one of --frame (a PNG per row) or --video (an MP4 per row)"
                    .into(),
            ));
        }
    };
    let ws = Workspace::open(&a.dir)?;
    let rows = batch::read_rows_file(&a.rows).map_err(|e| CliError::Invalid(e.0))?;
    let request = BatchRequest {
        rows,
        target,
        preset: a.preset,
        out_template: a.out_template,
        strict: a.strict_variables,
        scale: Some(a.scale),
    };
    let report = batch::run(&ws, &request).map_err(|e| CliError::Invalid(e.0))?;
    println!("{}", report.to_json());
    if report.ok() {
        return Ok(());
    }
    let failed = report.rows.iter().filter(|r| !r.ok()).count();
    Err(CliError::Violations(format!(
        "{failed} of {} rows were not exported",
        report.rows.len()
    )))
}

fn parse_id(s: &str) -> Result<od_core::NodeId, CliError> {
    Ok(od_core::NodeId::parse(s)?)
}

/// `apply --dry-run`: prints what the operations would do and exits 2 if they would be rejected.
pub fn dry_run_apply(dir: &Path, ops: &[Operation]) -> Result<(), CliError> {
    let ws = Workspace::open(dir)?;
    let check = ws.check_ops(ops);
    println!("{}", doctools::dry_run_json(&check));
    if check.ok {
        return Ok(());
    }
    let reasons: Vec<String> = check
        .ops
        .iter()
        .filter_map(|r| r.error.as_ref())
        .map(|e| format!("{}: {}", e.code, e.message))
        .collect();
    Err(CliError::Violations(format!(
        "the operations would be rejected ({}); nothing was changed",
        reasons.join("; ")
    )))
}

/// `package-plugin`: regenerates the agent plugin under `root`, or with `check` reports what is
/// stale and exits 2.
pub fn package_plugin(root: &Path, check: bool) -> Result<(), CliError> {
    if check {
        let problems = od_agent::plugin::check(root)?;
        if problems.is_empty() {
            println!("the agent plugin is current");
            return Ok(());
        }
        return Err(CliError::Violations(format!(
            "the agent plugin is stale; run scripts/package-agent-plugin.sh:\n{}",
            problems.join("\n")
        )));
    }
    for path in od_agent::plugin::write(root)? {
        println!("wrote {}", path.display());
    }
    Ok(())
}

//! `opendesigner`: the headless front end. It runs the same operations as the editor against a
//! project folder, so agents and CI work without a window (`docs/wiki/agent-integration.md`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use od_core::{
    Actor, AnnotationTarget, Node, NodeId, OpError, Operation, ProjectError, Session, Workspace,
    project,
};
mod serve;

use od_render::RenderError;
use od_store::{ImageFacts, Preset, Verification};
use thiserror::Error;

const MAX_OPS_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Parser)]
#[command(name = "opendesigner", version, about = "Headless OpenDesigner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a project folder with one empty page.
    New {
        dir: PathBuf,
        #[arg(long, default_value = "Untitled")]
        name: String,
    },
    /// Print the page tree with node ids.
    Show { dir: PathBuf },
    /// Apply a JSON array of operations as one transaction and print the receipt as JSON.
    Apply {
        dir: PathBuf,
        ops: PathBuf,
        /// Attribute the edit to this agent instead of the user.
        #[arg(long)]
        agent: Option<String>,
        /// Group the edit under an agent turn id.
        #[arg(long)]
        turn: Option<String>,
    },
    /// Name the current state so it can be restored.
    Checkpoint {
        dir: PathBuf,
        name: String,
        #[arg(long)]
        agent: Option<String>,
    },
    /// List checkpoints.
    Checkpoints { dir: PathBuf },
    /// Return to a checkpoint by appending the inverse of everything since it.
    Restore {
        dir: PathBuf,
        checkpoint: u64,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Render a frame to a PNG, validating it against a store preset when one is given.
    Export {
        dir: PathBuf,
        #[arg(long)]
        frame: String,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        preset: Option<String>,
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Keep the alpha channel (stores reject it for screenshots).
        #[arg(long)]
        alpha: bool,
    },
    /// Validate PNG files as one set against a store preset.
    Validate {
        #[arg(long)]
        preset: String,
        files: Vec<PathBuf>,
    },
    /// List the built-in store presets.
    Presets,
    /// Pin a note on a node (or a page region) for an agent to read.
    Annotate {
        dir: PathBuf,
        note: String,
        /// The node the note is pinned to.
        #[arg(long, conflicts_with = "page")]
        node: Option<String>,
        /// A page, with --region x,y,w,h, instead of a node.
        #[arg(long, requires = "region")]
        page: Option<String>,
        #[arg(long, requires = "page")]
        region: Option<String>,
        #[arg(long)]
        agent: Option<String>,
    },
    /// List annotations.
    Annotations {
        dir: PathBuf,
        /// Include resolved ones.
        #[arg(long)]
        all: bool,
    },
    /// Mark an annotation resolved.
    Resolve {
        dir: PathBuf,
        id: u64,
        /// What was done, in the user's terms.
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Print the guidance an agent receives: pages, selection, open notes, recent user edits.
    Guidance {
        dir: PathBuf,
        /// Pretend these nodes are selected (a front end holds the real selection).
        #[arg(long, value_delimiter = ',')]
        select: Vec<String>,
    },
    /// Serve the project to coding agents over MCP on 127.0.0.1 (stop with Ctrl-C).
    ServeMcp {
        dir: PathBuf,
        /// First port to try; the next ones are tried when it is taken.
        #[arg(long, default_value_t = od_agent::DEFAULT_PORT)]
        port: u16,
        /// Print the token itself (by default only the file that holds it).
        #[arg(long)]
        show_token: bool,
        /// Replace the token first; agents must be reconnected.
        #[arg(long)]
        rotate_token: bool,
    },
}

#[derive(Debug, Error)]
enum CliError {
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Agent(#[from] od_agent::AgentError),
    #[error(transparent)]
    Op(#[from] OpError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Invalid(String),
    /// The export or set does not meet the store rules; exit code 2.
    #[error("{0}")]
    Violations(String),
}

fn io(path: &Path, source: std::io::Error) -> CliError {
    CliError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Violations(msg)) => {
            eprintln!("{msg}");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn actor(agent: Option<String>) -> Result<Actor, CliError> {
    match agent {
        Some(name) => Ok(Actor::agent(&name)?),
        None => Ok(Actor::User),
    }
}

fn read_capped(path: &Path, limit: u64) -> Result<Vec<u8>, CliError> {
    let size = fs::metadata(path).map_err(|e| io(path, e))?.len();
    if size > limit {
        return Err(CliError::Invalid(format!(
            "{} is {size} bytes, over the {limit} byte limit",
            path.display()
        )));
    }
    fs::read(path).map_err(|e| io(path, e))
}

fn preset(id: &str) -> Result<&'static Preset, CliError> {
    od_store::find(id).ok_or_else(|| {
        let ids: Vec<&str> = od_store::PRESETS.iter().map(|p| p.id).collect();
        CliError::Invalid(format!(
            "unknown preset {id:?}; available: {}",
            ids.join(", ")
        ))
    })
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::New { dir, name } => {
            project::create(&dir, &name)?;
            println!("created {}", dir.display());
        }
        Command::Show { dir } => {
            let session = project::load(&dir)?;
            for page in &session.document().pages {
                println!("page {} {:?}", page.id, page.name);
                for child in &page.root.children {
                    print_node(child, 1);
                }
            }
        }
        Command::Apply {
            dir,
            ops,
            agent,
            turn,
        } => {
            let mut session = project::load(&dir)?;
            let ops: Vec<Operation> =
                serde_json::from_slice(&read_capped(&ops, MAX_OPS_FILE_BYTES)?)
                    .map_err(|e| CliError::Invalid(format!("operations file: {e}")))?;
            let receipt = session.apply(actor(agent)?, turn, &ops)?;
            project::save(&dir, &session)?;
            let out = serde_json::json!({
                "transaction": receipt.id,
                "created": receipt.created,
                "touched": receipt.touched,
            });
            println!("{out}");
        }
        Command::Checkpoint { dir, name, agent } => {
            let mut session = project::load(&dir)?;
            let id = session.checkpoint(actor(agent)?, &name)?;
            project::save(&dir, &session)?;
            println!("checkpoint {id}");
        }
        Command::Checkpoints { dir } => {
            let session = project::load(&dir)?;
            for cp in session.checkpoints() {
                println!("{}\t{}\t{}\t@{}", cp.id, cp.actor, cp.name, cp.position);
            }
        }
        Command::Restore {
            dir,
            checkpoint,
            agent,
        } => {
            let mut session = project::load(&dir)?;
            let receipt = session.restore(actor(agent)?, checkpoint)?;
            project::save(&dir, &session)?;
            println!(
                "restored checkpoint {checkpoint} (transaction {})",
                receipt.id
            );
        }
        Command::Export {
            dir,
            frame,
            out,
            preset: preset_id,
            scale,
            alpha,
        } => {
            export(&dir, &frame, &out, preset_id.as_deref(), scale, alpha)?;
        }
        Command::Validate {
            preset: preset_id,
            files,
        } => validate(&preset_id, &files)?,
        Command::Presets => {
            for p in od_store::PRESETS {
                println!("{}\t{}", p.id, p.description);
            }
        }
        Command::Annotate {
            dir,
            note,
            node,
            page,
            region,
            agent,
        } => {
            let mut ws = Workspace::open(&dir)?;
            let target = annotation_target(node, page, region)?;
            let id = ws
                .annotations
                .add(ws.session.document(), target, &note, actor(agent)?)?;
            ws.save()?;
            println!("annotation {id}");
        }
        Command::Annotations { dir, all } => {
            let ws = Workspace::open(&dir)?;
            for a in ws.annotations.all() {
                if all || a.status == od_core::AnnotationStatus::Open {
                    println!("{}", serde_json::to_string(a).map_err(json_err)?);
                }
            }
        }
        Command::Resolve {
            dir,
            id,
            note,
            agent,
        } => {
            let mut ws = Workspace::open(&dir)?;
            ws.annotations.resolve(id, actor(agent)?, note.as_deref())?;
            ws.save()?;
            println!("resolved {id}");
        }
        Command::Guidance { dir, select } => {
            let mut ws = Workspace::open(&dir)?;
            let ids = select
                .iter()
                .map(|s| NodeId::parse(s))
                .collect::<Result<Vec<_>, _>>()?;
            ws.select(ids)?;
            let json = serde_json::to_string_pretty(&ws.guidance()).map_err(json_err)?;
            println!("{json}");
        }
        Command::ServeMcp {
            dir,
            port,
            show_token,
            rotate_token,
        } => serve::run(&dir, port, show_token, rotate_token)?,
    }
    Ok(())
}

fn json_err(e: serde_json::Error) -> CliError {
    CliError::Invalid(e.to_string())
}

fn annotation_target(
    node: Option<String>,
    page: Option<String>,
    region: Option<String>,
) -> Result<AnnotationTarget, CliError> {
    if let (Some(page), Some(region)) = (page, region) {
        let parts: Vec<f64> = region
            .split(',')
            .map(|p| p.trim().parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| CliError::Invalid("--region takes x,y,width,height numbers".into()))?;
        let [x, y, width, height] = parts[..] else {
            return Err(CliError::Invalid("--region takes x,y,width,height".into()));
        };
        return Ok(AnnotationTarget::Region {
            page: NodeId::parse(&page)?,
            x,
            y,
            width,
            height,
        });
    }
    match node {
        Some(id) => Ok(AnnotationTarget::Node {
            id: NodeId::parse(&id)?,
        }),
        None => Err(CliError::Invalid(
            "give --node, or --page with --region".into(),
        )),
    }
}

fn print_node(node: &Node, depth: usize) {
    println!(
        "{}{} {} {:?} at {},{} {}x{}",
        "  ".repeat(depth),
        node.id,
        node.kind.as_str(),
        node.name,
        node.x,
        node.y,
        node.width,
        node.height
    );
    for child in &node.children {
        print_node(child, depth + 1);
    }
}

fn note_verification(p: &Preset) {
    if p.verification == Verification::CopiedFromReference {
        eprintln!(
            "note: the {} preset was copied from a reference project; check it against {}",
            p.id, p.source
        );
    }
}

fn export(
    dir: &Path,
    frame: &str,
    out: &Path,
    preset_id: Option<&str>,
    scale: f32,
    alpha: bool,
) -> Result<(), CliError> {
    let target = preset_id.map(preset).transpose()?;
    let session: Session = project::load(dir)?;
    let frame_id = NodeId::parse(frame)?;
    let rendered = od_render::render_frame(session.document(), &frame_id, scale)?;
    for warning in &rendered.warnings {
        eprintln!("warning: {warning:?} was not drawn");
    }
    let keep_alpha = alpha && target.is_none_or(|p| p.allow_alpha);
    let bytes = od_render::encode_png(&rendered.pixmap, keep_alpha)?;
    if let Some(p) = target {
        let facts = od_store::inspect_png(&bytes).map_err(|e| CliError::Invalid(e.to_string()))?;
        let violations = p.validate_image(&facts);
        if !violations.is_empty() {
            let list: Vec<String> = violations.iter().map(|v| format!("  - {v}")).collect();
            return Err(CliError::Violations(format!(
                "not written: the export does not meet {} ({})\n{}",
                p.id,
                p.source,
                list.join("\n")
            )));
        }
        note_verification(p);
    }
    fs::write(out, &bytes).map_err(|e| io(out, e))?;
    println!(
        "wrote {} ({} bytes, {}x{})",
        out.display(),
        bytes.len(),
        rendered.pixmap.width(),
        rendered.pixmap.height()
    );
    Ok(())
}

fn validate(preset_id: &str, files: &[PathBuf]) -> Result<(), CliError> {
    let p = preset(preset_id)?;
    let mut facts: Vec<ImageFacts> = Vec::with_capacity(files.len());
    for file in files {
        let bytes = read_capped(file, MAX_IMAGE_BYTES)?;
        facts.push(
            od_store::inspect_png(&bytes)
                .map_err(|e| CliError::Invalid(format!("{}: {e}", file.display())))?,
        );
    }
    let violations = p.validate_set(&facts);
    if violations.is_empty() {
        println!("{} files meet {}", files.len(), p.id);
        note_verification(p);
        return Ok(());
    }
    let list: Vec<String> = violations
        .iter()
        .map(|(i, v)| match i {
            Some(i) => format!("  - {}: {v}", files[*i].display()),
            None => format!("  - set: {v}"),
        })
        .collect();
    Err(CliError::Violations(format!(
        "the set does not meet {} ({})\n{}",
        p.id,
        p.source,
        list.join("\n")
    )))
}

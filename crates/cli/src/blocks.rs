//! `opendesigner blocks`: the block catalog from the command line. A block is added to a project
//! the same way an agent adds it: one attributed, undoable change through the same operations.

use std::path::PathBuf;

use clap::Subcommand;
use od_agent::blocks::{self, Colors, Request, plan, search, write_assets};
use serde_json::{Map, Value, json};

use crate::CliError;

#[derive(Subcommand)]
pub enum BlocksAction {
    /// List the blocks; filter by a word or by category.
    List {
        /// Words describing what you need.
        #[arg(long)]
        query: Option<String>,
        /// One of titles, overlays, backgrounds, data, store, motion.
        #[arg(long)]
        category: Option<String>,
    },
    /// Show one block: what it makes and every parameter it takes.
    Show { block: String },
    /// Add a block to a frame in the project as one undoable change.
    Add {
        dir: PathBuf,
        /// A block id from `blocks list`.
        block: String,
        /// The frame to put it in (the only frame when the project has one).
        #[arg(long)]
        frame: Option<String>,
        /// The video to keyframe in (the only video when the project has one).
        #[arg(long)]
        video: Option<String>,
        /// When it starts, from the start of the video.
        #[arg(long)]
        start_ms: Option<u64>,
        /// Fade it out this long after start_ms.
        #[arg(long)]
        hold_ms: Option<u64>,
        /// Where it lays itself out, in frame coordinates: `x,y,width,height`.
        #[arg(long = "box", value_name = "X,Y,W,H")]
        region: Option<String>,
        /// A JSON file with the block's own parameters.
        #[arg(long)]
        params: Option<PathBuf>,
        /// Attribute the change to this agent instead of the user.
        #[arg(long)]
        agent: Option<String>,
    },
}

fn region_of(text: &str) -> Result<blocks::BoxArgs, CliError> {
    let numbers: Result<Vec<f64>, _> = text.split(',').map(|n| n.trim().parse::<f64>()).collect();
    match numbers.as_deref() {
        Ok([x, y, width, height]) => Ok(blocks::BoxArgs {
            x: Some(*x),
            y: Some(*y),
            width: Some(*width),
            height: Some(*height),
        }),
        _ => Err(CliError::Invalid(format!(
            "--box takes four numbers, x,y,width,height; got {text:?}"
        ))),
    }
}

fn params_of(path: Option<PathBuf>) -> Result<Option<Map<String, Value>>, CliError> {
    let Some(path) = path else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path).map_err(|e| CliError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| CliError::Invalid(format!("{}: {e}", path.display())))?;
    match value {
        Value::Object(map) => Ok(Some(map)),
        _ => Err(CliError::Invalid(format!(
            "{}: the parameters are a JSON object",
            path.display()
        ))),
    }
}

pub fn run(action: BlocksAction) -> Result<(), CliError> {
    match action {
        BlocksAction::List { query, category } => {
            let category = match category.as_deref() {
                Some(c) => Some(blocks::Category::parse(c).ok_or_else(|| {
                    CliError::Invalid(format!(
                        "unknown category {c:?}; use one of {}",
                        blocks::Category::ALL.map(|c| c.as_str()).join(", ")
                    ))
                })?),
                None => None,
            };
            let found = search(query.as_deref().unwrap_or(""), category);
            for b in found {
                println!("{}\t{}\t{}", b.id, b.category.as_str(), b.about);
            }
        }
        BlocksAction::Show { block } => {
            let b = blocks::find(&block).map_err(|e| CliError::Invalid(e.to_string()))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&b.describe()).unwrap_or_default()
            );
        }
        BlocksAction::Add {
            dir,
            block,
            frame,
            video,
            start_ms,
            hold_ms,
            region,
            params,
            agent,
        } => {
            let mut session = od_core::project::load(&dir)?;
            let request = Request {
                block,
                frame,
                video,
                start_ms,
                hold_ms,
                region: region.as_deref().map(region_of).transpose()?,
                colors: None::<Colors>,
                font: None,
                params: params_of(params)?,
            };
            let p =
                plan(session.document(), &request).map_err(|e| CliError::Invalid(e.to_string()))?;
            write_assets(&dir, &p).map_err(|e| CliError::Invalid(e.to_string()))?;
            let actor = crate::actor(agent)?;
            let receipt = session.apply(actor, None, &p.ops)?;
            od_core::project::save(&dir, &session)?;
            let mut out = p.report();
            out["transaction"] = json!(receipt.id);
            println!("{}", serde_json::to_string(&out).unwrap_or_default());
        }
    }
    Ok(())
}

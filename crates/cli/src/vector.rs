//! `opendesigner import-svg-vector`: an SVG illustration brought into a project as editable shape
//! nodes, the same way the agent tool does it: one attributed, undoable change.

use std::path::PathBuf;

use clap::Args;
use od_agent::svg_vector::MAX_SVG_BYTES;
use od_agent::vector_tools::{self, Request};
use od_core::project;

use crate::CliError;

#[derive(Args)]
pub struct ImportSvgVector {
    /// The project folder.
    dir: PathBuf,
    /// Id of the frame or group to put the drawing in.
    #[arg(long)]
    parent: String,
    /// An SVG file on this computer.
    #[arg(long, value_name = "PATH")]
    svg: Option<PathBuf>,
    /// An SVG in the project's media folder (media/<name>.svg).
    #[arg(long, value_name = "MEDIA_PATH")]
    file: Option<String>,
    /// An asset id from search_assets (lucide:<icon> or <pack>:<file>).
    #[arg(long)]
    asset: Option<String>,
    /// Name of the group (default: the SVG's own id, else "illustration").
    #[arg(long)]
    name: Option<String>,
    /// Where the drawing's top-left goes in the parent.
    #[arg(long)]
    x: Option<f64>,
    #[arg(long)]
    y: Option<f64>,
    /// Width of the whole drawing in document units; the height follows.
    #[arg(long)]
    width: Option<f64>,
    /// Position among the parent's children (default: on top).
    #[arg(long)]
    index: Option<usize>,
    /// Attribute the change to this agent instead of the user.
    #[arg(long)]
    agent: Option<String>,
}

pub fn run(args: ImportSvgVector) -> Result<(), CliError> {
    let svg = args
        .svg
        .as_deref()
        .map(|path| {
            let bytes = crate::read_capped(path, MAX_SVG_BYTES as u64)?;
            String::from_utf8(bytes)
                .map_err(|_| CliError::Invalid(format!("{} is not UTF-8 text", path.display())))
        })
        .transpose()?;
    let request = Request {
        parent: args.parent,
        svg,
        file: args.file,
        asset: args.asset,
        name: args.name,
        x: args.x,
        y: args.y,
        width: args.width,
        index: args.index,
    };
    let mut session = project::load(&args.dir)?;
    let invalid = |e: od_agent::tools::ToolError| CliError::Invalid(e.0);
    let source = vector_tools::read_source(Some(&args.dir), &request).map_err(invalid)?;
    let plan = vector_tools::plan(session.document(), &source, &request).map_err(invalid)?;
    let receipt = session
        .apply(crate::actor(args.agent)?, None, &plan.ops)
        .map_err(|e| CliError::Invalid(od_agent::doctools::failure_text(&e)))?;
    project::save(&args.dir, &session)?;
    let group = receipt
        .created
        .first()
        .and_then(|id| session.document().node(id))
        .ok_or_else(|| CliError::Invalid("the imported group is missing".to_string()))?;
    let mut out = vector_tools::report(&plan.imported, group, source.credit.as_ref());
    out["transaction"] = serde_json::json!(receipt.id);
    println!("{out}");
    Ok(())
}

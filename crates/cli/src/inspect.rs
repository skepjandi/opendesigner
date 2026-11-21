//! `opendesigner inspect`: the views an agent uses to look closely at its own video work (strip,
//! crop, grid, diff), the same code as the MCP tools `video_strip`, `video_crop`, `frame_grid`
//! and `video_diff`. Each writes one PNG into the project's `exports/` folder. Read-only
//! otherwise.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use od_agent::inspect;
use od_agent::video_export::Source;
use od_core::Workspace;
use serde_json::{Map, Value, json};

use crate::CliError;

#[derive(Args)]
pub struct Inspect {
    /// The project folder.
    dir: PathBuf,
    #[command(subcommand)]
    view: View,
}

#[derive(Subcommand)]
enum View {
    /// Every frame (or every Nth) of a short stretch, at most 36, labeled with their times.
    Strip {
        #[arg(long)]
        video: String,
        #[arg(long)]
        from_ms: u64,
        #[arg(long)]
        to_ms: u64,
        #[arg(long)]
        every: Option<u32>,
        #[arg(long)]
        cell_side: Option<u32>,
        /// File name in exports/.
        #[arg(long, default_value = "strip.png")]
        out: String,
    },
    /// A full-resolution piece of the picture: a region, or the box of a node.
    Crop {
        #[arg(long)]
        video: String,
        #[arg(long)]
        time_ms: u64,
        #[arg(long)]
        x: Option<u32>,
        #[arg(long)]
        y: Option<u32>,
        #[arg(long)]
        width: Option<u32>,
        #[arg(long)]
        height: Option<u32>,
        /// Follow this node's box through the camera instead of a region.
        #[arg(long)]
        node: Option<String>,
        #[arg(long)]
        padding: Option<u32>,
        /// Enlarge each pixel 1 to 4 times.
        #[arg(long)]
        zoom: Option<u32>,
        #[arg(long, default_value = "crop.png")]
        out: String,
    },
    /// The picture with a labeled coordinate grid.
    Grid {
        #[arg(long)]
        video: String,
        #[arg(long)]
        time_ms: u64,
        /// Video pixels between lines.
        #[arg(long)]
        spacing: Option<u32>,
        #[arg(long)]
        max_side: Option<u32>,
        #[arg(long, default_value = "grid.png")]
        out: String,
    },
    /// Two moments side by side with the changed pixels highlighted.
    Diff {
        #[arg(long)]
        video: String,
        #[arg(long)]
        time_ms: u64,
        #[arg(long)]
        other_time_ms: Option<u64>,
        #[arg(long)]
        other_video: Option<String>,
        /// Only the picture of the changes.
        #[arg(long)]
        overlay: bool,
        #[arg(long)]
        threshold: Option<u8>,
        #[arg(long)]
        max_side: Option<u32>,
        #[arg(long, default_value = "diff.png")]
        out: String,
    },
}

/// The tool arguments for the options given.
fn arguments(pairs: Vec<(&str, Option<Value>)>) -> Value {
    let map: Map<String, Value> = pairs
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k.to_string(), v)))
        .collect();
    Value::Object(map)
}

fn some<T: Into<Value>>(v: T) -> Option<Value> {
    Some(v.into())
}

/// The tool to run, its arguments and the file to write.
fn request(view: View) -> (&'static str, Value, String) {
    match view {
        View::Strip {
            video,
            from_ms,
            to_ms,
            every,
            cell_side,
            out,
        } => (
            "video_strip",
            arguments(vec![
                ("video", some(video)),
                ("from_ms", some(from_ms)),
                ("to_ms", some(to_ms)),
                ("every", every.map(Value::from)),
                ("cell_side", cell_side.map(Value::from)),
            ]),
            out,
        ),
        View::Crop {
            video,
            time_ms,
            x,
            y,
            width,
            height,
            node,
            padding,
            zoom,
            out,
        } => (
            "video_crop",
            arguments(vec![
                ("video", some(video)),
                ("time_ms", some(time_ms)),
                ("x", x.map(Value::from)),
                ("y", y.map(Value::from)),
                ("width", width.map(Value::from)),
                ("height", height.map(Value::from)),
                ("node", node.map(Value::from)),
                ("padding", padding.map(Value::from)),
                ("zoom", zoom.map(Value::from)),
            ]),
            out,
        ),
        View::Grid {
            video,
            time_ms,
            spacing,
            max_side,
            out,
        } => (
            "frame_grid",
            arguments(vec![
                ("video", some(video)),
                ("time_ms", some(time_ms)),
                ("spacing", spacing.map(Value::from)),
                ("max_side", max_side.map(Value::from)),
            ]),
            out,
        ),
        View::Diff {
            video,
            time_ms,
            other_time_ms,
            other_video,
            overlay,
            threshold,
            max_side,
            out,
        } => (
            "video_diff",
            arguments(vec![
                ("video", some(video)),
                ("time_ms", some(time_ms)),
                ("other_time_ms", other_time_ms.map(Value::from)),
                ("other_video", other_video.map(Value::from)),
                ("mode", overlay.then(|| json!("overlay"))),
                ("threshold", threshold.map(Value::from)),
                ("max_side", max_side.map(Value::from)),
            ]),
            out,
        ),
    }
}

pub fn run(args: Inspect) -> Result<(), CliError> {
    let ws = Workspace::open(&args.dir)?;
    let (tool, arguments, out) = request(args.view);
    let picture =
        inspect::run(&Source::of(&ws), tool, arguments).map_err(|e| CliError::Invalid(e.0))?;
    let path =
        inspect::write_export(&args.dir, &out, &picture.png).map_err(|e| CliError::Invalid(e.0))?;
    println!("{path}: {}", picture.note);
    Ok(())
}

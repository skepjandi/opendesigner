//! `opendesigner export-atlas` and `opendesigner quantize-image`: the pixel-art game-asset tools of
//! the MCP server (`od_agent::pixel_tools`), run headless against a project folder.

use std::path::{Path, PathBuf};

use clap::Args;
use od_agent::tools::{self, CallCtx, Content, Turns};
use od_core::{Actor, Workspace};
use serde_json::{Map, Value, json};

use crate::{CliError, actor, read_capped};

const MAX_META_BYTES: u64 = 1024 * 1024;

#[derive(Args)]
pub struct ExportAtlas {
    /// The project folder.
    dir: PathBuf,
    /// File name stem: writes exports/<name>.png and exports/<name>.json.
    #[arg(long)]
    name: String,
    /// Grid: the frame to cut into cells (with --cols, --rows, --cell-w and --cell-h).
    #[arg(long, requires_all = ["cols", "rows", "cell_w", "cell_h"], conflicts_with = "pack_frame")]
    frame: Option<String>,
    #[arg(long)]
    cols: Option<u32>,
    #[arg(long)]
    rows: Option<u32>,
    /// Width of a cell in output pixels.
    #[arg(long)]
    cell_w: Option<u32>,
    /// Height of a cell in output pixels.
    #[arg(long)]
    cell_h: Option<u32>,
    /// Pack: a frame to put in the sheet (repeat the flag, in order).
    #[arg(long = "pack-frame")]
    pack_frame: Vec<String>,
    /// Pack: cells per row (default 8).
    #[arg(long)]
    columns: Option<u32>,
    /// Pack: transparent pixels around every cell.
    #[arg(long)]
    padding: Option<u32>,
    /// A name per cell, row by row (repeat the flag).
    #[arg(long = "cell-name")]
    cell_names: Vec<String>,
    /// Pivot of every frame, in pixels from the cell's top-left corner.
    #[arg(long, requires = "pivot_y")]
    pivot_x: Option<f64>,
    #[arg(long, requires = "pivot_x")]
    pivot_y: Option<f64>,
    /// 9-slice margins of every frame in pixels: left top right bottom.
    #[arg(long, num_args = 4, value_names = ["L", "T", "R", "B"])]
    nine_slice: Option<Vec<u32>>,
    /// A JSON file `{ "<frame name>": { "pivot": {"x", "y"}, "nine_slice": {"l","t","r","b"} } }`.
    #[arg(long, value_name = "PATH")]
    frame_meta: Option<PathBuf>,
    #[arg(long, default_value_t = 1.0)]
    scale: f32,
    /// Hard pixel edges: no anti-aliasing, nearest-neighbour images (use an integer scale).
    #[arg(long)]
    pixel_art: bool,
}

#[derive(Args)]
pub struct QuantizeImage {
    /// The project folder.
    dir: PathBuf,
    /// The picture to snap: media/<name>.png.
    #[arg(long)]
    file: String,
    /// A colour of the palette: a token name or #rrggbb (repeat the flag).
    #[arg(
        long = "color",
        required_unless_present = "pack",
        conflicts_with = "pack"
    )]
    colors: Vec<String>,
    /// Use the palette of a style pack instead.
    #[arg(long)]
    pack: Option<String>,
    /// Alpha at or above which a pixel stays (default 128).
    #[arg(long)]
    alpha_threshold: Option<u8>,
}

/// Opens the project as the user, runs the tool and prints its answer.
fn run_tool(dir: &Path, name: &str, args: Map<String, Value>) -> Result<(), CliError> {
    let mut ws = Workspace::open(dir)?;
    let who: Actor = actor(None)?;
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws: &mut ws,
        actor: who,
        turns: &mut turns,
        extra_servers: &[],
    };
    let out =
        tools::call(&mut ctx, name, Value::Object(args)).map_err(|e| CliError::Invalid(e.0))?;
    for part in out.content {
        if let Content::Text(text) = part {
            match serde_json::from_str::<Value>(&text) {
                Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap_or(text)),
                Err(_) => println!("{text}"),
            }
        }
    }
    Ok(())
}

pub fn export_atlas(a: ExportAtlas) -> Result<(), CliError> {
    let mut args = Map::new();
    args.insert("name".into(), json!(a.name));
    let fields = [
        ("frame", a.frame.map(Value::from)),
        ("cols", a.cols.map(Value::from)),
        ("rows", a.rows.map(Value::from)),
        ("cell_w", a.cell_w.map(Value::from)),
        ("cell_h", a.cell_h.map(Value::from)),
        ("columns", a.columns.map(Value::from)),
        ("padding", a.padding.map(Value::from)),
    ];
    for (key, value) in fields {
        if let Some(v) = value {
            args.insert(key.into(), v);
        }
    }
    if !a.pack_frame.is_empty() {
        args.insert("frames".into(), json!(a.pack_frame));
    }
    if !a.cell_names.is_empty() {
        args.insert("names".into(), json!(a.cell_names));
    }
    if let (Some(x), Some(y)) = (a.pivot_x, a.pivot_y) {
        args.insert("pivot".into(), json!({"x": x, "y": y}));
    }
    if let Some(n) = a.nine_slice.filter(|n| n.len() == 4) {
        args.insert(
            "nine_slice".into(),
            json!({"l": n[0], "t": n[1], "r": n[2], "b": n[3]}),
        );
    }
    if let Some(path) = a.frame_meta {
        let text = read_capped(&path, MAX_META_BYTES)?;
        let meta: Value = serde_json::from_slice(&text)
            .map_err(|e| CliError::Invalid(format!("{}: {e}", path.display())))?;
        args.insert("frame_meta".into(), meta);
    }
    args.insert("scale".into(), json!(a.scale));
    args.insert("pixel_art".into(), json!(a.pixel_art));
    run_tool(&a.dir, "export_atlas", args)
}

pub fn quantize_image(a: QuantizeImage) -> Result<(), CliError> {
    let mut args = Map::new();
    args.insert("file".into(), json!(a.file));
    match a.pack {
        Some(pack) => args.insert("palette".into(), json!(pack)),
        None => args.insert("palette".into(), json!(a.colors)),
    };
    if let Some(t) = a.alpha_threshold {
        args.insert("alpha_threshold".into(), json!(t));
    }
    run_tool(&a.dir, "quantize_image", args)
}

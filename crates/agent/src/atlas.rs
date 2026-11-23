//! Atlas export: one PNG of cells plus a JSON manifest that says where every cell is, for a game
//! engine to load (`docs/wiki/store-assets.md#atlas-export`). A sprite sheet is one frame cut
//! into a `cols` x `rows` grid of equal cells; a set of named frames is packed into a grid of
//! cells as big as the biggest frame. Everything is rendered, checked and encoded before the first
//! file is written, so a mistake leaves nothing behind.

use std::collections::{BTreeMap, BTreeSet};

use od_core::{NodeId, Workspace};
use od_render::Pixmap;
use serde::{Deserialize, Serialize};

use crate::export::{export_name, exports_dir};
use crate::tools::ToolError;

/// Largest side of an atlas, in pixels.
pub const MAX_SIDE: u32 = 16_384;
/// Largest number of pixels of an atlas.
pub const MAX_PIXELS: u64 = 64 * 1024 * 1024;
/// Largest number of cells of a grid, or of frames packed into a sheet.
pub const MAX_CELLS: usize = 1024;
/// Most distinct colours a manifest lists; a sheet with more has no `palette`.
pub const MAX_PALETTE: usize = 256;

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

/// Where a frame's origin sits inside its cell, in pixels from the cell's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pivot {
    pub x: f64,
    pub y: f64,
}

/// Margins (in pixels) that stay fixed when a frame is stretched: the corners of a 9-slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NineSlice {
    pub l: u32,
    pub t: u32,
    pub r: u32,
    pub b: u32,
}

/// What the manifest says about a frame besides where it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot: Option<Pivot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nine_slice: Option<NineSlice>,
}

/// Where the cells come from.
pub enum Source<'a> {
    /// One frame cut into `cols` x `rows` cells of `cell_w` x `cell_h` output pixels.
    Grid {
        frame: &'a NodeId,
        cols: u32,
        rows: u32,
        cell_w: u32,
        cell_h: u32,
    },
    /// Several frames packed `columns` to a row into cells as big as the biggest frame, with
    /// `padding` pixels around every cell.
    Frames {
        frames: &'a [NodeId],
        columns: u32,
        padding: u32,
    },
}

pub struct AtlasPlan<'a> {
    /// File name stem: the files are `exports/<name>.png` and `exports/<name>.json`.
    pub name: &'a str,
    pub source: Source<'a>,
    /// One name per cell (row by row for a grid, in order for frames). Without them a grid's cells
    /// are `<name>-00`, `-01`, ... and packed frames take their node's name.
    pub names: Option<&'a [String]>,
    /// Applies to every frame that `overrides` does not name.
    pub default_meta: FrameMeta,
    pub overrides: &'a BTreeMap<String, FrameMeta>,
    pub scale: f32,
    pub pixel_art: bool,
}

/// A cell as the manifest lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AtlasFrame {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pivot: Option<Pivot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nine_slice: Option<NineSlice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

/// The JSON file written next to the PNG.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Manifest {
    /// File name of the PNG, next to the manifest.
    pub image: String,
    pub size: Size,
    pub frames: Vec<AtlasFrame>,
    /// Every distinct colour of the sheet as `#rrggbb` (`#rrggbbaa` when translucent), unless
    /// there are more than [`MAX_PALETTE`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette: Option<Vec<String>>,
    pub generator: String,
    /// UTC time of the export, `YYYY-MM-DDTHH:MM:SSZ`.
    pub created: String,
}

pub struct AtlasOutcome {
    /// Relative to the project folder, for example `exports/hero.png`.
    pub image_path: String,
    pub manifest_path: String,
    pub bytes: usize,
    pub manifest: Manifest,
    pub warnings: Vec<String>,
}

/// A cell name: letters, digits and `. _ -`, up to 64 characters.
fn cell_name(name: &str) -> Result<(), ToolError> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        fail(format!(
            "{name:?} is not a frame name: use 1 to 64 letters, digits and . _ -"
        ))
    }
}

/// The node name as a frame name: anything outside the allowed characters becomes `-`.
fn sanitized(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .take(64)
        .collect();
    s.trim_matches('-').to_string()
}

fn check_size(w: u64, h: u64) -> Result<(), ToolError> {
    if w == 0 || h == 0 || w > u64::from(MAX_SIDE) || h > u64::from(MAX_SIDE) || w * h > MAX_PIXELS
    {
        return fail(format!(
            "an atlas of {w}x{h} pixels is out of range: each side at most {MAX_SIDE}, at most {MAX_PIXELS} pixels"
        ));
    }
    Ok(())
}

/// The cells of a grid, row by row, with their names still to be chosen.
fn grid_rects(cols: u32, rows: u32, cell_w: u32, cell_h: u32) -> Vec<(u32, u32, u32, u32)> {
    (0..rows)
        .flat_map(|row| (0..cols).map(move |col| (col * cell_w, row * cell_h, cell_w, cell_h)))
        .collect()
}

/// Copies `src` into `dst` with its top-left corner at (`x`, `y`). Both are premultiplied RGBA.
fn blit(dst: &mut Pixmap, src: &Pixmap, x: u32, y: u32) {
    let dst_stride = dst.width() as usize * 4;
    let src_stride = src.width() as usize * 4;
    for row in 0..src.height() as usize {
        let from = row * src_stride;
        let to = (y as usize + row) * dst_stride + x as usize * 4;
        dst.data_mut()[to..to + src_stride].copy_from_slice(&src.data()[from..from + src_stride]);
    }
}

/// The distinct colours of `pixmap` that are not fully transparent, sorted, or `None` when there
/// are more than [`MAX_PALETTE`].
fn palette_of(pixmap: &Pixmap) -> Option<Vec<String>> {
    let mut seen: BTreeSet<[u8; 4]> = BTreeSet::new();
    for px in pixmap.data().chunks_exact(4) {
        let a = u32::from(px[3]);
        if a == 0 {
            continue;
        }
        let straight = |c: u8| ((u32::from(c) * 255 + a / 2) / a).min(255) as u8;
        seen.insert([straight(px[0]), straight(px[1]), straight(px[2]), px[3]]);
        if seen.len() > MAX_PALETTE {
            return None;
        }
    }
    Some(
        seen.into_iter()
            .map(|[r, g, b, a]| {
                if a == 255 {
                    format!("#{r:02x}{g:02x}{b:02x}")
                } else {
                    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
                }
            })
            .collect(),
    )
}

fn check_meta(name: &str, w: u32, h: u32, meta: &FrameMeta) -> Result<(), ToolError> {
    if let Some(p) = meta.pivot
        && !(p.x.is_finite()
            && p.y.is_finite()
            && (0.0..=f64::from(w)).contains(&p.x)
            && (0.0..=f64::from(h)).contains(&p.y))
    {
        return fail(format!(
            "pivot of {name} is outside its {w}x{h} cell: x 0 to {w}, y 0 to {h}"
        ));
    }
    if let Some(n) = meta.nine_slice
        && (u64::from(n.l) + u64::from(n.r) > u64::from(w)
            || u64::from(n.t) + u64::from(n.b) > u64::from(h))
    {
        return fail(format!(
            "nine_slice of {name} does not fit its {w}x{h} cell: l + r at most {w}, t + b at most {h}"
        ));
    }
    Ok(())
}

/// What `plan` makes: the sheet and the manifest that describes it, nothing written yet.
struct Built {
    sheet: Pixmap,
    frames: Vec<AtlasFrame>,
    warnings: Vec<String>,
}

fn build(
    ws: &Workspace,
    plan: &AtlasPlan,
    render: &dyn Fn(&NodeId) -> Result<od_render::Rendered, od_render::RenderError>,
) -> Result<Built, ToolError> {
    let (sheet, rects, defaults, warnings) = match &plan.source {
        Source::Grid {
            frame,
            cols,
            rows,
            cell_w,
            cell_h,
        } => {
            if !(1..=MAX_CELLS as u32).contains(cols) || !(1..=MAX_CELLS as u32).contains(rows) {
                return fail(format!("cols and rows must be between 1 and {MAX_CELLS}"));
            }
            if (*cols as usize) * (*rows as usize) > MAX_CELLS {
                return fail(format!("a grid has at most {MAX_CELLS} cells"));
            }
            if *cell_w == 0 || *cell_h == 0 {
                return fail("cell_w and cell_h must be at least 1");
            }
            let rendered = render(frame)?;
            let want = (
                u64::from(*cols) * u64::from(*cell_w),
                u64::from(*rows) * u64::from(*cell_h),
            );
            let got = (
                u64::from(rendered.pixmap.width()),
                u64::from(rendered.pixmap.height()),
            );
            if got != want {
                return fail(format!(
                    "{frame} renders at {}x{} pixels, but {cols} x {rows} cells of {cell_w}x{cell_h} need {}x{}; \
                     resize the frame or change the grid (cell sizes are output pixels, after scale)",
                    got.0, got.1, want.0, want.1
                ));
            }
            let warnings = rendered.warnings.iter().map(ToString::to_string).collect();
            let rects = grid_rects(*cols, *rows, *cell_w, *cell_h);
            let defaults: Vec<String> = (0..rects.len())
                .map(|i| format!("{}-{i:02}", plan.name))
                .collect();
            (rendered.pixmap, rects, defaults, warnings)
        }
        Source::Frames {
            frames,
            columns,
            padding,
        } => {
            if frames.is_empty() || frames.len() > MAX_CELLS {
                return fail(format!("pack 1 to {MAX_CELLS} frames"));
            }
            if *columns == 0 {
                return fail("columns must be at least 1");
            }
            let mut pictures = Vec::with_capacity(frames.len());
            let mut warnings: Vec<String> = Vec::new();
            for frame in *frames {
                let rendered = render(frame)?;
                warnings.extend(rendered.warnings.iter().map(ToString::to_string));
                pictures.push(rendered.pixmap);
            }
            let cell_w = pictures.iter().map(Pixmap::width).max().unwrap_or(1);
            let cell_h = pictures.iter().map(Pixmap::height).max().unwrap_or(1);
            let columns = (*columns).min(frames.len() as u32);
            let rows = (frames.len() as u32).div_ceil(columns);
            let pad = u64::from(*padding);
            let width = pad + u64::from(columns) * (u64::from(cell_w) + pad);
            let height = pad + u64::from(rows) * (u64::from(cell_h) + pad);
            check_size(width, height)?;
            let mut sheet = Pixmap::new(width as u32, height as u32)
                .ok_or_else(|| ToolError("the atlas is too large to draw".into()))?;
            let mut rects = Vec::with_capacity(pictures.len());
            for (i, picture) in pictures.iter().enumerate() {
                let (col, row) = (i as u32 % columns, i as u32 / columns);
                let x = *padding + col * (cell_w + padding);
                let y = *padding + row * (cell_h + padding);
                blit(&mut sheet, picture, x, y);
                rects.push((x, y, picture.width(), picture.height()));
            }
            let defaults: Vec<String> = frames
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let node = ws.session.document().node(id).map(|n| sanitized(&n.name));
                    node.filter(|n| !n.is_empty())
                        .unwrap_or_else(|| format!("{}-{i:02}", plan.name))
                })
                .collect();
            (sheet, rects, defaults, warnings)
        }
    };
    check_size(u64::from(sheet.width()), u64::from(sheet.height()))?;

    let names: Vec<String> = match plan.names {
        Some(given) => {
            if given.len() != rects.len() {
                return fail(format!(
                    "names has {} entries but the atlas has {} cells",
                    given.len(),
                    rects.len()
                ));
            }
            given.to_vec()
        }
        None => defaults,
    };
    let mut seen = BTreeSet::new();
    for name in &names {
        cell_name(name)?;
        if !seen.insert(name.as_str()) {
            return fail(format!("the frame name {name:?} is used twice"));
        }
    }
    if let Some(unknown) = plan.overrides.keys().find(|k| !seen.contains(k.as_str())) {
        return fail(format!(
            "frame_meta names {unknown:?}, which is not a frame of this atlas"
        ));
    }
    let mut frames = Vec::with_capacity(rects.len());
    for ((x, y, w, h), name) in rects.into_iter().zip(names) {
        let meta = plan
            .overrides
            .get(&name)
            .copied()
            .unwrap_or(plan.default_meta);
        check_meta(&name, w, h, &meta)?;
        frames.push(AtlasFrame {
            name,
            x,
            y,
            w,
            h,
            pivot: meta.pivot,
            nine_slice: meta.nine_slice,
        });
    }
    Ok(Built {
        sheet,
        frames,
        warnings,
    })
}

/// UTC time as `YYYY-MM-DDTHH:MM:SSZ`.
fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = crate::memory::civil((secs / 86_400) as i64);
    let rest = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Renders the cells of `plan`, checks them, and writes `exports/<name>.png` and
/// `exports/<name>.json`. Nothing is written unless the whole atlas is valid; if the second file
/// cannot be written the first is removed again.
pub fn export_atlas(ws: &Workspace, plan: &AtlasPlan) -> Result<AtlasOutcome, ToolError> {
    let dir = exports_dir(ws)?;
    let png_name = export_name(&format!("{}.png", plan.name))?.to_string();
    let json_name = format!("{}.json", plan.name);
    if !(0.01..=16.0).contains(&plan.scale) || !plan.scale.is_finite() {
        return fail("scale must be between 0.01 and 16");
    }
    let doc = ws.session.document();
    let render = |frame: &NodeId| {
        crate::video_export::render_still_with(doc, frame, plan.scale, ws.dir(), plan.pixel_art)
    };
    let built = build(ws, plan, &render)?;
    let png = od_render::encode_png(&built.sheet, true)?;
    let manifest = Manifest {
        image: png_name.clone(),
        size: Size {
            w: built.sheet.width(),
            h: built.sheet.height(),
        },
        frames: built.frames,
        palette: palette_of(&built.sheet),
        generator: format!("opendesigner {}", env!("CARGO_PKG_VERSION")),
        created: now_utc(),
    };
    let mut text = serde_json::to_string_pretty(&manifest).map_err(|e| ToolError(e.to_string()))?;
    text.push('\n');

    std::fs::create_dir_all(&dir)?;
    let png_path = dir.join(&png_name);
    std::fs::write(&png_path, &png)?;
    if let Err(e) = std::fs::write(dir.join(&json_name), text) {
        let _ = std::fs::remove_file(&png_path);
        return Err(e.into());
    }
    Ok(AtlasOutcome {
        image_path: format!("exports/{png_name}"),
        manifest_path: format!("exports/{json_name}"),
        bytes: png.len(),
        manifest,
        warnings: built.warnings,
    })
}

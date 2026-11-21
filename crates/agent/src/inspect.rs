//! Tools an agent uses to look closely at its own video work (`docs/wiki/agent-integration.md`,
//! "Inspection tools"): every frame of a short stretch, a full-resolution crop, a labeled
//! coordinate grid, and the difference between two moments. All read-only; each answers with one
//! PNG image and a line of text, like `video_contact_sheet`. The CLI (`opendesigner inspect`)
//! runs the same code and writes the picture into `exports/`.

mod draw;
mod geometry;

use std::borrow::Cow;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use od_core::{Document, NodeId, Video};
use od_media::{Ffmpeg, FootageDecoder};
use od_render::Pixmap;
use od_render::video::VideoRenderer;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{CallCtx, Content, Output, ToolError, node_id, parse, tool};
use crate::video_export::{Decoder, Source, document_in, has_footage};
use draw::PixelRect;

pub use geometry::node_rect;

pub const NAMES: [&str; 4] = ["video_strip", "video_crop", "frame_grid", "video_diff"];

/// Most pictures in a strip.
pub const MAX_STRIP_PICTURES: usize = 36;
const STRIP_COLUMNS: usize = 6;
const STRIP_COLUMNS_PORTRAIT: usize = 9;
const DEFAULT_CELL_SIDE: u32 = 192;
/// Longest side of a crop, of the picture a crop returns, and of a grid or diff picture.
const MAX_CROP_SIDE: u32 = 2_048;
const MAX_ZOOM: u32 = 4;
const MAX_PADDING: u32 = 512;
const DEFAULT_PADDING: u32 = 24;
const DEFAULT_GRID_SIDE: u32 = 768;
const DEFAULT_DIFF_SIDE: u32 = 512;
const DEFAULT_DIFF_THRESHOLD: u8 = 12;
const GRID_STEPS: [u32; 10] = [10, 20, 25, 50, 100, 200, 250, 500, 1_000, 2_000];
const MAX_GRID_LINES: u32 = 12;

/// One finished picture: the PNG, its size and the text that says what it shows.
pub struct Picture {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub note: String,
}

impl Picture {
    fn of(pixmap: &Pixmap, note: String) -> Result<Picture, ToolError> {
        Ok(Picture {
            png: od_render::encode_png(pixmap, false)?,
            width: pixmap.width(),
            height: pixmap.height(),
            note,
        })
    }

    fn into_output(self) -> Output {
        Output {
            content: vec![
                Content::Image {
                    data: STANDARD.encode(self.png),
                    mime: "image/png",
                },
                Content::Text(self.note),
            ],
        }
    }
}

pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    let ms = |d: &str| json!({"type": "integer", "minimum": 0, "description": d});
    let locale = id("A language from list_locales.");
    vec![
        tool(
            "video_strip",
            "Every frame (or every Nth) of a short stretch of a video, in one labeled grid read \
             left to right, top to bottom: the way to judge anticipation, overshoot, settling and \
             a transition frame by frame. At most 36 pictures; a longer stretch is refused with the \
             `every` that fits, so narrow `from_ms` and `to_ms` to the move you are judging. Each \
             picture carries its time in milliseconds.",
            json!({
                "video": id("Id of a video from list_videos."),
                "from_ms": ms("First moment."),
                "to_ms": ms("Last moment, included."),
                "every": {"type": "integer", "minimum": 1, "maximum": 600, "description": "Take every Nth frame (default 1: all)."},
                "cell_side": {"type": "integer", "minimum": 64, "maximum": 512, "description": "Longest side of each picture (default 192)."},
                "locale": locale.clone()
            }),
            &["video", "from_ms", "to_ms"],
            true,
        ),
        tool(
            "video_crop",
            "A piece of the video's picture at full resolution (1:1), to inspect a face, a hand, a \
             contact point or small text. Give `x`, `y`, `width`, `height` in video pixels, or a \
             `node` id: the crop then follows that node's box through the camera, the clip's zoom \
             and the node's own transforms at that moment, plus `padding`. `zoom` 2 to 4 enlarges \
             each pixel (no smoothing) for a tiny region. Use frame_grid first to read positions.",
            json!({
                "video": id("Id of a video from list_videos."),
                "time_ms": ms("The moment."),
                "x": {"type": "integer", "minimum": 0}, "y": {"type": "integer", "minimum": 0},
                "width": {"type": "integer", "minimum": 1, "maximum": MAX_CROP_SIDE},
                "height": {"type": "integer", "minimum": 1, "maximum": MAX_CROP_SIDE},
                "node": id("Instead of a region: a node id from get_tree whose box to follow."),
                "padding": {"type": "integer", "minimum": 0, "maximum": MAX_PADDING, "description": "Video pixels added around a node's box (default 24)."},
                "zoom": {"type": "integer", "minimum": 1, "maximum": MAX_ZOOM, "description": "Enlarge each pixel this many times (default 1)."},
                "locale": locale.clone()
            }),
            &["video", "time_ms"],
            true,
        ),
        tool(
            "frame_grid",
            "The picture at a moment with a labeled coordinate grid over it: lines every `spacing` \
             video pixels, numbered along the top (x) and left (y) edges in video pixels, so you \
             can read where things are and give exact x, y, width, height to video_crop or to \
             apply_ops.",
            json!({
                "video": id("Id of a video from list_videos."),
                "time_ms": ms("The moment."),
                "spacing": {"type": "integer", "minimum": 5, "maximum": 2000, "description": "Video pixels between lines (default: about 10 lines along the long side)."},
                "max_side": {"type": "integer", "minimum": 256, "maximum": 1536, "description": "Longest side of the picture (default 768)."},
                "locale": locale.clone()
            }),
            &["video", "time_ms"],
            true,
        ),
        tool(
            "video_diff",
            "Two moments side by side with the pixels that changed painted magenta over a dimmed \
             copy: the first picture, the second, then the changes (`mode` overlay gives only the \
             changes). Compare a time with another time of the video (`other_time_ms`), or the same \
             or another time of another video of the project (`other_video`, same size). The text \
             gives the share of pixels changed and their box. Use it on a nearly identical pair, \
             such as the frames before and after a hit, or two takes of a scene.",
            json!({
                "video": id("Id of a video from list_videos."),
                "time_ms": ms("The first moment."),
                "other_time_ms": ms("The second moment (default: the same as time_ms)."),
                "other_video": id("A second video to take the second moment from (default: the same video)."),
                "mode": {"type": "string", "enum": ["side_by_side", "overlay"], "description": "Default side_by_side."},
                "threshold": {"type": "integer", "minimum": 0, "maximum": 255, "description": "A channel must differ by more than this to count as changed (default 12)."},
                "max_side": {"type": "integer", "minimum": 128, "maximum": 1024, "description": "Longest side of each picture (default 512)."},
                "locale": locale
            }),
            &["video", "time_ms"],
            true,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    Ok(run(&Source::of(ctx.ws), name, args)?.into_output())
}

/// Runs the inspection tool `name` on the project in `src`.
pub fn run(src: &Source, name: &str, args: Value) -> Result<Picture, ToolError> {
    match name {
        "video_strip" => strip(src, parse(args)?),
        "video_crop" => crop(src, parse(args)?),
        "frame_grid" => frame_grid(src, parse(args)?),
        "video_diff" => diff(src, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

/// Writes `png` to `exports/<file>` in the project folder `dir` and returns the relative path.
/// The name must be a plain `.png` name; an `exports/` folder or file that is a symbolic link is
/// refused so a link cannot send the write elsewhere.
pub fn write_export(dir: &Path, file: &str, png: &[u8]) -> Result<String, ToolError> {
    let name = crate::export::export_name(file)?;
    let exports = dir.join("exports");
    let target = exports.join(name);
    for path in [&exports, &target] {
        if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(ToolError(format!(
                "{} is a symbolic link; nothing is written through it",
                path.display()
            )));
        }
    }
    std::fs::create_dir_all(&exports)?;
    std::fs::write(&target, png)?;
    Ok(format!("exports/{name}"))
}

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

/// What a video's timeline is, from the project as saved.
struct Facts {
    id: NodeId,
    duration_ms: u64,
    fps: u32,
    width: u32,
    height: u32,
}

fn facts(src: &Source, video: &str) -> Result<Facts, ToolError> {
    let id = node_id(video)?;
    let v = src.doc.video(&id).ok_or_else(|| {
        ToolError(format!(
            "video {video} does not exist; list_videos shows them"
        ))
    })?;
    if v.clips.is_empty() {
        return fail(format!(
            "video {video} has no clips, so there is nothing to look at"
        ));
    }
    Ok(Facts {
        id,
        duration_ms: v.duration_ms(),
        fps: v.fps,
        width: v.width,
        height: v.height,
    })
}

impl Facts {
    fn check_time(&self, label: &str, ms: u64) -> Result<(), ToolError> {
        if ms >= self.duration_ms {
            return fail(format!(
                "{label} {ms} ms is past the end: the video is {} ms long (last moment {})",
                self.duration_ms,
                self.duration_ms - 1
            ));
        }
        Ok(())
    }

    /// The size a picture of the video takes when its longest side is at most `side`.
    fn fit(&self, side: u32) -> (u32, u32) {
        let shrink = (f64::from(side.max(16)) / f64::from(self.width.max(self.height))).min(1.0);
        (
            ((f64::from(self.width) * shrink).round() as u32).max(1),
            ((f64::from(self.height) * shrink).round() as u32).max(1),
        )
    }
}

/// The pictures of a video at `times`, `size` pixels each, and what could not be drawn.
fn render(
    src: &Source,
    video: &NodeId,
    locale: Option<&str>,
    size: (u32, u32),
    times: &[u64],
) -> Result<(Vec<Pixmap>, Vec<String>), ToolError> {
    let doc: Cow<Document> = document_in(src, locale)?;
    let v: &Video = doc
        .video(video)
        .ok_or_else(|| ToolError(format!("video {video} does not exist")))?;
    let mut footage = if has_footage(&doc, v) {
        let dir = src
            .dir()
            .ok_or_else(|| ToolError("the project has no folder, so no footage".into()))?;
        let ffmpeg = Ffmpeg::locate().map_err(|e| ToolError(e.to_string()))?;
        Some(Decoder(FootageDecoder::new(
            ffmpeg,
            dir.to_path_buf(),
            v.fps,
        )))
    } else {
        None
    };
    let mut renderer = VideoRenderer::new(&doc, v, src.dir()).with_size(size.0, size.1);
    if let Some(f) = footage.as_mut() {
        renderer = renderer.with_footage(f);
    }
    let pictures = times
        .iter()
        .map(|&t| renderer.frame_at(t))
        .collect::<Result<Vec<_>, _>>()?;
    let warnings = renderer
        .warnings()
        .iter()
        .map(ToString::to_string)
        .collect();
    Ok((pictures, warnings))
}

fn with_warnings(mut note: String, warnings: &[String]) -> String {
    if !warnings.is_empty() {
        note.push_str(&format!("; warnings: {}", warnings.join("; ")));
    }
    note
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StripArgs {
    video: String,
    from_ms: u64,
    to_ms: u64,
    #[serde(default)]
    every: Option<u32>,
    #[serde(default)]
    cell_side: Option<u32>,
    #[serde(default)]
    locale: Option<String>,
}

/// The moments of every `every`th frame from `from` to `to`, both included.
fn frame_times(from: u64, to: u64, fps: u32, every: u32) -> Vec<u64> {
    let step = u64::from(every) * 1000;
    let rate = u64::from(fps.max(1));
    (0..)
        .map(|k| from + (k * step + rate / 2) / rate)
        .take_while(|t| *t <= to)
        .collect()
}

fn strip(src: &Source, a: StripArgs) -> Result<Picture, ToolError> {
    let f = facts(src, &a.video)?;
    if a.from_ms > a.to_ms {
        return fail("from_ms must not be after to_ms");
    }
    f.check_time("to_ms", a.to_ms)?;
    let every = a.every.unwrap_or(1).max(1);
    let times = frame_times(a.from_ms, a.to_ms, f.fps, every);
    if times.len() > MAX_STRIP_PICTURES {
        let fits =
            (1..).find(|n| frame_times(a.from_ms, a.to_ms, f.fps, *n).len() <= MAX_STRIP_PICTURES);
        return fail(format!(
            "{} to {} ms holds {} frames at {} fps and a strip shows at most {MAX_STRIP_PICTURES}: \
             narrow the range or set every to {} or more",
            a.from_ms,
            a.to_ms,
            times.len(),
            f.fps,
            fits.unwrap_or(every)
        ));
    }
    let cell = a.cell_side.unwrap_or(DEFAULT_CELL_SIDE).clamp(64, 512);
    let (mut pictures, warnings) = render(src, &f.id, a.locale.as_deref(), f.fit(cell), &times)?;
    for (pic, t) in pictures.iter_mut().zip(&times) {
        let scale = draw::label_scale(pic);
        draw::label(pic, &t.to_string(), (3 * scale, 3 * scale), scale);
    }
    let columns = if f.height > f.width {
        STRIP_COLUMNS_PORTRAIT
    } else {
        STRIP_COLUMNS
    }
    .min(times.len()) as u32;
    let sheet = od_render::grid(&pictures, columns)
        .ok_or_else(|| ToolError("the strip is too large to draw".into()))?;
    let note = format!(
        "{}x{} strip of {}: {} frames from {} to {} ms, every {} frame{} ({} fps), {} to a row, \
         each labeled with its time in ms",
        sheet.width(),
        sheet.height(),
        a.video,
        times.len(),
        times.first().copied().unwrap_or(a.from_ms),
        times.last().copied().unwrap_or(a.to_ms),
        every,
        if every == 1 { "" } else { "s" },
        f.fps,
        columns
    );
    Picture::of(&sheet, with_warnings(note, &warnings))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CropArgs {
    video: String,
    time_ms: u64,
    #[serde(default)]
    x: Option<u32>,
    #[serde(default)]
    y: Option<u32>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    node: Option<String>,
    #[serde(default)]
    padding: Option<u32>,
    #[serde(default)]
    zoom: Option<u32>,
    #[serde(default)]
    locale: Option<String>,
}

/// The region to cut: the one given, or the node's box with padding, kept inside the picture.
fn crop_region(src: &Source, f: &Facts, a: &CropArgs) -> Result<(PixelRect, String), ToolError> {
    let bounds = PixelRect {
        x: 0,
        y: 0,
        width: f.width,
        height: f.height,
    };
    let Some(node) = &a.node else {
        let (Some(x), Some(y), Some(width), Some(height)) = (a.x, a.y, a.width, a.height) else {
            return fail("give x, y, width and height, or a node");
        };
        let rect = clamp_rect(
            (
                i64::from(x),
                i64::from(y),
                i64::from(x) + i64::from(width),
                i64::from(y) + i64::from(height),
            ),
            bounds,
        )
        .ok_or_else(|| {
            ToolError(format!(
                "the region {x},{y} {width}x{height} lies outside the {}x{} picture",
                f.width, f.height
            ))
        })?;
        return Ok((rect, format!("region {x},{y} {width}x{height}")));
    };
    if a.x.is_some() || a.y.is_some() || a.width.is_some() || a.height.is_some() {
        return fail("give either a node or x, y, width and height, not both");
    }
    let doc = document_in(src, a.locale.as_deref())?;
    let v = doc
        .video(&f.id)
        .ok_or_else(|| ToolError("no such video".into()))?;
    let id = node_id(node)?;
    let at = geometry::node_rect(&doc, v, &id, a.time_ms).ok_or_else(|| {
        ToolError(format!(
            "{node} is not on screen at {} ms: no clip or overlay shows it then, or it is hidden",
            a.time_ms
        ))
    })?;
    let pad = f64::from(a.padding.unwrap_or(DEFAULT_PADDING).min(MAX_PADDING));
    let edges = (
        (at.x - pad).floor() as i64,
        (at.y - pad).floor() as i64,
        (at.x + at.width + pad).ceil() as i64,
        (at.y + at.height + pad).ceil() as i64,
    );
    let rect = clamp_rect(edges, bounds).ok_or_else(|| {
        ToolError(format!(
            "{node} is entirely outside the picture at {} ms",
            a.time_ms
        ))
    })?;
    let note = format!(
        "node {node} box {:.0},{:.0} {:.0}x{:.0} plus {pad} px padding",
        at.x, at.y, at.width, at.height
    );
    Ok((rect, note))
}

/// `(left, top, right, bottom)` cut to `bounds`, or `None` when nothing is left.
fn clamp_rect((l, t, r, b): (i64, i64, i64, i64), bounds: PixelRect) -> Option<PixelRect> {
    let (l, t) = (l.max(0), t.max(0));
    let (r, b) = (
        r.min(i64::from(bounds.width)),
        b.min(i64::from(bounds.height)),
    );
    (r > l && b > t).then(|| PixelRect {
        x: l as u32,
        y: t as u32,
        width: (r - l) as u32,
        height: (b - t) as u32,
    })
}

fn crop(src: &Source, a: CropArgs) -> Result<Picture, ToolError> {
    let f = facts(src, &a.video)?;
    f.check_time("time_ms", a.time_ms)?;
    let zoom = a.zoom.unwrap_or(1).clamp(1, MAX_ZOOM);
    let (region, what) = crop_region(src, &f, &a)?;
    if region.width.max(region.height) * zoom > MAX_CROP_SIDE {
        return fail(format!(
            "a {}x{} crop at zoom {zoom} is over {MAX_CROP_SIDE} px on a side: ask for a smaller region or a lower zoom",
            region.width, region.height
        ));
    }
    let (frames, warnings) = render(
        src,
        &f.id,
        a.locale.as_deref(),
        (f.width, f.height),
        &[a.time_ms],
    )?;
    let piece = frames
        .first()
        .and_then(|p| draw::crop(p, region))
        .and_then(|p| draw::enlarge(&p, zoom))
        .ok_or_else(|| ToolError("the crop could not be drawn".into()))?;
    let note = format!(
        "{}x{} crop of {} at {} ms from {},{} ({what}), {}",
        piece.width(),
        piece.height(),
        a.video,
        a.time_ms,
        region.x,
        region.y,
        if zoom == 1 {
            "full resolution".to_string()
        } else {
            format!("each pixel enlarged {zoom} times")
        }
    );
    Picture::of(&piece, with_warnings(note, &warnings))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GridArgs {
    video: String,
    time_ms: u64,
    #[serde(default)]
    spacing: Option<u32>,
    #[serde(default)]
    max_side: Option<u32>,
    #[serde(default)]
    locale: Option<String>,
}

/// A round spacing with at most [`MAX_GRID_LINES`] lines along the long side.
fn auto_spacing(long_side: u32) -> u32 {
    GRID_STEPS
        .iter()
        .copied()
        .find(|s| long_side / s <= MAX_GRID_LINES)
        .unwrap_or(GRID_STEPS[GRID_STEPS.len() - 1])
}

fn frame_grid(src: &Source, a: GridArgs) -> Result<Picture, ToolError> {
    let f = facts(src, &a.video)?;
    f.check_time("time_ms", a.time_ms)?;
    let side = a.max_side.unwrap_or(DEFAULT_GRID_SIDE).clamp(256, 1536);
    let spacing = a
        .spacing
        .unwrap_or_else(|| auto_spacing(f.width.max(f.height)))
        .clamp(5, 2_000);
    let (mut frames, warnings) =
        render(src, &f.id, a.locale.as_deref(), f.fit(side), &[a.time_ms])?;
    let pic = frames
        .first_mut()
        .ok_or_else(|| ToolError("the picture could not be drawn".into()))?;
    draw::coordinate_grid(pic, (f.width, f.height), spacing);
    let note = format!(
        "{}x{} picture of {} at {} ms with a grid every {spacing} video pixels; the numbers are \
         video pixels ({}x{} in all), x along the top and y down the left",
        pic.width(),
        pic.height(),
        a.video,
        a.time_ms,
        f.width,
        f.height
    );
    Picture::of(pic, with_warnings(note, &warnings))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiffArgs {
    video: String,
    time_ms: u64,
    #[serde(default)]
    other_time_ms: Option<u64>,
    #[serde(default)]
    other_video: Option<String>,
    #[serde(default)]
    mode: Option<DiffMode>,
    #[serde(default)]
    threshold: Option<u8>,
    #[serde(default)]
    max_side: Option<u32>,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
enum DiffMode {
    SideBySide,
    Overlay,
}

fn diff(src: &Source, a: DiffArgs) -> Result<Picture, ToolError> {
    let first = facts(src, &a.video)?;
    let second_name = a.other_video.as_deref().unwrap_or(&a.video);
    let second = facts(src, second_name)?;
    let (t1, t2) = (a.time_ms, a.other_time_ms.unwrap_or(a.time_ms));
    if first.id == second.id && t1 == t2 {
        return fail("these are the same picture: give other_time_ms, or other_video");
    }
    first.check_time("time_ms", t1)?;
    second.check_time("other_time_ms", t2)?;
    if (first.width, first.height) != (second.width, second.height) {
        return fail(format!(
            "{} is {}x{} and {} is {}x{}: a diff needs two videos of one size",
            a.video, first.width, first.height, second_name, second.width, second.height
        ));
    }
    let size = first.fit(a.max_side.unwrap_or(DEFAULT_DIFF_SIDE).clamp(128, 1024));
    let locale = a.locale.as_deref();
    let (mut one, mut warnings) = render(src, &first.id, locale, size, &[t1])?;
    let (two, more) = render(src, &second.id, locale, size, &[t2])?;
    warnings.extend(more);
    let (Some(left), Some(right)) = (one.pop(), two.into_iter().next()) else {
        return fail("the pictures could not be drawn");
    };
    let (marked, changes) =
        draw::highlight(&left, &right, a.threshold.unwrap_or(DEFAULT_DIFF_THRESHOLD))
            .ok_or_else(|| ToolError("the pictures differ in size".into()))?;
    let sheet = match a.mode.unwrap_or(DiffMode::SideBySide) {
        DiffMode::Overlay => marked,
        DiffMode::SideBySide => od_render::grid(&[left, right, marked], 3)
            .ok_or_else(|| ToolError("the diff is too large to draw".into()))?,
    };
    let scale = f64::from(first.width) / f64::from(size.0);
    let where_ = changes.bounds.map_or_else(
        || "nothing changed".to_string(),
        |b| {
            format!(
                "changes lie in {:.0},{:.0} {:.0}x{:.0} (video pixels)",
                f64::from(b.x) * scale,
                f64::from(b.y) * scale,
                f64::from(b.width) * scale,
                f64::from(b.height) * scale
            )
        },
    );
    let note = format!(
        "{}x{} diff of {} at {} ms against {} at {} ms: {:.2}% of pixels changed; {where_}",
        sheet.width(),
        sheet.height(),
        a.video,
        t1,
        second_name,
        t2,
        changes.share() * 100.0
    );
    Picture::of(&sheet, with_warnings(note, &warnings))
}

#[cfg(test)]
mod tests;

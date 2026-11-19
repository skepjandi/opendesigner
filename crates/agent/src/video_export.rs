//! Videos out of the project: one picture of a video at a moment, and the whole video as an MP4
//! in `<project>/exports/` (`docs/wiki/video-editing.md`). Every frame is drawn by `render`;
//! `ffmpeg` only compresses them (see `od_media`).

use od_core::{NodeId, Workspace};
use od_media::{EncodeSettings, Ffmpeg, VideoWriter};
use od_render::video::VideoRenderer;
use od_store::video::{VideoFacts, VideoPreset, VideoViolation};

use crate::export::exports_dir;
use crate::tools::ToolError;

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

/// File names only, ending in `.mp4`: `preview.mp4`.
pub(crate) fn video_file_name(file: &str) -> Result<&str, ToolError> {
    let stem_ok = file.strip_suffix(".mp4").is_some_and(|stem| {
        !stem.is_empty()
            && stem.len() <= 96
            && !stem.contains("..")
            && stem
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
            && stem
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    });
    if stem_ok {
        Ok(file)
    } else {
        fail("file must be a plain name like preview.mp4 (letters, digits . _ -)")
    }
}

/// The document as it reads in `locale`, or as it is.
fn document_in<'a>(
    ws: &'a Workspace,
    locale: Option<&str>,
) -> Result<std::borrow::Cow<'a, od_core::Document>, ToolError> {
    match locale {
        None => Ok(std::borrow::Cow::Borrowed(ws.session.document())),
        Some(l) => ws
            .session
            .document()
            .localized(l)
            .map(std::borrow::Cow::Owned)
            .ok_or_else(|| ToolError(format!("the project has no translations for {l:?}"))),
    }
}

/// The picture at `time_ms`, as a PNG, with anything that could not be drawn.
pub fn video_frame_png(
    ws: &Workspace,
    video: &NodeId,
    time_ms: u64,
    locale: Option<&str>,
    max_side: u32,
) -> Result<(Vec<u8>, u32, u32, Vec<String>), ToolError> {
    let doc = document_in(ws, locale)?;
    let v = doc
        .video(video)
        .ok_or_else(|| ToolError(format!("video {video} does not exist")))?;
    let shrink = (f64::from(max_side.max(16)) / f64::from(v.width.max(v.height))).min(1.0);
    let (w, h) = (
        (f64::from(v.width) * shrink).round() as u32,
        (f64::from(v.height) * shrink).round() as u32,
    );
    let mut renderer = VideoRenderer::new(&doc, v, ws.dir()).with_size(w, h);
    let pixmap = renderer.frame_at(time_ms)?;
    let png = od_render::encode_png(&pixmap, false)?;
    let warnings = renderer
        .warnings()
        .iter()
        .map(ToString::to_string)
        .collect();
    Ok((png, pixmap.width(), pixmap.height(), warnings))
}

pub struct VideoOutcome {
    /// Relative to the project folder, for example `exports/preview.mp4`.
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: u64,
    pub duration_ms: u64,
    pub bytes: u64,
    /// The preset the video was checked against, the day its rules were read from the store's
    /// page, and what it does not check.
    pub preset: Option<String>,
    pub preset_checked: Option<String>,
    pub preset_not_checked: Option<String>,
    /// The H.264 encoder ffmpeg used.
    pub encoder: String,
    pub ffmpeg: String,
    /// The ffmpeg found is a GPL build (the user's own; the app does not ship it).
    pub ffmpeg_gpl: bool,
    pub warnings: Vec<String>,
}

/// Everything about an export that can be known before ffmpeg is needed: the file name, that the
/// video has clips, and that it meets the preset. Returns the preset.
pub fn check_export(
    ws: &Workspace,
    video: &NodeId,
    file: &str,
    locale: Option<&str>,
    preset_id: Option<&str>,
) -> Result<Option<&'static VideoPreset>, ToolError> {
    video_file_name(file)?;
    exports_dir(ws)?;
    let preset = preset_id
        .map(|id| {
            od_store::video::find_video(id).ok_or_else(|| {
                let ids: Vec<&str> = od_store::video::VIDEO_PRESETS
                    .iter()
                    .map(|p| p.id)
                    .collect();
                ToolError(format!(
                    "unknown video preset {id:?}; available: {}",
                    ids.join(", ")
                ))
            })
        })
        .transpose()?;
    let doc = document_in(ws, locale)?;
    let v = doc
        .video(video)
        .ok_or_else(|| ToolError(format!("video {video} does not exist")))?;
    if v.clips.is_empty() {
        return fail(format!(
            "video {video} has no clips yet; add one with add_clip"
        ));
    }
    if let Some(p) = preset {
        let broken = p.validate(&VideoFacts {
            width: v.width,
            height: v.height,
            fps: v.fps,
            duration_ms: v.duration_ms(),
            bytes: 0,
        });
        if !broken.is_empty() {
            return fail(violations_message(p, &broken));
        }
    }
    Ok(preset)
}

/// Draws every frame of `video` and encodes them to `exports/<file>`. `progress(done, total)` is
/// called after each frame and returns `false` to stop; stopping leaves no file. Nothing is
/// written unless the whole video encodes (and meets `preset_id`, when one is given).
pub fn export_video(
    ws: &Workspace,
    video: &NodeId,
    file: &str,
    locale: Option<&str>,
    preset_id: Option<&str>,
    ffmpeg: &Ffmpeg,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<VideoOutcome, ToolError> {
    let preset = check_export(ws, video, file, locale, preset_id)?;
    let file = video_file_name(file)?;
    let dir = exports_dir(ws)?;
    let doc = document_in(ws, locale)?;
    let v = doc
        .video(video)
        .ok_or_else(|| ToolError(format!("video {video} does not exist")))?;
    std::fs::create_dir_all(&dir).map_err(|e| ToolError(format!("{}: {e}", dir.display())))?;
    let settings = EncodeSettings {
        width: v.width,
        height: v.height,
        fps: v.fps,
        output: dir.join(file),
    };
    let mut writer = VideoWriter::start(ffmpeg, &settings).map_err(|e| ToolError(e.to_string()))?;
    let mut renderer = VideoRenderer::new(&doc, v, ws.dir());
    let total = v.frame_count();
    for n in 0..total {
        let time_ms = n * 1000 / u64::from(v.fps);
        let sent = renderer
            .frame_at(time_ms)
            .map_err(|e| ToolError(e.to_string()))
            .and_then(|p| {
                writer
                    .write_frame(p.data())
                    .map_err(|e| ToolError(e.to_string()))
            });
        if let Err(e) = sent {
            writer.abort();
            return Err(e);
        }
        if !progress(n + 1, total) {
            writer.abort();
            return fail("the export was cancelled");
        }
    }
    let encoder = writer.encoder.to_string();
    let bytes = writer.finish().map_err(|e| ToolError(e.to_string()))?;
    if let Some(p) = preset {
        let broken = p.validate(&VideoFacts {
            width: v.width,
            height: v.height,
            fps: v.fps,
            duration_ms: v.duration_ms(),
            bytes,
        });
        if !broken.is_empty() {
            let _ = std::fs::remove_file(dir.join(file));
            return fail(violations_message(p, &broken));
        }
    }
    Ok(VideoOutcome {
        path: format!("exports/{file}"),
        width: v.width,
        height: v.height,
        fps: v.fps,
        frames: total,
        duration_ms: v.duration_ms(),
        bytes,
        preset: preset.map(|p| p.id.to_string()),
        preset_checked: preset.map(|p| p.checked.to_string()),
        preset_not_checked: preset.map(|p| p.not_checked.to_string()),
        encoder,
        ffmpeg: ffmpeg.version.clone(),
        ffmpeg_gpl: ffmpeg.gpl,
        warnings: renderer
            .warnings()
            .iter()
            .map(ToString::to_string)
            .collect(),
    })
}

fn violations_message(preset: &VideoPreset, broken: &[VideoViolation]) -> String {
    let list: Vec<String> = broken.iter().map(|v| format!("  - {v}")).collect();
    format!(
        "not written: the video does not meet {} ({}):\n{}",
        preset.id,
        preset.source,
        list.join("\n")
    )
}

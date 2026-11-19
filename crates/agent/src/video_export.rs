//! Videos out of the project: one picture of a video at a moment, and the whole video as an MP4
//! in `<project>/exports/` (`docs/wiki/video-editing.md`). Every frame is drawn by `render`;
//! `ffmpeg` only compresses them (see `od_media`).

use od_core::{Document, NodeId, Workspace};
use od_media::{AudioPiece, EncodeSettings, Ffmpeg, FootageDecoder, VideoWriter};
use od_render::video::{FootageSource, VideoRenderer};
use od_store::video::{VideoFacts, VideoPreset, VideoViolation};

use crate::tools::ToolError;

/// What a video export reads from the project, copied out so the editor is not held while a
/// video of minutes is drawn and encoded (the workspace lock is taken only to make the copy).
pub struct Source {
    pub doc: Document,
    pub(crate) dir: Option<std::path::PathBuf>,
}

impl Source {
    pub fn of(ws: &Workspace) -> Source {
        Source {
            doc: ws.session.document().clone(),
            dir: ws.dir().map(std::path::Path::to_path_buf),
        }
    }

    pub fn dir(&self) -> Option<&std::path::Path> {
        self.dir.as_deref()
    }
}

fn exports_dir(src: &Source) -> Result<std::path::PathBuf, ToolError> {
    match src.dir() {
        Some(d) => Ok(d.join("exports")),
        None => fail("the project has no folder on disk, so there is nowhere to export to"),
    }
}

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
    ws: &'a Source,
    locale: Option<&str>,
) -> Result<std::borrow::Cow<'a, od_core::Document>, ToolError> {
    match locale {
        None => Ok(std::borrow::Cow::Borrowed(&ws.doc)),
        Some(l) => ws
            .doc
            .localized(l)
            .map(std::borrow::Cow::Owned)
            .ok_or_else(|| ToolError(format!("the project has no translations for {l:?}"))),
    }
}

/// Lets the renderer ask `ffmpeg` for footage pictures.
pub(crate) struct Decoder(pub(crate) FootageDecoder);

impl FootageSource for Decoder {
    fn frame(&mut self, file: &str, at_ms: u64, w: u32, h: u32) -> Result<Vec<u8>, String> {
        self.0.frame(file, at_ms, w, h).map_err(|e| e.to_string())
    }
}

pub(crate) fn has_footage(v: &od_core::Video) -> bool {
    v.clips.iter().any(|c| c.footage.is_some())
}

/// The picture at `time_ms`, as a PNG, with anything that could not be drawn.
pub fn video_frame_png(
    ws: &Source,
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
    let mut source = if has_footage(v) {
        let dir = ws
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
    let mut renderer = VideoRenderer::new(&doc, v, ws.dir()).with_size(w, h);
    if let Some(s) = source.as_mut() {
        renderer = renderer.with_footage(s);
    }
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
    /// How many footage sounds were mixed in.
    pub audio_pieces: usize,
    pub warnings: Vec<String>,
}

/// Everything about an export that can be known before ffmpeg is needed: the file name, that the
/// video has clips, and that it meets the preset. Returns the preset.
pub fn check_export(
    ws: &Source,
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
    ws: &Source,
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
    // Footage: each clip must lie inside its file; the files' sound goes under the picture.
    let project = ws.dir().map(std::path::Path::to_path_buf);
    let mut audio: Vec<AudioPiece> = Vec::new();
    let mut lengths: std::collections::HashMap<String, od_media::FileInfo> = Default::default();
    for (clip, placed) in v.clips.iter().zip(v.placed()) {
        let Some(f) = &clip.footage else { continue };
        let project = project
            .as_deref()
            .ok_or_else(|| ToolError("the project has no folder, so no footage".into()))?;
        let path =
            od_media::footage_path(project, &f.file).map_err(|e| ToolError(e.to_string()))?;
        if !lengths.contains_key(&f.file) {
            let info = ffmpeg
                .probe_file(&path)
                .map_err(|e| ToolError(e.to_string()))?;
            lengths.insert(f.file.clone(), info);
        }
        let info = &lengths[&f.file];
        let needs = f.in_ms + u64::from(clip.duration_ms);
        if needs > info.duration_ms + 100 {
            return fail(format!(
                "clip {} plays {} to {} ms of {}, which is only {} ms long; shorten the clip or move its in point",
                clip.id, f.in_ms, needs, f.file, info.duration_ms
            ));
        }
        if info.has_audio {
            audio.push(AudioPiece {
                file: path,
                in_ms: f.in_ms,
                start_ms: placed.start_ms,
                duration_ms: u64::from(clip.duration_ms),
                gain: 1.0,
                fade_in_ms: 0,
                fade_out_ms: 0,
            });
        }
    }
    // The sound track of the video: played under the picture, cut at the end of the video.
    let mut warnings: Vec<String> = Vec::new();
    if let Some(track) = &v.audio {
        let project = project
            .as_deref()
            .ok_or_else(|| ToolError("the project has no folder, so no sound track".into()))?;
        let path =
            od_media::audio_path(project, &track.file).map_err(|e| ToolError(e.to_string()))?;
        let length = ffmpeg
            .probe_duration(&path)
            .map_err(|e| ToolError(e.to_string()))?;
        let start = u64::from(track.start_ms);
        let left = v.duration_ms().saturating_sub(start);
        if left == 0 {
            warnings.push(format!(
                "the sound track {} starts after the video ends, so it is not in the file",
                track.file
            ));
        } else {
            audio.push(AudioPiece {
                file: path,
                in_ms: 0,
                start_ms: start,
                duration_ms: length.min(left),
                gain: track.gain,
                fade_in_ms: u64::from(track.fade_in_ms),
                fade_out_ms: u64::from(track.fade_out_ms),
            });
        }
    }
    std::fs::create_dir_all(&dir).map_err(|e| ToolError(format!("{}: {e}", dir.display())))?;
    let final_path = dir.join(file);
    let picture_path = if audio.is_empty() {
        final_path.clone()
    } else {
        dir.join(format!(".{file}.picture.mp4"))
    };
    let settings = EncodeSettings {
        width: v.width,
        height: v.height,
        fps: v.fps,
        output: picture_path.clone(),
    };
    let mut source = match &project {
        Some(p) if has_footage(v) => Some(Decoder(FootageDecoder::new(
            ffmpeg.clone(),
            p.clone(),
            v.fps,
        ))),
        _ => None,
    };
    let mut writer = VideoWriter::start(ffmpeg, &settings).map_err(|e| ToolError(e.to_string()))?;
    let mut renderer = VideoRenderer::new(&doc, v, ws.dir());
    if let Some(s) = source.as_mut() {
        renderer = renderer.with_footage(s);
    }
    let total = v.frame_count();
    // While nothing moves the picture is the same: draw it once and send it again.
    let mut held: Option<(Vec<i64>, Vec<u8>)> = None;
    for n in 0..total {
        let time_ms = n * 1000 / u64::from(v.fps);
        let sig = renderer.signature(time_ms);
        let sent = match (&sig, &held) {
            (Some(s), Some((last, pixels))) if s == last => writer
                .write_frame(pixels)
                .map_err(|e| ToolError(e.to_string())),
            _ => renderer
                .frame_at(time_ms)
                .map_err(|e| ToolError(e.to_string()))
                .and_then(|p| {
                    let sent = writer
                        .write_frame(p.data())
                        .map_err(|e| ToolError(e.to_string()));
                    held = sig.map(|s| (s, p.data().to_vec()));
                    sent
                }),
        };
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
    let mut bytes = writer.finish().map_err(|e| ToolError(e.to_string()))?;
    if !audio.is_empty() {
        let mixed = od_media::add_audio(ffmpeg, &picture_path, &audio, &final_path);
        let _ = std::fs::remove_file(&picture_path);
        mixed.map_err(|e| ToolError(e.to_string()))?;
        bytes = std::fs::metadata(&final_path)
            .map(|m| m.len())
            .map_err(|e| ToolError(e.to_string()))?;
    }
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
        audio_pieces: audio.len(),
        warnings: warnings
            .into_iter()
            .chain(renderer.warnings().iter().map(ToString::to_string))
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

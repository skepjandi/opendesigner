//! Videos out of the project: one picture of a video at a moment, and the whole video as an MP4
//! in `<project>/exports/` (`docs/wiki/video-editing.md`). Every frame is drawn by `render`;
//! `ffmpeg` only compresses them (see `od_media`).

use od_core::{Document, NodeId, Workspace};
use od_media::{
    AudioPiece, DuckSettings, EncodeSettings, Ffmpeg, FootageDecoder, PieceRole, VideoWriter,
};
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

    /// A source with no project folder, for a document that exists only in memory.
    pub fn of_document(doc: Document) -> Source {
        Source { doc, dir: None }
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

/// Whether drawing `v` needs ffmpeg to decode recordings: footage clips, or device screens that
/// play one.
pub(crate) fn has_footage(doc: &Document, v: &od_core::Video) -> bool {
    od_render::video::needs_footage(doc, v)
}

/// A still of `frame`. A device screen that plays a recording shows the picture at its in point
/// when ffmpeg can be found; otherwise the renderer warns and shows the node's image.
pub fn render_still(
    doc: &Document,
    frame: &NodeId,
    scale: f32,
    dir: Option<&std::path::Path>,
) -> Result<od_render::Rendered, od_render::RenderError> {
    let plays = doc
        .node(frame)
        .is_some_and(od_render::video::plays_recording);
    let mut decoder = match (plays, dir) {
        (true, Some(dir)) => Ffmpeg::locate()
            .ok()
            .map(|ffmpeg| Decoder(FootageDecoder::new(ffmpeg, dir.to_path_buf(), 30))),
        _ => None,
    };
    let mut media = od_render::Media {
        footage: decoder.as_mut().map(|d| d as &mut dyn FootageSource),
        played_ms: 0,
    };
    od_render::render_frame_with(doc, frame, scale, dir, &mut media)
}

/// A contact sheet: the PNG, its width and height, the grid's columns, and what could not be
/// drawn.
pub struct ContactSheet {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub warnings: Vec<String>,
}

/// The pictures of `video` at `times`, left to right and top to bottom in a grid as close to
/// square as the count allows, each at most `cell_side` on its longest side.
pub fn contact_sheet_png(
    ws: &Source,
    video: &NodeId,
    times: &[u64],
    cell_side: u32,
    locale: Option<&str>,
) -> Result<ContactSheet, ToolError> {
    let doc = document_in(ws, locale)?;
    let v = doc
        .video(video)
        .ok_or_else(|| ToolError(format!("video {video} does not exist")))?;
    let shrink = (f64::from(cell_side.max(16)) / f64::from(v.width.max(v.height))).min(1.0);
    let (cw, ch) = (
        ((f64::from(v.width) * shrink).round() as u32).max(1),
        ((f64::from(v.height) * shrink).round() as u32).max(1),
    );
    let mut source = if has_footage(&doc, v) {
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
    let mut renderer = VideoRenderer::new(&doc, v, ws.dir()).with_size(cw, ch);
    if let Some(s) = source.as_mut() {
        renderer = renderer.with_footage(s);
    }
    let pictures = times
        .iter()
        .map(|&t| renderer.frame_at(t))
        .collect::<Result<Vec<_>, _>>()?;
    let cols = f64::from(pictures.len().max(1) as u32).sqrt().ceil() as u32;
    let sheet = od_render::grid(&pictures, cols)
        .ok_or_else(|| ToolError("the contact sheet is too large to draw".into()))?;
    let png = od_render::encode_png(&sheet, false)?;
    let warnings = renderer
        .warnings()
        .iter()
        .map(ToString::to_string)
        .collect();
    Ok(ContactSheet {
        png,
        width: sheet.width(),
        height: sheet.height(),
        columns: cols,
        warnings,
    })
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
    let mut source = if has_footage(&doc, v) {
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
    export_video_with(
        ws,
        video,
        file,
        locale,
        preset_id,
        ffmpeg,
        od_render::export::default_workers(),
        progress,
    )
}

/// [`export_video`] drawing frames on `workers` threads (`od_render::export`); the file does not
/// depend on the number.
#[allow(clippy::too_many_arguments)]
pub fn export_video_with(
    ws: &Source,
    video: &NodeId,
    file: &str,
    locale: Option<&str>,
    preset_id: Option<&str>,
    ffmpeg: &Ffmpeg,
    workers: usize,
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
    let mut temporary = TempFiles::default();
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
        let needs = f.in_ms + clip.source_span_ms();
        if needs > info.duration_ms + 100 {
            return fail(format!(
                "clip {} plays {} to {} ms of {}, which is only {} ms long; shorten the clip, slow it down or move its in point",
                clip.id, f.in_ms, needs, f.file, info.duration_ms
            ));
        }
        if info.has_audio {
            audio.push(AudioPiece {
                file: path,
                role: PieceRole::Footage,
                in_ms: f.in_ms,
                start_ms: placed.start_ms,
                duration_ms: u64::from(clip.duration_ms),
                gain: 1.0,
                fade_in_ms: 0,
                fade_out_ms: 0,
                pan: 0.0,
                duck: None,
            });
            let piece = if clip.retimed() {
                let wav = dir.join(format!(".{file}.{}.retimed.wav", clip.id));
                retime_clip_sound(ffmpeg, clip, &path, f.in_ms, &wav)?;
                temporary.0.push(wav.clone());
                AudioPiece {
                    file: wav,
                    in_ms: 0,
                    ..footage_piece(path, 0, placed.start_ms, u64::from(clip.duration_ms))
                }
            } else {
                footage_piece(path, f.in_ms, placed.start_ms, u64::from(clip.duration_ms))
            };
            audio.push(piece);
        }
    }
    // The sound tracks of the video: played under the picture, cut at the end of the video.
    let mut warnings: Vec<String> = Vec::new();
    for track in v.sound_tracks() {
        let project = project
            .as_deref()
            .ok_or_else(|| ToolError("the project has no folder, so no sound track".into()))?;
        let path =
            od_media::audio_path(project, &track.file).map_err(|e| ToolError(e.to_string()))?;
        let length = ffmpeg
            .probe_duration(&path)
            .map_err(|e| ToolError(e.to_string()))?;
        let start = u64::from(track.start_ms);
        let room = v.duration_ms().saturating_sub(start);
        let played = track.played_ms(length, room);
        if played == 0 {
            warnings.push(format!(
                "the sound track {} ({}) is not in the file: it starts after the video ends or its trim is empty",
                track.id, track.file
            ));
        } else {
            audio.push(piece_of(&track, path, start, played));
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
    let mut writer = VideoWriter::start(ffmpeg, &settings).map_err(|e| ToolError(e.to_string()))?;
    let total = v.frame_count();
    let footage = has_footage(&doc, v);
    let new_source = || -> Option<Box<dyn FootageSource + Send>> {
        let dir = project.as_ref().filter(|_| footage)?;
        let decoder = FootageDecoder::new(ffmpeg.clone(), dir.clone(), v.fps);
        Some(Box::new(Decoder(decoder)))
    };
    let drawn = od_render::export::export_frames(
        &doc,
        v,
        ws.dir(),
        &new_source,
        workers,
        &mut |n, pixels| {
            writer.write_frame(pixels).map_err(|e| e.to_string())?;
            Ok(progress(n + 1, total))
        },
    );
    let drawn = match drawn {
        Ok(d) if !d.stopped => d,
        Ok(_) => {
            writer.abort();
            return fail("the export was cancelled");
        }
        Err(e) => {
            writer.abort();
            return Err(ToolError(e.to_string()));
        }
    };
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
            .chain(drawn.warnings.iter().map(ToString::to_string))
            .collect(),
    })
}

fn piece_of(
    track: &od_core::SoundTrack,
    file: std::path::PathBuf,
    start_ms: u64,
    duration_ms: u64,
) -> AudioPiece {
    AudioPiece {
        file,
        role: match track.role {
            od_core::TrackRole::Music => PieceRole::Music,
            od_core::TrackRole::Voice => PieceRole::Voice,
            od_core::TrackRole::Sfx => PieceRole::Sfx,
        },
        in_ms: u64::from(track.in_ms),
        start_ms,
        duration_ms,
        gain: track.gain,
        fade_in_ms: u64::from(track.fade_in_ms),
        fade_out_ms: u64::from(track.fade_out_ms),
        pan: track.pan,
        duck: track.duck.map(|d| DuckSettings {
            depth: d.depth,
            attack_ms: d.attack_ms,
            release_ms: d.release_ms,
        }),
    }
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

/// Files written next to the export that are removed when the export ends, however it ends.
#[derive(Default)]
struct TempFiles(Vec<std::path::PathBuf>);

impl Drop for TempFiles {
    fn drop(&mut self) {
        for file in &self.0 {
            let _ = std::fs::remove_file(file);
        }
    }
}

fn footage_piece(
    file: std::path::PathBuf,
    in_ms: u64,
    start_ms: u64,
    duration_ms: u64,
) -> AudioPiece {
    AudioPiece {
        file,
        in_ms,
        start_ms,
        duration_ms,
        gain: 1.0,
        fade_in_ms: 0,
        fade_out_ms: 0,
    }
}

/// How finely a speed ramp's sound is cut: one slice per this many milliseconds of clip time.
const RAMP_SLICE_MS: u64 = 250;

/// Writes the sound of a retimed clip: its stretch of the file squeezed or stretched to the
/// clip's length on the timeline (`od_media::retime`).
fn retime_clip_sound(
    ffmpeg: &Ffmpeg,
    clip: &od_core::Clip,
    source: &std::path::Path,
    in_ms: u64,
    output: &std::path::Path,
) -> Result<(), ToolError> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ToolError(format!("{}: {e}", parent.display())))?;
    }
    let map = od_core::speed::SpeedMap::new(clip);
    let length = u64::from(clip.duration_ms);
    let step = if clip.speed_ramp.is_empty() {
        length
    } else {
        RAMP_SLICE_MS
    };
    let slices: Vec<od_media::retime::Slice> = (0..length)
        .step_by(step as usize)
        .map(|from| {
            let to = (from + step).min(length);
            let (a, b) = (map.source_ms(from), map.source_ms(to));
            od_media::retime::Slice {
                source_start_ms: in_ms + a,
                source_ms: (b - a).max(1),
                timeline_ms: to - from,
            }
        })
        .collect();
    od_media::retime::retime_audio(ffmpeg, source, &slices, output)
        .map_err(|e| ToolError(e.to_string()))
}

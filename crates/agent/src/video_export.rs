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

/// What an export can write besides the video itself.
#[derive(Debug, Clone, Default)]
pub struct VideoOptions {
    /// Start of the part exported, in milliseconds of the video (default 0).
    pub from_ms: Option<u64>,
    /// End of the part exported (default: the end of the video).
    pub to_ms: Option<u64>,
    /// Write every Nth frame (default 1): a GIF at 15 fps from a 30 fps video is 2.
    pub every_nth_frame: Option<u32>,
    /// Keep what no clip covers transparent (`.webm`, `.mov` and `.gif` only).
    pub transparent: bool,
    /// Scale to this width while encoding (`.gif`, `.webm`, `.mov`; a GIF defaults to 480).
    pub width: Option<u32>,
    /// A GIF plays again this many times after the first: none for ever, 0 once.
    pub gif_loops: Option<u32>,
}

const MAX_EVERY_NTH: u32 = 60;
const DEFAULT_GIF_WIDTH: u32 = 480;

/// File names only, ending in `.mp4`, `.gif`, `.webm` or `.mov`: `preview.mp4`. Returns the name
/// and the kind of file it asks for.
pub(crate) fn export_file_name(file: &str) -> Result<(&str, od_media::Container), ToolError> {
    let container = od_media::Container::of_file(file);
    let stem_ok = file.rsplit_once('.').is_some_and(|(stem, _)| {
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
    match container {
        Some(c) if stem_ok => Ok((file, c)),
        _ => fail(
            "file must be a plain name like preview.mp4, loop.gif, overlay.webm or overlay.mov (letters, digits . _ -)",
        ),
    }
}

/// The document as it reads in `locale`, or as it is.
pub(crate) fn document_in<'a>(
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

#[derive(Debug)]
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
    /// The file's kind (`mp4`, `gif`, `webm` or `mov`).
    pub format: &'static str,
    /// What no clip covers is transparent in the file.
    pub transparent: bool,
    /// The part of the video written, in milliseconds, and the step between frames.
    pub from_ms: u64,
    pub to_ms: u64,
    pub every_nth_frame: u32,
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
    let (_, container) = export_file_name(file)?;
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
    if preset.is_some() && container != od_media::Container::Mp4 {
        return fail(
            "store presets describe .mp4 files: leave `preset` out for a .gif, .webm or .mov",
        );
    }
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
    export_video_mastered(
        ws, video, file, locale, preset_id, ffmpeg, workers, None, progress,
    )
}

/// [`export_video_with`] bringing the final mix to a `loudness` target when there is sound
/// (`od_media::loudness`); a video with no sound is exported as it is.
#[allow(clippy::too_many_arguments)]
pub fn export_video_mastered(
    ws: &Source,
    video: &NodeId,
    file: &str,
    locale: Option<&str>,
    preset_id: Option<&str>,
    ffmpeg: &Ffmpeg,
    workers: usize,
    loudness: Option<od_media::Loudness>,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<VideoOutcome, ToolError> {
    export_video_custom(
        ws,
        video,
        file,
        locale,
        preset_id,
        ffmpeg,
        workers,
        loudness,
        &VideoOptions::default(),
        progress,
    )
}

/// [`export_video_mastered`] for a part of the video (`from_ms` to `to_ms`, every Nth frame) and
/// for the other kinds of file (`.gif`, `.webm`, `.mov`, with transparency when asked). A
/// store preset is checked against the whole `.mp4` only.
#[allow(clippy::too_many_arguments)]
pub fn export_video_custom(
    ws: &Source,
    video: &NodeId,
    file: &str,
    locale: Option<&str>,
    preset_id: Option<&str>,
    ffmpeg: &Ffmpeg,
    workers: usize,
    loudness: Option<od_media::Loudness>,
    options: &VideoOptions,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<VideoOutcome, ToolError> {
    let preset = check_export(ws, video, file, locale, preset_id)?;
    let (file, container) = export_file_name(file)?;
    let dir = exports_dir(ws)?;
    let doc = document_in(ws, locale)?;
    let v = doc
        .video(video)
        .ok_or_else(|| ToolError(format!("video {video} does not exist")))?;
    let sel = select(v, container, options, preset.is_some())?;
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
    let had_sound = !audio.is_empty();
    if !container.has_sound() {
        audio.clear();
        if had_sound {
            warnings.push(format!(
                "a .{} carries no sound here: the video's sound was left out; export an .mp4 for it",
                od_media::Container::EXTENSIONS[container as usize]
            ));
        }
    } else if sel.every_nth > 1 {
        audio.clear();
        if had_sound {
            warnings.push(
                "every_nth_frame changes the speed of the picture, so the sound was left out"
                    .into(),
            );
        }
    } else if sel.is_part {
        audio = window_pieces(audio, sel.from_ms, sel.to_ms);
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
        container,
        every_nth: sel.every_nth,
        alpha: options.transparent,
        scale_width: sel.scale_width,
        gif_loops: options.gif_loops,
    };
    let mut writer = VideoWriter::start(ffmpeg, &settings).map_err(|e| ToolError(e.to_string()))?;
    let total = sel.frames;
    let footage = has_footage(&doc, v);
    let new_source = || -> Option<Box<dyn FootageSource + Send>> {
        let dir = project.as_ref().filter(|_| footage)?;
        let decoder = FootageDecoder::new(ffmpeg.clone(), dir.clone(), v.fps);
        Some(Box::new(Decoder(decoder)))
    };
    let frame_options = od_render::export::ExportOptions {
        start_frame: sel.start_frame,
        end_frame: Some(sel.end_frame),
        every_nth: u64::from(sel.every_nth),
        transparent: options.transparent,
    };
    let mut written = 0u64;
    let drawn = od_render::export::export_frames_with(
        &doc,
        v,
        ws.dir(),
        &new_source,
        workers,
        &frame_options,
        &mut |_, pixels| {
            let sent = if options.transparent {
                writer.write_frame(&od_media::container::unpremultiply(pixels))
            } else {
                writer.write_frame(pixels)
            };
            sent.map_err(|e| e.to_string())?;
            written += 1;
            Ok(progress(written, total))
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
        let mixed = od_media::add_audio(ffmpeg, &picture_path, &audio, loudness, &final_path);
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
            duration_ms: sel.duration_ms,
            bytes,
        });
        if !broken.is_empty() {
            let _ = std::fs::remove_file(dir.join(file));
            return fail(violations_message(p, &broken));
        }
    }
    let (out_width, out_height) = od_media::container::output_size(&settings);
    Ok(VideoOutcome {
        path: format!("exports/{file}"),
        width: out_width,
        height: out_height,
        fps: v.fps,
        frames: total,
        duration_ms: sel.duration_ms,
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
        format: od_media::Container::EXTENSIONS[container as usize],
        transparent: options.transparent,
        from_ms: sel.from_ms,
        to_ms: sel.to_ms,
        every_nth_frame: sel.every_nth,
    })
}

/// What an export writes, worked out from the options and checked against the file and video.
struct Selection {
    start_frame: u64,
    end_frame: u64,
    every_nth: u32,
    /// How many frames are written.
    frames: u64,
    /// The span of the video those frames show, in milliseconds.
    from_ms: u64,
    to_ms: u64,
    duration_ms: u64,
    /// Not the whole video.
    is_part: bool,
    scale_width: Option<u32>,
}

fn select(
    v: &od_core::Video,
    container: od_media::Container,
    o: &VideoOptions,
    has_preset: bool,
) -> Result<Selection, ToolError> {
    let every_nth = o.every_nth_frame.unwrap_or(1);
    if !(1..=MAX_EVERY_NTH).contains(&every_nth) {
        return fail(format!(
            "every_nth_frame is 1 to {MAX_EVERY_NTH}, not {every_nth}"
        ));
    }
    let total_ms = v.duration_ms();
    let (from, to) = (
        o.from_ms.unwrap_or(0),
        o.to_ms.unwrap_or(total_ms).min(total_ms),
    );
    if from >= to {
        return fail(format!(
            "the range {from} to {to} ms is empty; the video is {total_ms} ms long"
        ));
    }
    let rate = u64::from(v.fps);
    let start_frame = (from * rate).div_ceil(1000);
    let end_frame = ((to * rate).div_ceil(1000)).min(v.frame_count());
    let picked = od_render::export::ExportOptions {
        start_frame,
        end_frame: Some(end_frame),
        every_nth: u64::from(every_nth),
        transparent: false,
    }
    .frames(v.frame_count())
    .len() as u64;
    if picked == 0 {
        return fail("that range holds no frame");
    }
    let is_part = start_frame > 0 || end_frame < v.frame_count() || every_nth > 1;
    if has_preset && is_part {
        return fail(
            "a store preset checks the whole video: leave out from_ms, to_ms and every_nth_frame",
        );
    }
    if o.transparent && !container.has_alpha() {
        return fail("an .mp4 cannot hold transparency: write a .webm, .mov or .gif");
    }
    if o.gif_loops.is_some() && container != od_media::Container::Gif {
        return fail("gif_loops is for a .gif");
    }
    let scale_width = match (o.width, container) {
        (Some(_), od_media::Container::Mp4) => {
            return fail(
                "width is for a .gif, .webm or .mov; an .mp4 is written at the video's size",
            );
        }
        (Some(w), _) if !(16..=v.width).contains(&w) => {
            return fail(format!(
                "width is 16 to {} (the video's width), not {w}",
                v.width
            ));
        }
        (Some(w), _) => Some(w),
        (None, od_media::Container::Gif) => Some(DEFAULT_GIF_WIDTH.min(v.width)),
        (None, _) => None,
    };
    if container == od_media::Container::Gif {
        let probe = EncodeSettings {
            width: v.width,
            height: v.height,
            scale_width,
            ..EncodeSettings::default()
        };
        let (w, h) = od_media::container::output_size(&probe);
        let bytes = picked * u64::from(w) * u64::from(h) * 4;
        if bytes > od_media::container::MAX_GIF_BYTES {
            return fail(format!(
                "a GIF of {picked} frames at {w}x{h} is too large to build ({} MB of pictures); use a shorter range, a larger every_nth_frame or a smaller width",
                bytes >> 20
            ));
        }
    }
    let start_ms = od_core::timebase::frame_ms(start_frame, v.fps);
    let duration_ms = (picked * u64::from(every_nth) * 1000).div_ceil(rate);
    Ok(Selection {
        start_frame,
        end_frame,
        every_nth,
        frames: picked,
        from_ms: start_ms,
        to_ms: start_ms + duration_ms,
        duration_ms,
        is_part,
        scale_width,
    })
}

/// The pieces of sound as they play from `from_ms` to `to_ms` of the video: shifted to start at
/// zero, cut where they cross either end (a cut end has no fade), and left out when they lie
/// outside.
fn window_pieces(pieces: Vec<AudioPiece>, from_ms: u64, to_ms: u64) -> Vec<AudioPiece> {
    pieces
        .into_iter()
        .filter_map(|mut p| {
            let (start, end) = (p.start_ms, p.start_ms + p.duration_ms);
            let (a, b) = (start.max(from_ms), end.min(to_ms));
            if a >= b {
                return None;
            }
            if a > start {
                p.in_ms += a - start;
                p.fade_in_ms = 0;
            }
            if b < end {
                p.fade_out_ms = 0;
            }
            p.start_ms = a - from_ms;
            p.duration_ms = b - a;
            Some(p)
        })
        .collect()
}

pub(crate) fn piece_of(
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

pub(crate) fn footage_piece(
    file: std::path::PathBuf,
    in_ms: u64,
    start_ms: u64,
    duration_ms: u64,
) -> AudioPiece {
    AudioPiece {
        file,
        role: PieceRole::Footage,
        in_ms,
        start_ms,
        duration_ms,
        gain: 1.0,
        fade_in_ms: 0,
        fade_out_ms: 0,
        pan: 0.0,
        duck: None,
    }
}

/// How finely a speed ramp's sound is cut: one slice per this many milliseconds of clip time.
const RAMP_SLICE_MS: u64 = 250;

/// Writes the sound of a retimed clip: its stretch of the file squeezed or stretched to the
/// clip's length on the timeline (`od_media::retime`).
pub(crate) fn retime_clip_sound(
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

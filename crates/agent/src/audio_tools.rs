//! MCP tools for sound tracks, pauses, waveforms and captions
//! (`docs/wiki/agent-integration.md#sound-and-captions`).
//!
//! Every tool that changes the video computes a new value with the helpers in `od_core::audio`,
//! `od_core::captions` and `od_core::pauses` and applies it through `apply_ops`, so it is one
//! attributed, checkpointed, undoable transaction. Nothing here recognizes speech: a transcript
//! comes from a speech MCP server the user added, and arrives as SRT, WebVTT or timed words.

use std::path::Path;

use od_core::captions::{self, CaptionPreset, CaptionStyle};
use od_core::{
    Captions, NewCue, NewTrack, NodeId, Operation, SoundTrack, TrackPatch, TrackRole, Video,
    VideoProperty,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::tools::{
    ApplyArgs, CallCtx, Content, Output, ToolError, apply_ops, node_id, parse, tool,
};

pub const NAMES: [&str; 12] = [
    "add_sound_track",
    "set_sound_track",
    "remove_sound_track",
    "detect_pauses",
    "remove_pauses",
    "get_waveform",
    "import_captions",
    "captions_from_script",
    "add_caption_cue",
    "remove_caption_cue",
    "clear_captions",
    "set_caption_style",
];

const MAX_CAPTION_SOURCE_BYTES: usize = 2_000_000;
const MAX_SCRIPT_CHARS: usize = 100_000;
const DEFAULT_BUCKETS: usize = 400;
const DEFAULT_NOISE_DB: f64 = -30.0;
const DEFAULT_PAUSE_MS: u64 = 500;
const DEFAULT_CUT_PAUSE_MS: u64 = 400;
const DEFAULT_KEEP_MS: u64 = 120;
const MAX_KEEP_MS: u64 = 1_000;

fn id(d: &str) -> Value {
    json!({"type": "string", "description": d})
}

fn turn_props(mut props: Value) -> Value {
    if let Some(m) = props.as_object_mut() {
        m.insert(
            "turn".into(),
            id("Groups related calls into one turn; omit and each call is its own turn."),
        );
        m.insert(
            "summary".into(),
            id("One line, in the user's terms, naming what this turn does."),
        );
    }
    props
}

fn track_props() -> Value {
    json!({
        "role": {"type": "string", "enum": TrackRole::NAMES, "description": "music (default; may duck), voice (a voice-over: music ducks under it) or sfx."},
        "file": id("An audio file in media/ from list_sounds (media/<name>.mp3|wav|m4a|aac|ogg)."),
        "start_ms": {"type": "integer", "minimum": 0, "maximum": 600000, "description": "When it starts, from the start of the video."},
        "in_ms": {"type": "integer", "minimum": 0, "description": "Trim in: where in the file it starts playing."},
        "out_ms": {"type": ["integer", "null"], "minimum": 1, "description": "Trim out: where in the file it stops (null or omitted plays to the end)."},
        "gain": {"type": "number", "minimum": 0, "maximum": 2, "description": "1 is as recorded. A music bed under a voice: 0.15 to 0.3."},
        "fade_in_ms": {"type": "integer", "minimum": 0, "maximum": 600000},
        "fade_out_ms": {"type": "integer", "minimum": 0, "maximum": 600000},
        "pan": {"type": "number", "minimum": -1, "maximum": 1, "description": "-1 all left, 1 all right."},
        "duck": {"type": ["object", "null"], "description": "Music only: lower under the voice tracks. {depth 0-1 (default 0.7), attack_ms (default 40), release_ms (default 500)}; null turns it off.",
                 "properties": {"depth": {"type": "number"}, "attack_ms": {"type": "integer"}, "release_ms": {"type": "integer"}}, "additionalProperties": false}
    })
}

pub fn definitions() -> Vec<Value> {
    let video = id("Id of a video from list_videos.");
    let style_props = json!({
        "preset": {"type": "string", "enum": CaptionPreset::ALL.map(CaptionPreset::name), "description": "Start from a named look: clean, bold_pop (big, uppercase, yellow active word), karaoke (mid-screen, green active word), boxed (dark box)."},
        "font_family": id("A font family; unknown ones fall back to Lato."),
        "size_pct": {"type": "number", "minimum": 1, "maximum": 20, "description": "Text size as a percent of the video's height."},
        "font_weight": {"type": "integer", "minimum": 100, "maximum": 900},
        "color": id("#rrggbb or #rrggbbaa."),
        "highlight": {"type": ["string", "null"], "description": "Colour of the word being spoken (needs word timings, or the estimate by word length); null removes it."},
        "background": {"type": ["string", "null"], "description": "A box behind each line, #rrggbbaa; null removes it."},
        "position": {"type": "string", "enum": ["top", "middle", "bottom"]},
        "align": {"type": "string", "enum": ["left", "center", "right"]},
        "max_lines": {"type": "integer", "minimum": 1, "maximum": 4},
        "uppercase": {"type": "boolean"}
    });
    vec![
        tool(
            "add_sound_track",
            "Add a sound track to a video, up to 8 besides the older single `audio`: music, a \
             voice-over or effects, each with its own start, trim (in_ms/out_ms), gain, fades, pan \
             and, for music, `duck`: the music lowers while a voice track speaks (a side-chain \
             compressor in the export's one ffmpeg pass; the picture is copied, not re-encoded; \
             footage sound is still mixed). Put a voice-over in with role voice, then music with \
             `duck: {}` and gain 0.3 to 0.6. Returns the track id for set_sound_track and \
             remove_sound_track. Speech is not made here: for a voice-over use a speech MCP server \
             the user added (get_guidance lists them) and import the file with import_file.",
            turn_props(merge(json!({"video": video}), track_props())),
            &["video", "file"],
            false,
        ),
        tool(
            "set_sound_track",
            "Change one sound track of a video: only the fields you give change; `out_ms: null` and \
             `duck: null` clear them. The older single track (id `audio`) is changed with set_video audio.",
            turn_props(merge(
                json!({"video": video, "track": id("A track id from list_videos (a1, a2, ...).")}),
                track_props(),
            )),
            &["video", "track"],
            false,
        ),
        tool(
            "remove_sound_track",
            "Remove one sound track from a video.",
            turn_props(json!({"video": video, "track": id("A track id from list_videos.")})),
            &["video", "track"],
            false,
        ),
        tool(
            "detect_pauses",
            "Find the pauses in the sound of a file in media/ (audio or footage): stretches at least \
             `min_ms` long quieter than `noise_db`, by ffmpeg's silencedetect. Returns the pauses and \
             the stretches of sound between them, in the file's own time. Needs ffmpeg. Use it to \
             plan a cut with remove_pauses or to time captions to speech.",
            json!({
                "file": id("A media/ file: audio or footage."),
                "noise_db": {"type": "number", "minimum": -90, "maximum": -10, "description": "Quieter than this is a pause (default -30)."},
                "min_ms": {"type": "integer", "minimum": 100, "maximum": 10000, "description": "A pause is at least this long (default 500)."}
            }),
            &["file"],
            true,
        ),
        tool(
            "remove_pauses",
            "Cut the pauses out of a footage clip: the clip becomes several clips that play only the \
             stretches with sound, one after the other, in one undoable step (a zoom on the clip carries \
             on across the cuts). `keep_ms` of each pause is left at both ends so speech is not clipped. \
             The video gets shorter; overlays, captions and sound tracks keep their times, so move them \
             after, and read the new story in list_videos. Needs ffmpeg.",
            turn_props(json!({
                "video": video,
                "clip": id("A footage clip id from list_videos."),
                "noise_db": {"type": "number", "minimum": -90, "maximum": -10, "description": "Default -30."},
                "min_ms": {"type": "integer", "minimum": 100, "maximum": 10000, "description": "Shortest pause to cut (default 400)."},
                "keep_ms": {"type": "integer", "minimum": 0, "maximum": MAX_KEEP_MS, "description": "Pause kept at each end (default 120)."}
            })),
            &["video", "clip"],
            false,
        ),
        tool(
            "get_waveform",
            "Downsampled peaks (0 to 1) of the sound in a media/ file (audio or footage), decoded by ffmpeg: \
             `buckets` numbers from 50 to 2000 across the file's whole length. The window's Sound tracks draw \
             the same. Needs ffmpeg.",
            json!({
                "file": id("A media/ file: audio or footage."),
                "buckets": {"type": "integer", "minimum": od_media::analysis::MIN_BUCKETS, "maximum": od_media::analysis::MAX_BUCKETS, "description": "Default 400."}
            }),
            &["file"],
            true,
        ),
        tool(
            "import_captions",
            "Set a video's captions from a transcript the user (or their speech MCP server) produced. \
             format `srt` or `vtt` (WebVTT inline <00:00:01.500> word times become word timings), or \
             `words_json`: a list of {text|word, start_ms|start (seconds), end_ms|end}, which is what a \
             speech-to-text server gives, grouped into cues at sentence ends and pauses. This tool does \
             not recognize speech: OpenDesigner has no speech-to-text built in, so a transcript must come \
             from the user's own speech server. Replaces every cue; keeps the style. `offset_ms` shifts \
             all times; cues outside the video are dropped and counted. Captions are drawn in the preview \
             and burned into export_video.",
            turn_props(json!({
                "video": video,
                "format": {"type": "string", "enum": ["srt", "vtt", "words_json"]},
                "text": id("The file's text (at most 2 MB)."),
                "offset_ms": {"type": "integer", "minimum": -600000, "maximum": 600000}
            })),
            &["video", "format", "text"],
            false,
        ),
        tool(
            "captions_from_script",
            "Set captions from a plain script spread over a time range in proportion to word length. \
             This is an ESTIMATE, not speech recognition: it knows nothing of how the words are spoken, so \
             it fits a voice-over only roughly; prefer import_captions with real timings when there is a \
             transcript. Cues break after sentence ends and at `words_per_cue`. Replaces every cue; keeps the style.",
            turn_props(json!({
                "video": video,
                "script": id("The words to show."),
                "start_ms": {"type": "integer", "minimum": 0, "description": "Default 0."},
                "end_ms": {"type": "integer", "minimum": 1, "description": "Default the end of the video."},
                "words_per_cue": {"type": "integer", "minimum": 1, "maximum": 80, "description": "Default 7."}
            })),
            &["video", "script"],
            false,
        ),
        tool(
            "add_caption_cue",
            "Add one caption cue. Cues are in time order and may not overlap; they end inside the video. \
             Give `words` ([{text, start_ms, end_ms}] spelling the text) to highlight the spoken word.",
            turn_props(json!({
                "video": video,
                "start_ms": {"type": "integer", "minimum": 0},
                "end_ms": {"type": "integer", "minimum": 1},
                "text": id("At most 400 characters."),
                "words": {"type": "array", "items": {"type": "object", "properties": {"text": {"type": "string"}, "start_ms": {"type": "integer"}, "end_ms": {"type": "integer"}}, "required": ["text", "start_ms", "end_ms"], "additionalProperties": false}}
            })),
            &["video", "start_ms", "end_ms", "text"],
            false,
        ),
        tool(
            "remove_caption_cue",
            "Remove one caption cue (ids are in list_videos under captions.cues).",
            turn_props(json!({"video": video, "cue": id("A cue id (q1, q2, ...).")})),
            &["video", "cue"],
            false,
        ),
        tool(
            "clear_captions",
            "Remove every caption cue of a video (the style stays).",
            turn_props(json!({"video": video})),
            &["video"],
            false,
        ),
        tool(
            "set_caption_style",
            "Set how captions look: start from a `preset` and/or change single fields; fields you leave \
             out keep their value (`highlight: null` and `background: null` remove them). Sizes are a \
             percent of the video's height so one style fits every output size.",
            turn_props(merge(json!({"video": video}), style_props)),
            &["video"],
            false,
        ),
    ]
}

fn merge(mut a: Value, b: Value) -> Value {
    if let (Some(a), Value::Object(b)) = (a.as_object_mut(), b) {
        a.extend(b);
    }
    a
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "add_sound_track" => add_sound_track(ctx, args),
        "set_sound_track" => set_sound_track(ctx, args),
        "remove_sound_track" => remove_sound_track(ctx, parse(args)?),
        "detect_pauses" => detect_pauses(ctx, parse(args)?),
        "remove_pauses" => remove_pauses(ctx, parse(args)?),
        "get_waveform" => get_waveform(ctx, parse(args)?),
        "import_captions" => import_captions(ctx, parse(args)?),
        "captions_from_script" => captions_from_script(ctx, parse(args)?),
        "add_caption_cue" => add_caption_cue(ctx, args),
        "remove_caption_cue" => remove_caption_cue(ctx, parse(args)?),
        "clear_captions" => clear_captions(ctx, parse(args)?),
        "set_caption_style" => set_caption_style(ctx, args),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

/// The arguments every changing tool takes besides its own.
#[derive(Deserialize)]
struct Common {
    video: String,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// Splits the arguments the tool does not own (`video`, `turn`, `summary`) from the rest.
fn split(args: Value) -> Result<(Common, Map<String, Value>), ToolError> {
    let Value::Object(mut map) = args else {
        return Err(ToolError("invalid arguments: expected an object".into()));
    };
    let mut common = Map::new();
    for key in ["video", "turn", "summary"] {
        if let Some(v) = map.remove(key) {
            common.insert(key.into(), v);
        }
    }
    Ok((parse(Value::Object(common))?, map))
}

fn video_of<'a>(ctx: &'a CallCtx, id: &NodeId) -> Result<&'a Video, ToolError> {
    ctx.ws
        .session
        .document()
        .video(id)
        .ok_or_else(|| ToolError(format!("video {id} does not exist")))
}

/// Applies `ops` as one turn and adds `extra` fields to the receipt.
fn applied(
    ctx: &mut CallCtx,
    common: &Common,
    ops: Vec<Operation>,
    extra: Value,
) -> Result<Output, ToolError> {
    let out = apply_ops(
        ctx,
        ApplyArgs {
            ops,
            turn: common.turn.clone(),
            summary: common.summary.clone(),
        },
    )?;
    let mut receipt: Value = match out.content.first() {
        Some(Content::Text(t)) => serde_json::from_str(t).unwrap_or(Value::Null),
        _ => Value::Null,
    };
    if let (Some(r), Value::Object(e)) = (receipt.as_object_mut(), extra) {
        r.extend(e);
    }
    Ok(Output::json(&receipt))
}

fn set_video(video: NodeId, prop: VideoProperty) -> Vec<Operation> {
    vec![Operation::SetVideo { id: video, prop }]
}

fn project_dir(ctx: &CallCtx) -> Result<std::path::PathBuf, ToolError> {
    ctx.ws
        .dir()
        .map(Path::to_path_buf)
        .ok_or_else(|| ToolError("the project has no folder, so no media/".into()))
}

/// A `media/` reference to audio or footage, as a path inside the project.
fn media_file(dir: &Path, reference: &str) -> Result<std::path::PathBuf, ToolError> {
    if od_core::media::audio_name(reference).is_ok() {
        od_media::audio_path(dir, reference).map_err(|e| ToolError(e.to_string()))
    } else if od_core::media::footage_name(reference).is_ok() {
        od_media::footage_path(dir, reference).map_err(|e| ToolError(e.to_string()))
    } else {
        Err(ToolError(format!(
            "{reference:?} is not an audio or footage file in media/ (.mp3 .wav .m4a .aac .ogg .mp4 .mov .m4v)"
        )))
    }
}

fn ffmpeg() -> Result<od_media::Ffmpeg, ToolError> {
    od_media::Ffmpeg::locate().map_err(|e| ToolError(e.to_string()))
}

fn check_track_file(ctx: &CallCtx, file: &str) -> Result<(), ToolError> {
    od_core::media::audio_name(file)?;
    od_media::audio_path(&project_dir(ctx)?, file).map_err(|e| ToolError(e.to_string()))?;
    Ok(())
}

fn late_warning(video: &Video, track: &SoundTrack) -> Vec<String> {
    if u64::from(track.start_ms) >= video.duration_ms() {
        vec![format!(
            "track {} starts at {} ms, after the video ends at {} ms, so it is not heard",
            track.id,
            track.start_ms,
            video.duration_ms()
        )]
    } else {
        Vec::new()
    }
}

fn add_sound_track(ctx: &mut CallCtx, args: Value) -> Result<Output, ToolError> {
    let (common, rest) = split(args)?;
    let new: NewTrack = parse(Value::Object(rest))?;
    check_track_file(ctx, &new.file)?;
    let video = node_id(&common.video)?;
    let (tracks, track, warnings) = {
        let v = video_of(ctx, &video)?;
        let (tracks, track) = v.tracks_with_added(new)?;
        let warnings = tracks
            .iter()
            .find(|t| t.id == track)
            .map_or_else(Vec::new, |t| late_warning(v, t));
        (tracks, track, warnings)
    };
    applied(
        ctx,
        &common,
        set_video(video, VideoProperty::Tracks(tracks)),
        json!({"track": track, "warnings": warnings}),
    )
}

fn set_sound_track(ctx: &mut CallCtx, args: Value) -> Result<Output, ToolError> {
    let (common, mut rest) = split(args)?;
    let track = match rest.remove("track") {
        Some(Value::String(t)) => node_id(&t)?,
        _ => return Err(ToolError("invalid arguments: `track` is required".into())),
    };
    let patch: TrackPatch = parse(Value::Object(rest))?;
    if let Some(file) = &patch.file {
        check_track_file(ctx, file)?;
    }
    let video = node_id(&common.video)?;
    let (tracks, warnings) = {
        let v = video_of(ctx, &video)?;
        let tracks = v.tracks_with_changed(&track, &patch)?;
        let warnings = tracks
            .iter()
            .find(|t| t.id == track)
            .map_or_else(Vec::new, |t| late_warning(v, t));
        (tracks, warnings)
    };
    applied(
        ctx,
        &common,
        set_video(video, VideoProperty::Tracks(tracks)),
        json!({"track": track, "warnings": warnings}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveTrackArgs {
    video: String,
    track: String,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

fn remove_sound_track(ctx: &mut CallCtx, a: RemoveTrackArgs) -> Result<Output, ToolError> {
    let video = node_id(&a.video)?;
    let track = node_id(&a.track)?;
    let tracks = video_of(ctx, &video)?.tracks_with_removed(&track)?;
    let common = Common {
        video: a.video,
        turn: a.turn,
        summary: a.summary,
    };
    applied(
        ctx,
        &common,
        set_video(video, VideoProperty::Tracks(tracks)),
        json!({"removed": track}),
    )
}

// ---- pauses and waveforms ----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PausesArgs {
    file: String,
    #[serde(default)]
    noise_db: Option<f64>,
    #[serde(default)]
    min_ms: Option<u64>,
}

fn noise_db(v: Option<f64>) -> Result<f64, ToolError> {
    let db = v.unwrap_or(DEFAULT_NOISE_DB);
    if db.is_finite() && (-90.0..=-10.0).contains(&db) {
        Ok(db)
    } else {
        Err(ToolError(format!("noise_db is from -90 to -10, not {db}")))
    }
}

fn min_pause_ms(v: Option<u64>, default: u64) -> Result<u64, ToolError> {
    let ms = v.unwrap_or(default);
    if (100..=10_000).contains(&ms) {
        Ok(ms)
    } else {
        Err(ToolError(format!("min_ms is from 100 to 10000, not {ms}")))
    }
}

fn detect_pauses(ctx: &mut CallCtx, a: PausesArgs) -> Result<Output, ToolError> {
    let path = media_file(&project_dir(ctx)?, &a.file)?;
    let ff = ffmpeg()?;
    let length = ff
        .probe_duration(&path)
        .map_err(|e| ToolError(e.to_string()))?;
    let pauses = ff
        .detect_pauses(
            &path,
            noise_db(a.noise_db)?,
            min_pause_ms(a.min_ms, DEFAULT_PAUSE_MS)?,
        )
        .map_err(|e| ToolError(e.to_string()))?;
    let speech = sound_between(&pauses, length);
    let row = |s: u64, e: u64| json!({"start_ms": s, "end_ms": e, "duration_ms": e - s});
    Ok(Output::json(&json!({
        "file": a.file,
        "duration_ms": length,
        "pauses": pauses.iter().map(|p| row(p.start_ms, p.end_ms)).collect::<Vec<_>>(),
        "sound": speech.iter().map(|(s, e)| row(*s, *e)).collect::<Vec<_>>(),
        "note": "times are in the file's own time; for a footage clip add clip.footage.in_ms to compare with the timeline",
    })))
}

/// The stretches of a file of `length` ms that are not pauses.
fn sound_between(pauses: &[od_media::Pause], length: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut from = 0;
    for p in pauses {
        if p.start_ms > from {
            out.push((from, p.start_ms));
        }
        from = p.end_ms;
    }
    if from < length {
        out.push((from, length));
    }
    out
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemovePausesArgs {
    video: String,
    clip: String,
    #[serde(default)]
    noise_db: Option<f64>,
    #[serde(default)]
    min_ms: Option<u64>,
    #[serde(default)]
    keep_ms: Option<u64>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

fn remove_pauses(ctx: &mut CallCtx, a: RemovePausesArgs) -> Result<Output, ToolError> {
    let (video, clip) = (node_id(&a.video)?, node_id(&a.clip)?);
    let keep_ms = a.keep_ms.unwrap_or(DEFAULT_KEEP_MS);
    if keep_ms > MAX_KEEP_MS {
        return Err(ToolError(format!("keep_ms is at most {MAX_KEEP_MS}")));
    }
    let file = {
        let v = video_of(ctx, &video)?;
        let at = v
            .clip(&clip)
            .ok_or_else(|| ToolError(format!("clip {clip} does not exist in {video}")))?;
        v.clips[at]
            .footage
            .as_ref()
            .map(|f| f.file.clone())
            .ok_or_else(|| {
                ToolError(format!(
                    "clip {clip} shows a design frame; only footage has pauses"
                ))
            })?
    };
    let path = media_file(&project_dir(ctx)?, &file)?;
    let pauses = ffmpeg()?
        .detect_pauses(
            &path,
            noise_db(a.noise_db)?,
            min_pause_ms(a.min_ms, DEFAULT_CUT_PAUSE_MS)?,
        )
        .map_err(|e| ToolError(e.to_string()))?;
    let ranges: Vec<(u64, u64)> = pauses.iter().map(|p| (p.start_ms, p.end_ms)).collect();
    let (ops, cut) = video_of(ctx, &video)?.pause_cut_ops(&clip, &ranges, keep_ms)?;
    let before = video_of(ctx, &video)?.duration_ms();
    let common = Common {
        video: a.video,
        turn: a.turn,
        summary: a.summary,
    };
    applied(
        ctx,
        &common,
        ops,
        json!({
            "file": file,
            "pauses_found": pauses.len(),
            "clips": cut.kept.len(),
            "removed_ms": cut.removed_ms,
            "video_ms_before": before,
            "note": "overlays, captions and sound tracks keep their times; move any that came after the cut",
        }),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WaveformArgs {
    file: String,
    #[serde(default)]
    buckets: Option<usize>,
}

/// Peaks (0 to 1, three decimals) of the sound in a `media/` file of the project at `dir`.
pub fn waveform_for(dir: &Path, reference: &str, buckets: usize) -> Result<Vec<f32>, ToolError> {
    Ok(waveform_of(dir, reference, buckets)?.1)
}

/// The length of a `media/` file in milliseconds and the peaks (0 to 1, three decimals) of its
/// sound: what a timeline row needs to draw the file's shape.
pub fn waveform_of(
    dir: &Path,
    reference: &str,
    buckets: usize,
) -> Result<(u64, Vec<f32>), ToolError> {
    let path = media_file(dir, reference)?;
    let ff = ffmpeg()?;
    let file_ms = ff
        .probe_duration(&path)
        .map_err(|e| ToolError(e.to_string()))?;
    let peaks = ff
        .waveform(&path, buckets)
        .map_err(|e| ToolError(e.to_string()))?;
    Ok((
        file_ms,
        peaks
            .into_iter()
            .map(|p| (p * 1000.0).round() / 1000.0)
            .collect(),
    ))
}

fn get_waveform(ctx: &mut CallCtx, a: WaveformArgs) -> Result<Output, ToolError> {
    let buckets = a.buckets.unwrap_or(DEFAULT_BUCKETS);
    let peaks = waveform_for(&project_dir(ctx)?, &a.file, buckets)?;
    Ok(Output::json(
        &json!({"file": a.file, "buckets": peaks.len(), "peaks": peaks}),
    ))
}

// ---- captions ----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportArgs {
    video: String,
    format: String,
    text: String,
    #[serde(default)]
    offset_ms: Option<i64>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

fn set_captions(
    ctx: &mut CallCtx,
    common: &Common,
    video: NodeId,
    captions: Captions,
    extra: Value,
) -> Result<Output, ToolError> {
    applied(
        ctx,
        common,
        set_video(video, VideoProperty::Captions(captions)),
        extra,
    )
}

fn import_captions(ctx: &mut CallCtx, a: ImportArgs) -> Result<Output, ToolError> {
    if a.text.len() > MAX_CAPTION_SOURCE_BYTES {
        return Err(ToolError(format!(
            "the transcript is longer than {MAX_CAPTION_SOURCE_BYTES} bytes"
        )));
    }
    let cues = match a.format.as_str() {
        "srt" | "vtt" => captions::parse_subtitles(&a.text)?,
        "words_json" => captions::parse_word_timings(&a.text)?,
        other => {
            return Err(ToolError(format!(
                "format is srt, vtt or words_json, not {other:?}"
            )));
        }
    };
    let video = node_id(&a.video)?;
    let (new, dropped) = {
        let v = video_of(ctx, &video)?;
        let (cues, dropped) = captions::fit_cues(cues, a.offset_ms.unwrap_or(0), v.duration_ms())?;
        (v.captions_with_cues(cues)?, dropped)
    };
    let extra = json!({
        "cues": new.cues.len(),
        "with_word_timings": new.cues.iter().filter(|c| !c.words.is_empty()).count(),
        "dropped_outside_the_video": dropped,
    });
    let common = Common {
        video: a.video,
        turn: a.turn,
        summary: a.summary,
    };
    set_captions(ctx, &common, video, new, extra)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScriptArgs {
    video: String,
    script: String,
    #[serde(default)]
    start_ms: Option<u32>,
    #[serde(default)]
    end_ms: Option<u32>,
    #[serde(default)]
    words_per_cue: Option<usize>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

fn captions_from_script(ctx: &mut CallCtx, a: ScriptArgs) -> Result<Output, ToolError> {
    if a.script.chars().count() > MAX_SCRIPT_CHARS {
        return Err(ToolError(format!(
            "the script is longer than {MAX_SCRIPT_CHARS} characters"
        )));
    }
    let video = node_id(&a.video)?;
    let new = {
        let v = video_of(ctx, &video)?;
        let end = a
            .end_ms
            .unwrap_or_else(|| u32::try_from(v.duration_ms()).unwrap_or(u32::MAX));
        let cues = captions::captions_from_script(
            &a.script,
            a.start_ms.unwrap_or(0),
            end,
            a.words_per_cue.unwrap_or(captions::DEFAULT_WORDS_PER_CUE),
        )?;
        v.captions_with_cues(cues)?
    };
    let extra = json!({
        "cues": new.cues.len(),
        "estimate": "timings are spread by word length, not recognized from speech",
    });
    let common = Common {
        video: a.video,
        turn: a.turn,
        summary: a.summary,
    };
    set_captions(ctx, &common, video, new, extra)
}

fn add_caption_cue(ctx: &mut CallCtx, args: Value) -> Result<Output, ToolError> {
    let (common, rest) = split(args)?;
    let cue: NewCue = parse(Value::Object(rest))?;
    let video = node_id(&common.video)?;
    let (new, cue_id) = video_of(ctx, &video)?.captions_with_cue(cue)?;
    set_captions(ctx, &common, video, new, json!({"cue": cue_id}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveCueArgs {
    video: String,
    cue: String,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

fn remove_caption_cue(ctx: &mut CallCtx, a: RemoveCueArgs) -> Result<Output, ToolError> {
    let video = node_id(&a.video)?;
    let cue = node_id(&a.cue)?;
    let new = video_of(ctx, &video)?.captions_without_cue(&cue)?;
    let common = Common {
        video: a.video,
        turn: a.turn,
        summary: a.summary,
    };
    set_captions(ctx, &common, video, new, json!({"removed": cue}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClearArgs {
    video: String,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

fn clear_captions(ctx: &mut CallCtx, a: ClearArgs) -> Result<Output, ToolError> {
    let video = node_id(&a.video)?;
    let new = video_of(ctx, &video)?.captions_with_cues(Vec::new())?;
    let common = Common {
        video: a.video,
        turn: a.turn,
        summary: a.summary,
    };
    set_captions(ctx, &common, video, new, json!({"cues": 0}))
}

/// The style after `changes`: from the preset when one is named, else from `current`; each
/// other key replaces its field, and `null` removes an optional one.
fn styled(
    current: &CaptionStyle,
    mut changes: Map<String, Value>,
) -> Result<CaptionStyle, ToolError> {
    let base = match changes.remove("preset") {
        Some(p) => {
            let preset: CaptionPreset = serde_json::from_value(p)
                .map_err(|e| ToolError(format!("invalid arguments: preset: {e}")))?;
            CaptionStyle::preset(preset)
        }
        None => current.clone(),
    };
    let Value::Object(mut fields) =
        serde_json::to_value(&base).map_err(|e| ToolError(e.to_string()))?
    else {
        return Err(ToolError("the style is not an object".into()));
    };
    for (key, value) in changes {
        if value.is_null() {
            fields.remove(&key);
        } else {
            fields.insert(key, value);
        }
    }
    serde_json::from_value(Value::Object(fields))
        .map_err(|e| ToolError(format!("invalid arguments: {e}")))
}

fn set_caption_style(ctx: &mut CallCtx, args: Value) -> Result<Output, ToolError> {
    let (common, rest) = split(args)?;
    let video = node_id(&common.video)?;
    let (new, style) = {
        let v = video_of(ctx, &video)?;
        let style = styled(&v.captions.style, rest)?;
        (v.captions_with_style(style.clone())?, style)
    };
    set_captions(ctx, &common, video, new, json!({"style": style}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_has_exactly_one_definition_and_none_collides_with_another_module() {
        let defs = definitions();
        let names: Vec<&str> = defs.iter().filter_map(|d| d["name"].as_str()).collect();
        assert_eq!(names, NAMES);
    }

    #[test]
    fn the_style_takes_a_preset_then_overrides_and_null_clears() {
        let base = CaptionStyle::default();
        let changes = |v: Value| match v {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        let s = styled(
            &base,
            changes(json!({"preset": "karaoke", "size_pct": 9.0})),
        )
        .unwrap();
        assert_eq!(s.size_pct, 9.0);
        assert!(s.highlight.is_some());
        let s = styled(&s, changes(json!({"highlight": null, "uppercase": true}))).unwrap();
        assert!(s.highlight.is_none() && s.uppercase);
        assert_eq!(s.size_pct, 9.0, "fields not named keep their value");
        assert!(styled(&base, changes(json!({"fontsize": 4}))).is_err());
        assert!(styled(&base, changes(json!({"preset": "neon"}))).is_err());
    }

    #[test]
    fn sound_is_the_complement_of_the_pauses() {
        let pauses = [
            od_media::Pause {
                start_ms: 0,
                end_ms: 500,
            },
            od_media::Pause {
                start_ms: 2000,
                end_ms: 3000,
            },
        ];
        assert_eq!(sound_between(&pauses, 5000), [(500, 2000), (3000, 5000)]);
        assert_eq!(sound_between(&pauses[..1], 500), Vec::<(u64, u64)>::new());
    }
}

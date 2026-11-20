//! `opendesigner sound ...` and `opendesigner captions ...`: the sound-track, pause, waveform and
//! caption tools of the MCP server, run headless against a project folder. Each subcommand calls
//! the same tool (`od_agent::audio_tools`), so the CLI and an agent behave the same way and every
//! change is one attributed, undoable transaction.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use od_agent::tools::{self, CallCtx, Content, Turns};
use od_core::{Actor, Workspace};
use serde_json::{Map, Value, json};

use crate::{CliError, actor, read_capped};

const MAX_TEXT_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Which project, and who is editing it.
#[derive(Args)]
pub struct Target {
    /// The project folder.
    dir: PathBuf,
    /// Attribute the edit to this agent instead of the user.
    #[arg(long)]
    agent: Option<String>,
    /// Group the edit under an agent turn id.
    #[arg(long)]
    turn: Option<String>,
}

/// The settings of a sound track; unset ones stay as they are (or take their defaults on `add`).
#[derive(Args, Default)]
pub struct TrackSettings {
    /// music, voice or sfx.
    #[arg(long)]
    role: Option<String>,
    /// A media/ audio file.
    #[arg(long)]
    file: Option<String>,
    #[arg(long)]
    start_ms: Option<u32>,
    /// Trim in: where in the file it starts.
    #[arg(long)]
    in_ms: Option<u32>,
    /// Trim out: where in the file it stops.
    #[arg(long)]
    out_ms: Option<u32>,
    #[arg(long)]
    gain: Option<f32>,
    #[arg(long)]
    fade_in_ms: Option<u32>,
    #[arg(long)]
    fade_out_ms: Option<u32>,
    /// -1 all left to 1 all right.
    #[arg(long, allow_negative_numbers = true)]
    pan: Option<f32>,
    /// Music only: lower under voice tracks, by this depth (0 to 1).
    #[arg(long, conflicts_with = "no_duck")]
    duck_depth: Option<f32>,
    /// Stop ducking.
    #[arg(long)]
    no_duck: bool,
}

#[derive(Subcommand)]
pub enum SoundAction {
    /// Add a sound track to a video (up to 8 besides its older single audio).
    Add {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        video: String,
        #[command(flatten)]
        track: TrackSettings,
    },
    /// Change one sound track.
    Set {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        video: String,
        /// A track id from `videos` (a1, a2, ...).
        #[arg(long)]
        track_id: String,
        #[command(flatten)]
        track: TrackSettings,
    },
    /// Remove one sound track.
    Remove {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        video: String,
        #[arg(long)]
        track_id: String,
    },
    /// List the pauses in a media/ file's sound (needs ffmpeg).
    Pauses {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        file: String,
        #[arg(long, allow_negative_numbers = true)]
        noise_db: Option<f64>,
        #[arg(long)]
        min_ms: Option<u64>,
    },
    /// Cut the pauses out of a footage clip, closing the gaps (needs ffmpeg).
    CutPauses {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        video: String,
        #[arg(long)]
        clip: String,
        #[arg(long, allow_negative_numbers = true)]
        noise_db: Option<f64>,
        #[arg(long)]
        min_ms: Option<u64>,
        #[arg(long)]
        keep_ms: Option<u64>,
    },
    /// Print waveform peaks (0 to 1) of a media/ file's sound (needs ffmpeg).
    Waveform {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        file: String,
        #[arg(long)]
        buckets: Option<usize>,
    },
}

#[derive(Subcommand)]
pub enum CaptionsAction {
    /// Replace the captions with a transcript: an SRT or WebVTT file, or word timings as JSON.
    /// OpenDesigner does no speech recognition; the transcript comes from your own speech tool.
    Import {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        video: String,
        /// srt, vtt or words_json.
        #[arg(long)]
        format: String,
        /// The transcript file.
        #[arg(long)]
        from: PathBuf,
        #[arg(long, allow_negative_numbers = true)]
        offset_ms: Option<i64>,
    },
    /// Replace the captions with a script spread over a time range by word length (an estimate).
    Script {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        video: String,
        /// The script, on the command line.
        #[arg(long, conflicts_with = "from")]
        text: Option<String>,
        /// The script, in a file.
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long)]
        start_ms: Option<u32>,
        #[arg(long)]
        end_ms: Option<u32>,
        #[arg(long)]
        words_per_cue: Option<usize>,
    },
    /// Add one cue.
    AddCue {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        video: String,
        #[arg(long)]
        start_ms: u32,
        #[arg(long)]
        end_ms: u32,
        #[arg(long)]
        text: String,
    },
    /// Remove one cue.
    RemoveCue {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        video: String,
        /// A cue id from `videos` (q1, q2, ...).
        #[arg(long)]
        cue: String,
    },
    /// Remove every cue (the style stays).
    Clear {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        video: String,
    },
    /// Set the caption style: a preset and/or single fields.
    Style {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        video: String,
        /// clean, bold_pop, karaoke or boxed.
        #[arg(long)]
        preset: Option<String>,
        #[arg(long)]
        font_family: Option<String>,
        /// Percent of the video's height.
        #[arg(long)]
        size_pct: Option<f32>,
        #[arg(long)]
        font_weight: Option<u16>,
        #[arg(long)]
        color: Option<String>,
        #[arg(long)]
        highlight: Option<String>,
        #[arg(long)]
        background: Option<String>,
        /// top, middle or bottom.
        #[arg(long)]
        position: Option<String>,
        /// left, center or right.
        #[arg(long)]
        align: Option<String>,
        #[arg(long)]
        max_lines: Option<u8>,
        #[arg(long)]
        uppercase: Option<bool>,
    },
}

fn put(args: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(v) = value {
        args.insert(key.into(), v);
    }
}

fn base(video: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("video".into(), json!(video));
    m
}

fn track_args(mut args: Map<String, Value>, t: TrackSettings) -> Map<String, Value> {
    put(&mut args, "role", t.role.map(|v| json!(v)));
    put(&mut args, "file", t.file.map(|v| json!(v)));
    put(&mut args, "start_ms", t.start_ms.map(|v| json!(v)));
    put(&mut args, "in_ms", t.in_ms.map(|v| json!(v)));
    put(&mut args, "out_ms", t.out_ms.map(|v| json!(v)));
    put(&mut args, "gain", t.gain.map(|v| json!(v)));
    put(&mut args, "fade_in_ms", t.fade_in_ms.map(|v| json!(v)));
    put(&mut args, "fade_out_ms", t.fade_out_ms.map(|v| json!(v)));
    put(&mut args, "pan", t.pan.map(|v| json!(v)));
    if t.no_duck {
        args.insert("duck".into(), Value::Null);
    } else if let Some(depth) = t.duck_depth {
        args.insert("duck".into(), json!({"depth": depth}));
    }
    args
}

fn text_file(path: &std::path::Path) -> Result<String, CliError> {
    let bytes = read_capped(path, MAX_TEXT_FILE_BYTES)?;
    String::from_utf8(bytes)
        .map_err(|_| CliError::Invalid(format!("{} is not UTF-8 text", path.display())))
}

/// Opens the project, runs the tool as the user or the named agent, and prints its answer.
fn run_tool(target: Target, name: &str, mut args: Map<String, Value>) -> Result<(), CliError> {
    put(&mut args, "turn", target.turn.map(|v| json!(v)));
    let mut ws = Workspace::open(&target.dir)?;
    let who: Actor = actor(target.agent)?;
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

pub fn sound(action: SoundAction) -> Result<(), CliError> {
    match action {
        SoundAction::Add {
            target,
            video,
            track,
        } => run_tool(target, "add_sound_track", track_args(base(&video), track)),
        SoundAction::Set {
            target,
            video,
            track_id,
            track,
        } => {
            let mut args = track_args(base(&video), track);
            args.insert("track".into(), json!(track_id));
            run_tool(target, "set_sound_track", args)
        }
        SoundAction::Remove {
            target,
            video,
            track_id,
        } => {
            let mut args = base(&video);
            args.insert("track".into(), json!(track_id));
            run_tool(target, "remove_sound_track", args)
        }
        SoundAction::Pauses {
            target,
            file,
            noise_db,
            min_ms,
        } => {
            let mut args = Map::new();
            args.insert("file".into(), json!(file));
            put(&mut args, "noise_db", noise_db.map(|v| json!(v)));
            put(&mut args, "min_ms", min_ms.map(|v| json!(v)));
            run_tool(target, "detect_pauses", args)
        }
        SoundAction::CutPauses {
            target,
            video,
            clip,
            noise_db,
            min_ms,
            keep_ms,
        } => {
            let mut args = base(&video);
            args.insert("clip".into(), json!(clip));
            put(&mut args, "noise_db", noise_db.map(|v| json!(v)));
            put(&mut args, "min_ms", min_ms.map(|v| json!(v)));
            put(&mut args, "keep_ms", keep_ms.map(|v| json!(v)));
            run_tool(target, "remove_pauses", args)
        }
        SoundAction::Waveform {
            target,
            file,
            buckets,
        } => {
            let mut args = Map::new();
            args.insert("file".into(), json!(file));
            put(&mut args, "buckets", buckets.map(|v| json!(v)));
            run_tool(target, "get_waveform", args)
        }
    }
}

pub fn captions(action: CaptionsAction) -> Result<(), CliError> {
    match action {
        CaptionsAction::Import {
            target,
            video,
            format,
            from,
            offset_ms,
        } => {
            let mut args = base(&video);
            args.insert("format".into(), json!(format));
            args.insert("text".into(), json!(text_file(&from)?));
            put(&mut args, "offset_ms", offset_ms.map(|v| json!(v)));
            run_tool(target, "import_captions", args)
        }
        CaptionsAction::Script {
            target,
            video,
            text,
            from,
            start_ms,
            end_ms,
            words_per_cue,
        } => {
            let script = match (text, from) {
                (Some(t), _) => t,
                (None, Some(path)) => text_file(&path)?,
                (None, None) => return Err(CliError::Invalid("give --text or --from".into())),
            };
            let mut args = base(&video);
            args.insert("script".into(), json!(script));
            put(&mut args, "start_ms", start_ms.map(|v| json!(v)));
            put(&mut args, "end_ms", end_ms.map(|v| json!(v)));
            put(&mut args, "words_per_cue", words_per_cue.map(|v| json!(v)));
            run_tool(target, "captions_from_script", args)
        }
        CaptionsAction::AddCue {
            target,
            video,
            start_ms,
            end_ms,
            text,
        } => {
            let mut args = base(&video);
            args.insert("start_ms".into(), json!(start_ms));
            args.insert("end_ms".into(), json!(end_ms));
            args.insert("text".into(), json!(text));
            run_tool(target, "add_caption_cue", args)
        }
        CaptionsAction::RemoveCue { target, video, cue } => {
            let mut args = base(&video);
            args.insert("cue".into(), json!(cue));
            run_tool(target, "remove_caption_cue", args)
        }
        CaptionsAction::Clear { target, video } => run_tool(target, "clear_captions", base(&video)),
        CaptionsAction::Style {
            target,
            video,
            preset,
            font_family,
            size_pct,
            font_weight,
            color,
            highlight,
            background,
            position,
            align,
            max_lines,
            uppercase,
        } => {
            let mut args = base(&video);
            put(&mut args, "preset", preset.map(|v| json!(v)));
            put(&mut args, "font_family", font_family.map(|v| json!(v)));
            put(&mut args, "size_pct", size_pct.map(|v| json!(v)));
            put(&mut args, "font_weight", font_weight.map(|v| json!(v)));
            put(&mut args, "color", color.map(|v| json!(v)));
            put(&mut args, "highlight", highlight.map(|v| json!(v)));
            put(&mut args, "background", background.map(|v| json!(v)));
            put(&mut args, "position", position.map(|v| json!(v)));
            put(&mut args, "align", align.map(|v| json!(v)));
            put(&mut args, "max_lines", max_lines.map(|v| json!(v)));
            put(&mut args, "uppercase", uppercase.map(|v| json!(v)));
            run_tool(target, "set_caption_style", args)
        }
    }
}

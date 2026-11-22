//! Footage memory: a way for an agent to know what is in a recording without watching all of it
//! again every time (`docs/wiki/video-editing.md#footage-memory`).
//!
//! `watch_footage` shows a few evenly spaced pictures of a file, for the agent to look at.
//! `index_footage` cuts a file into shots, optionally adds a transcript from the user's own speech
//! program, and keeps what the agent writes about each shot. `search_footage` finds moments by
//! describing them and answers with the numbers `add_clip` wants. The index is one plain JSON file
//! per recording in the project's `footage-index/` folder; nothing is uploaded and nothing is
//! recognized by a model of ours: the agent is the one that looks, and it writes down what it saw.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use od_media::Ffmpeg;
use od_media::footage::shots_from_cuts;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::inspect::draw;
use crate::tools::{CallCtx, Content, Output, ToolError, parse, tool};

pub const NAMES: [&str; 3] = ["watch_footage", "index_footage", "search_footage"];

pub const INDEX_DIR: &str = "footage-index";
const INDEX_VERSION: u32 = 1;
const DEFAULT_FRAMES: u32 = 8;
const MAX_FRAMES: u32 = 10;
const DEFAULT_CELL_WIDTH: u32 = 320;
const MIN_CELL_WIDTH: u32 = 128;
const MAX_CELL_WIDTH: u32 = 640;
const MAX_COLUMNS: u32 = 5;
const DEFAULT_THRESHOLD: f64 = 0.3;
const DEFAULT_MIN_SHOT_MS: u64 = 500;
/// More shots than this means the threshold is too low: it is the flicker that is being cut.
const MAX_SHOTS: usize = 400;
const MAX_WORDS: usize = 20_000;
const MAX_NOTE_CHARS: usize = 300;
const MAX_TAGS: usize = 8;
const MAX_TAG_CHARS: usize = 32;
const MAX_NOTES_PER_CALL: usize = 100;
const MAX_QUESTION_CHARS: usize = 500;
const MAX_INDEX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_INDEXES_READ: usize = 200;
const MAX_QUERY_CHARS: usize = 200;
const DEFAULT_RESULTS: usize = 10;
const MAX_RESULTS: usize = 50;
const SHOT_TEXT_CHARS: usize = 160;
const CARRY_WITHIN_MS: u64 = 250;
/// How long a clip a search result proposes at most.
const MAX_PROPOSED_CLIP_MS: u64 = 10_000;

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

pub fn definitions() -> Vec<Value> {
    let file = json!({"type": "string", "description": "A footage file from list_footage: media/<name>.mp4, .mov or .m4v."});
    vec![
        tool(
            "watch_footage",
            "Look at a recording without importing it into a video: up to 10 evenly spaced \
             pictures from a footage file (or from `from_ms` to `to_ms` of it), in one labeled grid \
             you can see, each picture carrying its time in ms. Pass the `question` you want \
             answered and it is repeated next to the picture so you look for the right thing. The \
             pictures are one per equal slice of the range, so motion between them is not \
             visible: narrow the range to look closer. Read-only; needs ffmpeg. Use it before \
             writing a note with index_footage, and again on a narrower range to find an exact \
             moment to cut at.",
            json!({
                "file": file.clone(),
                "question": {"type": "string", "maxLength": MAX_QUESTION_CHARS, "description": "What you are trying to learn from the pictures."},
                "frames": {"type": "integer", "minimum": 1, "maximum": MAX_FRAMES, "description": "How many pictures (default 8)."},
                "from_ms": {"type": "integer", "minimum": 0, "description": "Start of the range (default 0)."},
                "to_ms": {"type": "integer", "minimum": 1, "description": "End of the range (default: the end of the file)."},
                "cell_width": {"type": "integer", "minimum": MIN_CELL_WIDTH, "maximum": MAX_CELL_WIDTH, "description": "Width of each picture in pixels (default 320)."}
            }),
            &["file"],
            true,
        ),
        tool(
            "index_footage",
            "Remember what is in a footage file. Cuts it into shots where the picture changes \
             (ffmpeg's scene score; `threshold` 0.3 is a start, lower finds more cuts), \
             optionally adds a word-timed transcript with `transcribe: true` (the user's own \
             whisper program: call get_capabilities first), and stores the result in \
             footage-index/ in the project. Then `watch_footage` each shot you have not seen and \
             write what is in it with `notes`: [{shot, note, tags?}] (shot numbers start at 0; an \
             empty note clears one). Calling it again for an unchanged file with the same \
             threshold only updates notes and the transcript; a changed file or threshold scans \
             again and carries notes over to shots that start within a quarter of a second of \
             where they did. The answer lists every shot with its note and the words spoken in \
             it. Nothing is uploaded.",
            json!({
                "file": file.clone(),
                "threshold": {"type": "number", "minimum": 0.01, "maximum": 0.99, "description": "How different a picture must be from the one before to count as a cut (default 0.3)."},
                "min_shot_ms": {"type": "integer", "minimum": 100, "maximum": 60000, "description": "A cut closer than this to the previous one is ignored (default 500)."},
                "rescan": {"type": "boolean", "description": "Scan again even though the index is current."},
                "transcribe": {"type": "boolean", "description": "Run the user's speech program and keep the words (slow for long speech)."},
                "language": {"type": "string", "description": "With transcribe: `auto` (default) or a code such as en, de."},
                "model": {"type": "string", "description": "With transcribe: the speech model, as for the transcribe tool."},
                "timeout_s": {"type": "integer", "minimum": 10, "maximum": 1800, "description": "With transcribe: stop the speech program after this long (default 300)."},
                "notes": {
                    "type": "array", "maxItems": MAX_NOTES_PER_CALL,
                    "description": "What you saw: one entry per shot.",
                    "items": {"type": "object", "properties": {
                        "shot": {"type": "integer", "minimum": 0},
                        "note": {"type": "string", "maxLength": MAX_NOTE_CHARS, "description": "A plain sentence: who or what, doing what, where, how it is shot. Empty clears the note."},
                        "tags": {"type": "array", "maxItems": MAX_TAGS, "items": {"type": "string", "maxLength": MAX_TAG_CHARS}}
                    }, "required": ["shot", "note"]}
                }
            }),
            &["file"],
            false,
        ),
        tool(
            "search_footage",
            "Find a moment in the footage you indexed by describing it: words are matched against \
             your shot notes and tags (strongest) and the transcript. Each result is a shot with \
             its file, start and end in ms, the note, the words spoken and a ready `clip` \
             ({footage: {file, in_ms}, duration_ms}) to put in add_clip. Without `query` it lists \
             what is indexed and how much of it has notes. A file that changed since it was \
             indexed is marked `stale`: run index_footage on it again. Read-only; works from the \
             index alone, so it needs neither ffmpeg nor a speech program.",
            json!({
                "query": {"type": "string", "maxLength": MAX_QUERY_CHARS, "description": "Words to look for, for example `founder laughing at the desk`."},
                "file": file,
                "limit": {"type": "integer", "minimum": 1, "maximum": MAX_RESULTS, "description": "Most results (default 10)."}
            }),
            &[],
            true,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "watch_footage" => watch(ctx, parse(args)?),
        "index_footage" => index(ctx, parse(args)?),
        "search_footage" => search(ctx, parse(args)?),
        other => fail(format!("unknown tool {other:?}")),
    }
}

fn project_dir(ctx: &CallCtx) -> Result<PathBuf, ToolError> {
    match ctx.ws.dir() {
        Some(d) => Ok(d.to_path_buf()),
        None => fail("the project has no folder, so no footage; open or create a project first"),
    }
}

/// `media/<name>` as a plain footage file inside the project.
fn footage_file(dir: &Path, reference: &str) -> Result<PathBuf, ToolError> {
    od_core::media::footage_name(reference)?;
    od_media::footage_path(dir, reference).map_err(|e| ToolError(e.to_string()))
}

fn ffmpeg() -> Result<Ffmpeg, ToolError> {
    Ffmpeg::locate().map_err(|e| ToolError(e.to_string()))
}

// ---- watch_footage ----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WatchArgs {
    file: String,
    #[serde(default)]
    question: Option<String>,
    #[serde(default)]
    frames: Option<u32>,
    #[serde(default)]
    from_ms: Option<u64>,
    #[serde(default)]
    to_ms: Option<u64>,
    #[serde(default)]
    cell_width: Option<u32>,
}

/// The middle of each of `count` equal slices of `from..to`: a picture of the start of a file is
/// often a fade or black, the middle of a slice rarely is.
fn even_times(from: u64, to: u64, count: u32) -> Vec<u64> {
    let (span, n) = (to - from, u64::from(count));
    (0..n)
        .map(|i| from + (2 * i + 1) * span / (2 * n))
        .collect()
}

fn watch(ctx: &mut CallCtx, a: WatchArgs) -> Result<Output, ToolError> {
    let dir = project_dir(ctx)?;
    let path = footage_file(&dir, &a.file)?;
    let frames = a.frames.unwrap_or(DEFAULT_FRAMES);
    if !(1..=MAX_FRAMES).contains(&frames) {
        return fail(format!("frames is 1 to {MAX_FRAMES}, not {frames}"));
    }
    let width = a.cell_width.unwrap_or(DEFAULT_CELL_WIDTH);
    if !(MIN_CELL_WIDTH..=MAX_CELL_WIDTH).contains(&width) {
        return fail(format!(
            "cell_width is {MIN_CELL_WIDTH} to {MAX_CELL_WIDTH}, not {width}"
        ));
    }
    if a.question
        .as_ref()
        .is_some_and(|q| q.chars().count() > MAX_QUESTION_CHARS)
    {
        return fail(format!(
            "question is at most {MAX_QUESTION_CHARS} characters"
        ));
    }
    let ff = ffmpeg()?;
    let info = ff.probe_file(&path).map_err(|e| ToolError(e.to_string()))?;
    let (from, to) = (
        a.from_ms.unwrap_or(0),
        a.to_ms.unwrap_or(info.duration_ms).min(info.duration_ms),
    );
    if from >= to {
        return fail(format!(
            "the range {from} to {to} ms is empty; {} is {} ms long",
            a.file, info.duration_ms
        ));
    }
    let height = ((u64::from(width) * u64::from(info.height) + u64::from(info.width) / 2)
        / u64::from(info.width.max(1)))
    .max(1) as u32;
    let times = even_times(from, to, frames);
    let mut pictures = Vec::with_capacity(times.len());
    for &t in &times {
        let rgba = ff
            .frame_rgba(&path, t, width, height)
            .map_err(|e| ToolError(e.to_string()))?;
        let mut pixmap = od_render::pixmap_from_rgba(width, height, &rgba)
            .ok_or_else(|| ToolError("a picture came back at the wrong size".into()))?;
        let scale = draw::label_scale(&pixmap);
        draw::label(&mut pixmap, &t.to_string(), (3 * scale, 3 * scale), scale);
        pictures.push(pixmap);
    }
    let columns = (times.len() as u32).min(MAX_COLUMNS);
    let sheet = od_render::grid(&pictures, columns)
        .ok_or_else(|| ToolError("the sheet is too large to draw".into()))?;
    let png = od_render::encode_png(&sheet, false)?;
    let mut note = format!(
        "{} ({}x{}, {:.1} fps, {} ms, {}): {} pictures, one from the middle of each equal slice of \
         {from} to {to} ms, {columns} to a row, each labeled with its time in ms",
        a.file,
        info.width,
        info.height,
        info.fps,
        info.duration_ms,
        if info.has_audio {
            "with sound"
        } else {
            "no sound"
        },
        times.len(),
    );
    if let Some(q) = a
        .question
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        note.push_str(&format!(". Answer from these pictures only: {q}"));
    }
    Ok(Output {
        content: vec![
            Content::Image {
                data: STANDARD.encode(png),
                mime: "image/png",
            },
            Content::Text(note),
        ],
    })
}

// ---- the index ----

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ShotEntry {
    start_ms: u64,
    end_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct WordEntry {
    text: String,
    start_ms: u64,
    end_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Index {
    version: u32,
    file: String,
    bytes: u64,
    duration_ms: u64,
    threshold: f64,
    min_shot_ms: u64,
    shots: Vec<ShotEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    words: Vec<WordEntry>,
}

/// `media/My clip.mp4` is kept as `footage-index/My_clip.mp4.json`.
fn index_path(dir: &Path, reference: &str) -> PathBuf {
    let name = reference.strip_prefix("media/").unwrap_or(reference);
    dir.join(INDEX_DIR)
        .join(format!("{}.json", name.replace(' ', "_")))
}

fn refuse_link(path: &Path) -> Result<(), ToolError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => fail(format!(
            "{} is a link; the footage index is never read or written through one",
            path.display()
        )),
        Ok(_) | Err(_) => Ok(()),
    }
}

fn load(path: &Path) -> Result<Option<Index>, ToolError> {
    if let Some(folder) = path.parent() {
        refuse_link(folder)?;
    }
    refuse_link(path)?;
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    if meta.len() > MAX_INDEX_BYTES {
        return fail(format!(
            "{} is over {MAX_INDEX_BYTES} bytes; delete it and index the file again",
            path.display()
        ));
    }
    let text = fs::read_to_string(path)
        .map_err(|e| ToolError(format!("cannot read {}: {e}", path.display())))?;
    let index: Index = serde_json::from_str(&text)
        .map_err(|e| ToolError(format!("{} is not a footage index: {e}", path.display())))?;
    if index.version != INDEX_VERSION {
        return fail(format!(
            "{} is version {}; this app reads version {INDEX_VERSION}",
            path.display(),
            index.version
        ));
    }
    Ok(Some(index))
}

fn store(dir: &Path, index: &Index) -> Result<String, ToolError> {
    let bytes = serde_json::to_vec_pretty(index).map_err(|e| ToolError(e.to_string()))?;
    if bytes.len() as u64 > MAX_INDEX_BYTES {
        return fail(format!(
            "the index would be {} bytes; the most is {MAX_INDEX_BYTES}: raise the threshold for fewer shots or leave the transcript out",
            bytes.len()
        ));
    }
    let folder = dir.join(INDEX_DIR);
    refuse_link(&folder)?;
    fs::create_dir_all(&folder)
        .map_err(|e| ToolError(format!("cannot create the footage index folder: {e}")))?;
    let target = index_path(dir, &index.file);
    refuse_link(&target)?;
    let temp = folder.join(format!(
        ".{}.tmp",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("index")
    ));
    fs::write(&temp, &bytes).map_err(|e| ToolError(format!("cannot write the index: {e}")))?;
    fs::rename(&temp, &target).map_err(|e| ToolError(format!("cannot write the index: {e}")))?;
    Ok(format!(
        "{INDEX_DIR}/{}",
        target.file_name().and_then(|n| n.to_str()).unwrap_or("")
    ))
}

fn clean_note(note: &str) -> Result<Option<String>, ToolError> {
    let note = note.split_whitespace().collect::<Vec<_>>().join(" ");
    if note.chars().count() > MAX_NOTE_CHARS {
        return fail(format!(
            "a note is at most {MAX_NOTE_CHARS} characters: say what is in the shot, not everything about it"
        ));
    }
    Ok((!note.is_empty()).then_some(note))
}

fn clean_tags(tags: &[String]) -> Result<Vec<String>, ToolError> {
    if tags.len() > MAX_TAGS {
        return fail(format!("a shot has at most {MAX_TAGS} tags"));
    }
    let mut out = BTreeSet::new();
    for tag in tags {
        let tag = tag.trim().to_lowercase();
        if tag.is_empty() || tag.chars().count() > MAX_TAG_CHARS {
            return fail(format!(
                "a tag is 1 to {MAX_TAG_CHARS} characters, not {tag:?}"
            ));
        }
        out.insert(tag);
    }
    Ok(out.into_iter().collect())
}

/// The old shot's notes, moved to the new shots that start within [`CARRY_WITHIN_MS`] of where
/// the old one did (the nearest one each). Returns how many moved and how many had no home.
fn carry_notes(old: &[ShotEntry], new: &mut [ShotEntry]) -> (usize, usize) {
    let (mut carried, mut dropped) = (0, 0);
    for shot in old
        .iter()
        .filter(|s| s.note.is_some() || !s.tags.is_empty())
    {
        let home = new
            .iter_mut()
            .filter(|n| n.start_ms.abs_diff(shot.start_ms) <= CARRY_WITHIN_MS)
            .filter(|n| n.note.is_none() && n.tags.is_empty())
            .min_by_key(|n| n.start_ms.abs_diff(shot.start_ms));
        match home {
            Some(n) => {
                n.note.clone_from(&shot.note);
                n.tags.clone_from(&shot.tags);
                carried += 1;
            }
            None => dropped += 1,
        }
    }
    (carried, dropped)
}

fn words_in<'a>(index: &'a Index, shot: &ShotEntry) -> Vec<&'a WordEntry> {
    index
        .words
        .iter()
        .filter(|w| w.start_ms >= shot.start_ms && w.start_ms < shot.end_ms)
        .collect()
}

fn excerpt(words: &[&WordEntry], max_chars: usize) -> String {
    let mut text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if text.chars().count() > max_chars {
        text = text.chars().take(max_chars).collect::<String>() + "…";
    }
    text
}

// ---- index_footage ----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteArg {
    shot: usize,
    note: String,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexArgs {
    file: String,
    #[serde(default)]
    threshold: Option<f64>,
    #[serde(default)]
    min_shot_ms: Option<u64>,
    #[serde(default)]
    rescan: bool,
    #[serde(default)]
    transcribe: bool,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    timeout_s: Option<u64>,
    #[serde(default)]
    notes: Vec<NoteArg>,
}

fn scan(
    ff: &Ffmpeg,
    path: &Path,
    file: &str,
    bytes: u64,
    threshold: f64,
    min_shot_ms: u64,
) -> Result<Index, ToolError> {
    let duration_ms = ff
        .probe_duration(path)
        .map_err(|e| ToolError(e.to_string()))?;
    let cuts = ff
        .detect_cuts(path, threshold)
        .map_err(|e| ToolError(e.to_string()))?;
    let shots: Vec<ShotEntry> = shots_from_cuts(&cuts, duration_ms, min_shot_ms)
        .into_iter()
        .map(|s| ShotEntry {
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            note: None,
            tags: Vec::new(),
        })
        .collect();
    if shots.len() > MAX_SHOTS {
        return fail(format!(
            "{file} came out as {} shots at threshold {threshold}; the most is {MAX_SHOTS}. Raise the threshold (0.4 or 0.5) or the min_shot_ms",
            shots.len()
        ));
    }
    Ok(Index {
        version: INDEX_VERSION,
        file: file.to_string(),
        bytes,
        duration_ms,
        threshold,
        min_shot_ms,
        shots,
        language: None,
        words: Vec::new(),
    })
}

fn apply_notes(index: &mut Index, notes: &[NoteArg]) -> Result<(), ToolError> {
    if notes.len() > MAX_NOTES_PER_CALL {
        return fail(format!("at most {MAX_NOTES_PER_CALL} notes a call"));
    }
    let count = index.shots.len();
    for n in notes {
        if n.shot >= count {
            return fail(format!(
                "shot {} does not exist: {} has shots 0 to {}",
                n.shot,
                index.file,
                count - 1
            ));
        }
    }
    for n in notes {
        let tags = n.tags.as_deref().map(clean_tags).transpose()?;
        let note = clean_note(&n.note)?;
        let shot = &mut index.shots[n.shot];
        shot.note = note;
        if let Some(tags) = tags {
            shot.tags = tags;
        }
    }
    Ok(())
}

fn shot_rows(index: &Index) -> Vec<Value> {
    index
        .shots
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut row = json!({
                "shot": i, "start_ms": s.start_ms, "end_ms": s.end_ms,
                "duration_ms": s.end_ms - s.start_ms,
            });
            if let Some(note) = &s.note {
                row["note"] = json!(note);
            }
            if !s.tags.is_empty() {
                row["tags"] = json!(s.tags);
            }
            let said = excerpt(&words_in(index, s), SHOT_TEXT_CHARS);
            if !said.is_empty() {
                row["said"] = json!(said);
            }
            row
        })
        .collect()
}

fn index(ctx: &mut CallCtx, a: IndexArgs) -> Result<Output, ToolError> {
    let dir = project_dir(ctx)?;
    let path = footage_file(&dir, &a.file)?;
    let bytes = fs::metadata(&path)
        .map_err(|e| ToolError(format!("{}: {e}", a.file)))?
        .len();
    let existing = load(&index_path(&dir, &a.file))?;
    let threshold = a
        .threshold
        .or(existing.as_ref().map(|e| e.threshold))
        .unwrap_or(DEFAULT_THRESHOLD);
    let min_shot_ms = a
        .min_shot_ms
        .or(existing.as_ref().map(|e| e.min_shot_ms))
        .unwrap_or(DEFAULT_MIN_SHOT_MS);
    let current = existing.as_ref().filter(|e| {
        !a.rescan && e.bytes == bytes && e.threshold == threshold && e.min_shot_ms == min_shot_ms
    });
    let (mut index, scanned, carried) = match current {
        Some(e) => (e.clone(), false, (0, 0)),
        None => {
            let mut fresh = scan(&ffmpeg()?, &path, &a.file, bytes, threshold, min_shot_ms)?;
            let carried = match &existing {
                Some(old) => {
                    let moved = carry_notes(&old.shots, &mut fresh.shots);
                    fresh.language.clone_from(&old.language);
                    fresh.words.clone_from(&old.words);
                    moved
                }
                None => (0, 0),
            };
            (fresh, true, carried)
        }
    };
    if a.transcribe {
        let language = a.language.as_deref().unwrap_or("auto");
        let words = crate::external_tools::words_of_media(
            ctx,
            &a.file,
            language,
            a.model.as_deref(),
            a.timeout_s,
        )?;
        index.words = words
            .into_iter()
            .take(MAX_WORDS)
            .map(|w| WordEntry {
                text: w.text,
                start_ms: w.start_ms,
                end_ms: w.end_ms,
            })
            .collect();
        index.language = Some(language.to_string());
    }
    apply_notes(&mut index, &a.notes)?;
    let saved = store(&dir, &index)?;
    let unnoted: Vec<usize> = index
        .shots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.note.is_none())
        .map(|(i, _)| i)
        .collect();
    let mut out = json!({
        "file": index.file,
        "index": saved,
        "duration_ms": index.duration_ms,
        "threshold": index.threshold,
        "scanned": scanned,
        "shot_count": index.shots.len(),
        "shots": shot_rows(&index),
        "unnoted_shots": unnoted,
        "transcript_words": index.words.len(),
        "notes_set": a.notes.len(),
    });
    if scanned && existing.is_some() {
        out["notes_carried"] = json!(carried.0);
        out["notes_dropped"] = json!(carried.1);
    }
    out["next"] = json!(if unnoted.is_empty() {
        "every shot has a note: search_footage finds moments by description"
    } else {
        "watch_footage each unnoted shot (from_ms, to_ms of its row), then call index_footage with notes: [{shot, note, tags?}]"
    });
    Ok(Output::json(&out))
}

// ---- search_footage ----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

/// Lowercase words of letters and digits.
fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// How well one shot answers the query tokens: a token in the note or a tag counts 3, one only
/// in the transcript counts 1, and the whole query as a phrase in the note or the transcript adds
/// 2. Zero means nothing matched.
fn score(query: &[String], phrase: &str, index: &Index, shot: &ShotEntry) -> (u32, Vec<String>) {
    let note_words: BTreeSet<String> = shot
        .note
        .iter()
        .flat_map(|n| tokens(n))
        .chain(shot.tags.iter().flat_map(|t| tokens(t)))
        .collect();
    let spoken: Vec<String> = words_in(index, shot)
        .iter()
        .flat_map(|w| tokens(&w.text))
        .collect();
    let (mut total, mut matched) = (0, Vec::new());
    for token in query {
        if note_words.contains(token) {
            total += 3;
            matched.push(token.clone());
        } else if spoken.contains(token) {
            total += 1;
            matched.push(token.clone());
        }
    }
    if total > 0 && query.len() > 1 {
        let note_text = shot
            .note
            .as_deref()
            .map(|n| tokens(n).join(" "))
            .unwrap_or_default();
        if note_text.contains(phrase) || spoken.join(" ").contains(phrase) {
            total += 2;
        }
    }
    (total, matched)
}

struct Found {
    file: String,
    stale: bool,
    index: Index,
}

fn indexes(dir: &Path, only: Option<&str>) -> Result<(Vec<Found>, Vec<String>), ToolError> {
    let names: Vec<PathBuf> = match only {
        Some(file) => {
            footage_file(dir, file)?;
            vec![index_path(dir, file)]
        }
        None => {
            let folder = dir.join(INDEX_DIR);
            refuse_link(&folder)?;
            let mut files: Vec<PathBuf> = match fs::read_dir(&folder) {
                Ok(entries) => entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json"))
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| !n.starts_with('.'))
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
            files.sort();
            files.truncate(MAX_INDEXES_READ);
            files
        }
    };
    let (mut found, mut skipped) = (Vec::new(), Vec::new());
    for path in names {
        match load(&path) {
            Ok(Some(index)) => {
                let stale = od_media::footage_path(dir, &index.file)
                    .ok()
                    .and_then(|p| fs::metadata(p).ok())
                    .is_none_or(|m| m.len() != index.bytes);
                found.push(Found {
                    file: index.file.clone(),
                    stale,
                    index,
                });
            }
            Ok(None) => {
                if only.is_some() {
                    return fail(format!(
                        "{} is not indexed yet: index_footage {{file}} first",
                        only.unwrap_or_default()
                    ));
                }
            }
            Err(e) => skipped.push(e.0),
        }
    }
    Ok((found, skipped))
}

fn search(ctx: &mut CallCtx, a: SearchArgs) -> Result<Output, ToolError> {
    let dir = project_dir(ctx)?;
    let limit = a.limit.unwrap_or(DEFAULT_RESULTS);
    if !(1..=MAX_RESULTS).contains(&limit) {
        return fail(format!("limit is 1 to {MAX_RESULTS}, not {limit}"));
    }
    let (found, skipped) = indexes(&dir, a.file.as_deref())?;
    let query_text = a.query.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let Some(query_text) = query_text else {
        let files: Vec<Value> = found
            .iter()
            .map(|f| {
                json!({
                    "file": f.file,
                    "duration_ms": f.index.duration_ms,
                    "shots": f.index.shots.len(),
                    "with_notes": f.index.shots.iter().filter(|s| s.note.is_some()).count(),
                    "transcript_words": f.index.words.len(),
                    "stale": f.stale,
                })
            })
            .collect();
        return Ok(Output::json(&json!({
            "indexed": files,
            "skipped": skipped,
            "next": "search_footage {query} finds moments; list_footage shows files that are not indexed yet",
        })));
    };
    if query_text.chars().count() > MAX_QUERY_CHARS {
        return fail(format!("query is at most {MAX_QUERY_CHARS} characters"));
    }
    let query = tokens(query_text);
    if query.is_empty() {
        return fail("the query has no words in it");
    }
    let phrase = query.join(" ");
    let mut hits: Vec<(u32, usize, usize, Vec<String>)> = Vec::new();
    for (fi, f) in found.iter().enumerate() {
        for (si, shot) in f.index.shots.iter().enumerate() {
            let (points, matched) = score(&query, &phrase, &f.index, shot);
            if points > 0 {
                hits.push((points, fi, si, matched));
            }
        }
    }
    hits.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(found[a.1].file.cmp(&found[b.1].file))
            .then(a.2.cmp(&b.2))
    });
    let results: Vec<Value> = hits
        .iter()
        .take(limit)
        .map(|(points, fi, si, matched)| {
            let f = &found[*fi];
            let shot = &f.index.shots[*si];
            let length = (shot.end_ms - shot.start_ms).min(MAX_PROPOSED_CLIP_MS);
            let mut row = json!({
                "file": f.file, "shot": si, "score": points, "matched": matched,
                "start_ms": shot.start_ms, "end_ms": shot.end_ms,
                "clip": {"footage": {"file": f.file, "in_ms": shot.start_ms}, "duration_ms": length},
                "stale": f.stale,
            });
            if let Some(note) = &shot.note {
                row["note"] = json!(note);
            }
            if !shot.tags.is_empty() {
                row["tags"] = json!(shot.tags);
            }
            let said = excerpt(&words_in(&f.index, shot), SHOT_TEXT_CHARS);
            if !said.is_empty() {
                row["said"] = json!(said);
            }
            row
        })
        .collect();
    let mut out = json!({
        "query": query_text,
        "matches": hits.len(),
        "results": results,
        "searched": found.len(),
        "skipped": skipped,
    });
    if hits.is_empty() {
        out["hint"] = json!(
            "nothing matched: shots without notes only match the words spoken in them. Run watch_footage on the unnoted shots and write notes with index_footage, or try other words"
        );
    }
    if found.iter().any(|f| f.stale) {
        out["warning"] = json!(
            "some files changed since they were indexed (stale): run index_footage on them again"
        );
    }
    Ok(Output::json(&out))
}

#[cfg(test)]
mod tests;

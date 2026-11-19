//! The MCP tool surface: thin wrappers over `core`, `render` and `store`. Arguments are parsed
//! into closed types and every rejection carries a reason the agent can act on
//! (`docs/wiki/agent-integration.md#the-mcp-server`).

use crate::export::{ExportOutcome, Plan, export_assets, export_name, exports_dir, preset};
use crate::synth;
use std::collections::{HashMap, HashSet};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use od_core::{Actor, AnnotationStatus, Node, NodeId, NodeKind, OpError, Operation, Workspace};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const DEFAULT_PREVIEW_SIDE: f64 = 1024.0;
const MAX_PREVIEW_SIDE: f64 = 2048.0;
const MAX_DEPTH_ARG: usize = 10;
const MAX_FIND_RESULTS: usize = 200;
const MAX_VALIDATE_FILES: usize = 100;
const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

/// A rejected tool call. The text goes back to the agent as the tool result.
#[derive(Debug)]
pub struct ToolError(pub String);

impl<E: std::fmt::Display + ToolErrorSource> From<E> for ToolError {
    fn from(e: E) -> Self {
        ToolError(e.to_string())
    }
}

/// Marker for the error types that may become a tool error.
pub trait ToolErrorSource {}
impl ToolErrorSource for OpError {}
impl ToolErrorSource for od_core::ProjectError {}
impl ToolErrorSource for od_render::RenderError {}
impl ToolErrorSource for std::io::Error {}
impl ToolErrorSource for crate::memory::MemoryError {}

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

pub enum Content {
    Text(String),
    Image { data: String, mime: &'static str },
}

pub struct Output {
    pub content: Vec<Content>,
}

impl Output {
    fn text(s: impl Into<String>) -> Output {
        Output {
            content: vec![Content::Text(s.into())],
        }
    }

    fn json(v: &Value) -> Output {
        Output::text(v.to_string())
    }
}

/// Per-server bookkeeping that tool calls share: which agent turns already have a checkpoint.
#[derive(Default)]
pub struct Turns {
    checkpoint_by_turn: HashMap<String, u64>,
    counter: u64,
    seen: HashSet<String>,
    /// For each frame, how long the history was when `review_frame` last looked at it.
    reviewed: HashMap<NodeId, usize>,
}

pub struct CallCtx<'a> {
    pub ws: &'a mut Workspace,
    pub actor: Actor,
    pub turns: &'a mut Turns,
    /// The names of the MCP servers the user added (`Handler::set_extra_servers`).
    pub extra_servers: &'a [String],
}

fn tool(name: &str, description: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    tool_full(name, description, props, required, read_only, false)
}

/// A tool that reaches the network: it says so with `openWorldHint`.
fn tool_net(
    name: &str,
    description: &str,
    props: Value,
    required: &[&str],
    read_only: bool,
) -> Value {
    tool_full(name, description, props, required, read_only, true)
}

fn tool_full(
    name: &str,
    description: &str,
    props: Value,
    required: &[&str],
    read_only: bool,
    open_world: bool,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
            "additionalProperties": false,
        },
        "annotations": { "readOnlyHint": read_only, "openWorldHint": open_world },
    })
}

/// The `tools/list` result.
pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    vec![
        tool(
            "get_guidance",
            "Read this first. Returns the pages, the nodes the user has selected (with their data), \
             the open notes the user pinned, and the edits the user made after your last turn. \
             'Make this bigger' means the selection.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_playbook",
            "Craft guidance for making work that is not mediocre. Topics: design (images), style \
             (named looks with rules), video, motion (timings, easing, recipes), formats (sizes and \
             safe zones). Call it with no topic for the list; read the ones that apply before you \
             build, not after.",
            json!({"topic": {"type": "string", "enum": ["design", "style", "video", "motion", "formats"]}}),
            &[],
            true,
        ),
        tool(
            "review_frame",
            "An art director's check of one frame, in numbers: text contrast against what is \
             under it, text too small to read at thumbnail size, text too close to or past an edge, \
             overlapping text, text that overflows its box, misaligned left edges, too many fonts, \
             hard-coded colors that should be tokens, and a size that differs from what the user \
             chose. Fix every `error`, and every `warn` unless you have a reason. Does not change \
             the design.",
            json!({"frame": id("Id of a frame from list_pages or get_tree.")}),
            &["frame"],
            true,
        ),
        tool(
            "list_formats",
            "Sizes to make things in, in sections (Video, Store images, Social images, Web and \
             icons): id, label, width, height, frame rate. Use when the workspace has no format \
             yet; then record your pick with set_format.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_brief",
            "What this workspace is for: its format (size, aspect ratio, frame rate), who chose it \
             (the user or you), a request waiting for the user's answer, and the user's last answer.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "set_format",
            "Set this workspace's format: a `format` id from list_formats, or a custom size \
             (`kind` video or image, `width`, `height`, `fps` for video). If the user chose the \
             current format this does NOT change it: the user is asked, and you must stop and wait \
             (call get_brief later to read the answer). If they chose nothing it applies at once. \
             A video cannot be created or resized away from a format the user chose.",
            json!({
                "format": id("An id from list_formats."),
                "kind": {"type": "string", "enum": ["video", "image"]},
                "width": {"type": "integer", "minimum": 16, "maximum": 16384},
                "height": {"type": "integer", "minimum": 16, "maximum": 16384},
                "fps": {"type": "integer", "minimum": 1, "maximum": 60},
                "reason": id("One line for the user: why this format.")
            }),
            &["reason"],
            false,
        ),
        tool(
            "get_selection",
            "The nodes the user has selected, with their data.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_annotations",
            "The notes the user pinned on nodes or page regions.",
            json!({"include_resolved": {"type": "boolean", "description": "Also list resolved notes."}}),
            &[],
            true,
        ),
        tool(
            "resolve_annotation",
            "Mark a note resolved once you have acted on it. Say what you did in the user's terms.",
            json!({"id": {"type": "integer"}, "note": id("What you changed.")}),
            &["id"],
            false,
        ),
        tool(
            "list_pages",
            "Pages and their top-level nodes.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_node",
            "One node with its properties and, to `depth` levels, its children.",
            json!({"id": id("Node id, for example n_7."), "depth": {"type": "integer", "minimum": 0, "maximum": 10}}),
            &["id"],
            true,
        ),
        tool(
            "get_tree",
            "A page as an indented outline: id, kind, name, position and size.",
            json!({"page": id("Page id; defaults to the first page."), "depth": {"type": "integer", "minimum": 1, "maximum": 10}}),
            &[],
            true,
        ),
        tool(
            "find_nodes",
            "Find nodes by name and kind.",
            json!({
                "name_contains": {"type": "string"},
                "kind": {"type": "string", "enum": ["frame", "group", "rect", "ellipse", "text", "image"]},
                "limit": {"type": "integer", "minimum": 1, "maximum": 200}
            }),
            &[],
            true,
        ),
        tool(
            "list_tokens",
            "Design tokens: named colors usable as paints like {brand.gold}.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "apply_ops",
            "Change the design with a list of operations, applied together or not at all. \
             Operations: {op:'create', parent, index?, node:{kind, name?, x, y, width, height, fill?, stroke?, \
             corner_radius?, opacity?, visible?, text?, font_size?, font_family?, font_weight?, text_align?, \
             line_height?, image?, image_fit?, children?}}, \
             {op:'set', id, prop:{name, value}} with name one of name, x, y, width, height, visible, opacity, \
             fill, stroke, corner_radius, text, font_size, font_family, font_weight, text_align, line_height, \
             image (a media/ path from list_media), image_fit (cover, contain or fill), {op:'move', id, x, y}, \
             {op:'reparent', id, parent, index?}, {op:'delete', id}, \
             {op:'set_token', name, value} (a named color '#rrggbb'; value null removes an unused token), \
             {op:'set_localized_text', locale, id, text} (a translation of a text node; text null removes it), \
             and for videos (see list_videos): {op:'create_video', name, fps, width, height} (even sizes), {op:'delete_video', id}, \
             {op:'set_video', id, prop:{name, value}} with name one of name, fps, size (value {width, height}), \
             audio (value {file:'media/x.mp3', start_ms?, gain? (0 to 2), fade_in_ms?, fade_out_ms?} for the sound under the picture; null takes it away), \
             {op:'add_clip', video, frame | footage:{file:'media/x.mp4', in_ms}, duration_ms, index?, transition?, motion?} (a clip shows a design frame or plays footage from list_footage), \
             {op:'remove_clip', video, clip}, {op:'move_clip', video, clip, index}, \
             {op:'set_clip', video, clip, prop:{name, value}} with name one of duration_ms, frame, \
             source (value {frame} or {footage:{file,in_ms}}), transition (value {kind: cut|fade|slide_left|slide_right|slide_up|slide_down|dip|zoom_in, duration_ms, ease?}), motion (value {scale_from, scale_to, ease?}, 1 to 3), \
             {op:'add_overlay', video, frame, start_ms, duration_ms, fade_ms?, enter?, exit?, ease?} (a design frame drawn over the whole video for a while: titles, lower thirds, call-outs; a frame with no fill is transparent; enter and exit are none, fade, slide_up, slide_down, slide_left, slide_right, pop or zoom, taking fade_ms each; ease is linear, ease_in, ease_out, ease_in_out or back (overshoots)), {op:'remove_overlay', video, overlay}, \
             {op:'set_overlay', video, overlay, prop:{name, value}} with name one of frame, start_ms, duration_ms, fade_ms, enter, exit, ease. Colors are '#rrggbb', '#rrggbbaa' or \
             '{token.name}'. Ids are minted for you; the result lists the created ids. A checkpoint is taken \
             before your first change in a turn so the user can restore it.",
            json!({
                "ops": {"type": "array", "minItems": 1, "maxItems": 1000, "items": {"type": "object"}},
                "turn": id("Groups related calls into one turn; omit and each call is its own turn."),
                "summary": id("One line, in the user's terms, naming what this turn does.")
            }),
            &["ops"],
            false,
        ),
        tool(
            "list_checkpoints",
            "Named points the design can be restored to.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "diff_checkpoint",
            "The nodes changed since a checkpoint (those that still exist).",
            json!({"checkpoint": {"type": "integer"}}),
            &["checkpoint"],
            true,
        ),
        tool(
            "restore_checkpoint",
            "Return the design to a checkpoint. This appends an undoable change; nothing is lost.",
            json!({"checkpoint": {"type": "integer"}}),
            &["checkpoint"],
            false,
        ),
        tool(
            "list_export_presets",
            "Store export presets: sizes, formats and rules.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "list_media",
            "Images the user has imported into the project (media/), with their size in pixels. Use \
             a path as the `image` of an image node. If the one you need is not here, use find_files \
             and import_file (when the user allows it) or ask the user to add it in the app.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "list_locales",
            "The languages of the design and what each still lacks, plus every text node with its \
             default text. Translate with apply_ops set_localized_text; export a language with the \
             `locale` argument of export, export_set and export_panorama.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "render_preview",
            "Render a frame to an image so you can look at the result.",
            json!({
                "frame": id("Id of a frame node."),
                "scale": {"type": "number", "minimum": 0.01, "maximum": 16},
                "max_side": {"type": "integer", "minimum": 16, "maximum": 2048}
            }),
            &["frame"],
            true,
        ),
        tool(
            "export",
            "Render a frame to exports/<file> in the project, validated against a store preset. \
             Nothing is written when the image does not meet the preset.",
            json!({
                "frame": id("Id of a frame node."),
                "file": id("File name ending in .png, for example shot-01.png."),
                "preset": id("A preset id from list_export_presets."),
                "scale": {"type": "number", "minimum": 0.01, "maximum": 16},
                "alpha": {"type": "boolean", "description": "Keep transparency (ignored when the preset forbids it)."},
                "locale": id("A language from list_locales: text nodes show that language; set files are named <prefix>-<locale>-NN.png.")
            }),
            &["frame", "file"],
            false,
        ),
        tool(
            "export_set",
            "Render several frames to exports/<prefix>-01.png, -02, ... in the order given, validated \
             as a set against a store preset (every image, and the count of images). Nothing is \
             written unless every image and the count meet the preset.",
            json!({
                "frames": {"type": "array", "items": id("Id of a frame node."), "minItems": 1, "maxItems": 50},
                "prefix": id("File name stem, for example shot (gives shot-01.png)."),
                "preset": id("A preset id from list_export_presets."),
                "scale": {"type": "number", "minimum": 0.01, "maximum": 16},
                "alpha": {"type": "boolean", "description": "Keep transparency (ignored when the preset forbids it)."},
                "locale": id("A language from list_locales: text nodes show that language; set files are named <prefix>-<locale>-NN.png.")
            }),
            &["frames", "prefix"],
            false,
        ),
        tool(
            "export_panorama",
            "Render one wide frame and cut it into `slices` equal columns, exports/<prefix>-01.png, \
             -02, ... so a background, ribbon or device can cross the seams between store \
             screenshots. The frame width (times scale) must divide evenly; each slice and the count \
             are validated against the preset before anything is written.",
            json!({
                "frame": id("Id of the wide frame."),
                "slices": {"type": "integer", "minimum": 1, "maximum": 50},
                "prefix": id("File name stem, for example shot (gives shot-01.png)."),
                "preset": id("A preset id from list_export_presets."),
                "scale": {"type": "number", "minimum": 0.01, "maximum": 16},
                "alpha": {"type": "boolean", "description": "Keep transparency (ignored when the preset forbids it)."},
                "locale": id("A language from list_locales: text nodes show that language; set files are named <prefix>-<locale>-NN.png.")
            }),
            &["frame", "slices", "prefix"],
            false,
        ),
        tool(
            "device_template",
            "A generic phone or tablet outline (body, screen, camera) as a node you can put in an \
             apply_ops create operation. The node named Screen is where a capture goes: pass an \
             `image` from list_media, or set `image` on Screen later.",
            json!({
                "kind": {"type": "string", "enum": ["phone", "phone-punch-hole", "tablet", "tablet-wide"]},
                "screen_width": {"type": "number", "minimum": 50, "maximum": 8192, "description": "Width of the screen in document units."},
                "image": id("A media/ path from list_media.")
            }),
            &["kind", "screen_width"],
            true,
        ),
        tool(
            "list_videos",
            "The project's videos: each is a timeline of clips that show design frames, with the \
             time every clip starts, its transition (which overlaps the clip before it) and its zoom. \
             A video may also have one sound track (the file, when it starts and its volume). \
             Change them with apply_ops (create_video, add_clip, set_clip, ...); look at one moment \
             with render_video_frame and write the MP4 with export_video.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "list_memory",
            "The agent memory of this repository: small text notes in .opendesigner/memory/ that \
             earlier runs left for the next one (brand facts, what the user corrected or liked, \
             lessons, decisions). Returns name, bytes, title (first line) and modified time. Read \
             the relevant ones with read_memory before you build.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "read_memory",
            "Read one memory note by name (for example brand.md or lessons.md).",
            json!({"name": id("A note name from list_memory: lowercase letters, digits, . _ -, ending .md or .txt.")}),
            &["name"],
            true,
        ),
        tool(
            "write_memory",
            "Leave a note for the next run, in .opendesigner/memory/ (a folder the user tracks in \
             git). Write what future work should know: brand facts (colors, fonts, copy) and where \
             they came from, what the user corrected or liked, what worked, mistakes to avoid, \
             decisions. Files like brand.md, lessons.md, decisions.md, style.md. Short, factual, \
             Markdown. NEVER store secrets, tokens or passwords. mode append (default) adds a dated \
             entry; replace rewrites the file (use it to merge and tidy). Limits: 64 KB per note, \
             200 notes, 2 MB total.",
            json!({
                "name": id("Note name: lowercase letters, digits, . _ -, ending .md or .txt."),
                "text": id("The note text (Markdown)."),
                "mode": {"type": "string", "enum": ["append", "replace"], "description": "append (default) or replace."},
            }),
            &["name", "text"],
            false,
        ),
        tool(
            "list_workspaces",
            "The workspaces of this repository: separate designs side by side (an app store set, a \
             reel, a landing page), each with its own pages, videos, notes and media. Says which \
             one is open; every other tool works on the open one.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "create_workspace",
            "Make a new, empty workspace and open it. Use one workspace per deliverable (for \
             example \"app store\" and \"instagram reel\") instead of putting different things on \
             the same page. Names are up to 40 letters, digits, spaces, - or _.",
            json!({"name": id("The workspace name.")}),
            &["name"],
            false,
        ),
        tool(
            "open_workspace",
            "Switch to another workspace of this repository (from list_workspaces). The open one is \
             saved first; the user sees the switch.",
            json!({"name": id("A name from list_workspaces.")}),
            &["name"],
            false,
        ),
        tool(
            "find_files",
            "Look for pictures, videos and sounds (.mp3 .wav .m4a .aac .ogg) anywhere under the user's \
             home folder (the user allows this; hidden folders and caches are skipped). Returns \
             paths, sizes and dates, newest first, at most 200. Use `import_file` to bring one \
             into the project.",
            json!({
                "kind": {"type": "string", "enum": ["image", "video", "audio", "any"], "description": "Default any."},
                "name_contains": {"type": "string", "description": "Part of the file name."},
                "folder": id("A folder to search instead of the whole home folder, for example ~/Pictures or an absolute path under your home folder.")
            }),
            &[],
            true,
        ),
        tool(
            "import_file",
            "Copy one picture, video or sound from the user's computer into this project's media/ folder \
             (it is checked to be a real PNG, JPEG, WebP, MP4/MOV, or MP3, WAV, M4A, AAC or OGG \
             first) and return its media/ reference. The original is never changed. An imported sound \
             can be the video's sound track: set_video prop {name: \"audio\", value: {file, start_ms, \
             gain, fade_in_ms, fade_out_ms}}.",
            json!({"path": id("Absolute path (or ~/...) of the file, from find_files.")}),
            &["path"],
            false,
        ),
        tool(
            "list_footage",
            "Videos the user imported into the project (media/): reference, size on disk and, when \
             ffmpeg is installed, length, picture size, frame rate and whether it has sound. Play \
             one in a video with add_clip footage:{file, in_ms}. Bring in another with find_files and import_file.",
            json!({}),
            &[],
            true,
        ),
        tool_net(
            "search_sounds",
            "Find music and sound effects on Freesound (https://freesound.org) for a video. \
             Returns each sound's id, name, length, license, author and a preview to listen to. \
             Freesound sounds are licensed by their authors, not by us: the license in the result \
             is the license the user must obey, and you must show the attribution the result \
             carries (title, author, license, link) wherever the sound is used. Prefer CC0. \
             Needs the user's Freesound token (the Connect tab); without one the call says so.",
            json!({
                "query": id("Words to search for, for example \"soft piano loop\"."),
                "duration_max_ms": {"type": "integer", "minimum": 1, "maximum": 600000, "description": "Keep only sounds no longer than this."},
                "page": {"type": "integer", "minimum": 1, "maximum": 20, "description": "Which page of results (1 is the first)."}
            }),
            &["query"],
            true,
        ),
        tool_net(
            "import_sound",
            "Download one Freesound sound (the preview recording, not the original file) into \
             this project's media/ folder and write what it is next to it. Returns the media/ \
             reference and the one line the user must show: \"Title by Author, License, link\". \
             The license must be honoured: say who made it, under which license, and where it \
             came from. The file is never overwritten. Then put it under a video with set_video \
             prop {name: \"audio\", value: {file, start_ms, gain, fade_in_ms, fade_out_ms}}.",
            json!({
                "id": {"type": "integer", "minimum": 1, "description": "The id of a sound from search_sounds."},
                "name": id("A plain file name for the sound, without the .mp3 ending.")
            }),
            &["id"],
            false,
        ),
        tool(
            "list_sounds",
            "All the sounds in this project's media/ folder (.mp3 .wav .m4a .aac .ogg), whether \
             made with make_sound, downloaded with import_sound or imported with import_file: the \
             file, size, length when ffmpeg is installed, where it came from (synthesized, \
             freesound or imported) and, for a Freesound sound, the attribution line the user must \
             show. Any of them can be the video's sound track (set_video audio).",
            json!({}),
            &[],
            true,
        ),
        tool(
            "make_sound",
            "Make a sound effect or a simple loop yourself, with no download and no licence to \
             honour (it is synthesized and written to media/<name>.wav, normalized, with short \
             fades so it never clicks). Use it for the basics: click, pop, tick, whoosh, \
             swoosh_up, swoosh_down, riser (builds into a cut), hit (impact: put it on a cut or a \
             reveal), chime (a notification or a finish), stinger (a 2 to 3 note sting), drone \
             (a soft pad that loops) and beat (4-on-the-floor kick and hat at `bpm`, `bars` bars). \
             It cannot make real music or speech: for a music bed use search_sounds (Freesound, \
             needs the user's token) or ask the user; for a voice-over use a speech MCP server if \
             the user added one (get_guidance lists them). The same kind, duration and seed give \
             the same sound. Then set it as the track: set_video prop {name: \"audio\", value: \
             {file, start_ms, gain, fade_in_ms, fade_out_ms}}.",
            json!({
                "kind": {"type": "string", "enum": synth::Kind::NAMES, "description": "Which sound."},
                "duration_ms": {"type": "integer", "minimum": synth::MIN_DURATION_MS, "maximum": synth::MAX_DURATION_MS, "description": "How long. Optional: each kind has a fitting default. A beat takes its length from bpm and bars."},
                "seed": {"type": "integer", "minimum": 0, "description": "Another seed gives another variation (noise, and the notes of a stinger). Default 1."},
                "pitch_hz": {"type": "number", "minimum": 20, "maximum": 8000, "description": "The base pitch of chime, drone, tick, pop, riser, hit and stinger."},
                "bpm": {"type": "number", "minimum": 40, "maximum": 240, "description": "Beat only: tempo, default 120."},
                "bars": {"type": "integer", "minimum": 1, "maximum": 16, "description": "Beat only: bars of four beats."},
                "stereo": {"type": "boolean", "description": "Two channels (default one)."},
                "name": id("A plain file name without the .wav ending; default is the kind. An existing file is never overwritten.")
            }),
            &["kind"],
            false,
        ),
        tool(
            "list_video_presets",
            "Store rules for videos (app previews): sizes, length, frame rate and file size, with the \
             page each was read from and what is not checked.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "render_video_frame",
            "Render one moment of a video to an image so you can look at the result.",
            json!({
                "video": id("Id of a video from list_videos."),
                "time_ms": {"type": "integer", "minimum": 0, "description": "Milliseconds from the start."},
                "max_side": {"type": "integer", "minimum": 16, "maximum": 2048},
                "locale": id("A language from list_locales.")
            }),
            &["video", "time_ms"],
            true,
        ),
        tool(
            "export_video",
            "Draw every frame of a video and write it as an H.264 MP4 to exports/<file>. Needs ffmpeg \
             on the user's computer (the error says what to install). This can take a while.",
            json!({
                "video": id("Id of a video from list_videos."),
                "file": id("File name ending in .mp4, for example preview.mp4."),
                "preset": id("A preset id from list_video_presets: the video is checked against the store's rules and nothing is written if it breaks one."),
                "locale": id("A language from list_locales: text nodes show that language.")
            }),
            &["video", "file"],
            false,
        ),
        tool(
            "validate_export",
            "Check exported PNGs in exports/ as one set against a store preset.",
            json!({"preset": id("A preset id."), "files": {"type": "array", "items": {"type": "string"}, "maxItems": 100}}),
            &["preset", "files"],
            true,
        ),
    ]
}

fn parse<T: DeserializeOwned>(args: Value) -> Result<T, ToolError> {
    let args = if args.is_null() { json!({}) } else { args };
    serde_json::from_value(args).map_err(|e| ToolError(format!("invalid arguments: {e}")))
}

fn persist(ws: &Workspace) -> Result<(), ToolError> {
    if ws.dir().is_some() {
        ws.save()?;
    }
    Ok(())
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "get_guidance" => get_guidance(ctx),
        "get_playbook" => get_playbook(parse(args)?),
        "review_frame" => review_frame_tool(ctx, parse(args)?),
        "list_formats" => Ok(Output::json(&crate::formats::sections_json())),
        "get_brief" => Ok(Output::json(&brief_of(ctx).to_json())),
        "set_format" => set_format(ctx, parse(args)?),
        "get_selection" => Ok(Output::json(&to_json(&ctx.ws.guidance().selection)?)),
        "get_annotations" => get_annotations(ctx, parse(args)?),
        "resolve_annotation" => resolve_annotation(ctx, parse(args)?),
        "list_pages" => list_pages(ctx),
        "get_node" => get_node(ctx, parse(args)?),
        "get_tree" => get_tree(ctx, parse(args)?),
        "find_nodes" => find_nodes(ctx, parse(args)?),
        "list_tokens" => Ok(Output::json(&to_json(&ctx.ws.session.document().tokens)?)),
        "apply_ops" => apply_ops(ctx, parse(args)?),
        "list_checkpoints" => Ok(Output::json(&to_json(ctx.ws.session.checkpoints())?)),
        "diff_checkpoint" => diff_checkpoint(ctx, parse(args)?),
        "restore_checkpoint" => restore_checkpoint(ctx, parse(args)?),
        "list_export_presets" => Ok(Output::json(&presets_json())),
        "list_media" => list_media(ctx),
        "list_videos" => Ok(Output::json(&to_json(&od_core::view::videos(
            ctx.ws.session.document(),
        ))?)),
        "list_video_presets" => Ok(Output::json(&video_presets_json())),
        "list_footage" => list_footage(ctx),
        "list_workspaces" => list_workspaces(ctx),
        "list_memory" => list_memory(ctx),
        "read_memory" => read_memory(ctx, parse(args)?),
        "write_memory" => write_memory(ctx, parse(args)?),
        "create_workspace" => switch_workspace(ctx, parse(args)?, true),
        "open_workspace" => switch_workspace(ctx, parse(args)?, false),
        "find_files" => find_files(ctx, parse(args)?),
        "import_file" => import_file(ctx, parse(args)?),
        "search_sounds" => search_sounds(parse(args)?),
        "import_sound" => import_sound(ctx, parse(args)?),
        "list_sounds" => list_sounds(ctx),
        "make_sound" => make_sound(ctx, parse(args)?),
        "render_video_frame" => render_video_frame(ctx, parse(args)?),
        "export_video" => export_video_tool(ctx, parse(args)?),
        "list_locales" => {
            let doc = ctx.ws.session.document();
            Ok(Output::json(&json!({
                "default_language_nodes": od_core::view::text_nodes(doc),
                "languages": od_core::view::locales(doc),
            })))
        }
        "render_preview" => render_preview(ctx, parse(args)?),
        "export" => export(ctx, parse(args)?),
        "export_set" => export_set(ctx, parse(args)?),
        "export_panorama" => export_panorama(ctx, parse(args)?),
        "device_template" => device_template(parse(args)?),
        "validate_export" => validate_export(ctx, parse(args)?),
        other => fail(format!("unknown tool {other:?}")),
    }
}

fn to_json<T: serde::Serialize + ?Sized>(v: &T) -> Result<Value, ToolError> {
    serde_json::to_value(v).map_err(|e| ToolError(e.to_string()))
}

fn list_media(ctx: &mut CallCtx) -> Result<Output, ToolError> {
    let files = match ctx.ws.dir() {
        Some(_) => ctx.ws.media()?,
        None => Vec::new(),
    };
    let listed: Vec<Value> = files
        .iter()
        .map(|f| {
            let size = ctx
                .ws
                .media_path(&f.path)
                .map_err(|e| e.to_string())
                .and_then(|p| od_render::image_dimensions(&p));
            match size {
                Ok((width, height)) => {
                    json!({"path": f.path, "bytes": f.bytes, "width": width, "height": height})
                }
                Err(reason) => json!({"path": f.path, "bytes": f.bytes, "unreadable": reason}),
            }
        })
        .collect();
    Ok(Output::json(&Value::Array(listed)))
}

fn node_id(s: &str) -> Result<NodeId, ToolError> {
    Ok(NodeId::parse(s)?)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnnotationsArgs {
    #[serde(default)]
    include_resolved: bool,
}

fn get_annotations(ctx: &mut CallCtx, a: AnnotationsArgs) -> Result<Output, ToolError> {
    let list: Vec<&od_core::Annotation> = ctx
        .ws
        .annotations
        .all()
        .iter()
        .filter(|n| a.include_resolved || n.status == AnnotationStatus::Open)
        .collect();
    Ok(Output::json(&to_json(&list)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveArgs {
    id: u64,
    #[serde(default)]
    note: Option<String>,
}

fn resolve_annotation(ctx: &mut CallCtx, a: ResolveArgs) -> Result<Output, ToolError> {
    ctx.ws
        .annotations
        .resolve(a.id, ctx.actor.clone(), a.note.as_deref())?;
    persist(ctx.ws)?;
    Ok(Output::text(format!("resolved annotation {}", a.id)))
}

fn node_summary(n: &Node) -> Value {
    json!({"id": n.id, "kind": n.kind.as_str(), "name": n.name,
           "x": n.x, "y": n.y, "width": n.width, "height": n.height})
}

fn list_pages(ctx: &mut CallCtx) -> Result<Output, ToolError> {
    let pages: Vec<Value> = ctx
        .ws
        .session
        .document()
        .pages
        .iter()
        .map(|p| {
            json!({
                "id": p.id, "name": p.name, "root": p.root.id,
                "nodes": p.root.children.iter().map(node_summary).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(Output::json(&Value::Array(pages)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeArgs {
    id: String,
    #[serde(default)]
    depth: Option<usize>,
}

fn node_json(n: &Node, depth: usize) -> Result<Value, ToolError> {
    let mut shallow = n.clone();
    let children = std::mem::take(&mut shallow.children);
    let mut v = to_json(&shallow)?;
    if let Value::Object(map) = &mut v {
        if depth > 0 {
            let kids: Result<Vec<Value>, ToolError> =
                children.iter().map(|c| node_json(c, depth - 1)).collect();
            if !children.is_empty() {
                map.insert("children".into(), Value::Array(kids?));
            }
        } else if !children.is_empty() {
            map.insert("child_count".into(), json!(children.len()));
        }
    }
    Ok(v)
}

fn get_node(ctx: &mut CallCtx, a: NodeArgs) -> Result<Output, ToolError> {
    let id = node_id(&a.id)?;
    let doc = ctx.ws.session.document();
    let node = doc
        .node(&id)
        .ok_or_else(|| ToolError(OpError::NoSuchNode(a.id.clone()).to_string()))?;
    let depth = a.depth.unwrap_or(2).min(MAX_DEPTH_ARG);
    Ok(Output::json(&node_json(node, depth)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeArgs {
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    depth: Option<usize>,
}

fn outline(n: &Node, depth: usize, max: usize, out: &mut String) {
    out.push_str(&format!(
        "{}{} {} {:?} at {},{} {}x{}\n",
        "  ".repeat(depth),
        n.id,
        n.kind.as_str(),
        n.name,
        n.x,
        n.y,
        n.width,
        n.height
    ));
    if depth + 1 >= max {
        if !n.children.is_empty() {
            out.push_str(&format!("{}...\n", "  ".repeat(depth + 1)));
        }
        return;
    }
    for c in &n.children {
        outline(c, depth + 1, max, out);
    }
}

fn get_tree(ctx: &mut CallCtx, a: TreeArgs) -> Result<Output, ToolError> {
    let doc = ctx.ws.session.document();
    let page = match &a.page {
        Some(p) => doc
            .page(&node_id(p)?)
            .ok_or_else(|| ToolError(OpError::NoSuchPage(p.clone()).to_string()))?,
        None => doc
            .pages
            .first()
            .ok_or_else(|| ToolError("the project has no pages".into()))?,
    };
    let max = a.depth.unwrap_or(4).clamp(1, MAX_DEPTH_ARG);
    let mut out = format!("page {} {:?} (root {})\n", page.id, page.name, page.root.id);
    for c in &page.root.children {
        outline(c, 1, max + 1, &mut out);
    }
    Ok(Output::text(out))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindArgs {
    #[serde(default)]
    name_contains: Option<String>,
    #[serde(default)]
    kind: Option<NodeKind>,
    #[serde(default)]
    limit: Option<usize>,
}

fn collect(n: &Node, page: &NodeId, a: &FindArgs, limit: usize, out: &mut Vec<Value>) {
    if out.len() >= limit {
        return;
    }
    let name_ok = a
        .name_contains
        .as_ref()
        .is_none_or(|s| n.name.to_lowercase().contains(&s.to_lowercase()));
    if name_ok && a.kind.is_none_or(|k| k == n.kind) {
        let mut v = node_summary(n);
        if let Value::Object(m) = &mut v {
            m.insert("page".into(), json!(page));
        }
        out.push(v);
    }
    for c in &n.children {
        collect(c, page, a, limit, out);
    }
}

fn find_nodes(ctx: &mut CallCtx, a: FindArgs) -> Result<Output, ToolError> {
    let limit = a.limit.unwrap_or(50).min(MAX_FIND_RESULTS);
    let mut out = Vec::new();
    for page in &ctx.ws.session.document().pages {
        for c in &page.root.children {
            collect(c, &page.id, &a, limit, &mut out);
        }
    }
    Ok(Output::json(&Value::Array(out)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyArgs {
    ops: Vec<Operation>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

fn brief_of(ctx: &CallCtx) -> crate::formats::Brief {
    ctx.ws
        .dir()
        .map(crate::formats::load_brief)
        .unwrap_or_default()
}

fn get_guidance(ctx: &mut CallCtx) -> Result<Output, ToolError> {
    let mut v = to_json(&ctx.ws.guidance())?;
    let brief = brief_of(ctx);
    v["brief"] = brief.to_json();
    v["before_you_build"] = json!(
        "Call get_playbook: design (images), style (pick a named look), video and motion (video), \
         formats (sizes). Review your work with review_frame and render_preview before you say it is done."
    );
    v["memory"] = memory_index(ctx);
    v["extra_mcp_servers"] = crate::mcp_servers::describe(ctx.extra_servers);
    v["sound"] = json!(crate::mcp_servers::SOUND_NOTE);
    Ok(Output::json(&v))
}

fn project_dir_of(ctx: &CallCtx) -> Result<std::path::PathBuf, ToolError> {
    ctx.ws
        .dir()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| {
            ToolError("the workspace has no project folder, so there is no memory".into())
        })
}

fn note_json(n: &crate::memory::Note) -> Value {
    json!({"name": n.name, "bytes": n.bytes, "title": n.title, "modified": n.modified})
}

/// Where the notes live, so the agent can tell the user where to look.
fn memory_folder(ctx: &CallCtx) -> Option<String> {
    let dir = ctx.ws.dir()?;
    Some(crate::memory::folder_of(dir)?.display().to_string())
}

/// The names and titles of the notes, never their bodies.
fn memory_index(ctx: &CallCtx) -> Value {
    let notes = project_dir_of(ctx)
        .map_err(|e| e.0)
        .and_then(|d| crate::memory::list(&d).map_err(|e| e.to_string()));
    match notes {
        Ok(notes) => json!({
            "notes": notes.iter().map(note_json).collect::<Vec<_>>(),
            "folder": memory_folder(ctx),
            "instruction": "Read the relevant notes with read_memory before you build, and write back \
                            what the next run should know (write_memory) before you finish. No secrets.",
        }),
        Err(e) => json!({"notes": [], "unavailable": e}),
    }
}

fn list_memory(ctx: &mut CallCtx) -> Result<Output, ToolError> {
    let notes = crate::memory::list(&project_dir_of(ctx)?)?;
    Ok(Output::json(&json!({
        "notes": notes.iter().map(note_json).collect::<Vec<_>>(),
        "folder": memory_folder(ctx),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryNameArgs {
    name: String,
}

fn read_memory(ctx: &mut CallCtx, a: MemoryNameArgs) -> Result<Output, ToolError> {
    Ok(Output::text(crate::memory::read(
        &project_dir_of(ctx)?,
        &a.name,
    )?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteMemoryArgs {
    name: String,
    text: String,
    #[serde(default)]
    mode: Option<String>,
}

fn write_memory(ctx: &mut CallCtx, a: WriteMemoryArgs) -> Result<Output, ToolError> {
    let mode = match a.mode.as_deref() {
        None | Some("append") => crate::memory::Mode::Append,
        Some("replace") => crate::memory::Mode::Replace,
        Some(other) => return fail(format!("mode is append or replace, not {other:?}")),
    };
    let note = crate::memory::write(&project_dir_of(ctx)?, &a.name, &a.text, mode)?;
    Ok(Output::json(&note_json(&note)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaybookArgs {
    #[serde(default)]
    topic: Option<String>,
}

fn get_playbook(a: PlaybookArgs) -> Result<Output, ToolError> {
    match a.topic {
        None => {
            let mut out = String::from("Playbooks (call get_playbook with a topic):\n\n");
            for (name, about, _) in crate::playbook::TOPICS {
                out.push_str(&format!("- {name}: {about}\n"));
            }
            out.push_str(&format!("\n{}\n", crate::playbook::QUALITY_BAR));
            Ok(Output::text(out))
        }
        Some(name) => crate::playbook::topic(&name)
            .map(Output::text)
            .ok_or_else(|| {
                ToolError(format!(
                    "no playbook {name:?}; topics: {}",
                    crate::playbook::TOPICS
                        .iter()
                        .map(|(n, _, _)| *n)
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewArgs {
    frame: NodeId,
}

/// What to tell an agent that is about to call a frame finished: frames that were never reviewed,
/// or changed after their last review. A nudge, not a gate: an export still goes through.
fn unreviewed(ctx: &CallCtx, frames: &[NodeId]) -> Vec<String> {
    fn collect(n: &od_core::Node, out: &mut HashSet<NodeId>) {
        out.insert(n.id.clone());
        n.children.iter().for_each(|c| collect(c, out));
    }
    let doc = ctx.ws.session.document();
    let log = ctx.ws.session.log();
    let mut notes = Vec::new();
    for f in frames {
        let Some(node) = doc.node(f) else { continue };
        let mut ids = HashSet::new();
        collect(node, &mut ids);
        let last_touch = log.iter().rposition(|tx| {
            tx.ops
                .iter()
                .any(|a| a.touched.iter().any(|t| ids.contains(t)))
        });
        match (ctx.turns.reviewed.get(f), last_touch) {
            (None, _) => notes.push(format!("{} was never checked with review_frame", node.name)),
            (Some(at), Some(i)) if i >= *at => {
                notes.push(format!("{} changed after its last review_frame", node.name));
            }
            _ => {}
        }
    }
    notes
}

fn with_review_note(ctx: &CallCtx, frames: &[NodeId], mut v: Value) -> Value {
    let notes = unreviewed(ctx, frames);
    if !notes.is_empty() {
        v["before_you_call_it_done"] = json!(format!(
            "{}. Run review_frame and look at render_preview, fix what they find, then export again.",
            notes.join("; ")
        ));
    }
    v
}

fn review_frame_tool(ctx: &mut CallCtx, a: ReviewArgs) -> Result<Output, ToolError> {
    let doc = ctx.ws.session.document();
    let mut v = crate::review::review_frame(doc, &a.frame).map_err(ToolError)?;
    ctx.turns
        .reviewed
        .insert(a.frame.clone(), ctx.ws.session.log().len());
    let brief = brief_of(ctx);
    if let (Some(f), Some(size)) = (
        brief.format.as_ref().filter(|f| f.kind == "image"),
        v["size"]
            .as_array()
            .map(|s| (s[0].as_f64().unwrap_or(0.0), s[1].as_f64().unwrap_or(0.0))),
    ) {
        let (w, h) = (f64::from(f.width), f64::from(f.height));
        // a panorama is a whole number of slides side by side
        let slides = (size.0 / w).round();
        let matches = (size.0 - w).abs() < 0.5 && (size.1 - h).abs() < 0.5
            || (slides >= 2.0 && (size.0 - slides * w).abs() < 0.5 && (size.1 - h).abs() < 0.5);
        if !matches && size.0 >= 200.0 {
            let by = if brief.chosen_by_user {
                "the user chose"
            } else {
                "you set"
            };
            if let Some(list) = v["findings"].as_array_mut() {
                list.push(json!({
                    "severity": if brief.chosen_by_user { "warn" } else { "info" },
                    "node": null, "name": null,
                    "message": format!("this frame is {}x{} but {by} {} ({}x{}) for the workspace", size.0, size.1, f.label, f.width, f.height),
                }));
            }
        }
    }
    Ok(Output::json(&v))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetFormatArgs {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    fps: Option<u32>,
    reason: String,
}

fn set_format(ctx: &mut CallCtx, a: SetFormatArgs) -> Result<Output, ToolError> {
    let dir = ctx
        .ws
        .dir()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| ToolError("the project has no folder to keep a format in".into()))?;
    let new = match (&a.format, &a.kind, a.width, a.height) {
        (Some(id), None, None, None) => crate::formats::find(id)
            .map(crate::formats::BriefFormat::from)
            .ok_or_else(|| ToolError(format!("no format {id:?}; call list_formats")))?,
        (None, Some(kind), Some(w), Some(h)) => {
            crate::formats::custom(kind, w, h, a.fps).map_err(ToolError)?
        }
        _ => {
            return Err(ToolError(
                "give either `format` (an id from list_formats) or `kind`, `width` and `height`"
                    .into(),
            ));
        }
    };
    let mut brief = crate::formats::load_brief(&dir);
    let label = new.label.clone();
    let outcome = brief.agent_set(new, &a.reason);
    crate::formats::save_brief(&dir, &brief).map_err(ToolError)?;
    Ok(Output::json(&match outcome {
        crate::formats::SetOutcome::Applied => json!({
            "applied": true, "format": brief.format,
            "next": "Say what you chose and why in one line, then build at this size.",
        }),
        crate::formats::SetOutcome::NeedsApproval => json!({
            "applied": false,
            "waiting_for_user": true,
            "message": format!("The user chose {} for this workspace. They have been asked whether to change it to {label}. Stop and tell them what you would change and why; do not build at the new size until get_brief shows it approved.", brief.format.as_ref().map_or("a format", |f| f.label.as_str())),
        }),
    }))
}

fn check_brief(ctx: &CallCtx, ops: &[Operation]) -> Result<(), ToolError> {
    let brief = brief_of(ctx);
    if !brief.chosen_by_user {
        return Ok(());
    }
    for op in ops {
        let verdict = match op {
            Operation::CreateVideo {
                width, height, fps, ..
            } => brief.check_video(Some(*width), Some(*height), Some(*fps)),
            Operation::SetVideo {
                prop: od_core::VideoProperty::Size { width, height },
                ..
            } => brief.check_video(Some(*width), Some(*height), None),
            Operation::SetVideo {
                prop: od_core::VideoProperty::Fps(r),
                ..
            } => brief.check_video(None, None, Some(*r)),
            _ => Ok(()),
        };
        verdict.map_err(ToolError)?;
    }
    Ok(())
}

fn apply_ops(ctx: &mut CallCtx, a: ApplyArgs) -> Result<Output, ToolError> {
    check_brief(ctx, &a.ops)?;
    let turn = match a.turn {
        Some(t) => t,
        None => {
            ctx.turns.counter += 1;
            format!("t{}", ctx.turns.counter)
        }
    };
    let key = format!("{}/{turn}", ctx.actor);
    if ctx.turns.seen.insert(key.clone()) {
        let name = match &a.summary {
            Some(s) => format!("before: {s}"),
            None => format!("before {turn}"),
        };
        let name: String = name.chars().take(od_core::limits::MAX_NAME_LEN).collect();
        let id = ctx.ws.session.checkpoint(ctx.actor.clone(), &name)?;
        ctx.turns.checkpoint_by_turn.insert(key.clone(), id);
    }
    let checkpoint = ctx.turns.checkpoint_by_turn.get(&key).copied();
    let receipt = ctx
        .ws
        .session
        .apply(ctx.actor.clone(), Some(turn.clone()), &a.ops)?;
    persist(ctx.ws)?;
    Ok(Output::json(&json!({
        "transaction": receipt.id,
        "turn": turn,
        "checkpoint_before": checkpoint,
        "created": receipt.created,
        "touched": receipt.touched,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointArgs {
    checkpoint: u64,
}

fn diff_checkpoint(ctx: &mut CallCtx, a: CheckpointArgs) -> Result<Output, ToolError> {
    let position = ctx
        .ws
        .session
        .checkpoints()
        .iter()
        .find(|c| c.id == a.checkpoint)
        .map(|c| c.position)
        .ok_or_else(|| ToolError(OpError::NoSuchCheckpoint(a.checkpoint).to_string()))?;
    let changed = ctx.ws.session.changed_since(position);
    Ok(Output::json(
        &json!({"checkpoint": a.checkpoint, "changed": changed}),
    ))
}

fn restore_checkpoint(ctx: &mut CallCtx, a: CheckpointArgs) -> Result<Output, ToolError> {
    let receipt = ctx.ws.session.restore(ctx.actor.clone(), a.checkpoint)?;
    persist(ctx.ws)?;
    Ok(Output::json(&json!({
        "restored": a.checkpoint,
        "transaction": receipt.id,
        "touched": receipt.touched,
    })))
}

fn presets_json() -> Value {
    let list: Vec<Value> = od_store::PRESETS
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "description": p.description,
                "size": p.describe_size(),
                "formats": p.formats.iter().map(|f| f.name()).collect::<Vec<_>>(),
                "allow_alpha": p.allow_alpha,
                "min_count": p.min_count,
                "max_count": p.max_count,
                "source": p.source,
                "verified_against_primary_source":
                    matches!(p.verification, od_store::Verification::PrimaryChecked(_)),
            })
        })
        .collect();
    Value::Array(list)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewArgs {
    frame: String,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    max_side: Option<u32>,
}

fn render_preview(ctx: &mut CallCtx, a: PreviewArgs) -> Result<Output, ToolError> {
    let id = node_id(&a.frame)?;
    let doc = ctx.ws.session.document();
    let node = doc
        .node(&id)
        .ok_or_else(|| ToolError(OpError::NoSuchNode(a.frame.clone()).to_string()))?;
    let scale = match a.scale {
        Some(s) => s,
        None => {
            let side = a
                .max_side
                .map_or(DEFAULT_PREVIEW_SIDE, f64::from)
                .min(MAX_PREVIEW_SIDE);
            let longest = node.width.max(node.height).max(1.0);
            // render sizes round up, so aim half a pixel under the limit
            if longest <= side {
                1.0
            } else {
                ((side - 0.5) / longest) as f32
            }
        }
    };
    let rendered = od_render::render_frame(doc, &id, scale, ctx.ws.dir())?;
    let png = od_render::encode_png(&rendered.pixmap, false)?;
    let mut note = format!(
        "{}x{} preview of {} at scale {scale:.3}",
        rendered.pixmap.width(),
        rendered.pixmap.height(),
        a.frame
    );
    if !rendered.warnings.is_empty() {
        note.push_str(&format!(
            "; warnings: {}",
            rendered
                .warnings
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        ));
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportArgs {
    frame: String,
    file: String,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    alpha: bool,
    #[serde(default)]
    locale: Option<String>,
}

fn export(ctx: &mut CallCtx, a: ExportArgs) -> Result<Output, ToolError> {
    let id = node_id(&a.frame)?;
    let plan = Plan::Frame {
        frame: &id,
        file: &a.file,
    };
    let out = export_assets(
        ctx.ws,
        &plan,
        a.preset.as_deref(),
        a.scale,
        a.alpha,
        a.locale.as_deref(),
    )?;
    let Some(file) = out.files.first() else {
        return fail("nothing was exported");
    };
    let v = json!({
        "path": file.path,
        "width": file.width,
        "height": file.height,
        "bytes": file.bytes,
        "preset": a.preset,
        "preset_verified_against_primary_source": out.preset_verified,
        "warnings": out.warnings,
    });
    Ok(Output::json(&with_review_note(ctx, &[id], v)))
}

fn set_json(out: &ExportOutcome, preset: Option<&str>) -> Value {
    let files: Vec<Value> = out
        .files
        .iter()
        .map(|f| json!({"path": f.path, "width": f.width, "height": f.height, "bytes": f.bytes}))
        .collect();
    json!({
        "files": files,
        "preset": preset,
        "preset_verified_against_primary_source": out.preset_verified,
        "warnings": out.warnings,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportSetArgs {
    frames: Vec<String>,
    prefix: String,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    alpha: bool,
    #[serde(default)]
    locale: Option<String>,
}

fn export_set(ctx: &mut CallCtx, a: ExportSetArgs) -> Result<Output, ToolError> {
    let frames = a
        .frames
        .iter()
        .map(|f| node_id(f))
        .collect::<Result<Vec<_>, _>>()?;
    let plan = Plan::Set {
        frames: &frames,
        prefix: &a.prefix,
    };
    let out = export_assets(
        ctx.ws,
        &plan,
        a.preset.as_deref(),
        a.scale,
        a.alpha,
        a.locale.as_deref(),
    )?;
    let v = set_json(&out, a.preset.as_deref());
    Ok(Output::json(&with_review_note(ctx, &frames, v)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PanoramaArgs {
    frame: String,
    slices: u32,
    prefix: String,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    alpha: bool,
    #[serde(default)]
    locale: Option<String>,
}

fn export_panorama(ctx: &mut CallCtx, a: PanoramaArgs) -> Result<Output, ToolError> {
    let id = node_id(&a.frame)?;
    let plan = Plan::Panorama {
        frame: &id,
        slices: a.slices,
        prefix: &a.prefix,
    };
    let out = export_assets(
        ctx.ws,
        &plan,
        a.preset.as_deref(),
        a.scale,
        a.alpha,
        a.locale.as_deref(),
    )?;
    let v = set_json(&out, a.preset.as_deref());
    Ok(Output::json(&with_review_note(ctx, &[id], v)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceArgs {
    kind: String,
    screen_width: f64,
    #[serde(default)]
    image: Option<String>,
}

fn device_template(a: DeviceArgs) -> Result<Output, ToolError> {
    let Some(device) = od_store::devices::Device::parse(&a.kind) else {
        let ids: Vec<&str> = od_store::devices::Device::ALL
            .iter()
            .map(|d| d.id())
            .collect();
        return fail(format!(
            "unknown device {:?}; available: {}",
            a.kind,
            ids.join(", ")
        ));
    };
    if !(a.screen_width.is_finite() && (50.0..=8192.0).contains(&a.screen_width)) {
        return fail("screen_width must be between 50 and 8192");
    }
    let node = od_store::devices::template(device, a.screen_width, a.image.as_deref());
    Ok(Output::json(&json!({
        "node": node,
        "usage": "Put this node in an apply_ops create operation (set parent, then x and y on the \
                  group to place it). Swap the capture later by setting `image` on the node named \
                  Screen.",
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateArgs {
    preset: String,
    files: Vec<String>,
}

fn validate_export(ctx: &mut CallCtx, a: ValidateArgs) -> Result<Output, ToolError> {
    if a.files.len() > MAX_VALIDATE_FILES {
        return fail(format!("at most {MAX_VALIDATE_FILES} files"));
    }
    let p = preset(&a.preset)?;
    let dir = exports_dir(ctx.ws)?;
    let mut facts = Vec::with_capacity(a.files.len());
    for f in &a.files {
        let path = dir.join(export_name(f)?);
        let size = std::fs::metadata(&path)?.len();
        if size > MAX_IMAGE_BYTES {
            return fail(format!("{f} is larger than {MAX_IMAGE_BYTES} bytes"));
        }
        let bytes = std::fs::read(&path)?;
        facts.push(od_store::inspect_png(&bytes).map_err(|e| ToolError(format!("{f}: {e}")))?);
    }
    let violations = p.validate_set(&facts);
    let list: Vec<Value> = violations
        .iter()
        .map(|(i, v)| match i {
            Some(i) => json!({"file": a.files[*i], "problem": v.to_string()}),
            None => json!({"file": null, "problem": v.to_string()}),
        })
        .collect();
    Ok(Output::json(&json!({
        "preset": p.id,
        "ok": violations.is_empty(),
        "violations": list,
        "source": p.source,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VideoFrameArgs {
    video: String,
    time_ms: u64,
    #[serde(default)]
    max_side: Option<u32>,
    #[serde(default)]
    locale: Option<String>,
}

fn render_video_frame(ctx: &mut CallCtx, a: VideoFrameArgs) -> Result<Output, ToolError> {
    let id = node_id(&a.video)?;
    let (png, width, height, warnings) = crate::video_export::video_frame_png(
        &crate::video_export::Source::of(ctx.ws),
        &id,
        a.time_ms,
        a.locale.as_deref(),
        a.max_side.unwrap_or(768),
    )?;
    let mut note = format!(
        "{width}x{height} picture of {} at {} ms",
        a.video, a.time_ms
    );
    if !warnings.is_empty() {
        note.push_str(&format!("; warnings: {}", warnings.join("; ")));
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportVideoArgs {
    video: String,
    file: String,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    locale: Option<String>,
}

fn export_video_tool(ctx: &mut CallCtx, a: ExportVideoArgs) -> Result<Output, ToolError> {
    run_export_video(&crate::video_export::Source::of(ctx.ws), a, &mut |_, _| {
        true
    })
}

/// Runs `export_video` on a copy of the project (see [`crate::video_export::Source`]) so the
/// caller need not hold the editor while it works. `progress(done, total)` returns false to stop.
pub fn export_video_detached(
    src: &crate::video_export::Source,
    args: Value,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<Output, ToolError> {
    run_export_video(src, parse(args)?, progress)
}

fn run_export_video(
    src: &crate::video_export::Source,
    a: ExportVideoArgs,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<Output, ToolError> {
    let id = node_id(&a.video)?;
    crate::video_export::check_export(src, &id, &a.file, a.locale.as_deref(), a.preset.as_deref())?;
    let ffmpeg = od_media::Ffmpeg::locate().map_err(|e| ToolError(e.to_string()))?;
    let out = crate::video_export::export_video(
        src,
        &id,
        &a.file,
        a.locale.as_deref(),
        a.preset.as_deref(),
        &ffmpeg,
        progress,
    )?;
    Ok(Output::json(&video_outcome_json(&out)))
}

/// The result of a video export as the tools and the CLI report it.
pub fn video_outcome_json(out: &crate::video_export::VideoOutcome) -> Value {
    json!({
        "path": out.path,
        "width": out.width,
        "height": out.height,
        "fps": out.fps,
        "frames": out.frames,
        "duration_ms": out.duration_ms,
        "bytes": out.bytes,
        "preset": out.preset,
        "preset_rules_read_on": out.preset_checked,
        "preset_does_not_check": out.preset_not_checked,
        "encoder": out.encoder,
        "ffmpeg": out.ffmpeg,
        "ffmpeg_is_gpl_build": out.ffmpeg_gpl,
        "footage_sounds_mixed": out.audio_pieces,
        "warnings": out.warnings,
    })
}

fn video_presets_json() -> Value {
    Value::Array(
        od_store::video::VIDEO_PRESETS
            .iter()
            .map(|p| {
                json!({
                    "id": p.id,
                    "description": p.description,
                    "sizes": p.sizes.iter().map(|(w, h)| format!("{w}x{h}")).collect::<Vec<_>>(),
                    "min_seconds": p.min_ms / 1000,
                    "max_seconds": p.max_ms / 1000,
                    "max_fps": p.max_fps,
                    "max_megabytes": p.max_bytes / 1_000_000,
                    "source": p.source,
                    "rules_read_on": p.checked,
                    "not_checked": p.not_checked,
                })
            })
            .collect(),
    )
}

fn list_footage(ctx: &mut CallCtx) -> Result<Output, ToolError> {
    let Some(dir) = ctx.ws.dir().map(std::path::Path::to_path_buf) else {
        return Ok(Output::json(&json!([])));
    };
    let ffmpeg = od_media::Ffmpeg::locate().ok();
    let files = od_core::media::list_footage(&dir)?;
    let rows: Vec<Value> = files
        .iter()
        .map(|f| {
            let info = ffmpeg.as_ref().and_then(|ff| {
                let path = od_media::footage_path(&dir, &f.path).ok()?;
                ff.probe_file(&path).ok()
            });
            match info {
                Some(i) => json!({
                    "file": f.path, "bytes": f.bytes, "duration_ms": i.duration_ms,
                    "width": i.width, "height": i.height, "fps": i.fps, "has_audio": i.has_audio,
                }),
                None => json!({"file": f.path, "bytes": f.bytes, "note": "ffmpeg is not available, so its length and size are unknown"}),
            }
        })
        .collect();
    Ok(Output::json(&Value::Array(rows)))
}

fn expand_home(path: &str) -> Result<std::path::PathBuf, ToolError> {
    let home =
        crate::device::home().ok_or_else(|| ToolError("cannot find the home folder".into()))?;
    let p = match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if path == "~" => home.clone(),
        None => std::path::PathBuf::from(path),
    };
    if !p.is_absolute() {
        return fail("give an absolute path (or one starting with ~/)");
    }
    Ok(p)
}

fn device_enabled() -> Result<(), ToolError> {
    if crate::device::enabled() {
        Ok(())
    } else {
        fail(
            "the user turned off the agent's access to files outside the project (Connect tab); ask them to add the file in the app instead",
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindFilesArgs {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    name_contains: Option<String>,
    #[serde(default)]
    folder: Option<String>,
}

fn find_files(_ctx: &mut CallCtx, a: FindFilesArgs) -> Result<Output, ToolError> {
    device_enabled()?;
    let kind = crate::device::Kind::parse(a.kind.as_deref().unwrap_or("any"))
        .ok_or_else(|| ToolError("kind is image, video, audio or any".into()))?;
    let home =
        crate::device::home().ok_or_else(|| ToolError("cannot find the home folder".into()))?;
    let root = match &a.folder {
        Some(f) => {
            let p = expand_home(f)?;
            let real = std::fs::canonicalize(&p).map_err(|e| ToolError(format!("{f}: {e}")))?;
            let home_real = std::fs::canonicalize(&home).unwrap_or(home);
            if !real.starts_with(&home_real) {
                return fail("search inside the user's home folder only");
            }
            real
        }
        None => home,
    };
    let found = crate::device::find(&[root], kind, a.name_contains.as_deref().unwrap_or(""));
    let rows: Vec<Value> = found
        .iter()
        .map(|f| json!({"path": f.path.display().to_string(), "kind": f.kind, "bytes": f.bytes, "modified_unix": f.modified_secs}))
        .collect();
    Ok(Output::json(&json!({"count": rows.len(), "files": rows})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportFileArgs {
    path: String,
}

fn import_file(ctx: &mut CallCtx, a: ImportFileArgs) -> Result<Output, ToolError> {
    device_enabled()?;
    let p = expand_home(&a.path)?;
    let real = std::fs::canonicalize(&p).map_err(|e| ToolError(format!("{}: {e}", a.path)))?;
    if !real.is_file() {
        return fail(format!("{} is not a file", a.path));
    }
    let is_audio = real.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        od_core::media::AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())
    });
    let reference = if is_audio {
        let dir = ctx
            .ws
            .dir()
            .map(std::path::Path::to_path_buf)
            .ok_or_else(|| {
                ToolError("the project has no folder, so nowhere to put the sound".into())
            })?;
        crate::device::import_audio(&dir, &real).map_err(ToolError)?
    } else {
        match ctx.ws.import_media(&real) {
            Ok(r) => r,
            Err(_) => ctx.ws.import_footage(&real)?,
        }
    };
    Ok(Output::json(
        &json!({"reference": reference, "copied_from": real.display().to_string()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchSoundsArgs {
    query: String,
    #[serde(default)]
    duration_max_ms: Option<u32>,
    #[serde(default)]
    page: Option<u32>,
}

fn search_sounds(a: SearchSoundsArgs) -> Result<Output, ToolError> {
    let sounds = crate::freesound::search(&a.query, a.duration_max_ms, a.page.unwrap_or(1))
        .map_err(|e| ToolError(e.to_string()))?;
    let rows: Vec<Value> = sounds
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "duration_ms": s.duration_ms,
                "license": s.license,
                "license_url": s.license_url,
                "author": s.author,
                "preview_url": s.preview_url,
                "source_url": s.source_url,
                "attribution": s.attribution(),
            })
        })
        .collect();
    Ok(Output::json(&json!({"count": rows.len(), "sounds": rows})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportSoundArgs {
    id: u64,
    #[serde(default)]
    name: Option<String>,
}

fn import_sound(ctx: &mut CallCtx, a: ImportSoundArgs) -> Result<Output, ToolError> {
    let dir = ctx
        .ws
        .dir()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| {
            ToolError("the project has no folder, so nowhere to put the sound".into())
        })?;
    let imported = crate::freesound::import(&dir, a.id, a.name.as_deref())
        .map_err(|e| ToolError(e.to_string()))?;
    Ok(Output::json(&json!({
        "file": imported.file,
        "sidecar": imported.sidecar,
        "attribution": imported.attribution,
        "note": "show this attribution wherever the sound is used",
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MakeSoundArgs {
    kind: String,
    #[serde(default)]
    duration_ms: Option<u32>,
    #[serde(default)]
    seed: Option<u64>,
    #[serde(default)]
    pitch_hz: Option<f32>,
    #[serde(default)]
    bpm: Option<f32>,
    #[serde(default)]
    bars: Option<u32>,
    #[serde(default)]
    stereo: bool,
    #[serde(default)]
    name: Option<String>,
}

fn make_sound(ctx: &mut CallCtx, a: MakeSoundArgs) -> Result<Output, ToolError> {
    let kind = synth::Kind::parse(&a.kind)
        .ok_or_else(|| ToolError(format!("kind is one of {}", synth::Kind::NAMES.join(", "))))?;
    let dir = ctx
        .ws
        .dir()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| {
            ToolError("the project has no folder, so nowhere to put the sound".into())
        })?;
    let spec = synth::Spec {
        kind,
        duration_ms: a.duration_ms.unwrap_or_else(|| kind.default_ms()),
        seed: a.seed.unwrap_or(1),
        pitch_hz: a.pitch_hz,
        bpm: a.bpm,
        bars: a.bars,
        stereo: a.stereo,
    };
    let made = synth::make(&dir, &spec, a.name.as_deref()).map_err(ToolError)?;
    Ok(Output::json(&json!({
        "file": made.file,
        "sidecar": made.sidecar,
        "duration_ms": made.duration_ms,
        "channels": made.channels,
        "bytes": made.bytes,
        "source": "synthesized",
        "note": "synthesized here: no licence, nothing to attribute. Set it with set_video prop {name: \"audio\", value: {file, start_ms, gain, fade_in_ms, fade_out_ms}}.",
    })))
}

fn list_sounds(ctx: &mut CallCtx) -> Result<Output, ToolError> {
    let Some(dir) = ctx.ws.dir().map(std::path::Path::to_path_buf) else {
        return Ok(Output::json(&json!([])));
    };
    let ffmpeg = od_media::Ffmpeg::locate().ok();
    let media = dir.join("media");
    let rows: Vec<Value> = crate::device::list_audio(&dir)
        .into_iter()
        .map(|(file, bytes)| {
            let name = file.trim_start_matches("media/");
            let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
            let freesound: Option<crate::freesound::Sidecar> =
                std::fs::read_to_string(media.join(format!("{stem}.freesound.json")))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok());
            let synthesized = media
                .join(format!("{stem}{}", synth::SIDECAR_SUFFIX))
                .is_file();
            let mut row = json!({"file": file, "bytes": bytes});
            if let Some(ms) = ffmpeg.as_ref().and_then(|ff| {
                let path = od_media::audio_path(&dir, &file).ok()?;
                ff.probe_duration(&path).ok()
            }) {
                row["duration_ms"] = json!(ms);
            }
            if let Some(side) = freesound {
                row["source"] = json!("freesound");
                row["sidecar"] = json!(format!("media/{stem}.freesound.json"));
                row["attribution"] = json!(side.attribution());
            } else if synthesized {
                row["source"] = json!("synthesized");
                row["sidecar"] = json!(format!("media/{stem}{}", synth::SIDECAR_SUFFIX));
            } else {
                row["source"] = json!("imported");
            }
            row
        })
        .collect();
    Ok(Output::json(&Value::Array(rows)))
}

fn list_workspaces(ctx: &mut CallCtx) -> Result<Output, ToolError> {
    let Some(dir) = ctx.ws.dir().map(std::path::Path::to_path_buf) else {
        return Ok(Output::json(&json!({"open": null, "workspaces": []})));
    };
    let rows: Vec<Value> = crate::workspaces::list(&dir)
        .iter()
        .map(|e| {
            let doc = od_core::project::load(&e.path).ok();
            json!({
                "name": e.name,
                "open": e.path == dir,
                "pages": doc.as_ref().map(|s| s.document().pages.len()),
                "frames": doc.as_ref().map(|s| od_core::view::frames(s.document()).len()),
                "videos": doc.as_ref().map(|s| s.document().videos.len()),
            })
        })
        .collect();
    Ok(Output::json(
        &json!({"open": crate::workspaces::name_of(&dir), "workspaces": rows}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceArgs {
    name: String,
}

fn switch_workspace(
    ctx: &mut CallCtx,
    a: WorkspaceArgs,
    create: bool,
) -> Result<Output, ToolError> {
    let dir = ctx
        .ws
        .dir()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| ToolError("the project has no folder, so no workspaces".into()))?;
    let next = if create {
        crate::workspaces::create(&dir, &a.name).map_err(ToolError)?
    } else {
        let path = crate::workspaces::find(&dir, &a.name).ok_or_else(|| {
            let names: Vec<String> = crate::workspaces::list(&dir)
                .into_iter()
                .map(|e| e.name)
                .collect();
            ToolError(format!(
                "no workspace {:?}; there are: {}",
                a.name,
                names.join(", ")
            ))
        })?;
        Workspace::open(&path)?
    };
    ctx.ws.save()?;
    *ctx.ws = next;
    Ok(Output::json(&json!({"open": a.name, "created": create})))
}

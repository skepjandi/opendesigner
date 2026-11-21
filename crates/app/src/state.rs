//! The editor's state and actions, without Qt. The QObject in `editor.rs` forwards to this and
//! publishes the results as properties; QML presents them. Everything that decides something about
//! the document lives here or in the core crates (AGENTS.md invariant 2).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::chat::{Chat, ChatHook, ChatInput, split_command};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use od_agent::acp::{AgentCommand, McpEndpoint};
use od_agent::connect;
use od_agent::export::{Plan, export_assets};
use od_agent::mcp::{ChangeEvent, Handler, JobHook, JobState, JobUpdate, Jobs};
use od_agent::skill;
use od_agent::{RunningServer, token};
use od_core::annotations::AnnotationTarget;
use od_core::geometry::{self, Rect};
use od_core::view;
use od_core::{
    Actor, Device3d, DeviceModel, Footage, NewNode, Node, NodeId, NodeKind, Operation, Paint,
    Property, Workspace,
};
use od_store::devices::{self, Device};
use serde_json::{Value, json};

const MAX_CANVAS_SIDE: f64 = 4096.0;
const LOG_ROWS: usize = 60;

type Res<T> = Result<T, String>;

mod arrange_ops;

struct Server {
    // The runtime owns the server's tasks; it must outlive `running`.
    runtime: tokio::runtime::Runtime,
    running: Option<RunningServer>,
    url: String,
    token_file: PathBuf,
    jobs: Arc<Jobs>,
    handler: Arc<Handler>,
}

pub struct EditorState {
    ws: Option<Arc<Mutex<Workspace>>>,
    active_frame: Option<NodeId>,
    /// The language the canvas, the inspector's text and exports use; `None` is the default text.
    language: Option<String>,
    chat: Option<Chat>,
    agent_changed: BTreeSet<NodeId>,
    server: Option<Server>,
    /// The folder the user chose to work in (usually a repository); the agent starts there. The
    /// project itself lives in `<repo>/opendesigner`.
    repo: Option<PathBuf>,
    /// Work in progress the window shows as bars and spinners, by job id.
    jobs: std::collections::BTreeMap<u64, JobRow>,
    next_local_job: u64,
    /// The workspace the window last showed, to notice when an agent switches it.
    known_dir: Option<PathBuf>,
    /// Where the panels are (`layout.rs`) and the file that keeps it between runs.
    layout: crate::layout::Layout,
    layout_file: Option<PathBuf>,
    /// The frames of the video the Video tab plays, drawn on a worker thread.
    preview: Option<PreviewSlot>,
    /// The Preview and Variants panels: the user's choice of preview and drawn thumbnails.
    showcase: crate::showcase::ShowcaseSession,
    /// What the user copied (`od_core::arrange::Clip` as JSON), kept in memory.
    clipboard: Option<String>,
    /// The picture of the open frame, kept so an edit redraws only what it touched.
    canvas: Mutex<crate::canvas::Canvas>,
    /// The sound mixed for the video the Video tab plays.
    audio: crate::preview::AudioSlot,
}

/// A running or finished preview and what it was made from: it is stale when the project has
/// had an edit since (every change, the user's or an agent's, adds to the log).
struct PreviewSlot {
    video: String,
    log_len: usize,
    preview: od_agent::video_preview::Preview,
}

/// A job on the window's progress strip: `total` 0 means "working, no percentage".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRow {
    pub label: String,
    pub done: u64,
    pub total: u64,
    pub cancellable: bool,
}

/// The project folder inside a chosen working folder.
pub const PROJECT_DIR: &str = "opendesigner";
const RECENT_FILE: &str = "recent.json";
const MAX_RECENT: usize = 8;

/// Everything the panels show, as JSON strings QML parses.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Snapshot {
    pub has_project: bool,
    pub project_name: String,
    pub layers: String,
    pub frames: String,
    pub active_frame: String,
    pub frame_size: String,
    pub selection: String,
    pub selection_bounds: String,
    pub inspector: String,
    pub annotations: String,
    pub checkpoints: String,
    pub log: String,
    pub changed: String,
    pub unrendered: String,
    pub overflows: String,
    pub memory: String,
    pub videos: String,
    pub jobs: String,
    pub workspaces: String,
    pub brief: String,
    pub showcase: String,
    pub tokens: String,
    pub languages: String,
    pub language: String,
    pub chat: String,
    pub can_undo: bool,
    pub can_redo: bool,
    pub selection_rects: String,
    pub can_paste: bool,
    pub can_group: bool,
    pub can_ungroup: bool,
    pub mcp_running: bool,
    pub mcp_url: String,
}

fn rect_json(r: Option<Rect>) -> Value {
    match r {
        Some(r) => json!({"x": r.x, "y": r.y, "width": r.width, "height": r.height}),
        None => Value::Null,
    }
}

/// Whether the file is a video the editor can play (mp4, mov or m4v).
pub fn is_footage(src: &std::path::Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 12];
    let Ok(mut f) = std::fs::File::open(src) else {
        return false;
    };
    let Ok(n) = f.read(&mut head) else {
        return false;
    };
    od_core::media::sniff_footage(&head[..n])
}

fn parse_id(s: &str) -> Res<NodeId> {
    NodeId::parse(s).map_err(|e| e.to_string())
}

impl Default for EditorState {
    fn default() -> Self {
        EditorState::new()
    }
}

impl EditorState {
    pub fn new() -> EditorState {
        EditorState {
            ws: None,
            active_frame: None,
            language: None,
            chat: None,
            agent_changed: BTreeSet::new(),
            server: None,
            repo: None,
            jobs: Default::default(),
            next_local_job: 1_000_000,
            known_dir: None,
            layout: crate::layout::Layout::default(),
            layout_file: None,
            preview: None,
            showcase: Default::default(),
            clipboard: None,
            canvas: Mutex::default(),
            audio: Default::default(),
        }
    }

    fn with_ws<R>(&self, f: impl FnOnce(&mut Workspace) -> Res<R>) -> Res<R> {
        let ws = self.ws.as_ref().ok_or("no project is open")?;
        let mut guard = ws.lock().map_err(|_| "the editor state is unavailable")?;
        f(&mut guard)
    }

    fn persist(&self) -> Res<()> {
        self.with_ws(|ws| ws.save().map_err(|e| e.to_string()))
    }

    // ---- projects ----

    fn adopt(&mut self, ws: Workspace) {
        self.chat_stop();
        self.stop_mcp();
        // the agent's memory folder: best effort, never a reason to refuse to open
        if let Some(dir) = ws.dir() {
            let _ = od_agent::memory::ensure(dir);
        }
        let first_frame = view::frames(ws.session.document())
            .first()
            .and_then(|f| NodeId::parse(&f.id).ok());
        self.ws = Some(Arc::new(Mutex::new(ws)));
        self.forget_canvas();
        self.audio.stop();
        self.active_frame = first_frame;
        self.language = None;
        self.agent_changed.clear();
        self.showcase.reset();
    }

    /// Creates a project folder with one frame of the given size (a store preset or custom).
    pub fn new_project(&mut self, dir: &Path, name: &str, width: f64, height: f64) -> Res<()> {
        let mut ws = Workspace::create(dir, name).map_err(|e| e.to_string())?;
        let root = ws.session.document().pages[0].root.id.clone();
        let mut frame = NewNode::new(NodeKind::Frame);
        frame.name = Some("Frame 1".to_string());
        frame.width = width;
        frame.height = height;
        frame.fill = Some(Paint::parse("#313131").map_err(|e| e.to_string())?);
        let receipt = ws
            .session
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: root,
                    index: None,
                    node: frame,
                }],
            )
            .map_err(|e| e.to_string())?;
        ws.save().map_err(|e| e.to_string())?;
        self.adopt(ws);
        self.active_frame = receipt.created.first().cloned();
        Ok(())
    }

    /// What the start screen should say about `folder`: it is a project itself, it holds
    /// workspaces in `opendesigner/` (listed by name), or it has none yet.
    pub fn inspect_folder(folder: &Path) -> Value {
        let anchor = folder.join(PROJECT_DIR);
        let name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Project");
        let workspaces: Vec<String> = od_agent::workspaces::list(&anchor)
            .into_iter()
            .map(|e| e.name)
            .collect();
        let found = if folder.join("od.toml").is_file() {
            "project"
        } else if workspaces.is_empty() {
            "none"
        } else {
            "workspaces"
        };
        json!({
            "found": found,
            "folder": folder.display().to_string(),
            "name": name,
            "workspaces": workspaces,
            "exists": folder.is_dir(),
        })
    }

    /// Opens a workspace of the project in `folder` (the first one when `workspace` is `None`),
    /// making it when it does not exist and `create` is true. The agent will work in `folder`.
    /// Nothing is drawn: what exists is up to the agent.
    pub fn open_workspace(
        &mut self,
        folder: &Path,
        workspace: Option<&str>,
        create: bool,
    ) -> Res<()> {
        self.open_workspace_in(
            folder,
            workspace,
            create,
            token::config_dir().ok().as_deref(),
        )
    }

    /// [`EditorState::open_workspace`] remembering the folder in `config` (`None`: not at all).
    pub fn open_workspace_in(
        &mut self,
        folder: &Path,
        workspace: Option<&str>,
        create: bool,
        config: Option<&Path>,
    ) -> Res<()> {
        if !folder.is_dir() {
            return Err(format!("{} is not a folder", folder.display()));
        }
        if folder.join("od.toml").is_file() {
            self.open_project(folder)?;
        } else {
            let anchor = folder.join(PROJECT_DIR);
            let existing = od_agent::workspaces::list(&anchor);
            let chosen = match workspace {
                Some(n) => existing
                    .iter()
                    .find(|e| e.name == n)
                    .map(|e| e.path.clone()),
                None => existing.first().map(|e| e.path.clone()),
            };
            let ws = match (chosen, create) {
                (Some(path), _) => Workspace::open(&path).map_err(|e| e.to_string())?,
                (None, true) => od_agent::workspaces::create(&anchor, workspace.unwrap_or("main"))?,
                (None, false) => {
                    return Err(format!(
                        "{} has no OpenDesigner workspace{} yet",
                        folder.display(),
                        workspace
                            .map(|n| format!(" called {n:?}"))
                            .unwrap_or_default()
                    ));
                }
            };
            self.adopt(ws);
        }
        self.repo = Some(folder.to_path_buf());
        if let Some(dir) = config {
            remember_recent(folder, dir);
        }
        Ok(())
    }

    /// The open workspace's folder.
    fn project_dir(&self) -> Option<PathBuf> {
        self.with_ws(|ws| Ok(ws.dir().map(Path::to_path_buf)))
            .ok()
            .flatten()
    }

    /// The workspaces next to the open one and which is open, for the window.
    pub fn workspaces_json(&self) -> String {
        self.with_ws(|ws| Ok(workspaces_of(ws)))
            .unwrap_or_else(|_| json!({"open": null, "names": []}).to_string())
    }

    /// Switches to (or first makes) another workspace without stopping the agent connection: the
    /// agent's tools follow, because they share the same workspace.
    pub fn switch_workspace(&mut self, name: &str, create: bool) -> Res<()> {
        let dir = self.project_dir().ok_or("no project is open")?;
        let next = if create {
            od_agent::workspaces::create(&dir, name)?
        } else {
            let path = od_agent::workspaces::find(&dir, name)
                .ok_or_else(|| format!("no workspace called {name:?}"))?;
            Workspace::open(&path).map_err(|e| e.to_string())?
        };
        self.with_ws(|ws| {
            ws.save().map_err(|e| e.to_string())?;
            *ws = next;
            Ok(())
        })?;
        self.sync_workspace();
        Ok(())
    }

    /// Forgets what belongs to a workspace that is no longer open (language, outlines, frame).
    pub fn sync_workspace(&mut self) {
        let dir = self.project_dir();
        if dir != self.known_dir {
            self.known_dir = dir;
            self.active_frame = None;
            self.language = None;
            self.agent_changed.clear();
            self.showcase.reset();
        }
    }

    /// The working folders opened before, newest first, that still exist.
    pub fn recent_projects() -> Value {
        token::config_dir()
            .map(|d| Self::recent_projects_in(&d))
            .unwrap_or_else(|_| Value::Array(Vec::new()))
    }

    pub fn recent_projects_in(config: &Path) -> Value {
        let list = std::fs::read_to_string(config.join(RECENT_FILE))
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
            .unwrap_or_default();
        Value::Array(
            list.iter()
                .filter(|p| Path::new(p).is_dir())
                .map(|p| Self::inspect_folder(Path::new(p)))
                .collect(),
        )
    }

    /// Chooses the open frame when none is chosen, or the chosen one is gone: the project may
    /// start empty and get its first frame from the agent.
    pub fn ensure_active_frame(&mut self) {
        self.sync_workspace();
        let Some(ws) = self.ws.clone() else {
            return;
        };
        let Ok(ws) = ws.lock() else {
            return;
        };
        let doc = ws.session.document();
        let valid = self
            .active_frame
            .as_ref()
            .is_some_and(|f| doc.node(f).is_some_and(|n| n.kind == NodeKind::Frame));
        if !valid {
            self.active_frame = view::frames(doc)
                .first()
                .and_then(|f| NodeId::parse(&f.id).ok());
        }
    }

    pub fn open_project(&mut self, dir: &Path) -> Res<()> {
        let ws = Workspace::open(dir).map_err(|e| e.to_string())?;
        self.adopt(ws);
        Ok(())
    }

    pub fn save(&self) -> Res<()> {
        self.persist()
    }

    /// Size presets for new projects: the store presets that fix one size, plus web sizes.
    pub fn frame_presets() -> Value {
        let mut list: Vec<Value> = od_store::PRESETS
            .iter()
            .filter_map(|p| match p.size {
                od_store::SizeRule::OneOf(sizes) => sizes
                    .iter()
                    .max_by_key(|(w, h)| u64::from(*w) * u64::from(*h))
                    .map(|(w, h)| json!({"id": p.id, "label": p.description, "width": w, "height": h})),
                _ => None,
            })
            .collect();
        list.push(json!({"id": "google-phone", "label": "Google Play phone screenshot (1080 x 1920)", "width": 1080, "height": 1920}));
        for (id, label, w, h) in [
            ("web-og", "Open Graph / social card (1200 x 630)", 1200, 630),
            ("web-icon", "App icon (1024 x 1024)", 1024, 1024),
            ("web-hero", "Web hero (1920 x 1080)", 1920, 1080),
        ] {
            list.push(json!({"id": id, "label": label, "width": w, "height": h}));
        }
        Value::Array(list)
    }

    // ---- selection and navigation ----

    pub fn set_active_frame(&mut self, id: &str) -> Res<()> {
        let id = parse_id(id)?;
        self.with_ws(|ws| match ws.session.document().node(&id) {
            Some(n) if n.kind == NodeKind::Frame => Ok(()),
            Some(_) => Err(format!("{id} is not a frame")),
            None => Err(format!("{id} does not exist")),
        })?;
        self.active_frame = Some(id);
        Ok(())
    }

    pub fn select(&mut self, id: Option<&str>) -> Res<()> {
        let ids = match id {
            Some(s) if !s.is_empty() => vec![parse_id(s)?],
            _ => Vec::new(),
        };
        self.with_ws(|ws| ws.select(ids).map_err(|e| e.to_string()))
    }

    /// The node under `(x, y)` in the active frame's coordinates.
    pub fn hit_test(&self, x: f64, y: f64) -> Option<String> {
        let frame = self.active_frame.as_ref()?;
        self.with_ws(|ws| Ok(geometry::hit_test(ws.session.document(), frame, x, y)))
            .ok()
            .flatten()
            .map(|id| id.to_string())
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.with_ws(|ws| Ok(ws.selection().into_iter().next()))
            .ok()
            .flatten()
    }

    // ---- editing (as the user) ----

    fn apply_user(&mut self, ops: &[Operation]) -> Res<Vec<NodeId>> {
        let receipt = self.with_ws(|ws| {
            ws.session
                .apply(Actor::User, None, ops)
                .map_err(|e| e.to_string())
        })?;
        self.persist()?;
        Ok(receipt.created)
    }

    pub fn move_node(&mut self, id: &str, x: f64, y: f64) -> Res<()> {
        self.apply_user(&[Operation::Move {
            id: parse_id(id)?,
            x,
            y,
        }])
        .map(|_| ())
    }

    /// Sets position and size in one undo step. Groups have no size of their own.
    pub fn resize_node(&mut self, id: &str, x: f64, y: f64, w: f64, h: f64) -> Res<()> {
        let id = parse_id(id)?;
        let is_group = self.with_ws(|ws| {
            Ok(ws
                .session
                .document()
                .node(&id)
                .is_some_and(|n| n.kind == NodeKind::Group))
        })?;
        let set = |prop| Operation::Set {
            id: id.clone(),
            prop,
        };
        let mut ops = vec![set(Property::X(x)), set(Property::Y(y))];
        if !is_group {
            ops.push(set(Property::Width(w.max(0.0))));
            ops.push(set(Property::Height(h.max(0.0))));
        }
        self.apply_user(&ops).map(|_| ())
    }

    /// Sets one property from the inspector: `value` is JSON (`10`, `"#ffcc00"`, `null`).
    pub fn set_property(&mut self, id: &str, name: &str, value: &str) -> Res<()> {
        let value: Value = serde_json::from_str(value).map_err(|e| format!("value: {e}"))?;
        let language = self.active_language();
        if let (Some(language), "text", Value::String(text)) = (language, name, &value) {
            return self
                .apply_user(&[Operation::SetLocalizedText {
                    locale: language,
                    id: parse_id(id)?,
                    text: Some(text.clone()),
                }])
                .map(|_| ());
        }
        let prop: Property = serde_json::from_value(json!({"name": name, "value": value}))
            .map_err(|e| format!("{name}: {e}"))?;
        self.apply_user(&[Operation::Set {
            id: parse_id(id)?,
            prop,
        }])
        .map(|_| ())
    }

    /// The language in use, or `None` when it is the default or its translations are gone (an
    /// undo or an agent can remove a language under the editor).
    fn active_language(&self) -> Option<String> {
        let language = self.language.clone()?;
        self.with_ws(|ws| Ok(ws.session.document().locales.contains_key(&language)))
            .ok()
            .filter(|known| *known)
            .map(|_| language)
    }

    /// Switches the language the canvas, the inspector's text and exports show (`""` is the default
    /// text).
    pub fn switch_language(&mut self, tag: &str) -> Res<()> {
        if tag.is_empty() {
            self.language = None;
            return Ok(());
        }
        let known = self.with_ws(|ws| Ok(ws.session.document().locales.contains_key(tag)))?;
        if !known {
            return Err(format!("the project has no language {tag:?}"));
        }
        self.language = Some(tag.to_string());
        Ok(())
    }

    /// Adds a language: every text node gets a translation that starts as its current text, ready
    /// to edit, in one undoable step. Switches to it.
    pub fn add_language(&mut self, tag: &str) -> Res<()> {
        let tag = tag.trim();
        let ops: Vec<Operation> = self.with_ws(|ws| {
            let doc = ws.session.document();
            if doc.locales.contains_key(tag) {
                return Err(format!("{tag} already exists"));
            }
            Ok(view::text_nodes(doc)
                .into_iter()
                .filter_map(|t| {
                    Some(Operation::SetLocalizedText {
                        locale: tag.to_string(),
                        id: NodeId::parse(&t.id).ok()?,
                        text: Some(t.text),
                    })
                })
                .collect())
        })?;
        if ops.is_empty() {
            return Err("there are no text nodes to translate yet".to_string());
        }
        self.apply_user(&ops)?;
        self.language = Some(tag.to_string());
        Ok(())
    }

    /// Removes a language and all of its translations, in one undoable step.
    pub fn remove_language(&mut self, tag: &str) -> Res<()> {
        let ops: Vec<Operation> = self.with_ws(|ws| {
            let texts = ws
                .session
                .document()
                .locales
                .get(tag)
                .ok_or_else(|| format!("the project has no language {tag:?}"))?;
            Ok(texts
                .keys()
                .map(|id| Operation::SetLocalizedText {
                    locale: tag.to_string(),
                    id: id.clone(),
                    text: None,
                })
                .collect())
        })?;
        self.apply_user(&ops)?;
        if self.language.as_deref() == Some(tag) {
            self.language = None;
        }
        Ok(())
    }

    /// Defines or recolors a design token (`value` is `#rrggbb` or `#rrggbbaa`).
    pub fn set_token(&mut self, name: &str, value: &str) -> Res<()> {
        self.apply_user(&[Operation::SetToken {
            name: name.trim().to_string(),
            value: Some(value.trim().to_string()),
        }])
        .map(|_| ())
    }

    /// Removes a token nothing uses.
    pub fn remove_token(&mut self, name: &str) -> Res<()> {
        self.apply_user(&[Operation::SetToken {
            name: name.trim().to_string(),
            value: None,
        }])
        .map(|_| ())
    }

    pub fn toggle_visible(&mut self, id: &str) -> Res<()> {
        let id = parse_id(id)?;
        let visible = self.with_ws(|ws| {
            ws.session
                .document()
                .node(&id)
                .map(|n| n.visible)
                .ok_or_else(|| format!("{id} does not exist"))
        })?;
        self.apply_user(&[Operation::Set {
            id,
            prop: Property::Visible(!visible),
        }])
        .map(|_| ())
    }

    /// Draws a new node in the active frame (or a frame on the page when `kind` is `frame`).
    /// Coordinates are in the active frame. Selects the new node.
    pub fn create_node(&mut self, kind: &str, x: f64, y: f64, w: f64, h: f64) -> Res<String> {
        let kind: NodeKind = serde_json::from_value(json!(kind)).map_err(|_| {
            format!("unknown kind {kind:?}; use frame, group, rect, ellipse, text or image")
        })?;
        let mut node = NewNode::new(kind);
        node.x = x.min(x + w);
        node.y = y.min(y + h);
        node.width = w.abs();
        node.height = h.abs();
        let paint = |hex: &str| Paint::parse(hex).map_err(|e| e.to_string());
        match kind {
            NodeKind::Rect => node.fill = Some(paint("#c7a060")?),
            NodeKind::Ellipse => node.fill = Some(paint("#a17e47")?),
            NodeKind::Frame => node.fill = Some(paint("#313131")?),
            NodeKind::Text => {
                node.text = Some("Text".to_string());
                node.font_size = Some(48.0);
                node.fill = Some(paint("#fcf1d9")?);
            }
            NodeKind::Group | NodeKind::Image | NodeKind::Device3d => {}
            NodeKind::Path => {
                return Err("a path needs path data: ask the agent, or import an SVG".to_string());
            }
        }
        let parent = match (kind, &self.active_frame) {
            (NodeKind::Frame, _) | (_, None) => self.with_ws(|ws| {
                ws.session
                    .document()
                    .pages
                    .first()
                    .map(|p| p.root.id.clone())
                    .ok_or_else(|| "the project has no page".to_string())
            })?,
            (_, Some(frame)) => frame.clone(),
        };
        if kind == NodeKind::Frame {
            // a new frame sits beside the others rather than on top of them
            node.x = self.with_ws(|ws| Ok(next_free_x(ws.session.document())))?;
            node.y = 0.0;
            node.width = node.width.max(1.0);
        }
        let created = self.apply_user(&[Operation::Create {
            parent,
            index: None,
            node,
        }])?;
        let id = created.first().ok_or("nothing was created")?.clone();
        if kind == NodeKind::Frame {
            self.active_frame = Some(id.clone());
        }
        self.select(Some(id.as_str()))?;
        Ok(id.to_string())
    }

    /// Copies the image into the project's `media/`, adds an image node for it in the active
    /// frame (at most 80% of the frame, centered on `center` or on the frame) and selects it.
    /// Copies a file the user chose into the project's `media/` folder without placing it
    /// anywhere: it waits there for the agent (`list_media`). Returns its reference.
    /// The files in the agent's memory (`media/`): reference, size and where they are on disk.
    pub fn memory_list(&self) -> Value {
        self.with_ws(|ws| Ok(memory_of(ws)))
            .unwrap_or_else(|_| Value::Array(Vec::new()))
    }

    /// The text notes of the agent's memory (`.opendesigner/memory/`): name and size.
    pub fn agent_notes(&self) -> Value {
        let notes = self
            .project_dir()
            .and_then(|dir| od_agent::memory::list(&dir).ok())
            .unwrap_or_default();
        Value::Array(
            notes
                .iter()
                .map(|n| json!({"id": n.name, "name": n.name, "bytes": n.bytes, "title": n.title}))
                .collect(),
        )
    }

    /// One note: its text, or the reason it cannot be read.
    pub fn agent_note(&self, name: &str) -> Value {
        match self.project_dir() {
            Some(dir) => match od_agent::memory::read(&dir, name) {
                Ok(text) => json!({"name": name, "text": text}),
                Err(e) => json!({"name": name, "error": e.to_string()}),
            },
            None => json!({"name": name, "error": "no project is open"}),
        }
    }

    /// One picture of a video at `time_ms` as a PNG data URL, at most `max_side` pixels long.
    pub fn video_preview(&self, video: &str, time_ms: u64, max_side: u32) -> Res<String> {
        let id = parse_id(video)?;
        self.with_ws(|ws| {
            let (png, ..) = od_agent::video_export::video_frame_png(
                &od_agent::video_export::Source::of(ws),
                &id,
                time_ms,
                None,
                max_side,
            )
            .map_err(|e| e.0)?;
            Ok(format!("data:image/png;base64,{}", STANDARD.encode(png)))
        })
    }

    /// Starts drawing the preview of `video` on a worker thread unless the one already running
    /// or finished is for the same video and the project has not changed since. A different
    /// video, or an edit, replaces it: the new preview takes the pictures the edit left alone
    /// from the old one, which is then cancelled. The sound is mixed alongside. Returns the
    /// status, or `{"error": ...}` when the preview could not be started.
    pub fn preview_start(&mut self, video: &str) -> Res<String> {
        let id = parse_id(video)?;
        let log_len = self.with_ws(|ws| Ok(ws.session.log().len()))?;
        let current = self
            .preview
            .as_ref()
            .is_some_and(|p| p.video == video && p.log_len == log_len);
        if !current {
            let (src, sound_src) = self.with_ws(|ws| {
                Ok((
                    od_agent::video_export::Source::of(ws),
                    od_agent::video_export::Source::of(ws),
                ))
            })?;
            let reuse = self
                .preview
                .as_ref()
                .map(|p| p.preview.reusable())
                .unwrap_or_default();
            let preview = match od_agent::video_preview::Preview::start(
                src,
                &id,
                od_agent::video_preview::Limits::default(),
                reuse,
            ) {
                Ok(p) => p,
                Err(e) => return Ok(json!({ "error": e.0 }).to_string()),
            };
            // the old preview is dropped here: its worker stops before its next picture
            self.preview = Some(PreviewSlot {
                video: video.to_string(),
                log_len,
                preview,
            });
            if let Err(e) = self.audio.start(sound_src, &id) {
                return Ok(json!({ "error": e }).to_string());
            }
        }
        Ok(self.preview_status())
    }

    /// The preview's progress as JSON: `video`, `ready`, `total`, `fps`, `width`, `height`,
    /// `drawn`, `reused`, `done`, `capped`, `error`, `stale` (the project changed since it was
    /// made) and `audio` (see [`crate::preview::AudioSlot::status`]). `{}` when there is none.
    pub fn preview_status(&self) -> String {
        let Some(p) = &self.preview else {
            return "{}".into();
        };
        let stale = self
            .with_ws(|ws| Ok(ws.session.log().len() != p.log_len))
            .unwrap_or(true);
        let mut status = serde_json::to_value(p.preview.status()).unwrap_or(Value::Null);
        status["video"] = json!(p.video);
        status["stale"] = json!(stale);
        status["audio"] = self.audio.status();
        status.to_string()
    }

    /// Hands picture `index` of the preview, as premultiplied RGBA, to `sink`; `false` when it
    /// is not drawn yet. With `max_side` the picture is shrunk to at most that on a side.
    pub fn preview_frame_into(
        &self,
        index: u64,
        max_side: Option<u32>,
        sink: &mut dyn FnMut(u32, u32, &[u8]),
    ) -> bool {
        let Some(frame) = self.preview.as_ref().and_then(|p| p.preview.frame(index)) else {
            return false;
        };
        let mut pixels = Vec::new();
        frame.decode_into(&mut pixels);
        match max_side {
            Some(side) => {
                let (w, h, small) = shrink(frame.width, frame.height, &pixels, side);
                sink(w, h, &small);
            }
            None => sink(frame.width, frame.height, &pixels),
        }
        true
    }

    /// Cancels and forgets the preview and its sound.
    pub fn preview_stop(&mut self) {
        self.preview = None;
        self.audio.stop();
    }

    /// For a video file the user chose: the project folder to copy it into, so the copy (it can
    /// be gigabytes) can run on another thread. `None` for anything that is not a video.
    pub fn footage_import_target(&self, src: &std::path::Path) -> Option<PathBuf> {
        if !is_footage(src) {
            return None;
        }
        self.with_ws(|ws| ws.dir().map(Path::to_path_buf).ok_or_else(String::new))
            .ok()
    }

    pub fn import_to_memory(&mut self, src: &std::path::Path) -> Res<String> {
        let is_video = std::fs::File::open(src)
            .ok()
            .and_then(|mut f| {
                use std::io::Read;
                let mut head = [0u8; 12];
                let n = f.read(&mut head).ok()?;
                Some(od_core::media::sniff_footage(&head[..n]))
            })
            .unwrap_or(false);
        if is_video {
            // Callers that must not wait use `footage_import_target` and copy on a thread.
            return self.with_ws(|ws| ws.import_footage(src).map_err(|e| e.to_string()));
        }
        self.with_ws(|ws| {
            let reference = ws.import_media(src).map_err(|e| e.to_string())?;
            let path = ws.media_path(&reference).map_err(|e| e.to_string())?;
            od_render::image_dimensions(&path).inspect_err(|_| {
                let _ = std::fs::remove_file(&path);
            })?;
            Ok(reference)
        })
    }

    pub fn import_image(
        &mut self,
        src: &std::path::Path,
        center: Option<(f64, f64)>,
    ) -> Res<String> {
        let frame = self
            .active_frame
            .clone()
            .ok_or("open or create a frame first")?;
        let (reference, (iw, ih), (fw, fh)) = self.with_ws(|ws| {
            let reference = ws.import_media(src).map_err(|e| e.to_string())?;
            let path = ws.media_path(&reference).map_err(|e| e.to_string())?;
            let size = od_render::image_dimensions(&path).inspect_err(|_| {
                let _ = std::fs::remove_file(&path);
            })?;
            let node = ws
                .session
                .document()
                .node(&frame)
                .ok_or("the frame no longer exists")?;
            Ok((reference, size, (node.width, node.height)))
        })?;
        let (iw, ih) = (f64::from(iw), f64::from(ih));
        let fit = (fw * 0.8 / iw).min(fh * 0.8 / ih).min(1.0);
        let (w, h) = ((iw * fit).round().max(1.0), (ih * fit).round().max(1.0));
        let (cx, cy) = center.unwrap_or((fw / 2.0, fh / 2.0));
        let mut node = NewNode::new(NodeKind::Image);
        node.name = src
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.chars().take(60).collect());
        node.image = Some(reference);
        node.x = (cx - w / 2.0).round();
        node.y = (cy - h / 2.0).round();
        node.width = w;
        node.height = h;
        let created = self.apply_user(&[Operation::Create {
            parent: frame,
            index: None,
            node,
        }])?;
        let id = created.first().ok_or("nothing was created")?.clone();
        self.select(Some(id.as_str()))?;
        Ok(id.to_string())
    }

    /// Moves a node one step in its parent's stacking order (`+1` forward, `-1` backward).
    pub fn reorder(&mut self, id: &str, delta: i32) -> Res<()> {
        let id = parse_id(id)?;
        let (parent, index, len) = self.with_ws(|ws| {
            let doc = ws.session.document();
            let loc = doc
                .locate(&id)
                .filter(|l| !l.path.is_empty())
                .ok_or_else(|| format!("{id} cannot be reordered"))?;
            let (&index, rest) = loc.path.split_last().ok_or("no parent")?;
            let parent = doc
                .node_at(&od_core::model::Location {
                    page: loc.page,
                    path: rest.to_vec(),
                })
                .ok_or("no parent")?;
            Ok((parent.id.clone(), index, parent.children.len()))
        })?;
        let target = i64::try_from(index).map_err(|e| e.to_string())? + i64::from(delta);
        if target < 0 || target >= i64::try_from(len).map_err(|e| e.to_string())? {
            return Ok(());
        }
        self.apply_user(&[Operation::Reparent {
            id,
            parent,
            index: Some(target as usize),
        }])
        .map(|_| ())
    }

    pub fn undo(&mut self) -> Res<()> {
        self.with_ws(|ws| {
            ws.session
                .undo(Actor::User)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })?;
        self.persist()
    }

    pub fn redo(&mut self) -> Res<()> {
        self.with_ws(|ws| {
            ws.session
                .redo(Actor::User)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })?;
        self.persist()
    }

    pub fn checkpoint(&mut self, name: &str) -> Res<()> {
        self.with_ws(|ws| {
            ws.session
                .checkpoint(Actor::User, name)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })?;
        self.persist()
    }

    pub fn restore(&mut self, checkpoint: u64) -> Res<()> {
        self.with_ws(|ws| {
            ws.session
                .restore(Actor::User, checkpoint)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })?;
        self.agent_changed.clear();
        self.persist()
    }

    // ---- pins ----

    pub fn add_pin(&mut self, note: &str, node: Option<&str>) -> Res<u64> {
        let id = match node {
            Some(n) if !n.is_empty() => parse_id(n)?,
            _ => self.selected().ok_or("select a node to pin the note on")?,
        };
        let id = self.with_ws(|ws| {
            ws.annotations
                .add(
                    ws.session.document(),
                    AnnotationTarget::Node { id },
                    note,
                    Actor::User,
                )
                .map_err(|e| e.to_string())
        })?;
        self.persist()?;
        Ok(id)
    }

    /// Pins a note on a box drawn on the active frame (coordinates in the frame).
    pub fn add_region_pin(&mut self, note: &str, x: f64, y: f64, w: f64, h: f64) -> Res<u64> {
        let frame = self.active_frame.clone().ok_or("open a frame first")?;
        let id = self.with_ws(|ws| {
            let doc = ws.session.document();
            let origin = geometry::page_bounds(doc, &frame).ok_or("the frame has no bounds")?;
            let loc = doc.locate(&frame).ok_or("the frame does not exist")?;
            let page = doc.pages.get(loc.page).ok_or("no page")?.id.clone();
            ws.annotations
                .add(
                    doc,
                    AnnotationTarget::Region {
                        page,
                        x: origin.x + x.min(x + w),
                        y: origin.y + y.min(y + h),
                        width: w.abs(),
                        height: h.abs(),
                    },
                    note,
                    Actor::User,
                )
                .map_err(|e| e.to_string())
        })?;
        self.persist()?;
        Ok(id)
    }

    pub fn resolve_pin(&mut self, id: u64) -> Res<()> {
        self.with_ws(|ws| {
            ws.annotations
                .resolve(id, Actor::User, None)
                .map_err(|e| e.to_string())
        })?;
        self.persist()
    }

    pub fn reopen_pin(&mut self, id: u64) -> Res<()> {
        self.with_ws(|ws| ws.annotations.reopen(id).map_err(|e| e.to_string()))?;
        self.persist()
    }

    pub fn remove_pin(&mut self, id: u64) -> Res<()> {
        self.with_ws(|ws| ws.annotations.remove(id).map_err(|e| e.to_string()))?;
        self.persist()
    }

    // ---- agent changes ----

    pub fn note_agent_change(&mut self, ids: &[NodeId]) {
        self.agent_changed.extend(ids.iter().cloned());
    }

    pub fn clear_agent_changes(&mut self) {
        self.agent_changed.clear();
    }

    // ---- rendering and export ----

    /// Draws the active frame at `scale` (limited to 4096 px on a side) and hands its pixels, as
    /// premultiplied RGBA, to `sink`. After an edit only the tiles the touched nodes reach are
    /// drawn again.
    pub fn render_canvas_into(
        &self,
        scale: f64,
        sink: &mut dyn FnMut(u32, u32, &[u8]),
    ) -> Res<crate::canvas::Drawn> {
        let frame = self.active_frame.as_ref().ok_or("no frame is open")?;
        self.with_ws(|ws| {
            let language = self.language.as_deref();
            let localized = language.and_then(|l| ws.session.document().localized(l));
            let doc = localized.as_ref().unwrap_or_else(|| ws.session.document());
            let node = doc.node(frame).ok_or("the frame no longer exists")?;
            let scale = canvas_scale(node, scale);
            let request = crate::canvas::Request {
                ws,
                doc,
                language,
                frame,
                scale,
            };
            let full = || od_agent::video_export::render_still(doc, frame, scale, ws.dir());
            let mut canvas = self
                .canvas
                .lock()
                .map_err(|_| "the canvas is unavailable")?;
            canvas.draw(&request, full, sink)
        })
    }

    fn forget_canvas(&self) {
        if let Ok(mut canvas) = self.canvas.lock() {
            canvas.forget();
        }
    }

    /// Draws the open frame with the 3D device `id` turned by `rotate_x` and `rotate_y` degrees
    /// more than it is, without touching the document, as a PNG data URL: the ghost shown while
    /// an orbit drag runs.
    pub fn render_orbit(&self, scale: f64, id: &str, rotate_x: f64, rotate_y: f64) -> Res<String> {
        let frame = self.active_frame.as_ref().ok_or("no frame is open")?;
        let device = parse_id(id)?;
        self.with_ws(|ws| {
            let base = self
                .language
                .as_deref()
                .and_then(|l| ws.session.document().localized(l));
            let source = base.as_ref().unwrap_or_else(|| ws.session.document());
            let doc = turned_copy(source, &device, rotate_x, rotate_y)?;
            let node = doc.node(frame).ok_or("the frame no longer exists")?;
            let scale = canvas_scale(node, scale);
            let rendered =
                od_render::render_frame(&doc, frame, scale, ws.dir()).map_err(|e| e.to_string())?;
            // keep the alpha: a frame with no background is transparent, and the canvas shows
            // that as a checkerboard instead of a white screen
            let png = od_render::encode_png(&rendered.pixmap, true).map_err(|e| e.to_string())?;
            Ok(format!("data:image/png;base64,{}", STANDARD.encode(png)))
        })
    }

    /// Exports into the project's `exports/`: `mode` is `frame` (the open frame), `set` (every
    /// frame on the open frame's page, in drawing order, as `<name>-01.png`, ...) or `panorama`
    /// (the open frame cut into `slices` columns). With a preset, nothing is written unless every
    /// image and the count meet it.
    pub fn export(
        &self,
        mode: &str,
        preset: Option<&str>,
        name: &str,
        slices: u32,
        alpha: bool,
    ) -> Res<Value> {
        let frame = self.active_frame.clone().ok_or("no frame is open")?;
        self.with_ws(|ws| {
            let siblings: Vec<NodeId>;
            let plan = match mode {
                "frame" => Plan::Frame {
                    frame: &frame,
                    file: name,
                },
                "set" => {
                    let doc = ws.session.document();
                    let page = doc
                        .locate(&frame)
                        .and_then(|l| doc.pages.get(l.page))
                        .ok_or("the open frame is not on a page")?;
                    siblings = view::frames(doc)
                        .into_iter()
                        .filter(|f| f.page == page.id.as_str())
                        .map(|f| parse_id(&f.id))
                        .collect::<Res<Vec<_>>>()?;
                    Plan::Set {
                        frames: &siblings,
                        prefix: name,
                    }
                }
                "panorama" => Plan::Panorama {
                    frame: &frame,
                    slices,
                    prefix: name,
                },
                other => return Err(format!("unknown export mode {other:?}")),
            };
            let out = export_assets(
                ws,
                &plan,
                preset,
                None,
                alpha,
                self.language.as_deref().filter(|l| ws.session.document().locales.contains_key(*l)),
            ).map_err(|e| e.0)?;
            let files: Vec<Value> = out
                .files
                .iter()
                .map(|f| json!({"path": f.path, "width": f.width, "height": f.height, "bytes": f.bytes}))
                .collect();
            Ok(json!({
                "files": files, "warnings": out.warnings, "preset_verified": out.preset_verified,
            }))
        })
    }

    /// The device frames the editor can add.
    pub fn device_kinds() -> Value {
        Value::Array(
            Device::ALL
                .iter()
                .map(|d| json!({"id": d.id(), "label": d.label()}))
                .collect(),
        )
    }

    /// Adds a generic device frame (see `od_store::devices`) to the open frame, centered, as large
    /// as fits within 70% of the frame's width and 85% of its height unless `screen_width` is
    /// given.
    pub fn add_device(&mut self, kind: &str, screen_width: Option<f64>) -> Res<String> {
        let device = Device::parse(kind).ok_or_else(|| format!("unknown device {kind:?}"))?;
        let frame = self
            .active_frame
            .clone()
            .ok_or("open or create a frame first")?;
        let (fw, fh) = self.with_ws(|ws| {
            ws.session
                .document()
                .node(&frame)
                .map(|n| (n.width, n.height))
                .ok_or_else(|| "the frame no longer exists".to_string())
        })?;
        let unit = devices::template(device, 100.0, None);
        let (unit_w, unit_h) = unit
            .children
            .first()
            .map_or((1.0, 1.0), |b| (b.width / 100.0, b.height / 100.0));
        let sw = screen_width.unwrap_or_else(|| {
            (fw * 0.7 / unit_w)
                .min(fh * 0.85 / unit_h)
                .round()
                .max(50.0)
        });
        let mut node = devices::template(device, sw, None);
        let (body_w, body_h) = node
            .children
            .first()
            .map_or((0.0, 0.0), |b| (b.width, b.height));
        node.x = ((fw - body_w) / 2.0).round();
        node.y = ((fh - body_h) / 2.0).round();
        let created = self.apply_user(&[Operation::Create {
            parent: frame,
            index: None,
            node,
        }])?;
        let id = created.first().ok_or("nothing was created")?.clone();
        self.select(Some(id.as_str()))?;
        Ok(id.to_string())
    }

    /// The 3D device models the editor can add.
    pub fn device3d_models() -> Value {
        Value::Array(
            DeviceModel::ALL
                .iter()
                .map(|m| json!({"id": m.as_str(), "label": device_model_label(*m)}))
                .collect(),
        )
    }

    /// Adds a 3D device to the open frame, centered, its viewport covering 90% of the frame, as
    /// one undoable edit. Selects it.
    pub fn add_device3d(&mut self, model: &str) -> Res<String> {
        let model: DeviceModel = serde_json::from_value(json!(model)).map_err(|_| {
            format!(
                "unknown 3D model {model:?}; use phone, phone_punch_hole, tablet, laptop or monitor"
            )
        })?;
        let frame = self
            .active_frame
            .clone()
            .ok_or("open or create a frame first")?;
        let (fw, fh) = self.with_ws(|ws| {
            ws.session
                .document()
                .node(&frame)
                .map(|n| (n.width, n.height))
                .ok_or_else(|| "the frame no longer exists".to_string())
        })?;
        let (w, h) = ((fw * 0.9).round().max(1.0), (fh * 0.9).round().max(1.0));
        let mut node = NewNode::new(NodeKind::Device3d);
        node.name = Some(format!("{} 3D", device_model_label(model)));
        node.x = ((fw - w) / 2.0).round();
        node.y = ((fh - h) / 2.0).round();
        node.width = w;
        node.height = h;
        node.device3d = Some(Device3d {
            model,
            ..Device3d::default()
        });
        let created = self.apply_user(&[Operation::Create {
            parent: frame,
            index: None,
            node,
        }])?;
        let id = created.first().ok_or("nothing was created")?.clone();
        self.select(Some(id.as_str()))?;
        Ok(id.to_string())
    }

    /// Shows the recording `file` (`media/<name>.mp4`, from the start) on a 3D device's screen;
    /// an empty `file` goes back to its picture.
    pub fn set_screen_footage(&mut self, id: &str, file: &str) -> Res<()> {
        let footage = (!file.is_empty()).then(|| Footage {
            file: file.to_string(),
            in_ms: 0,
        });
        self.apply_user(&[Operation::Set {
            id: parse_id(id)?,
            prop: Property::ScreenFootage(footage),
        }])
        .map(|_| ())
    }

    /// Turns a 3D device by `rotate_x` and `rotate_y` degrees (added to the angles it already
    /// has) as one undoable edit: the commit of an orbit drag.
    pub fn orbit_device3d(&mut self, id: &str, rotate_x: f64, rotate_y: f64) -> Res<()> {
        let id = parse_id(id)?;
        let (rotate_x, rotate_y) = self.with_ws(|ws| {
            let device = ws
                .session
                .document()
                .node(&id)
                .and_then(|n| n.device3d.clone())
                .ok_or_else(|| format!("{id} is not a 3D device"))?;
            Ok((device.rotate_x + rotate_x, device.rotate_y + rotate_y))
        })?;
        let set = |prop| Operation::Set {
            id: id.clone(),
            prop,
        };
        self.apply_user(&[
            set(Property::RotateX(rotate_x)),
            set(Property::RotateY(rotate_y)),
        ])
        .map(|_| ())
    }

    /// A file dropped on a 3D device: an image becomes its screen picture, a video (mp4, mov,
    /// m4v) is imported as footage and becomes its recording. One undoable edit for the screen.
    pub fn drop_on_device3d(&mut self, src: &std::path::Path, id: &str) -> Res<String> {
        if is_footage(src) {
            let reference = self.import_to_memory(src)?;
            self.set_screen_footage(id, &reference)?;
            self.select(Some(id))?;
            return Ok(id.to_string());
        }
        self.replace_image(src, id)
    }

    /// The 3D device under `(x, y)` in the open frame, if any.
    pub fn device3d_at(&self, x: f64, y: f64) -> Option<String> {
        let id = parse_id(&self.hit_test(x, y)?).ok()?;
        self.with_ws(|ws| {
            Ok(ws
                .session
                .document()
                .node(&id)
                .is_some_and(|n| n.kind == NodeKind::Device3d))
        })
        .ok()?
        .then(|| id.to_string())
    }

    /// Sets how many pictures a video blends per frame for motion blur (0 is off, else 2 to 16).
    pub fn set_motion_blur(&mut self, video: &str, samples: i32) -> Res<()> {
        let samples = u32::try_from(samples).map_err(|_| "motion blur cannot be negative")?;
        self.apply_user(&[Operation::SetVideo {
            id: parse_id(video)?,
            prop: od_core::VideoProperty::MotionBlur(samples),
        }])
        .map(|_| ())
    }

    /// Puts a new image into the node `id` (an image node, for example a device's screen, or the
    /// screen of a 3D device, which stops showing a recording), keeping its position and size.
    pub fn replace_image(&mut self, src: &std::path::Path, id: &str) -> Res<String> {
        let id = parse_id(id)?;
        let (reference, recording) = self.with_ws(|ws| {
            let node = ws
                .session
                .document()
                .node(&id)
                .ok_or_else(|| format!("{id} does not exist"))?;
            if !matches!(node.kind, NodeKind::Image | NodeKind::Device3d) {
                return Err(format!("{id} is a {}, not an image", node.kind.as_str()));
            }
            let recording = node.device3d.as_ref().is_some_and(|d| d.footage.is_some());
            let reference = ws.import_media(src).map_err(|e| e.to_string())?;
            let path = ws.media_path(&reference).map_err(|e| e.to_string())?;
            od_render::image_dimensions(&path).inspect_err(|_| {
                let _ = std::fs::remove_file(&path);
            })?;
            Ok((reference, recording))
        })?;
        let set = |prop| Operation::Set {
            id: id.clone(),
            prop,
        };
        let mut ops = vec![set(Property::Image(Some(reference)))];
        if recording {
            ops.push(set(Property::ScreenFootage(None)));
        }
        self.apply_user(&ops)?;
        self.select(Some(id.as_str()))?;
        Ok(id.to_string())
    }

    /// Turns a device's placeholder screen (a rectangle named `Screen`) into an image node with
    /// the same position, size and corners, in one undoable edit.
    fn fill_screen(&mut self, src: &std::path::Path, screen: &NodeId) -> Res<String> {
        let (parent, index, mut node) = self.with_ws(|ws| {
            let doc = ws.session.document();
            let loc = doc.locate(screen).ok_or("the screen no longer exists")?;
            let (&index, up) = loc.path.split_last().ok_or("the screen has no parent")?;
            let parent = doc
                .node_at(&od_core::model::Location {
                    page: loc.page,
                    path: up.to_vec(),
                })
                .ok_or("the screen has no parent")?;
            let s = doc.node(screen).ok_or("the screen no longer exists")?;
            let mut n = NewNode::new(NodeKind::Image);
            n.name = Some(s.name.clone());
            n.x = s.x;
            n.y = s.y;
            n.width = s.width;
            n.height = s.height;
            n.corner_radius = Some(s.corner_radius);
            Ok((parent.id.clone(), index, n))
        })?;
        node.image = Some(self.with_ws(|ws| {
            let reference = ws.import_media(src).map_err(|e| e.to_string())?;
            let path = ws.media_path(&reference).map_err(|e| e.to_string())?;
            od_render::image_dimensions(&path).inspect_err(|_| {
                let _ = std::fs::remove_file(&path);
            })?;
            Ok(reference)
        })?);
        node.image_fit = Some(od_core::ImageFit::Cover);
        let created = self.apply_user(&[
            Operation::Create {
                parent,
                index: Some(index + 1),
                node,
            },
            Operation::Delete { id: screen.clone() },
        ])?;
        let id = created.first().ok_or("nothing was created")?.clone();
        self.select(Some(id.as_str()))?;
        Ok(id.to_string())
    }

    /// A file dropped on the canvas at `(x, y)` (frame coordinates): swaps the picture of the
    /// image node under it, fills a device's screen, or adds a new image there.
    pub fn drop_image(&mut self, src: &std::path::Path, x: f64, y: f64) -> Res<String> {
        let under = self
            .hit_test(x, y)
            .and_then(|id| parse_id(&id).ok())
            .and_then(|id| {
                self.with_ws(|ws| {
                    Ok(ws
                        .session
                        .document()
                        .node(&id)
                        .map(|n| (n.kind, n.name == devices::SCREEN_NAME)))
                })
                .ok()
                .flatten()
                .map(|(kind, is_screen)| (id, kind, is_screen))
            });
        match under {
            Some((id, NodeKind::Image | NodeKind::Device3d, _)) => {
                self.replace_image(src, id.as_str())
            }
            Some((id, NodeKind::Rect, true)) => self.fill_screen(src, &id),
            _ => self.import_image(src, Some((x, y))),
        }
    }

    // ---- agents ----

    /// Starts the MCP server for the open project. The token lives in `token_file`.
    pub fn start_mcp(
        &mut self,
        token_file: &Path,
        on_change: Arc<dyn Fn(ChangeEvent) + Send + Sync>,
    ) -> Res<String> {
        self.start_mcp_with_jobs(token_file, on_change, Arc::new(|_| {}))
    }

    /// [`EditorState::start_mcp`] that also reports the progress of long jobs (video exports).
    pub fn start_mcp_with_jobs(
        &mut self,
        token_file: &Path,
        on_change: Arc<dyn Fn(ChangeEvent) + Send + Sync>,
        on_job: JobHook,
    ) -> Res<String> {
        if let Some(s) = &self.server {
            return Ok(s.url.clone());
        }
        let ws = self.ws.clone().ok_or("open a project first")?;
        let secret = token::load_or_create(token_file).map_err(|e| e.to_string())?;
        let jobs = Arc::new(Jobs::default());
        let handler = Arc::new(
            Handler::new(ws)
                .with_change_hook(on_change)
                .with_job_hook(on_job, Arc::clone(&jobs)),
        );
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let running = runtime
            .block_on(od_agent::start(
                Arc::clone(&handler),
                secret,
                od_agent::DEFAULT_PORT,
            ))
            .map_err(|e| e.to_string())?;
        let url = running.url();
        self.server = Some(Server {
            runtime,
            running: Some(running),
            url: url.clone(),
            token_file: token_file.to_path_buf(),
            jobs,
            handler,
        });
        Ok(url)
    }

    /// Applies a progress report from a long job; returns a message for the user when it ended.
    pub fn job_update(&mut self, u: JobUpdate) -> Option<(String, bool)> {
        match u.state {
            JobState::Running => {
                self.jobs.insert(
                    u.id,
                    JobRow {
                        label: u.label,
                        done: u.done,
                        total: u.total,
                        cancellable: true,
                    },
                );
                None
            }
            JobState::Finished => {
                self.jobs.remove(&u.id);
                Some((
                    format!("{} done", u.label.replace("Exporting", "Exported")),
                    false,
                ))
            }
            JobState::Cancelled => {
                self.jobs.remove(&u.id);
                Some((format!("{} stopped", u.label), false))
            }
            JobState::Failed(why) => {
                self.jobs.remove(&u.id);
                Some((format!("{} failed: {why}", u.label), true))
            }
        }
    }

    /// Asks a running agent job to stop (a local job cannot be stopped).
    pub fn cancel_job(&self, id: u64) {
        if let Some(s) = &self.server {
            s.jobs.cancel(id);
        }
    }

    /// Adds a job of our own (no percentage) and returns its id.
    pub fn begin_local_job(&mut self, label: &str) -> u64 {
        self.next_local_job += 1;
        self.jobs.insert(
            self.next_local_job,
            JobRow {
                label: label.to_string(),
                done: 0,
                total: 0,
                cancellable: false,
            },
        );
        self.next_local_job
    }

    pub fn end_local_job(&mut self, id: u64) {
        self.jobs.remove(&id);
    }

    pub fn jobs_json(&self) -> String {
        Value::Array(
            self.jobs
                .iter()
                .map(|(id, j)| {
                    json!({"id": id.to_string(), "label": j.label, "done": j.done, "total": j.total, "cancellable": j.cancellable})
                })
                .collect(),
        )
        .to_string()
    }

    pub fn stop_mcp(&mut self) {
        if let Some(mut s) = self.server.take() {
            if let Some(r) = s.running.take() {
                s.runtime.block_on(r.stop());
            }
            s.runtime.shutdown_background();
        }
    }

    // ---- in-app chat ----

    /// The agents the chat offers, as JSON, with which are installed.
    pub fn chat_presets() -> String {
        crate::chat::presets_json()
    }

    /// The agent command last used, kept next to the connection token.
    pub fn chat_last_command() -> String {
        token::config_dir()
            .ok()
            .and_then(|d| std::fs::read_to_string(d.join("chat-agent.txt")).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }

    /// Remembers the agent command for next time (next to the connection token).
    pub fn save_chat_command(command_line: &str) {
        if let Ok(dir) = token::config_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("chat-agent.txt"), command_line.trim());
        }
    }

    /// Starts the agent in `command_line` (a program and arguments, no shell) with the project
    /// folder as its working directory and the editor's MCP server as its tool. The agent
    /// connection must be on. The MCP servers the user added are given to the agent as well.
    pub fn chat_start(&mut self, command_line: &str, hook: ChatHook) -> Res<()> {
        let (program, args) = split_command(command_line)?;
        let project_dir = self.with_ws(|ws| {
            ws.dir()
                .map(Path::to_path_buf)
                .ok_or_else(|| "the project has no folder".to_string())
        })?;
        // The agent works in the workspace's own folder, so whatever it keeps per working
        // directory (its sessions, its own notes) stays with this workspace and never reaches
        // another one of the same repository.
        let cwd = project_dir;
        let server = self
            .server
            .as_ref()
            .ok_or("turn on the agent connection first")?;
        let handler = Arc::clone(&server.handler);
        let mcp = McpEndpoint {
            url: server.url.clone(),
            token: token::load_or_create(&server.token_file).map_err(|e| e.to_string())?,
        };
        // the servers the user added; a list that cannot be read is said in the chat, not fatal
        let (servers, broken) = match crate::servers::load() {
            Ok(list) => (list, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        // get_guidance tells the agent which servers the user added (names only)
        handler.set_extra_servers(servers.iter().map(|s| s.name.clone()).collect());
        if self.chat.is_none() {
            self.chat = Some(Chat::new()?);
        }
        if let Some(chat) = self.chat.as_mut() {
            chat.start(AgentCommand { program, args, cwd }, mcp, servers, hook);
            if let Some(e) = broken {
                chat.note(&format!("The MCP servers could not be read: {e}"));
            }
        }
        Ok(())
    }

    /// Sends `prompt` to the agent as soon as it is ready (after [`EditorState::chat_start`]).
    /// The model and effort picked on the start screen ("" keeps the agent's own).
    pub fn chat_wanted(&mut self, model: &str, effort: &str) {
        if let Some(chat) = self.chat.as_mut() {
            chat.set_wanted(model, effort);
        }
    }

    pub fn chat_first_prompt(&mut self, prompt: &str) {
        if let Some(chat) = self.chat.as_mut() {
            chat.set_first_prompt(prompt);
        }
    }

    pub fn chat_send(&mut self, text: &str) -> Res<()> {
        self.chat
            .as_mut()
            .ok_or_else(|| "start the chat first".to_string())?
            .send(text)
    }

    /// Answers the agent's permission request; an empty `option` cancels it.
    pub fn chat_answer(&mut self, request: &str, option: &str) -> Res<()> {
        let request: u64 = request.parse().map_err(|_| "bad request id".to_string())?;
        let chat = self.chat.as_mut().ok_or("start the chat first")?;
        chat.answer(request, (!option.is_empty()).then_some(option))
    }

    /// Changes a setting of the agent's session (model, mode, effort).
    pub fn chat_set_config(&mut self, id: &str, value: &str) -> Res<()> {
        self.chat
            .as_mut()
            .ok_or_else(|| "start the chat first".to_string())?
            .set_config(id, value)
    }

    pub fn chat_cancel(&mut self) {
        if let Some(c) = self.chat.as_mut() {
            c.cancel();
        }
    }

    pub fn chat_auto_allow(&mut self, on: bool) {
        if let Some(c) = self.chat.as_mut() {
            c.set_auto_allow(on);
        }
    }

    pub fn chat_stop(&mut self) {
        if let Some(c) = self.chat.as_mut() {
            c.stop();
        }
    }

    pub fn chat_apply(&mut self, input: ChatInput) {
        if let Some(c) = self.chat.as_mut() {
            c.apply(input);
        }
    }

    /// The MCP servers the user added for the chat, as the window shows them.
    pub fn mcp_servers_json() -> String {
        crate::servers::list_json()
    }

    /// Adds an MCP server the user described, or replaces the one with that name. `previous` is
    /// the name it had before, so a rename does not leave the old one behind.
    pub fn save_mcp_server(previous: &str, name: &str, command: &str, env: &str) -> Res<()> {
        crate::servers::save(previous, name, command, env)
    }

    /// Removes an MCP server the user added.
    pub fn remove_mcp_server(name: &str) -> Res<()> {
        crate::servers::remove(name)
    }

    /// How to connect each supported agent, as a JSON array; empty while the server is off.
    pub fn connect_recipes(&self) -> String {
        match &self.server {
            Some(s) => serde_json::to_string(&connect::recipes(
                &s.url,
                &s.token_file.display().to_string(),
            ))
            .unwrap_or_default(),
            None => "[]".to_string(),
        }
    }

    /// Whether to offer installing the skill on this start: an agent that reads skills is on this
    /// computer without ours, and the user has not said "don't ask again".
    pub fn skill_offer_pending() -> bool {
        match (skill::home_dir(), token::config_dir()) {
            (Ok(home), Ok(config)) => skill_offer_pending_in(&home, &config),
            _ => false,
        }
    }

    /// The user's answer to the offer: `install` writes the skill for the agents found, and
    /// either way the offer is not repeated. `install == false` is "don't ask again".
    pub fn answer_skill_offer(install: bool) -> Res<Value> {
        let config = token::config_dir().map_err(|e| e.to_string())?;
        let home = skill::home_dir().map_err(|e| e.to_string())?;
        answer_skill_offer_in(&home, &config, install)
    }

    // ---- showcase: the Preview and Variants panels ----

    fn project_folder(&self) -> Res<PathBuf> {
        self.with_ws(|ws| {
            ws.dir()
                .map(Path::to_path_buf)
                .ok_or_else(|| "the project has no folder".to_string())
        })
    }

    /// Stores the user's pick and tells the chat agent when one is connected; the agent can
    /// also read it with `get_variants`. Returns whether the chat was told.
    pub fn pick_variant(&mut self, id: &str) -> Res<bool> {
        let dir = self.project_folder()?;
        let mut saved = od_agent::showcase::load(&dir);
        let label = saved.pick(id).map_err(|e| e.to_string())?.label.clone();
        od_agent::showcase::save(&dir, &saved).map_err(|e| e.to_string())?;
        if self.chat.is_none() {
            return Ok(false);
        }
        self.chat_send(&format!("I pick variant {label}. Continue with it."))
            .map_err(|e| format!("Your pick is saved, but the chat did not get it: {e}"))?;
        Ok(true)
    }

    pub fn dismiss_variants(&mut self) -> Res<()> {
        let dir = self.project_folder()?;
        let mut saved = od_agent::showcase::load(&dir);
        if saved.clear_variants() {
            od_agent::showcase::save(&dir, &saved).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// The user looks at something else than the agent chose, until `clear_preview_override`.
    pub fn set_preview_override(&mut self, kind: &str, target: &str) -> Res<()> {
        let chosen = self.with_ws(|ws| crate::showcase::check_override(ws, kind, target))?;
        self.showcase.preview_override = Some(chosen);
        Ok(())
    }

    pub fn clear_preview_override(&mut self) {
        self.showcase.preview_override = None;
    }

    /// Any frame drawn clean, as a PNG data URL (cached until the document changes).
    pub fn render_frame_image(&self, id: &str, max_side: u32) -> Res<String> {
        self.with_ws(|ws| {
            self.showcase
                .frame_image(ws, self.language.as_deref(), id, max_side)
        })
    }

    /// For an agent's showcase tool call: the panel to reveal and what to tell the user.
    pub fn showcase_notice(&self, tool: &str, who: &str) -> Option<(&'static str, String)> {
        self.with_ws(|ws| Ok(crate::showcase::notice(ws, tool, who)))
            .ok()
            .flatten()
    }

    // ---- panel layout ----

    /// Loads the saved arrangement of panels (or the default). With `persist` false (the window
    /// session) nothing is read or written.
    pub fn layout_load(&mut self, persist: bool) {
        self.layout_file = persist
            .then(|| token::config_dir().ok().map(|d| d.join("layout.json")))
            .flatten();
        self.layout = self
            .layout_file
            .as_deref()
            .map(crate::layout::Layout::load)
            .unwrap_or_default();
    }

    pub fn layout_json(&self) -> String {
        self.layout.to_json()
    }

    /// Applies one change to the arrangement and keeps the result on disk. A change the layout
    /// refuses leaves it as it was.
    pub fn layout_apply<T>(
        &mut self,
        f: impl FnOnce(&mut crate::layout::Layout) -> Res<T>,
    ) -> Res<T> {
        let out = f(&mut self.layout)?;
        if let Some(path) = &self.layout_file {
            self.layout.save(path)?;
        }
        Ok(out)
    }

    pub fn layout_reset(&mut self) -> Res<()> {
        self.layout_apply(|l| {
            *l = crate::layout::Layout::default();
            Ok(())
        })
    }

    /// The sections of formats the user can pick from, as JSON.
    pub fn formats() -> String {
        od_agent::formats::sections_json().to_string()
    }

    /// The user chooses the workspace's format: a list entry (`id`), their own size (`kind`,
    /// `width`, `height`, `fps`), or nothing at all (all empty), which hands the choice back to
    /// the agent.
    pub fn set_format(
        &mut self,
        id: &str,
        kind: &str,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Res<()> {
        let chosen = if !id.is_empty() {
            Some(
                od_agent::formats::find(id)
                    .map(od_agent::formats::BriefFormat::from)
                    .ok_or_else(|| format!("no format {id:?}"))?,
            )
        } else if kind.is_empty() {
            None
        } else {
            Some(od_agent::formats::custom(
                kind,
                width,
                height,
                (fps > 0).then_some(fps),
            )?)
        };
        self.with_ws(|ws| {
            let dir = ws.dir().ok_or("the project has no folder")?.to_path_buf();
            let mut brief = od_agent::formats::load_brief(&dir);
            brief.user_set(chosen);
            od_agent::formats::save_brief(&dir, &brief)
        })
    }

    /// The user answers the agent's request to change the format.
    pub fn answer_format(&mut self, approve: bool) -> Res<()> {
        self.with_ws(|ws| {
            let dir = ws.dir().ok_or("the project has no folder")?.to_path_buf();
            let mut brief = od_agent::formats::load_brief(&dir);
            brief.answer(approve)?;
            od_agent::formats::save_brief(&dir, &brief)
        })
    }

    /// Whether the agent may look for pictures and videos outside the project.
    pub fn device_access() -> bool {
        od_agent::device::enabled()
    }

    pub fn set_device_access(on: bool) -> Res<()> {
        let dir = token::config_dir().map_err(|e| e.to_string())?;
        od_agent::device::set_enabled_in(&dir, on).map_err(|e| e.to_string())
    }

    /// Whether the user has added a Freesound token. The token itself never reaches the UI: it
    /// is a credential, and the agent only needs to know that there is one.
    pub fn freesound_token_set() -> bool {
        od_agent::freesound::has_token()
    }

    /// Stores the Freesound token the user pasted. Only the user can do this.
    pub fn set_freesound_token(value: &str) -> Res<()> {
        od_agent::freesound::save_token(value).map_err(|e| e.to_string())
    }

    /// Forgets the Freesound token.
    pub fn clear_freesound_token() -> Res<()> {
        od_agent::freesound::clear_token().map_err(|e| e.to_string())
    }

    /// Takes the sound track off a video (the user does this from the Video tab). It is an
    /// operation, so it is attributed, undoable and checkpointed like any other edit.
    pub fn remove_video_audio(&mut self, video: &str) -> Res<()> {
        let id = parse_id(video)?;
        self.apply_user(&[Operation::SetVideo {
            id,
            prop: od_core::VideoProperty::Audio(None),
        }])?;
        Ok(())
    }

    pub fn skill_status(&self) -> Res<Value> {
        let home = skill::home_dir().map_err(|e| e.to_string())?;
        skill_status_in(&home)
    }

    /// Installs the skill for every detected agent. The UI shows `skill_status` first.
    pub fn install_skill(&self) -> Res<Value> {
        let home = skill::home_dir().map_err(|e| e.to_string())?;
        install_skill_in(&home)
    }

    // ---- views ----

    pub fn snapshot(&self) -> Snapshot {
        let Some(ws) = &self.ws else {
            return Snapshot {
                mcp_running: self.server.is_some(),
                ..Snapshot::default()
            };
        };
        let Ok(ws) = ws.lock() else {
            return Snapshot::default();
        };
        let doc = ws.session.document();
        let frame = self.active_frame.as_ref().filter(|f| doc.node(f).is_some());
        let in_frame = |id: &NodeId| frame.and_then(|f| geometry::bounds_in_frame(doc, f, id));
        let selection = ws.selection();
        let inspector = selection.first().and_then(|id| doc.node(id)).map(|n| {
            let mut shallow: Node = n.clone();
            let kids = shallow.children.len();
            shallow.children.clear();
            let translation = self
                .language
                .as_ref()
                .and_then(|l| doc.locales.get(l))
                .and_then(|texts| texts.get(&n.id));
            if let Some(t) = translation {
                shallow.text = Some(t.clone());
            }
            let mut v = serde_json::to_value(&shallow).unwrap_or(Value::Null);
            if let Value::Object(m) = &mut v {
                m.insert("child_count".into(), json!(kids));
                if let Some(gradient) = n.fill.as_ref().filter(|f| f.stops().is_some()) {
                    // the inspector's fill box shows a paint as text
                    m.insert("fill".into(), json!(gradient.to_string()));
                }
                if self.language.is_some() && n.kind == NodeKind::Text {
                    m.insert("translated".into(), json!(translation.is_some()));
                }
            }
            v
        });
        let annotations: Vec<Value> = ws
            .annotations
            .all()
            .iter()
            .map(|a| {
                let (bounds, exists) = match &a.target {
                    AnnotationTarget::Node { id } => (in_frame(id), doc.node(id).is_some()),
                    AnnotationTarget::Region {
                        page,
                        x,
                        y,
                        width,
                        height,
                    } => {
                        let origin = frame.and_then(|f| geometry::page_bounds(doc, f));
                        let on_page = frame
                            .and_then(|f| doc.locate(f))
                            .and_then(|l| doc.pages.get(l.page))
                            .is_some_and(|p| &p.id == page);
                        let b = origin.filter(|_| on_page).map(|o| Rect {
                            x: x - o.x,
                            y: y - o.y,
                            width: *width,
                            height: *height,
                        });
                        (b, doc.page(page).is_some())
                    }
                };
                let mut v = serde_json::to_value(a).unwrap_or(Value::Null);
                if let Value::Object(m) = &mut v {
                    m.insert("bounds".into(), rect_json(bounds));
                    m.insert("target_exists".into(), json!(exists));
                }
                v
            })
            .collect();
        let changed: Vec<Value> = self
            .agent_changed
            .iter()
            .filter_map(|id| {
                let b = in_frame(id)?;
                let mut v = rect_json(Some(b));
                if let Value::Object(m) = &mut v {
                    m.insert("id".into(), json!(id));
                }
                Some(v)
            })
            .collect();
        let log: Vec<Value> = ws
            .session
            .log()
            .iter()
            .rev()
            .take(LOG_ROWS)
            .map(|tx| {
                let names: Vec<&str> = tx.ops.iter().map(|a| a.forward.name()).collect();
                json!({
                    "id": tx.id,
                    "actor": tx.actor.to_string(),
                    "agent": matches!(tx.actor, Actor::Agent(_)),
                    "turn": tx.turn,
                    "kind": serde_json::to_value(&tx.kind).unwrap_or(Value::Null),
                    "ops": names.len(),
                    "summary": summarize(&names),
                })
            })
            .collect();
        let frame_size = frame.and_then(|f| doc.node(f)).map_or(
            Value::Null,
            |n| json!({"width": n.width, "height": n.height}),
        );
        Snapshot {
            has_project: true,
            project_name: doc.name.clone(),
            layers: to_string(&view::layers(doc)),
            frames: to_string(&view::frames(doc)),
            active_frame: frame.map(NodeId::to_string).unwrap_or_default(),
            frame_size: frame_size.to_string(),
            selection: to_string(&selection),
            selection_bounds: arrange_ops::union_bounds_json(doc, frame, &selection),
            inspector: inspector.unwrap_or(Value::Null).to_string(),
            annotations: to_string(&annotations),
            checkpoints: to_string(ws.session.checkpoints()),
            log: to_string(&log),
            changed: to_string(&changed),
            unrendered: to_string(
                &frame
                    .map(|f| collect_unrendered(doc, f, ws.dir()))
                    .unwrap_or_default(),
            ),
            memory: memory_of(&ws).to_string(),
            videos: videos_json(doc, ws.dir()),
            jobs: self.jobs_json(),
            workspaces: workspaces_of(&ws),
            brief: brief_of(&ws),
            showcase: crate::showcase::view(&ws, &self.showcase, frame).to_string(),
            overflows: to_string(&frame.map(|f| collect_overflows(doc, f)).unwrap_or_default()),
            tokens: to_string(&view::tokens(doc)),
            languages: to_string(&view::locales(doc)),
            language: self
                .language
                .clone()
                .filter(|l| doc.locales.contains_key(l))
                .unwrap_or_default(),
            chat: self
                .chat
                .as_ref()
                .map_or_else(Chat::idle_view, Chat::view)
                .to_string(),
            can_undo: ws.session.can_undo(),
            can_redo: ws.session.can_redo(),
            selection_rects: arrange_ops::rects_json(doc, frame, &selection),
            can_paste: self.clipboard.is_some(),
            can_group: od_core::arrange::can_group(doc, &selection),
            can_ungroup: od_core::arrange::can_ungroup(doc, &selection),
            mcp_running: self.server.is_some(),
            mcp_url: self
                .server
                .as_ref()
                .map(|s| s.url.clone())
                .unwrap_or_default(),
        }
    }
}

/// Why an image node cannot be drawn, or `None` when its file is readable.
fn image_problem(n: &Node, dir: Option<&std::path::Path>) -> Option<String> {
    let check = || -> Result<(), String> {
        let reference = n.image.as_deref().ok_or("no image is set")?;
        let dir = dir.ok_or("the project has no folder")?;
        let path = od_core::media::resolve(dir, reference).map_err(|e| e.to_string())?;
        od_render::image_dimensions(&path).map(|_| ())
    };
    check().err()
}

/// Image nodes in a frame that the renderer cannot draw (no file, unreadable file): the canvas
/// marks where they are and says why.
/// Text in `frame` that wraps to more rows than its box is tall, with the box's place in the frame.
/// The files in a workspace's `media/` folder, for the memory tab.
/// What the open workspace is for (its format, who chose it, a waiting request), as JSON.
fn brief_of(ws: &Workspace) -> String {
    ws.dir()
        .map_or(Value::Null, |d| od_agent::formats::load_brief(d).to_json())
        .to_string()
}

/// The open workspace's name and its siblings, as JSON (for the window's picker).
fn workspaces_of(ws: &Workspace) -> String {
    let Some(dir) = ws.dir() else {
        return json!({"open": null, "names": []}).to_string();
    };
    json!({
        "open": od_agent::workspaces::name_of(dir),
        "names": od_agent::workspaces::list(dir).into_iter().map(|e| e.name).collect::<Vec<_>>(),
    })
    .to_string()
}

/// `pixels` (premultiplied RGBA, `width` x `height`) shrunk by picking pixels so the longest side
/// is at most `side`: a timeline thumbnail does not need more.
fn shrink(width: u32, height: u32, pixels: &[u8], side: u32) -> (u32, u32, Vec<u8>) {
    let longest = width.max(height).max(1);
    if longest <= side {
        return (width, height, pixels.to_vec());
    }
    let scale = f64::from(side.max(1)) / f64::from(longest);
    let (w, h) = (
        ((f64::from(width) * scale).round() as u32).max(1),
        ((f64::from(height) * scale).round() as u32).max(1),
    );
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        let sy = (u64::from(y) * u64::from(height) / u64::from(h)) as usize;
        for x in 0..w {
            let sx = (u64::from(x) * u64::from(width) / u64::from(w)) as usize;
            let at = (sy * width as usize + sx) * 4;
            out.extend_from_slice(&pixels[at..at + 4]);
        }
    }
    (w, h, out)
}

/// The scale to draw `frame` at: `wanted`, kept between 0.01, 16 and what fits 4096 px.
fn canvas_scale(frame: &Node, wanted: f64) -> f32 {
    let longest = frame.width.max(frame.height).max(1.0);
    wanted.clamp(0.01, MAX_CANVAS_SIDE / longest).min(16.0) as f32
}

/// A copy of `doc` with the 3D device `id` turned; the real document is untouched.
fn turned_copy(
    doc: &od_core::Document,
    id: &NodeId,
    rotate_x: f64,
    rotate_y: f64,
) -> Res<od_core::Document> {
    let mut copy = doc.clone();
    let location = copy
        .locate(id)
        .ok_or_else(|| format!("{id} does not exist"))?;
    let device = copy
        .node_at_mut(&location)
        .and_then(|n| n.device3d.as_mut())
        .ok_or_else(|| format!("{id} is not a 3D device"))?;
    let limit = od_core::device3d::MAX_ROTATION;
    device.rotate_x = (device.rotate_x + rotate_x).clamp(-limit, limit);
    device.rotate_y = (device.rotate_y + rotate_y).clamp(-limit, limit);
    Ok(copy)
}

fn device_model_label(model: DeviceModel) -> &'static str {
    match model {
        DeviceModel::Phone => "Phone",
        DeviceModel::PhonePunchHole => "Phone (punch-hole camera)",
        DeviceModel::Tablet => "Tablet",
        DeviceModel::Laptop => "Laptop",
        DeviceModel::Monitor => "Monitor",
    }
}

/// The videos the panel shows: each animation also carries its node's name, for its lane.
fn videos_json(doc: &od_core::Document, dir: Option<&Path>) -> String {
    let mut videos = serde_json::to_value(view::videos(doc)).unwrap_or(Value::Null);
    let rows = videos.as_array_mut().into_iter().flatten();
    for animation in rows
        .filter_map(|v| v.get_mut("animations")?.as_array_mut())
        .flatten()
    {
        let name = animation
            .get("node")
            .and_then(Value::as_str)
            .and_then(|id| NodeId::parse(id).ok())
            .and_then(|id| doc.node(&id))
            .map(|n| n.name.clone());
        if let Value::Object(m) = animation {
            m.insert("node_name".into(), json!(name.unwrap_or_default()));
        }
    }
    crate::sound_rows::annotate(&mut videos, dir);
    videos.to_string()
}

fn memory_of(ws: &Workspace) -> Value {
    let files = ws.media().unwrap_or_default();
    let videos = ws.footage().unwrap_or_default();
    let footage_rows = videos.iter().map(|f| {
        json!({
            "id": f.path,
            "name": f.path.trim_start_matches("media/"),
            "bytes": f.bytes,
            "kind": "video",
            "file": Value::Null,
            "used": ws.session.document().videos.iter().any(|v| {
                v.clips.iter().any(|c| c.footage.as_ref().is_some_and(|x| x.file == f.path))
            }) || ws.session.document().pages.iter().any(|p| shows_footage(&p.root, &f.path)),
        })
    });
    Value::Array(
        files
            .iter()
            .map(|f| {
                json!({
                    "kind": "image",
                    "id": f.path,
                    "name": f.path.trim_start_matches("media/"),
                    "bytes": f.bytes,
                    "file": ws.media_path(&f.path).ok().map(|p| p.display().to_string()),
                    "used": ws.session.document().pages.iter().any(|p| uses_image(&p.root, &f.path)),
                })
            })
            .chain(footage_rows)
            .collect(),
    )
}

fn uses_image(node: &Node, reference: &str) -> bool {
    node.image.as_deref() == Some(reference)
        || node.children.iter().any(|c| uses_image(c, reference))
}

fn shows_footage(node: &Node, file: &str) -> bool {
    node.device3d
        .as_ref()
        .and_then(|d| d.footage.as_ref())
        .is_some_and(|f| f.file == file)
        || node.children.iter().any(|c| shows_footage(c, file))
}

fn collect_overflows(doc: &od_core::Document, frame: &NodeId) -> Vec<Value> {
    let Some(node) = doc.node(frame) else {
        return Vec::new();
    };
    od_render::check_text(node)
        .into_iter()
        .filter_map(|w| {
            let od_render::Warning::TextOverflow {
                id,
                needed,
                available,
            } = w
            else {
                return None;
            };
            let b = geometry::bounds_in_frame(doc, frame, &id)?;
            Some(json!({
                "id": id, "needed": needed, "available": available,
                "x": b.x, "y": b.y, "width": b.width, "height": b.height,
            }))
        })
        .collect()
}

fn collect_unrendered(
    doc: &od_core::Document,
    frame: &NodeId,
    dir: Option<&std::path::Path>,
) -> Vec<Value> {
    fn walk(
        doc: &od_core::Document,
        frame: &NodeId,
        dir: Option<&std::path::Path>,
        n: &Node,
        out: &mut Vec<Value>,
    ) {
        if !n.visible {
            return;
        }
        if n.kind == NodeKind::Image {
            if let (Some(reason), Some(b)) = (
                image_problem(n, dir),
                geometry::bounds_in_frame(doc, frame, &n.id),
            ) {
                out.push(json!({
                    "id": n.id, "kind": "image", "reason": reason,
                    "x": b.x, "y": b.y, "width": b.width, "height": b.height,
                }));
            }
        }
        for c in &n.children {
            walk(doc, frame, dir, c, out);
        }
    }
    let mut out = Vec::new();
    if let Some(root) = doc.node(frame) {
        walk(doc, frame, dir, root, &mut out);
    }
    out
}

fn to_string<T: serde::Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".to_string())
}

fn summarize(names: &[&str]) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for n in names {
        match counts.iter_mut().find(|(k, _)| k == n) {
            Some((_, c)) => *c += 1,
            None => counts.push((n, 1)),
        }
    }
    counts
        .iter()
        .map(|(k, c)| {
            if *c > 1 {
                format!("{k} x{c}")
            } else {
                (*k).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Where a new frame goes so it does not overlap the existing ones on the first page.
fn next_free_x(doc: &od_core::Document) -> f64 {
    doc.pages
        .first()
        .map(|p| {
            p.root
                .children
                .iter()
                .map(|c| c.x + c.width + 100.0)
                .fold(0.0, f64::max)
        })
        .unwrap_or(0.0)
}

fn state_name(s: &skill::State) -> Value {
    match s {
        skill::State::NotInstalled => json!({"state": "not installed"}),
        skill::State::Foreign => json!({"state": "foreign"}),
        skill::State::Installed { version, modified } => {
            json!({"state": "installed", "version": version, "edited": modified})
        }
    }
}

pub fn skill_status_in(home: &Path) -> Res<Value> {
    let plan = skill::plan(home).map_err(|e| e.to_string())?;
    let rows: Vec<Value> = plan
        .iter()
        .map(|p| {
            let mut v = state_name(&p.state);
            if let Value::Object(m) = &mut v {
                m.insert("id".into(), json!(p.target.id));
                m.insert("label".into(), json!(p.target.label));
                m.insert("readers".into(), json!(p.target.readers));
                m.insert("detected".into(), json!(p.detected));
                m.insert("folder".into(), json!(p.dir.display().to_string()));
                m.insert(
                    "files".into(),
                    json!(
                        skill::generate()
                            .iter()
                            .map(|f| p.dir.join(f.path).display().to_string())
                            .collect::<Vec<_>>()
                    ),
                );
            }
            v
        })
        .collect();
    Ok(Value::Array(rows))
}

/// Puts `folder` first in the list of recent working folders (kept in the config folder).
fn remember_recent(folder: &Path, dir: &Path) {
    let path = dir.join(RECENT_FILE);
    let mut list: Vec<String> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let this = folder.display().to_string();
    list.retain(|p| *p != this);
    list.insert(0, this);
    list.truncate(MAX_RECENT);
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(&path, serde_json::to_string(&list).unwrap_or_default());
    }
}

const SKILL_OFFER_MARKER: &str = "skill-offer-answered";

pub fn skill_offer_pending_in(home: &Path, config: &Path) -> bool {
    !config.join(SKILL_OFFER_MARKER).exists()
        && skill::plan(home).is_ok_and(|plan| {
            plan.iter()
                .any(|p| p.detected && p.state == skill::State::NotInstalled)
        })
}

pub fn answer_skill_offer_in(home: &Path, config: &Path, install: bool) -> Res<Value> {
    let result = if install {
        install_skill_in(home)?
    } else {
        Value::Array(Vec::new())
    };
    std::fs::create_dir_all(config).map_err(|e| e.to_string())?;
    std::fs::write(config.join(SKILL_OFFER_MARKER), "answered\n").map_err(|e| e.to_string())?;
    Ok(result)
}

pub fn install_skill_in(home: &Path) -> Res<Value> {
    let plan = skill::plan(home).map_err(|e| e.to_string())?;
    let ids: Vec<&str> = plan
        .iter()
        .filter(|p| p.detected && p.state != skill::State::Foreign)
        .map(|p| p.target.id)
        .collect();
    if ids.is_empty() {
        return Err("no supported agent was found (or its skill folder is not ours)".to_string());
    }
    let reports = skill::install(home, &ids, false).map_err(|e| e.to_string())?;
    Ok(Value::Array(
        reports
            .iter()
            .map(|r| json!({"target": r.target, "folder": r.dir.display().to_string(), "files": r.actions.len()}))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> (EditorState, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = EditorState::new();
        s.new_project(&tmp.path().join("p.od"), "Store", 400.0, 300.0)
            .unwrap();
        (s, tmp)
    }

    /// The canvas as the window gets it: size, premultiplied RGBA and what the draw did.
    fn draw_canvas(s: &EditorState, scale: f64) -> Res<(u32, u32, Vec<u8>, crate::canvas::Drawn)> {
        let mut picture = (0, 0, Vec::new());
        let drawn = s.render_canvas_into(scale, &mut |w, h, px| picture = (w, h, px.to_vec()))?;
        Ok((picture.0, picture.1, picture.2, drawn))
    }

    fn insp(s: &EditorState) -> Value {
        serde_json::from_str(&s.snapshot().inspector).unwrap()
    }

    #[test]
    fn a_new_project_opens_its_frame_and_the_snapshot_describes_it() {
        let (s, _t) = state();
        let snap = s.snapshot();
        assert!(snap.has_project && snap.project_name == "Store");
        assert!(!snap.active_frame.is_empty());
        let size: Value = serde_json::from_str(&snap.frame_size).unwrap();
        assert_eq!(size["width"], 400.0);
        let layers: Value = serde_json::from_str(&snap.layers).unwrap();
        assert_eq!(layers.as_array().unwrap().len(), 2, "page and frame");
        assert!(
            snap.can_undo,
            "creating the first frame is an undoable edit"
        );
    }

    #[test]
    fn drawing_selecting_moving_and_resizing_are_undoable_user_edits() {
        let (mut s, _t) = state();
        let id = s.create_node("rect", 40.0, 30.0, 100.0, 60.0).unwrap();
        assert_eq!(s.selected().map(|i| i.to_string()), Some(id.clone()));
        assert_eq!(insp(&s)["name"], "rect");
        assert_eq!(s.hit_test(50.0, 40.0), Some(id.clone()));
        assert_eq!(
            s.hit_test(300.0, 250.0).as_deref(),
            Some(s.snapshot().active_frame.as_str()),
            "empty frame area hits the frame"
        );
        assert_eq!(s.hit_test(-5.0, 10.0), None);

        s.move_node(&id, 200.0, 100.0).unwrap();
        assert_eq!(
            s.hit_test(50.0, 40.0).as_deref(),
            Some(s.snapshot().active_frame.as_str())
        );
        assert_eq!(s.hit_test(210.0, 110.0), Some(id.clone()));
        s.resize_node(&id, 200.0, 100.0, 50.0, 50.0).unwrap();
        let b: Value = serde_json::from_str(&s.snapshot().selection_bounds).unwrap();
        assert_eq!(
            (b["width"].as_f64(), b["height"].as_f64()),
            (Some(50.0), Some(50.0))
        );

        s.undo().unwrap();
        s.undo().unwrap();
        assert_eq!(
            s.hit_test(50.0, 40.0),
            Some(id.clone()),
            "undone back to the drawn position"
        );
        s.redo().unwrap();
        assert!(s.snapshot().can_redo);
        s.delete_selected().unwrap();
        assert!(s.selected().is_none());
        s.undo().unwrap();
        s.select(Some(&id)).unwrap();
        assert_eq!(s.selected().map(|i| i.to_string()), Some(id));
    }

    #[test]
    fn a_gradient_fill_reaches_the_inspector_as_text() {
        let (mut s, _t) = state();
        let id = s.create_node("rect", 10.0, 10.0, 100.0, 60.0).unwrap();
        let gradient = json!({"type": "linear", "angle": 0, "stops": [
            {"offset": 0, "color": "#ff0000"}, {"offset": 1, "color": "#0000ff"}]});
        s.set_property(&id, "fill", &gradient.to_string()).unwrap();
        assert_eq!(insp(&s)["fill"], "linear(0deg, #ff0000 0, #0000ff 1)");
        assert!(s.set_property(&id, "fill", "\"linear(0deg)\"").is_err());
    }

    #[test]
    fn the_inspector_sets_typed_properties_and_rejects_bad_values_with_a_reason() {
        let (mut s, _t) = state();
        let id = s.create_node("text", 10.0, 10.0, 200.0, 60.0).unwrap();
        s.set_property(&id, "text", "\"Hold for everything.\"")
            .unwrap();
        s.set_property(&id, "font_size", "96").unwrap();
        s.set_property(&id, "fill", "\"#ffcc00\"").unwrap();
        s.set_property(&id, "opacity", "0.5").unwrap();
        let v = insp(&s);
        assert_eq!(v["text"], "Hold for everything.");
        assert_eq!(v["font_size"], 96.0);
        assert_eq!(v["fill"], "#ffcc00");
        assert!(
            s.set_property(&id, "opacity", "7")
                .unwrap_err()
                .contains("opacity")
        );
        assert!(
            s.set_property(&id, "corner_radius", "4")
                .unwrap_err()
                .contains("does not apply")
        );
        assert!(s.set_property(&id, "nope", "1").is_err());
        assert!(s.set_property(&id, "x", "not json").is_err());
    }

    #[test]
    fn stacking_order_and_visibility_follow_the_layers_panel() {
        let (mut s, _t) = state();
        let a = s.create_node("rect", 0.0, 0.0, 50.0, 50.0).unwrap();
        let b = s.create_node("rect", 0.0, 0.0, 50.0, 50.0).unwrap();
        assert_eq!(
            s.hit_test(10.0, 10.0),
            Some(b.clone()),
            "the later node is on top"
        );
        s.reorder(&a, 1).unwrap();
        assert_eq!(s.hit_test(10.0, 10.0), Some(a.clone()));
        s.reorder(&a, 1).unwrap(); // already on top: nothing happens
        s.toggle_visible(&a).unwrap();
        assert_eq!(s.hit_test(10.0, 10.0), Some(b));
    }

    #[test]
    fn pins_are_user_notes_that_survive_save_and_reopen() {
        let (mut s, t) = state();
        let id = s.create_node("rect", 10.0, 10.0, 80.0, 40.0).unwrap();
        assert!(s.add_pin("", Some(&id)).is_err());
        let pin = s.add_pin("too small", Some(&id)).unwrap();
        s.add_region_pin("more air", 5.0, 5.0, 30.0, 30.0).unwrap();
        let notes: Value = serde_json::from_str(&s.snapshot().annotations).unwrap();
        assert_eq!(notes.as_array().unwrap().len(), 2);
        assert_eq!(notes[0]["bounds"]["width"], 80.0);
        assert_eq!(
            notes[1]["bounds"]["x"], 5.0,
            "region pins are shown in frame coordinates"
        );
        s.resolve_pin(pin).unwrap();

        let mut again = EditorState::new();
        again.open_project(&t.path().join("p.od")).unwrap();
        let notes: Value = serde_json::from_str(&again.snapshot().annotations).unwrap();
        assert_eq!(notes[0]["status"], "resolved");
        assert_eq!(notes.as_array().unwrap().len(), 2);
        again.reopen_pin(pin).unwrap();
        again.remove_pin(pin).unwrap();
    }

    #[test]
    fn agent_changes_are_outlined_until_cleared_or_restored() {
        let (mut s, _t) = state();
        let id = s.create_node("rect", 10.0, 10.0, 80.0, 40.0).unwrap();
        s.checkpoint("before").unwrap();
        s.note_agent_change(&[NodeId::parse(&id).unwrap()]);
        let changed: Value = serde_json::from_str(&s.snapshot().changed).unwrap();
        assert_eq!(changed[0]["id"], id.as_str());
        assert_eq!(changed[0]["width"], 80.0);
        s.clear_agent_changes();
        assert_eq!(s.snapshot().changed, "[]");
        s.note_agent_change(&[NodeId::parse(&id).unwrap()]);
        s.set_property(&id, "x", "99").unwrap();
        s.restore(1).unwrap();
        assert_eq!(s.snapshot().changed, "[]", "restoring clears the outlines");
        let cps: Value = serde_json::from_str(&s.snapshot().checkpoints).unwrap();
        assert_eq!(cps[0]["name"], "before");
        let log: Value = serde_json::from_str(&s.snapshot().log).unwrap();
        assert_eq!(log[0]["kind"]["type"], "restore");
    }

    #[test]
    fn the_canvas_renders_raw_pixels_and_exports_validate_against_presets() {
        let (mut s, _t) = state();
        s.create_node("rect", 10.0, 10.0, 80.0, 40.0).unwrap();
        let (w, h, pixels, _) = draw_canvas(&s, 1.0).unwrap();
        assert_eq!((w, h), (400, 300));
        assert_eq!(pixels.len(), 400 * 300 * 4);
        // huge scales are clamped instead of exhausting memory
        let (bw, bh, big, _) = draw_canvas(&s, 100.0).unwrap();
        assert!(bw <= 4096 && bh <= 4096 && big.len() < 80_000_000);

        let err = s
            .export("frame", Some("apple-iphone-6.9"), "shot.png", 1, false)
            .unwrap_err();
        assert!(err.contains("1260x2736"), "{err}");
        let ok = s.export("frame", None, "draft.png", 1, false).unwrap();
        assert_eq!(ok["files"][0]["path"], "exports/draft.png");
        assert!(s.export("frame", None, "../x.png", 1, false).is_err());
        assert!(s.export("zip", None, "x", 1, false).is_err());
    }

    #[test]
    fn sets_and_panoramas_export_all_or_nothing() {
        let (mut s, t) = state();
        s.create_node("frame", 0.0, 0.0, 400.0, 300.0).unwrap();
        let exports = t.path().join("p.od/exports");

        let set = s.export("set", None, "slide", 1, false).unwrap();
        let paths: Vec<&str> = set["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            paths,
            ["exports/slide-01.png", "exports/slide-02.png"],
            "both frames on the page"
        );

        // Google phone screenshots need 2 to 8 images of at most 2:1; 400x300 is fine on shape
        // but the preset's size rules refuse it, so nothing is written
        let refused = s
            .export("set", Some("google-phone"), "play", 1, false)
            .unwrap_err();
        assert!(refused.contains("play-01.png"), "{refused}");
        assert!(!exports.join("play-01.png").exists());

        let pano = s.export("panorama", None, "wide", 2, false).unwrap();
        assert_eq!(pano["files"][0]["width"], 200);
        assert!(s.export("panorama", None, "odd", 7, false).is_err());
        assert!(!exports.join("odd-01.png").exists());
    }

    #[test]
    fn a_device_frame_is_added_centered_and_a_dropped_capture_replaces_its_screen() {
        let (mut s, t) = state();
        let id = s.add_device("phone", None).unwrap();
        assert_eq!(s.selected().map(|i| i.to_string()), Some(id));
        let layers: Value = serde_json::from_str(&s.snapshot().layers).unwrap();
        let names: Vec<&str> = layers
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap())
            .collect();
        assert!(
            names.contains(&"Body") && names.contains(&"Screen"),
            "{names:?}"
        );
        assert!(s.add_device("watch", None).is_err());

        // the screen starts as a placeholder rectangle: a drop on it turns it into an image of
        // the same size and place (one undoable edit), and a drop on that image swaps the capture
        let screen_id = layers
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["name"] == "Screen")
            .map(|l| parse_id(l["id"].as_str().unwrap()).unwrap())
            .unwrap();
        let frame = s.active_frame.clone().unwrap();
        let screen = s
            .with_ws(|ws| {
                Ok(geometry::bounds_in_frame(
                    ws.session.document(),
                    &frame,
                    &screen_id,
                ))
            })
            .unwrap()
            .unwrap();
        let (cx, cy) = (
            screen.x + screen.width / 2.0,
            screen.y + screen.height / 2.0,
        );
        let layer_count = layers.as_array().unwrap().len();

        let capture = t.path().join("capture.png");
        write_png(&capture, 90, 195);
        let first = s.drop_image(&capture, cx, cy).unwrap();
        let node = insp(&s);
        assert_eq!(node["kind"], "image");
        assert_eq!(node["name"], "Screen");
        assert_eq!(node["image"], "media/capture.png");
        assert_eq!(node["width"].as_f64(), Some(screen.width));
        let after: Value = serde_json::from_str(&s.snapshot().layers).unwrap();
        assert_eq!(
            after.as_array().unwrap().len(),
            layer_count,
            "the placeholder was replaced, not stacked"
        );
        let (w, h) = (node["width"].clone(), node["height"].clone());

        let second_file = t.path().join("second.png");
        write_png(&second_file, 60, 130);
        let again = s.drop_image(&second_file, cx, cy).unwrap();
        assert_eq!(
            again, first,
            "the picture under the drop is replaced, not stacked"
        );
        assert_eq!(insp(&s)["image"], "media/second.png");
        assert_eq!(
            (insp(&s)["width"].clone(), insp(&s)["height"].clone()),
            (w, h)
        );
        s.undo().unwrap();
        assert_eq!(
            insp(&s)["image"],
            "media/capture.png",
            "the swap is one undoable edit"
        );
        s.undo().unwrap();
        let reverted: Value = serde_json::from_str(&s.snapshot().layers).unwrap();
        assert!(
            reverted
                .as_array()
                .unwrap()
                .iter()
                .any(|l| l["name"] == "Screen" && l["kind"] == "rect"),
            "undoing the fill brings the placeholder back"
        );
    }

    #[test]
    fn frames_go_beside_each_other_and_presets_cover_the_stores() {
        let (mut s, _t) = state();
        let second = s.create_node("frame", 0.0, 0.0, 200.0, 100.0).unwrap();
        assert_eq!(
            s.snapshot().active_frame,
            second,
            "a new frame becomes the active one"
        );
        let frames: Value = serde_json::from_str(&s.snapshot().frames).unwrap();
        assert_eq!(frames.as_array().unwrap().len(), 2);
        let presets = EditorState::frame_presets();
        let list = presets.as_array().unwrap();
        assert!(
            list.iter()
                .any(|p| p["id"] == "apple-iphone-6.9" && p["width"] == 1320)
        );
        assert!(list.iter().any(|p| p["id"] == "web-og"));
        assert!(s.set_active_frame("n_999").is_err());
    }

    #[test]
    fn no_project_means_a_quiet_snapshot_and_clear_errors() {
        let mut s = EditorState::new();
        assert!(!s.snapshot().has_project);
        assert!(
            s.create_node("rect", 0.0, 0.0, 1.0, 1.0)
                .unwrap_err()
                .contains("no project")
        );
        assert!(draw_canvas(&s, 1.0).is_err());
        assert_eq!(s.hit_test(0.0, 0.0), None);
        assert_eq!(s.connect_recipes(), "[]");
    }

    #[test]
    fn pictures_added_to_memory_are_listed_and_marked_used_once_the_design_has_them() {
        let (mut s, t) = state();
        let src = t.path().join("shot.png");
        write_png(&src, 8, 8);
        assert_eq!(s.memory_list(), json!([]));
        let reference = s.import_to_memory(&src).unwrap();
        assert_eq!(reference, "media/shot.png");
        let list = s.memory_list();
        assert_eq!(list[0]["name"], "shot.png");
        assert_eq!(list[0]["used"], false);
        assert!(
            list[0]["file"]
                .as_str()
                .unwrap()
                .ends_with("media/shot.png")
        );
        s.create_node("frame", 0.0, 0.0, 100.0, 100.0).unwrap();
        s.import_image(&src, None).unwrap();
        assert_eq!(s.memory_list()[0]["used"], true);
        assert!(
            s.import_to_memory(&t.path().join("notes.txt")).is_err(),
            "only pictures"
        );
    }

    #[test]
    fn a_video_preview_is_drawn_in_the_background_and_goes_stale_when_the_project_changes() {
        let (mut s, _t) = state();
        let frame = s.create_node("frame", 0.0, 0.0, 200.0, 400.0).unwrap();
        let ids = s
            .apply_user(&[Operation::CreateVideo {
                name: "V".into(),
                fps: 30,
                width: 100,
                height: 200,
            }])
            .unwrap();
        let video = ids[0].to_string();
        s.apply_user(&[Operation::AddClip {
            video: ids[0].clone(),
            frame: Some(parse_id(&frame).unwrap()),
            footage: None,
            duration_ms: 2000,
            index: None,
            transition: None,
            motion: None,
        }])
        .unwrap();
        assert_eq!(s.preview_status(), "{}", "none until started");
        let status =
            |s: &EditorState| -> Value { serde_json::from_str(&s.preview_status()).unwrap() };
        let wait_done = |s: &EditorState| {
            for _ in 0..1000 {
                if status(s)["done"] == true && status(s)["audio"]["state"] != "mixing" {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            status(s)
        };
        let first: Value = serde_json::from_str(&s.preview_start(&video).unwrap()).unwrap();
        assert_eq!(first["video"], video.as_str());
        let done = wait_done(&s);
        let fps = done["fps"].as_u64().unwrap();
        assert!(fps == 15 || fps == 30, "{done}");
        assert_eq!(
            (
                done["ready"].as_u64(),
                done["total"].as_u64(),
                done["stale"].as_bool()
            ),
            (Some(2 * fps), Some(2 * fps), Some(false)),
            "{done}"
        );
        assert_eq!(
            done["audio"]["state"], "silent",
            "a design video has no sound"
        );
        let mut shown = (0, 0, 0);
        assert!(s.preview_frame_into(0, None, &mut |w, h, px| shown = (w, h, px.len())));
        assert_eq!(shown, (100, 200, 100 * 200 * 4));
        assert!(s.preview_frame_into(0, Some(50), &mut |w, h, px| shown = (w, h, px.len())));
        assert_eq!(shown, (25, 50, 25 * 50 * 4), "a thumbnail is shrunk");
        assert!(
            !s.preview_frame_into(2 * fps, None, &mut |_, _, _| {}),
            "past the end"
        );
        // starting again with nothing changed keeps the finished preview
        s.preview_start(&video).unwrap();
        assert_eq!(status(&s)["done"], true);
        // an edit makes it stale; starting again draws a new one that keeps the old pictures
        s.apply_user(&[Operation::AddClip {
            video: ids[0].clone(),
            frame: Some(parse_id(&frame).unwrap()),
            footage: None,
            duration_ms: 1000,
            index: None,
            transition: None,
            motion: None,
        }])
        .unwrap();
        assert_eq!(status(&s)["stale"], true);
        s.preview_start(&video).unwrap();
        let again = wait_done(&s);
        assert_eq!(
            (again["total"].as_u64(), again["stale"].as_bool()),
            (Some(3 * again["fps"].as_u64().unwrap()), Some(false)),
            "{again}"
        );
        assert!(again["reused"].as_u64().unwrap() > 0, "{again}");
        s.preview_stop();
        assert_eq!(s.preview_status(), "{}");
        let missing = s.preview_start("v_999").unwrap();
        assert!(missing.contains("does not exist"), "{missing}");
    }

    fn device(s: &EditorState) -> Value {
        insp(s)["device3d"].clone()
    }

    #[test]
    fn a_3d_device_is_added_centered_in_one_undoable_step() {
        let (mut s, _t) = state();
        assert!(s.add_device3d("watch").is_err());
        let id = s.add_device3d("tablet").unwrap();
        assert_eq!(s.selected().map(|i| i.to_string()), Some(id.clone()));
        let node = insp(&s);
        assert_eq!(node["kind"], "device3d");
        assert_eq!(device(&s)["model"], "tablet");
        let (w, h) = (
            node["width"].as_f64().unwrap(),
            node["height"].as_f64().unwrap(),
        );
        assert!(
            (w - 360.0).abs() < 1.0 && (h - 270.0).abs() < 1.0,
            "{w}x{h}"
        );
        assert_eq!(node["x"].as_f64().unwrap() + w / 2.0, 200.0);
        assert_eq!(node["y"].as_f64().unwrap() + h / 2.0, 150.0);
        assert_eq!(
            s.device3d_at(200.0, 150.0).as_deref(),
            Some(id.as_str()),
            "the device is what the canvas hits at its centre"
        );

        s.undo().unwrap();
        assert!(s.selected().is_none() || insp(&s)["kind"] != "device3d");
        assert_eq!(s.device3d_at(200.0, 150.0), None, "one undo removes it");
        let layers: Value = serde_json::from_str(&s.snapshot().layers).unwrap();
        assert_eq!(layers.as_array().unwrap().len(), 2, "page and frame only");
        let models = EditorState::device3d_models();
        assert_eq!(models.as_array().unwrap().len(), 5);
    }

    #[test]
    fn the_inspector_sets_3d_device_fields_validates_them_and_undoes() {
        let (mut s, _t) = state();
        let id = s.add_device3d("phone").unwrap();
        s.set_property(&id, "model", "\"laptop\"").unwrap();
        s.set_property(&id, "rotate_x", "12.5").unwrap();
        s.set_property(&id, "rotate_y", "-40").unwrap();
        s.set_property(&id, "rotate_z", "3").unwrap();
        s.set_property(&id, "zoom", "1.5").unwrap();
        s.set_property(&id, "fov", "45").unwrap();
        s.set_property(&id, "light_angle", "90").unwrap();
        s.set_property(&id, "shadow", "0.8").unwrap();
        s.set_property(&id, "glare", "0").unwrap();
        s.set_property(&id, "screen_scroll", "0.75").unwrap();
        s.set_property(&id, "fill", "\"#202020\"").unwrap();
        let d = device(&s);
        assert_eq!(d["model"], "laptop");
        assert_eq!(d["rotate_x"], 12.5);
        assert_eq!(d["rotate_y"], -40.0);
        assert_eq!(d["zoom"], 1.5);
        assert_eq!(d["fov"], 45.0);
        assert_eq!(d["light_angle"], 90.0);
        assert_eq!(d["shadow"], 0.8);
        assert_eq!(d["screen_scroll"], 0.75);
        assert_eq!(insp(&s)["fill"], "#202020");

        for (name, bad) in [
            ("rotate_x", "9000"),
            ("zoom", "0"),
            ("zoom", "11"),
            ("fov", "0.5"),
            ("fov", "101"),
            ("shadow", "1.5"),
            ("glare", "-0.1"),
            ("screen_scroll", "2"),
            ("model", "\"watch\""),
        ] {
            assert!(s.set_property(&id, name, bad).is_err(), "{name}={bad}");
        }
        assert_eq!(device(&s)["zoom"], 1.5, "a refused value changes nothing");

        s.set_property(&id, "screen_scroll", "null").unwrap();
        assert!(
            device(&s).get("screen_scroll").is_none(),
            "cleared back to centred"
        );
        s.undo().unwrap();
        assert_eq!(
            device(&s)["screen_scroll"],
            0.75,
            "each change is one undo step"
        );
        s.undo().unwrap();
        s.undo().unwrap();
        let d = device(&s);
        assert!(d.get("screen_scroll").is_none());
        assert_eq!(
            (d["shadow"].as_f64(), d["glare"].as_f64()),
            (Some(0.8), Some(0.0))
        );
        s.undo().unwrap();
        assert!(
            device(&s).get("glare").is_none(),
            "back to the default glare"
        );
        assert!(s.set_property(&id, "corner_radius", "4").is_err());
    }

    #[test]
    fn dropping_a_picture_or_a_video_on_a_3d_device_sets_its_screen() {
        let (mut s, t) = state();
        let id = s.add_device3d("phone").unwrap();
        let capture = t.path().join("capture.png");
        write_png(&capture, 90, 195);
        assert_eq!(s.device3d_at(200.0, 150.0).as_deref(), Some(id.as_str()));
        assert_eq!(s.drop_image(&capture, 200.0, 150.0).unwrap(), id);
        let node = insp(&s);
        assert_eq!(node["kind"], "device3d");
        assert_eq!(node["image"], "media/capture.png");
        s.undo().unwrap();
        assert!(
            insp(&s).get("image").is_none_or(Value::is_null),
            "one undo takes the drop back"
        );

        let clip = t.path().join("take.mov");
        std::fs::write(&clip, b"\0\0\0\x14ftypqt  0000").unwrap();
        assert_eq!(s.drop_on_device3d(&clip, &id).unwrap(), id);
        assert_eq!(device(&s)["screen_footage"]["file"], "media/take.mov");
        let used = s
            .memory_list()
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == "media/take.mov")
            .map(|m| m["used"].clone());
        assert_eq!(used, Some(json!(true)), "the recording counts as used");

        // a picture dropped later shows instead of the recording, in the same edit
        let screen = t.path().join("screen.png");
        write_png(&screen, 120, 260);
        s.drop_on_device3d(&screen, &id).unwrap();
        assert!(device(&s).get("screen_footage").is_none());
        assert_eq!(insp(&s)["image"], "media/screen.png");
        s.undo().unwrap();
        assert_eq!(device(&s)["screen_footage"]["file"], "media/take.mov");

        s.set_screen_footage(&id, "").unwrap();
        assert!(device(&s).get("screen_footage").is_none());
        assert!(s.set_screen_footage(&id, "../x.mov").is_err());
        let wrong = s.create_node("rect", 0.0, 0.0, 10.0, 10.0).unwrap();
        assert!(s.set_screen_footage(&wrong, "media/take.mov").is_err());
    }

    #[test]
    fn an_orbit_commit_is_one_transaction_and_the_ghost_leaves_the_document_alone() {
        let (mut s, _t) = state();
        let id = s.add_device3d("phone").unwrap();
        let ghost = s.render_orbit(0.5, &id, 20.0, 35.0).unwrap();
        assert!(ghost.starts_with("data:image/png;base64,"));
        assert!(
            device(&s).get("rotate_y").is_none(),
            "a ghost is not an edit"
        );
        assert!(s.render_orbit(0.5, "n_999", 1.0, 1.0).is_err());

        s.orbit_device3d(&id, 20.0, 35.0).unwrap();
        let d = device(&s);
        assert_eq!(
            (d["rotate_x"].as_f64(), d["rotate_y"].as_f64()),
            (Some(20.0), Some(35.0))
        );
        s.undo().unwrap();
        let d = device(&s);
        assert!(
            d.get("rotate_x").is_none() && d.get("rotate_y").is_none(),
            "{d}"
        );
        assert!(s.orbit_device3d(&id, 5000.0, 0.0).is_err());
        assert!(
            device(&s).get("rotate_x").is_none(),
            "a refused orbit changes nothing"
        );
        let rect = s.create_node("rect", 0.0, 0.0, 10.0, 10.0).unwrap();
        assert!(s.orbit_device3d(&rect, 1.0, 1.0).is_err());
        assert!(s.render_orbit(0.5, &rect, 1.0, 1.0).is_err());
    }

    #[test]
    fn motion_blur_is_off_or_two_to_sixteen_pictures_and_undoes() {
        let (mut s, _t) = state();
        let ids = s
            .apply_user(&[Operation::CreateVideo {
                name: "V".into(),
                fps: 10,
                width: 100,
                height: 200,
            }])
            .unwrap();
        let video = ids[0].to_string();
        let blur = |s: &EditorState| {
            let videos: Vec<Value> = serde_json::from_str(&s.snapshot().videos).unwrap();
            videos[0]["motion_blur"].as_u64().unwrap()
        };
        assert_eq!(blur(&s), 0);
        s.set_motion_blur(&video, 8).unwrap();
        assert_eq!(blur(&s), 8);
        for bad in [1, 17, -3] {
            assert!(s.set_motion_blur(&video, bad).is_err(), "{bad}");
        }
        assert_eq!(blur(&s), 8, "a refused value changes nothing");
        s.set_motion_blur(&video, 0).unwrap();
        assert_eq!(blur(&s), 0);
        s.undo().unwrap();
        assert_eq!(blur(&s), 8);
        assert!(s.set_motion_blur("v_999", 4).is_err());
    }

    #[test]
    fn the_videos_the_panel_reads_name_the_node_of_each_animation() {
        let (mut s, _t) = state();
        let id = s.add_device3d("phone").unwrap();
        let ids = s
            .apply_user(&[Operation::CreateVideo {
                name: "V".into(),
                fps: 10,
                width: 100,
                height: 200,
            }])
            .unwrap();
        s.apply_user(&[Operation::Animate {
            video: ids[0].clone(),
            node: parse_id(&id).unwrap(),
            prop: od_core::animation::AnimProp::RotateY,
            keys: vec![
                od_core::animation::Key {
                    at_ms: 0,
                    value: 0.0,
                    ease: od_core::Ease::Linear,
                },
                od_core::animation::Key {
                    at_ms: 1500,
                    value: 90.0,
                    ease: od_core::Ease::Linear,
                },
            ],
        }])
        .unwrap();
        let videos: Vec<Value> = serde_json::from_str(&s.snapshot().videos).unwrap();
        let lane = &videos[0]["animations"][0];
        assert_eq!(lane["node"], id.as_str());
        assert_eq!(lane["node_name"], "Phone 3D");
        assert_eq!(lane["prop"], "rotate_y");
        assert_eq!(lane["keys"][1]["at_ms"], 1500);
    }

    #[test]
    fn videos_dropped_in_go_to_memory_as_footage_and_a_video_previews_as_a_picture() {
        let (mut s, t) = state();
        let clip = t.path().join("take.mov");
        std::fs::write(&clip, b"\0\0\0\x14ftypqt  0000").unwrap();
        assert_eq!(s.import_to_memory(&clip).unwrap(), "media/take.mov");
        let memory = s.memory_list();
        assert_eq!(memory[0]["kind"], "video");
        assert_eq!(memory[0]["used"], false);

        let frame = s.create_node("frame", 0.0, 0.0, 200.0, 400.0).unwrap();
        let ids = s
            .apply_user(&[Operation::CreateVideo {
                name: "V".into(),
                fps: 10,
                width: 100,
                height: 200,
            }])
            .unwrap();
        let video = ids[0].to_string();
        s.apply_user(&[Operation::AddClip {
            video: ids[0].clone(),
            frame: Some(parse_id(&frame).unwrap()),
            footage: None,
            duration_ms: 1000,
            index: None,
            transition: None,
            motion: None,
        }])
        .unwrap();
        let videos: Vec<Value> = serde_json::from_str(&s.snapshot().videos).unwrap();
        assert_eq!(videos[0]["duration_ms"], 1000);
        let url = s.video_preview(&video, 0, 64).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(s.video_preview("v_999", 0, 64).is_err());
    }

    #[test]
    fn a_message_typed_while_the_agent_works_is_queued_and_always_allow_answers_its_questions() {
        let (mut s, t) = state();
        let (tx, rx) = std::sync::mpsc::channel();
        let hook: ChatHook = Arc::new(move |input| {
            let _ = tx.send(input);
        });
        let command = format!("\"{}\" normal", mock_agent().display());
        s.start_mcp(&t.path().join("token"), Arc::new(|_| {}))
            .unwrap();
        s.chat_start(&command, hook).unwrap();
        pump(&mut s, &rx, |c| c["status"] == "ready");

        s.chat_send("first").unwrap();
        pump(&mut s, &rx, |c| !c["permission"].is_null());
        s.chat_send("second").unwrap();
        let chat: Value = serde_json::from_str(&s.snapshot().chat).unwrap();
        assert_eq!(chat["queued"], 1, "typed while busy");
        assert_eq!(chat["status"], "busy");

        // allowing for good answers the waiting question and the next turn's
        s.chat_auto_allow(true);
        let chat = pump(&mut s, &rx, |c| {
            c["status"] == "ready"
                && c["queued"] == 0
                && c["messages"]
                    .as_array()
                    .is_some_and(|m| m.iter().filter(|m| m["role"] == "user").count() == 2)
        });
        assert!(chat["permission"].is_null());
        assert_eq!(chat["auto_allow"], true);
        let users: Vec<(&str, bool)> = chat["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "user")
            .map(|m| (m["text"].as_str().unwrap(), m["queued"] == true))
            .collect();
        assert_eq!(
            users,
            [("first", false), ("second", false)],
            "in order, none left queued"
        );
        s.chat_stop();
    }

    #[test]
    fn a_running_job_shows_progress_and_its_end_says_what_happened() {
        let mut s = EditorState::new();
        let update = |state, done, total| JobUpdate {
            id: 7,
            label: "Exporting reel.mp4".into(),
            done,
            total,
            state,
        };
        assert!(s.job_update(update(JobState::Running, 0, 0)).is_none());
        let rows: Vec<Value> = serde_json::from_str(&s.jobs_json()).unwrap();
        assert_eq!(
            (rows[0]["total"].as_u64(), rows[0]["cancellable"].as_bool()),
            (Some(0), Some(true))
        );
        s.job_update(update(JobState::Running, 30, 120));
        let rows: Vec<Value> = serde_json::from_str(&s.jobs_json()).unwrap();
        assert_eq!(
            (
                rows.len(),
                rows[0]["done"].as_u64(),
                rows[0]["total"].as_u64()
            ),
            (1, Some(30), Some(120))
        );
        assert_eq!(
            s.job_update(update(JobState::Finished, 1, 1)),
            Some(("Exported reel.mp4 done".to_string(), false))
        );
        assert_eq!(s.jobs_json(), "[]");
        s.job_update(update(JobState::Running, 1, 2));
        let (text, is_error) = s
            .job_update(update(JobState::Failed("no space".into()), 0, 0))
            .unwrap();
        assert!(is_error && text.contains("no space"), "{text}");
        let local = s.begin_local_job("Adding big.mov");
        assert!(s.jobs_json().contains("Adding big.mov"));
        s.end_local_job(local);
        assert_eq!(s.jobs_json(), "[]");
    }

    #[test]
    fn the_skill_is_offered_once_to_a_computer_with_an_agent_and_no_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let (home, config) = (tmp.path().join("home"), tmp.path().join("cfg"));
        std::fs::create_dir_all(&home).unwrap();
        assert!(
            !skill_offer_pending_in(&home, &config),
            "no agent, nothing to offer"
        );

        std::fs::create_dir_all(home.join(".claude")).unwrap();
        assert!(skill_offer_pending_in(&home, &config));

        let done = answer_skill_offer_in(&home, &config, true).unwrap();
        assert_eq!(done.as_array().map(Vec::len), Some(1), "{done}");
        assert!(home.join(".claude/skills/opendesigner/SKILL.md").exists());
        assert!(!skill_offer_pending_in(&home, &config), "answered once");
    }

    #[test]
    fn not_asking_again_installs_nothing_and_an_installed_skill_is_not_offered() {
        let tmp = tempfile::tempdir().unwrap();
        let (home, config) = (tmp.path().join("home"), tmp.path().join("cfg"));
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        answer_skill_offer_in(&home, &config, false).unwrap();
        assert!(!home.join(".claude/skills").exists());
        assert!(!skill_offer_pending_in(&home, &config));

        let (home2, config2) = (tmp.path().join("h2"), tmp.path().join("c2"));
        std::fs::create_dir_all(home2.join(".claude")).unwrap();
        install_skill_in(&home2).unwrap();
        assert!(
            !skill_offer_pending_in(&home2, &config2),
            "already installed"
        );
    }

    #[test]
    fn text_that_runs_past_its_box_is_listed_with_where_the_box_is() {
        let (mut s, _t) = state();
        s.create_node("frame", 0.0, 0.0, 400.0, 300.0).unwrap();
        let id = s.create_node("text", 20.0, 30.0, 120.0, 20.0).unwrap();
        s.set_property(&id, "font_size", "40").unwrap();
        s.set_property(&id, "text", "\"a long line of words that wraps over rows\"")
            .unwrap();
        let listed: Vec<Value> = serde_json::from_str(&s.snapshot().overflows).unwrap();
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0]["id"], json!(id));
        assert_eq!(
            (listed[0]["x"].as_f64(), listed[0]["y"].as_f64()),
            (Some(20.0), Some(30.0))
        );
        assert!(listed[0]["needed"].as_u64() > listed[0]["available"].as_u64());

        s.resize_node(&id, 20.0, 30.0, 380.0, 300.0).unwrap();
        assert_eq!(s.snapshot().overflows, "[]");
    }

    #[test]
    fn the_mcp_server_starts_once_reports_a_command_and_stops() {
        let tmp = tempfile::tempdir().unwrap();
        let token_file = tmp.path().join("cfg/mcp-token");
        let mut s = EditorState::new();
        s.new_project(&tmp.path().join("p.od"), "x", 100.0, 100.0)
            .unwrap();
        let url = s.start_mcp(&token_file, Arc::new(|_| {})).unwrap();
        assert!(url.starts_with("http://127.0.0.1:"));
        assert!(token_file.exists(), "the token is created where we said");
        assert!(s.snapshot().mcp_running);
        let recipes = s.connect_recipes();
        assert!(
            recipes.contains("claude mcp add") && recipes.contains(&url),
            "{recipes}"
        );
        assert!(
            !recipes.contains(std::fs::read_to_string(&token_file).unwrap().trim()),
            "no token in any recipe"
        );
        assert_eq!(
            s.start_mcp(&token_file, Arc::new(|_| {})).unwrap(),
            url,
            "starting twice is a no-op"
        );
        s.stop_mcp();
        assert!(!s.snapshot().mcp_running);
        assert_eq!(s.connect_recipes(), "[]");
    }

    fn write_png(path: &std::path::Path, w: u32, h: u32) {
        let mut bytes = Vec::new();
        let mut enc = png::Encoder::new(&mut bytes, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer
            .write_image_data(&[10, 20, 30].repeat((w * h) as usize))
            .unwrap();
        drop(writer);
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn importing_an_image_copies_it_adds_a_fitted_node_and_refuses_non_images() {
        let (mut s, t) = state();
        let small = t.path().join("small.png");
        let large = t.path().join("large.png");
        write_png(&small, 100, 50);
        write_png(&large, 1600, 800);

        let id = s.import_image(&small, None).unwrap();
        let node = insp(&s);
        assert_eq!(node["id"], id);
        assert_eq!(node["image"], "media/small.png");
        assert_eq!(
            (node["width"].as_f64(), node["height"].as_f64()),
            (Some(100.0), Some(50.0))
        );
        assert_eq!(
            (node["x"].as_f64(), node["y"].as_f64()),
            (Some(150.0), Some(125.0)),
            "centered in the 400x300 frame"
        );

        s.import_image(&large, Some((100.0, 100.0))).unwrap();
        let node = insp(&s);
        assert_eq!(
            node["width"].as_f64(),
            Some(320.0),
            "no wider than 80% of the frame"
        );
        assert_eq!(node["height"].as_f64(), Some(160.0));
        assert_eq!(
            node["x"].as_f64(),
            Some(-60.0),
            "centered on the drop point"
        );
        assert!(s.snapshot().unrendered == "[]", "both files are readable");

        let text = t.path().join("notes.png");
        std::fs::write(&text, b"not an image").unwrap();
        let before = s.snapshot().layers;
        assert!(s.import_image(&text, None).is_err());
        assert_eq!(s.snapshot().layers, before, "a refused file adds nothing");
        assert!(!t.path().join("p.od/media/notes.png").exists());

        s.undo().unwrap();
        s.undo().unwrap();
        assert!(s.selected().is_none() || insp(&s)["kind"] != "image");
    }

    #[test]
    fn skill_install_previews_state_and_writes_only_for_detected_agents() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".claude")).unwrap();
        let status = skill_status_in(home.path()).unwrap();
        let claude = status
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "claude-code")
            .unwrap();
        assert_eq!(claude["detected"], true);
        assert_eq!(claude["state"], "not installed");
        assert!(
            claude["files"][0]
                .as_str()
                .unwrap()
                .contains(".claude/skills/opendesigner/")
        );
        let done = install_skill_in(home.path()).unwrap();
        assert_eq!(
            done.as_array().unwrap().len(),
            1,
            "only Claude Code was detected"
        );
        assert!(
            home.path()
                .join(".claude/skills/opendesigner/SKILL.md")
                .exists()
        );
        assert!(!home.path().join(".agents").exists());
        let empty = tempfile::tempdir().unwrap();
        assert!(install_skill_in(empty.path()).is_err());
    }

    #[test]
    fn a_language_starts_as_a_copy_edits_only_its_own_text_and_survives_undo() {
        let (mut s, _t) = state();
        let text = s.create_node("text", 10.0, 10.0, 300.0, 100.0).unwrap();
        assert!(s.add_language("not a tag").is_err());
        assert!(
            s.switch_language("ja").is_err(),
            "unknown languages are refused"
        );

        s.add_language("ja").unwrap();
        assert_eq!(s.snapshot().language, "ja");
        let langs: Value = serde_json::from_str(&s.snapshot().languages).unwrap();
        assert_eq!(langs[0]["locale"], "ja");
        assert_eq!(langs[0]["translated"], 1);
        assert!(s.add_language("ja").is_err(), "no duplicates");

        s.set_property(&text, "text", "\"守る\"").unwrap();
        assert_eq!(insp(&s)["text"], "守る");
        assert_eq!(insp(&s)["translated"], true);
        let ja_canvas = draw_canvas(&s, 1.0).unwrap().2;

        s.switch_language("").unwrap();
        assert_eq!(insp(&s)["text"], "Text", "the default text did not change");
        assert!(insp(&s).get("translated").is_none());
        assert_ne!(
            draw_canvas(&s, 1.0).unwrap().2,
            ja_canvas,
            "the canvas follows the language"
        );

        // exports follow the active language and name their files after it
        s.switch_language("ja").unwrap();
        let out = s.export("set", None, "tr", 1, false).unwrap();
        assert_eq!(out["files"][0]["path"], "exports/tr-ja-01.png");

        // removing the language is one undoable step; an undo can also pull it from under the editor
        s.remove_language("ja").unwrap();
        assert_eq!(s.snapshot().language, "");
        s.undo().unwrap();
        assert!(s.snapshot().languages.contains("\"ja\""));
        s.switch_language("ja").unwrap();
        s.undo().unwrap();
        s.undo().unwrap();
        assert_eq!(
            s.snapshot().language,
            "",
            "a language that no longer exists is not active"
        );
        s.set_property(&text, "text", "\"Hello\"").unwrap();
        assert_eq!(
            insp(&s)["text"],
            "Hello",
            "with no language, text edits change the default"
        );
    }

    fn mock_agent() -> std::path::PathBuf {
        let exe = std::env::current_exe().unwrap();
        let name = if cfg!(windows) {
            "mock_acp_agent.exe"
        } else {
            "mock_acp_agent"
        };
        let path = exe
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("examples")
            .join(name);
        if !path.exists() {
            let built = std::process::Command::new(env!("CARGO"))
                .args([
                    "build",
                    "--example",
                    "mock_acp_agent",
                    "-p",
                    "opendesigner-agent",
                ])
                .status()
                .unwrap();
            assert!(built.success());
        }
        path
    }

    fn pump(
        s: &mut EditorState,
        rx: &std::sync::mpsc::Receiver<ChatInput>,
        done: impl Fn(&Value) -> bool,
    ) -> Value {
        loop {
            let chat: Value = serde_json::from_str(&s.snapshot().chat).unwrap();
            if done(&chat) {
                return chat;
            }
            let input = rx
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap_or_else(|_| panic!("the agent should keep talking; the chat is {chat}"));
            s.chat_apply(input);
        }
    }

    #[test]
    fn a_working_folder_opens_a_workspace_or_sets_one_up_empty_and_the_agent_starts_there() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, config) = (tmp.path().join("my-app"), tmp.path().join("cfg"));
        std::fs::create_dir_all(&repo).unwrap();

        let info = EditorState::inspect_folder(&repo);
        assert_eq!(info["found"], "none");
        assert_eq!(info["name"], "my-app");
        assert_eq!(info["workspaces"], json!([]));

        let mut s = EditorState::new();
        let err = s
            .open_workspace_in(&repo, Some("store"), false, Some(&config))
            .unwrap_err();
        assert!(err.contains("no OpenDesigner workspace"), "{err}");
        assert!(
            !repo.join("opendesigner").exists(),
            "nothing is made without a yes"
        );

        s.open_workspace_in(&repo, Some("app store"), true, Some(&config))
            .unwrap();
        assert!(
            repo.join("opendesigner/workspaces/app store/od.toml")
                .is_file()
        );
        assert!(
            repo.join(".opendesigner/workspaces/app store/README.md")
                .is_file()
        );
        assert!(
            repo.join(".opendesigner/workspaces/app store/memory")
                .is_dir()
        );
        assert_eq!(s.agent_notes(), json!([]));
        let dir = s.project_dir().unwrap();
        od_agent::memory::write(
            &dir,
            "brand.md",
            "# Brand\nInter",
            od_agent::memory::Mode::Append,
        )
        .unwrap();
        assert_eq!(s.agent_notes()[0]["name"], "brand.md");
        assert!(
            s.agent_note("brand.md")["text"]
                .as_str()
                .unwrap()
                .contains("Inter")
        );
        assert!(
            s.agent_note("../x.md")["error"].is_string(),
            "a refusal is explained"
        );
        assert!(s.snapshot().has_project);
        assert_eq!(s.snapshot().frames, "[]", "the agent decides what to make");
        assert_eq!(s.repo.as_deref(), Some(repo.as_path()));
        let listed = EditorState::inspect_folder(&repo);
        assert_eq!(listed["found"], "workspaces");
        assert_eq!(listed["workspaces"], json!(["app store"]));

        // opening again finds it; a second workspace sits beside it
        let mut again = EditorState::new();
        again
            .open_workspace_in(&repo, None, false, Some(&config))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&again.snapshot().workspaces).unwrap()["open"],
            "app store"
        );
        again
            .open_workspace_in(&repo, Some("reel"), true, Some(&config))
            .unwrap();
        assert_eq!(
            EditorState::inspect_folder(&repo)["workspaces"],
            json!(["app store", "reel"])
        );
        // a folder that is itself a project opens as it is
        let mut direct = EditorState::new();
        direct
            .open_workspace_in(
                &repo.join("opendesigner/workspaces/reel"),
                None,
                false,
                None,
            )
            .unwrap();

        let recent = EditorState::recent_projects_in(&config);
        assert_eq!(recent[0]["folder"], repo.display().to_string());
        assert_eq!(recent.as_array().map(Vec::len), Some(1), "listed once");
        std::fs::remove_dir_all(&repo).unwrap();
        assert_eq!(
            EditorState::recent_projects_in(&config),
            json!([]),
            "gone folders are not offered"
        );
        assert!(s.open_workspace_in(&repo, None, true, None).is_err());
    }

    #[test]
    fn switching_workspaces_keeps_each_ones_work_and_resets_the_view() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = EditorState::new();
        s.open_workspace_in(tmp.path(), Some("main"), true, None)
            .unwrap();
        let frame = s.create_node("frame", 0.0, 0.0, 100.0, 100.0).unwrap();
        assert!(s.snapshot().active_frame == frame);

        s.switch_workspace("reel", true).unwrap();
        let ws: Value = serde_json::from_str(&s.snapshot().workspaces).unwrap();
        assert_eq!(
            (ws["open"].as_str(), ws["names"].as_array().map(Vec::len)),
            (Some("reel"), Some(2))
        );
        assert_eq!(s.snapshot().frames, "[]", "a new workspace is empty");
        assert_eq!(s.snapshot().active_frame, "");
        s.create_node("frame", 0.0, 0.0, 50.0, 50.0).unwrap();

        s.switch_workspace("main", false).unwrap();
        let frames: Vec<Value> = serde_json::from_str(&s.snapshot().frames).unwrap();
        assert_eq!(frames.len(), 1, "the first workspace kept its own frame");
        assert!(s.switch_workspace("nope", false).is_err());
        assert!(s.switch_workspace("main", true).is_err(), "already exists");
    }

    #[test]
    fn the_user_picks_a_format_answers_the_agents_request_and_can_hand_the_choice_back() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = EditorState::new();
        s.open_workspace_in(tmp.path(), Some("reel"), true, None)
            .unwrap();
        let brief =
            |s: &EditorState| -> Value { serde_json::from_str(&s.snapshot().brief).unwrap() };
        assert_eq!(brief(&s)["chosen_by"], "none");

        s.set_format("reel", "", 0, 0, 0).unwrap();
        assert_eq!(brief(&s)["format"]["width"], 1080);
        assert_eq!(brief(&s)["chosen_by"], "user");
        assert!(s.set_format("nope", "", 0, 0, 0).is_err());
        assert!(
            s.set_format("", "video", 1081, 1920, 30).is_err(),
            "odd video size"
        );

        // the agent asks for another size; the file is how the two sides share it
        let dir = s.with_ws(|ws| Ok(ws.dir().unwrap().to_path_buf())).unwrap();
        let mut b = od_agent::formats::load_brief(&dir);
        b.agent_set(
            od_agent::formats::find("youtube")
                .map(od_agent::formats::BriefFormat::from)
                .unwrap(),
            "the footage is landscape",
        );
        od_agent::formats::save_brief(&dir, &b).unwrap();
        assert_eq!(
            brief(&s)["pending_request"]["reason"],
            "the footage is landscape"
        );
        s.answer_format(false).unwrap();
        assert_eq!(brief(&s)["format"]["width"], 1080, "kept");
        assert!(s.answer_format(true).is_err(), "nothing is waiting");

        s.set_format("", "image", 1000, 500, 0).unwrap();
        assert_eq!(brief(&s)["format"]["label"], "Custom 1000x500");
        s.set_format("", "", 0, 0, 0).unwrap();
        assert_eq!(brief(&s)["chosen_by"], "none");
        let all: Value = serde_json::from_str(&EditorState::formats()).unwrap();
        assert_eq!(all[0]["section"], "Video");
    }

    #[test]
    fn the_start_screen_can_read_an_agents_models_and_the_picks_apply_once_it_is_up() {
        let line = format!("\"{}\" settings", mock_agent().display());
        let probed = crate::chat::probe_options(&line);
        let config = probed["config"]
            .as_array()
            .unwrap_or_else(|| panic!("{probed}"));
        assert!(
            config
                .iter()
                .any(|o| o["category"] == "model" && o["choices"].as_array().unwrap().len() == 2)
        );
        assert!(config.iter().any(|o| o["category"] == "thought_level"));
        assert!(crate::chat::probe_options("definitely-not-an-agent-xyz")["error"].is_string());
    }

    #[test]
    fn the_first_frame_the_agent_makes_becomes_the_open_one() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = EditorState::new();
        s.open_workspace_in(tmp.path(), None, true, None).unwrap();
        s.ensure_active_frame();
        assert_eq!(s.snapshot().active_frame, "");
        s.apply_user(&[Operation::Create {
            parent: s
                .with_ws(|ws| Ok(ws.session.document().pages[0].root.id.clone()))
                .unwrap(),
            index: None,
            node: {
                let mut f = NewNode::new(NodeKind::Frame);
                f.width = 100.0;
                f.height = 100.0;
                f
            },
        }])
        .unwrap();
        s.ensure_active_frame();
        assert!(
            s.snapshot().active_frame.starts_with("n_"),
            "{}",
            s.snapshot().active_frame
        );
    }

    #[test]
    fn the_task_typed_on_the_start_screen_goes_to_the_agent_once_it_is_connected() {
        let (mut s, t) = state();
        let (tx, rx) = std::sync::mpsc::channel();
        let hook: ChatHook = Arc::new(move |input| {
            let _ = tx.send(input);
        });
        let command = format!("\"{}\" normal", mock_agent().display());
        s.start_mcp(&t.path().join("token"), Arc::new(|_| {}))
            .unwrap();
        s.chat_start(&command, hook).unwrap();
        s.chat_first_prompt("  start the app and make the store set  ");
        let chat = pump(&mut s, &rx, |c| {
            c["messages"]
                .as_array()
                .is_some_and(|m| m.iter().any(|m| m["role"] == "user"))
        });
        let said: Vec<&str> = chat["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "user")
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(said, ["start the app and make the store set"]);
        assert_eq!(chat["status"], "busy", "the agent is working on it");
        s.chat_stop();
    }

    #[test]
    fn the_chat_runs_a_scripted_agent_through_the_editor_and_hands_it_the_local_server() {
        let (mut s, t) = state();
        let (tx, rx) = std::sync::mpsc::channel();
        let hook: ChatHook = Arc::new(move |input| {
            let _ = tx.send(input);
        });
        let command = format!(
            "\"{}\" normal {}",
            mock_agent().display(),
            t.path().join("new.json").display()
        );
        assert!(
            s.chat_start(&command, hook.clone()).is_err(),
            "the agent connection must be on first"
        );
        assert!(s.chat_send("hi").is_err());

        s.start_mcp(&t.path().join("token"), Arc::new(|_| {}))
            .unwrap();
        s.chat_start(&command, hook).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&s.snapshot().chat).unwrap()["status"],
            "starting"
        );
        pump(&mut s, &rx, |c| c["status"] == "ready");

        // the agent was given this editor's MCP server and the bearer token, and nothing else
        let new: Value =
            serde_json::from_str(&std::fs::read_to_string(t.path().join("new.json")).unwrap())
                .unwrap();
        let token = std::fs::read_to_string(t.path().join("token")).unwrap();
        assert_eq!(
            new["mcpServers"][0]["headers"][0]["value"],
            format!("Bearer {}", token.trim())
        );
        assert!(
            new["mcpServers"][0]["url"]
                .as_str()
                .unwrap()
                .starts_with("http://127.0.0.1:")
        );
        assert_eq!(new["cwd"], t.path().join("p.od").display().to_string());

        s.chat_send("make the headline bigger").unwrap();
        let waiting = pump(&mut s, &rx, |c| !c["permission"].is_null());
        assert_eq!(waiting["status"], "busy");
        let request = waiting["permission"]["request"]
            .as_str()
            .unwrap()
            .to_string();
        s.chat_answer(
            &request,
            waiting["permission"]["options"][0]["id"].as_str().unwrap(),
        )
        .unwrap();
        let done = pump(&mut s, &rx, |c| c["status"] == "ready");
        let said: String = done["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "agent")
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(said, "Hello world [fs refused] [chose yes]");
        assert_eq!(done["messages"][0]["role"], "note");
        assert_eq!(done["messages"][1]["text"], "make the headline bigger");

        s.chat_stop();
        assert_eq!(
            serde_json::from_str::<Value>(&s.snapshot().chat).unwrap()["status"],
            "off"
        );
        assert!(s.chat_send("hello?").is_err());
    }

    #[test]
    fn the_freesound_token_is_stored_in_the_config_folder_and_can_be_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("freesound/token");
        assert!(!od_agent::freesound::has_token_in(&path));
        assert!(od_agent::freesound::token_in(&path).is_err());
        od_agent::freesound::save_token_in(&path, " a-token ").unwrap();
        assert!(od_agent::freesound::has_token_in(&path));
        assert_eq!(od_agent::freesound::token_in(&path).unwrap(), "a-token");
        od_agent::freesound::clear_token_in(&path).unwrap();
        assert!(!od_agent::freesound::has_token_in(&path));
        // an empty token is not a token
        assert!(od_agent::freesound::save_token_in(&path, "  ").is_err());
    }

    #[test]
    fn the_user_can_take_the_sound_track_off_a_video() {
        let (mut s, _t) = state();
        let video = s
            .with_ws(|ws| {
                let receipt = ws
                    .session
                    .apply(
                        Actor::User,
                        None,
                        &[Operation::CreateVideo {
                            name: "Teaser".into(),
                            fps: 30,
                            width: 400,
                            height: 300,
                        }],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(receipt.created[0].clone())
            })
            .unwrap();
        s.with_ws(|ws| {
            ws.session
                .apply(
                    Actor::User,
                    None,
                    &[Operation::SetVideo {
                        id: video.clone(),
                        prop: od_core::VideoProperty::Audio(Some(od_core::AudioTrack {
                            file: "media/song.mp3".into(),
                            start_ms: 0,
                            gain: 1.0,
                            fade_in_ms: 0,
                            fade_out_ms: 0,
                        })),
                    }],
                )
                .map_err(|e| e.to_string())
        })
        .unwrap();
        let videos: Value = serde_json::from_str(&s.snapshot().videos).unwrap();
        assert_eq!(videos[0]["audio"]["file"], "media/song.mp3");

        s.remove_video_audio(&video.to_string()).unwrap();
        let videos: Value = serde_json::from_str(&s.snapshot().videos).unwrap();
        assert!(
            videos[0]["audio"].is_null(),
            "the sound track should be gone"
        );

        // it is an undoable edit like any other
        s.undo().unwrap();
        let videos: Value = serde_json::from_str(&s.snapshot().videos).unwrap();
        assert_eq!(videos[0]["audio"]["file"], "media/song.mp3");
    }
    #[test]
    fn the_snapshot_publishes_sound_tracks_sound_rows_and_captions_for_the_timeline() {
        let (s, _t) = state();
        let video = s
            .with_ws(|ws| {
                let created = ws
                    .session
                    .apply(
                        Actor::User,
                        None,
                        &[Operation::CreateVideo {
                            name: "Teaser".into(),
                            fps: 30,
                            width: 400,
                            height: 300,
                        }],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(created.created[0].clone())
            })
            .unwrap();
        let frame = s.snapshot().active_frame;
        s.with_ws(|ws| {
            let v = ws.session.document().video(&video).cloned();
            assert!(v.is_some());
            ws.session
                .apply(
                    Actor::User,
                    None,
                    &[Operation::AddClip {
                        video: video.clone(),
                        frame: NodeId::parse(&frame).ok(),
                        footage: None,
                        duration_ms: 4000,
                        index: None,
                        transition: None,
                        motion: None,
                    }],
                )
                .map_err(|e| e.to_string())?;
            let doc = ws.session.document().video(&video).cloned().unwrap();
            let (tracks, _) = doc
                .tracks_with_added(
                    serde_json::from_value(json!({"role": "voice", "file": "media/vo.wav"}))
                        .unwrap(),
                )
                .map_err(|e| e.to_string())?;
            let cues =
                od_core::captions::parse_subtitles("1\n00:00:00,500 --> 00:00:01,500\nHello\n")
                    .map_err(|e| e.to_string())?;
            let captions = doc.captions_with_cues(cues).map_err(|e| e.to_string())?;
            ws.session
                .apply(
                    Actor::User,
                    None,
                    &[
                        Operation::SetVideo {
                            id: video.clone(),
                            prop: od_core::VideoProperty::Tracks(tracks),
                        },
                        Operation::SetVideo {
                            id: video.clone(),
                            prop: od_core::VideoProperty::Captions(captions),
                        },
                    ],
                )
                .map_err(|e| e.to_string())?;
            Ok(())
        })
        .unwrap();
        let videos: Value = serde_json::from_str(&s.snapshot().videos).unwrap();
        let v = &videos[0];
        assert_eq!(v["tracks"][0]["role"], "voice");
        assert_eq!(v["sound_rows"][0]["id"], "a1");
        assert_eq!(v["sound_rows"][0]["legacy"], false);
        assert_eq!(v["captions"]["cues"][0]["text"], "Hello");
        assert_eq!(v["captions"]["cues"][0]["end_ms"], 1500);
        assert!(v["captions"]["style"]["font_family"].is_string());
    }

    fn offer_two(s: &EditorState) -> (Vec<String>, Value) {
        let frames: Vec<String> = serde_json::from_str::<Vec<Value>>(&s.snapshot().frames)
            .unwrap()
            .iter()
            .map(|f| f["id"].as_str().unwrap().to_string())
            .collect();
        let dir = s.project_dir().unwrap();
        let opts: Vec<_> = frames
            .iter()
            .map(|f| od_agent::showcase::NewOption {
                label: format!("Look {f}"),
                kind: od_agent::showcase::Kind::Frame,
                target: f.clone(),
                note: None,
            })
            .collect();
        let mut saved = od_agent::showcase::load(&dir);
        s.with_ws(|ws| {
            saved
                .offer(ws, "Directions", None, &opts, "agent")
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap();
        od_agent::showcase::save(&dir, &saved).unwrap();
        (
            frames,
            serde_json::from_str(&s.snapshot().showcase).unwrap(),
        )
    }

    #[test]
    fn a_pick_is_stored_for_get_variants_and_the_snapshot_without_a_chat() {
        let (mut s, _t) = state();
        s.create_node("frame", 0.0, 0.0, 120.0, 90.0).unwrap();
        let (frames, shown) = offer_two(&s);
        assert_eq!(shown["variants"]["picked"], Value::Null);
        assert!(!s.pick_variant("v2").unwrap(), "no chat to tell");
        let after: Value = serde_json::from_str(&s.snapshot().showcase).unwrap();
        assert_eq!(after["variants"]["picked"], "v2");
        let stored = od_agent::showcase::load(&s.project_dir().unwrap());
        assert_eq!(
            stored.variants.unwrap().picked_option().unwrap().target,
            frames[1]
        );
        assert!(s.pick_variant("v9").is_err());
        s.dismiss_variants().unwrap();
        assert!(
            serde_json::from_str::<Value>(&s.snapshot().showcase).unwrap()["variants"].is_null()
        );
    }

    #[test]
    fn picking_with_a_chat_connected_tells_the_agent_which_variant() {
        let (mut s, t) = state();
        s.create_node("frame", 0.0, 0.0, 120.0, 90.0).unwrap();
        offer_two(&s);
        let (tx, rx) = std::sync::mpsc::channel();
        let hook: ChatHook = Arc::new(move |input| {
            let _ = tx.send(input);
        });
        let command = format!("\"{}\" normal", mock_agent().display());
        s.start_mcp(&t.path().join("token"), Arc::new(|_| {}))
            .unwrap();
        s.chat_start(&command, hook).unwrap();
        pump(&mut s, &rx, |c| c["status"] == "ready");
        assert!(s.pick_variant("v1").unwrap());
        let chat = pump(&mut s, &rx, |c| {
            c["messages"]
                .as_array()
                .is_some_and(|m| m.iter().any(|m| m["role"] == "user"))
        });
        let said: Vec<&str> = chat["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "user")
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(said.len(), 1);
        assert!(
            said[0].starts_with("I pick variant Look ") && said[0].ends_with(". Continue with it."),
            "{said:?}"
        );
        s.chat_stop();
    }

    #[test]
    fn a_frame_that_is_not_open_renders_to_a_png_and_the_preview_override_is_checked() {
        let (mut s, _t) = state();
        let first = s.snapshot().active_frame;
        s.create_node("frame", 0.0, 0.0, 120.0, 90.0).unwrap();
        assert_ne!(
            s.snapshot().active_frame,
            first,
            "the new frame is the open one"
        );
        let url = s.render_frame_image(&first, 64).unwrap();
        let png = STANDARD
            .decode(url.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        assert_eq!(&png[..4], b"\x89PNG");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 64);
        assert!(s.render_frame_image("n_99999", 64).is_err());

        let view =
            |s: &EditorState| -> Value { serde_json::from_str(&s.snapshot().showcase).unwrap() };
        assert_eq!(view(&s)["preview"]["chosen_by"], "default");
        s.set_preview_override("frame", &first).unwrap();
        assert_eq!(view(&s)["preview"]["target"], first.as_str());
        assert_eq!(view(&s)["preview"]["chosen_by"], "user");
        assert!(s.set_preview_override("frame", "n_99999").is_err());
        s.clear_preview_override();
        assert_eq!(view(&s)["preview"]["chosen_by"], "default");
    }

    #[test]
    fn edits_redraw_only_their_tiles_and_equal_a_fresh_draw() {
        use crate::canvas::Drawn;
        let (mut s, _t) = state();
        let id = s.create_node("rect", 10.0, 10.0, 80.0, 40.0).unwrap();
        s.create_node("ellipse", 200.0, 250.0, 60.0, 40.0).unwrap();
        let (_, _, _, first) = draw_canvas(&s, 1.0).unwrap();
        assert_eq!(first, Drawn::Full);
        assert_eq!(draw_canvas(&s, 1.0).unwrap().3, Drawn::Reused);

        s.move_node(&id, 30.0, 12.0).unwrap();
        let (w, h, kept, drawn) = draw_canvas(&s, 1.0).unwrap();
        let Drawn::Tiles { redrawn, tiles } = drawn else {
            panic!("expected a partial redraw, got {drawn:?}");
        };
        assert!(redrawn < tiles, "{redrawn} of {tiles}");

        s.undo().unwrap();
        s.redo().unwrap();
        s.set_property(&id, "width", "150").unwrap();
        let kept_after = draw_canvas(&s, 1.0).unwrap();
        s.forget_canvas();
        let fresh = draw_canvas(&s, 1.0).unwrap();
        assert_eq!(fresh.3, Drawn::Full);
        assert_eq!((w, h), (fresh.0, fresh.1));
        assert!(
            kept_after.2 == fresh.2,
            "the kept picture equals a fresh draw"
        );
        assert_ne!(kept, fresh.2);
    }

    #[test]
    fn a_new_scale_language_or_project_draws_the_frame_whole() {
        use crate::canvas::Drawn;
        let (mut s, _t) = state();
        s.create_node("rect", 10.0, 10.0, 80.0, 40.0).unwrap();
        s.create_node("text", 10.0, 60.0, 200.0, 30.0).unwrap();
        draw_canvas(&s, 1.0).unwrap();
        assert_eq!(draw_canvas(&s, 0.5).unwrap().3, Drawn::Full, "scale");
        s.add_language("ja").unwrap();
        s.switch_language("ja").unwrap();
        assert_eq!(draw_canvas(&s, 0.5).unwrap().3, Drawn::Full, "language");
        s.switch_language("").unwrap();
        assert_eq!(
            draw_canvas(&s, 0.5).unwrap().3,
            Drawn::Full,
            "back to the default"
        );
        let (_, _, _, again) = draw_canvas(&s, 0.5).unwrap();
        assert_eq!(again, Drawn::Reused);
        let other = tempfile::tempdir().unwrap();
        s.new_project(&other.path().join("q.od"), "Other", 400.0, 300.0)
            .unwrap();
        assert_eq!(draw_canvas(&s, 0.5).unwrap().3, Drawn::Full, "project");
    }
}

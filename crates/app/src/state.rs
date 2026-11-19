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
use od_agent::mcp::{ChangeEvent, Handler};
use od_agent::skill;
use od_agent::{RunningServer, token};
use od_core::annotations::AnnotationTarget;
use od_core::geometry::{self, Rect};
use od_core::view;
use od_core::{Actor, NewNode, Node, NodeId, NodeKind, Operation, Paint, Property, Workspace};
use od_store::devices::{self, Device};
use serde_json::{Value, json};

const MAX_CANVAS_SIDE: f64 = 4096.0;
const LOG_ROWS: usize = 60;

type Res<T> = Result<T, String>;

struct Server {
    // The runtime owns the server's tasks; it must outlive `running`.
    runtime: tokio::runtime::Runtime,
    running: Option<RunningServer>,
    url: String,
    token_file: PathBuf,
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
    pub tokens: String,
    pub languages: String,
    pub language: String,
    pub chat: String,
    pub can_undo: bool,
    pub can_redo: bool,
    pub mcp_running: bool,
    pub mcp_url: String,
}

fn rect_json(r: Option<Rect>) -> Value {
    match r {
        Some(r) => json!({"x": r.x, "y": r.y, "width": r.width, "height": r.height}),
        None => Value::Null,
    }
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
        let first_frame = view::frames(ws.session.document())
            .first()
            .and_then(|f| NodeId::parse(&f.id).ok());
        self.ws = Some(Arc::new(Mutex::new(ws)));
        self.active_frame = first_frame;
        self.language = None;
        self.agent_changed.clear();
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

    /// What the start screen should say about `folder`: it is a project itself, it holds one in
    /// `opendesigner/`, or it has none yet (one would be set up in `opendesigner/`).
    pub fn inspect_folder(folder: &Path) -> Value {
        let nested = folder.join(PROJECT_DIR);
        let name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Project");
        let (found, project) = if folder.join("od.toml").is_file() {
            ("project", folder.to_path_buf())
        } else if nested.join("od.toml").is_file() {
            ("nested", nested)
        } else {
            ("none", nested)
        };
        json!({
            "found": found,
            "folder": folder.display().to_string(),
            "project": project.display().to_string(),
            "name": name,
            "exists": folder.is_dir(),
        })
    }

    /// Opens the project in `folder` (see [`EditorState::inspect_folder`]), setting one up there
    /// when it has none and `create` is true. The agent will work in `folder`. The first frame is
    /// not made: what exists is up to the agent.
    pub fn open_workspace(&mut self, folder: &Path, create: bool) -> Res<()> {
        self.open_workspace_in(folder, create, token::config_dir().ok().as_deref())
    }

    /// [`EditorState::open_workspace`] remembering the folder in `config` (`None`: not at all).
    pub fn open_workspace_in(
        &mut self,
        folder: &Path,
        create: bool,
        config: Option<&Path>,
    ) -> Res<()> {
        let info = Self::inspect_folder(folder);
        if !folder.is_dir() {
            return Err(format!("{} is not a folder", folder.display()));
        }
        let project = PathBuf::from(info["project"].as_str().unwrap_or_default());
        match info["found"].as_str() {
            Some("project" | "nested") => self.open_project(&project)?,
            _ if create => {
                let name = info["name"].as_str().unwrap_or("Project").to_string();
                let ws = Workspace::create(&project, &name).map_err(|e| e.to_string())?;
                self.adopt(ws);
            }
            _ => {
                return Err(format!(
                    "{} has no OpenDesigner project yet",
                    folder.display()
                ));
            }
        }
        self.repo = Some(folder.to_path_buf());
        if let Some(dir) = config {
            remember_recent(folder, dir);
        }
        Ok(())
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
            NodeKind::Group | NodeKind::Image => {}
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
    pub fn import_to_memory(&mut self, src: &std::path::Path) -> Res<String> {
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

    pub fn delete_selected(&mut self) -> Res<()> {
        let Some(id) = self.selected() else {
            return Ok(());
        };
        self.apply_user(&[Operation::Delete { id: id.clone() }])?;
        if self.active_frame.as_ref() == Some(&id) {
            self.active_frame = None;
        }
        Ok(())
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

    /// The active frame as a PNG data URL at `scale`, limited to 4096 px on a side.
    pub fn render_canvas(&self, scale: f64) -> Res<String> {
        let frame = self.active_frame.as_ref().ok_or("no frame is open")?;
        self.with_ws(|ws| {
            let localized = self
                .language
                .as_deref()
                .and_then(|l| ws.session.document().localized(l));
            let doc = localized.as_ref().unwrap_or_else(|| ws.session.document());
            let node = doc.node(frame).ok_or("the frame no longer exists")?;
            let longest = node.width.max(node.height).max(1.0);
            let scale = scale.clamp(0.01, MAX_CANVAS_SIDE / longest).min(16.0);
            let rendered = od_render::render_frame(doc, frame, scale as f32, ws.dir())
                .map_err(|e| e.to_string())?;
            let png = od_render::encode_png(&rendered.pixmap, false).map_err(|e| e.to_string())?;
            Ok(format!("data:image/png;base64,{}", STANDARD.encode(png)))
        })
    }

    /// Exports into the project's `exports/`: `mode` is `frame` (the open frame), `set` (every
    /// frame on the open frame's page, in drawing order, as `<name>-01.png`, ...) or `panorama`
    /// (the open frame cut into `slices` columns). With a preset, nothing is written unless every
    /// image and the count meet it.
    pub fn export(&self, mode: &str, preset: Option<&str>, name: &str, slices: u32) -> Res<Value> {
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
                false,
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

    /// Puts a new image into the node `id` (an image node, for example a device's screen),
    /// keeping its position and size.
    pub fn replace_image(&mut self, src: &std::path::Path, id: &str) -> Res<String> {
        let id = parse_id(id)?;
        let reference = self.with_ws(|ws| {
            let node = ws
                .session
                .document()
                .node(&id)
                .ok_or_else(|| format!("{id} does not exist"))?;
            if node.kind != NodeKind::Image {
                return Err(format!("{id} is a {}, not an image", node.kind.as_str()));
            }
            let reference = ws.import_media(src).map_err(|e| e.to_string())?;
            let path = ws.media_path(&reference).map_err(|e| e.to_string())?;
            od_render::image_dimensions(&path).inspect_err(|_| {
                let _ = std::fs::remove_file(&path);
            })?;
            Ok(reference)
        })?;
        self.apply_user(&[Operation::Set {
            id: id.clone(),
            prop: Property::Image(Some(reference)),
        }])?;
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
            Some((id, NodeKind::Image, _)) => self.replace_image(src, id.as_str()),
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
        if let Some(s) = &self.server {
            return Ok(s.url.clone());
        }
        let ws = self.ws.clone().ok_or("open a project first")?;
        let secret = token::load_or_create(token_file).map_err(|e| e.to_string())?;
        let handler = Arc::new(Handler::new(ws).with_change_hook(on_change));
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let running = runtime
            .block_on(od_agent::start(handler, secret, od_agent::DEFAULT_PORT))
            .map_err(|e| e.to_string())?;
        let url = running.url();
        self.server = Some(Server {
            runtime,
            running: Some(running),
            url: url.clone(),
            token_file: token_file.to_path_buf(),
        });
        Ok(url)
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
    /// folder as its working directory and the editor's MCP server as its tool. The agent connection
    /// must be on.
    pub fn chat_start(&mut self, command_line: &str, hook: ChatHook) -> Res<()> {
        let (program, args) = split_command(command_line)?;
        let project_dir = self.with_ws(|ws| {
            ws.dir()
                .map(Path::to_path_buf)
                .ok_or_else(|| "the project has no folder".to_string())
        })?;
        // The agent works in the folder the user chose (a repository, so it can run the app and
        // take screenshots); the project is a folder inside it.
        let cwd = self.repo.clone().unwrap_or(project_dir);
        let server = self
            .server
            .as_ref()
            .ok_or("turn on the agent connection first")?;
        let mcp = McpEndpoint {
            url: server.url.clone(),
            token: token::load_or_create(&server.token_file).map_err(|e| e.to_string())?,
        };
        if self.chat.is_none() {
            self.chat = Some(Chat::new()?);
        }
        if let Some(chat) = self.chat.as_mut() {
            chat.start(AgentCommand { program, args, cwd }, mcp, hook);
        }
        Ok(())
    }

    /// Sends `prompt` to the agent as soon as it is ready (after [`EditorState::chat_start`]).
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

    pub fn chat_cancel(&self) {
        if let Some(c) = &self.chat {
            c.cancel();
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
            selection_bounds: rect_json(selection.first().and_then(&in_frame)).to_string(),
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
    fn the_canvas_renders_to_a_png_data_url_and_exports_validate_against_presets() {
        let (mut s, _t) = state();
        s.create_node("rect", 10.0, 10.0, 80.0, 40.0).unwrap();
        let url = s.render_canvas(1.0).unwrap();
        let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
        let bytes = STANDARD.decode(b64).unwrap();
        let facts = od_store::inspect_png(&bytes).unwrap();
        assert_eq!((facts.width, facts.height), (400, 300));
        // huge scales are clamped instead of exhausting memory
        let big = s.render_canvas(100.0).unwrap();
        assert!(big.len() < 40_000_000);

        let err = s
            .export("frame", Some("apple-iphone-6.9"), "shot.png", 1)
            .unwrap_err();
        assert!(err.contains("1260x2736"), "{err}");
        let ok = s.export("frame", None, "draft.png", 1).unwrap();
        assert_eq!(ok["files"][0]["path"], "exports/draft.png");
        assert!(s.export("frame", None, "../x.png", 1).is_err());
        assert!(s.export("zip", None, "x", 1).is_err());
    }

    #[test]
    fn sets_and_panoramas_export_all_or_nothing() {
        let (mut s, t) = state();
        s.create_node("frame", 0.0, 0.0, 400.0, 300.0).unwrap();
        let exports = t.path().join("p.od/exports");

        let set = s.export("set", None, "slide", 1).unwrap();
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
            .export("set", Some("google-phone"), "play", 1)
            .unwrap_err();
        assert!(refused.contains("play-01.png"), "{refused}");
        assert!(!exports.join("play-01.png").exists());

        let pano = s.export("panorama", None, "wide", 2).unwrap();
        assert_eq!(pano["files"][0]["width"], 200);
        assert!(s.export("panorama", None, "odd", 7).is_err());
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
        assert!(s.render_canvas(1.0).is_err());
        assert_eq!(s.hit_test(0.0, 0.0), None);
        assert_eq!(s.connect_recipes(), "[]");
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
        let ja_canvas = s.render_canvas(1.0).unwrap();

        s.switch_language("").unwrap();
        assert_eq!(insp(&s)["text"], "Text", "the default text did not change");
        assert!(insp(&s).get("translated").is_none());
        assert_ne!(
            s.render_canvas(1.0).unwrap(),
            ja_canvas,
            "the canvas follows the language"
        );

        // exports follow the active language and name their files after it
        s.switch_language("ja").unwrap();
        let out = s.export("set", None, "tr", 1).unwrap();
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
                .expect("the agent should keep talking");
            s.chat_apply(input);
        }
    }

    #[test]
    fn a_working_folder_opens_its_project_or_sets_one_up_empty_and_the_agent_starts_there() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, config) = (tmp.path().join("my-app"), tmp.path().join("cfg"));
        std::fs::create_dir_all(&repo).unwrap();

        let info = EditorState::inspect_folder(&repo);
        assert_eq!(info["found"], "none");
        assert_eq!(info["name"], "my-app");
        assert_eq!(
            info["project"],
            repo.join("opendesigner").display().to_string()
        );

        let mut s = EditorState::new();
        let err = s
            .open_workspace_in(&repo, false, Some(&config))
            .unwrap_err();
        assert!(err.contains("no OpenDesigner project yet"), "{err}");
        assert!(
            !repo.join("opendesigner").exists(),
            "nothing is made without a yes"
        );

        s.open_workspace_in(&repo, true, Some(&config)).unwrap();
        assert!(repo.join("opendesigner/od.toml").is_file());
        assert!(s.snapshot().has_project);
        assert_eq!(s.snapshot().frames, "[]", "the agent decides what to make");
        assert_eq!(s.snapshot().project_name, "my-app");
        assert_eq!(s.repo.as_deref(), Some(repo.as_path()));
        assert_eq!(EditorState::inspect_folder(&repo)["found"], "nested");

        // opening again (also from the project folder itself) finds it
        let mut again = EditorState::new();
        again
            .open_workspace_in(&repo, false, Some(&config))
            .unwrap();
        again
            .open_workspace_in(&repo.join("opendesigner"), false, None)
            .unwrap();
        assert_eq!(
            EditorState::inspect_folder(&repo.join("opendesigner"))["found"],
            "project"
        );

        let recent = EditorState::recent_projects_in(&config);
        assert_eq!(recent[0]["folder"], repo.display().to_string());
        assert_eq!(recent.as_array().map(Vec::len), Some(1), "listed once");
        std::fs::remove_dir_all(&repo).unwrap();
        assert_eq!(
            EditorState::recent_projects_in(&config),
            json!([]),
            "gone folders are not offered"
        );
        assert!(s.open_workspace_in(&repo, true, None).is_err());
    }

    #[test]
    fn the_first_frame_the_agent_makes_becomes_the_open_one() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = EditorState::new();
        s.open_workspace_in(tmp.path(), true, None).unwrap();
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
        assert!(s.chat_send("again").is_err(), "one turn at a time");
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
}

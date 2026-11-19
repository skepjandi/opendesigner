//! The one QObject QML talks to. It forwards every action to `EditorState` (Qt-free, tested on its
//! own) and publishes the resulting snapshot as properties. No document logic lives here.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    impl cxx_qt::Threading for Editor {}

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, has_project, cxx_name = "hasProject")]
        #[qproperty(QString, project_name, cxx_name = "projectName")]
        #[qproperty(QString, layers)]
        #[qproperty(QString, frames)]
        #[qproperty(QString, active_frame, cxx_name = "activeFrame")]
        #[qproperty(QString, frame_size, cxx_name = "frameSize")]
        #[qproperty(QString, selection)]
        #[qproperty(QString, selection_bounds, cxx_name = "selectionBounds")]
        #[qproperty(QString, inspector)]
        #[qproperty(QString, annotations)]
        #[qproperty(QString, checkpoints)]
        #[qproperty(QString, log)]
        #[qproperty(QString, changed)]
        #[qproperty(QString, unrendered)]
        #[qproperty(QString, overflows)]
        #[qproperty(QString, memory)]
        #[qproperty(QString, tokens)]
        #[qproperty(QString, languages)]
        #[qproperty(QString, language)]
        #[qproperty(QString, chat)]
        #[qproperty(bool, can_undo, cxx_name = "canUndo")]
        #[qproperty(bool, can_redo, cxx_name = "canRedo")]
        #[qproperty(bool, mcp_running, cxx_name = "mcpRunning")]
        #[qproperty(QString, mcp_url, cxx_name = "mcpUrl")]
        #[qproperty(QString, canvas)]
        #[qproperty(QString, message)]
        #[qproperty(bool, message_is_error, cxx_name = "messageIsError")]
        #[qproperty(i32, message_seq, cxx_name = "messageSeq")]
        #[qproperty(i32, revision)]
        type Editor = super::EditorRust;

        #[qinvokable]
        #[cxx_name = "newProject"]
        fn new_project(
            self: Pin<&mut Self>,
            path: &QString,
            name: &QString,
            width: f64,
            height: f64,
        );
        #[qinvokable]
        #[cxx_name = "openProject"]
        fn open_project(self: Pin<&mut Self>, path: &QString);
        #[qinvokable]
        #[cxx_name = "inspectFolder"]
        fn inspect_folder(self: &Self, url: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "openWorkspace"]
        fn open_workspace(self: Pin<&mut Self>, url: &QString, create: bool) -> bool;
        #[qinvokable]
        #[cxx_name = "recentProjects"]
        fn recent_projects(self: &Self) -> QString;
        #[qinvokable]
        #[cxx_name = "chatStartWith"]
        fn chat_start_with(self: Pin<&mut Self>, command: &QString, prompt: &QString);
        #[qinvokable]
        #[cxx_name = "saveProject"]
        fn save_project(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "framePresets"]
        fn frame_presets(self: &Self) -> QString;
        #[qinvokable]
        #[cxx_name = "localPath"]
        fn local_path(self: &Self, url: &QString) -> QString;

        #[qinvokable]
        #[cxx_name = "openFrame"]
        fn set_active_frame_id(self: Pin<&mut Self>, id: &QString);
        #[qinvokable]
        fn select(self: Pin<&mut Self>, id: &QString);
        #[qinvokable]
        #[cxx_name = "hitTest"]
        fn hit_test(self: &Self, x: f64, y: f64) -> QString;
        #[qinvokable]
        #[cxx_name = "createNode"]
        fn create_node(
            self: Pin<&mut Self>,
            kind: &QString,
            x: f64,
            y: f64,
            w: f64,
            h: f64,
        ) -> QString;
        #[qinvokable]
        #[cxx_name = "importImage"]
        fn import_image(self: Pin<&mut Self>, url: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "importToMemory"]
        fn import_to_memory(self: Pin<&mut Self>, url: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "replaceImage"]
        fn replace_image(self: Pin<&mut Self>, url: &QString, id: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "dropImage"]
        fn drop_image(self: Pin<&mut Self>, url: &QString, x: f64, y: f64) -> QString;
        #[qinvokable]
        #[cxx_name = "deviceKinds"]
        fn device_kinds(self: &Self) -> QString;
        #[qinvokable]
        #[cxx_name = "addDevice"]
        fn add_device(self: Pin<&mut Self>, kind: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "deleteSelected"]
        fn delete_selected(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "moveNode"]
        fn move_node(self: Pin<&mut Self>, id: &QString, x: f64, y: f64);
        #[qinvokable]
        #[cxx_name = "resizeNode"]
        fn resize_node(self: Pin<&mut Self>, id: &QString, x: f64, y: f64, w: f64, h: f64);
        #[qinvokable]
        #[cxx_name = "setNodeProperty"]
        fn set_node_property(
            self: Pin<&mut Self>,
            id: &QString,
            name: &QString,
            value_json: &QString,
        );
        #[qinvokable]
        #[cxx_name = "toggleVisible"]
        fn toggle_visible(self: Pin<&mut Self>, id: &QString);
        #[qinvokable]
        #[cxx_name = "switchLanguage"]
        fn switch_language(self: Pin<&mut Self>, tag: &QString);
        #[qinvokable]
        #[cxx_name = "addLanguage"]
        fn add_language(self: Pin<&mut Self>, tag: &QString);
        #[qinvokable]
        #[cxx_name = "removeLanguage"]
        fn remove_language(self: Pin<&mut Self>, tag: &QString);
        #[qinvokable]
        #[cxx_name = "setToken"]
        fn set_token(self: Pin<&mut Self>, name: &QString, value: &QString);
        #[qinvokable]
        #[cxx_name = "removeToken"]
        fn remove_token(self: Pin<&mut Self>, name: &QString);
        #[qinvokable]
        fn reorder(self: Pin<&mut Self>, id: &QString, delta: i32);

        #[qinvokable]
        fn undo(self: Pin<&mut Self>);
        #[qinvokable]
        fn redo(self: Pin<&mut Self>);
        #[qinvokable]
        fn checkpoint(self: Pin<&mut Self>, name: &QString);
        #[qinvokable]
        fn restore(self: Pin<&mut Self>, checkpoint: i32);

        #[qinvokable]
        #[cxx_name = "addPin"]
        fn add_pin(self: Pin<&mut Self>, note: &QString, node: &QString) -> i32;
        #[qinvokable]
        #[cxx_name = "addRegionPin"]
        fn add_region_pin(
            self: Pin<&mut Self>,
            note: &QString,
            x: f64,
            y: f64,
            w: f64,
            h: f64,
        ) -> i32;
        #[qinvokable]
        #[cxx_name = "resolvePin"]
        fn resolve_pin(self: Pin<&mut Self>, id: i32);
        #[qinvokable]
        #[cxx_name = "reopenPin"]
        fn reopen_pin(self: Pin<&mut Self>, id: i32);
        #[qinvokable]
        #[cxx_name = "removePin"]
        fn remove_pin(self: Pin<&mut Self>, id: i32);
        #[qinvokable]
        #[cxx_name = "clearAgentChanges"]
        fn clear_agent_changes(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "renderCanvas"]
        fn render_canvas(self: Pin<&mut Self>, scale: f64);
        #[qinvokable]
        #[cxx_name = "exportAssets"]
        fn export_assets(
            self: Pin<&mut Self>,
            mode: &QString,
            preset: &QString,
            name: &QString,
            slices: i32,
        ) -> QString;

        #[qinvokable]
        #[cxx_name = "chatStart"]
        fn chat_start(self: Pin<&mut Self>, command: &QString);
        #[qinvokable]
        #[cxx_name = "chatSend"]
        fn chat_send(self: Pin<&mut Self>, text: &QString);
        #[qinvokable]
        #[cxx_name = "chatAnswer"]
        fn chat_answer(self: Pin<&mut Self>, request: &QString, option: &QString);
        #[qinvokable]
        #[cxx_name = "chatCancel"]
        fn chat_cancel(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "chatStop"]
        fn chat_stop(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "chatLastCommand"]
        fn chat_last_command(self: &Self) -> QString;
        #[qinvokable]
        #[cxx_name = "chatPresets"]
        fn chat_presets(self: &Self) -> QString;
        #[qinvokable]
        #[cxx_name = "startMcp"]
        fn start_mcp(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "stopMcp"]
        fn stop_mcp(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "connectRecipes"]
        fn connect_recipes(self: &Self) -> QString;
        #[qinvokable]
        #[cxx_name = "skillStatus"]
        fn skill_status(self: &Self) -> QString;
        #[qinvokable]
        #[cxx_name = "installSkill"]
        fn install_skill(self: Pin<&mut Self>) -> QString;
        #[qinvokable]
        #[cxx_name = "skillOfferPending"]
        fn skill_offer_pending(self: &Self) -> bool;
        #[qinvokable]
        #[cxx_name = "answerSkillOffer"]
        fn answer_skill_offer(self: Pin<&mut Self>, install: bool) -> QString;
    }
}

use core::pin::Pin;
use std::path::Path;
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use od_agent::mcp::ChangeEvent;
use od_core::NodeId;

use crate::state::{EditorState, Snapshot};

#[derive(Default)]
pub struct EditorRust {
    state: EditorState,
    has_project: bool,
    project_name: QString,
    layers: QString,
    frames: QString,
    active_frame: QString,
    frame_size: QString,
    selection: QString,
    selection_bounds: QString,
    inspector: QString,
    annotations: QString,
    checkpoints: QString,
    log: QString,
    changed: QString,
    unrendered: QString,
    overflows: QString,
    memory: QString,
    tokens: QString,
    languages: QString,
    language: QString,
    chat: QString,
    can_undo: bool,
    can_redo: bool,
    mcp_running: bool,
    mcp_url: QString,
    canvas: QString,
    message: QString,
    message_is_error: bool,
    message_seq: i32,
    revision: i32,
}

fn q(s: &str) -> QString {
    QString::from(s)
}

fn id_arg(id: i32) -> u64 {
    u64::try_from(id).unwrap_or(0)
}

/// `file:///Users/me/p.od` to `/Users/me/p.od` (and `file:///C:/x` to `C:/x`), percent-decoded.
fn path_from_url(url: &str) -> String {
    let rest = url.strip_prefix("file://").unwrap_or(url);
    let rest = if rest.len() > 2 && rest.as_bytes()[0] == b'/' && rest.as_bytes()[2] == b':' {
        &rest[1..]
    } else {
        rest
    };
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%')
            .then(|| bytes.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        if let Some(v) = hex {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl qobject::Editor {
    fn show(mut self: Pin<&mut Self>, text: &str, is_error: bool) {
        let seq = *self.message_seq() + 1;
        self.as_mut().set_message(q(text));
        self.as_mut().set_message_is_error(is_error);
        self.as_mut().set_message_seq(seq);
    }

    fn import_image_impl(
        mut self: Pin<&mut Self>,
        url: &QString,
        center: Option<(f64, f64)>,
    ) -> QString {
        let path = path_from_url(&url.to_string());
        q(&self
            .as_mut()
            .act(|s| s.import_image(std::path::Path::new(&path), center))
            .unwrap_or_default())
    }

    /// Runs an action on the state, reports an error to the user, and republishes the state.
    fn act<T>(
        mut self: Pin<&mut Self>,
        f: impl FnOnce(&mut EditorState) -> Result<T, String>,
    ) -> Option<T> {
        let result = {
            let mut rust = self.as_mut().rust_mut();
            f(&mut rust.state)
        };
        let out = match result {
            Ok(v) => Some(v),
            Err(e) => {
                self.as_mut().show(&e, true);
                None
            }
        };
        self.as_mut().refresh();
        out
    }

    fn publish(mut self: Pin<&mut Self>, snap: &Snapshot) {
        self.as_mut().set_has_project(snap.has_project);
        self.as_mut().set_project_name(q(&snap.project_name));
        self.as_mut().set_layers(q(&snap.layers));
        self.as_mut().set_frames(q(&snap.frames));
        self.as_mut().set_active_frame(q(&snap.active_frame));
        self.as_mut().set_frame_size(q(&snap.frame_size));
        self.as_mut().set_selection(q(&snap.selection));
        self.as_mut()
            .set_selection_bounds(q(&snap.selection_bounds));
        self.as_mut().set_inspector(q(&snap.inspector));
        self.as_mut().set_annotations(q(&snap.annotations));
        self.as_mut().set_checkpoints(q(&snap.checkpoints));
        self.as_mut().set_log(q(&snap.log));
        self.as_mut().set_changed(q(&snap.changed));
        self.as_mut().set_unrendered(q(&snap.unrendered));
        self.as_mut().set_overflows(q(&snap.overflows));
        self.as_mut().set_memory(q(&snap.memory));
        self.as_mut().set_tokens(q(&snap.tokens));
        self.as_mut().set_languages(q(&snap.languages));
        self.as_mut().set_language(q(&snap.language));
        self.as_mut().set_chat(q(&snap.chat));
        self.as_mut().set_can_undo(snap.can_undo);
        self.as_mut().set_can_redo(snap.can_redo);
        self.as_mut().set_mcp_running(snap.mcp_running);
        self.as_mut().set_mcp_url(q(&snap.mcp_url));
    }

    fn refresh(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().state.ensure_active_frame();
        let snap = self.state.snapshot();
        self.as_mut().publish(&snap);
        let revision = *self.revision() + 1;
        self.as_mut().set_revision(revision);
    }

    /// Called on the Qt thread when an agent changed the document through MCP.
    fn on_agent_change(mut self: Pin<&mut Self>, who: &str, ids: Vec<NodeId>) {
        self.as_mut().rust_mut().state.note_agent_change(&ids);
        self.as_mut()
            .show(&format!("{who} changed {} layer(s)", ids.len()), false);
        self.as_mut().refresh();
    }

    pub fn new_project(
        mut self: Pin<&mut Self>,
        path: &QString,
        name: &QString,
        width: f64,
        height: f64,
    ) {
        let (path, name) = (path_from_url(&path.to_string()), name.to_string());
        if self
            .as_mut()
            .act(|s| s.new_project(Path::new(&path), &name, width, height))
            .is_some()
        {
            self.as_mut().show("Project created", false);
        }
    }

    pub fn inspect_folder(&self, url: &QString) -> QString {
        let path = path_from_url(&url.to_string());
        q(&EditorState::inspect_folder(Path::new(&path)).to_string())
    }

    pub fn open_workspace(mut self: Pin<&mut Self>, url: &QString, create: bool) -> bool {
        let path = path_from_url(&url.to_string());
        let opened = self
            .as_mut()
            .act(|s| s.open_workspace(Path::new(&path), create))
            .is_some();
        if opened {
            self.as_mut().show("Project ready", false);
        }
        opened
    }

    pub fn recent_projects(&self) -> QString {
        q(&EditorState::recent_projects().to_string())
    }

    pub fn chat_start_with(mut self: Pin<&mut Self>, command: &QString, prompt: &QString) {
        self.as_mut().chat_start(command);
        let prompt = prompt.to_string();
        self.as_mut().rust_mut().state.chat_first_prompt(&prompt);
    }

    pub fn open_project(mut self: Pin<&mut Self>, path: &QString) {
        let path = path_from_url(&path.to_string());
        if self
            .as_mut()
            .act(|s| s.open_project(Path::new(&path)))
            .is_some()
        {
            self.as_mut().show("Project opened", false);
        }
    }

    pub fn save_project(mut self: Pin<&mut Self>) {
        if self.as_mut().act(|s| s.save()).is_some() {
            self.as_mut().show("Saved", false);
        }
    }

    pub fn frame_presets(&self) -> QString {
        q(&EditorState::frame_presets().to_string())
    }

    pub fn local_path(&self, url: &QString) -> QString {
        q(&path_from_url(&url.to_string()))
    }

    pub fn set_active_frame_id(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        self.as_mut().act(|s| s.set_active_frame(&id));
    }

    pub fn select(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        self.as_mut().act(|s| s.select(Some(&id)));
    }

    pub fn hit_test(&self, x: f64, y: f64) -> QString {
        q(&self.state.hit_test(x, y).unwrap_or_default())
    }

    pub fn create_node(
        mut self: Pin<&mut Self>,
        kind: &QString,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> QString {
        let kind = kind.to_string();
        q(&self
            .as_mut()
            .act(|s| s.create_node(&kind, x, y, w, h))
            .unwrap_or_default())
    }

    pub fn import_to_memory(mut self: Pin<&mut Self>, url: &QString) -> QString {
        let path = path_from_url(&url.to_string());
        let added = self
            .as_mut()
            .act(|s| s.import_to_memory(std::path::Path::new(&path)));
        if let Some(reference) = &added {
            self.as_mut()
                .show(&format!("Added {reference} to the agent's memory"), false);
        }
        q(&added.unwrap_or_default())
    }

    pub fn import_image(self: Pin<&mut Self>, url: &QString) -> QString {
        self.import_image_impl(url, None)
    }

    pub fn replace_image(mut self: Pin<&mut Self>, url: &QString, id: &QString) -> QString {
        let (path, id) = (path_from_url(&url.to_string()), id.to_string());
        q(&self
            .as_mut()
            .act(|s| s.replace_image(std::path::Path::new(&path), &id))
            .unwrap_or_default())
    }

    pub fn drop_image(mut self: Pin<&mut Self>, url: &QString, x: f64, y: f64) -> QString {
        let path = path_from_url(&url.to_string());
        q(&self
            .as_mut()
            .act(|s| s.drop_image(std::path::Path::new(&path), x, y))
            .unwrap_or_default())
    }

    pub fn device_kinds(&self) -> QString {
        q(&EditorState::device_kinds().to_string())
    }

    pub fn add_device(mut self: Pin<&mut Self>, kind: &QString) -> QString {
        let kind = kind.to_string();
        q(&self
            .as_mut()
            .act(|s| s.add_device(&kind, None))
            .unwrap_or_default())
    }

    pub fn delete_selected(mut self: Pin<&mut Self>) {
        self.as_mut().act(|s| s.delete_selected());
    }

    pub fn move_node(mut self: Pin<&mut Self>, id: &QString, x: f64, y: f64) {
        let id = id.to_string();
        self.as_mut().act(|s| s.move_node(&id, x, y));
    }

    pub fn resize_node(mut self: Pin<&mut Self>, id: &QString, x: f64, y: f64, w: f64, h: f64) {
        let id = id.to_string();
        self.as_mut().act(|s| s.resize_node(&id, x, y, w, h));
    }

    pub fn set_node_property(
        mut self: Pin<&mut Self>,
        id: &QString,
        name: &QString,
        value_json: &QString,
    ) {
        let (id, name, value) = (id.to_string(), name.to_string(), value_json.to_string());
        self.as_mut().act(|s| s.set_property(&id, &name, &value));
    }

    pub fn toggle_visible(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        self.as_mut().act(|s| s.toggle_visible(&id));
    }

    pub fn switch_language(mut self: Pin<&mut Self>, tag: &QString) {
        let tag = tag.to_string();
        self.as_mut().act(|s| s.switch_language(&tag));
    }

    pub fn add_language(mut self: Pin<&mut Self>, tag: &QString) {
        let tag = tag.to_string();
        self.as_mut().act(|s| s.add_language(&tag));
    }

    pub fn remove_language(mut self: Pin<&mut Self>, tag: &QString) {
        let tag = tag.to_string();
        self.as_mut().act(|s| s.remove_language(&tag));
    }

    pub fn set_token(mut self: Pin<&mut Self>, name: &QString, value: &QString) {
        let (name, value) = (name.to_string(), value.to_string());
        self.as_mut().act(|s| s.set_token(&name, &value));
    }

    pub fn remove_token(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        self.as_mut().act(|s| s.remove_token(&name));
    }

    pub fn reorder(mut self: Pin<&mut Self>, id: &QString, delta: i32) {
        let id = id.to_string();
        self.as_mut().act(|s| s.reorder(&id, delta));
    }

    pub fn undo(mut self: Pin<&mut Self>) {
        self.as_mut().act(|s| s.undo());
    }

    pub fn redo(mut self: Pin<&mut Self>) {
        self.as_mut().act(|s| s.redo());
    }

    pub fn checkpoint(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        if self.as_mut().act(|s| s.checkpoint(&name)).is_some() {
            self.as_mut().show("Checkpoint saved", false);
        }
    }

    pub fn restore(mut self: Pin<&mut Self>, checkpoint: i32) {
        if self
            .as_mut()
            .act(|s| s.restore(id_arg(checkpoint)))
            .is_some()
        {
            self.as_mut().show("Restored", false);
        }
    }

    pub fn add_pin(mut self: Pin<&mut Self>, note: &QString, node: &QString) -> i32 {
        let (note, node) = (note.to_string(), node.to_string());
        self.as_mut()
            .act(|s| s.add_pin(&note, Some(&node)))
            .map_or(0, |id| i32::try_from(id).unwrap_or(0))
    }

    pub fn add_region_pin(
        mut self: Pin<&mut Self>,
        note: &QString,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> i32 {
        let note = note.to_string();
        self.as_mut()
            .act(|s| s.add_region_pin(&note, x, y, w, h))
            .map_or(0, |id| i32::try_from(id).unwrap_or(0))
    }

    pub fn resolve_pin(mut self: Pin<&mut Self>, id: i32) {
        self.as_mut().act(|s| s.resolve_pin(id_arg(id)));
    }

    pub fn reopen_pin(mut self: Pin<&mut Self>, id: i32) {
        self.as_mut().act(|s| s.reopen_pin(id_arg(id)));
    }

    pub fn remove_pin(mut self: Pin<&mut Self>, id: i32) {
        self.as_mut().act(|s| s.remove_pin(id_arg(id)));
    }

    pub fn clear_agent_changes(mut self: Pin<&mut Self>) {
        self.as_mut().act(|s| {
            s.clear_agent_changes();
            Ok(())
        });
    }

    /// Renders the active frame and publishes it as `canvas` (a PNG data URL).
    pub fn render_canvas(mut self: Pin<&mut Self>, scale: f64) {
        match self.state.render_canvas(scale) {
            Ok(url) => self.as_mut().set_canvas(q(&url)),
            Err(e) => {
                self.as_mut().set_canvas(QString::default());
                if self.state.snapshot().has_project
                    && !self.state.snapshot().active_frame.is_empty()
                {
                    self.as_mut().show(&e, true);
                }
            }
        }
    }

    pub fn export_assets(
        mut self: Pin<&mut Self>,
        mode: &QString,
        preset: &QString,
        name: &QString,
        slices: i32,
    ) -> QString {
        let (mode, preset, name) = (mode.to_string(), preset.to_string(), name.to_string());
        let preset = (!preset.is_empty()).then_some(preset);
        let slices = u32::try_from(slices).unwrap_or(0);
        let json = match self.state.export(&mode, preset.as_deref(), &name, slices) {
            Ok(v) => v,
            Err(e) => serde_json::json!({"error": e}),
        };
        if let Some(err) = json.get("error").and_then(|e| e.as_str()) {
            self.as_mut().show(err, true);
        } else if let Some(files) = json.get("files").and_then(|f| f.as_array()) {
            let text = match files.as_slice() {
                [one] => format!("Exported {}", one["path"].as_str().unwrap_or("")),
                many => format!("Exported {} files to the exports folder", many.len()),
            };
            self.as_mut().show(&text, false);
        }
        q(&json.to_string())
    }

    pub fn start_mcp(mut self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        let hook: Arc<dyn Fn(ChangeEvent) + Send + Sync> = Arc::new(move |e: ChangeEvent| {
            let (who, ids) = (e.actor, e.touched);
            let _ = qt.queue(move |editor| editor.on_agent_change(&who, ids));
        });
        let token_file = od_agent::token::token_path();
        let started = match token_file {
            Ok(path) => self.as_mut().act(|s| s.start_mcp(&path, hook)),
            Err(e) => {
                self.as_mut().show(&e.to_string(), true);
                None
            }
        };
        if let Some(url) = started {
            self.as_mut()
                .show(&format!("Agents can connect at {url}"), false);
        }
    }

    /// Starts the chat agent; the agent connection is turned on first if it is off.
    pub fn chat_start(mut self: Pin<&mut Self>, command: &QString) {
        if !*self.mcp_running() {
            self.as_mut().start_mcp();
        }
        let command = command.to_string();
        let qt = self.qt_thread();
        let hook: crate::chat::ChatHook = Arc::new(move |input| {
            let _ = qt.queue(move |editor| editor.on_chat_input(input));
        });
        if self
            .as_mut()
            .act(|s| s.chat_start(&command, hook))
            .is_some()
        {
            EditorState::save_chat_command(&command);
        }
    }

    pub fn chat_send(mut self: Pin<&mut Self>, text: &QString) {
        let text = text.to_string();
        self.as_mut().act(|s| s.chat_send(&text));
    }

    pub fn chat_answer(mut self: Pin<&mut Self>, request: &QString, option: &QString) {
        let (request, option) = (request.to_string(), option.to_string());
        self.as_mut().act(|s| s.chat_answer(&request, &option));
    }

    pub fn chat_cancel(mut self: Pin<&mut Self>) {
        self.as_mut().act(|s| {
            s.chat_cancel();
            Ok(())
        });
    }

    pub fn chat_stop(mut self: Pin<&mut Self>) {
        self.as_mut().act(|s| {
            s.chat_stop();
            Ok(())
        });
    }

    pub fn chat_presets(&self) -> QString {
        q(&EditorState::chat_presets())
    }

    pub fn chat_last_command(&self) -> QString {
        q(&EditorState::chat_last_command())
    }

    /// Called on the Qt thread when the chat connection reports something.
    fn on_chat_input(mut self: Pin<&mut Self>, input: crate::chat::ChatInput) {
        self.as_mut().act(|s| {
            s.chat_apply(input);
            Ok(())
        });
    }

    pub fn stop_mcp(mut self: Pin<&mut Self>) {
        self.as_mut().act(|s| {
            s.stop_mcp();
            Ok(())
        });
        self.as_mut().show("Agent connection stopped", false);
    }

    pub fn connect_recipes(&self) -> QString {
        q(&self.state.connect_recipes())
    }

    pub fn skill_status(&self) -> QString {
        match self.state.skill_status() {
            Ok(v) => q(&v.to_string()),
            Err(e) => q(&serde_json::json!({"error": e}).to_string()),
        }
    }

    pub fn skill_offer_pending(&self) -> bool {
        EditorState::skill_offer_pending()
    }

    pub fn answer_skill_offer(mut self: Pin<&mut Self>, install: bool) -> QString {
        match EditorState::answer_skill_offer(install) {
            Ok(v) => {
                if install {
                    self.as_mut().show("Skill installed", false);
                }
                q(&v.to_string())
            }
            Err(e) => {
                self.as_mut().show(&e, true);
                q(&serde_json::json!({"error": e}).to_string())
            }
        }
    }

    pub fn install_skill(mut self: Pin<&mut Self>) -> QString {
        let result = self.state.install_skill();
        match result {
            Ok(v) => {
                self.as_mut().show("Skill installed", false);
                q(&v.to_string())
            }
            Err(e) => {
                self.as_mut().show(&e, true);
                q(&serde_json::json!({"error": e}).to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::path_from_url;

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(
            path_from_url("file:///Users/me/My%20Project.od"),
            "/Users/me/My Project.od"
        );
        assert_eq!(
            path_from_url("file:///C:/Users/me/p.od"),
            "C:/Users/me/p.od"
        );
        assert_eq!(path_from_url("/already/a/path"), "/already/a/path");
        assert_eq!(path_from_url("file:///a/100%"), "/a/100%");
        assert_eq!(path_from_url("file:///a/%zz"), "/a/%zz");
    }
}

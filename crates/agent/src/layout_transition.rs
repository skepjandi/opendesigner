//! Layout-diff transitions: `snapshot_layout` records where every node of a frame is, the agent
//! edits the document, and `transition_layout` writes the keys that carry each node from where
//! it was to where it is (`docs/wiki/video-editing.md#layout-transitions`). The same record, edit,
//! diff idea as anime.js layout animation, over stable node ids.
//!
//! Snapshots are plain JSON files in `layout-snapshots/` inside the project folder, named with
//! plain names, never through a link, capped in size and number. They hold the document's values,
//! not what a video makes of them.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use od_core::animation::{AnimProp, Repeat};
use od_core::geometry::{Rect, bounds_in_frame};
use od_core::{Document, Ease, Node, NodeId, Operation, Rgba};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::layering::{Tween, splice_replace};
use crate::scene_tool::distribution::StaggerArgs;
use crate::tools::{
    ApplyArgs, CallCtx, Output, ToolError, apply_ops_receipt, node_id, parse, tool,
};

pub const NAMES: [&str; 2] = ["snapshot_layout", "transition_layout"];

pub const SNAPSHOT_DIR: &str = "layout-snapshots";
pub const MAX_SNAPSHOT_NODES: usize = 500;
pub const MAX_SNAPSHOT_BYTES: u64 = 256 * 1024;
pub const MAX_SNAPSHOTS: usize = 24;
const MAX_NAME_LEN: usize = 48;
const MAX_ANIMATED_NODES: usize = 200;
const DEFAULT_DURATION_MS: u64 = 350;
const DEFAULT_STAGGER_MS: f64 = 40.0;
/// Position changes below this are not moves (document units).
const POSITION_TOLERANCE: f64 = 0.01;
/// Size changes up to this many document units create no width or height track.
const SIZE_TOLERANCE: f64 = 1.0;
const OPACITY_TOLERANCE: f64 = 0.001;
const FORMAT_VERSION: u32 = 1;

pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    vec![
        tool(
            "snapshot_layout",
            "Record where every node inside a frame (or group) is right now: its box in the frame, \
             its size, opacity and solid fill, the parent it sits in. Stored as a small JSON file in \
             the project's layout-snapshots folder (at most 500 nodes, 24 snapshots; the oldest is \
             dropped). Then edit the document (move, resize, add, restyle, reparent) and call \
             transition_layout to animate from the recorded state to the new one. Needs a project \
             folder. Returns the snapshot name.",
            json!({
                "frame": id("Id of the frame or group to record."),
                "name": id("A plain name: a-z, 0-9, '.', '_' and '-', up to 48 characters. Default snap-1, snap-2, ...")
            }),
            &["frame"],
            false,
        ),
        tool(
            "transition_layout",
            "Animate from a snapshot_layout record to the document as it is now, matching nodes by id. \
             A node that moved or resized gets x, y, width and height keys from its old values to \
             its live ones (a change under 0.01 for a position or 1 for a size writes nothing); a \
             changed opacity or solid fill_color is carried the same way; a node that is new (or \
             newly visible) fades in, opacity 0 to its own; a node that moved to another parent is \
             started where it was on the page. A node that was deleted cannot be animated: it is \
             reported as leaving (keep a node you want to fade out and set visible false after the \
             move instead of deleting it). The keys from at_ms on that the node's tracks already \
             had on these properties are replaced. One attributed, undoable turn; returns a table \
             of what each node does. The node's value before at_ms is its old one, after the move \
             its live one.",
            json!({
                "video": id("Id of a video from list_videos."),
                "snapshot": id("Name returned by snapshot_layout."),
                "at_ms": {"type": "integer", "minimum": 0, "description": "When the transition starts, from the start of the video."},
                "duration_ms": {"type": "integer", "minimum": 1, "description": "How long each node takes (default 350)."},
                "ease": {"type": "string", "description": format!("One of {} or a tuned one: {}. Default ease_in_out.", Ease::NAMES.join(", "), od_core::ease::GRAMMAR)},
                "stagger": {"type": "object", "description": "Spreads the starts over the nodes that move, and over those that enter, in document order: each_ms (default 40), from, reversed, ease, grid, columns, jitter_ms, seed, start_ms."},
                "turn": id("Groups related calls into one turn."),
                "summary": id("One line, in the user's terms, naming what this turn does.")
            }),
            &["video", "snapshot", "at_ms"],
            false,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "snapshot_layout" => snapshot(ctx, parse(args)?),
        "transition_layout" => transition(ctx, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    name: String,
    parent: String,
    visible: bool,
    /// x, y, width, height as the node stores them, in its parent.
    local: [f64; 4],
    /// The node's box in the frame's coordinates, its turn and scale included.
    world: [f64; 4],
    opacity: f64,
    /// The solid fill as `0xRRGGBBAA`, when it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fill: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    frame: String,
    nodes: Vec<Entry>,
}

/// A node of the live document inside the frame.
struct Live<'a> {
    node: &'a Node,
    parent: NodeId,
    world: Rect,
}

fn rect_array(r: Rect) -> [f64; 4] {
    [r.x, r.y, r.width, r.height]
}

fn live_nodes<'a>(doc: &'a Document, frame: &NodeId) -> Result<Vec<Live<'a>>, ToolError> {
    fn walk<'a>(doc: &'a Document, frame: &NodeId, parent: &'a Node, out: &mut Vec<Live<'a>>) {
        for child in &parent.children {
            if let Some(world) = bounds_in_frame(doc, frame, &child.id) {
                out.push(Live {
                    node: child,
                    parent: parent.id.clone(),
                    world,
                });
            }
            walk(doc, frame, child, out);
        }
    }
    let root = doc
        .node(frame)
        .ok_or_else(|| ToolError(format!("node {frame} does not exist")))?;
    let mut out = Vec::new();
    walk(doc, frame, root, &mut out);
    Ok(out)
}

fn capture(doc: &Document, frame: &NodeId) -> Result<Snapshot, ToolError> {
    let live = live_nodes(doc, frame)?;
    if live.is_empty() {
        return Err(ToolError(format!(
            "{frame} has nothing inside it to record"
        )));
    }
    if live.len() > MAX_SNAPSHOT_NODES {
        return Err(ToolError(format!(
            "{frame} holds {} nodes; a snapshot records at most {MAX_SNAPSHOT_NODES}: record a smaller group",
            live.len()
        )));
    }
    let nodes = live
        .iter()
        .map(|l| Entry {
            id: l.node.id.to_string(),
            name: l.node.name.clone(),
            parent: l.parent.to_string(),
            visible: l.node.visible,
            local: [l.node.x, l.node.y, l.node.width, l.node.height],
            world: rect_array(l.world),
            opacity: l.node.opacity,
            fill: AnimProp::FillColor.read(l.node),
        })
        .collect();
    Ok(Snapshot {
        version: FORMAT_VERSION,
        frame: frame.to_string(),
        nodes,
    })
}

fn valid_name(name: &str) -> Result<(), ToolError> {
    let plain = !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && !name.starts_with('.')
        && !name.contains("..")
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        });
    if plain {
        Ok(())
    } else {
        Err(ToolError(format!(
            "{name:?} is not a plain snapshot name: a-z, 0-9, '.', '_' and '-', 1 to {MAX_NAME_LEN} characters, no leading dot"
        )))
    }
}

fn io_error(what: &str, e: std::io::Error) -> ToolError {
    ToolError(format!("{what}: {e}"))
}

fn refuse_link(path: &Path) -> Result<(), ToolError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Err(ToolError(format!(
            "{} is a link; snapshots are never read or written through one",
            path.display()
        ))),
        Ok(_) | Err(_) => Ok(()),
    }
}

fn project_of(ctx: &CallCtx) -> Result<PathBuf, ToolError> {
    ctx.ws.dir().map(Path::to_path_buf).ok_or_else(|| {
        ToolError(
            "layout snapshots are kept in the project folder; open or create a project first"
                .into(),
        )
    })
}

fn snapshot_files(dir: &Path) -> Vec<(String, std::time::SystemTime)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".json")?.to_string();
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((stem, modified))
        })
        .collect()
}

fn next_default_name(existing: &[(String, std::time::SystemTime)]) -> String {
    let highest = existing
        .iter()
        .filter_map(|(n, _)| n.strip_prefix("snap-")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("snap-{}", highest + 1)
}

fn store(
    project: &Path,
    name: Option<&str>,
    snapshot: &Snapshot,
) -> Result<(String, u64), ToolError> {
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|e| ToolError(e.to_string()))?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(ToolError(format!(
            "the snapshot would be {} bytes; the most is {MAX_SNAPSHOT_BYTES}: record a smaller group",
            bytes.len()
        )));
    }
    let dir = project.join(SNAPSHOT_DIR);
    refuse_link(&dir)?;
    fs::create_dir_all(&dir).map_err(|e| io_error("cannot create the snapshot folder", e))?;
    let existing = snapshot_files(&dir);
    let name = match name {
        Some(n) => {
            valid_name(n)?;
            n.to_string()
        }
        None => next_default_name(&existing),
    };
    let target = dir.join(format!("{name}.json"));
    refuse_link(&target)?;
    let temp = dir.join(format!(".{name}.json.tmp"));
    fs::write(&temp, &bytes).map_err(|e| io_error("cannot write the snapshot", e))?;
    fs::rename(&temp, &target).map_err(|e| io_error("cannot write the snapshot", e))?;
    evict_oldest(&dir, &name);
    Ok((name, bytes.len() as u64))
}

/// Drops the oldest snapshots past the cap, never the one just written.
fn evict_oldest(dir: &Path, keep: &str) {
    let mut files = snapshot_files(dir);
    if files.len() <= MAX_SNAPSHOTS {
        return;
    }
    files.sort_by_key(|(_, modified)| *modified);
    let excess = files.len() - MAX_SNAPSHOTS;
    for (name, _) in files.into_iter().filter(|(n, _)| n != keep).take(excess) {
        let _ = fs::remove_file(dir.join(format!("{name}.json")));
    }
}

fn load(project: &Path, name: &str) -> Result<Snapshot, ToolError> {
    valid_name(name)?;
    let path = project.join(SNAPSHOT_DIR).join(format!("{name}.json"));
    refuse_link(&path)?;
    let meta = fs::metadata(&path).map_err(|_| {
        ToolError(format!(
            "there is no snapshot {name:?}; snapshot_layout records one"
        ))
    })?;
    if meta.len() > MAX_SNAPSHOT_BYTES {
        return Err(ToolError(format!(
            "snapshot {name:?} is larger than {MAX_SNAPSHOT_BYTES} bytes"
        )));
    }
    let text = fs::read_to_string(&path).map_err(|e| io_error("cannot read the snapshot", e))?;
    let snapshot: Snapshot = serde_json::from_str(&text)
        .map_err(|e| ToolError(format!("snapshot {name:?} is not a layout snapshot: {e}")))?;
    if snapshot.version != FORMAT_VERSION || snapshot.nodes.len() > MAX_SNAPSHOT_NODES {
        return Err(ToolError(format!(
            "snapshot {name:?} has an unsupported format"
        )));
    }
    Ok(snapshot)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotArgs {
    frame: String,
    #[serde(default)]
    name: Option<String>,
}

fn snapshot(ctx: &mut CallCtx, a: SnapshotArgs) -> Result<Output, ToolError> {
    let frame = node_id(&a.frame)?;
    let taken = capture(ctx.ws.session.document(), &frame)?;
    let project = project_of(ctx)?;
    let (name, bytes) = store(&project, a.name.as_deref(), &taken)?;
    Ok(Output::json(&json!({
        "snapshot": name,
        "frame": frame,
        "nodes": taken.nodes.len(),
        "bytes": bytes,
        "next": "Edit the document, then transition_layout with this snapshot name.",
    })))
}

/// One property of one node, carried from `from` to `to`.
#[derive(Debug, Clone, PartialEq)]
struct Change {
    prop: AnimProp,
    from: f64,
    to: f64,
}

#[derive(Debug)]
struct Mover {
    id: NodeId,
    name: String,
    changes: Vec<Change>,
}

#[derive(Debug)]
struct Leaving {
    id: String,
    name: String,
    reason: &'static str,
}

#[derive(Debug, Default)]
struct Diff {
    moved: Vec<Mover>,
    entering: Vec<Mover>,
    leaving: Vec<Leaving>,
    unchanged: usize,
}

fn differs(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() > tolerance
}

/// Where the node was in its live parent's coordinates: as it stored it when the parent is the
/// same, else its old place on the frame less the live parent's place on the frame.
fn old_origin(entry: &Entry, live: &Live, by_id: &HashMap<String, &Live>) -> (f64, f64) {
    if entry.parent == live.parent.as_str() {
        return (entry.local[0], entry.local[1]);
    }
    let (px, py) = by_id
        .get(live.parent.as_str())
        .map_or((0.0, 0.0), |p| (p.world.x, p.world.y));
    (entry.world[0] - px, entry.world[1] - py)
}

fn changes_of(entry: &Entry, live: &Live, by_id: &HashMap<String, &Live>) -> Vec<Change> {
    let (x0, y0) = old_origin(entry, live, by_id);
    let n = live.node;
    let sized = !(n.kind == od_core::NodeKind::Group && n.width <= 0.0 && n.height <= 0.0);
    let mut out = Vec::new();
    let mut push = |prop, from, to, tolerance| {
        if differs(from, to, tolerance) {
            out.push(Change { prop, from, to });
        }
    };
    push(AnimProp::X, x0, n.x, POSITION_TOLERANCE);
    push(AnimProp::Y, y0, n.y, POSITION_TOLERANCE);
    if sized {
        push(AnimProp::Width, entry.local[2], n.width, SIZE_TOLERANCE);
        push(AnimProp::Height, entry.local[3], n.height, SIZE_TOLERANCE);
    }
    push(
        AnimProp::Opacity,
        entry.opacity,
        n.opacity,
        OPACITY_TOLERANCE,
    );
    if let (Some(from), Some(to)) = (entry.fill, AnimProp::FillColor.read(n)) {
        if Rgba::unpack(from) != Rgba::unpack(to) {
            out.push(Change {
                prop: AnimProp::FillColor,
                from,
                to,
            });
        }
    }
    out
}

fn diff(old: &Snapshot, doc: &Document, frame: &NodeId) -> Result<Diff, ToolError> {
    let live = live_nodes(doc, frame)?;
    let by_id: HashMap<String, &Live> = live.iter().map(|l| (l.node.id.to_string(), l)).collect();
    let before: BTreeMap<&str, &Entry> = old.nodes.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut out = Diff::default();
    for l in &live {
        let mover = |changes| Mover {
            id: l.node.id.clone(),
            name: l.node.name.clone(),
            changes,
        };
        match before.get(l.node.id.as_str()) {
            Some(e) if e.visible && l.node.visible => {
                let changes = changes_of(e, l, &by_id);
                if changes.is_empty() {
                    out.unchanged += 1;
                } else {
                    out.moved.push(mover(changes));
                }
            }
            Some(e) if e.visible => out.leaving.push(Leaving {
                id: l.node.id.to_string(),
                name: l.node.name.clone(),
                reason: "hidden (visible is false now)",
            }),
            _ if l.node.visible && l.node.opacity > OPACITY_TOLERANCE => {
                out.entering.push(mover(vec![Change {
                    prop: AnimProp::Opacity,
                    from: 0.0,
                    to: l.node.opacity,
                }]));
            }
            _ => {}
        }
    }
    for e in old.nodes.iter().filter(|e| e.visible) {
        if !by_id.contains_key(&e.id) {
            out.leaving.push(Leaving {
                id: e.id.clone(),
                name: e.name.clone(),
                reason: "deleted or moved out of the frame",
            });
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionArgs {
    video: String,
    snapshot: String,
    at_ms: u64,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    ease: Option<Ease>,
    #[serde(default)]
    stagger: Option<StaggerArgs>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// The start delay of each node of `group`, in order: none without a stagger.
fn delays(
    doc: &Document,
    group: &[Mover],
    stagger: Option<&StaggerArgs>,
) -> Result<Vec<u64>, ToolError> {
    let Some(spec) = stagger else {
        return Ok(vec![0; group.len()]);
    };
    if group.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = group.iter().map(|m| m.id.to_string()).collect();
    Ok(spec
        .distribute(doc, &ids, 0, DEFAULT_STAGGER_MS)?
        .offsets_ms)
}

fn transition(ctx: &mut CallCtx, a: TransitionArgs) -> Result<Output, ToolError> {
    let video_id = node_id(&a.video)?;
    let project = project_of(ctx)?;
    let old = load(&project, &a.snapshot)?;
    let frame = node_id(&old.frame)?;
    let doc = ctx.ws.session.document();
    let video = doc
        .video(&video_id)
        .ok_or_else(|| ToolError(format!("video {video_id} does not exist")))?;
    let found = diff(&old, doc, &frame)?;
    let animated = found.moved.len() + found.entering.len();
    if animated == 0 {
        return Err(ToolError(nothing_to_animate(&found)));
    }
    if animated > MAX_ANIMATED_NODES {
        return Err(ToolError(format!(
            "{animated} nodes would animate; at most {MAX_ANIMATED_NODES}: transition a smaller group"
        )));
    }
    let duration = a.duration_ms.unwrap_or(DEFAULT_DURATION_MS);
    let ease = a.ease.unwrap_or_default();
    let mut ops = Vec::new();
    for (group, offsets) in [
        (&found.moved, delays(doc, &found.moved, a.stagger.as_ref())?),
        (
            &found.entering,
            delays(doc, &found.entering, a.stagger.as_ref())?,
        ),
    ] {
        for (mover, offset) in group.iter().zip(offsets) {
            for change in &mover.changes {
                let start = a.at_ms.saturating_add(offset);
                let tween = Tween {
                    at_ms: start,
                    end_ms: start.saturating_add(duration),
                    from: Some(change.from),
                    to: change.to,
                    ease,
                    keep_later: false,
                };
                let spliced = splice_replace(
                    video.base_track(&mover.id, change.prop),
                    change.to,
                    video.fps,
                    &tween,
                )?;
                ops.push(Operation::Animate {
                    video: video_id.clone(),
                    node: mover.id.clone(),
                    prop: change.prop,
                    keys: spliced.keys,
                    repeat: Repeat::Once,
                    additive: false,
                });
            }
        }
    }
    let table = table_of(&found, a.at_ms, duration);
    let mut receipt = apply_ops_receipt(
        ctx,
        ApplyArgs {
            ops,
            turn: a.turn,
            summary: a
                .summary
                .or_else(|| Some(format!("layout transition from {}", a.snapshot))),
        },
    )?;
    if let Some(body) = receipt.as_object_mut() {
        body.insert("table".into(), json!(table));
        body.insert("moved".into(), rows(&found.moved));
        body.insert("entering".into(), rows(&found.entering));
        body.insert("leaving".into(), leaving_rows(&found.leaving));
        body.insert("unchanged".into(), json!(found.unchanged));
        if !found.leaving.is_empty() {
            body.insert("leaving_note".into(), json!(LEAVING_NOTE));
        }
    }
    Ok(Output::json(&receipt))
}

const LEAVING_NOTE: &str = "A deleted node cannot be animated. To fade a node out, keep it, \
    animate its opacity to 0, and set visible false once the move is done.";

fn nothing_to_animate(found: &Diff) -> String {
    let mut text = format!(
        "the document matches the snapshot ({} nodes unchanged): nothing to animate",
        found.unchanged
    );
    if !found.leaving.is_empty() {
        let names: Vec<&str> = found.leaving.iter().map(|l| l.id.as_str()).collect();
        text.push_str(&format!(
            "; {} nodes are gone ({}) and cannot be animated: {LEAVING_NOTE}",
            names.len(),
            names.join(", ")
        ));
    }
    text
}

fn number(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn describe(c: &Change) -> String {
    if c.prop == AnimProp::FillColor {
        return format!(
            "fill_color {}->{}",
            Rgba::unpack(c.from).to_hex(),
            Rgba::unpack(c.to).to_hex()
        );
    }
    format!("{} {}->{}", c.prop.as_str(), number(c.from), number(c.to))
}

fn table_of(found: &Diff, at_ms: u64, duration_ms: u64) -> String {
    let mut lines = vec![
        format!("transition at {at_ms} ms, {duration_ms} ms each"),
        "| node | name | what | change |".to_string(),
        "|---|---|---|---|".to_string(),
    ];
    for (what, group) in [("moves", &found.moved), ("enters", &found.entering)] {
        for m in group {
            let changes: Vec<String> = m.changes.iter().map(describe).collect();
            lines.push(format!(
                "| {} | {} | {what} | {} |",
                m.id,
                m.name,
                changes.join(", ")
            ));
        }
    }
    for l in &found.leaving {
        lines.push(format!(
            "| {} | {} | leaves | {} (cannot be animated) |",
            l.id, l.name, l.reason
        ));
    }
    lines.join("\n")
}

fn rows(group: &[Mover]) -> Value {
    Value::Array(
        group
            .iter()
            .map(|m| {
                json!({
                    "id": m.id,
                    "name": m.name,
                    "changes": m.changes.iter().map(|c| json!({
                        "prop": c.prop.as_str(),
                        "from": c.from,
                        "to": c.to,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

fn leaving_rows(group: &[Leaving]) -> Value {
    Value::Array(
        group
            .iter()
            .map(|l| json!({"id": l.id, "name": l.name, "reason": l.reason}))
            .collect(),
    )
}

#[cfg(test)]
mod tests;

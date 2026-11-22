//! From a request to operations. The block draws into a [`Kit`]; the plan creates its group on a
//! scratch copy of the document so the nodes have ids, then turns the block's moves into
//! `animate` operations (named moves through `od_core::animation::preset_ops`). Planning has no
//! side effects: the pictures a block needs are written afterwards by [`write_assets`].

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use od_core::animation::{Key, PresetRequest};
use od_core::geometry::Rect;
use od_core::limits;
use od_core::video::MAX_VIDEO_MS;
use od_core::{Document, Node, NodeId, NodeKind, Operation, Paint, ops};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::BlockError;
use super::args::Args;
use super::files::Asset;
use super::kit::{Kit, Move, Role, Target, at};
use super::registry::{Block, Layer, find};

const EXIT_MS: u64 = 400;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoxArgs {
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub foreground: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub muted: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Request {
    pub block: String,
    pub frame: Option<String>,
    pub video: Option<String>,
    pub start_ms: Option<u64>,
    pub hold_ms: Option<u64>,
    pub region: Option<BoxArgs>,
    pub colors: Option<Colors>,
    pub font: Option<String>,
    pub params: Option<Map<String, Value>>,
}

/// What adding a block does: the operations, and what the caller needs to report.
#[derive(Debug, Clone)]
pub struct Plan {
    pub block: &'static str,
    /// The group's name, `block:<id>#<n>`.
    pub name: String,
    pub frame: NodeId,
    pub video: Option<NodeId>,
    pub ops: Vec<Operation>,
    pub assets: Vec<Asset>,
    /// The root group, then its direct parts, by name.
    pub parts: Vec<(String, NodeId)>,
    pub node_count: usize,
    pub animated: bool,
    /// Moves were asked for but there is no video to keyframe in.
    pub static_only: bool,
}

impl Plan {
    pub fn report(&self) -> Value {
        let parts: Map<String, Value> = self
            .parts
            .iter()
            .skip(1)
            .map(|(n, id)| (n.clone(), json!(id)))
            .collect();
        let root = self.parts.first().map(|(_, id)| id.to_string());
        let what = if self.animated {
            "with its animation keyed in the video"
        } else if self.static_only {
            "as a static layout (no video to animate in)"
        } else {
            "as a static layout"
        };
        let mut v = json!({
            "block": self.block,
            "name": self.name,
            "root": root,
            "parts": parts,
            "created_count": self.node_count,
            "frame": self.frame,
            "video": self.video,
            "animated": self.animated,
            "description": match &root {
                Some(id) => format!(
                    "Added {} ({id}) to frame {} with {} nodes {what}; edit its parts by name with apply_ops, or delete the group to remove it.",
                    self.name, self.frame, self.node_count
                ),
                None => format!(
                    "Keyframed existing nodes in video {} with {}; undo to remove the keys.",
                    self.video.as_ref().map_or("?".to_string(), ToString::to_string),
                    self.block
                ),
            },
            "next": "Replace the placeholder copy and colors with the project's own, then render_preview and review_frame.",
        });
        let credits: Vec<String> = self.assets.iter().filter_map(Asset::credit).collect();
        if let Some(first) = credits.first() {
            v["attribution_required"] = json!(true);
            v["attribution"] = json!(first);
        }
        v
    }
}

fn walk_frames(node: &Node, out: &mut Vec<NodeId>) {
    if node.kind == NodeKind::Frame {
        out.push(node.id.clone());
    }
    for c in &node.children {
        walk_frames(c, out);
    }
}

fn pick_frame(doc: &Document, given: Option<&str>) -> Result<NodeId, BlockError> {
    if let Some(id) = given {
        let id = NodeId::parse(id)?;
        let node = doc
            .node(&id)
            .ok_or_else(|| BlockError::Target(format!("frame {id} does not exist")))?;
        if node.kind != NodeKind::Frame {
            return Err(BlockError::Target(format!(
                "{id} is a {}, not a frame",
                node.kind.as_str()
            )));
        }
        return Ok(id);
    }
    let mut frames = Vec::new();
    for page in &doc.pages {
        walk_frames(&page.root, &mut frames);
    }
    match frames.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(BlockError::Target(
            "the document has no frame; create one with apply_ops first".into(),
        )),
        many => Err(BlockError::Target(format!(
            "the document has {} frames; give `frame` (one of {})",
            many.len(),
            many.iter()
                .take(6)
                .map(NodeId::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn pick_video(doc: &Document, given: Option<&str>) -> Result<Option<NodeId>, BlockError> {
    if let Some(id) = given {
        let id = NodeId::parse(id)?;
        doc.video(&id)
            .ok_or_else(|| BlockError::Target(format!("video {id} does not exist")))?;
        return Ok(Some(id));
    }
    Ok(match doc.videos.as_slice() {
        [only] => Some(only.id.clone()),
        _ => None,
    })
}

fn highest_number(node: &Node, prefix: &str, best: &mut u32) {
    if let Some(rest) = node.name.strip_prefix(prefix) {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(n) = digits.parse::<u32>() {
            *best = (*best).max(n);
        }
    }
    for c in &node.children {
        highest_number(c, prefix, best);
    }
}

fn unique_name(doc: &Document, block: &str) -> String {
    let prefix = format!("block:{block}#");
    let mut best = 0;
    for page in &doc.pages {
        highest_number(&page.root, &prefix, &mut best);
    }
    format!("{prefix}{}", best + 1)
}

fn palette(doc: &Document, colors: Option<&Colors>) -> Result<BTreeMap<Role, Paint>, BlockError> {
    let mut out = BTreeMap::new();
    let Some(c) = colors else {
        return Ok(out);
    };
    let given = [
        (Role::Background, &c.background),
        (Role::Foreground, &c.foreground),
        (Role::Accent, &c.accent),
        (Role::Muted, &c.muted),
    ];
    for (role, value) in given {
        if let Some(v) = value {
            let paint = Paint::parse(v)?;
            paint.resolve(&doc.tokens)?;
            out.insert(role, paint);
        }
    }
    Ok(out)
}

fn box_of(
    block: &Block,
    frame: Rect,
    margin: f64,
    over: Option<&BoxArgs>,
) -> Result<Rect, BlockError> {
    let base = block.place.resolve(frame, margin);
    let Some(o) = over else {
        return Ok(base);
    };
    let b = at(
        o.x.unwrap_or(base.x),
        o.y.unwrap_or(base.y),
        o.width.unwrap_or(base.width),
        o.height.unwrap_or(base.height),
    );
    let finite = [b.x, b.y, b.width, b.height].iter().all(|v| v.is_finite());
    if !finite || b.width < 16.0 || b.height < 16.0 {
        return Err(BlockError::Param(
            "box needs a finite x, y and a width and height of at least 16".into(),
        ));
    }
    if b.x < 0.0
        || b.y < 0.0
        || b.x + b.width > frame.width + 0.5
        || b.y + b.height > frame.height + 0.5
    {
        return Err(BlockError::Param(format!(
            "box {:.0},{:.0} {:.0}x{:.0} leaves the {:.0}x{:.0} frame",
            b.x, b.y, b.width, b.height, frame.width, frame.height
        )));
    }
    Ok(b)
}

/// The groups below `node` (a rig's head, eyes, pupils), by name, so they can be animated further.
fn nested_groups(node: &Node, out: &mut Vec<(String, NodeId)>) {
    for c in &node.children {
        if c.kind == NodeKind::Group {
            let short = c.name.rsplit('/').next().unwrap_or(&c.name);
            out.push((short.to_string(), c.id.clone()));
        }
        nested_groups(c, out);
    }
}

fn index_parts(node: &Node, out: &mut HashMap<String, NodeId>) {
    out.insert(node.name.clone(), node.id.clone());
    for c in &node.children {
        index_parts(c, out);
    }
}

fn target_id(
    t: &Target,
    root: Option<&NodeId>,
    names: &HashMap<String, NodeId>,
) -> Result<NodeId, BlockError> {
    match t {
        Target::Root => root.cloned().ok_or_else(|| {
            BlockError::Param(
                "this block makes no nodes of its own, so there is no group to fade out".into(),
            )
        }),
        Target::Node(id) => Ok(id.clone()),
        Target::Part(name) => names.get(name).cloned().ok_or_else(|| {
            BlockError::Internal(format!("a move names {name:?}, which is not a part"))
        }),
    }
}

fn move_ops(
    scratch: &Document,
    video: &NodeId,
    mv: &Move,
    start: u64,
    root: Option<&NodeId>,
    names: &HashMap<String, NodeId>,
) -> Result<Vec<Operation>, BlockError> {
    match mv {
        Move::Preset {
            target,
            preset,
            at,
            duration,
            strength,
            ease,
        } => {
            let node = target_id(target, root, names)?;
            let request = PresetRequest {
                preset: *preset,
                start_ms: start + at,
                duration_ms: *duration,
                strength: *strength,
                ease: *ease,
            };
            Ok(od_core::animation::preset_ops(
                scratch, video, &node, request,
            )?)
        }
        Move::Keys {
            target,
            prop,
            keys,
            repeat,
        } => {
            let node = target_id(target, root, names)?;
            Ok(vec![Operation::Animate {
                additive: false,
                video: video.clone(),
                node,
                prop: *prop,
                keys: keys
                    .iter()
                    .map(|&(t, value, ease)| Key {
                        at_ms: start + t,
                        value,
                        ease,
                    })
                    .collect(),
                repeat: *repeat,
            }])
        }
    }
}

/// The scenery rank of a block's group (`block:<id>#<n>`), when it is a scenery block.
fn scenery_rank(node: &Node) -> Option<u8> {
    let id = node.name.strip_prefix("block:")?.split('#').next()?;
    match find(id).ok()?.layer {
        Layer::Scenery(rank) => Some(rank),
        _ => None,
    }
}

/// Where in the frame's children a block's group goes.
fn insert_index(doc: &Document, frame: &NodeId, layer: Layer) -> Option<usize> {
    match layer {
        Layer::Front => None,
        Layer::Back => Some(0),
        Layer::Scenery(rank) => {
            let kids = doc.node(frame).map_or(&[][..], |f| &f.children[..]);
            Some(
                kids.iter()
                    .take_while(|c| scenery_rank(c).is_some_and(|r| r <= rank))
                    .count(),
            )
        }
    }
}

/// Everything a request needs, checked, as one list of operations that apply together.
pub fn plan(doc: &Document, req: &Request) -> Result<Plan, BlockError> {
    let block = find(&req.block)?;
    let frame_id = pick_frame(doc, req.frame.as_deref())?;
    let frame_node = doc
        .node(&frame_id)
        .ok_or_else(|| BlockError::Target(format!("frame {frame_id} does not exist")))?;
    let frame = at(0.0, 0.0, frame_node.width, frame_node.height);
    if frame.width < 64.0 || frame.height < 64.0 {
        return Err(BlockError::Target(
            "the frame is smaller than 64 x 64: make it larger first".into(),
        ));
    }
    let video = pick_video(doc, req.video.as_deref())?;
    let font = req
        .font
        .as_deref()
        .map(|f| limits::family_name(f).map(|()| f.to_string()))
        .transpose()?;
    let args = Args::resolve(
        block.id,
        block.params,
        req.params.clone().unwrap_or_default(),
        doc,
        &frame_id,
    )?;
    let name = unique_name(doc, block.id);
    let margin = (frame.width.min(frame.height) * 0.06).round().max(8.0);
    let b = box_of(block, frame, margin, req.region.as_ref())?;
    let mut kit = Kit::new(
        doc,
        frame,
        b,
        name.clone(),
        font,
        palette(doc, req.colors.as_ref())?,
    );
    (block.build)(&mut kit, &args)?;
    args.check()?;
    let built = kit.finish();

    let start = req.start_ms.unwrap_or(0);
    let mut moves = built.moves;
    let entrance = moves.iter().map(Move::end).max().unwrap_or(0);
    if let Some(hold) = req.hold_ms {
        if video.is_none() {
            return Err(BlockError::Param(
                "hold_ms needs a video to fade out in".into(),
            ));
        }
        if hold < entrance {
            return Err(BlockError::Param(format!(
                "hold_ms {hold} is shorter than the block's entrance ({entrance} ms)"
            )));
        }
        moves.push(Move::Preset {
            target: Target::Root,
            preset: od_core::Preset::FadeOut,
            at: hold,
            duration: EXIT_MS,
            strength: 1.0,
            ease: None,
        });
    }
    let end = start + moves.iter().map(Move::end).max().unwrap_or(0);
    if end > MAX_VIDEO_MS {
        return Err(BlockError::Param(format!(
            "the block would end at {end} ms, after the longest video ({MAX_VIDEO_MS} ms)"
        )));
    }

    built.assets.iter().try_for_each(Asset::check)?;
    let mut ops: Vec<Operation> = built
        .tokens
        .iter()
        .map(|(n, v)| Operation::SetToken {
            name: n.clone(),
            value: Some(v.clone()),
        })
        .collect();
    let makes_nodes = !built.root.children.is_empty();
    if makes_nodes {
        ops.push(Operation::Create {
            parent: frame_id.clone(),
            index: insert_index(doc, &frame_id, block.layer),
            node: built.root,
        });
    }
    let mut scratch = doc.clone();
    let mut root = None;
    for op in &ops {
        let applied = ops::apply(&mut scratch, op, false)?;
        if matches!(op, Operation::Create { .. }) {
            root = applied.touched.first().cloned();
        }
    }
    let mut names = HashMap::new();
    let mut parts = Vec::new();
    let mut node_count = 0;
    if let Some(id) = &root {
        let made = scratch
            .node(id)
            .ok_or_else(|| BlockError::Internal("the block's group vanished".into()))?;
        index_parts(made, &mut names);
        node_count = made.count();
        parts.push((name.clone(), id.clone()));
        parts.extend(made.children.iter().map(|c| {
            let short = c.name.rsplit('/').next().unwrap_or(&c.name);
            (short.to_string(), c.id.clone())
        }));
        for c in &made.children {
            nested_groups(c, &mut parts);
        }
    }

    let animate = video.is_some() && !moves.is_empty();
    if let (true, Some(v)) = (animate, &video) {
        for mv in &moves {
            for op in move_ops(&scratch, v, mv, start, root.as_ref(), &names)? {
                ops::apply(&mut scratch, &op, false)?;
                ops.push(op);
            }
        }
    }
    if !makes_nodes && !animate {
        return Err(BlockError::Param(format!(
            "{} only animates nodes that exist, and there is no video: give `video`",
            block.id
        )));
    }
    Ok(Plan {
        block: block.id,
        name,
        frame: frame_id,
        video,
        ops,
        assets: built.assets,
        parts,
        node_count,
        animated: animate,
        static_only: !animate && !moves.is_empty(),
    })
}

/// Puts the plan's pictures in the project's `media/` folder.
pub fn write_assets(dir: &Path, plan: &Plan) -> Result<(), BlockError> {
    plan.assets.iter().try_for_each(|a| a.write(dir))
}

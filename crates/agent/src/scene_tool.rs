//! The scene moves of `animate_preset`: named moves for any node and for the camera, staggers over
//! a list of nodes, and a node riding a path (`docs/wiki/video-editing.md#scene-motion`). The
//! motion itself is `crate::scene_motion`; this module reads the tool's arguments, measures the
//! nodes where the move starts and turns the tracks into `animate` operations, merged with the keys
//! a node already has so moves chain.

use od_core::animation::{AnimProp, Key, MAX_STRENGTH, MIN_STRENGTH, Repeat};
use od_core::timebase::snap_within;
use od_core::video::{MAX_VIDEO_MS, Video};
use od_core::{Document, Ease, Node, NodeId, NodeKind, Operation, path, vector};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::scene_motion::{
    CameraPose, Named, Origin, Pose, Side, Spec, Track, camera_tracks, follow, node_tracks, orbit,
    resample, stagger_delays,
};
use crate::tools::{ApplyArgs, CallCtx, Output, ToolError, apply_ops, node_id};

const PATH_MOVE: &str = "animate_along_path";
const STAGGER: &str = "stagger";
const MAX_STAGGERED: usize = 60;
const MIN_MS: u64 = 100;
const MAX_MS: u64 = 600_000;

/// Whether `preset` is one of the scene moves.
pub fn handles(preset: &str) -> bool {
    Named::parse(preset).is_some() || preset == PATH_MOVE || preset == STAGGER
}

/// Whether a request for `preset` on `node` is a scene move. A name the 3D devices already had
/// (`spin`, `swing`, `float`, `orbit`) stays the device's own move on a 3D device.
pub fn takes(doc: &Document, preset: &str, node: Option<&str>) -> bool {
    if !handles(preset) {
        return false;
    }
    let shared = od_core::Preset::ALL.iter().any(|p| p.as_str() == preset);
    let on_device = node
        .and_then(|id| NodeId::parse(id).ok())
        .and_then(|id| doc.node(&id))
        .is_some_and(|n| n.kind == NodeKind::Device3d);
    !(shared && on_device)
}

/// [`operations`] for the arguments of the `animate_preset` tool, as JSON.
pub fn operations_from_json(doc: &Document, args: Value) -> Result<Vec<Operation>, ToolError> {
    operations(doc, &crate::tools::parse(args)?)
}

pub fn names() -> Vec<&'static str> {
    let mut all = Named::NAMES.to_vec();
    all.extend([STAGGER, PATH_MOVE]);
    all
}

pub fn description() -> &'static str {
    "Scene moves work on any node, not only 3D devices, and write ordinary keyframes. Ambient (loop \
     by default, end where they start, `seed` shifts the phase so nodes do not move in lockstep; \
     duration_ms is one cycle): float (lazy drift), bob (up and down with a little tilt), breathe \
     (squash and stretch), pulse (a beat of scale), spin (intensity = turns), wobble, swing (a \
     pendulum tilt), orbit (the node's middle circles `center_x`/`center_y`, or a point to its left; \
     `radius`). Entrances and impacts (play once): pop_in (scale 0 to 1 with a small overshoot), \
     bounce_in (drops in and bounces), elastic_in (rings in), slide_in_left|right|up|down (with \
     overshoot), fade_up, zoom_in, shake (decays), draw_on (a stroked node's trim_end 0 to 1). \
     Camera moves write the video's camera (the node is the video; layers with a `depth` follow it \
     by that much): camera_push_in, camera_pull_back, camera_pan (`direction` left|right|up|down, \
     `amount` in document units), camera_drift (slow loop), camera_shake (decays), camera_whip \
     (fast pan, `direction`). animate_along_path: the `path` node's curve is sampled into x/y keys (and, \
     with `orient`, rotation) so `node` follows it over the time range with `ease`. stagger: apply \
     the move named in `of` to every id in `nodes`, `step_ms` apart, starting `from` first|last| \
     center|random (seeded). `intensity` scales how far a move goes (1 as designed), `repeat` is \
     once|loop|pingpong, `ease` overrides the move's own."
}

/// The arguments the scene moves add to `animate_preset`.
pub fn extra_properties() -> Map<String, Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    let num = |d: &str| json!({"type": "number", "description": d});
    let props = json!({
        "intensity": {"type": "number", "minimum": MIN_STRENGTH, "maximum": MAX_STRENGTH, "description": "How far the move goes; 1 is as designed (the same as strength)."},
        "repeat": {"type": "string", "enum": ["once", "loop", "pingpong"], "description": "What the track does after its last key (scene moves; ambient ones loop by default)."},
        "seed": {"type": "integer", "minimum": 0, "description": "Shifts an ambient move's phase and orders a random stagger."},
        "direction": {"type": "string", "enum": ["left", "right", "up", "down"], "description": "camera_pan and camera_whip."},
        "amount": num("camera_pan and camera_whip: how far the camera goes, in document units."),
        "center_x": num("orbit: the middle of the circle, in the node's parent coordinates."),
        "center_y": num("orbit: the middle of the circle, in the node's parent coordinates."),
        "radius": num("orbit: the radius when there is no center."),
        "path": id("animate_path: id of the path node to follow."),
        "orient": {"type": "boolean", "description": "animate_path: turn the node to face along the path."},
        "of": id("stagger: the move to apply to each node (any move name except stagger)."),
        "nodes": {"type": "array", "items": {"type": "string"}, "maxItems": MAX_STAGGERED, "description": "stagger: ids of the nodes, in order."},
        "step_ms": {"type": "integer", "minimum": 0, "maximum": 5000, "description": "stagger: delay between one node and the next."},
        "from": {"type": "string", "enum": ["first", "last", "center", "random"], "description": "stagger: which node goes first."},
    });
    props.as_object().cloned().unwrap_or_default()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneArgs {
    video: String,
    #[serde(default)]
    node: Option<String>,
    preset: String,
    start_ms: u64,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    strength: Option<f64>,
    #[serde(default)]
    intensity: Option<f64>,
    #[serde(default)]
    ease: Option<Ease>,
    #[serde(default)]
    repeat: Option<Repeat>,
    #[serde(default)]
    seed: Option<u64>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    amount: Option<f64>,
    #[serde(default)]
    center_x: Option<f64>,
    #[serde(default)]
    center_y: Option<f64>,
    #[serde(default)]
    radius: Option<f64>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    orient: Option<bool>,
    #[serde(default)]
    of: Option<String>,
    #[serde(default)]
    nodes: Vec<String>,
    #[serde(default)]
    step_ms: Option<u64>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

pub fn animate(ctx: &mut CallCtx, a: SceneArgs) -> Result<Output, ToolError> {
    let ops = operations(ctx.ws.session.document(), &a)?;
    if ops.is_empty() {
        return Err(ToolError(format!(
            "{} would not move anything from where it is; give an intensity or pick another move",
            a.preset
        )));
    }
    apply_ops(
        ctx,
        ApplyArgs {
            ops,
            turn: a.turn,
            summary: a.summary,
        },
    )
}

/// The `animate` operations a request amounts to.
pub fn operations(doc: &Document, a: &SceneArgs) -> Result<Vec<Operation>, ToolError> {
    let video_id = node_id(&a.video)?;
    let video = doc
        .video(&video_id)
        .ok_or_else(|| ToolError(format!("no video {}", a.video)))?;
    let intensity = a.intensity.or(a.strength).unwrap_or(1.0);
    if !(MIN_STRENGTH..=MAX_STRENGTH).contains(&intensity) {
        return Err(ToolError(format!(
            "intensity {intensity} is not from {MIN_STRENGTH} to {MAX_STRENGTH}"
        )));
    }
    let request = Request {
        doc,
        video,
        a,
        intensity,
    };
    if a.preset == STAGGER {
        return request.stagger();
    }
    if a.preset == PATH_MOVE {
        return request.along_path();
    }
    let named = Named::parse(&a.preset)
        .ok_or_else(|| ToolError(format!("{:?} is not a scene move", a.preset)))?;
    request.named(named, a.start_ms, a.node.as_deref(), a.seed.unwrap_or(0))
}

struct Request<'a> {
    doc: &'a Document,
    video: &'a Video,
    a: &'a SceneArgs,
    intensity: f64,
}

impl Request<'_> {
    fn spec(&self, named: Option<Named>, start_ms: u64, seed: u64) -> Result<Spec, ToolError> {
        let duration = self
            .a
            .duration_ms
            .or_else(|| named.map(Named::default_duration))
            .ok_or_else(|| ToolError("duration_ms is needed for this move".into()))?;
        if !(MIN_MS..=MAX_MS).contains(&duration) || start_ms + duration > MAX_VIDEO_MS {
            return Err(ToolError(format!(
                "start_ms {start_ms} and duration_ms {duration} must give a move of 100 to {MAX_MS} ms that ends by {MAX_VIDEO_MS} ms"
            )));
        }
        let side = self
            .a
            .direction
            .as_deref()
            .map(|d| {
                Side::parse(d).ok_or_else(|| {
                    ToolError(format!("direction {d:?} is not left, right, up or down"))
                })
            })
            .transpose()?;
        Ok(Spec {
            ease: self.a.ease,
            intensity: self.intensity,
            repeat: self.a.repeat,
            seed,
            side,
            amount: self.a.amount,
            gap_ms: 1_000 / u64::from(self.video.fps.max(1)) + 2,
            ..Spec::new(start_ms, duration)
        })
    }

    fn target(&self, id: Option<&str>) -> Result<&Node, ToolError> {
        let id = id.ok_or_else(|| ToolError("this move needs `node`".into()))?;
        let id = node_id(id)?;
        let node = self
            .doc
            .node(&id)
            .ok_or_else(|| ToolError(format!("no node {id}")))?;
        if node.kind == NodeKind::Frame {
            return Err(ToolError(format!(
                "{} does not apply to frame nodes: move a node inside the frame",
                self.a.preset
            )));
        }
        Ok(node)
    }

    fn named(
        &self,
        named: Named,
        start_ms: u64,
        node: Option<&str>,
        seed: u64,
    ) -> Result<Vec<Operation>, ToolError> {
        let spec = self.spec(Some(named), start_ms, seed)?;
        if named.is_camera() {
            let camera = self.video.camera_at(start_ms as f64);
            let cam = CameraPose {
                x: camera.x,
                y: camera.y,
                zoom: camera.zoom,
                rotation: camera.rotation,
                stage_w: f64::from(self.video.width),
                stage_h: f64::from(self.video.height),
            };
            let tracks = camera_tracks(named, &spec, &cam);
            return Ok(self.operations(&self.video.id, tracks, &spec));
        }
        let n = self.target(node)?;
        let pose = self.pose(n, start_ms);
        let tracks = match named {
            Named::Orbit => {
                let center = self.a.center_x.zip(self.a.center_y);
                orbit(&spec, &pose, center, self.a.radius)
            }
            Named::DrawOn if n.stroke.is_none() => {
                return Err(ToolError(format!(
                    "{} has no stroke to draw on; give it a stroke first",
                    n.id
                )));
            }
            _ => node_tracks(named, &spec, &pose),
        };
        Ok(self.operations(&n.id, tracks, &spec))
    }

    fn stagger(&self) -> Result<Vec<Operation>, ToolError> {
        let of = self.a.of.as_deref().and_then(Named::parse).ok_or_else(|| {
            ToolError("stagger needs `of`: the name of a move such as pop_in".into())
        })?;
        if self.a.nodes.is_empty() || self.a.nodes.len() > MAX_STAGGERED {
            return Err(ToolError(format!(
                "stagger needs `nodes`: 1 to {MAX_STAGGERED} ids"
            )));
        }
        let from = match self.a.from.as_deref() {
            None => Origin::First,
            Some(f) => Origin::parse(f).ok_or_else(|| {
                ToolError(format!("from {f:?} is not first, last, center or random"))
            })?,
        };
        let seed = self.a.seed.unwrap_or(0);
        let delays = stagger_delays(
            self.a.nodes.len(),
            self.a.step_ms.unwrap_or(120),
            from,
            seed,
        );
        let mut ops = Vec::new();
        for (i, (id, delay)) in self.a.nodes.iter().zip(delays).enumerate() {
            let each = seed.wrapping_add(i as u64 * 7919);
            ops.extend(self.named(of, self.a.start_ms + delay, Some(id), each)?);
        }
        Ok(ops)
    }

    fn along_path(&self) -> Result<Vec<Operation>, ToolError> {
        let n = self.target(self.a.node.as_deref())?;
        let curve = self.target(self.a.path.as_deref())?;
        if curve.kind != NodeKind::Path {
            return Err(ToolError(format!("{} is not a path node", curve.id)));
        }
        let spec = self.spec(None, self.a.start_ms, self.a.seed.unwrap_or(0))?;
        let points = self.path_points(curve, n)?;
        let pose = self.pose(n, spec.start_ms);
        let tracks = follow(&spec, &pose, &points, self.a.orient.unwrap_or(false));
        Ok(self.operations(&n.id, tracks, &spec))
    }

    /// The curve's points in the moving node's parent coordinates.
    fn path_points(&self, curve: &Node, mover: &Node) -> Result<Vec<(f64, f64)>, ToolError> {
        let parsed = vector::node_path(curve)
            .ok_or_else(|| ToolError(format!("{} has no usable path data", curve.id)))?;
        let fitted = vector::fit_to_box(&parsed, curve.view_box, curve.width, curve.height);
        let step = (curve.width.max(curve.height) / 64.0).max(0.25);
        let contour: path::Contour = fitted
            .flatten(step)
            .into_iter()
            .next()
            .ok_or_else(|| ToolError("the path has no points".into()))?;
        let mut line: Vec<(f64, f64)> = contour.points.iter().map(|p| (p[0], p[1])).collect();
        if contour.closed {
            if let Some(first) = line.first().copied() {
                line.push(first);
            }
        }
        let from = origin_of(self.doc, &curve.id)
            .ok_or_else(|| ToolError("the path is not in a page".into()))?;
        let to = origin_of(self.doc, &mover.id)
            .ok_or_else(|| ToolError("the node is not in a page".into()))?;
        let (dx, dy) = (from.0 + curve.x - to.0, from.1 + curve.y - to.1);
        Ok(resample(&line, line.len().max(2))
            .into_iter()
            .map(|p| (p.0 + dx, p.1 + dy))
            .collect())
    }

    fn pose(&self, n: &Node, at: u64) -> Pose {
        let base = |prop: AnimProp, fallback: f64| {
            self.video
                .animations
                .iter()
                .find(|a| a.node == n.id && a.prop == prop)
                .and_then(|a| a.value_at(at as f64))
                .or_else(|| prop.read(n))
                .unwrap_or(fallback)
        };
        let (w, h) = extent_of(n);
        Pose {
            x: base(AnimProp::X, n.x),
            y: base(AnimProp::Y, n.y),
            width: base(AnimProp::Width, w),
            height: base(AnimProp::Height, h),
            rotation: base(AnimProp::Rotation, 0.0),
            scale_x: base(AnimProp::ScaleX, 1.0),
            scale_y: base(AnimProp::ScaleY, 1.0),
            opacity: base(AnimProp::Opacity, 1.0),
            stage_w: f64::from(self.video.width),
            stage_h: f64::from(self.video.height),
        }
    }

    /// `animate` operations for the tracks: a repeating track replaces the one there; a track that
    /// plays once keeps the keys outside its stretch of time, so moves chain.
    fn operations(&self, node: &NodeId, tracks: Vec<Track>, spec: &Spec) -> Vec<Operation> {
        tracks
            .into_iter()
            .map(|mut t| {
                self.snap(&mut t);
                let existing = self
                    .video
                    .animations
                    .iter()
                    .find(|a| &a.node == node && a.prop == t.prop);
                let (keys, repeat) = match existing {
                    Some(e) if t.repeat == Repeat::Once => {
                        let (first, last) = window(&t.keys, spec);
                        let mut keys: Vec<Key> = e
                            .keys
                            .iter()
                            .filter(|k| k.at_ms < first || k.at_ms > last)
                            .copied()
                            .collect();
                        keys.extend(t.keys);
                        keys.sort_by_key(|k| k.at_ms);
                        (keys, e.repeat)
                    }
                    _ => (t.keys, t.repeat),
                };
                Operation::Animate {
                    video: self.video.id.clone(),
                    node: node.clone(),
                    prop: t.prop,
                    keys,
                    repeat,
                }
            })
            .collect()
    }
}

impl Request<'_> {
    /// The keys on the video's frame grid, which is where `animate` puts them, so the keys kept
    /// from before are compared with where the new ones will really be.
    fn snap(&self, t: &mut Track) {
        for k in &mut t.keys {
            k.at_ms = snap_within(k.at_ms, self.video.fps, 0, MAX_VIDEO_MS).unwrap_or(k.at_ms);
        }
        t.keys.dedup_by_key(|k| k.at_ms);
    }
}

fn window(keys: &[Key], spec: &Spec) -> (u64, u64) {
    let first = keys.first().map_or(spec.start_ms, |k| k.at_ms);
    let last = keys.last().map_or(spec.end_ms(), |k| k.at_ms);
    (first, last)
}

/// The size a node has, or for a group the box around its children.
fn extent_of(n: &Node) -> (f64, f64) {
    if n.kind != NodeKind::Group || n.width > 0.0 || n.height > 0.0 {
        return (n.width, n.height);
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for c in &n.children {
        let (w, h) = extent_of(c);
        x0 = x0.min(c.x);
        y0 = y0.min(c.y);
        x1 = x1.max(c.x + w);
        y1 = y1.max(c.y + h);
    }
    if x1 > x0 && y1 > y0 {
        (x1 - x0, y1 - y0)
    } else {
        (0.0, 0.0)
    }
}

/// Where a node's own origin is, summing its ancestors' offsets (their turns and scales aside).
pub fn origin_of(doc: &Document, id: &NodeId) -> Option<(f64, f64)> {
    fn walk(n: &Node, id: &NodeId, at: (f64, f64)) -> Option<(f64, f64)> {
        if &n.id == id {
            return Some(at);
        }
        let inner = (at.0 + n.x, at.1 + n.y);
        n.children.iter().find_map(|c| walk(c, id, inner))
    }
    doc.pages.iter().find_map(|p| walk(&p.root, id, (0.0, 0.0)))
}

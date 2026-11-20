//! Keyframe animation: any number of a node (its position, size, opacity, a 3D device's turn,
//! zoom, lens, light or screen scroll) moving over a video's time
//! (`docs/wiki/video-editing.md#keyframes`).
//!
//! Animations belong to a video, not to the node: the node keeps the value the design shows,
//! and the video moves it while it plays. They change only through the `animate` operation, so
//! they have history, attribution and checkpoints like every other edit. Motion presets are
//! data too: [`preset_ops`] turns a named move into ordinary `animate` operations.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::grade::GradeParam;
use crate::model::{Document, Node, NodeId, NodeKind};
use crate::ops::{AppliedOp, Operation, Property, set_property};
use crate::video::{Ease, MAX_VIDEO_MS, Video};

/// Animations per video, and keys per animation.
pub const MAX_ANIMATIONS: usize = 2_000;
pub const MAX_KEYS: usize = 500;
/// Most pictures blended into one frame for motion blur.
pub const MAX_MOTION_BLUR: u32 = 16;

/// A number of a node a video can move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimProp {
    X,
    Y,
    Width,
    Height,
    Opacity,
    CornerRadius,
    RotateX,
    RotateY,
    RotateZ,
    Zoom,
    Fov,
    LightAngle,
    Shadow,
    Glare,
    ScreenScroll,
    GradeExposure,
    GradeContrast,
    GradeSaturation,
    GradeTemperature,
    GradeTint,
    GradeVignette,
    GradeBlur,
}

impl AnimProp {
    /// The grade number this property moves, for the image nodes that carry a grade.
    fn grade_param(self) -> Option<GradeParam> {
        Some(match self {
            AnimProp::GradeExposure => GradeParam::Exposure,
            AnimProp::GradeContrast => GradeParam::Contrast,
            AnimProp::GradeSaturation => GradeParam::Saturation,
            AnimProp::GradeTemperature => GradeParam::Temperature,
            AnimProp::GradeTint => GradeParam::Tint,
            AnimProp::GradeVignette => GradeParam::Vignette,
            AnimProp::GradeBlur => GradeParam::Blur,
            _ => return None,
        })
    }

    pub const ALL: [AnimProp; 22] = [
        AnimProp::X,
        AnimProp::Y,
        AnimProp::Width,
        AnimProp::Height,
        AnimProp::Opacity,
        AnimProp::CornerRadius,
        AnimProp::RotateX,
        AnimProp::RotateY,
        AnimProp::RotateZ,
        AnimProp::Zoom,
        AnimProp::Fov,
        AnimProp::LightAngle,
        AnimProp::Shadow,
        AnimProp::Glare,
        AnimProp::ScreenScroll,
        AnimProp::GradeExposure,
        AnimProp::GradeContrast,
        AnimProp::GradeSaturation,
        AnimProp::GradeTemperature,
        AnimProp::GradeTint,
        AnimProp::GradeVignette,
        AnimProp::GradeBlur,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AnimProp::X => "x",
            AnimProp::Y => "y",
            AnimProp::Width => "width",
            AnimProp::Height => "height",
            AnimProp::Opacity => "opacity",
            AnimProp::CornerRadius => "corner_radius",
            AnimProp::RotateX => "rotate_x",
            AnimProp::RotateY => "rotate_y",
            AnimProp::RotateZ => "rotate_z",
            AnimProp::Zoom => "zoom",
            AnimProp::Fov => "fov",
            AnimProp::LightAngle => "light_angle",
            AnimProp::Shadow => "shadow",
            AnimProp::Glare => "glare",
            AnimProp::ScreenScroll => "screen_scroll",
            AnimProp::GradeExposure => "grade_exposure",
            AnimProp::GradeContrast => "grade_contrast",
            AnimProp::GradeSaturation => "grade_saturation",
            AnimProp::GradeTemperature => "grade_temperature",
            AnimProp::GradeTint => "grade_tint",
            AnimProp::GradeVignette => "grade_vignette",
            AnimProp::GradeBlur => "grade_blur",
        }
    }

    /// The `set` that puts `value` on a node.
    pub fn property(self, value: f64) -> Property {
        match self {
            AnimProp::X => Property::X(value),
            AnimProp::Y => Property::Y(value),
            AnimProp::Width => Property::Width(value),
            AnimProp::Height => Property::Height(value),
            AnimProp::Opacity => Property::Opacity(value),
            AnimProp::CornerRadius => Property::CornerRadius(value),
            AnimProp::RotateX => Property::RotateX(value),
            AnimProp::RotateY => Property::RotateY(value),
            AnimProp::RotateZ => Property::RotateZ(value),
            AnimProp::Zoom => Property::Zoom(value),
            AnimProp::Fov => Property::Fov(value),
            AnimProp::LightAngle => Property::LightAngle(value),
            AnimProp::Shadow => Property::Shadow(value),
            AnimProp::Glare => Property::Glare(value),
            AnimProp::ScreenScroll => Property::ScreenScroll(Some(value)),
            grade => Property::GradeParam {
                param: grade.grade_param().unwrap_or(GradeParam::Exposure),
                value,
            },
        }
    }

    /// The node's own value (what the design shows when nothing plays).
    pub fn read(self, node: &Node) -> Option<f64> {
        let d = node.device3d.as_ref();
        match self {
            AnimProp::X => Some(node.x),
            AnimProp::Y => Some(node.y),
            AnimProp::Width => Some(node.width),
            AnimProp::Height => Some(node.height),
            AnimProp::Opacity => Some(node.opacity),
            AnimProp::CornerRadius => Some(node.corner_radius),
            AnimProp::RotateX => d.map(|d| d.rotate_x),
            AnimProp::RotateY => d.map(|d| d.rotate_y),
            AnimProp::RotateZ => d.map(|d| d.rotate_z),
            AnimProp::Zoom => d.map(|d| d.zoom),
            AnimProp::Fov => d.map(|d| d.fov),
            AnimProp::LightAngle => d.map(|d| d.light_angle),
            AnimProp::Shadow => d.map(|d| d.shadow),
            AnimProp::Glare => d.map(|d| d.glare),
            // unset means centred, which is halfway down a long capture
            AnimProp::ScreenScroll => d.map(|d| d.screen_scroll.unwrap_or(0.5)),
            grade => grade
                .grade_param()
                .filter(|_| node.kind == NodeKind::Image)
                .map(|p| p.get(&node.grade.unwrap_or_default())),
        }
    }

    /// The range a value is held to while easing overshoots (`back`, `spring`).
    fn range(self) -> (f64, f64) {
        use crate::device3d as d;
        use crate::limits::{MAX_COORD, MAX_SIZE};
        match self {
            AnimProp::X | AnimProp::Y => (-MAX_COORD, MAX_COORD),
            AnimProp::Width | AnimProp::Height | AnimProp::CornerRadius => (0.0, MAX_SIZE),
            AnimProp::Opacity | AnimProp::Shadow | AnimProp::Glare | AnimProp::ScreenScroll => {
                (0.0, 1.0)
            }
            AnimProp::RotateX | AnimProp::RotateY | AnimProp::RotateZ => {
                (-d::MAX_ROTATION, d::MAX_ROTATION)
            }
            AnimProp::Zoom => (d::MIN_ZOOM, d::MAX_ZOOM),
            AnimProp::Fov => (d::MIN_FOV, d::MAX_FOV),
            AnimProp::LightAngle => (-d::MAX_LIGHT_ANGLE, d::MAX_LIGHT_ANGLE),
            grade => grade.grade_param().map_or((0.0, 0.0), GradeParam::range),
        }
    }
}

/// One keyframe: the value at `at_ms` from the start of the video, reached from the key before
/// with `ease`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Key {
    pub at_ms: u64,
    pub value: f64,
    #[serde(default)]
    pub ease: Ease,
}

/// One number of one node moving through its keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Animation {
    pub node: NodeId,
    pub prop: AnimProp,
    pub keys: Vec<Key>,
}

impl Animation {
    /// The value at `time_ms` (fractions allowed, for motion blur): the first key's before it,
    /// the last key's after it, eased in between and held to the property's range.
    pub fn value_at(&self, time_ms: f64) -> Option<f64> {
        let first = self.keys.first()?;
        let last = self.keys.last()?;
        if time_ms <= first.at_ms as f64 {
            return Some(first.value);
        }
        if time_ms >= last.at_ms as f64 {
            return Some(last.value);
        }
        let i = self.keys.partition_point(|k| (k.at_ms as f64) <= time_ms);
        let (a, b) = (self.keys[i - 1], self.keys[i]);
        let span = (b.at_ms - a.at_ms) as f64;
        let p = b.ease.apply((time_ms - a.at_ms as f64) / span);
        let (lo, hi) = self.prop.range();
        Some((a.value + (b.value - a.value) * p).clamp(lo, hi))
    }

    /// Whether the value changes around `time_ms`: inside a stretch between two keys that differ.
    pub fn moving_at(&self, time_ms: f64) -> bool {
        self.keys.windows(2).any(|w| {
            (w[0].at_ms as f64) <= time_ms
                && time_ms < w[1].at_ms as f64
                && (w[0].value - w[1].value).abs() > f64::EPSILON
        })
    }
}

/// Keys in time order, at most [`MAX_KEYS`], inside the longest video.
fn check_keys(keys: &[Key]) -> Result<(), OpError> {
    if keys.len() > MAX_KEYS {
        return Err(OpError::Limit(format!(
            "at most {MAX_KEYS} keys per animation"
        )));
    }
    for k in keys {
        if k.at_ms > MAX_VIDEO_MS {
            return Err(OpError::OutOfBounds {
                what: "key at_ms",
                detail: format!("{} is later than {MAX_VIDEO_MS} ms", k.at_ms),
            });
        }
    }
    if keys.windows(2).any(|w| w[0].at_ms >= w[1].at_ms) {
        return Err(OpError::OutOfBounds {
            what: "keys",
            detail: "at_ms must strictly increase from one key to the next".to_string(),
        });
    }
    Ok(())
}

/// The keys as `set` stores them: each value checked against the node (kind and bounds) and
/// rounded the same way.
fn resolve_keys(
    node: &Node,
    prop: AnimProp,
    keys: &[Key],
    tokens: &BTreeMap<String, String>,
) -> Result<Vec<Key>, OpError> {
    check_keys(keys)?;
    let mut scratch = node.clone();
    scratch.children.clear();
    keys.iter()
        .map(|k| {
            set_property(&mut scratch, prop.property(k.value), tokens)?;
            let value = prop
                .read(&scratch)
                .ok_or_else(|| OpError::Internal("animated value vanished".into()))?;
            Ok(Key { value, ..*k })
        })
        .collect()
}

/// Structure checks a video's animations need without a document: sorted, unique, bounded.
pub(crate) fn check_animations(v: &Video) -> Result<(), OpError> {
    if v.animations.len() > MAX_ANIMATIONS {
        return Err(OpError::Limit(format!(
            "at most {MAX_ANIMATIONS} animations per video"
        )));
    }
    if v.animations
        .windows(2)
        .any(|w| (&w[0].node, w[0].prop) >= (&w[1].node, w[1].prop))
    {
        return Err(OpError::OutOfBounds {
            what: "animations",
            detail: "must be sorted by node and property, one per pair".to_string(),
        });
    }
    if v.motion_blur == 1 || v.motion_blur > MAX_MOTION_BLUR {
        return Err(OpError::OutOfBounds {
            what: "motion_blur",
            detail: format!(
                "{} is not 0 (off) or a number of samples from 2 to {MAX_MOTION_BLUR}",
                v.motion_blur
            ),
        });
    }
    for a in &v.animations {
        if a.keys.is_empty() {
            return Err(OpError::OutOfBounds {
                what: "animation",
                detail: format!("{} {} has no keys", a.node, a.prop.as_str()),
            });
        }
        check_keys(&a.keys)?;
    }
    Ok(())
}

impl Document {
    /// Every animation's node exists and every key is a value its node accepts.
    pub(crate) fn validate_animations(&self, v: &Video) -> Result<(), OpError> {
        for a in &v.animations {
            let node = self
                .node(&a.node)
                .ok_or_else(|| OpError::NoSuchNode(a.node.to_string()))?;
            let resolved = resolve_keys(node, a.prop, &a.keys, &self.tokens)?;
            if resolved != a.keys {
                return Err(OpError::OutOfBounds {
                    what: "keys",
                    detail: format!(
                        "{} {} holds values that are not stored as set would store them",
                        a.node,
                        a.prop.as_str()
                    ),
                });
            }
        }
        Ok(())
    }

    /// The video, node and property of an animation on any of `ids`.
    pub fn animation_of(&self, ids: &[NodeId]) -> Option<(NodeId, NodeId, &'static str)> {
        self.videos.iter().find_map(|v| {
            v.animations
                .iter()
                .find(|a| ids.contains(&a.node))
                .map(|a| (v.id.clone(), a.node.clone(), a.prop.as_str()))
        })
    }

    /// The document as it looks at `time_ms` into `video`: every animated node shows its value
    /// at that moment. `None` when the video animates nothing.
    pub fn at_time(&self, video: &Video, time_ms: f64) -> Option<Document> {
        if video.animations.is_empty() {
            return None;
        }
        let mut values: HashMap<&NodeId, Vec<(AnimProp, f64)>> = HashMap::new();
        for a in &video.animations {
            if let Some(v) = a.value_at(time_ms) {
                values.entry(&a.node).or_default().push((a.prop, v));
            }
        }
        fn walk(
            n: &mut Node,
            values: &HashMap<&NodeId, Vec<(AnimProp, f64)>>,
            tokens: &BTreeMap<String, String>,
        ) {
            if let Some(list) = values.get(&n.id) {
                for (prop, v) in list {
                    // values were checked when the keys were set; a clamped overshoot stays valid
                    let _ = set_property(n, prop.property(*v), tokens);
                }
            }
            for c in &mut n.children {
                walk(c, values, tokens);
            }
        }
        let mut doc = self.clone();
        let tokens = doc.tokens.clone();
        for page in &mut doc.pages {
            walk(&mut page.root, &values, &tokens);
        }
        Some(doc)
    }
}

/// Applies `animate`: replaces the keys of one node's property in a video (none removes them).
pub(crate) fn apply(
    doc: &mut Document,
    video: &NodeId,
    node: &NodeId,
    prop: AnimProp,
    keys: &[Key],
    allow_internal: bool,
) -> Result<AppliedOp, OpError> {
    let snapped;
    let keys = match doc.videos.iter().find(|v| &v.id == video) {
        Some(v) if !allow_internal => {
            snapped = crate::snap::keys(keys, v.fps);
            &snapped[..]
        }
        _ => keys,
    };
    let at = doc
        .videos
        .iter()
        .position(|v| &v.id == video)
        .ok_or_else(|| OpError::NoSuchVideo(video.to_string()))?;
    let loc = doc
        .locate(node)
        .ok_or_else(|| OpError::NoSuchNode(node.to_string()))?;
    if loc.path.is_empty() {
        let page = doc.pages.get(loc.page).map(|p| p.id.to_string());
        return Err(OpError::RootImmutable(page.unwrap_or_default()));
    }
    let target = doc
        .node_at(&loc)
        .ok_or_else(|| OpError::NoSuchNode(node.to_string()))?;
    let resolved = resolve_keys(target, prop, keys, &doc.tokens)?;
    let list = &mut doc.videos[at].animations;
    let slot = list.binary_search_by(|a| (&a.node, a.prop).cmp(&(node, prop)));
    let old = match (slot, resolved.is_empty()) {
        (Ok(i), true) => list.remove(i).keys,
        (Ok(i), false) => std::mem::replace(&mut list[i].keys, resolved.clone()),
        (Err(_), true) => Vec::new(),
        (Err(i), false) => {
            if list.len() >= MAX_ANIMATIONS {
                return Err(OpError::Limit(format!(
                    "at most {MAX_ANIMATIONS} animations per video"
                )));
            }
            list.insert(
                i,
                Animation {
                    node: node.clone(),
                    prop,
                    keys: resolved.clone(),
                },
            );
            Vec::new()
        }
    };
    let op = |keys: Vec<Key>| Operation::Animate {
        video: video.clone(),
        node: node.clone(),
        prop,
        keys,
    };
    Ok(AppliedOp {
        forward: op(resolved),
        inverse: op(old),
        touched: vec![video.clone(), node.clone()],
    })
}

/// A named move an agent asks for instead of writing keys by hand. Each one is measured from
/// the node's own pose (or its animated value where it starts), so it lands on the design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// One full turn around the vertical axis (3D).
    Spin,
    /// A slow turn from one side to the other, as on a display stand (3D).
    Turntable,
    /// Turns away one way, then the other, and settles back (3D).
    Swing,
    /// Arrives from showing its back, turning to face the viewer with a spring (3D).
    FlipIn,
    /// Starts flat and square to the viewer and settles into the designed pose (3D).
    HeroReveal,
    /// Rises from below while fading in; a 3D device also tips up.
    RiseIn,
    /// Falls in from above and settles with a spring; a 3D device also rolls level.
    DropIn,
    /// Drifts gently up and down; a 3D device also sways.
    Float,
    /// Moves the camera closer (3D zoom).
    PushIn,
    /// Starts close and pulls back to the designed size (3D zoom).
    PullBack,
    /// Swings the view around the device, a camera orbit (3D).
    Orbit,
    /// Scrolls the screen capture from its top to its bottom (3D).
    ScrollScreen,
    FadeIn,
    FadeOut,
    /// Slides in from its own width to the left.
    SlideInLeft,
    /// Slides in from its own width to the right.
    SlideInRight,
}

impl Preset {
    pub const ALL: [Preset; 16] = [
        Preset::Spin,
        Preset::Turntable,
        Preset::Swing,
        Preset::FlipIn,
        Preset::HeroReveal,
        Preset::RiseIn,
        Preset::DropIn,
        Preset::Float,
        Preset::PushIn,
        Preset::PullBack,
        Preset::Orbit,
        Preset::ScrollScreen,
        Preset::FadeIn,
        Preset::FadeOut,
        Preset::SlideInLeft,
        Preset::SlideInRight,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Preset::Spin => "spin",
            Preset::Turntable => "turntable",
            Preset::Swing => "swing",
            Preset::FlipIn => "flip_in",
            Preset::HeroReveal => "hero_reveal",
            Preset::RiseIn => "rise_in",
            Preset::DropIn => "drop_in",
            Preset::Float => "float",
            Preset::PushIn => "push_in",
            Preset::PullBack => "pull_back",
            Preset::Orbit => "orbit",
            Preset::ScrollScreen => "scroll_screen",
            Preset::FadeIn => "fade_in",
            Preset::FadeOut => "fade_out",
            Preset::SlideInLeft => "slide_in_left",
            Preset::SlideInRight => "slide_in_right",
        }
    }

    /// Whether the move needs a 3D device.
    pub fn needs_device(self) -> bool {
        !matches!(
            self,
            Preset::RiseIn
                | Preset::DropIn
                | Preset::Float
                | Preset::FadeIn
                | Preset::FadeOut
                | Preset::SlideInLeft
                | Preset::SlideInRight
        )
    }

    /// The ease a preset uses when none is given.
    pub fn default_ease(self) -> Ease {
        match self {
            Preset::FlipIn | Preset::DropIn => Ease::Spring,
            Preset::HeroReveal
            | Preset::RiseIn
            | Preset::PullBack
            | Preset::FadeIn
            | Preset::SlideInLeft
            | Preset::SlideInRight => Ease::EaseOut,
            Preset::FadeOut => Ease::EaseIn,
            Preset::Turntable => Ease::Linear,
            _ => Ease::EaseInOut,
        }
    }
}

/// How a preset is asked for.
#[derive(Debug, Clone, Copy)]
pub struct PresetRequest {
    pub preset: Preset,
    pub start_ms: u64,
    pub duration_ms: u64,
    /// Scales how far the move goes: 1 is the preset as designed.
    pub strength: f64,
    pub ease: Option<Ease>,
}

pub const MIN_STRENGTH: f64 = 0.1;
pub const MAX_STRENGTH: f64 = 4.0;

/// The `animate` operations that perform `req` on `node` in `video`. Keys the preset's stretch
/// of time covers are replaced; keys before and after it are kept, so moves can be chained.
pub fn preset_ops(
    doc: &Document,
    video: &NodeId,
    node: &NodeId,
    req: PresetRequest,
) -> Result<Vec<Operation>, OpError> {
    let v = doc
        .video(video)
        .ok_or_else(|| OpError::NoSuchVideo(video.to_string()))?;
    let n = doc
        .node(node)
        .ok_or_else(|| OpError::NoSuchNode(node.to_string()))?;
    let is_device = n.kind == NodeKind::Device3d;
    if req.preset.needs_device() && !is_device {
        return Err(OpError::NotApplicable {
            property: req.preset.as_str(),
            kind: n.kind.as_str(),
        });
    }
    crate::limits::finite_in("strength", req.strength, MIN_STRENGTH, MAX_STRENGTH)?;
    if !(100..=crate::video::MAX_CLIP_MS as u64 * 10).contains(&req.duration_ms) {
        return Err(OpError::OutOfBounds {
            what: "duration_ms",
            detail: format!("{} is not from 100 to 600000 ms", req.duration_ms),
        });
    }
    let end = req.start_ms + req.duration_ms;
    if end > MAX_VIDEO_MS {
        return Err(OpError::OutOfBounds {
            what: "preset",
            detail: format!("ends at {end} ms, after the longest video"),
        });
    }
    let base = |prop: AnimProp| -> f64 {
        v.animations
            .iter()
            .find(|a| &a.node == node && a.prop == prop)
            .and_then(|a| a.value_at(req.start_ms as f64))
            .or_else(|| prop.read(n))
            .unwrap_or(0.0)
    };
    let tracks = preset_tracks(req, n, is_device, &base);
    let mut ops = Vec::new();
    for (prop, values) in tracks {
        if values.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-9) {
            continue;
        }
        let fresh = spread(
            &values,
            req.start_ms,
            req.duration_ms,
            req.ease.unwrap_or(req.preset.default_ease()),
        );
        let mut keys: Vec<Key> = v
            .animations
            .iter()
            .find(|a| &a.node == node && a.prop == prop)
            .map(|a| {
                a.keys
                    .iter()
                    .filter(|k| k.at_ms < req.start_ms || k.at_ms > end)
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        keys.extend(fresh);
        keys.sort_by_key(|k| k.at_ms);
        ops.push(Operation::Animate {
            video: video.clone(),
            node: node.clone(),
            prop,
            keys,
        });
    }
    Ok(ops)
}

/// Values spaced evenly over the stretch; the first is the start, eased into each next one.
fn spread(values: &[f64], start: u64, duration: u64, ease: Ease) -> Vec<Key> {
    let last = values.len().saturating_sub(1).max(1) as u64;
    values
        .iter()
        .enumerate()
        .map(|(i, &value)| Key {
            at_ms: start + duration * i as u64 / last,
            value,
            ease: if i == 0 { Ease::default() } else { ease },
        })
        .collect()
}

/// The values each property passes through, start to end.
fn preset_tracks(
    req: PresetRequest,
    n: &Node,
    is_device: bool,
    base: &dyn Fn(AnimProp) -> f64,
) -> Vec<(AnimProp, Vec<f64>)> {
    use AnimProp as P;
    let s = req.strength;
    let b = base;
    let to = |p: P, from: f64| (p, vec![from, b(p)]);
    let mut t = Vec::new();
    match req.preset {
        Preset::Spin => t.push((
            P::RotateY,
            vec![b(P::RotateY), b(P::RotateY) + 360.0 * s.round().max(1.0)],
        )),
        Preset::Turntable => t.push((
            P::RotateY,
            vec![b(P::RotateY) - 25.0 * s, b(P::RotateY) + 25.0 * s],
        )),
        Preset::Swing => {
            let y = b(P::RotateY);
            t.push((P::RotateY, vec![y, y - 18.0 * s, y + 18.0 * s, y]));
        }
        Preset::FlipIn => t.push(to(P::RotateY, b(P::RotateY) + 180.0)),
        Preset::HeroReveal => {
            t.push(to(P::RotateX, 0.0));
            t.push(to(P::RotateY, 0.0));
            t.push(to(P::RotateZ, 0.0));
            t.push(to(P::Zoom, b(P::Zoom) * (1.0 - 0.08 * s)));
        }
        Preset::RiseIn => {
            t.push(to(P::Y, b(P::Y) + n.height * 0.3 * s));
            t.push(to(P::Opacity, 0.0));
            if is_device {
                t.push(to(P::RotateX, b(P::RotateX) + 30.0 * s));
            }
        }
        Preset::DropIn => {
            t.push(to(P::Y, b(P::Y) - n.height * 0.5 * s));
            if is_device {
                t.push(to(P::RotateZ, b(P::RotateZ) - 10.0 * s));
            }
        }
        Preset::Float => {
            let y = b(P::Y);
            let a = n.height * 0.025 * s;
            t.push((P::Y, vec![y, y - a, y, y + a, y]));
            if is_device {
                let (rx, ry) = (b(P::RotateX), b(P::RotateY));
                t.push((P::RotateX, vec![rx, rx + 3.0 * s, rx, rx - 3.0 * s, rx]));
                t.push((P::RotateY, vec![ry, ry - 5.0 * s, ry, ry + 5.0 * s, ry]));
            }
        }
        Preset::PushIn => t.push((
            P::Zoom,
            vec![b(P::Zoom), clamp_zoom(b(P::Zoom) * (1.0 + 0.15 * s))],
        )),
        Preset::PullBack => t.push(to(P::Zoom, clamp_zoom(b(P::Zoom) * (1.0 + 0.2 * s)))),
        Preset::Orbit => {
            let (rx, ry) = (b(P::RotateX), b(P::RotateY));
            t.push((P::RotateY, vec![ry - 40.0 * s, ry + 40.0 * s]));
            t.push((P::RotateX, vec![rx + 8.0 * s, rx - 8.0 * s]));
        }
        Preset::ScrollScreen => t.push((P::ScreenScroll, vec![0.0, 1.0])),
        Preset::FadeIn => t.push(to(P::Opacity, 0.0)),
        Preset::FadeOut => t.push((P::Opacity, vec![b(P::Opacity), 0.0])),
        Preset::SlideInLeft => t.push(to(P::X, b(P::X) - n.width * 1.2 * s)),
        Preset::SlideInRight => t.push(to(P::X, b(P::X) + n.width * 1.2 * s)),
    }
    t
}

fn clamp_zoom(z: f64) -> f64 {
    z.clamp(crate::device3d::MIN_ZOOM, crate::device3d::MAX_ZOOM)
}

/// One sentence per animation for a video's story: what moves, through which values, when.
pub fn describe(doc: &Document, a: &Animation) -> String {
    let name = doc.node(&a.node).map_or_else(
        || a.node.to_string(),
        |n| format!("{} ({})", n.name, a.node),
    );
    let ease_name = |e: Ease| {
        serde_json::to_value(e)
            .ok()
            .and_then(|j| j.as_str().map(str::to_string))
            .unwrap_or_default()
    };
    let steps: Vec<String> = a
        .keys
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let at = format!("{} at {:.1} s", trim(k.value), k.at_ms as f64 / 1000.0);
            if i == 0 {
                at
            } else {
                format!("{at} ({})", ease_name(k.ease))
            }
        })
        .collect();
    format!("{name} {}: {}", a.prop.as_str(), steps.join(", then "))
}

fn trim(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim(keys: &[(u64, f64, Ease)]) -> Animation {
        Animation {
            node: NodeId::parse("n_1").unwrap(),
            prop: AnimProp::Opacity,
            keys: keys
                .iter()
                .map(|&(at_ms, value, ease)| Key { at_ms, value, ease })
                .collect(),
        }
    }

    #[test]
    fn values_hold_outside_the_keys_and_ease_between_them() {
        let a = anim(&[(1000, 0.0, Ease::Linear), (2000, 1.0, Ease::Linear)]);
        assert_eq!(a.value_at(0.0), Some(0.0));
        assert_eq!(a.value_at(1500.0), Some(0.5));
        assert_eq!(a.value_at(9000.0), Some(1.0));
        assert!(a.moving_at(1500.0) && !a.moving_at(2500.0) && !a.moving_at(500.0));
    }

    #[test]
    fn overshooting_eases_stay_inside_the_property_range() {
        let a = anim(&[(0, 0.0, Ease::Linear), (1000, 1.0, Ease::Spring)]);
        let peak = (0..1000)
            .filter_map(|t| a.value_at(f64::from(t)))
            .fold(0.0_f64, f64::max);
        assert!(peak <= 1.0, "opacity overshot to {peak}");
    }

    #[test]
    fn keys_must_move_forward_in_time() {
        let bad = [
            Key {
                at_ms: 500,
                value: 0.0,
                ease: Ease::Linear,
            },
            Key {
                at_ms: 500,
                value: 1.0,
                ease: Ease::Linear,
            },
        ];
        assert!(check_keys(&bad).is_err());
    }
}

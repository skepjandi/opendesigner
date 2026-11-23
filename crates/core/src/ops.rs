//! Operations: the only way a document changes. Each is validated, applied, and returns the
//! resolved forward operation plus its inverse so history can undo, redo and restore.

use std::collections::BTreeMap;
use std::mem::replace;

use serde::{Deserialize, Serialize};

use crate::animation::{AnimProp, Key, Repeat};
use crate::device3d::{self, Device3d, DeviceModel};
use crate::error::OpError;
use crate::limits::{self, finite_in, text_within};
use crate::model::{
    Document, ImageFit, ImageSmoothing, Node, NodeId, NodeKind, Stroke, TextAlign,
    validate_node_fields,
};
use crate::paint::Paint;
use crate::transform::Shadow;
use crate::vector::{FillRule, ViewBox};
use crate::video::{
    Clip, ClipProperty, Footage, Motion, Overlay, OverlayProperty, Transition, Video, VideoProperty,
};

/// One settable property with its new value. Serialized as `{"name": "x", "value": 10}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum Property {
    Name(String),
    X(f64),
    Y(f64),
    Width(f64),
    Height(f64),
    Visible(bool),
    Opacity(f64),
    /// Degrees clockwise about the pivot.
    Rotation(f64),
    ScaleX(f64),
    ScaleY(f64),
    /// Both scale factors at once; undone by `scale_xy`.
    Scale(f64),
    ScaleXy {
        x: f64,
        y: f64,
    },
    /// Fractions of the node's box that rotation and scale turn about.
    PivotX(f64),
    PivotY(f64),
    Blur(f64),
    /// The node's drop shadow (`null` for none). Named apart from the 3D device's `shadow`.
    DropShadow(Option<Shadow>),
    ShadowDx(f64),
    ShadowDy(f64),
    ShadowBlur(f64),
    Clip(bool),
    Depth(f64),
    Fill(Option<Paint>),
    Stroke(Option<Stroke>),
    CornerRadius(f64),
    Text(String),
    FontSize(f64),
    FontFamily(Option<String>),
    FontWeight(Option<f64>),
    TextAlign(Option<TextAlign>),
    LineHeight(Option<f64>),
    Image(Option<String>),
    ImageFit(Option<ImageFit>),
    ImageSmoothing(Option<ImageSmoothing>),
    /// Which generated 3D model a device3d node draws.
    Model(DeviceModel),
    RotateX(f64),
    RotateY(f64),
    RotateZ(f64),
    Zoom(f64),
    Fov(f64),
    LightAngle(f64),
    Shadow(f64),
    Glare(f64),
    ScreenScroll(Option<f64>),
    /// A recording on a device3d's screen (`null` shows the node's `image` again).
    ScreenFootage(Option<Footage>),
    /// The colour grade of an image node (`null` for none).
    Grade(Option<crate::grade::Grade>),
    /// One number of an image node's grade (what keyframes move).
    GradeParam {
        param: crate::grade::GradeParam,
        value: f64,
    },
    /// SVG path data of a path node.
    Path(String),
    /// The rectangle of path coordinates mapped onto the node's box (`null`: the path's own bounds).
    ViewBox(Option<ViewBox>),
    /// Which side of a self-crossing path is inside (`null`: nonzero).
    FillRule(Option<FillRule>),
    /// The shapes a path node morphs through (`null` for none); see `crate::morph`.
    PathMorph(Option<crate::morph::PathMorph>),
    /// The stroke's width (the node must have a stroke); what a stroke-width keyframe moves.
    StrokeWidth(f64),
    /// Where along the outline the stroke starts, 0 to 1 (the node must have a stroke).
    TrimStart(f64),
    /// Where along the outline the stroke ends, 0 to 1 (the node must have a stroke).
    TrimEnd(f64),
}

impl Property {
    /// Whether the property belongs to `crate::transform` (rotation, scale, pivot, blur, shadow,
    /// clip, depth).
    pub(crate) fn is_transform(&self) -> bool {
        matches!(
            self,
            Property::Rotation(_)
                | Property::ScaleX(_)
                | Property::ScaleY(_)
                | Property::Scale(_)
                | Property::ScaleXy { .. }
                | Property::PivotX(_)
                | Property::PivotY(_)
                | Property::Blur(_)
                | Property::DropShadow(_)
                | Property::ShadowDx(_)
                | Property::ShadowDy(_)
                | Property::ShadowBlur(_)
                | Property::Clip(_)
                | Property::Depth(_)
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            Property::Name(_) => "name",
            Property::X(_) => "x",
            Property::Y(_) => "y",
            Property::Width(_) => "width",
            Property::Height(_) => "height",
            Property::Visible(_) => "visible",
            Property::Opacity(_) => "opacity",
            Property::Rotation(_) => "rotation",
            Property::ScaleX(_) => "scale_x",
            Property::ScaleY(_) => "scale_y",
            Property::Scale(_) => "scale",
            Property::ScaleXy { .. } => "scale_xy",
            Property::PivotX(_) => "pivot_x",
            Property::PivotY(_) => "pivot_y",
            Property::Blur(_) => "blur",
            Property::DropShadow(_) => "drop_shadow",
            Property::ShadowDx(_) => "shadow_dx",
            Property::ShadowDy(_) => "shadow_dy",
            Property::ShadowBlur(_) => "shadow_blur",
            Property::Clip(_) => "clip",
            Property::Depth(_) => "depth",
            Property::Fill(_) => "fill",
            Property::Stroke(_) => "stroke",
            Property::CornerRadius(_) => "corner_radius",
            Property::Text(_) => "text",
            Property::FontSize(_) => "font_size",
            Property::FontFamily(_) => "font_family",
            Property::FontWeight(_) => "font_weight",
            Property::TextAlign(_) => "text_align",
            Property::LineHeight(_) => "line_height",
            Property::Image(_) => "image",
            Property::ImageFit(_) => "image_fit",
            Property::ImageSmoothing(_) => "image_smoothing",
            Property::Model(_) => "model",
            Property::RotateX(_) => "rotate_x",
            Property::RotateY(_) => "rotate_y",
            Property::RotateZ(_) => "rotate_z",
            Property::Zoom(_) => "zoom",
            Property::Fov(_) => "fov",
            Property::LightAngle(_) => "light_angle",
            Property::Shadow(_) => "shadow",
            Property::Glare(_) => "glare",
            Property::ScreenScroll(_) => "screen_scroll",
            Property::ScreenFootage(_) => "screen_footage",
            Property::Grade(_) => "grade",
            Property::GradeParam { .. } => "grade_param",
            Property::Path(_) => "path",
            Property::ViewBox(_) => "view_box",
            Property::FillRule(_) => "fill_rule",
            Property::PathMorph(_) => "path_morph",
            Property::StrokeWidth(_) => "stroke_width",
            Property::TrimStart(_) => "trim_start",
            Property::TrimEnd(_) => "trim_end",
        }
    }

    fn applies_to(&self, kind: NodeKind) -> bool {
        use NodeKind::*;
        match self {
            Property::Name(_)
            | Property::X(_)
            | Property::Y(_)
            | Property::Visible(_)
            | Property::Opacity(_)
            | Property::Width(_)
            | Property::Height(_)
            | Property::Rotation(_)
            | Property::ScaleX(_)
            | Property::ScaleY(_)
            | Property::Scale(_)
            | Property::ScaleXy { .. }
            | Property::PivotX(_)
            | Property::PivotY(_)
            | Property::Blur(_)
            | Property::DropShadow(_)
            | Property::ShadowDx(_)
            | Property::ShadowDy(_)
            | Property::ShadowBlur(_)
            | Property::Depth(_) => true,
            Property::Clip(_) => kind.is_container(),
            Property::Fill(_) => matches!(kind, Frame | Rect | Ellipse | Text | Device3d | Path),
            Property::Stroke(_)
            | Property::StrokeWidth(_)
            | Property::TrimStart(_)
            | Property::TrimEnd(_) => matches!(kind, Frame | Rect | Ellipse | Path),
            Property::Path(_)
            | Property::ViewBox(_)
            | Property::FillRule(_)
            | Property::PathMorph(_) => kind == Path,
            Property::CornerRadius(_) => matches!(kind, Frame | Rect | Image),
            Property::Text(_)
            | Property::FontSize(_)
            | Property::FontFamily(_)
            | Property::FontWeight(_)
            | Property::TextAlign(_)
            | Property::LineHeight(_) => kind == Text,
            Property::Image(_) | Property::ImageFit(_) => matches!(kind, Image | Device3d),
            Property::ImageSmoothing(_) => kind == Image,
            Property::Grade(_) | Property::GradeParam { .. } => kind == Image,
            Property::Model(_)
            | Property::RotateX(_)
            | Property::RotateY(_)
            | Property::RotateZ(_)
            | Property::Zoom(_)
            | Property::Fov(_)
            | Property::LightAngle(_)
            | Property::Shadow(_)
            | Property::Glare(_)
            | Property::ScreenScroll(_)
            | Property::ScreenFootage(_) => kind == Device3d,
        }
    }
}

/// Sets `prop` on `node` and returns the previous value as a property that undoes it.
fn apply_property(
    node: &mut Node,
    prop: Property,
    tokens: &BTreeMap<String, String>,
) -> Result<Property, OpError> {
    if !prop.applies_to(node.kind) {
        return Err(OpError::NotApplicable {
            property: prop.name(),
            kind: node.kind.as_str(),
        });
    }
    let coord = |what, v| finite_in(what, v, -limits::MAX_COORD, limits::MAX_COORD);
    let size = |what, v| finite_in(what, v, 0.0, limits::MAX_SIZE);
    Ok(match prop {
        Property::Name(v) => {
            text_within("name", &v, limits::MAX_NAME_LEN)?;
            Property::Name(replace(&mut node.name, v))
        }
        Property::X(v) => Property::X(replace(&mut node.x, coord("x", v)?)),
        Property::Y(v) => Property::Y(replace(&mut node.y, coord("y", v)?)),
        Property::Width(v) => Property::Width(replace(&mut node.width, size("width", v)?)),
        Property::Height(v) => Property::Height(replace(&mut node.height, size("height", v)?)),
        Property::Visible(v) => Property::Visible(replace(&mut node.visible, v)),
        Property::Opacity(v) => {
            let v = finite_in("opacity", v, 0.0, 1.0)?;
            Property::Opacity(replace(&mut node.opacity, v))
        }
        Property::Fill(v) => {
            let v = v.as_ref().map(|p| p.normalized(tokens)).transpose()?;
            Property::Fill(replace(&mut node.fill, v))
        }
        Property::Stroke(v) => {
            let v = v
                .as_ref()
                .map(|s| crate::vector::checked_stroke(s, tokens))
                .transpose()?;
            Property::Stroke(replace(&mut node.stroke, v))
        }
        Property::CornerRadius(v) => {
            let v = size("corner_radius", v)?;
            Property::CornerRadius(replace(&mut node.corner_radius, v))
        }
        Property::Text(v) => {
            text_within("text", &v, limits::MAX_TEXT_LEN)?;
            Property::Text(node.text.replace(v).unwrap_or_default())
        }
        Property::FontSize(v) => {
            let v = finite_in("font_size", v, 1.0, limits::MAX_FONT_SIZE)?;
            Property::FontSize(node.font_size.replace(v).unwrap_or(16.0))
        }
        Property::FontFamily(v) => {
            if let Some(f) = &v {
                limits::family_name(f)?;
            }
            Property::FontFamily(replace(&mut node.font_family, v))
        }
        Property::FontWeight(v) => {
            let v = v
                .map(|w| {
                    finite_in(
                        "font_weight",
                        w,
                        limits::MIN_FONT_WEIGHT,
                        limits::MAX_FONT_WEIGHT,
                    )
                })
                .transpose()?;
            Property::FontWeight(replace(&mut node.font_weight, v))
        }
        Property::TextAlign(v) => Property::TextAlign(replace(&mut node.text_align, v)),
        Property::LineHeight(v) => {
            let v = v
                .map(|h| {
                    finite_in(
                        "line_height",
                        h,
                        limits::MIN_LINE_HEIGHT,
                        limits::MAX_LINE_HEIGHT,
                    )
                })
                .transpose()?;
            Property::LineHeight(replace(&mut node.line_height, v))
        }
        Property::Image(v) => {
            if let Some(p) = &v {
                crate::media::reference_name(p)?;
            }
            Property::Image(replace(&mut node.image, v))
        }
        Property::ImageFit(v) => Property::ImageFit(replace(&mut node.image_fit, v)),
        Property::ImageSmoothing(v) => {
            let v = v.filter(|s| *s != ImageSmoothing::Smooth);
            Property::ImageSmoothing(replace(&mut node.image_smoothing, v))
        }
        Property::Grade(v) => {
            let v = v.as_ref().map(crate::grade::Grade::checked).transpose()?;
            Property::Grade(replace(&mut node.grade, v.filter(|g| !g.is_neutral())))
        }
        Property::GradeParam { param, value } => {
            let value = param.checked(value)?;
            let mut grade = node.grade.unwrap_or_default();
            let old = param.get(&grade);
            param.set(&mut grade, value);
            node.grade = Some(grade).filter(|g| !g.is_neutral());
            Property::GradeParam { param, value: old }
        }
        transform if transform.is_transform() => crate::transform::apply(node, transform, tokens)?,
        vector_prop @ (Property::Path(_)
        | Property::ViewBox(_)
        | Property::FillRule(_)
        | Property::PathMorph(_)
        | Property::StrokeWidth(_)
        | Property::TrimStart(_)
        | Property::TrimEnd(_)) => crate::vector::apply_property(node, vector_prop)?,
        device_prop => apply_device_property(node, device_prop)?,
    })
}

/// Sets one of a device3d node's own properties; `apply_property` has checked the kind.
fn apply_device_property(node: &mut Node, prop: Property) -> Result<Property, OpError> {
    let d = node
        .device3d
        .get_or_insert_with(crate::device3d::Device3d::default);
    Ok(match prop {
        Property::Model(v) => Property::Model(replace(&mut d.model, v)),
        Property::RotateX(v) => {
            Property::RotateX(replace(&mut d.rotate_x, device3d::rotation("rotate_x", v)?))
        }
        Property::RotateY(v) => {
            Property::RotateY(replace(&mut d.rotate_y, device3d::rotation("rotate_y", v)?))
        }
        Property::RotateZ(v) => {
            Property::RotateZ(replace(&mut d.rotate_z, device3d::rotation("rotate_z", v)?))
        }
        Property::Zoom(v) => Property::Zoom(replace(&mut d.zoom, device3d::zoom(v)?)),
        Property::Fov(v) => Property::Fov(replace(&mut d.fov, device3d::fov(v)?)),
        Property::LightAngle(v) => {
            Property::LightAngle(replace(&mut d.light_angle, device3d::light_angle(v)?))
        }
        Property::Shadow(v) => {
            Property::Shadow(replace(&mut d.shadow, device3d::unit("shadow", v)?))
        }
        Property::Glare(v) => Property::Glare(replace(&mut d.glare, device3d::unit("glare", v)?)),
        Property::ScreenScroll(v) => {
            let v = v.map(|s| device3d::unit("screen_scroll", s)).transpose()?;
            Property::ScreenScroll(replace(&mut d.screen_scroll, v))
        }
        Property::ScreenFootage(v) => {
            if let Some(f) = &v {
                device3d::check_footage(f)?;
            }
            Property::ScreenFootage(replace(&mut d.footage, v))
        }
        other => {
            return Err(OpError::Internal(format!(
                "{} is not a device property",
                other.name()
            )));
        }
    })
}

/// A node to create. Ids are minted by the document, never supplied by the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewNode {
    pub kind: NodeKind,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
    #[serde(default)]
    pub visible: Option<bool>,
    #[serde(default)]
    pub opacity: Option<f64>,
    /// Transform and effects (`crate::transform`); omitted means none.
    #[serde(default)]
    pub rotation: Option<f64>,
    #[serde(default)]
    pub scale_x: Option<f64>,
    #[serde(default)]
    pub scale_y: Option<f64>,
    #[serde(default)]
    pub pivot_x: Option<f64>,
    #[serde(default)]
    pub pivot_y: Option<f64>,
    #[serde(default)]
    pub blur: Option<f64>,
    #[serde(default)]
    pub shadow: Option<Shadow>,
    #[serde(default)]
    pub clip: Option<bool>,
    #[serde(default)]
    pub depth: Option<f64>,
    #[serde(default)]
    pub fill: Option<Paint>,
    #[serde(default)]
    pub stroke: Option<Stroke>,
    #[serde(default)]
    pub corner_radius: Option<f64>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub font_size: Option<f64>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_weight: Option<f64>,
    #[serde(default)]
    pub text_align: Option<TextAlign>,
    #[serde(default)]
    pub line_height: Option<f64>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub image_fit: Option<ImageFit>,
    #[serde(default)]
    pub image_smoothing: Option<ImageSmoothing>,
    /// For a `device3d` node: the model and how it is turned and lit (defaults when omitted).
    #[serde(default)]
    pub device3d: Option<Device3d>,
    /// For a `path` node: SVG path data. Without a width and a height the box takes the size of
    /// the path's own bounds.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub view_box: Option<ViewBox>,
    #[serde(default)]
    pub fill_rule: Option<FillRule>,
    #[serde(default)]
    pub children: Vec<NewNode>,
}

impl NewNode {
    pub fn new(kind: NodeKind) -> NewNode {
        NewNode {
            kind,
            name: None,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            visible: None,
            opacity: None,
            rotation: None,
            scale_x: None,
            scale_y: None,
            pivot_x: None,
            pivot_y: None,
            blur: None,
            shadow: None,
            clip: None,
            depth: None,
            fill: None,
            stroke: None,
            corner_radius: None,
            text: None,
            font_size: None,
            font_family: None,
            font_weight: None,
            text_align: None,
            line_height: None,
            image: None,
            image_fit: None,
            image_smoothing: None,
            device3d: None,
            path: None,
            view_box: None,
            fill_rule: None,
            children: Vec::new(),
        }
    }

    fn count(&self) -> usize {
        1 + self.children.iter().map(NewNode::count).sum::<usize>()
    }

    /// The `set`s that give a fresh node the transform and effects asked for.
    fn transform_properties(&self) -> Vec<Property> {
        let numbers = [
            self.rotation.map(Property::Rotation),
            self.scale_x.map(Property::ScaleX),
            self.scale_y.map(Property::ScaleY),
            self.pivot_x.map(Property::PivotX),
            self.pivot_y.map(Property::PivotY),
            self.blur.map(Property::Blur),
            self.shadow.clone().map(|s| Property::DropShadow(Some(s))),
            self.clip.map(Property::Clip),
            self.depth.map(Property::Depth),
        ];
        numbers.into_iter().flatten().collect()
    }

    fn build(
        &self,
        next: &mut u64,
        tokens: &BTreeMap<String, String>,
        depth: usize,
    ) -> Result<Node, OpError> {
        if depth > limits::MAX_DEPTH {
            return Err(OpError::Limit(format!(
                "nesting deeper than {}",
                limits::MAX_DEPTH
            )));
        }
        if !self.kind.is_container() && !self.children.is_empty() {
            return Err(OpError::NoChildren {
                parent_kind: self.kind.as_str(),
            });
        }
        let id = NodeId::parse(&format!("n_{next}"))?;
        *next += 1;
        let name = self
            .name
            .clone()
            .unwrap_or_else(|| self.kind.as_str().to_string());
        let mut node = Node::blank(id, self.kind, String::new());
        let mut set = |prop: Property| apply_property(&mut node, prop, tokens).map(|_| ());
        set(Property::Name(name))?;
        set(Property::X(self.x))?;
        set(Property::Y(self.y))?;
        let (width, height) = crate::vector::box_size(self);
        if self.kind != NodeKind::Group || width != 0.0 {
            set(Property::Width(width))?;
        }
        if self.kind != NodeKind::Group || height != 0.0 {
            set(Property::Height(height))?;
        }
        if let Some(v) = self.visible {
            set(Property::Visible(v))?;
        }
        if let Some(v) = self.opacity {
            set(Property::Opacity(v))?;
        }
        for prop in self.transform_properties() {
            set(prop)?;
        }
        if self.fill.is_some() {
            set(Property::Fill(self.fill.clone()))?;
        }
        if self.stroke.is_some() {
            set(Property::Stroke(self.stroke.clone()))?;
        }
        if let Some(v) = self.corner_radius {
            set(Property::CornerRadius(v))?;
        }
        if let Some(v) = &self.text {
            set(Property::Text(v.clone()))?;
        }
        if let Some(v) = self.font_size {
            set(Property::FontSize(v))?;
        }
        if self.font_family.is_some() {
            set(Property::FontFamily(self.font_family.clone()))?;
        }
        if self.font_weight.is_some() {
            set(Property::FontWeight(self.font_weight))?;
        }
        if self.text_align.is_some() {
            set(Property::TextAlign(self.text_align))?;
        }
        if self.line_height.is_some() {
            set(Property::LineHeight(self.line_height))?;
        }
        if self.image.is_some() {
            set(Property::Image(self.image.clone()))?;
        }
        if self.image_fit.is_some() {
            set(Property::ImageFit(self.image_fit))?;
        }
        if self.image_smoothing.is_some() {
            set(Property::ImageSmoothing(self.image_smoothing))?;
        }
        if let Some(d) = &self.device3d {
            for prop in device_properties(d) {
                set(prop)?;
            }
        }
        for prop in crate::vector::path_properties(self)? {
            set(prop)?;
        }
        for child in &self.children {
            node.children.push(child.build(next, tokens, depth + 1)?);
        }
        Ok(node)
    }
}

/// The properties that make a fresh device3d look like `d`, each validated as it is set.
fn device_properties(d: &Device3d) -> [Property; 11] {
    [
        Property::Model(d.model),
        Property::RotateX(d.rotate_x),
        Property::RotateY(d.rotate_y),
        Property::RotateZ(d.rotate_z),
        Property::Zoom(d.zoom),
        Property::Fov(d.fov),
        Property::LightAngle(d.light_angle),
        Property::Shadow(d.shadow),
        Property::Glare(d.glare),
        Property::ScreenScroll(d.screen_scroll),
        Property::ScreenFootage(d.footage.clone()),
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    /// Add a node under `parent` at `index` (append when omitted).
    Create {
        parent: NodeId,
        #[serde(default)]
        index: Option<usize>,
        node: NewNode,
    },
    /// Set one property.
    Set { id: NodeId, prop: Property },
    /// Move a node to `x`, `y` in its parent's coordinates.
    Move { id: NodeId, x: f64, y: f64 },
    /// Move a node to another parent (append when `index` is omitted).
    Reparent {
        id: NodeId,
        parent: NodeId,
        #[serde(default)]
        index: Option<usize>,
    },
    /// Delete a node and its subtree.
    Delete { id: NodeId },
    /// Define or change a design token (a named color, `value` as `#rrggbb` or `#rrggbbaa`), or
    /// remove it with `value: null` (refused while a node still uses it). Nodes that use the
    /// token change color with it, so they are reported as touched.
    SetToken {
        name: String,
        #[serde(default)]
        value: Option<String>,
    },
    /// Set the translation of a text node in a language (`text: null` removes it). The node's own
    /// `text` is the default every language falls back to.
    SetLocalizedText {
        locale: String,
        id: NodeId,
        #[serde(default)]
        text: Option<String>,
    },
    /// Add an empty video: a timeline of clips shown at `fps`, rendered at `width` x `height`
    /// pixels (even numbers, 16 to 4096).
    CreateVideo {
        name: String,
        fps: u32,
        width: u32,
        height: u32,
    },
    /// Delete a video and its clips (the frames it showed are untouched).
    DeleteVideo { id: NodeId },
    /// Set one property of a video.
    SetVideo { id: NodeId, prop: VideoProperty },
    /// Add a clip showing `frame` for `duration_ms` to a video (appended when `index` is omitted).
    /// `transition` and `motion` default to a cut and no zoom.
    AddClip {
        video: NodeId,
        /// A design frame to show (give this or `footage`).
        #[serde(default)]
        frame: Option<NodeId>,
        /// A stretch of imported footage to play (give this or `frame`).
        #[serde(default)]
        footage: Option<crate::video::Footage>,
        duration_ms: u32,
        #[serde(default)]
        index: Option<usize>,
        #[serde(default)]
        transition: Option<Transition>,
        #[serde(default)]
        motion: Option<Motion>,
    },
    /// Remove a clip from a video.
    RemoveClip { video: NodeId, clip: NodeId },
    /// Move a clip to position `index` in its video.
    MoveClip {
        video: NodeId,
        clip: NodeId,
        index: usize,
    },
    /// Set one property of a clip.
    SetClip {
        video: NodeId,
        clip: NodeId,
        prop: ClipProperty,
    },
    /// Draw a design frame over the whole video from `start_ms` for `duration_ms`, fading in and
    /// out over `fade_ms` (a title, a lower third). A frame with no fill is transparent.
    AddOverlay {
        video: NodeId,
        frame: NodeId,
        start_ms: u64,
        duration_ms: u32,
        #[serde(default)]
        fade_ms: Option<u32>,
        /// How it arrives (default fade) and leaves (default fade).
        #[serde(default)]
        enter: Option<crate::video::Entrance>,
        #[serde(default)]
        exit: Option<crate::video::Entrance>,
        /// The easing of the arrival and exit (default ease_out).
        #[serde(default)]
        ease: Option<crate::Ease>,
    },
    /// Remove an overlay.
    RemoveOverlay { video: NodeId, overlay: NodeId },
    /// Set one property of an overlay.
    SetOverlay {
        video: NodeId,
        overlay: NodeId,
        prop: OverlayProperty,
    },
    /// Keyframe one number of a node over a video's time: `keys` at `at_ms` from the start of
    /// the video, each reached with its `ease`. The value holds before the first key and after
    /// the last, or cycles when `repeat` is `loop` or `pingpong`. Empty `keys` removes the
    /// animation. The camera's properties (`camera_x`, `camera_y`, `camera_zoom`,
    /// `camera_rotation`) take the video's own id as `node`.
    Animate {
        video: NodeId,
        node: NodeId,
        prop: AnimProp,
        keys: Vec<Key>,
        #[serde(default, skip_serializing_if = "Repeat::is_once")]
        repeat: Repeat,
        /// Adds the keys' values to the node's value instead of replacing it; see
        /// [`crate::animation::Animation::additive`].
        #[serde(default, skip_serializing_if = "crate::animation::not_additive")]
        additive: bool,
    },
    /// Put back an overlay. Written by the history; front ends cannot issue it.
    RestoreOverlay {
        video: NodeId,
        index: usize,
        overlay: Overlay,
    },
    /// Put back a video. Written by the history; front ends cannot issue it.
    RestoreVideo { video: Video },
    /// Put back a clip. Written by the history; front ends cannot issue it.
    RestoreClip {
        video: NodeId,
        index: usize,
        clip: Clip,
    },
    /// Put back an existing subtree with its original ids. Written by the history when it
    /// resolves a `Create` or inverts a `Delete`; front ends cannot issue it.
    Restore {
        parent: NodeId,
        index: usize,
        node: Node,
    },
    /// Declare a template variable: a typed slot bound to node properties (`Document.variables`).
    /// Bound nodes take its default at once.
    DeclareVariable {
        variable: crate::variables::NewVariable,
    },
    /// Remove a variable; the bound nodes keep the values they show now.
    RemoveVariable { id: NodeId },
    /// Change one property of a variable (label, description, portrays, default, enum_values, binds).
    SetVariable {
        id: NodeId,
        prop: crate::variables::VariableProperty,
    },
    /// Set a variable's value: every bound node property is rewritten through `set`.
    SetVariableValue {
        id: NodeId,
        value: crate::variables::VarValue,
    },
    /// Put back a variable and the node values it had. Written by the history; front ends
    /// cannot issue it.
    RestoreVariable {
        id: NodeId,
        index: usize,
        variable: Option<crate::variables::Variable>,
        sets: Vec<crate::variables::Assignment>,
    },
}

impl Operation {
    pub fn name(&self) -> &'static str {
        match self {
            Operation::Create { .. } => "create",
            Operation::Set { .. } => "set",
            Operation::Move { .. } => "move",
            Operation::Reparent { .. } => "reparent",
            Operation::Delete { .. } => "delete",
            Operation::SetToken { .. } => "set_token",
            Operation::SetLocalizedText { .. } => "set_localized_text",
            Operation::CreateVideo { .. } => "create_video",
            Operation::DeleteVideo { .. } => "delete_video",
            Operation::SetVideo { .. } => "set_video",
            Operation::AddClip { .. } => "add_clip",
            Operation::RemoveClip { .. } => "remove_clip",
            Operation::MoveClip { .. } => "move_clip",
            Operation::SetClip { .. } => "set_clip",
            Operation::AddOverlay { .. } => "add_overlay",
            Operation::RemoveOverlay { .. } => "remove_overlay",
            Operation::SetOverlay { .. } => "set_overlay",
            Operation::Animate { .. } => "animate",
            Operation::RestoreOverlay { .. } => "restore_overlay",
            Operation::RestoreVideo { .. } => "restore_video",
            Operation::RestoreClip { .. } => "restore_clip",
            Operation::Restore { .. } => "restore",
            Operation::DeclareVariable { .. } => "declare_variable",
            Operation::RemoveVariable { .. } => "remove_variable",
            Operation::SetVariable { .. } => "set_variable",
            Operation::SetVariableValue { .. } => "set_variable_value",
            Operation::RestoreVariable { .. } => "restore_variable",
        }
    }
}

/// An applied operation: what ran (ids resolved), how to undo it, and which nodes it touched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedOp {
    pub forward: Operation,
    pub inverse: Operation,
    pub touched: Vec<NodeId>,
}

fn node_loc(doc: &Document, id: &NodeId) -> Result<crate::model::Location, OpError> {
    doc.locate(id)
        .ok_or_else(|| OpError::NoSuchNode(id.to_string()))
}

fn immutable_if_root(doc: &Document, loc: &crate::model::Location) -> Result<(), OpError> {
    if loc.path.is_empty() {
        let page = doc.pages.get(loc.page).map(|p| p.id.to_string());
        return Err(OpError::RootImmutable(page.unwrap_or_default()));
    }
    Ok(())
}

fn container_loc(doc: &Document, id: &NodeId) -> Result<crate::model::Location, OpError> {
    let loc = node_loc(doc, id)?;
    let kind = doc
        .node_at(&loc)
        .map(|n| n.kind)
        .ok_or_else(|| OpError::NoSuchNode(id.to_string()))?;
    if !kind.is_container() {
        return Err(OpError::NoChildren {
            parent_kind: kind.as_str(),
        });
    }
    Ok(loc)
}

fn check_insert_index(parent: &NodeId, index: usize, len: usize) -> Result<(), OpError> {
    if index > len {
        return Err(OpError::BadIndex {
            parent: parent.to_string(),
            index,
            len,
        });
    }
    Ok(())
}

fn check_count(doc: &Document, added: usize) -> Result<(), OpError> {
    if doc.node_count() + added > limits::MAX_NODES {
        return Err(OpError::Limit(format!(
            "at most {} nodes",
            limits::MAX_NODES
        )));
    }
    Ok(())
}

fn insert(
    doc: &mut Document,
    parent: &NodeId,
    index: Option<usize>,
    node: Node,
) -> Result<usize, OpError> {
    let loc = container_loc(doc, parent)?;
    let target = doc
        .node_at_mut(&loc)
        .ok_or_else(|| OpError::NoSuchNode(parent.to_string()))?;
    let index = index.unwrap_or(target.children.len());
    check_insert_index(parent, index, target.children.len())?;
    target.children.insert(index, node);
    if let Some(inserted) = doc.node_at(&loc).and_then(|p| p.children.get(index)) {
        doc.index.note_inserted(parent, index, inserted);
    }
    Ok(index)
}

/// Validates and applies one operation. `allow_internal` lets the history issue `Restore`.
pub fn apply(
    doc: &mut Document,
    op: &Operation,
    allow_internal: bool,
) -> Result<AppliedOp, OpError> {
    match op {
        Operation::Create {
            parent,
            index,
            node,
        } => {
            let ploc = container_loc(doc, parent)?;
            check_count(doc, node.count())?;
            let mut next = doc.next_id;
            let built = node.build(&mut next, &doc.tokens, ploc.path.len() + 1)?;
            let at = insert(doc, parent, *index, built.clone())?;
            doc.next_id = next;
            let mut touched = Vec::new();
            built.ids(&mut touched);
            Ok(AppliedOp {
                inverse: Operation::Delete {
                    id: built.id.clone(),
                },
                forward: Operation::Restore {
                    parent: parent.clone(),
                    index: at,
                    node: built,
                },
                touched,
            })
        }
        Operation::Restore {
            parent,
            index,
            node,
        } => {
            if !allow_internal {
                return Err(OpError::InternalOnly("restore"));
            }
            let ploc = container_loc(doc, parent)?;
            check_count(doc, node.count())?;
            if ploc.path.len() + 1 + node.height_below() > limits::MAX_DEPTH {
                return Err(OpError::Limit(format!(
                    "nesting deeper than {}",
                    limits::MAX_DEPTH
                )));
            }
            let mut ids = Vec::new();
            node.ids(&mut ids);
            for id in &ids {
                if doc.locate(id).is_some() {
                    return Err(OpError::DuplicateId(id.to_string()));
                }
            }
            validate_subtree(node, &doc.tokens)?;
            insert(doc, parent, Some(*index), node.clone())?;
            Ok(AppliedOp {
                forward: op.clone(),
                inverse: Operation::Delete {
                    id: node.id.clone(),
                },
                touched: ids,
            })
        }
        Operation::Set { id, prop } => {
            if let Property::PathMorph(morph) = prop {
                crate::morph::check_progress_keys(doc, id, morph.as_ref())?;
            }
            let loc = node_loc(doc, id)?;
            immutable_if_root(doc, &loc)?;
            let tokens = doc.tokens.clone();
            let node = doc
                .node_at_mut(&loc)
                .ok_or_else(|| OpError::NoSuchNode(id.to_string()))?;
            let old = apply_property(node, prop.clone(), &tokens)?;
            let resolved = current_value(node, prop);
            Ok(AppliedOp {
                forward: Operation::Set {
                    id: id.clone(),
                    prop: resolved,
                },
                inverse: Operation::Set {
                    id: id.clone(),
                    prop: old,
                },
                touched: vec![id.clone()],
            })
        }
        Operation::Move { id, x, y } => {
            let loc = node_loc(doc, id)?;
            immutable_if_root(doc, &loc)?;
            let x = finite_in("x", *x, -limits::MAX_COORD, limits::MAX_COORD)?;
            let y = finite_in("y", *y, -limits::MAX_COORD, limits::MAX_COORD)?;
            let node = doc
                .node_at_mut(&loc)
                .ok_or_else(|| OpError::NoSuchNode(id.to_string()))?;
            let (old_x, old_y) = (replace(&mut node.x, x), replace(&mut node.y, y));
            Ok(AppliedOp {
                forward: Operation::Move {
                    id: id.clone(),
                    x,
                    y,
                },
                inverse: Operation::Move {
                    id: id.clone(),
                    x: old_x,
                    y: old_y,
                },
                touched: vec![id.clone()],
            })
        }
        Operation::CreateVideo { .. }
        | Operation::DeleteVideo { .. }
        | Operation::SetVideo { .. }
        | Operation::AddClip { .. }
        | Operation::RemoveClip { .. }
        | Operation::MoveClip { .. }
        | Operation::SetClip { .. }
        | Operation::RestoreVideo { .. }
        | Operation::RestoreClip { .. }
        | Operation::AddOverlay { .. }
        | Operation::RemoveOverlay { .. }
        | Operation::SetOverlay { .. }
        | Operation::RestoreOverlay { .. } => crate::video::apply(doc, op, allow_internal),
        Operation::Animate {
            video,
            node,
            prop,
            keys,
            repeat,
            additive,
        } => crate::animation::apply(
            doc,
            crate::animation::Animate {
                video,
                node,
                prop: *prop,
                keys,
                repeat: *repeat,
                additive: *additive,
            },
            allow_internal,
        ),
        Operation::SetToken { name, value } => set_token(doc, name, value.as_deref()),
        Operation::SetLocalizedText { locale, id, text } => {
            let old = doc.set_locale_text(locale, id, text.as_deref())?;
            Ok(AppliedOp {
                forward: op.clone(),
                inverse: Operation::SetLocalizedText {
                    locale: locale.clone(),
                    id: id.clone(),
                    text: old,
                },
                touched: vec![id.clone()],
            })
        }
        Operation::DeclareVariable { .. }
        | Operation::RemoveVariable { .. }
        | Operation::SetVariable { .. }
        | Operation::SetVariableValue { .. }
        | Operation::RestoreVariable { .. } => crate::variables::apply(doc, op, allow_internal),
        Operation::Reparent { id, parent, index } => reparent(doc, id, parent, *index),
        Operation::Delete { id } => {
            let loc = node_loc(doc, id)?;
            immutable_if_root(doc, &loc)?;
            if let Some(subtree) = doc.node(id) {
                let mut ids = Vec::new();
                subtree.ids(&mut ids);
                if let Some((video, clip, frame)) = doc.video_using(&ids) {
                    return Err(OpError::FrameInUse {
                        frame: frame.map(|f| f.to_string()).unwrap_or_default(),
                        clip: clip.to_string(),
                        video: video.id.to_string(),
                    });
                }
                if let Some((video, node, prop)) = doc.animation_of(&ids) {
                    return Err(OpError::NodeAnimated {
                        node: node.to_string(),
                        prop,
                        video: video.to_string(),
                    });
                }
            }
            let (&index, parent_path) = loc
                .path
                .split_last()
                .ok_or_else(|| OpError::Internal("empty path after root check".to_string()))?;
            let parent_loc = crate::model::Location {
                page: loc.page,
                path: parent_path.to_vec(),
            };
            let parent_node = doc
                .node_at_mut(&parent_loc)
                .ok_or_else(|| OpError::Internal("parent vanished".to_string()))?;
            let parent_id = parent_node.id.clone();
            let removed = parent_node.children.remove(index);
            doc.index.note_removed(&removed);
            let mut touched = Vec::new();
            removed.ids(&mut touched);
            Ok(AppliedOp {
                forward: op.clone(),
                inverse: Operation::Restore {
                    parent: parent_id,
                    index,
                    node: removed,
                },
                touched,
            })
        }
    }
}

fn set_token(doc: &mut Document, name: &str, value: Option<&str>) -> Result<AppliedOp, OpError> {
    let old = doc.tokens.get(name).cloned();
    let users = doc.token_users(name);
    match value {
        Some(v) => doc.set_token(name, v)?,
        None => {
            if !crate::paint::valid_token_name(name) || old.is_none() {
                return Err(OpError::UnknownToken(name.to_string()));
            }
            if !users.is_empty() {
                return Err(OpError::TokenInUse {
                    name: name.to_string(),
                    count: users.len(),
                });
            }
            doc.tokens.remove(name);
        }
    }
    Ok(AppliedOp {
        forward: Operation::SetToken {
            name: name.to_string(),
            value: doc.tokens.get(name).cloned(),
        },
        inverse: Operation::SetToken {
            name: name.to_string(),
            value: old,
        },
        touched: users,
    })
}

/// The value a `Set` actually stored (rounded), so redo reproduces the exact bytes.
fn current_value(node: &Node, prop: &Property) -> Property {
    match prop {
        Property::Name(_) => Property::Name(node.name.clone()),
        Property::X(_) => Property::X(node.x),
        Property::Y(_) => Property::Y(node.y),
        Property::Width(_) => Property::Width(node.width),
        Property::Height(_) => Property::Height(node.height),
        Property::Visible(_) => Property::Visible(node.visible),
        Property::Opacity(_) => Property::Opacity(node.opacity),
        transform if transform.is_transform() => crate::transform::current(node, transform),
        Property::Fill(_) => Property::Fill(node.fill.clone()),
        Property::Stroke(_) => Property::Stroke(node.stroke.clone()),
        Property::CornerRadius(_) => Property::CornerRadius(node.corner_radius),
        Property::Text(_) => Property::Text(node.text.clone().unwrap_or_default()),
        Property::FontSize(_) => Property::FontSize(node.font_size.unwrap_or(16.0)),
        Property::FontFamily(_) => Property::FontFamily(node.font_family.clone()),
        Property::FontWeight(_) => Property::FontWeight(node.font_weight),
        Property::TextAlign(_) => Property::TextAlign(node.text_align),
        Property::LineHeight(_) => Property::LineHeight(node.line_height),
        Property::Image(_) => Property::Image(node.image.clone()),
        Property::ImageFit(_) => Property::ImageFit(node.image_fit),
        Property::ImageSmoothing(_) => Property::ImageSmoothing(node.image_smoothing),
        Property::Grade(_) => Property::Grade(node.grade),
        Property::GradeParam { param, .. } => Property::GradeParam {
            param: *param,
            value: param.get(&node.grade.unwrap_or_default()),
        },
        Property::Path(_) => Property::Path(node.path.clone().unwrap_or_default()),
        Property::ViewBox(_) => Property::ViewBox(node.view_box),
        Property::FillRule(_) => Property::FillRule(node.fill_rule),
        Property::PathMorph(_) => Property::PathMorph(node.morph.clone()),
        Property::StrokeWidth(_) | Property::TrimStart(_) | Property::TrimEnd(_) => {
            crate::vector::current_stroke_value(node, prop)
        }
        device_prop => {
            let d = node.device3d.clone().unwrap_or_default();
            match device_prop {
                Property::Model(_) => Property::Model(d.model),
                Property::RotateX(_) => Property::RotateX(d.rotate_x),
                Property::RotateY(_) => Property::RotateY(d.rotate_y),
                Property::RotateZ(_) => Property::RotateZ(d.rotate_z),
                Property::Zoom(_) => Property::Zoom(d.zoom),
                Property::Fov(_) => Property::Fov(d.fov),
                Property::LightAngle(_) => Property::LightAngle(d.light_angle),
                Property::Shadow(_) => Property::Shadow(d.shadow),
                Property::Glare(_) => Property::Glare(d.glare),
                Property::ScreenScroll(_) => Property::ScreenScroll(d.screen_scroll),
                Property::ScreenFootage(_) => Property::ScreenFootage(d.footage),
                other => other.clone(),
            }
        }
    }
}

/// Sets `prop` on `node` as `Set` would, for the animation module (validated the same way).
pub(crate) fn set_property(
    node: &mut Node,
    prop: Property,
    tokens: &BTreeMap<String, String>,
) -> Result<(), OpError> {
    apply_property(node, prop, tokens).map(|_| ())
}

fn validate_subtree(node: &Node, tokens: &BTreeMap<String, String>) -> Result<(), OpError> {
    validate_node_fields(node, tokens)?;
    if !node.kind.is_container() && !node.children.is_empty() {
        return Err(OpError::NoChildren {
            parent_kind: node.kind.as_str(),
        });
    }
    node.children
        .iter()
        .try_for_each(|c| validate_subtree(c, tokens))
}

fn reparent(
    doc: &mut Document,
    id: &NodeId,
    parent: &NodeId,
    index: Option<usize>,
) -> Result<AppliedOp, OpError> {
    let loc = node_loc(doc, id)?;
    immutable_if_root(doc, &loc)?;
    let target = container_loc(doc, parent)?;
    if target.page == loc.page && target.path.starts_with(&loc.path) {
        return Err(OpError::Cycle {
            node: id.to_string(),
            target: parent.to_string(),
        });
    }
    let node_height = doc
        .node_at(&loc)
        .map(Node::height_below)
        .ok_or_else(|| OpError::NoSuchNode(id.to_string()))?;
    if target.path.len() + 1 + node_height > limits::MAX_DEPTH {
        return Err(OpError::Limit(format!(
            "nesting deeper than {}",
            limits::MAX_DEPTH
        )));
    }
    let (&old_index, old_parent_path) = loc
        .path
        .split_last()
        .ok_or_else(|| OpError::Internal("empty path after root check".to_string()))?;
    let old_parent_loc = crate::model::Location {
        page: loc.page,
        path: old_parent_path.to_vec(),
    };
    let old_parent_id = doc
        .node_at(&old_parent_loc)
        .map(|n| n.id.clone())
        .ok_or_else(|| OpError::Internal("parent vanished".to_string()))?;
    let target_len = doc.node_at(&target).map_or(0, |n| n.children.len());
    let len_after_removal = if &old_parent_id == parent {
        target_len - 1
    } else {
        target_len
    };
    let new_index = index.unwrap_or(len_after_removal);
    check_insert_index(parent, new_index, len_after_removal)?;

    let removed = doc
        .node_at_mut(&old_parent_loc)
        .map(|p| p.children.remove(old_index))
        .ok_or_else(|| OpError::Internal("parent vanished".to_string()))?;
    insert(doc, parent, Some(new_index), removed)?;
    Ok(AppliedOp {
        forward: Operation::Reparent {
            id: id.clone(),
            parent: parent.clone(),
            index: Some(new_index),
        },
        inverse: Operation::Reparent {
            id: id.clone(),
            parent: old_parent_id.clone(),
            index: Some(old_index),
        },
        touched: vec![id.clone(), old_parent_id, parent.clone()],
    })
}

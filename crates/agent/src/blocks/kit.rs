//! What every block draws with: the target box, the palette (brand tokens first), text that is
//! measured and fitted before it is placed, and the moves a block asks for. A block never sees
//! ids: it names its parts, and the plan resolves the names once the nodes exist.

use std::collections::BTreeMap;

use od_core::animation::{AnimProp, Preset};
use od_core::geometry::Rect;
use od_core::{
    Document, Ease, ImageFit, NewNode, Node, NodeId, NodeKind, Paint, Rgba, Stroke, TextAlign,
};
use serde_json::json;

use super::BlockError;
use super::files::Asset;
use crate::review::contrast;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Background,
    Foreground,
    Accent,
    Muted,
}

impl Role {
    pub const ALL: [Role; 4] = [
        Role::Background,
        Role::Foreground,
        Role::Accent,
        Role::Muted,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Role::Background => "background",
            Role::Foreground => "foreground",
            Role::Accent => "accent",
            Role::Muted => "muted",
        }
    }

    /// Token names an existing brand is likely to use, most specific first.
    fn candidates(self) -> &'static [&'static str] {
        match self {
            Role::Background => &[
                "brand.background",
                "background",
                "brand.bg",
                "bg",
                "surface",
            ],
            Role::Foreground => &[
                "brand.text",
                "text",
                "brand.foreground",
                "foreground",
                "fg",
                "ink",
            ],
            Role::Accent => &[
                "brand.accent",
                "accent",
                "brand.primary",
                "primary",
                "brand.gold",
            ],
            Role::Muted => &["brand.muted", "muted", "brand.secondary", "secondary"],
        }
    }

    fn default_hex(self) -> &'static str {
        match self {
            Role::Background => "#0d0f14",
            Role::Foreground => "#f5f6f8",
            Role::Accent => "#7c6cff",
            Role::Muted => "#9aa3b2",
        }
    }
}

/// Where a move lands: one of the block's parts, the whole block, or a node that already exists.
#[derive(Debug, Clone)]
pub enum Target {
    Part(String),
    Root,
    Node(NodeId),
}

#[derive(Debug, Clone)]
pub enum Move {
    /// A named move from `od_core::animation`, measured from the node's designed pose.
    Preset {
        target: Target,
        preset: Preset,
        at: u64,
        duration: u64,
        strength: f64,
        ease: Option<Ease>,
    },
    /// Keys the presets do not cover (growing a bar, a cursor's path); the last key is the
    /// designed value.
    Keys {
        target: Target,
        prop: AnimProp,
        keys: Vec<(u64, f64, Ease)>,
    },
}

impl Move {
    /// When the move ends, from the start of the block.
    pub fn end(&self) -> u64 {
        match self {
            Move::Preset { at, duration, .. } => at + duration,
            Move::Keys { keys, .. } => keys.last().map_or(0, |k| k.0),
        }
    }
}

/// Text style: size, weight, color, alignment and line height as a multiple of the size.
#[derive(Debug, Clone)]
pub struct Ty {
    pub size: f64,
    pub weight: f64,
    pub color: Paint,
    pub align: TextAlign,
    pub line: f64,
}

impl Ty {
    pub fn of(size: f64, weight: f64, color: Paint) -> Ty {
        Ty {
            size,
            weight,
            color,
            align: TextAlign::Left,
            line: 1.2,
        }
    }

    pub fn align(mut self, align: TextAlign) -> Ty {
        self.align = align;
        self
    }

    pub fn line(mut self, line: f64) -> Ty {
        self.line = line;
        self
    }

    pub fn size(mut self, size: f64) -> Ty {
        self.size = size;
        self
    }
}

/// Style additions for a node being built.
pub trait Style {
    fn radius(self, r: f64) -> Self;
    fn alpha(self, a: f64) -> Self;
    fn outline(self, paint: Paint, width: f64) -> Self;
    fn fit(self, fit: ImageFit) -> Self;
}

impl Style for NewNode {
    fn radius(mut self, r: f64) -> Self {
        self.corner_radius = Some(r);
        self
    }

    fn alpha(mut self, a: f64) -> Self {
        self.opacity = Some(a);
        self
    }

    fn outline(mut self, paint: Paint, width: f64) -> Self {
        self.stroke = Some(Stroke { paint, width });
        self
    }

    fn fit(mut self, fit: ImageFit) -> Self {
        self.image_fit = Some(fit);
        self
    }
}

fn shape(kind: NodeKind, r: Rect) -> NewNode {
    let mut n = NewNode::new(kind);
    n.x = r.x;
    n.y = r.y;
    n.width = r.width;
    n.height = r.height;
    n
}

pub fn rect(r: Rect, fill: Paint) -> NewNode {
    let mut n = shape(NodeKind::Rect, r);
    n.fill = Some(fill);
    n
}

pub fn ellipse(r: Rect, fill: Paint) -> NewNode {
    let mut n = shape(NodeKind::Ellipse, r);
    n.fill = Some(fill);
    n
}

/// An ellipse that is only an outline.
pub fn ring(r: Rect, paint: Paint, width: f64) -> NewNode {
    shape(NodeKind::Ellipse, r).outline(paint, width)
}

/// A rectangle that is only an outline.
pub fn frame_line(r: Rect, paint: Paint, width: f64) -> NewNode {
    shape(NodeKind::Rect, r).outline(paint, width)
}

pub fn picture(r: Rect, file: String) -> NewNode {
    let mut n = shape(NodeKind::Image, r);
    n.image = Some(file);
    n
}

pub fn at(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

pub fn center(r: Rect) -> (f64, f64) {
    (r.x + r.width / 2.0, r.y + r.height / 2.0)
}

pub fn inset(r: Rect, by: f64) -> Rect {
    at(
        r.x + by,
        r.y + by,
        (r.width - 2.0 * by).max(0.0),
        (r.height - 2.0 * by).max(0.0),
    )
}

/// `a` mixed `t` of the way toward `b`, as `#rrggbb`.
pub fn mix(a: Rgba, b: Rgba, t: f64) -> String {
    let m = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
    Rgba {
        r: m(a.r, b.r),
        g: m(a.g, b.g),
        b: m(a.b, b.b),
        a: 255,
    }
    .to_hex()
}

/// One row of a vertical stack: fitted text, or empty space.
pub enum Row {
    Text {
        part: &'static str,
        text: String,
        ty: Ty,
        h: f64,
    },
    Gap(f64),
}

impl Row {
    pub fn height(&self) -> f64 {
        match self {
            Row::Text { h, .. } => *h,
            Row::Gap(h) => *h,
        }
    }
}

pub fn align_of(name: &str) -> TextAlign {
    match name {
        "center" => TextAlign::Center,
        "right" => TextAlign::Right,
        _ => TextAlign::Left,
    }
}

pub struct Kit<'a> {
    pub doc: &'a Document,
    pub frame: Rect,
    /// The box the block lays itself out in.
    pub b: Rect,
    /// The shorter side of the frame: sizes are shares of it so a block scales with its target.
    pub short: f64,
    /// Distance text keeps from the frame's edges.
    pub margin: f64,
    pub font: Option<String>,
    pub root: String,
    explicit: BTreeMap<Role, Paint>,
    tokens: BTreeMap<String, String>,
    created_tokens: BTreeMap<String, String>,
    stack: Vec<(String, Vec<NewNode>)>,
    pub moves: Vec<Move>,
    pub assets: Vec<Asset>,
}

pub struct Built {
    pub root: NewNode,
    pub moves: Vec<Move>,
    pub assets: Vec<Asset>,
    pub tokens: BTreeMap<String, String>,
}

impl<'a> Kit<'a> {
    pub fn new(
        doc: &'a Document,
        frame: Rect,
        b: Rect,
        root: String,
        font: Option<String>,
        explicit: BTreeMap<Role, Paint>,
    ) -> Kit<'a> {
        let short = frame.width.min(frame.height);
        Kit {
            doc,
            frame,
            b,
            short,
            margin: (short * 0.06).round().max(8.0),
            font,
            explicit,
            tokens: doc.tokens.clone(),
            created_tokens: BTreeMap::new(),
            stack: vec![(root.clone(), Vec::new())],
            root,
            moves: Vec::new(),
            assets: Vec::new(),
        }
    }

    /// `pct` percent of the frame's shorter side.
    pub fn s(&self, pct: f64) -> f64 {
        self.short * pct / 100.0
    }

    /// The frame inset by the margin.
    pub fn safe(&self) -> Rect {
        inset(self.frame, self.margin)
    }

    pub fn portrait(&self) -> bool {
        self.frame.height > self.frame.width * 1.1
    }

    pub fn paint(&mut self, role: Role) -> Paint {
        if let Some(p) = self.explicit.get(&role) {
            return p.clone();
        }
        if let Some(name) = role
            .candidates()
            .iter()
            .find(|n| self.tokens.contains_key(**n))
        {
            return Paint::Token((*name).to_string());
        }
        let name = format!("block.{}", role.key());
        if !self.tokens.contains_key(&name) {
            let hex = role.default_hex().to_string();
            self.tokens.insert(name.clone(), hex.clone());
            self.created_tokens.insert(name.clone(), hex);
        }
        Paint::Token(name)
    }

    pub fn rgba(&self, p: &Paint) -> Rgba {
        p.resolve(&self.tokens).unwrap_or(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        })
    }

    /// The role's color as `#rrggbb`, for pictures that carry their color.
    pub fn hex(&mut self, role: Role) -> String {
        let p = self.paint(role);
        let c = self.rgba(&p);
        Rgba { a: 255, ..c }.to_hex()
    }

    /// The foreground or background color, whichever reads better on `fill`.
    pub fn ink_on(&mut self, fill: &Paint) -> Paint {
        let under = self.rgba(fill);
        let (fg, bg) = (self.paint(Role::Foreground), self.paint(Role::Background));
        let best = [fg, bg]
            .into_iter()
            .max_by(|a, b| contrast(self.rgba(a), under).total_cmp(&contrast(self.rgba(b), under)))
            .unwrap_or(Paint::Color(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }));
        if contrast(self.rgba(&best), under) >= 4.5 {
            return best;
        }
        let white = Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        let black = Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        Paint::Color(if contrast(white, under) > contrast(black, under) {
            white
        } else {
            black
        })
    }

    pub fn ty(&mut self, size: f64, weight: f64, role: Role) -> Ty {
        Ty {
            size,
            weight,
            color: self.paint(role),
            align: TextAlign::Left,
            line: 1.2,
        }
    }

    pub fn full(&self, part: &str) -> String {
        format!("{}/{part}", self.root)
    }

    pub fn add(&mut self, part: &str, mut node: NewNode) {
        node.name = Some(self.full(part));
        if let Some((_, nodes)) = self.stack.last_mut() {
            nodes.push(node);
        }
    }

    /// Starts a group of parts; they move together.
    pub fn begin(&mut self, part: &str) {
        let name = self.full(part);
        self.stack.push((name, Vec::new()));
    }

    pub fn end(&mut self) {
        if self.stack.len() < 2 {
            return;
        }
        if let Some((name, children)) = self.stack.pop() {
            let mut g = NewNode::new(NodeKind::Group);
            g.name = Some(name);
            g.children = children;
            if let Some((_, nodes)) = self.stack.last_mut() {
                nodes.push(g);
            }
        }
    }

    pub fn asset(&mut self, asset: Asset) -> String {
        let file = asset.file();
        if !self.assets.contains(&asset) {
            self.assets.push(asset);
        }
        file
    }

    pub fn icon(&mut self, name: &str, role: Role, size: f64) -> String {
        let color = self.hex(role);
        self.asset(Asset::icon(name, color, size))
    }

    fn probe(&self, text: &str, size: f64, weight: f64, line: f64, wrap: f64) -> Option<Node> {
        let mut v = json!({
            "id": "probe", "kind": "text", "name": "probe", "x": 0, "y": 0,
            "width": wrap, "height": 0, "text": text, "font_size": size,
            "font_weight": weight, "line_height": line,
        });
        if let (Some(f), Some(o)) = (&self.font, v.as_object_mut()) {
            o.insert("font_family".into(), json!(f));
        }
        serde_json::from_value(v).ok()
    }

    /// `(width, height)` the text takes: wrapped at `wrap`, or on one line when `wrap` is 0.
    pub fn measure(&self, text: &str, ty: &Ty, wrap: f64) -> (f64, f64) {
        self.probe(text, ty.size, ty.weight, ty.line, wrap)
            .and_then(|n| od_render::measure_text(&n))
            .map_or((0.0, 0.0), |(w, h)| (f64::from(w), f64::from(h)))
    }

    /// The width of the text on one line.
    pub fn width_of(&self, text: &str, ty: &Ty) -> f64 {
        self.measure(text, ty, 0.0).0.ceil() + 2.0
    }

    /// The largest size from `ty.size` down to `floor` at which `text` wrapped at `width` is
    /// no taller than `max_h` and no more than `max_lines` lines.
    pub fn fit_size(
        &self,
        text: &str,
        ty: &Ty,
        width: f64,
        max_h: f64,
        max_lines: u32,
        floor: f64,
    ) -> Result<f64, BlockError> {
        let mut size = ty.size;
        loop {
            let probe = Ty { size, ..ty.clone() };
            let (w, h) = self.measure(text, &probe, width);
            let lines = (h / (size * ty.line)).round() as u32;
            let widest_word = text
                .split_whitespace()
                .map(|word| self.measure(word, &probe, 0.0).0)
                .fold(0.0_f64, f64::max);
            if h <= max_h + 0.5 && lines <= max_lines && w <= width + 0.5 && widest_word <= width {
                return Ok(size);
            }
            if size <= floor {
                return Err(BlockError::Fit(format!(
                    "{text:?} does not fit {width:.0} x {max_h:.0} px in {max_lines} line(s) even at {floor:.0} px: shorten it or give the block a larger box"
                )));
            }
            size = (size * 0.94).max(floor);
        }
    }

    /// Adds text wrapped at `w`, as tall as it lays out; returns the rectangle it takes.
    pub fn text(&mut self, part: &str, text: &str, ty: &Ty, x: f64, y: f64, w: f64) -> Rect {
        let (_, h) = self.measure(text, ty, w);
        let h = h.ceil().max(1.0);
        let mut n = NewNode::new(NodeKind::Text);
        n.x = x;
        n.y = y;
        n.width = w;
        n.height = h;
        n.text = Some(text.to_string());
        n.font_size = Some(ty.size);
        n.font_weight = Some(ty.weight);
        n.font_family = self.font.clone();
        n.text_align = Some(ty.align);
        n.line_height = Some(ty.line);
        n.fill = Some(ty.color.clone());
        self.add(part, n);
        at(x, y, w, h)
    }

    /// The padded area inside the box, for blocks that fill their box with a panel.
    pub fn content(&self) -> Rect {
        let pad = self.margin.min(0.1 * self.b.width.min(self.b.height));
        inset(self.b, pad)
    }

    /// `ty` shrunk until `text` fits `w` x `max_h` in `lines` lines, with the height it takes.
    pub fn fit(
        &self,
        text: &str,
        ty: &Ty,
        w: f64,
        max_h: f64,
        lines: u32,
    ) -> Result<(Ty, f64), BlockError> {
        let floor = (ty.size * 0.55).max(self.short * 0.023).min(ty.size);
        let size = self.fit_size(text, ty, w, max_h, lines, floor)?;
        let ty = Ty { size, ..ty.clone() };
        let h = self.measure(text, &ty, w).1.ceil().max(1.0);
        Ok((ty, h))
    }

    /// A fitted text row for [`Kit::stack`].
    pub fn row(
        &self,
        part: &'static str,
        text: &str,
        ty: &Ty,
        w: f64,
        max_h: f64,
        lines: u32,
    ) -> Result<Row, BlockError> {
        let (ty, h) = self.fit(text, ty, w, max_h, lines)?;
        Ok(Row::Text {
            part,
            text: text.to_string(),
            ty,
            h,
        })
    }

    /// Lays rows out top to bottom inside `area` (centered vertically when `middle`), text
    /// wrapped at the area's width. Returns each row's rectangle.
    pub fn stack(&mut self, rows: Vec<Row>, area: Rect, middle: bool) -> Vec<Rect> {
        let total: f64 = rows.iter().map(Row::height).sum();
        let mut y = if middle {
            area.y + ((area.height - total) / 2.0).max(0.0)
        } else {
            area.y
        };
        let mut out = Vec::new();
        for row in rows {
            let h = row.height();
            if let Row::Text { part, text, ty, .. } = &row {
                self.text(part, text, ty, area.x, y, area.width);
            }
            out.push(at(area.x, y, area.width, h));
            y += h;
        }
        out
    }

    /// A rectangle that opens from `0` to `w` wide (from its middle when `centered`).
    pub fn grow_x(
        &mut self,
        part: &str,
        x: f64,
        w: f64,
        centered: bool,
        start: u64,
        duration: u64,
    ) {
        let t = self.part(part);
        self.keys(
            t.clone(),
            AnimProp::Width,
            &[
                (start, 0.0, Ease::Linear),
                (start + duration, w, Ease::EaseOut),
            ],
        );
        if centered {
            self.keys(
                t,
                AnimProp::X,
                &[
                    (start, x + w / 2.0, Ease::Linear),
                    (start + duration, x, Ease::EaseOut),
                ],
            );
        }
    }

    pub fn keys(&mut self, target: Target, prop: AnimProp, keys: &[(u64, f64, Ease)]) {
        self.moves.push(Move::Keys {
            target,
            prop,
            keys: keys.to_vec(),
        });
    }

    pub fn part(&self, part: &str) -> Target {
        Target::Part(self.full(part))
    }

    pub fn preset(&mut self, part: &str, preset: Preset, at: u64, duration: u64) {
        self.moves.push(Move::Preset {
            target: self.part(part),
            preset,
            at,
            duration,
            strength: 1.0,
            ease: None,
        });
    }

    /// Rises from slightly below while fading in.
    pub fn rise(&mut self, part: &str, at: u64, duration: u64) {
        self.preset(part, Preset::RiseIn, at, duration);
    }

    pub fn fade(&mut self, part: &str, at: u64, duration: u64) {
        self.preset(part, Preset::FadeIn, at, duration);
    }

    pub fn fade_root(&mut self, at: u64, duration: u64) {
        self.moves.push(Move::Preset {
            target: Target::Root,
            preset: Preset::FadeIn,
            at,
            duration,
            strength: 1.0,
            ease: None,
        });
    }

    /// A group arriving: it fades in and rises `dy` (a group has no height for a preset to use).
    pub fn reveal_group(&mut self, part: &str, at: u64, duration: u64, dy: f64) {
        let t = self.part(part);
        self.reveal(t, at, duration, dy);
    }

    pub fn reveal(&mut self, target: Target, at: u64, duration: u64, dy: f64) {
        self.keys(
            target.clone(),
            AnimProp::Opacity,
            &[(at, 0.0, Ease::Linear), (at + duration, 1.0, Ease::EaseOut)],
        );
        self.keys(
            target,
            AnimProp::Y,
            &[(at, dy, Ease::Linear), (at + duration, 0.0, Ease::EaseOut)],
        );
    }

    pub fn finish(mut self) -> Built {
        while self.stack.len() > 1 {
            self.end();
        }
        let (name, children) = self.stack.pop().unwrap_or_default();
        let mut root = NewNode::new(NodeKind::Group);
        root.name = Some(name);
        root.children = children;
        Built {
            root,
            moves: self.moves,
            assets: self.assets,
            tokens: self.created_tokens,
        }
    }
}

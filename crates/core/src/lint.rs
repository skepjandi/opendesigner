//! The document linter: stable rule codes for things that make a design or a video wrong without
//! making it invalid (`docs/wiki/lint-rules.md`). Read-only; it never changes a document.
//!
//! The core knows nothing of fonts or the file system beyond `media/`, so text measuring and the
//! export targets come in through [`LintOptions`]; a rule whose input is missing is skipped, not
//! guessed.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::animation::AnimProp;
use crate::media;
use crate::model::{Document, Node, NodeId, NodeKind};
use crate::paint::{Paint, Rgba};
use crate::variables::{Variable, VariableKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Error => "error",
        }
    }
}

/// One rule: a code that never changes meaning, what it is called, and how severe it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub code: &'static str,
    pub name: &'static str,
    pub severity: Severity,
    pub summary: &'static str,
}

const fn rule(
    code: &'static str,
    name: &'static str,
    severity: Severity,
    summary: &'static str,
) -> Rule {
    Rule {
        code,
        name,
        severity,
        summary,
    }
}

pub const RULES: &[Rule] = &[
    rule(
        "OD001",
        "node-outside-frame",
        Severity::Warn,
        "A node lies entirely outside the frame that contains it, so it is never visible.",
    ),
    rule(
        "OD002",
        "node-clipped-by-frame",
        Severity::Info,
        "A node extends past its frame's edge (a bleed); the frame clips the overflow.",
    ),
    rule(
        "OD003",
        "text-clipped-by-frame",
        Severity::Warn,
        "A text node extends past its frame's edge, so some of the words are cut off.",
    ),
    rule(
        "OD010",
        "text-overflow",
        Severity::Warn,
        "Text wraps to more rows than its box is tall.",
    ),
    rule(
        "OD011",
        "text-empty",
        Severity::Warn,
        "A visible text node has no text.",
    ),
    rule(
        "OD012",
        "text-too-small",
        Severity::Warn,
        "Text is smaller than 1.2% of the frame's long side, unreadable at store-listing size.",
    ),
    rule(
        "OD013",
        "low-contrast",
        Severity::Warn,
        "Text contrast against its background fill is below WCAG AA (4.5, or 3 for large text).",
    ),
    rule(
        "OD020",
        "media-missing",
        Severity::Error,
        "An image or screen recording names a file that is not in media/.",
    ),
    rule(
        "OD021",
        "token-undefined",
        Severity::Error,
        "A paint refers to a design token that is not defined.",
    ),
    rule(
        "OD030",
        "invisible-node-animated",
        Severity::Warn,
        "An animated node is hidden, fully transparent or zero-sized and nothing animates that away.",
    ),
    rule(
        "OD031",
        "invisible-frame-in-video",
        Severity::Warn,
        "A frame shown by a clip or overlay is hidden, fully transparent or zero-sized.",
    ),
    rule(
        "OD040",
        "siblings-overlap",
        Severity::Warn,
        "Two texts, or two 3D devices, in the same container cover each other.",
    ),
    rule(
        "OD041",
        "frame-size-no-target",
        Severity::Warn,
        "A top-level frame's size matches none of the chosen export targets.",
    ),
    rule(
        "OD050",
        "clip-frame-missing",
        Severity::Error,
        "A clip shows a frame that does not exist.",
    ),
    rule(
        "OD051",
        "footage-missing",
        Severity::Error,
        "A clip plays footage that is not in media/.",
    ),
    rule(
        "OD052",
        "overlay-outside-video",
        Severity::Warn,
        "An overlay starts after the video ends or runs past its end.",
    ),
    rule(
        "OD053",
        "animation-beyond-video",
        Severity::Warn,
        "An animation has keys after the video ends.",
    ),
    rule(
        "OD054",
        "sound-missing",
        Severity::Error,
        "A video's sound names a file that is not in media/.",
    ),
    rule(
        "OD055",
        "video-empty",
        Severity::Warn,
        "A video has no clips.",
    ),
    rule(
        "OD060",
        "binding-node-missing",
        Severity::Error,
        "A template variable is bound to a node that no longer exists.",
    ),
    rule(
        "OD061",
        "binding-invalid",
        Severity::Error,
        "A template variable's binding cannot be written to its node (wrong property for the node).",
    ),
    rule(
        "OD062",
        "binding-shadowed-by-translation",
        Severity::Info,
        "A text node bound to a variable also has translations; those win in their language.",
    ),
    rule(
        "OD063",
        "binding-duplicated",
        Severity::Warn,
        "Two variables are bound to the same node property; the last one set wins.",
    ),
    rule(
        "OD080",
        "node-scaled-flat",
        Severity::Warn,
        "A node's scale_x or scale_y is 0 and nothing animates it, so the node is never visible.",
    ),
    rule(
        "OD081",
        "clip-without-area",
        Severity::Warn,
        "A group clips its children but has no size and no children to take a box from, so it shows nothing.",
    ),
    rule(
        "OD082",
        "loop-jumps",
        Severity::Info,
        "A looping animation ends on a different value than it starts with, so it jumps once per cycle.",
    ),
    rule(
        "OD070",
        "path-invisible",
        Severity::Warn,
        "A path has no fill and no visible stroke, so nothing of it is drawn.",
    ),
    rule(
        "OD071",
        "gradient-stops-unsorted",
        Severity::Warn,
        "A gradient's stops are not in increasing offset order; they are drawn sorted, which is rarely what was meant.",
    ),
    rule(
        "OD072",
        "trim-hides-stroke",
        Severity::Warn,
        "A stroke's trim start and end are equal, so none of the outline is drawn and nothing animates it.",
    ),
];

pub fn rule_for(code: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.code == code)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<NodeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<NodeId>,
    pub message: String,
    pub hint: String,
}

/// Where a finding points.
#[derive(Default, Clone)]
struct At {
    node: Option<NodeId>,
    frame: Option<NodeId>,
    video: Option<NodeId>,
}

impl At {
    fn node(node: &NodeId, frame: Option<&NodeId>) -> At {
        At {
            node: Some(node.clone()),
            frame: frame.cloned(),
            video: None,
        }
    }

    fn video(video: &NodeId) -> At {
        At {
            video: Some(video.clone()),
            ..At::default()
        }
    }
}

/// How a text box fails to hold its text, for [`TextMeasure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextOverflow {
    pub needed: u32,
    pub available: u32,
}

/// Measures laid-out text; the renderer implements it (the core has no fonts).
pub trait TextMeasure {
    fn overflow(&self, text: &Node) -> Option<TextOverflow>;
}

/// An export size rule the frames are checked against.
pub struct ExportTarget<'a> {
    pub id: String,
    pub accepts: Box<dyn Fn(u32, u32) -> bool + 'a>,
}

#[derive(Default)]
pub struct LintOptions<'a> {
    /// The project folder; without it the media rules (OD020, OD051, OD054) are skipped.
    pub project_dir: Option<&'a Path>,
    /// Without it text overflow (OD010) is skipped.
    pub text: Option<&'a dyn TextMeasure>,
    /// Without any, OD041 is skipped.
    pub export_targets: Vec<ExportTarget<'a>>,
}

pub fn worst(findings: &[Finding]) -> Option<Severity> {
    findings.iter().map(|f| f.severity).max()
}

/// Counts of findings per severity, `(errors, warnings, infos)`.
pub fn counts(findings: &[Finding]) -> (usize, usize, usize) {
    let n = |s| findings.iter().filter(|f| f.severity == s).count();
    (n(Severity::Error), n(Severity::Warn), n(Severity::Info))
}

pub fn lint(doc: &Document, opts: &LintOptions) -> Vec<Finding> {
    let mut linter = Linter {
        doc,
        opts,
        out: Vec::new(),
        backdrops: Vec::new(),
        used_frames: used_frames(doc),
    };
    for page in &doc.pages {
        linter.node(&page.root, (0.0, 0.0), None);
    }
    linter.variables();
    linter.videos();
    linter.out
}

fn used_frames(doc: &Document) -> HashSet<NodeId> {
    let mut used = HashSet::new();
    for v in &doc.videos {
        used.extend(v.clips.iter().filter_map(|c| c.frame.clone()));
        used.extend(v.overlays.iter().map(|o| o.frame.clone()));
    }
    used
}

struct FrameBox {
    id: NodeId,
    w: f64,
    h: f64,
}

/// What a drawn node leaves behind it for text above to sit on. `color: None` is an image or
/// device, whose pixels the linter cannot know.
struct Backdrop {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: Option<[f64; 3]>,
}

impl Backdrop {
    fn holds(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

struct Linter<'a> {
    doc: &'a Document,
    opts: &'a LintOptions<'a>,
    out: Vec<Finding>,
    backdrops: Vec<Backdrop>,
    used_frames: HashSet<NodeId>,
}

impl Linter<'_> {
    fn add(&mut self, code: &'static str, at: At, message: String, hint: &str) {
        let severity = rule_for(code).map_or(Severity::Warn, |r| r.severity);
        self.out.push(Finding {
            code,
            severity,
            node: at.node,
            frame: at.frame,
            video: at.video,
            message,
            hint: hint.to_string(),
        });
    }

    fn node(&mut self, n: &Node, origin: (f64, f64), frame: Option<&FrameBox>) {
        if !n.visible || n.opacity <= 0.0 {
            return;
        }
        let abs = (origin.0 + n.x, origin.1 + n.y);
        self.paint_tokens(n, frame);
        self.transform_rules(n, frame);
        self.stroke_and_gradients(n, frame);
        if let Some(f) = frame {
            self.bounds(n, f);
        }
        match n.kind {
            NodeKind::Text => self.text(n, abs, frame),
            NodeKind::Image | NodeKind::Device3d => self.picture(n, abs, frame),
            NodeKind::Rect | NodeKind::Ellipse => self.shape(n, abs),
            NodeKind::Frame => self.frame(n, abs, frame),
            NodeKind::Group => self.children(n, abs, frame),
            NodeKind::Path => self.vector(n, frame),
        }
    }

    fn children(&mut self, n: &Node, abs: (f64, f64), frame: Option<&FrameBox>) {
        if frame.is_some() {
            self.overlaps(n, frame.map(|f| &f.id));
        }
        for c in &n.children {
            self.node(c, abs, frame);
        }
    }

    fn frame(&mut self, n: &Node, abs: (f64, f64), outer: Option<&FrameBox>) {
        if outer.is_none() {
            self.backdrops.clear();
            self.target_size(n);
        }
        self.push_fill(n, abs);
        let mine = FrameBox {
            id: n.id.clone(),
            w: n.width,
            h: n.height,
        };
        self.children(n, abs, Some(&mine));
    }

    fn target_size(&mut self, n: &Node) {
        if self.opts.export_targets.is_empty() || self.used_frames.contains(&n.id) {
            return;
        }
        let (w, h) = (n.width.round() as u32, n.height.round() as u32);
        if self.opts.export_targets.iter().any(|t| (t.accepts)(w, h)) {
            return;
        }
        let ids: Vec<&str> = self
            .opts
            .export_targets
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        let message = format!(
            "frame {} is {w}x{h}, which none of the chosen export targets accept ({})",
            n.id,
            ids.join(", ")
        );
        self.add(
            "OD041",
            At::node(&n.id, None),
            message,
            "Resize the frame to a size the target accepts (list_export_presets), or pick the target this frame is for.",
        );
    }

    /// Nodes that turn, scale or clip: a flat scale shows nothing and a clip needs a box.
    fn transform_rules(&mut self, n: &Node, frame: Option<&FrameBox>) {
        let at = At::node(&n.id, frame.map(|f| &f.id));
        let animated = |prop: AnimProp| {
            self.doc.videos.iter().any(|v| {
                v.animations
                    .iter()
                    .any(|a| a.node == n.id && a.prop == prop)
            })
        };
        let flat = (n.scale_x == 0.0 && !animated(AnimProp::ScaleX) && !animated(AnimProp::Scale))
            || (n.scale_y == 0.0 && !animated(AnimProp::ScaleY) && !animated(AnimProp::Scale));
        if flat {
            self.add(
                "OD080",
                at.clone(),
                format!("{} has a scale of 0 and is never drawn", n.id),
                "Give scale_x and scale_y a real value, or animate them up from 0.",
            );
        }
        let no_area = || {
            n.layout_box()
                .is_none_or(|b| b.width <= 0.0 || b.height <= 0.0)
        };
        if n.kind == NodeKind::Group && n.clip && no_area() {
            self.add(
                "OD081",
                at,
                format!("group {} clips its children to a box with no area", n.id),
                "Give the group a width and height, or turn clip off.",
            );
        }
    }

    fn bounds(&mut self, n: &Node, f: &FrameBox) {
        if matches!(n.kind, NodeKind::Group) || n.width <= 0.0 || n.height <= 0.0 {
            return;
        }
        let Some(b) = crate::geometry::bounds_in_frame(self.doc, &f.id, &n.id) else {
            return;
        };
        let (left, top, right, bottom) = (b.x, b.y, b.right(), b.bottom());
        let outside = right <= 0.0 || left >= f.w || bottom <= 0.0 || top >= f.h;
        let partly = left < -0.5 || top < -0.5 || right > f.w + 0.5 || bottom > f.h + 0.5;
        if outside {
            self.add(
                "OD001",
                At::node(&n.id, Some(&f.id)),
                format!("{} lies entirely outside frame {}", n.id, f.id),
                "Move it inside the frame with a move operation, or delete it if it is a leftover.",
            );
        } else if partly && n.kind == NodeKind::Text {
            self.add(
                "OD003",
                At::node(&n.id, Some(&f.id)),
                format!("text {} extends past the edge of frame {}", n.id, f.id),
                "Move or narrow the text so all of it is inside the frame.",
            );
        } else if partly {
            self.add(
                "OD002",
                At::node(&n.id, Some(&f.id)),
                format!("{} extends past the edge of frame {}", n.id, f.id),
                "Fine for a deliberate bleed; otherwise move or resize it inside the frame.",
            );
        }
    }

    fn stroke_and_gradients(&mut self, n: &Node, frame: Option<&FrameBox>) {
        let at = At::node(&n.id, frame.map(|f| &f.id));
        let paints = n.fill.iter().chain(n.stroke.iter().map(|s| &s.paint));
        let unsorted = paints
            .filter_map(Paint::stops)
            .any(|stops| stops.windows(2).any(|w| w[0].offset > w[1].offset));
        if unsorted {
            self.add(
                "OD071",
                at.clone(),
                format!("{} has a gradient whose stops are out of order", n.id),
                "Order the stops by increasing offset.",
            );
        }
        let hidden = n.stroke.as_ref().is_some_and(|s| {
            s.width > 0.0
                && (s.trim_start - s.trim_end).abs() < f64::EPSILON
                && !self.trim_animated(&n.id)
        });
        if hidden {
            self.add(
                "OD072",
                at,
                format!("the trim of {}'s stroke leaves none of the outline", n.id),
                "Set trim_start and trim_end apart (0 and 1 draw it all), or animate trim_end to draw it on.",
            );
        }
    }

    fn trim_animated(&self, id: &NodeId) -> bool {
        self.doc.videos.iter().any(|v| {
            v.animations
                .iter()
                .any(|a| &a.node == id && matches!(a.prop, AnimProp::TrimStart | AnimProp::TrimEnd))
        })
    }

    fn vector(&mut self, n: &Node, frame: Option<&FrameBox>) {
        let drawn_stroke = n.stroke.as_ref().is_some_and(|s| s.width > 0.0);
        if n.fill.is_none() && !drawn_stroke {
            self.add(
                "OD070",
                At::node(&n.id, frame.map(|f| &f.id)),
                format!(
                    "path {} has no fill and no stroke, so it is invisible",
                    n.id
                ),
                "Give it a fill, or a stroke with a width above 0.",
            );
        }
    }

    fn paint_tokens(&mut self, n: &Node, frame: Option<&FrameBox>) {
        let paints = n.fill.iter().chain(n.stroke.iter().map(|s| &s.paint));
        let missing: Vec<String> = paints
            .flat_map(|p| p.undefined_tokens(&self.doc.tokens))
            .collect();
        for token in missing {
            self.add(
                "OD021",
                At::node(&n.id, frame.map(|f| &f.id)),
                format!("{} uses the token {{{token}}}, which is not defined", n.id),
                "Define it with set_token, or use a literal color or a token from list_tokens.",
            );
        }
    }

    fn text(&mut self, n: &Node, abs: (f64, f64), frame: Option<&FrameBox>) {
        let at = At::node(&n.id, frame.map(|f| &f.id));
        let text = n.text.as_deref().unwrap_or("");
        if text.trim().is_empty() {
            self.add(
                "OD011",
                at,
                format!("text {} is empty", n.id),
                "Set its text, or delete the node.",
            );
            return;
        }
        if let Some(measure) = self.opts.text
            && let Some(o) = measure.overflow(n)
        {
            self.add(
                "OD010",
                at.clone(),
                format!(
                    "text {} needs {} px of height but its box is {} px tall",
                    n.id, o.needed, o.available
                ),
                "Make the box taller, shorten the text, or reduce font_size.",
            );
        }
        if let Some(f) = frame {
            self.too_small(n, f, &at);
        }
        self.contrast(n, abs, &at);
    }

    fn too_small(&mut self, n: &Node, f: &FrameBox, at: &At) {
        let size = n.font_size.unwrap_or(16.0);
        let floor = 0.012 * f.w.max(f.h);
        if size < floor {
            self.add(
                "OD012",
                at.clone(),
                format!(
                    "text {} is {size} px in a {}x{} frame; at least {:.0} px stays readable at listing size",
                    n.id, f.w, f.h, floor
                ),
                "Raise font_size.",
            );
        }
    }

    fn contrast(&mut self, n: &Node, abs: (f64, f64), at: &At) {
        let Some(text) = self.text_color(n) else {
            return;
        };
        let (cx, cy) = (abs.0 + n.width / 2.0, abs.1 + n.height / 2.0);
        let under = match self.backdrops.iter().rev().find(|b| b.holds(cx, cy)) {
            Some(b) => match b.color {
                Some(c) => c,
                None => return,
            },
            None => [1.0, 1.0, 1.0],
        };
        let alpha = f64::from(text.a) / 255.0 * n.opacity;
        let ink = blend(rgb(text), under, alpha);
        let ratio = contrast_ratio(ink, under);
        let size = n.font_size.unwrap_or(16.0);
        let large = size >= 24.0 || (size >= 18.66 && n.font_weight.unwrap_or(400.0) >= 700.0);
        let needed = if large { 3.0 } else { 4.5 };
        if ratio < needed {
            self.add(
                "OD013",
                at.clone(),
                format!(
                    "text {} has contrast {ratio:.2}:1 against its background; {needed}:1 is the minimum",
                    n.id
                ),
                "Change the text fill or the background fill so they differ more in lightness.",
            );
        }
    }

    fn text_color(&self, n: &Node) -> Option<Rgba> {
        match &n.fill {
            Some(p) => p.resolve(&self.doc.tokens).ok(),
            None => Some(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
        }
    }

    fn shape(&mut self, n: &Node, abs: (f64, f64)) {
        self.push_fill(n, abs);
    }

    fn push_fill(&mut self, n: &Node, abs: (f64, f64)) {
        let Some(fill) = &n.fill else {
            return;
        };
        let Ok(c) = fill.resolve(&self.doc.tokens) else {
            return;
        };
        let (cx, cy) = (abs.0 + n.width / 2.0, abs.1 + n.height / 2.0);
        let under = self
            .backdrops
            .iter()
            .rev()
            .find(|b| b.holds(cx, cy))
            .map_or(Some([1.0, 1.0, 1.0]), |b| b.color);
        let alpha = f64::from(c.a) / 255.0 * n.opacity;
        let color = under.map(|u| blend(rgb(c), u, alpha));
        self.backdrops.push(Backdrop {
            x: abs.0,
            y: abs.1,
            w: n.width,
            h: n.height,
            color: if alpha < 0.05 { under } else { color },
        });
    }

    fn picture(&mut self, n: &Node, abs: (f64, f64), frame: Option<&FrameBox>) {
        self.backdrops.push(Backdrop {
            x: abs.0,
            y: abs.1,
            w: n.width,
            h: n.height,
            color: None,
        });
        let Some(dir) = self.opts.project_dir else {
            return;
        };
        let mut files: Vec<(&str, bool)> = Vec::new();
        if let Some(image) = &n.image {
            files.push((image, false));
        }
        if let Some(f) = n.device3d.as_ref().and_then(|d| d.footage.as_ref()) {
            files.push((&f.file, true));
        }
        for (reference, is_footage) in files {
            if !media_present(dir, reference) {
                let kind = if is_footage { "recording" } else { "image" };
                self.add(
                    "OD020",
                    At::node(&n.id, frame.map(|f| &f.id)),
                    format!(
                        "{} uses the {kind} {reference}, which is not in media/",
                        n.id
                    ),
                    "Pick a file from list_media (or list_footage), or import it first.",
                );
            }
        }
    }

    fn overlaps(&mut self, container: &Node, frame: Option<&NodeId>) {
        let boxes: Vec<&Node> = container
            .children
            .iter()
            .filter(|c| c.visible && c.width > 0.0 && c.height > 0.0)
            .filter(|c| matches!(c.kind, NodeKind::Text | NodeKind::Device3d))
            .collect();
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                if a.kind == b.kind && covers(a, b) {
                    self.add(
                        "OD040",
                        At::node(&b.id, frame),
                        format!("{} and {} cover each other", a.id, b.id),
                        "Move one of them so the two no longer share space.",
                    );
                }
            }
        }
    }

    fn variables(&mut self) {
        let mut owners: HashMap<(NodeId, String), NodeId> = HashMap::new();
        for var in &self.doc.variables {
            for b in &var.binds {
                if let Some(first) = owners.insert((b.node.clone(), b.prop.clone()), var.id.clone())
                    && first != var.id
                {
                    self.add(
                        "OD063",
                        At::node(&b.node, None),
                        format!(
                            "variables {first} and {} both write {}.{}",
                            var.id, b.node, b.prop
                        ),
                        "Bind the property to one variable only (set_variable binds).",
                    );
                }
            }
            self.variable(var);
        }
    }

    fn variable(&mut self, var: &Variable) {
        for b in &var.binds {
            let Some(node) = self.doc.node(&b.node) else {
                self.add(
                    "OD060",
                    At::node(&b.node, None),
                    format!(
                        "variable {} is bound to {}, which no longer exists",
                        var.id, b.node
                    ),
                    "Remove the binding (set_variable binds) or restore the node.",
                );
                continue;
            };
            if let Err(reason) = var.writes(node, &self.doc.tokens, &b.prop) {
                self.add(
                    "OD061",
                    At::node(&b.node, None),
                    format!(
                        "variable {} cannot write {}.{}: {reason}",
                        var.id, b.node, b.prop
                    ),
                    "Bind a property the node has, of a kind the variable can set.",
                );
            }
            let translated = b.prop == "text"
                && matches!(var.kind, VariableKind::String | VariableKind::Enum)
                && self.doc.locales.values().any(|t| t.contains_key(&b.node));
            if translated {
                self.add(
                    "OD062",
                    At::node(&b.node, None),
                    format!(
                        "{} has translations, so in those languages it shows them instead of variable {}",
                        b.node, var.id
                    ),
                    "Translate the variable's text in each language too (set_localized_text), or accept it.",
                );
            }
        }
    }

    fn videos(&mut self) {
        let doc = self.doc;
        for v in &doc.videos {
            let at = At::video(&v.id);
            if v.clips.is_empty() {
                self.add(
                    "OD055",
                    at.clone(),
                    format!("video {} has no clips", v.id),
                    "Add a clip with add_clip.",
                );
            }
            let duration = v.duration_ms();
            for c in &v.clips {
                self.clip(v.id.clone(), c);
            }
            for o in &v.overlays {
                self.shown_frame(&v.id, &o.frame);
                self.overlay(&v.id, o, duration);
            }
            self.sound(v);
            for a in &v.animations {
                self.animation(v, a, duration);
            }
        }
    }

    fn clip(&mut self, video: NodeId, c: &crate::video::Clip) {
        if let Some(frame) = &c.frame {
            self.shown_frame(&video, frame);
        }
        if let (Some(f), Some(dir)) = (&c.footage, self.opts.project_dir)
            && !media_present(dir, &f.file)
        {
            self.add(
                "OD051",
                At::video(&video),
                format!("clip {} plays {}, which is not in media/", c.id, f.file),
                "Choose footage from list_footage, or import the file first.",
            );
        }
    }

    fn shown_frame(&mut self, video: &NodeId, frame: &NodeId) {
        let Some(node) = self.doc.node(frame).filter(|n| n.kind == NodeKind::Frame) else {
            self.add(
                "OD050",
                At::video(video),
                format!("video {video} shows frame {frame}, which does not exist"),
                "Point the clip or overlay at an existing frame (set_clip frame / set_overlay frame).",
            );
            return;
        };
        if !node.visible || node.opacity <= 0.0 || node.width <= 0.0 || node.height <= 0.0 {
            self.add(
                "OD031",
                At::node(frame, None),
                format!(
                    "frame {frame}, shown by video {video}, is hidden, transparent or has no size"
                ),
                "Make the frame visible, opaque and sized before showing it in a video.",
            );
        }
    }

    fn overlay(&mut self, video: &NodeId, o: &crate::video::Overlay, duration: u64) {
        let end = o.start_ms + u64::from(o.duration_ms);
        let message = if o.start_ms >= duration {
            format!(
                "overlay {} starts at {} ms, after video {video} ends at {duration} ms",
                o.id, o.start_ms
            )
        } else if end > duration {
            format!(
                "overlay {} runs to {end} ms but video {video} ends at {duration} ms",
                o.id
            )
        } else {
            return;
        };
        self.add(
            "OD052",
            At::video(video),
            message,
            "Move the overlay earlier or shorten it (set_overlay start_ms / duration_ms), or lengthen the video.",
        );
    }

    fn sound(&mut self, v: &crate::video::Video) {
        let (Some(audio), Some(dir)) = (&v.audio, self.opts.project_dir) else {
            return;
        };
        if !media_present(dir, &audio.file) {
            self.add(
                "OD054",
                At::video(&v.id),
                format!(
                    "video {} plays {}, which is not in media/",
                    v.id, audio.file
                ),
                "Choose a sound from list_sounds, or import it first.",
            );
        }
    }

    fn animation(
        &mut self,
        v: &crate::video::Video,
        a: &crate::animation::Animation,
        duration: u64,
    ) {
        if a.repeat == crate::animation::Repeat::Loop
            && let (Some(first), Some(last)) = (a.keys.first(), a.keys.last())
            && (first.value - last.value).abs() > 1e-9
        {
            self.add(
                "OD082",
                At::node(&a.node, None),
                format!(
                    "the looping {} animation of {} in video {} ends on {} but starts on {}",
                    a.prop.as_str(),
                    a.node,
                    v.id,
                    last.value,
                    first.value
                ),
                "End the keys on the first key's value for a seamless loop, or use repeat pingpong.",
            );
        }
        if let Some(last) = a.keys.last()
            && last.at_ms > duration
        {
            self.add(
                "OD053",
                At::node(&a.node, None),
                format!(
                    "the {:?} animation of {} in video {} has a key at {} ms, after the video ends at {duration} ms",
                    a.prop, a.node, v.id, last.at_ms
                ),
                "Move the keys inside the video's duration, or lengthen the video.",
            );
        }
        let Some(node) = self.doc.node(&a.node) else {
            return;
        };
        let props: HashSet<AnimProp> = v
            .animations
            .iter()
            .filter(|o| o.node == a.node)
            .map(|o| o.prop)
            .collect();
        let hidden = !node.visible
            || (node.opacity <= 0.0 && !props.contains(&AnimProp::Opacity))
            || (node.kind != NodeKind::Group
                && ((node.width <= 0.0 && !props.contains(&AnimProp::Width))
                    || (node.height <= 0.0 && !props.contains(&AnimProp::Height))));
        let first = v
            .animations
            .iter()
            .find(|o| o.node == a.node)
            .is_some_and(|o| o.prop == a.prop);
        if hidden && first {
            self.add(
                "OD030",
                At::node(&a.node, None),
                format!(
                    "{} is animated in video {} but is hidden, transparent or zero-sized",
                    a.node, v.id
                ),
                "Make it visible with real size and opacity, or animate opacity/size in from zero.",
            );
        }
    }
}

fn media_present(dir: &Path, reference: &str) -> bool {
    let name = media::reference_name(reference)
        .or_else(|_| media::footage_name(reference))
        .or_else(|_| media::audio_name(reference));
    match name {
        Ok(name) => {
            std::fs::symlink_metadata(dir.join("media").join(name)).is_ok_and(|m| m.is_file())
        }
        Err(_) => false,
    }
}

fn covers(a: &Node, b: &Node) -> bool {
    let w = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let h = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    let smaller = (a.width * a.height).min(b.width * b.height);
    w * h > 0.2 * smaller
}

fn rgb(c: Rgba) -> [f64; 3] {
    [
        f64::from(c.r) / 255.0,
        f64::from(c.g) / 255.0,
        f64::from(c.b) / 255.0,
    ]
}

fn blend(fg: [f64; 3], bg: [f64; 3], alpha: f64) -> [f64; 3] {
    let a = alpha.clamp(0.0, 1.0);
    [
        fg[0] * a + bg[0] * (1.0 - a),
        fg[1] * a + bg[1] * (1.0 - a),
        fg[2] * a + bg[2] * (1.0 - a),
    ]
}

fn linear(channel: f64) -> f64 {
    if channel <= 0.03928 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(c: [f64; 3]) -> f64 {
    0.2126 * linear(c[0]) + 0.7152 * linear(c[1]) + 0.0722 * linear(c[2])
}

/// The WCAG 2 contrast ratio of two opaque colors, 1 to 21.
pub fn contrast_ratio(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

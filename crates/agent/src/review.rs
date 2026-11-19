//! A design review in numbers: what an art director would flag on a frame without being told
//! where to look (`docs/wiki/agent-integration.md#review_frame`). It never changes the design.

use od_core::geometry::{self, Rect};
use od_core::{Document, Node, NodeId, NodeKind, Rgba};
use serde_json::{Value, json};

/// WCAG: 4.5 for normal text, 3 for large text (24 px and up, or 19 px bold).
const MIN_CONTRAST: f64 = 4.5;
const MIN_CONTRAST_LARGE: f64 = 3.0;
/// Text smaller than this share of the frame's shorter side is unreadable at thumbnail size.
const MIN_TEXT_SHARE: f64 = 0.022;
/// Text closer to an edge than this share of the shorter side feels cramped (and gets cropped).
const MIN_MARGIN_SHARE: f64 = 0.04;
const MAX_FONT_FAMILIES: usize = 2;
const MAX_LOOSE_COLORS: usize = 6;

fn luminance(c: Rgba) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// The WCAG contrast ratio of two colors, 1 to 21.
pub fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn blend(top: Rgba, over: Rgba) -> Rgba {
    let a = f64::from(top.a) / 255.0;
    let mix = |t: u8, o: u8| (f64::from(t) * a + f64::from(o) * (1.0 - a)).round() as u8;
    Rgba {
        r: mix(top.r, over.r),
        g: mix(top.g, over.g),
        b: mix(top.b, over.b),
        a: 255,
    }
}

struct Item<'a> {
    node: &'a Node,
    bounds: Rect,
    /// Opacity multiplied down from the frame.
    opacity: f64,
}

/// Every visible node under `node` in drawing order (first is drawn first), with its bounds
/// relative to `root`, the frame being reviewed.
fn flatten<'a>(
    doc: &'a Document,
    root: &NodeId,
    node: &'a Node,
    out: &mut Vec<Item<'a>>,
    opacity: f64,
) {
    for child in &node.children {
        if !child.visible {
            continue;
        }
        let op = opacity * child.opacity;
        if let Some(bounds) = geometry::bounds_in_frame(doc, root, &child.id) {
            out.push(Item {
                node: child,
                bounds,
                opacity: op,
            });
        }
        flatten(doc, root, child, out, op);
    }
}

fn covers(r: Rect, x: f64, y: f64) -> bool {
    x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
}

/// What the text at `at` sits on: the topmost earlier fill under its middle, over the frame's own.
fn background_for(doc: &Document, frame: &Node, items: &[Item<'_>], at: usize) -> Rgba {
    let base = frame
        .fill
        .as_ref()
        .and_then(|f| f.resolve(&doc.tokens).ok())
        .unwrap_or(Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        });
    let b = items[at].bounds;
    let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    let mut color = base;
    for it in &items[..at] {
        if !matches!(
            it.node.kind,
            NodeKind::Rect | NodeKind::Ellipse | NodeKind::Frame
        ) {
            continue;
        }
        let Some(fill) = it
            .node
            .fill
            .as_ref()
            .and_then(|f| f.resolve(&doc.tokens).ok())
        else {
            continue;
        };
        if covers(it.bounds, cx, cy) {
            let mut c = fill;
            c.a = (f64::from(c.a) * it.opacity).round() as u8;
            color = blend(c, color);
        }
    }
    color
}

fn finding(severity: &str, node: Option<&Node>, message: String) -> Value {
    json!({
        "severity": severity,
        "node": node.map(|n| n.id.to_string()),
        "name": node.map(|n| n.name.clone()),
        "message": message,
    })
}

/// Reviews one frame. `severity` is `error` (fix it), `warn` (fix it or have a reason) or `info`.
pub fn review_frame(doc: &Document, frame_id: &NodeId) -> Result<Value, String> {
    let frame = doc
        .node(frame_id)
        .ok_or_else(|| format!("node {frame_id} does not exist"))?;
    if frame.kind != NodeKind::Frame {
        return Err(format!(
            "{frame_id} is a {}, not a frame",
            frame.kind.as_str()
        ));
    }
    let short = frame.width.min(frame.height).max(1.0);
    let mut items = Vec::new();
    flatten(doc, frame_id, frame, &mut items, 1.0);
    let mut found: Vec<Value> = Vec::new();
    let mut families: Vec<String> = Vec::new();
    let mut loose_colors: Vec<String> = Vec::new();
    let mut text_boxes: Vec<(usize, Rect)> = Vec::new();
    let mut lefts: Vec<f64> = Vec::new();

    for (i, it) in items.iter().enumerate() {
        let n = it.node;
        if let Some(Ok(fill)) = n.fill.as_ref().map(|f| f.resolve(&doc.tokens)) {
            if let Some(od_core::Paint::Color(_)) = n.fill {
                let hex = format!("#{:02x}{:02x}{:02x}", fill.r, fill.g, fill.b);
                if !loose_colors.contains(&hex) {
                    loose_colors.push(hex);
                }
            }
        }
        if n.kind != NodeKind::Text || n.text.as_deref().unwrap_or("").trim().is_empty() {
            // anything that leaves the frame on purpose is fine for pictures and shapes
            continue;
        }
        let size = n.font_size.unwrap_or(16.0);
        let family = n.font_family.clone().unwrap_or_else(|| "Lato".to_string());
        if !families.contains(&family) {
            families.push(family);
        }
        if size / short < MIN_TEXT_SHARE {
            found.push(finding(
                "warn",
                Some(n),
                format!(
                    "text is {size:.0} px on a {short:.0} px frame ({:.1}%): too small to read at thumbnail size; use at least {:.0} px",
                    100.0 * size / short,
                    MIN_TEXT_SHARE * short
                ),
            ));
        }
        let b = it.bounds;
        if b.x < 0.0
            || b.y < 0.0
            || b.x + b.width > frame.width + 0.5
            || b.y + b.height > frame.height + 0.5
        {
            found.push(finding(
                "error",
                Some(n),
                "the text box leaves the frame: it will be cut off".into(),
            ));
        } else {
            let margin =
                b.x.min(b.y)
                    .min(frame.width - b.x - b.width)
                    .min(frame.height - b.y - b.height);
            if margin / short < MIN_MARGIN_SHARE {
                found.push(finding(
                    "warn",
                    Some(n),
                    format!(
                        "text sits {margin:.0} px from an edge ({:.1}% of the short side); keep at least {:.0} px",
                        100.0 * margin / short,
                        MIN_MARGIN_SHARE * short
                    ),
                ));
            }
        }
        // contrast against what is under it
        if let Some(fg) = n
            .fill
            .as_ref()
            .and_then(|f| f.resolve(&doc.tokens).ok())
            .or(Some(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }))
        {
            let mut fg = fg;
            fg.a = (f64::from(fg.a) * it.opacity).round() as u8;
            let bg = background_for(doc, frame, &items, i);
            let fg = blend(fg, bg);
            let ratio = contrast(fg, bg);
            let large = size >= 24.0 || (size >= 19.0 && n.font_weight.unwrap_or(400.0) >= 700.0);
            let need = if large {
                MIN_CONTRAST_LARGE
            } else {
                MIN_CONTRAST
            };
            if ratio < need {
                found.push(finding(
                    "error",
                    Some(n),
                    format!(
                        "contrast {ratio:.1}:1 against what is under it; needs at least {need}:1. Change the text color or put a darker/lighter shape under it"
                    ),
                ));
            }
        }
        text_boxes.push((i, b));
        lefts.push(b.x);
    }

    // text that wraps to more rows than its box (the renderer's own layout)
    for w in od_render::check_text(frame) {
        found.push(finding("error", None, w.to_string()));
    }
    // text boxes that overlap each other
    for (a, (ia, ra)) in text_boxes.iter().enumerate() {
        for (ib, rb) in &text_boxes[a + 1..] {
            let ox = (ra.x + ra.width).min(rb.x + rb.width) - ra.x.max(rb.x);
            let oy = (ra.y + ra.height).min(rb.y + rb.height) - ra.y.max(rb.y);
            if ox > 4.0 && oy > 4.0 {
                found.push(finding(
                    "warn",
                    Some(items[*ia].node),
                    format!(
                        "text overlaps \"{}\" ({:.0} x {:.0} px): move or resize one",
                        items[*ib].node.name, ox, oy
                    ),
                ));
            }
        }
    }
    // left edges that almost line up
    lefts.sort_by(f64::total_cmp);
    for pair in lefts.windows(2) {
        let d = pair[1] - pair[0];
        if d > 0.5 && d < 0.012 * frame.width {
            found.push(finding(
                "info",
                None,
                format!("two text left edges differ by {d:.1} px: align them (same x) or make the difference deliberate"),
            ));
            break;
        }
    }
    if families.len() > MAX_FONT_FAMILIES {
        found.push(finding(
            "warn",
            None,
            format!(
                "{} font families ({}): use one or two",
                families.len(),
                families.join(", ")
            ),
        ));
    }
    if loose_colors.len() > MAX_LOOSE_COLORS {
        found.push(finding(
            "info",
            None,
            format!(
                "{} different hard-coded fill colors: define the palette as tokens (set_token) so it is consistent and one change recolors everything",
                loose_colors.len()
            ),
        ));
    }
    let text_count = text_boxes.len();
    if text_count == 0 {
        found.push(finding("info", None, "no text on this frame".into()));
    }
    if text_count > 6 {
        found.push(finding(
            "warn",
            None,
            format!("{text_count} text layers: a store image sells one idea; cut it down"),
        ));
    }
    let errors = found.iter().filter(|f| f["severity"] == "error").count();
    let warns = found.iter().filter(|f| f["severity"] == "warn").count();
    Ok(json!({
        "frame": frame_id.to_string(),
        "size": [frame.width, frame.height],
        "summary": format!("{errors} to fix, {warns} to consider"),
        "findings": found,
        "stats": {
            "text_layers": text_count,
            "font_families": families,
            "hard_coded_colors": loose_colors.len(),
            "tokens": doc.tokens.len(),
        },
        "also": "Review with render_preview at a small max_side (160 to 320): can you read the headline and name the focal point?",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::{Actor, NewNode, Operation, Paint, Session};

    fn frame_with(f: impl FnOnce(&mut Session, &NodeId)) -> (Session, NodeId) {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut fr = NewNode::new(NodeKind::Frame);
        (fr.width, fr.height) = (1000.0, 2000.0);
        fr.fill = Paint::parse("#101820").ok();
        let id = s
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: root,
                    index: None,
                    node: fr,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        f(&mut s, &id);
        (s, id)
    }

    fn text(s: &mut Session, parent: &NodeId, name: &str, at: [f64; 4], size: f64, color: &str) {
        let mut t = NewNode::new(NodeKind::Text);
        t.name = Some(name.into());
        (t.x, t.y, t.width, t.height) = (at[0], at[1], at[2], at[3]);
        t.text = Some("Hold for everything".into());
        t.font_size = Some(size);
        t.fill = Paint::parse(color).ok();
        s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: parent.clone(),
                index: None,
                node: t,
            }],
        )
        .unwrap();
    }

    fn messages(v: &Value) -> Vec<String> {
        v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                format!(
                    "{}: {}",
                    f["severity"].as_str().unwrap(),
                    f["message"].as_str().unwrap()
                )
            })
            .collect()
    }

    #[test]
    fn contrast_follows_wcag() {
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
        assert!((contrast(white, black) - 21.0).abs() < 0.01);
        assert!((contrast(white, white) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_good_headline_passes_and_every_flaw_is_named() {
        let (s, f) = frame_with(|s, f| {
            text(
                s,
                f,
                "Headline",
                [90.0, 200.0, 820.0, 300.0],
                120.0,
                "#f5f0e6",
            )
        });
        let r = review_frame(s.document(), &f).unwrap();
        assert!(
            r["summary"].as_str().unwrap().starts_with("0 to fix"),
            "{:?}",
            messages(&r)
        );

        let (s, f) = frame_with(|s, f| {
            text(s, f, "Low", [90.0, 200.0, 820.0, 300.0], 120.0, "#1b2530"); // dark on dark
            text(s, f, "Tiny", [90.0, 700.0, 820.0, 60.0], 20.0, "#f5f0e6");
            text(s, f, "Edge", [10.0, 900.0, 400.0, 100.0], 60.0, "#f5f0e6");
            text(s, f, "Off", [900.0, 1900.0, 400.0, 200.0], 60.0, "#f5f0e6");
            text(s, f, "A", [90.0, 1200.0, 500.0, 200.0], 80.0, "#f5f0e6");
            text(s, f, "B", [92.0, 1250.0, 500.0, 200.0], 80.0, "#f5f0e6");
        });
        let m = messages(&review_frame(s.document(), &f).unwrap()).join("\n");
        assert!(m.contains("contrast"), "{m}");
        assert!(m.contains("too small to read"), "{m}");
        assert!(m.contains("from an edge"), "{m}");
        assert!(m.contains("leaves the frame"), "{m}");
        assert!(m.contains("overlaps"), "{m}");
        assert!(m.contains("left edges differ"), "{m}");
    }

    #[test]
    fn a_shape_under_the_text_counts_as_its_background() {
        let (s, f) = frame_with(|s, f| {
            let mut card = NewNode::new(NodeKind::Rect);
            (card.x, card.y, card.width, card.height) = (60.0, 150.0, 880.0, 400.0);
            card.fill = Paint::parse("#ffffff").ok();
            s.apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: f.clone(),
                    index: None,
                    node: card,
                }],
            )
            .unwrap();
            // white text on a white card fails even though the frame behind is dark
            text(
                s,
                f,
                "Headline",
                [90.0, 200.0, 820.0, 300.0],
                120.0,
                "#ffffff",
            );
        });
        let m = messages(&review_frame(s.document(), &f).unwrap()).join("\n");
        assert!(m.contains("contrast 1.0:1"), "{m}");
    }

    #[test]
    fn only_frames_can_be_reviewed() {
        let (s, f) =
            frame_with(|s, f| text(s, f, "T", [90.0, 200.0, 820.0, 300.0], 120.0, "#ffffff"));
        let child = s.document().node(&f).unwrap().children[0].id.clone();
        assert!(review_frame(s.document(), &child).is_err());
        assert!(review_frame(s.document(), &NodeId::parse("n_99").unwrap()).is_err());
    }
}

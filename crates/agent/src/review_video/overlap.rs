//! The overlap pass: where two things of a scene sit on top of each other. Most mistakes in a long
//! animation are layering mistakes (a hand behind the cup, a label under the hill), and numbers
//! cannot see them. This lists the regions, with the layers in them from back to front, so the
//! agent looks at each with `video_crop` and confirms which should be on top. It is information,
//! not a finding: it never takes points off.

use od_core::geometry::{self, Rect};
use od_core::{Document, Node, NodeId, Video};
use serde_json::{Value, json};

use super::Scene;

/// Moments looked at inside each scene, as parts of its length.
const MOMENTS: [(u64, u64); 3] = [(1, 4), (1, 2), (3, 4)];
/// Layers covering this share of the frame are the backdrop, not a layer to check.
const BACKDROP_SHARE: f64 = 0.6;
/// Layers smaller than this share of the frame are specks.
const SPECK_SHARE: f64 = 0.001;
/// Two layers overlapping by less than this share of the frame are only touching.
const MIN_OVERLAP_SHARE: f64 = 0.01;
/// Regions reported in all, largest first.
const MAX_REGIONS: usize = 12;

struct Layer<'a> {
    node: &'a Node,
    bounds: Rect,
}

/// One region where two layers overlap.
struct Region {
    scene: usize,
    at_ms: u64,
    rect: Rect,
    /// The lower layer, then the one drawn over it.
    back_to_front: [(NodeId, String); 2],
}

impl Region {
    fn area(&self) -> f64 {
        self.rect.width * self.rect.height
    }

    fn to_json(&self, scenes: &[Scene]) -> Value {
        let names: Vec<&str> = self.back_to_front.iter().map(|(_, n)| n.as_str()).collect();
        json!({
            "scene": self.scene + 1,
            "scene_name": scenes[self.scene].name,
            "at_ms": self.at_ms,
            "rect": {
                "x": self.rect.x.round(), "y": self.rect.y.round(),
                "width": self.rect.width.round(), "height": self.rect.height.round(),
            },
            "back_to_front": names,
            "nodes": self.back_to_front.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            "check": format!(
                "video_crop at {} ms around {} (node {}): is {} meant to be over {}?",
                self.at_ms,
                names[1],
                self.back_to_front[1].0,
                names[1],
                names[0]
            ),
        })
    }
}

fn intersection(a: Rect, b: Rect) -> Option<Rect> {
    let (x, y) = (a.x.max(b.x), a.y.max(b.y));
    let (right, bottom) = (a.right().min(b.right()), a.bottom().min(b.bottom()));
    (right > x && bottom > y).then_some(Rect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}

/// The visible layers of the frame (its direct children) that are neither backdrop nor speck, in
/// drawing order, back to front.
fn layers<'a>(doc: &'a Document, frame: &'a Node) -> Vec<Layer<'a>> {
    let area = frame.width * frame.height;
    frame
        .children
        .iter()
        .filter(|c| c.visible && c.opacity > 0.0)
        .filter_map(|c| {
            let bounds = geometry::bounds_in_frame(doc, &frame.id, &c.id)?;
            let share = bounds.width * bounds.height / area;
            (SPECK_SHARE..BACKDROP_SHARE)
                .contains(&share)
                .then_some(Layer { node: c, bounds })
        })
        .collect()
}

fn regions_at(doc: &Document, frame: &Node, scene: usize, at_ms: u64) -> Vec<Region> {
    let all = layers(doc, frame);
    let min_area = frame.width * frame.height * MIN_OVERLAP_SHARE;
    let mut out = Vec::new();
    for (i, back) in all.iter().enumerate() {
        for front in &all[i + 1..] {
            let Some(rect) = intersection(back.bounds, front.bounds) else {
                continue;
            };
            if rect.width * rect.height >= min_area {
                out.push(Region {
                    scene,
                    at_ms,
                    rect,
                    back_to_front: [
                        (back.node.id.clone(), back.node.name.clone()),
                        (front.node.id.clone(), front.node.name.clone()),
                    ],
                });
            }
        }
    }
    out
}

/// The overlap regions of every scene at three moments, each pair of layers once (where they
/// overlap most), the largest first and at most [`MAX_REGIONS`]; with how many there were.
pub fn regions(doc: &Document, v: &Video, scenes: &[Scene]) -> (Vec<Value>, usize) {
    let mut found: Vec<Region> = Vec::new();
    for (i, s) in scenes.iter().enumerate() {
        for (num, den) in MOMENTS {
            let at = s.placed.start_ms + (s.placed.end_ms - s.placed.start_ms) * num / den;
            let animated = doc.at_time(v, at as f64);
            let d = animated.as_ref().unwrap_or(doc);
            let Some(frame) = d.node(&s.frame) else {
                continue;
            };
            for r in regions_at(d, frame, i, at) {
                let same = |o: &Region| {
                    o.scene == r.scene
                        && o.back_to_front[0].0 == r.back_to_front[0].0
                        && o.back_to_front[1].0 == r.back_to_front[1].0
                };
                match found.iter_mut().find(|o| same(o)) {
                    Some(o) if o.area() < r.area() => *o = r,
                    Some(_) => {}
                    None => found.push(r),
                }
            }
        }
    }
    found.sort_by(|a, b| b.area().total_cmp(&a.area()));
    let total = found.len();
    found.truncate(MAX_REGIONS);
    (found.iter().map(|r| r.to_json(scenes)).collect(), total)
}

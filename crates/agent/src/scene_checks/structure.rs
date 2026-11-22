//! The parts of a scene that are a structure, not a picture: which plan entry is which node,
//! which pair is a mirror of the other, and what must not change after the shapes pass.

use std::collections::BTreeMap;

use od_core::{Document, Node, NodeId, NodeKind, Video};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::geometry::place;
use super::{Calibration, Gate, Timeline, frame_height, frame_of, last_ms, spread, tidy_times};

/// One layer of the plan. Fields the gates do not use are kept: the plan has more to say.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LayerEntry {
    pub id: String,
    /// The node it is: a node id, or a name.
    #[serde(default)]
    pub node_ref: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    /// The layer must cover the frame under every camera move.
    #[serde(default)]
    pub bleed: bool,
    /// The layer it is attached to.
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub attach: Option<Attach>,
    /// `l` or `r`.
    #[serde(default)]
    pub side: Option<String>,
    /// Where the side came from: `reference` (measured) or `convention` (assumed).
    #[serde(default)]
    pub side_source: Option<String>,
    /// `front` or `side`.
    #[serde(default)]
    pub facing: Option<String>,
    /// The plan entry this one mirrors.
    #[serde(default)]
    pub mirror_of: Option<String>,
    /// What motion this layer is meant to have.
    #[serde(default)]
    pub expects_motion: Option<String>,
    /// The layer is not owned by the plan (a helper the plan does not name).
    #[serde(default)]
    pub ignore: bool,
}

/// How a child is attached to its parent.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Attach {
    /// How far the child's box may sit from the parent's, in pixels at 1080p.
    #[serde(default)]
    pub gap_tolerance_px: Option<f64>,
    /// `overlap`, `embedded`, `hinge` or `socket`.
    #[serde(default)]
    pub contact: Option<String>,
}

/// One detail of the inventory.
#[derive(Debug, Clone, Deserialize)]
pub struct Detail {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub maps_to: Option<MapsTo>,
}

/// What a detail is realised by.
#[derive(Debug, Clone, Deserialize)]
pub struct MapsTo {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
}

/// A snapshot of the shapes, to compare later passes against.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Freeze {
    pub hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub attributes: BTreeMap<String, BTreeMap<String, String>>,
}

/// What a snapshot holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub hashes: BTreeMap<String, String>,
    pub attributes: BTreeMap<String, BTreeMap<String, String>>,
}

impl Snapshot {
    pub fn to_json(&self) -> Value {
        json!({"hashes": self.hashes, "attributes": self.attributes})
    }
}

/// A name with only letters and digits, in lower case, so "Wing L" and "wing-l" are one name.
pub fn normalize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// The node a plan entry names, and how it was found.
pub fn resolve(doc: &Document, entry: &LayerEntry) -> Option<(NodeId, String)> {
    for candidate in [entry.node_ref.as_deref(), entry.name.as_deref()]
        .into_iter()
        .flatten()
    {
        if let Ok(id) = NodeId::parse(candidate)
            && doc.node(&id).is_some()
        {
            return Some((id, "node id".into()));
        }
        let want = normalize(candidate);
        if let Some(found) = first_named(doc, &want) {
            return Some((found, format!("the name {candidate:?}")));
        }
    }
    None
}

/// The first node with that name, in document order.
fn first_named(doc: &Document, want: &str) -> Option<NodeId> {
    for page in &doc.pages {
        let mut all = Vec::new();
        super::subtree(&page.root, &mut all);
        if let Some(n) = all.into_iter().find(|n| normalize(&n.name) == want) {
            return Some(n.id.clone());
        }
    }
    None
}

/// How two mirrored parts relate: a reflection (the lateral axis is negated), a half turn, a
/// move, or nothing in particular.
pub fn pair_relation(left: &Node, right: &Node) -> &'static str {
    let mirrored = (left.scale_x < 0.0) != (right.scale_x < 0.0);
    let opposite = (left.rotation + right.rotation).abs() <= 0.5;
    let half_turn = ((left.rotation - right.rotation).abs() - 180.0).abs() <= 0.5;
    if mirrored && opposite {
        "reflection"
    } else if half_turn {
        "rotation"
    } else if !mirrored && (left.rotation - right.rotation).abs() <= 0.5 {
        "translation"
    } else {
        "unrelated"
    }
}

/// The chirality gate: a left and a right part are a reflection of each other, and the left one
/// is on the viewer's right when the character faces front.
pub fn chirality_gate(doc: &Document, video: &Video, plan: Option<&[LayerEntry]>) -> Gate {
    let mut gate = Gate::new("chirality", Calibration::Derived);
    let Some(plan) = plan else {
        gate.unknowns
            .push("no layer plan was given, so no pair could be looked at".into());
        return gate;
    };
    let sided: Vec<&LayerEntry> = plan.iter().filter(|e| e.side.is_some()).collect();
    if sided.is_empty() {
        gate.unknowns
            .push("no layer declares a side, so no pair could be looked at".into());
        return gate;
    }
    let mut used: Vec<&str> = Vec::new();
    for entry in &sided {
        if used.contains(&entry.id.as_str()) {
            continue;
        }
        let Some(partner) = partner_of(plan, entry, &used) else {
            gate.failures.push(format!(
                "{}: no layer with the other side to pair it with",
                entry.id
            ));
            continue;
        };
        used.push(&entry.id);
        used.push(&partner.id);
        let (Some((l, _)), Some((r, _))) = (resolve(doc, entry), resolve(doc, partner)) else {
            gate.failures
                .push(format!("{}: the node it names was not found", entry.id));
            continue;
        };
        let (Some(ln), Some(rn)) = (doc.node(&l), doc.node(&r)) else {
            continue;
        };
        let relation = pair_relation(ln, rn);
        if relation != "reflection" {
            gate.failures.push(format!(
                "{} and {}: the two sides are a {relation}, not a reflection of each other",
                entry.id, partner.id
            ));
            continue;
        }
        let source = entry.side_source.as_deref().unwrap_or("");
        if source == "convention" {
            gate.unknowns.push(format!(
                "{} and {}: the sides were assigned by convention, so which side is left was not measured",
                entry.id, partner.id
            ));
            continue;
        }
        if entry.facing.as_deref() == Some("side") {
            gate.warnings.push(format!(
                "{} and {}: a side view has no left and right on screen, so the sign was not checked",
                entry.id, partner.id
            ));
            continue;
        }
        match (world_x(doc, video, &l), world_x(doc, video, &r)) {
            (Some(lx), Some(rx)) if lx > rx => {}
            (Some(lx), Some(rx)) => gate.failures.push(format!(
                "{} and {}: the left part is at x {lx:.1} and the right at {rx:.1}, so the left one is not on the viewer's right",
                entry.id, partner.id
            )),
            _ => gate.unknowns.push(format!(
                "{} and {}: the parts were not in a scene the video shows, so their sides could not be placed",
                entry.id, partner.id
            )),
        }
    }
    gate
}

/// The plan entry that goes with `entry` on the other side.
fn partner_of<'a>(
    plan: &'a [LayerEntry],
    entry: &LayerEntry,
    used: &[&str],
) -> Option<&'a LayerEntry> {
    if let Some(mirror) = entry.mirror_of.as_deref() {
        return plan.iter().find(|e| e.id == mirror);
    }
    let want = match entry.side.as_deref() {
        Some("l") => "r",
        Some("r") => "l",
        _ => return None,
    };
    plan.iter()
        .find(|e| e.side.as_deref() == Some(want) && !used.contains(&e.id.as_str()))
}

/// The middle of a node's box on screen, at the start of the video.
fn world_x(doc: &Document, video: &Video, id: &NodeId) -> Option<f64> {
    let (frame, _) = frame_of(doc, video, id)?;
    let p = place(doc, &frame, id)?;
    Some(p.rect.x + p.rect.width / 2.0)
}

/// The part coverage gate: every planned layer is a node, no two are the same node, and the
/// nodes nobody planned are named.
pub fn coverage_gate(
    doc: &Document,
    video: &Video,
    plan: Option<&[LayerEntry]>,
    details: Option<&[Detail]>,
) -> Gate {
    let mut gate = Gate::new("coverage", Calibration::Derived);
    let Some(plan) = plan else {
        gate.unknowns
            .push("no layer plan was given, so nothing could be matched to a node".into());
        return gate;
    };
    let mut owned: Vec<NodeId> = Vec::new();
    for entry in plan.iter().filter(|e| !e.ignore) {
        match resolve(doc, entry) {
            Some((id, how)) => {
                if owned.contains(&id) {
                    gate.failures.push(format!(
                        "{}: two plan entries name the same node ({id}, found by {how}), so two layers are one",
                        entry.id
                    ));
                } else {
                    owned.push(id);
                }
            }
            None => gate.failures.push(format!(
                "{}: no node in the document is called {:?}",
                entry.id,
                entry.node_ref.as_deref().or(entry.name.as_deref())
            )),
        }
    }
    let mut unowned: Vec<String> = Vec::new();
    for node in nodes_of_scenes(doc, video) {
        if !owned.contains(&node.id)
            && !plan
                .iter()
                .any(|e| e.ignore && resolve(doc, e).is_some_and(|(id, _)| id == node.id))
        {
            unowned.push(format!("{} ({})", node.id, node.name));
        }
    }
    if !unowned.is_empty() {
        gate.warnings.push(format!(
            "{} nodes belong to no planned layer: {}",
            unowned.len(),
            unowned
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for d in details.unwrap_or(&[]) {
        let Some(m) = &d.maps_to else {
            gate.warnings
                .push(format!("{}: the detail maps to nothing", d.id));
            continue;
        };
        let Some(reference) = m.reference.as_deref() else {
            gate.warnings
                .push(format!("{}: the detail maps to nothing", d.id));
            continue;
        };
        let known = match m.r#type.as_deref() {
            Some("track") => video
                .animations
                .iter()
                .any(|a| a.node.as_str() == reference),
            _ => od_core::NodeId::parse(reference).is_ok_and(|id| doc.node(&id).is_some()),
        };
        if !known {
            gate.warnings.push(format!(
                "{}: the detail maps to {:?}, which is not in the document",
                d.id, reference
            ));
        }
    }
    gate.data = json!({"layers": plan.len(), "unowned": unowned.len()});
    gate
}

/// Every node of the scenes the video shows, in document order.
fn nodes_of_scenes<'a>(doc: &'a Document, video: &Video) -> Vec<&'a Node> {
    let mut out = Vec::new();
    for (frame, _, _) in super::shown_frames(video) {
        if let Some(n) = doc.node(&frame) {
            let mut all = Vec::new();
            super::subtree(n, &mut all);
            out.extend(all.into_iter().skip(1));
        }
    }
    out
}

/// The motion roster: every layer that is meant to move has a track, and every track drives a
/// node somebody planned.
pub fn motion_roster_gate(doc: &Document, video: &Video, plan: Option<&[LayerEntry]>) -> Gate {
    let mut gate = Gate::new("motion_roster", Calibration::Derived);
    let Some(plan) = plan else {
        gate.unknowns
            .push("no layer plan was given, so no roster could be checked".into());
        return gate;
    };
    let mut planned: Vec<NodeId> = Vec::new();
    for entry in plan {
        if let Some((id, _)) = resolve(doc, entry) {
            planned.push(id);
        }
    }
    for entry in plan.iter().filter(|e| e.expects_motion.is_some()) {
        let Some((id, _)) = resolve(doc, entry) else {
            continue;
        };
        let has = video.animations.iter().any(|a| {
            !a.prop.is_camera()
                && (a.node == id || super::geometry::is_ancestor_of(doc, &a.node, &id))
        });
        if !has {
            gate.failures.push(format!(
                "{}: the plan expects {:?} but no track moves it",
                entry.id,
                entry.expects_motion.as_deref().unwrap_or("")
            ));
        }
    }
    for a in &video.animations {
        if a.prop.is_camera() {
            continue;
        }
        if !planned.contains(&a.node) && !plan.iter().any(|e| e.ignore) {
            gate.warnings.push(format!(
                "{}: a track moves {} but no planned layer names it",
                a.prop.as_str(),
                a.node
            ));
        }
    }
    gate
}

/// The hash of a node's shape: what it is, where it is, how big, its path, its view box, its
/// fill rule and its children in order. The bytes are the numbers' bit patterns, so a change
/// that only shows in the last decimal is still a change.
pub fn hash_node(node: &Node, children: &[NodeId]) -> String {
    let mut h = Sha256::new();
    h.update(node.kind.as_str().as_bytes());
    h.update([0u8]);
    for v in [node.x, node.y, node.width, node.height] {
        h.update(v.to_bits().to_le_bytes());
    }
    h.update(node.path.as_deref().unwrap_or("").as_bytes());
    h.update([0u8]);
    match node.view_box {
        Some(vb) => {
            for v in [vb.x, vb.y, vb.width, vb.height] {
                h.update(v.to_bits().to_le_bytes());
            }
        }
        None => h.update(b"no view box"),
    }
    h.update([match node.fill_rule {
        Some(od_core::FillRule::Evenodd) => 1u8,
        Some(od_core::FillRule::Nonzero) => 0u8,
        None => 2u8,
    }]);
    for c in children {
        h.update(c.as_str().as_bytes());
        h.update([0u8]);
    }
    let out = h.finalize();
    out.iter().map(|b| format!("{b:02x}")).collect()
}

/// The attributes of a node, as they are shown when they differ.
pub fn attributes_of(node: &Node, children: &[NodeId]) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("kind".into(), node.kind.as_str().into());
    m.insert("x".into(), format!("{:?}", node.x));
    m.insert("y".into(), format!("{:?}", node.y));
    m.insert("width".into(), format!("{:?}", node.width));
    m.insert("height".into(), format!("{:?}", node.height));
    m.insert(
        "path".into(),
        node.path.as_deref().unwrap_or("").to_string(),
    );
    m.insert(
        "view_box".into(),
        match node.view_box {
            Some(vb) => format!("{:?},{:?},{:?},{:?}", vb.x, vb.y, vb.width, vb.height),
            None => String::new(),
        },
    );
    m.insert(
        "fill_rule".into(),
        match node.fill_rule {
            Some(od_core::FillRule::Evenodd) => "evenodd".into(),
            Some(od_core::FillRule::Nonzero) => "nonzero".into(),
            None => String::new(),
        },
    );
    m.insert(
        "children".into(),
        children
            .iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(","),
    );
    m
}

/// The shapes of the video's scenes, hashed.
pub fn snapshot(doc: &Document, video: &Video, added_by: &BTreeMap<String, String>) -> Snapshot {
    let mut out = Snapshot::default();
    for (frame, _, _) in super::shown_frames(video) {
        let Some(node) = doc.node(&frame) else {
            continue;
        };
        let mut all = Vec::new();
        super::subtree(node, &mut all);
        for n in &all {
            let children: Vec<NodeId> = n
                .children
                .iter()
                .filter(|c| !added_by.contains_key(c.id.as_str()))
                .map(|c| c.id.clone())
                .collect();
            out.hashes
                .insert(n.id.as_str().to_string(), hash_node(n, &children));
            out.attributes
                .insert(n.id.as_str().to_string(), attributes_of(n, &children));
        }
    }
    out
}

/// The freeze gate: the shapes have not changed since they were frozen, except for the nodes a
/// later pass added.
pub fn freeze_gate(
    doc: &Document,
    video: &Video,
    freeze: &Freeze,
    added_by: &BTreeMap<String, String>,
) -> Gate {
    let mut gate = Gate::new("freeze", Calibration::Derived);
    let now = snapshot(doc, video, added_by);
    for (id, hash) in &freeze.hashes {
        match now.hashes.get(id) {
            None => gate
                .failures
                .push(format!("{id}: the node was frozen and is gone now")),
            Some(current) if current != hash => {
                let old = freeze.attributes.get(id);
                let current_attrs = now.attributes.get(id);
                let mut named = Vec::new();
                if let (Some(old), Some(current)) = (old, current_attrs) {
                    for (key, value) in old {
                        if current.get(key) != Some(value) {
                            named.push(format!(
                                "{key} was {value}, now {}",
                                current.get(key).cloned().unwrap_or_else(|| "gone".into())
                            ));
                        }
                    }
                }
                gate.failures.push(format!(
                    "{id}: the frozen shape changed ({})",
                    if named.is_empty() {
                        "the hash is different".to_string()
                    } else {
                        named.join("; ")
                    }
                ));
            }
            Some(_) => {}
        }
    }
    for id in now.hashes.keys() {
        if !freeze.hashes.contains_key(id) && !added_by.contains_key(id) {
            gate.failures.push(format!(
                "{id}: a node that was not frozen appeared (a later pass may only add nodes it names)"
            ));
        }
    }
    gate.data = json!({"frozen": freeze.hashes.len(), "now": now.hashes.len()});
    gate
}

/// The moments a geometry check looks at: the keyframe times of the chains involved, plus 12
/// even moments of the video.
pub fn sample_times(doc: &Document, video: &Video, ids: &[NodeId]) -> Vec<f64> {
    let mut times: Vec<f64> = Vec::new();
    for id in ids {
        for a in &video.animations {
            if &a.node == id || super::geometry::is_ancestor_of(doc, &a.node, id) {
                for k in &a.keys {
                    times.push(k.at_ms as f64);
                }
            }
        }
    }
    let end = last_ms(video);
    times.extend(spread(0.0, end, 12));
    tidy_times(times)
}

/// The frames of a video's scenes, used by the geometry gates.
pub fn frames_of(video: &Video) -> Vec<NodeId> {
    super::shown_frames(video)
        .into_iter()
        .map(|(f, _, _)| f)
        .collect()
}

/// The height of a scene's frame, in document units.
pub fn scene_height(doc: &Document, video: &Video) -> f64 {
    frame_height(doc, video)
}

/// The kinds of node a plan can name.
pub fn kinds() -> &'static [NodeKind] {
    &[
        NodeKind::Frame,
        NodeKind::Group,
        NodeKind::Rect,
        NodeKind::Ellipse,
        NodeKind::Text,
        NodeKind::Image,
        NodeKind::Device3d,
        NodeKind::Path,
    ]
}

/// The document as it is at a moment, for the gates that need it.
pub fn timeline<'a>(doc: &'a Document, video: &'a Video) -> Timeline<'a> {
    Timeline::new(doc, video)
}

//! Flat, serializable views of a document for panels: the layers list and the frames a user can
//! open. Shells present these; they do not walk the tree themselves.

use serde::Serialize;

use crate::model::{Document, Node, NodeId, NodeKind};

/// One row of the layers panel. Pages and nodes share the list; a page row has kind `page`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LayerRow {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub depth: usize,
    pub visible: bool,
    pub page: String,
    pub has_children: bool,
}

/// The layers in panel order: pages in order, each node before its children, the topmost layer
/// (last child) first, as design tools list them.
pub fn layers(doc: &Document) -> Vec<LayerRow> {
    let mut rows = Vec::new();
    for page in &doc.pages {
        rows.push(LayerRow {
            id: page.id.to_string(),
            kind: "page",
            name: page.name.clone(),
            depth: 0,
            visible: true,
            page: page.id.to_string(),
            has_children: !page.root.children.is_empty(),
        });
        for child in page.root.children.iter().rev() {
            push_node(child, 1, &page.id.to_string(), &mut rows);
        }
    }
    rows
}

fn push_node(node: &Node, depth: usize, page: &str, rows: &mut Vec<LayerRow>) {
    rows.push(LayerRow {
        id: node.id.to_string(),
        kind: node.kind.as_str(),
        name: node.name.clone(),
        depth,
        visible: node.visible,
        page: page.to_string(),
        has_children: !node.children.is_empty(),
    });
    for child in node.children.iter().rev() {
        push_node(child, depth + 1, page, rows);
    }
}

/// A frame that can be opened on the canvas and exported.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameRow {
    pub id: String,
    pub name: String,
    pub page: String,
    pub width: f64,
    pub height: f64,
}

/// Every frame in the document, pages in order, frames in drawing order.
pub fn frames(doc: &Document) -> Vec<FrameRow> {
    fn walk(node: &Node, page: &str, out: &mut Vec<FrameRow>) {
        if node.kind == NodeKind::Frame {
            out.push(FrameRow {
                id: node.id.to_string(),
                name: node.name.clone(),
                page: page.to_string(),
                width: node.width,
                height: node.height,
            });
        }
        for c in &node.children {
            walk(c, page, out);
        }
    }
    let mut out = Vec::new();
    for page in &doc.pages {
        walk(&page.root, &page.id.to_string(), &mut out);
    }
    out
}

/// A design token with how many nodes use it, for the tokens panel.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TokenRow {
    pub name: String,
    pub value: String,
    pub used_by: usize,
}

/// Every token, sorted by name.
pub fn tokens(doc: &Document) -> Vec<TokenRow> {
    doc.tokens
        .iter()
        .map(|(name, value)| TokenRow {
            name: name.clone(),
            value: value.clone(),
            used_by: doc.token_users(name).len(),
        })
        .collect()
}

/// A text node with its default text, for the translation worklist.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextRow {
    pub id: String,
    pub name: String,
    pub text: String,
}

/// Every text node in the document, in layer order (pages in order, parents before children).
pub fn text_nodes(doc: &Document) -> Vec<TextRow> {
    fn walk(n: &Node, out: &mut Vec<TextRow>) {
        if n.kind == NodeKind::Text {
            out.push(TextRow {
                id: n.id.to_string(),
                name: n.name.clone(),
                text: n.text.clone().unwrap_or_default(),
            });
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    for page in &doc.pages {
        walk(&page.root, &mut out);
    }
    out
}

/// One language and how much of the document it covers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocaleRow {
    pub locale: String,
    /// Text nodes that have a translation in this language.
    pub translated: usize,
    /// Text nodes that still show the default text in this language.
    pub missing: Vec<String>,
}

/// The languages of the document, sorted by tag, with what each still lacks. Translations of
/// nodes that no longer exist are not counted.
pub fn locales(doc: &Document) -> Vec<LocaleRow> {
    let texts = text_nodes(doc);
    doc.locales
        .iter()
        .map(|(locale, map)| {
            let missing: Vec<String> = texts
                .iter()
                .filter(|t| NodeId::parse(&t.id).is_ok_and(|id| !map.contains_key(&id)))
                .map(|t| t.id.clone())
                .collect();
            LocaleRow {
                locale: locale.clone(),
                translated: texts.len() - missing.len(),
                missing,
            }
        })
        .collect()
}

/// One clip of a video, placed on its timeline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClipRow {
    pub id: String,
    /// The design frame shown (empty for footage).
    /// The design frame shown (empty for footage).
    pub frame: String,
    pub frame_name: String,
    /// The footage shown (`media/<name>`), or empty.
    pub footage: String,
    pub footage_in_ms: u64,
    pub start_ms: u64,
    pub duration_ms: u32,
    pub transition: crate::video::Transition,
    /// How long the transition into this clip really lasts.
    pub transition_ms: u32,
    pub motion: crate::video::Motion,
}

/// A video with its clips placed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VideoRow {
    pub id: String,
    pub name: String,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    pub frame_count: u64,
    pub clips: Vec<ClipRow>,
    pub overlays: Vec<crate::video::Overlay>,
    /// The sound under the picture (none for a silent video).
    pub audio: Option<AudioRow>,
    /// What happens, in order, in words: the clips with their motion and how each one arrives,
    /// then the overlays by start time.
    pub story: Vec<String>,
}

/// The sound track of a video.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AudioRow {
    pub file: String,
    pub start_ms: u32,
    pub gain: f32,
    pub fade_in_ms: u32,
    pub fade_out_ms: u32,
}

fn secs(ms: u64) -> String {
    format!("{:.1} s", ms as f64 / 1000.0)
}

fn kind_name<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The video as a list of sentences: what is on screen when, and what leads into what.
fn story_of(doc: &Document, v: &crate::video::Video) -> Vec<String> {
    let placed = v.placed();
    let mut out = Vec::new();
    for (clip, p) in v.clips.iter().zip(&placed) {
        let what = match (&clip.frame, &clip.footage) {
            (Some(f), _) => format!(
                "frame {}",
                doc.node(f)
                    .map_or_else(|| f.to_string(), |n| n.name.clone())
            ),
            (None, Some(f)) => format!("footage {} from {}", f.file, secs(f.in_ms)),
            (None, None) => "nothing".to_string(),
        };
        let arrive = if p.index > 0 && p.overlap_ms > 0 {
            format!(
                "{} {} over {} ({}), then ",
                secs(p.start_ms),
                kind_name(&clip.transition.kind),
                secs(u64::from(p.overlap_ms)),
                kind_name(&clip.transition.ease)
            )
        } else if p.index > 0 {
            format!("{} cut to ", secs(p.start_ms))
        } else {
            format!("{} starts with ", secs(p.start_ms))
        };
        let zoom = if (clip.motion.scale_from - clip.motion.scale_to).abs() > f64::EPSILON {
            format!(
                ", zooming {:.2} to {:.2} ({})",
                clip.motion.scale_from,
                clip.motion.scale_to,
                kind_name(&clip.motion.ease)
            )
        } else {
            String::new()
        };
        out.push(format!(
            "{arrive}{what} for {}{zoom} (until {})",
            secs(u64::from(clip.duration_ms)),
            secs(p.end_ms)
        ));
    }
    let mut overlays: Vec<&crate::video::Overlay> = v.overlays.iter().collect();
    overlays.sort_by_key(|o| o.start_ms);
    for o in overlays {
        let name = doc
            .node(&o.frame)
            .map_or_else(|| o.frame.to_string(), |n| n.name.clone());
        out.push(format!(
            "{} overlay {name}: in {} ({}), out {}, until {}",
            secs(o.start_ms),
            kind_name(&o.enter),
            secs(u64::from(o.fade_ms)),
            kind_name(&o.exit),
            secs(o.start_ms + u64::from(o.duration_ms))
        ));
    }
    if let Some(a) = &v.audio {
        let fades = match (a.fade_in_ms, a.fade_out_ms) {
            (0, 0) => String::new(),
            (i, o) => format!(
                ", fading in over {} and out over {}",
                secs(u64::from(i)),
                secs(u64::from(o))
            ),
        };
        out.push(format!(
            "sound: {} from {} at volume {:.2}{fades}",
            a.file,
            secs(u64::from(a.start_ms)),
            a.gain
        ));
    }
    out
}

/// Every video with its clips placed on the timeline.
pub fn videos(doc: &Document) -> Vec<VideoRow> {
    doc.videos
        .iter()
        .map(|v| {
            let placed = v.placed();
            VideoRow {
                id: v.id.to_string(),
                name: v.name.clone(),
                fps: v.fps,
                width: v.width,
                height: v.height,
                duration_ms: v.duration_ms(),
                frame_count: v.frame_count(),
                clips: v
                    .clips
                    .iter()
                    .zip(&placed)
                    .map(|(c, p)| ClipRow {
                        id: c.id.to_string(),
                        frame: c
                            .frame
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_default(),
                        frame_name: c
                            .frame
                            .as_ref()
                            .and_then(|f| doc.node(f))
                            .map(|n| n.name.clone())
                            .unwrap_or_default(),
                        footage: c
                            .footage
                            .as_ref()
                            .map(|f| f.file.clone())
                            .unwrap_or_default(),
                        footage_in_ms: c.footage.as_ref().map_or(0, |f| f.in_ms),
                        start_ms: p.start_ms,
                        duration_ms: c.duration_ms,
                        transition: c.transition,
                        transition_ms: p.overlap_ms,
                        motion: c.motion,
                    })
                    .collect(),
                overlays: v.overlays.clone(),
                audio: v.audio.as_ref().map(|a| AudioRow {
                    file: a.file.clone(),
                    start_ms: a.start_ms,
                    gain: a.gain,
                    fade_in_ms: a.fade_in_ms,
                    fade_out_ms: a.fade_out_ms,
                }),
                story: story_of(doc, v),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Actor, NewNode, Operation, Session};

    #[test]
    fn layers_list_pages_then_nodes_topmost_first_with_depth() {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("Home").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut frame = NewNode::new(NodeKind::Frame);
        frame.name = Some("Frame".into());
        frame.width = 10.0;
        frame.height = 10.0;
        for name in ["bottom", "top"] {
            let mut r = NewNode::new(NodeKind::Rect);
            r.name = Some(name.into());
            frame.children.push(r);
        }
        s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: frame,
            }],
        )
        .unwrap();
        let rows = layers(s.document());
        let shape: Vec<(&str, usize)> = rows.iter().map(|r| (r.name.as_str(), r.depth)).collect();
        assert_eq!(
            shape,
            vec![("Home", 0), ("Frame", 1), ("top", 2), ("bottom", 2)]
        );
        assert_eq!(rows[0].kind, "page");
        assert!(rows[1].has_children && !rows[2].has_children);
        let f = frames(s.document());
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].name.as_str(), f[0].width), ("Frame", 10.0));
    }

    #[test]
    fn tokens_list_by_name_with_their_users() {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        s.set_token("b.two", "#000000").unwrap();
        s.set_token("a.one", "#ffffff").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut n = NewNode::new(crate::NodeKind::Rect);
        n.fill = Some(crate::Paint::parse("{a.one}").unwrap());
        s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: n,
            }],
        )
        .unwrap();
        let rows = tokens(s.document());
        assert_eq!(
            rows.iter()
                .map(|r| (r.name.as_str(), r.used_by))
                .collect::<Vec<_>>(),
            [("a.one", 1), ("b.two", 0)]
        );
    }
}

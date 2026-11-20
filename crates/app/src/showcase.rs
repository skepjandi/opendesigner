//! What the Preview and Variants panels show, resolved against the open workspace. The agent's
//! choices live in `showcase.json` (`od_agent::showcase`); this adds the user's session-only
//! override of the preview, the default when nobody chose, and clean frame thumbnails.

use std::collections::HashMap;
use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use od_agent::showcase::{Kind, check_target};
use od_core::{NodeId, Workspace};
use serde_json::{Value, json};

const MAX_THUMB_SIDE: u32 = 2048;
const MAX_CACHED: usize = 64;

/// The user's own pick of what the Preview shows (until they hand the choice back), and the
/// thumbnails drawn for the document as it was at `revision`.
#[derive(Default)]
pub struct ShowcaseSession {
    pub preview_override: Option<(Kind, String)>,
    cache: Mutex<Cache>,
}

#[derive(Default)]
struct Cache {
    revision: usize,
    frames: HashMap<(String, u32, Option<String>), String>,
}

impl ShowcaseSession {
    pub fn reset(&mut self) {
        self.preview_override = None;
        if let Ok(mut c) = self.cache.lock() {
            c.frames.clear();
        }
    }

    /// A PNG data URL of any frame as exported (no selection, handles or pins), at most
    /// `max_side` pixels on its longer side. Drawn again only after the document changed.
    pub fn frame_image(
        &self,
        ws: &Workspace,
        language: Option<&str>,
        frame: &str,
        max_side: u32,
    ) -> Result<String, String> {
        let id = NodeId::parse(frame).map_err(|e| e.to_string())?;
        let side = max_side.clamp(16, MAX_THUMB_SIDE);
        let revision = ws.session.log().len();
        let key = (frame.to_string(), side, language.map(str::to_string));
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "the thumbnail cache is busy")?;
        if cache.revision != revision || cache.frames.len() >= MAX_CACHED {
            cache.frames.clear();
            cache.revision = revision;
        }
        if let Some(url) = cache.frames.get(&key) {
            return Ok(url.clone());
        }
        let localized = language.and_then(|l| ws.session.document().localized(l));
        let doc = localized.as_ref().unwrap_or_else(|| ws.session.document());
        let node = doc.node(&id).ok_or("the frame no longer exists")?;
        let scale = f64::from(side) / node.width.max(node.height).max(1.0);
        let rendered = od_render::render_frame(doc, &id, scale.min(16.0) as f32, ws.dir())
            .map_err(|e| e.to_string())?;
        let png = od_render::encode_png(&rendered.pixmap, true).map_err(|e| e.to_string())?;
        let url = format!("data:image/png;base64,{}", STANDARD.encode(png));
        cache.frames.insert(key, url.clone());
        Ok(url)
    }
}

/// What a frame, video or image is called and how big it is, or `None` when it is gone.
fn describe(ws: &Workspace, kind: Kind, target: &str) -> Option<Value> {
    let doc = ws.session.document();
    match kind {
        Kind::Frame => {
            let n = doc.node(&NodeId::parse(target).ok()?)?;
            Some(json!({"name": n.name, "width": n.width, "height": n.height}))
        }
        Kind::Video => {
            let v = doc.videos.iter().find(|v| v.id.as_str() == target)?;
            Some(json!({"name": v.name, "width": v.width, "height": v.height}))
        }
        Kind::Image => {
            let path = ws.media_path(target).ok()?;
            let (w, h) = od_render::image_dimensions(&path).ok()?;
            Some(
                json!({"name": target.trim_start_matches("media/"), "width": w, "height": h,
                "file": path.display().to_string()}),
            )
        }
    }
}

fn with_info(ws: &Workspace, kind: Kind, target: &str, mut base: Value) -> Value {
    let info = describe(ws, kind, target);
    if let (Value::Object(m), Some(Value::Object(i))) = (&mut base, &info) {
        m.extend(i.clone());
    }
    base["exists"] = json!(info.is_some());
    base
}

/// Everything the Variants and Preview panels need, as one JSON value.
pub fn view(ws: &Workspace, session: &ShowcaseSession, active_frame: Option<&NodeId>) -> Value {
    let Some(dir) = ws.dir() else {
        return Value::Null;
    };
    let saved = od_agent::showcase::load(dir);
    let variants = saved.variants.as_ref().map(|set| {
        let mut v = serde_json::to_value(set).unwrap_or(Value::Null);
        if let Some(options) = v["options"].as_array_mut() {
            for (o, src) in options.iter_mut().zip(&set.options) {
                *o = with_info(ws, src.kind, &src.target, o.clone());
            }
        }
        v
    });
    json!({
        "variants": variants,
        "agent_preview": saved.preview,
        "preview": shown(ws, session, &saved, active_frame),
        "choices": choices(ws),
    })
}

/// What the Preview shows, and who chose it: the user's override, else the agent's
/// `show_preview`, else the newest video, else the open frame.
fn shown(
    ws: &Workspace,
    session: &ShowcaseSession,
    saved: &od_agent::showcase::Showcase,
    active_frame: Option<&NodeId>,
) -> Value {
    let exists = |kind: Kind, target: &str| describe(ws, kind, target).is_some();
    if let Some((kind, target)) = &session.preview_override
        && exists(*kind, target)
    {
        return with_info(
            ws,
            *kind,
            target,
            json!({"kind": kind, "target": target, "chosen_by": "user"}),
        );
    }
    if let Some(p) = saved.preview.as_ref().filter(|p| exists(p.kind, &p.target)) {
        let base = json!({"kind": p.kind, "target": p.target, "title": p.title,
            "note": p.note, "chosen_by": "agent", "agent": p.by, "seq": p.seq});
        return with_info(ws, p.kind, &p.target, base);
    }
    let doc = ws.session.document();
    let fallback = doc
        .videos
        .last()
        .map(|v| (Kind::Video, v.id.to_string()))
        .or_else(|| active_frame.map(|f| (Kind::Frame, f.to_string())))
        .filter(|(k, t)| exists(*k, t));
    match fallback {
        Some((kind, target)) => with_info(
            ws,
            kind,
            &target,
            json!({"kind": kind, "target": target, "chosen_by": "default"}),
        ),
        None => Value::Null,
    }
}

/// Everything the user can switch the Preview to.
fn choices(ws: &Workspace) -> Value {
    let doc = ws.session.document();
    let mut out: Vec<Value> = od_core::view::frames(doc)
        .into_iter()
        .map(|f| json!({"kind": "frame", "target": f.id, "name": f.name, "width": f.width, "height": f.height}))
        .collect();
    out.extend(doc.videos.iter().map(|v| {
        json!({"kind": "video", "target": v.id, "name": v.name, "width": v.width, "height": v.height})
    }));
    out.extend(ws.media().unwrap_or_default().into_iter().filter(|f| {
        let ext = f.path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif")
    }).map(|f| json!({"kind": "image", "target": f.path, "name": f.path.trim_start_matches("media/")})));
    Value::Array(out)
}

/// Checks the user's choice of preview before keeping it.
pub fn check_override(ws: &Workspace, kind: &str, target: &str) -> Result<(Kind, String), String> {
    let kind: Kind = serde_json::from_value(json!(kind))
        .map_err(|_| format!("unknown kind {kind:?}; use frame, video or image"))?;
    check_target(ws, kind, target).map_err(|e| e.to_string())?;
    Ok((kind, target.to_string()))
}

/// The toast and the panel to reveal after an agent's showcase tool call.
pub fn notice(ws: &Workspace, tool: &str, who: &str) -> Option<(&'static str, String)> {
    let saved = od_agent::showcase::load(ws.dir()?);
    match tool {
        "offer_variants" => {
            let set = saved.variants?;
            Some((
                "variants",
                format!(
                    "{who} offered {} options: pick one in Variants",
                    set.options.len()
                ),
            ))
        }
        "show_preview" => Some(("preview", format!("{who} updated the Preview"))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use od_core::{Actor, NewNode, NodeKind, Operation};

    fn two_frames() -> (tempfile::TempDir, Workspace, Vec<NodeId>) {
        let tmp = tempfile::tempdir().unwrap();
        let mut ws = Workspace::create(&tmp.path().join("p.od"), "S").unwrap();
        let root = ws.session.document().pages[0].root.id.clone();
        let ops: Vec<Operation> = [(200.0, 100.0), (50.0, 80.0)]
            .iter()
            .map(|(w, h)| {
                let mut n = NewNode::new(NodeKind::Frame);
                n.width = *w;
                n.height = *h;
                Operation::Create {
                    parent: root.clone(),
                    index: None,
                    node: n,
                }
            })
            .collect();
        let ids = ws.session.apply(Actor::User, None, &ops).unwrap().created;
        (tmp, ws, ids)
    }

    fn png_size(url: &str) -> (u32, u32) {
        let b = STANDARD
            .decode(url.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        (
            u32::from_be_bytes(b[16..20].try_into().unwrap()),
            u32::from_be_bytes(b[20..24].try_into().unwrap()),
        )
    }

    #[test]
    fn any_frame_renders_clean_to_the_asked_size_and_is_cached_until_an_edit() {
        let (_t, mut ws, ids) = two_frames();
        let s = ShowcaseSession::default();
        let url = s.frame_image(&ws, None, ids[0].as_str(), 100).unwrap();
        assert_eq!(png_size(&url), (100, 50));
        assert_eq!(
            png_size(&s.frame_image(&ws, None, ids[1].as_str(), 40).unwrap()),
            (25, 40)
        );
        assert!(s.cache.lock().unwrap().frames.len() == 2);
        ws.session
            .apply(
                Actor::User,
                None,
                &[Operation::Move {
                    id: ids[0].clone(),
                    x: 5.0,
                    y: 5.0,
                }],
            )
            .unwrap();
        s.frame_image(&ws, None, ids[0].as_str(), 100).unwrap();
        assert_eq!(
            s.cache.lock().unwrap().frames.len(),
            1,
            "an edit drops what was drawn before"
        );
        assert!(s.frame_image(&ws, None, "n_99999", 100).is_err());
    }

    #[test]
    fn the_preview_follows_override_then_agent_then_default() {
        let (_t, ws, ids) = two_frames();
        let dir = ws.dir().unwrap().to_path_buf();
        let mut s = ShowcaseSession::default();
        let pick = |s: &ShowcaseSession| view(&ws, s, Some(&ids[1]))["preview"].clone();
        assert_eq!(pick(&s)["chosen_by"], "default");
        assert_eq!(pick(&s)["target"], ids[1].as_str());
        let mut saved = od_agent::showcase::Showcase::default();
        saved
            .set_preview(
                &ws,
                Kind::Frame,
                ids[0].as_str(),
                None,
                Some("final"),
                "claude",
            )
            .unwrap();
        od_agent::showcase::save(&dir, &saved).unwrap();
        let p = pick(&s);
        assert_eq!(
            (
                p["chosen_by"].as_str(),
                p["note"].as_str(),
                p["width"].as_f64()
            ),
            (Some("agent"), Some("final"), Some(200.0))
        );
        s.preview_override = Some(check_override(&ws, "frame", ids[1].as_str()).unwrap());
        assert_eq!(pick(&s)["chosen_by"], "user");
        s.preview_override = None;
        assert_eq!(pick(&s)["target"], ids[0].as_str());
        assert!(check_override(&ws, "frame", "n_99999").is_err());
        assert!(check_override(&ws, "movie", ids[0].as_str()).is_err());
    }

    #[test]
    fn variants_come_with_names_and_sizes_and_a_gone_frame_is_marked() {
        let (_t, mut ws, ids) = two_frames();
        let dir = ws.dir().unwrap().to_path_buf();
        let mut saved = od_agent::showcase::Showcase::default();
        let opts = [&ids[0], &ids[1]].map(|id| od_agent::showcase::NewOption {
            label: "L".into(),
            kind: Kind::Frame,
            target: id.to_string(),
            note: None,
        });
        saved.offer(&ws, "T", None, &opts, "x").unwrap();
        od_agent::showcase::save(&dir, &saved).unwrap();
        let v = view(&ws, &ShowcaseSession::default(), None);
        assert_eq!(v["variants"]["options"][1]["height"], 80.0);
        ws.session
            .apply(
                Actor::User,
                None,
                &[Operation::Delete { id: ids[1].clone() }],
            )
            .unwrap();
        let v = view(&ws, &ShowcaseSession::default(), None);
        assert_eq!(v["variants"]["options"][1]["exists"], false);
        assert_eq!(v["variants"]["options"][0]["exists"], true);
    }
}

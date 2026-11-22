//! Whether the agent has looked at what it made (`docs/wiki/agent-integration.md#pixel-verified`).
//!
//! A change that was applied is not a change that looks right. Every tool that changes the
//! picture answers with `pixel_verified: false` until the agent has called a tool that draws a
//! picture it can see (a render, a review, a contact sheet), and the tools that hand work over
//! (the exports) warn when changes have gone unseen. Nothing is refused: a script may know
//! what it is doing. The point is that an agent can no longer say "done" about something it
//! never looked at without having been told so by its own tool results.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde_json::{Map, Value, json};

use crate::tools::{Content, Output};

/// Tools that draw a picture for the agent to see.
const LOOKS: &[&str] = &[
    "render_preview",
    "review_frame",
    "render_video_frame",
    "video_contact_sheet",
    "video_strip",
    "video_crop",
    "frame_grid",
    "video_diff",
];

/// Tools that hand the work over.
const HAND_OVERS: &[&str] = &[
    "export",
    "export_set",
    "export_panorama",
    "export_video",
    "batch_export",
    "export_icon_set",
    "show_preview",
];

/// Tools that write but do not change what is drawn.
const NOT_VISUAL: &[&str] = &[
    "write_memory",
    "make_sound",
    "import_sound",
    "import_file",
    "import_captions",
    "install_asset_pack",
    "unload_asset_pack",
    "remove_background",
    "resolve_annotation",
    "offer_variants",
    "clear_variants",
    "create_workspace",
    "open_workspace",
    "add_sound_track",
    "set_sound_track",
    "remove_sound_track",
    "sound_credits",
    "scene_spec_new",
    "record_review",
    "set_commercial",
    "import_asset",
    "make_asset",
];

/// How many changes went by since the agent last looked at a render.
#[derive(Debug, Default)]
pub struct Looks {
    unseen: u32,
}

fn changing_tools() -> &'static HashSet<String> {
    static CHANGING: OnceLock<HashSet<String>> = OnceLock::new();
    CHANGING.get_or_init(|| {
        crate::tools::definitions()
            .iter()
            .filter(|d| d["annotations"]["readOnlyHint"] == json!(false))
            .filter_map(|d| d["name"].as_str().map(str::to_string))
            .collect()
    })
}

fn changes_picture(name: &str) -> bool {
    changing_tools().contains(name)
        && !LOOKS.contains(&name)
        && !HAND_OVERS.contains(&name)
        && !NOT_VISUAL.contains(&name)
}

fn object_of(out: &mut Output) -> Option<Map<String, Value>> {
    let Content::Text(text) = out.content.first()? else {
        return None;
    };
    match serde_json::from_str::<Value>(text).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

fn put_back(out: &mut Output, map: Map<String, Value>) {
    out.content[0] = Content::Text(Value::Object(map).to_string());
}

impl Looks {
    /// Notes what `name` did and marks its answer: a change says it is unverified, a look
    /// clears the count, a hand-over warns when there is something nobody looked at.
    pub fn annotate(&mut self, name: &str, mut out: Output) -> Output {
        if LOOKS.contains(&name) {
            self.unseen = 0;
            return out;
        }
        if changes_picture(name) {
            self.unseen = self.unseen.saturating_add(1);
            let first = self.unseen == 1;
            if let Some(mut map) = object_of(&mut out) {
                map.insert("pixel_verified".into(), json!(false));
                map.insert("changes_not_yet_seen".into(), json!(self.unseen));
                if first {
                    map.insert(
                        "look_next".into(),
                        json!(
                            "applied, not seen: render it before you call it done (render_video_frame or \
                             video_contact_sheet for a video, render_preview or review_frame for an image)"
                        ),
                    );
                }
                put_back(&mut out, map);
            }
        } else if HAND_OVERS.contains(&name)
            && self.unseen > 0
            && let Some(mut map) = object_of(&mut out)
        {
            map.insert(
                "warning".into(),
                json!(format!(
                    "{} change(s) went into this that you have not looked at since: render it \
                     (render_video_frame, video_contact_sheet or render_preview), fix what you \
                     see, then hand it over again",
                    self.unseen
                )),
            );
            put_back(&mut out, map);
        }
        out
    }

    pub fn unseen(&self) -> u32 {
        self.unseen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(map: Value) -> Output {
        Output::json(&map)
    }

    fn field(out: &Output, key: &str) -> Value {
        let Content::Text(text) = &out.content[0] else {
            panic!("text")
        };
        serde_json::from_str::<Value>(text).unwrap()[key].clone()
    }

    #[test]
    fn a_change_is_unverified_until_the_agent_looks() {
        let mut looks = Looks::default();
        let first = looks.annotate("apply_ops", answer(json!({"applied": 3})));
        assert_eq!(field(&first, "pixel_verified"), json!(false));
        assert_eq!(field(&first, "changes_not_yet_seen"), json!(1));
        assert!(field(&first, "look_next").is_string());
        let second = looks.annotate("add_block", answer(json!({"ok": true})));
        assert_eq!(field(&second, "changes_not_yet_seen"), json!(2));
        assert!(
            field(&second, "look_next").is_null(),
            "said once, not every time"
        );
        assert_eq!(looks.unseen(), 2);

        looks.annotate("render_video_frame", answer(json!({"ok": true})));
        assert_eq!(looks.unseen(), 0);
        let after = looks.annotate("apply_ops", answer(json!({"applied": 1})));
        assert_eq!(field(&after, "changes_not_yet_seen"), json!(1));
    }

    #[test]
    fn an_export_warns_about_changes_nobody_saw() {
        let mut looks = Looks::default();
        looks.annotate("animate_preset", answer(json!({"ok": true})));
        looks.annotate("apply_ops", answer(json!({"ok": true})));
        let sent = looks.annotate("export_video", answer(json!({"file": "exports/a.mp4"})));
        assert!(
            field(&sent, "warning")
                .as_str()
                .is_some_and(|w| w.starts_with("2 change(s)"))
        );
        looks.annotate("video_contact_sheet", answer(json!({})));
        let clean = looks.annotate("export_video", answer(json!({"file": "exports/a.mp4"})));
        assert!(field(&clean, "warning").is_null());
    }

    #[test]
    fn reading_and_sound_work_do_not_count_as_changes() {
        let mut looks = Looks::default();
        for name in ["get_tree", "list_videos", "write_memory", "make_sound"] {
            let out = looks.annotate(name, answer(json!({"ok": true})));
            assert!(field(&out, "pixel_verified").is_null(), "{name}");
        }
        assert_eq!(looks.unseen(), 0);
    }

    #[test]
    fn plain_text_answers_are_left_alone() {
        let mut looks = Looks::default();
        let out = looks.annotate("apply_ops", Output::json(&json!("done")));
        let Content::Text(text) = &out.content[0] else {
            panic!("text")
        };
        assert_eq!(text, "\"done\"");
        assert_eq!(looks.unseen(), 1);
    }
}

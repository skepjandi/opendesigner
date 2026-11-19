//! The formats a workspace can be made for, in sections the user picks from when starting one.
//! Choosing is optional: with no format the agent decides (`skill/formats.md`), and it needs the
//! user's approval to change one they chose (`change_format`).

use std::path::Path;

use serde::{Deserialize, Serialize};

pub struct Format {
    pub section: &'static str,
    pub id: &'static str,
    pub label: &'static str,
    /// `video` or `image`.
    pub kind: &'static str,
    pub width: u32,
    pub height: u32,
    pub fps: Option<u32>,
    pub note: &'static str,
}

const fn f(
    section: &'static str,
    id: &'static str,
    label: &'static str,
    kind: &'static str,
    (width, height): (u32, u32),
    fps: Option<u32>,
    note: &'static str,
) -> Format {
    Format {
        section,
        id,
        label,
        kind,
        width,
        height,
        fps,
        note,
    }
}

pub const FORMATS: &[Format] = &[
    f(
        "Video",
        "reel",
        "Reel / TikTok / Short, 9:16",
        "video",
        (1080, 1920),
        Some(30),
        "keep text inside the safe zone",
    ),
    f(
        "Video",
        "youtube",
        "YouTube / web, 16:9 (1080p)",
        "video",
        (1920, 1080),
        Some(30),
        "",
    ),
    f(
        "Video",
        "youtube-4k",
        "YouTube / web, 16:9 (4K)",
        "video",
        (3840, 2160),
        Some(30),
        "slow to export",
    ),
    f(
        "Video",
        "square-video",
        "Square video, 1:1",
        "video",
        (1080, 1080),
        Some(30),
        "",
    ),
    f(
        "Video",
        "apple-app-preview-iphone",
        "App Store app preview, iPhone portrait",
        "video",
        (886, 1920),
        Some(30),
        "15 to 30 s",
    ),
    f(
        "Video",
        "apple-app-preview-iphone-landscape",
        "App Store app preview, iPhone landscape",
        "video",
        (1920, 886),
        Some(30),
        "15 to 30 s",
    ),
    f(
        "Video",
        "apple-app-preview-ipad",
        "App Store app preview, iPad portrait",
        "video",
        (1200, 1600),
        Some(30),
        "15 to 30 s",
    ),
    f(
        "Store images",
        "apple-iphone-6.9",
        "App Store iPhone 6.9\" screenshot",
        "image",
        (1320, 2868),
        None,
        "no transparency",
    ),
    f(
        "Store images",
        "apple-iphone-6.3",
        "App Store iPhone 6.3\" screenshot",
        "image",
        (1206, 2622),
        None,
        "",
    ),
    f(
        "Store images",
        "apple-ipad-13",
        "App Store iPad 13\" screenshot",
        "image",
        (2064, 2752),
        None,
        "",
    ),
    f(
        "Store images",
        "google-phone",
        "Google Play phone screenshot, 9:16",
        "image",
        (1080, 1920),
        None,
        "2:1 limit",
    ),
    f(
        "Store images",
        "google-feature-graphic",
        "Google Play feature graphic",
        "image",
        (1024, 500),
        None,
        "little text",
    ),
    f(
        "Social images",
        "square-post",
        "Square post, 1:1",
        "image",
        (1080, 1080),
        None,
        "",
    ),
    f(
        "Social images",
        "portrait-post",
        "Portrait post, 4:5",
        "image",
        (1080, 1350),
        None,
        "",
    ),
    f(
        "Social images",
        "story",
        "Story / vertical, 9:16",
        "image",
        (1080, 1920),
        None,
        "safe zone",
    ),
    f(
        "Social images",
        "og",
        "Open Graph / link card",
        "image",
        (1200, 630),
        None,
        "",
    ),
    f(
        "Web and icons",
        "web-hero",
        "Web hero, 16:9",
        "image",
        (1920, 1080),
        None,
        "",
    ),
    f(
        "Web and icons",
        "icon",
        "App icon, square",
        "image",
        (1024, 1024),
        None,
        "no transparency on iOS",
    ),
];

pub fn find(id: &str) -> Option<&'static Format> {
    FORMATS.iter().find(|f| f.id == id)
}

/// A format as stored in a brief: from the list, or custom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefFormat {
    #[serde(default)]
    pub id: Option<String>,
    pub label: String,
    pub kind: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub fps: Option<u32>,
}

/// A change the agent asked for and the user has not answered yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub format: BriefFormat,
    pub reason: String,
}

/// What a workspace is for. A format the user chose cannot be changed without asking them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Brief {
    #[serde(default)]
    pub format: Option<BriefFormat>,
    #[serde(default)]
    pub chosen_by_user: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<Pending>,
    /// What the user answered to the last request, for the agent to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_decision: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOutcome {
    Applied,
    /// The user chose the current format, so the change waits for their answer.
    NeedsApproval,
}

impl Brief {
    /// The agent sets the format. Free when the user chose nothing; a request when they did.
    pub fn agent_set(&mut self, new: BriefFormat, reason: &str) -> SetOutcome {
        if self.format.as_ref() == Some(&new) {
            self.pending = None;
            return SetOutcome::Applied;
        }
        if self.chosen_by_user && self.format.is_some() {
            self.pending = Some(Pending {
                format: new,
                reason: reason.trim().chars().take(400).collect(),
            });
            return SetOutcome::NeedsApproval;
        }
        self.format = Some(new);
        self.chosen_by_user = false;
        self.pending = None;
        SetOutcome::Applied
    }

    /// The user sets (or clears, with `None`, handing the choice back to the agent) the format.
    pub fn user_set(&mut self, new: Option<BriefFormat>) {
        self.chosen_by_user = new.is_some();
        self.format = new;
        self.pending = None;
    }

    /// The user answers the agent's request.
    pub fn answer(&mut self, approve: bool) -> Result<(), String> {
        let p = self.pending.take().ok_or("there is no request to answer")?;
        if approve {
            self.last_decision = Some(format!("The user approved {}.", p.format.label));
            self.format = Some(p.format);
            self.chosen_by_user = true;
        } else {
            let keep = self
                .format
                .as_ref()
                .map_or("the current format", |f| f.label.as_str());
            self.last_decision = Some(format!(
                "The user kept {keep}; do not ask again unless the work needs it."
            ));
        }
        Ok(())
    }

    /// For a video the agent is about to create or resize: refuse a size or rate the user did not
    /// choose. Images are guidance only (a panorama is several slides wide).
    pub fn check_video(
        &self,
        width: Option<u32>,
        height: Option<u32>,
        fps: Option<u32>,
    ) -> Result<(), String> {
        let Some(f) = self
            .format
            .as_ref()
            .filter(|f| self.chosen_by_user && f.kind == "video")
        else {
            return Ok(());
        };
        let wrong_size =
            width.is_some_and(|w| w != f.width) || height.is_some_and(|h| h != f.height);
        let wrong_fps = fps.is_some_and(|r| f.fps.is_some_and(|c| c != r));
        if wrong_size || wrong_fps {
            return Err(format!(
                "the user chose {} ({}x{}{}) for this workspace; make the video that size, or call set_format with a reason to ask them to change it",
                f.label,
                f.width,
                f.height,
                f.fps.map_or(String::new(), |r| format!(", {r} fps"))
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> serde_json::Value {
        let who = match (&self.format, self.chosen_by_user) {
            (None, _) => "none",
            (Some(_), true) => "user",
            (Some(_), false) => "agent",
        };
        serde_json::json!({
            "format": self.format,
            "chosen_by": who,
            "notes": self.notes,
            "pending_request": self.pending,
            "last_decision": self.last_decision,
            "rules": match who {
                "user" => "The user chose this format: use it. To change it, call set_format with a reason; the user is asked and you must wait for the answer (read get_brief again).",
                "agent" => "You chose this earlier; you may change it with set_format.",
                _ => "Nothing is chosen. Pick a format from list_formats by destination, call set_format with a one-line reason, and say what you chose.",
            },
        })
    }
}

const BRIEF_FILE: &str = "brief.json";

pub fn load_brief(dir: &Path) -> Brief {
    std::fs::read_to_string(dir.join(BRIEF_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_brief(dir: &Path, brief: &Brief) -> Result<(), String> {
    let mut json = serde_json::to_string_pretty(brief).map_err(|e| e.to_string())?;
    json.push('\n');
    std::fs::write(dir.join(BRIEF_FILE), json).map_err(|e| format!("{}: {e}", dir.display()))
}

/// Sizes are whole numbers; video sizes are even and at most 4096; images at most 16384.
pub fn custom(
    kind: &str,
    width: u32,
    height: u32,
    fps: Option<u32>,
) -> Result<BriefFormat, String> {
    let (min, max) = match kind {
        "video" => (16, 4096),
        "image" => (16, 16_384),
        _ => return Err("kind is video or image".into()),
    };
    if !(min..=max).contains(&width) || !(min..=max).contains(&height) {
        return Err(format!("width and height are {min} to {max}"));
    }
    if kind == "video" {
        if width % 2 != 0 || height % 2 != 0 {
            return Err("video sizes are even numbers".into());
        }
        if let Some(r) = fps {
            if !(1..=60).contains(&r) {
                return Err("fps is 1 to 60".into());
            }
        }
    }
    Ok(BriefFormat {
        id: None,
        label: format!("Custom {width}x{height}"),
        kind: kind.to_string(),
        width,
        height,
        fps: if kind == "video" {
            fps.or(Some(30))
        } else {
            None
        },
    })
}

impl From<&Format> for BriefFormat {
    fn from(f: &Format) -> BriefFormat {
        BriefFormat {
            id: Some(f.id.to_string()),
            label: f.label.to_string(),
            kind: f.kind.to_string(),
            width: f.width,
            height: f.height,
            fps: f.fps,
        }
    }
}

/// The list as JSON, grouped by section, for the window and the `list_formats` tool.
pub fn sections_json() -> serde_json::Value {
    let mut sections: Vec<(&str, Vec<serde_json::Value>)> = Vec::new();
    for fm in FORMATS {
        let row = serde_json::json!({
            "id": fm.id, "label": fm.label, "kind": fm.kind,
            "width": fm.width, "height": fm.height, "fps": fm.fps, "note": fm.note,
        });
        match sections.iter_mut().find(|(s, _)| *s == fm.section) {
            Some((_, rows)) => rows.push(row),
            None => sections.push((fm.section, vec![row])),
        }
    }
    serde_json::Value::Array(
        sections
            .into_iter()
            .map(|(name, rows)| serde_json::json!({"section": name, "formats": rows}))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_is_valid_for_its_kind_and_ids_are_unique() {
        let mut ids: Vec<&str> = FORMATS.iter().map(|f| f.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), FORMATS.len());
        for fm in FORMATS {
            assert!(
                custom(fm.kind, fm.width, fm.height, fm.fps).is_ok(),
                "{}",
                fm.id
            );
            assert_eq!(fm.fps.is_some(), fm.kind == "video", "{}", fm.id);
        }
    }

    #[test]
    fn custom_sizes_are_bounded_and_video_is_even() {
        assert!(custom("video", 1080, 1920, None).unwrap().fps == Some(30));
        assert!(custom("video", 1081, 1920, None).is_err());
        assert!(custom("video", 5000, 1920, None).is_err());
        assert!(custom("video", 1080, 1920, Some(0)).is_err());
        assert!(custom("image", 1081, 1921, None).is_ok());
        assert!(custom("audio", 100, 100, None).is_err());
    }

    #[test]
    fn a_brief_round_trips_and_a_missing_one_is_empty() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(load_brief(d.path()), Brief::default());
        let b = Brief {
            format: find("reel").map(BriefFormat::from),
            chosen_by_user: true,
            notes: "for the launch".into(),
            pending: None,
            last_decision: None,
        };
        save_brief(d.path(), &b).unwrap();
        assert_eq!(load_brief(d.path()), b);
    }

    #[test]
    fn a_format_the_user_chose_changes_only_when_they_agree() {
        let reel = BriefFormat::from(find("reel").unwrap());
        let wide = BriefFormat::from(find("youtube").unwrap());
        let mut b = Brief::default();
        // nothing chosen: the agent picks freely, and may change its own pick
        assert_eq!(b.agent_set(reel.clone(), "a reel"), SetOutcome::Applied);
        assert!(!b.chosen_by_user);
        assert_eq!(b.agent_set(wide.clone(), "better"), SetOutcome::Applied);
        // the user chooses: now the agent must ask
        b.user_set(Some(reel.clone()));
        assert!(b.check_video(Some(1920), Some(1080), None).is_err());
        assert!(b.check_video(Some(1080), Some(1920), Some(30)).is_ok());
        assert!(b.check_video(Some(1080), Some(1920), Some(60)).is_err());
        assert_eq!(
            b.agent_set(wide.clone(), "landscape suits the footage"),
            SetOutcome::NeedsApproval
        );
        assert_eq!(b.format.as_ref(), Some(&reel));
        assert_eq!(
            b.pending.as_ref().unwrap().reason,
            "landscape suits the footage"
        );
        // the user says no, then the agent asks again and they say yes
        b.answer(false).unwrap();
        assert!(b.last_decision.as_ref().unwrap().contains("kept"));
        assert_eq!(b.format.as_ref(), Some(&reel));
        assert_eq!(
            b.agent_set(wide.clone(), "again"),
            SetOutcome::NeedsApproval
        );
        b.answer(true).unwrap();
        assert_eq!(b.format.as_ref(), Some(&wide));
        assert!(b.chosen_by_user && b.pending.is_none());
        assert!(b.answer(true).is_err(), "nothing left to answer");
        // an image format never blocks a video
        b.user_set(Some(BriefFormat::from(find("apple-iphone-6.9").unwrap())));
        assert!(b.check_video(Some(10), Some(10), Some(5)).is_ok());
        // handing the choice back
        b.user_set(None);
        assert!(b.format.is_none() && !b.chosen_by_user);
    }

    #[test]
    fn formats_named_like_store_presets_have_sizes_the_store_accepts() {
        use od_store::{Format as Img, ImageFacts};
        for fm in FORMATS {
            if fm.kind == "image" {
                if let Some(p) = od_store::find(fm.id) {
                    let facts = ImageFacts {
                        width: fm.width,
                        height: fm.height,
                        format: Img::Png,
                        has_alpha: false,
                    };
                    let size_problems = p
                        .validate_image(&facts)
                        .into_iter()
                        .filter(|v| matches!(v, od_store::Violation::Size { .. }))
                        .count();
                    assert_eq!(size_problems, 0, "{} is not a size {} accepts", fm.id, p.id);
                }
            } else if let Some(p) =
                od_store::video::find_video(fm.id.trim_end_matches("-landscape"))
            {
                assert!(p.sizes.contains(&(fm.width, fm.height)), "{}", fm.id);
                assert!(fm.fps.unwrap_or(0) <= p.max_fps, "{}", fm.id);
            }
        }
        for id in [
            "apple-iphone-6.9",
            "google-phone",
            "google-feature-graphic",
            "apple-app-preview-iphone",
        ] {
            assert!(find(id).is_some(), "{id}");
        }
    }
}

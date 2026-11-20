//! What the agent shows the user outside the canvas: the end product in the Preview panel and a
//! set of options in the Variants panel (`docs/wiki/agent-integration.md#showing-work`). One small
//! JSON file next to `brief.json`; a missing, oversized or invalid file reads as empty.

use std::path::Path;

use od_core::{NodeId, NodeKind, Workspace};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MIN_OPTIONS: usize = 2;
pub const MAX_OPTIONS: usize = 8;
const MAX_LABEL: usize = 80;
const MAX_TITLE: usize = 120;
const MAX_NOTE: usize = 400;
const MAX_TARGET: usize = 200;
const MAX_FILE_BYTES: u64 = 64 * 1024;
const SHOWCASE_FILE: &str = "showcase.json";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShowcaseError {
    #[error("{field} is empty")]
    Empty { field: &'static str },
    #[error("{field} is longer than {max} characters")]
    TooLong { field: &'static str, max: usize },
    #[error("offer between {MIN_OPTIONS} and {MAX_OPTIONS} options, not {0}")]
    OptionCount(usize),
    #[error("{kind} {target:?} does not exist: {why}")]
    MissingTarget {
        kind: Kind,
        target: String,
        why: String,
    },
    #[error("two options show the same {kind} {target:?}: each option needs its own")]
    DuplicateTarget { kind: Kind, target: String },
    #[error("there is no option {0:?} in the open set")]
    NoSuchOption(String),
    #[error("no options are open")]
    NoVariants,
    #[error("the project has no folder to keep this in")]
    NoFolder,
    #[error("{0}")]
    Io(String),
}

/// What an option or the preview points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A frame, by node id.
    Frame,
    /// A video, by id.
    Video,
    /// An image file in `media/`, by reference (`media/<name>`).
    Image,
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Kind::Frame => "frame",
            Kind::Video => "video",
            Kind::Image => "image",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewTarget {
    pub kind: Kind,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Who chose it, as the tool call was attributed.
    #[serde(default)]
    pub by: String,
    /// Counts every `show_preview`, so the window can tell a new one from the same one.
    #[serde(default)]
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantOption {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub kind: Kind,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantSet {
    /// 1 for the first set offered in this workspace, then up by one for each new offer.
    pub round: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    pub options: Vec<VariantOption>,
    /// The id of the option the user picked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picked: Option<String>,
    /// When the user picked, in seconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picked_at: Option<u64>,
    #[serde(default)]
    pub offered_by: String,
}

impl VariantSet {
    pub fn picked_option(&self) -> Option<&VariantOption> {
        let id = self.picked.as_deref()?;
        self.options.iter().find(|o| o.id == id)
    }
}

/// An option as the agent offers it, before it has an id.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewOption {
    pub label: String,
    pub kind: Kind,
    pub target: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Showcase {
    #[serde(default)]
    pub preview: Option<PreviewTarget>,
    #[serde(default)]
    pub variants: Option<VariantSet>,
    /// How many sets were ever offered here, so rounds keep counting after a clear.
    #[serde(default)]
    pub rounds: u32,
}

fn bounded(field: &'static str, text: &str, max: usize) -> Result<String, ShowcaseError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ShowcaseError::Empty { field });
    }
    if text.chars().count() > max {
        return Err(ShowcaseError::TooLong { field, max });
    }
    Ok(text.to_string())
}

fn optional(
    field: &'static str,
    text: Option<&str>,
    max: usize,
) -> Result<Option<String>, ShowcaseError> {
    match text.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => bounded(field, t, max).map(Some),
        None => Ok(None),
    }
}

/// Checks that `target` is a `kind` that exists in the open workspace.
pub fn check_target(ws: &Workspace, kind: Kind, target: &str) -> Result<(), ShowcaseError> {
    let missing = |why: String| ShowcaseError::MissingTarget {
        kind,
        target: target.to_string(),
        why,
    };
    if target.chars().count() > MAX_TARGET {
        return Err(ShowcaseError::TooLong {
            field: "target",
            max: MAX_TARGET,
        });
    }
    let doc = ws.session.document();
    match kind {
        Kind::Frame => {
            let id = NodeId::parse(target).map_err(|e| missing(e.to_string()))?;
            match doc.node(&id) {
                Some(n) if n.kind == NodeKind::Frame => Ok(()),
                Some(_) => Err(missing(
                    "it is a layer, not a frame; use a frame id from list_pages or get_tree".into(),
                )),
                None => Err(missing(
                    "no node has this id; use a frame id from list_pages".into(),
                )),
            }
        }
        Kind::Video => {
            let id = NodeId::parse(target).map_err(|e| missing(e.to_string()))?;
            if doc.videos.iter().any(|v| v.id == id) {
                Ok(())
            } else {
                Err(missing(
                    "no video has this id; use an id from list_videos".into(),
                ))
            }
        }
        Kind::Image => ws
            .media_path(target)
            .map(|_| ())
            .map_err(|e| missing(format!("{e}; use a path from list_media"))),
    }
}

fn valid_shape(s: &Showcase) -> bool {
    let preview_ok = s.preview.as_ref().is_none_or(|p| {
        !p.target.is_empty()
            && p.target.chars().count() <= MAX_TARGET
            && p.title
                .as_deref()
                .is_none_or(|t| t.chars().count() <= MAX_TITLE)
            && p.note
                .as_deref()
                .is_none_or(|t| t.chars().count() <= MAX_NOTE)
    });
    let variants_ok = s.variants.as_ref().is_none_or(|v| {
        let ids: std::collections::HashSet<&str> =
            v.options.iter().map(|o| o.id.as_str()).collect();
        (MIN_OPTIONS..=MAX_OPTIONS).contains(&v.options.len())
            && ids.len() == v.options.len()
            && v.picked.as_deref().is_none_or(|p| ids.contains(p))
            && v.title.chars().count() <= MAX_TITLE
            && v.options.iter().all(|o| {
                !o.target.is_empty()
                    && o.target.chars().count() <= MAX_TARGET
                    && o.label.chars().count() <= MAX_LABEL
                    && o.note
                        .as_deref()
                        .is_none_or(|t| t.chars().count() <= MAX_NOTE)
            })
    });
    preview_ok && variants_ok
}

pub fn load(dir: &Path) -> Showcase {
    let path = dir.join(SHOWCASE_FILE);
    let small = std::fs::metadata(&path).is_ok_and(|m| m.len() <= MAX_FILE_BYTES);
    if !small {
        return Showcase::default();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Showcase>(&t).ok())
        .filter(valid_shape)
        .unwrap_or_default()
}

pub fn save(dir: &Path, showcase: &Showcase) -> Result<(), ShowcaseError> {
    let io = |e: std::io::Error| ShowcaseError::Io(format!("{}: {e}", dir.display()));
    let mut json =
        serde_json::to_string_pretty(showcase).map_err(|e| ShowcaseError::Io(e.to_string()))?;
    json.push('\n');
    let tmp = dir.join(format!("{SHOWCASE_FILE}.part"));
    std::fs::write(&tmp, json).map_err(io)?;
    std::fs::rename(&tmp, dir.join(SHOWCASE_FILE)).map_err(io)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Showcase {
    /// The agent decides what the user sees as the end product.
    pub fn set_preview(
        &mut self,
        ws: &Workspace,
        kind: Kind,
        target: &str,
        title: Option<&str>,
        note: Option<&str>,
        by: &str,
    ) -> Result<(), ShowcaseError> {
        let target = bounded("target", target, MAX_TARGET)?;
        check_target(ws, kind, &target)?;
        let seq = self.preview.as_ref().map_or(0, |p| p.seq) + 1;
        self.preview = Some(PreviewTarget {
            kind,
            target,
            title: optional("title", title, MAX_TITLE)?,
            note: optional("note", note, MAX_NOTE)?,
            by: by.to_string(),
            seq,
        });
        Ok(())
    }

    /// Replaces any open set with a new round, nothing picked.
    pub fn offer(
        &mut self,
        ws: &Workspace,
        title: &str,
        question: Option<&str>,
        options: &[NewOption],
        by: &str,
    ) -> Result<&VariantSet, ShowcaseError> {
        if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&options.len()) {
            return Err(ShowcaseError::OptionCount(options.len()));
        }
        let title = bounded("title", title, MAX_TITLE)?;
        let question = optional("question", question, MAX_NOTE)?;
        let mut built: Vec<VariantOption> = Vec::new();
        for (i, o) in options.iter().enumerate() {
            let target = bounded("target", &o.target, MAX_TARGET)?;
            if built.iter().any(|b| b.kind == o.kind && b.target == target) {
                return Err(ShowcaseError::DuplicateTarget {
                    kind: o.kind,
                    target,
                });
            }
            check_target(ws, o.kind, &target)?;
            built.push(VariantOption {
                id: format!("v{}", i + 1),
                label: bounded("label", &o.label, MAX_LABEL)?,
                note: optional("note", o.note.as_deref(), MAX_NOTE)?,
                kind: o.kind,
                target,
            });
        }
        self.rounds += 1;
        Ok(self.variants.insert(VariantSet {
            round: self.rounds,
            title,
            question,
            options: built,
            picked: None,
            picked_at: None,
            offered_by: by.to_string(),
        }))
    }

    /// The user's pick. Picking again changes it.
    pub fn pick(&mut self, id: &str) -> Result<&VariantOption, ShowcaseError> {
        let set = self.variants.as_mut().ok_or(ShowcaseError::NoVariants)?;
        if !set.options.iter().any(|o| o.id == id) {
            return Err(ShowcaseError::NoSuchOption(id.to_string()));
        }
        set.picked = Some(id.to_string());
        set.picked_at = Some(now_secs());
        set.picked_option().ok_or(ShowcaseError::NoVariants)
    }

    pub fn clear_variants(&mut self) -> bool {
        self.variants.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use od_core::{Actor, NewNode, Operation};

    fn workspace_with_frames(n: usize) -> (tempfile::TempDir, Workspace, Vec<String>) {
        let tmp = tempfile::tempdir().unwrap();
        let mut ws = Workspace::create(&tmp.path().join("p.od"), "Showcase").unwrap();
        let root = ws.session.document().pages[0].root.id.clone();
        let mut ops = Vec::new();
        for _ in 0..n {
            let mut node = NewNode::new(NodeKind::Frame);
            node.width = 400.0;
            node.height = 300.0;
            ops.push(Operation::Create {
                parent: root.clone(),
                index: None,
                node,
            });
        }
        let receipt = ws.session.apply(Actor::User, None, &ops).unwrap();
        let ids = receipt.created.iter().map(ToString::to_string).collect();
        (tmp, ws, ids)
    }

    fn opt(label: &str, target: &str) -> NewOption {
        NewOption {
            label: label.into(),
            kind: Kind::Frame,
            target: target.into(),
            note: None,
        }
    }

    #[test]
    fn a_showcase_round_trips_and_a_missing_file_is_empty() {
        let (tmp, ws, ids) = workspace_with_frames(2);
        assert_eq!(load(tmp.path()), Showcase::default());
        let mut s = Showcase::default();
        s.set_preview(
            &ws,
            Kind::Frame,
            &ids[0],
            Some("Hero"),
            Some("final"),
            "claude",
        )
        .unwrap();
        s.offer(
            &ws,
            "Directions",
            Some("Which?"),
            &[opt("A", &ids[0]), opt("B", &ids[1])],
            "claude",
        )
        .unwrap();
        s.pick("v2").unwrap();
        save(tmp.path(), &s).unwrap();
        assert_eq!(load(tmp.path()), s);
        assert_eq!(
            s.variants.as_ref().unwrap().picked_option().unwrap().label,
            "B"
        );
    }

    #[test]
    fn a_corrupt_oversized_or_inconsistent_file_reads_as_empty() {
        let (tmp, ws, ids) = workspace_with_frames(2);
        let path = tmp.path().join(SHOWCASE_FILE);
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(load(tmp.path()), Showcase::default());
        std::fs::write(&path, " ".repeat(MAX_FILE_BYTES as usize + 1)).unwrap();
        assert_eq!(load(tmp.path()), Showcase::default());
        let mut s = Showcase::default();
        s.offer(&ws, "T", None, &[opt("A", &ids[0]), opt("B", &ids[1])], "x")
            .unwrap();
        s.variants.as_mut().unwrap().picked = Some("v9".into());
        save(tmp.path(), &s).unwrap();
        assert_eq!(
            load(tmp.path()),
            Showcase::default(),
            "a pick that is not an option"
        );
    }

    #[test]
    fn offers_are_bounded_and_every_target_must_exist() {
        let (_tmp, ws, ids) = workspace_with_frames(2);
        let mut s = Showcase::default();
        let one = s
            .offer(&ws, "T", None, &[opt("A", &ids[0])], "x")
            .unwrap_err();
        assert_eq!(one, ShowcaseError::OptionCount(1));
        let nine: Vec<NewOption> = (0..9).map(|i| opt(&format!("o{i}"), &ids[0])).collect();
        assert_eq!(
            s.offer(&ws, "T", None, &nine, "x").unwrap_err(),
            ShowcaseError::OptionCount(9)
        );
        let missing = "n_99999".to_string();
        let err = s
            .offer(
                &ws,
                "T",
                None,
                &[opt("A", &ids[0]), opt("B", &missing)],
                "x",
            )
            .unwrap_err();
        assert!(
            matches!(
                err,
                ShowcaseError::MissingTarget {
                    kind: Kind::Frame,
                    ..
                }
            ),
            "{err}"
        );
        let err = s
            .offer(&ws, "T", None, &[opt("A", &ids[0]), opt("B", &ids[0])], "x")
            .unwrap_err();
        assert!(
            matches!(err, ShowcaseError::DuplicateTarget { .. }),
            "{err}"
        );
        let long = "x".repeat(MAX_LABEL + 1);
        let err = s
            .offer(
                &ws,
                "T",
                None,
                &[opt(&long, &ids[0]), opt("B", &ids[1])],
                "x",
            )
            .unwrap_err();
        assert_eq!(
            err,
            ShowcaseError::TooLong {
                field: "label",
                max: MAX_LABEL
            }
        );
        assert!(
            s.variants.is_none() && s.rounds == 0,
            "a refused offer changes nothing"
        );
        let mut img = opt("I", "media/none.png");
        img.kind = Kind::Image;
        assert!(matches!(
            s.offer(&ws, "T", None, &[opt("A", &ids[0]), img], "x")
                .unwrap_err(),
            ShowcaseError::MissingTarget {
                kind: Kind::Image,
                ..
            }
        ));
        let mut vid = opt("V", &ids[0]);
        vid.kind = Kind::Video;
        assert!(matches!(
            s.set_preview(&ws, vid.kind, &vid.target, None, None, "x")
                .unwrap_err(),
            ShowcaseError::MissingTarget {
                kind: Kind::Video,
                ..
            }
        ));
    }

    #[test]
    fn a_new_offer_replaces_the_open_set_and_rounds_keep_counting() {
        let (_tmp, ws, ids) = workspace_with_frames(2);
        let mut s = Showcase::default();
        let two = [opt("A", &ids[0]), opt("B", &ids[1])];
        assert_eq!(s.offer(&ws, "T", None, &two, "x").unwrap().round, 1);
        s.pick("v1").unwrap();
        let second = s.offer(&ws, "T2", None, &two, "x").unwrap();
        assert_eq!((second.round, second.picked.clone()), (2, None));
        assert!(s.clear_variants() && !s.clear_variants());
        assert_eq!(s.offer(&ws, "T3", None, &two, "x").unwrap().round, 3);
        assert_eq!(
            s.pick("v7").unwrap_err(),
            ShowcaseError::NoSuchOption("v7".into())
        );
    }

    #[test]
    fn show_preview_counts_each_call() {
        let (_tmp, ws, ids) = workspace_with_frames(1);
        let mut s = Showcase::default();
        s.set_preview(&ws, Kind::Frame, &ids[0], None, None, "a")
            .unwrap();
        s.set_preview(&ws, Kind::Frame, &ids[0], None, Some("  "), "b")
            .unwrap();
        let p = s.preview.unwrap();
        assert_eq!((p.seq, p.by.as_str(), p.note), (2, "b", None));
    }
}

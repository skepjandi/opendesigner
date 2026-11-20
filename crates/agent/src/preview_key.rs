//! What a preview picture was made from, as one number, so pictures survive an edit that did not
//! touch them. The renderer's own `signature` says when two moments of one video look alike; this
//! key also says what they look like: the video's settings, the clips and overlays on screen (as
//! the file stores them) with the design frames they show, the animations, the colour tokens and
//! the project's media files. Everything is hashed through the types' serialized form, so a field
//! added to a clip later is part of the key without touching this file.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use od_core::{Document, NodeId, Video};
use serde::Serialize;

use crate::video_export::Source;

pub struct Keys<'a> {
    video: &'a Video,
    placed: Vec<od_core::video::Placed>,
    base: u64,
    clips: Vec<u64>,
    overlays: Vec<u64>,
    /// How far back from a moment a picture can still show (a motion-blur shutter).
    reach_ms: u64,
}

fn digest(value: &impl Serialize) -> u64 {
    let mut hasher = DefaultHasher::new();
    serde_json::to_vec(value)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}

fn frame_digests(doc: &Document, video: &Video) -> HashMap<NodeId, u64> {
    let frames = video
        .clips
        .iter()
        .filter_map(|c| c.frame.as_ref())
        .chain(video.overlays.iter().map(|o| &o.frame));
    frames
        .filter_map(|id| doc.node(id).map(|n| (id.clone(), digest(n))))
        .collect()
}

/// Names, sizes and times of the files in the project's `media/`, which clips and images read.
fn media_stamp(dir: Option<&Path>) -> u64 {
    let Some(entries) = dir.and_then(|d| std::fs::read_dir(d.join("media")).ok()) else {
        return 0;
    };
    let mut files: Vec<(String, u64, u128)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            Some((
                e.file_name().to_string_lossy().into_owned(),
                meta.len(),
                modified,
            ))
        })
        .collect();
    files.sort();
    digest(&files)
}

impl<'a> Keys<'a> {
    pub fn new(src: &Source, video: &'a Video, (width, height, fps): (u32, u32, u32)) -> Keys<'a> {
        let doc = &src.doc;
        let frames = frame_digests(doc, video);
        let mut look = video.clone();
        look.clips.clear();
        look.overlays.clear();
        look.audio = None;
        let base = digest(&(
            (width, height, fps),
            digest(&look),
            digest(&doc.tokens),
            media_stamp(src.dir()),
        ));
        let of_frame = |id: &Option<&NodeId>| id.and_then(|i| frames.get(i)).copied();
        Keys {
            video,
            placed: video.placed(),
            base,
            clips: video
                .clips
                .iter()
                .map(|c| digest(&(c, of_frame(&c.frame.as_ref()))))
                .collect(),
            overlays: video
                .overlays
                .iter()
                .map(|o| digest(&(o, of_frame(&Some(&o.frame)))))
                .collect(),
            reach_ms: 1000 / u64::from(video.fps.max(1)),
        }
    }

    /// The key of the picture at `time_ms`; `signature` is the renderer's for that moment, or
    /// `None` while footage plays (every moment then differs).
    pub fn key(&self, time_ms: u64, signature: Option<&[i64]>) -> u64 {
        let placed = &self.placed;
        let last = placed.last().map_or(0, |p| p.end_ms.saturating_sub(1));
        let hi = time_ms.min(last);
        let lo = hi.saturating_sub(self.reach_ms);
        let mut hasher = DefaultHasher::new();
        self.base.hash(&mut hasher);
        for p in placed.iter().filter(|p| p.start_ms <= hi && p.end_ms > lo) {
            (self.clips[p.index], p.start_ms, p.end_ms, p.overlap_ms).hash(&mut hasher);
        }
        for (n, o) in self.video.overlays.iter().enumerate() {
            let end = o.start_ms + u64::from(o.duration_ms);
            if o.start_ms <= hi && end > lo {
                (self.overlays[n], n).hash(&mut hasher);
            }
        }
        match signature {
            Some(sig) => sig.hash(&mut hasher),
            None => time_ms.hash(&mut hasher),
        }
        hasher.finish()
    }
}

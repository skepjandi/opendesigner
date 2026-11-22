//! A video's timeline as plain text other editors read: a CMX 3600 edit decision list (Final Cut
//! Pro, Premiere Pro, DaVinci Resolve and most others import one) and SRT or WebVTT subtitles
//! (`docs/wiki/video-editing.md#timeline-export`).
//!
//! An edit decision list names sources and times, nothing more. Footage clips become events the
//! other editor links to the file by name; a clip that shows a design frame has no file, so its
//! event is a placeholder, and what an EDL cannot say (overlays, captions, keyframes, grades,
//! speed ramps, the kind of a transition) is listed in `warnings` rather than dropped quietly.

use crate::captions::Captions;
use crate::model::Document;
use crate::video::{TransitionKind, Video};

/// Record time starts at one hour, as editors expect, so a lead-in fits before the first clip.
const RECORD_START_HOURS: u64 = 1;
const MAX_TITLE_CHARS: usize = 60;

/// A finished edit decision list and what it left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edl {
    pub text: String,
    /// Edit lines written: picture and sound, a dissolve counting once.
    pub events: usize,
    pub warnings: Vec<String>,
}

/// `hh:mm:ss:ff` for `ms` at `fps`, non-drop, with `extra_hours` added.
pub fn timecode(ms: u64, fps: u32, extra_hours: u64) -> String {
    let rate = u64::from(fps.max(1));
    let frames = (ms * rate + 500) / 1000;
    let (h, m, s, f) = (
        frames / (rate * 3600) + extra_hours,
        frames / (rate * 60) % 60,
        frames / rate % 60,
        frames % rate,
    );
    format!("{h:02}:{m:02}:{s:02}:{f:02}")
}

fn clean(text: &str, max: usize) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

fn file_name(reference: &str) -> &str {
    reference.rsplit('/').next().unwrap_or(reference)
}

fn line(event: usize, track: &str, kind: &str, frames: &str, tcs: [String; 4]) -> String {
    format!(
        "{event:03}  AX       {track:<5} {kind:<4} {frames:<3} {} {} {} {}\n",
        tcs[0], tcs[1], tcs[2], tcs[3]
    )
}

/// The edit decision list of `video`. `footage_has_audio` says whether a footage file carries
/// sound, which makes its events picture and sound together (`B`) instead of picture only (`V`).
pub fn edl(doc: &Document, video: &Video, footage_has_audio: &dyn Fn(&str) -> bool) -> Edl {
    let fps = video.fps;
    let record = |ms: u64| timecode(ms, fps, RECORD_START_HOURS);
    let source = |ms: u64| timecode(ms, fps, 0);
    let mut text = format!(
        "TITLE: {}\nFCM: NON-DROP FRAME\n\n",
        clean(&video.name, MAX_TITLE_CHARS)
    );
    let mut warnings = Vec::new();
    let placed = video.placed();
    let mut event = 0usize;
    let mut designs = 0usize;
    let mut ramps = 0usize;
    let mut previous: Option<(String, u64, String)> = None;

    for (i, p) in placed.iter().enumerate() {
        let clip = &video.clips[p.index];
        let rec_in = p.start_ms;
        let rec_out = placed.get(i + 1).map_or(p.end_ms, |next| next.start_ms);
        let (track, name, file, in_ms, speed) = match &clip.footage {
            Some(f) => {
                let ramp = !clip.speed_ramp.is_empty();
                ramps += usize::from(ramp);
                let track = if footage_has_audio(&f.file) { "B" } else { "V" };
                let speed = if ramp { 1.0 } else { clip.speed };
                (
                    track,
                    file_name(&f.file).to_string(),
                    Some(f.file.clone()),
                    f.in_ms,
                    speed,
                )
            }
            None => {
                designs += 1;
                let name = clip
                    .frame
                    .as_ref()
                    .and_then(|id| doc.node(id))
                    .map_or_else(|| "design frame".to_string(), |n| n.name.clone());
                ("V", name, None, 0, 1.0)
            }
        };
        let src_out_ms = in_ms + ((rec_out - rec_in) as f64 * speed).round() as u64;
        event += 1;
        if p.overlap_ms > 0 && clip.transition.kind != TransitionKind::Cut {
            if let Some((prev_track, prev_out, _)) = &previous {
                text.push_str(&line(
                    event,
                    prev_track,
                    "C",
                    "",
                    [
                        source(*prev_out),
                        source(*prev_out),
                        record(rec_in),
                        record(rec_in),
                    ],
                ));
            }
            let frames = (u64::from(p.overlap_ms) * u64::from(fps) + 500) / 1000;
            text.push_str(&line(
                event,
                track,
                "D",
                &format!("{frames:03}"),
                [
                    source(in_ms),
                    source(src_out_ms),
                    record(rec_in),
                    record(rec_out),
                ],
            ));
            if clip.transition.kind != TransitionKind::Fade {
                warnings.push(format!(
                    "clip {} enters with a {:?} transition; an EDL has only a dissolve, so it is one",
                    p.index + 1,
                    clip.transition.kind
                ));
            }
        } else {
            text.push_str(&line(
                event,
                track,
                "C",
                "",
                [
                    source(in_ms),
                    source(src_out_ms),
                    record(rec_in),
                    record(rec_out),
                ],
            ));
        }
        if (speed - 1.0).abs() > f64::EPSILON {
            text.push_str(&format!(
                "M2   AX       {:<5.1}               {}\n",
                speed * f64::from(fps),
                source(in_ms)
            ));
        }
        text.push_str(&format!("* FROM CLIP NAME: {}\n", clean(&name, 80)));
        match &file {
            Some(f) => text.push_str(&format!("* SOURCE FILE: {}\n", clean(f, 120))),
            None => text.push_str(
                "* OPENDESIGNER: a design frame, no source file; export_video renders it\n",
            ),
        }
        previous = Some((track.to_string(), src_out_ms, name));
    }

    let end = video.duration_ms();
    for sound in video.sound_tracks() {
        let start = u64::from(sound.start_ms);
        if start >= end {
            warnings.push(format!(
                "{} starts after the video ends and is left out",
                sound.file
            ));
            continue;
        }
        let in_ms = u64::from(sound.in_ms);
        let length = sound
            .out_ms
            .map_or(end - start, |out| {
                u64::from(out.saturating_sub(sound.in_ms))
            })
            .min(end - start);
        event += 1;
        text.push_str(&line(
            event,
            "A",
            "C",
            "",
            [
                source(in_ms),
                source(in_ms + length),
                record(start),
                record(start + length),
            ],
        ));
        text.push_str(&format!(
            "* FROM CLIP NAME: {}\n",
            clean(file_name(&sound.file), 80)
        ));
        text.push_str(&format!("* SOURCE FILE: {}\n", clean(&sound.file, 120)));
        text.push_str(&format!(
            "* OPENDESIGNER: {:?} track, gain {:.2}, fade in {} ms, fade out {} ms\n",
            sound.role, sound.gain, sound.fade_in_ms, sound.fade_out_ms
        ));
    }

    if designs > 0 {
        warnings.push(format!(
            "{designs} clip(s) show design frames: the events are placeholders with no file to link to; export the video with export_video and use it as the source"
        ));
    }
    if ramps > 0 {
        warnings.push(format!("{ramps} clip(s) have a speed ramp, which an EDL cannot hold: they are written at constant speed"));
    }
    if !video.overlays.is_empty() {
        warnings.push(format!(
            "{} overlay(s) are not in the EDL",
            video.overlays.len()
        ));
    }
    if !video.captions.cues.is_empty() {
        warnings.push("captions are not in the EDL: export them as .srt or .vtt".into());
    }
    if !video.animations.is_empty() {
        warnings.push("keyframed animation is not in the EDL".into());
    }
    if video.clips.iter().any(|c| c.grade.is_some()) {
        warnings.push("colour grades are not in the EDL".into());
    }
    Edl {
        text,
        events: event,
        warnings,
    }
}

fn subtitle_time(ms: u32, comma: bool) -> String {
    let (h, m, s, milli) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    let sep = if comma { ',' } else { '.' };
    format!("{h:02}:{m:02}:{s:02}{sep}{milli:03}")
}

/// A cue's text with blank lines taken out: in both formats a blank line ends the cue.
fn cue_text(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// SubRip: numbered cues, `hh:mm:ss,mmm --> hh:mm:ss,mmm`.
pub fn srt(captions: &Captions) -> String {
    captions
        .cues
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "{}\n{} --> {}\n{}\n\n",
                i + 1,
                subtitle_time(c.start_ms, true),
                subtitle_time(c.end_ms, true),
                cue_text(&c.text)
            )
        })
        .collect()
}

/// WebVTT: a `WEBVTT` header, then cues with `hh:mm:ss.mmm` times. A `-->` in the text, which
/// would read as a timing line, is written with an arrow character instead.
pub fn vtt(captions: &Captions) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for c in &captions.cues {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            subtitle_time(c.start_ms, false),
            subtitle_time(c.end_ms, false),
            cue_text(&c.text).replace("-->", "\u{2192}")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::captions::Cue;
    use crate::model::NodeId;
    use crate::{Actor, Session};
    use serde_json::json;

    fn apply(s: &mut Session, ops: serde_json::Value) {
        let ops: Vec<crate::Operation> = serde_json::from_value(ops).unwrap();
        s.apply(Actor::User, None, &ops).unwrap();
    }

    fn two_clip_video(transition: serde_json::Value) -> (Session, String) {
        let mut s = Session::new(Document::new("edl"));
        apply(
            &mut s,
            json!([{"op": "create_video", "name": "Launch", "fps": 30, "width": 400, "height": 300}]),
        );
        let video = s.document().videos[0].id.to_string();
        apply(
            &mut s,
            json!([
                {"op": "add_clip", "video": video, "footage": {"file": "media/a.mp4", "in_ms": 1000},
                 "duration_ms": 3000},
                {"op": "add_clip", "video": video, "footage": {"file": "media/b.mov", "in_ms": 0},
                 "duration_ms": 2000, "transition": transition}
            ]),
        );
        (s, video)
    }

    #[test]
    fn timecode_counts_frames_and_carries_into_seconds_minutes_and_hours() {
        assert_eq!(timecode(0, 30, 1), "01:00:00:00");
        assert_eq!(timecode(1500, 30, 0), "00:00:01:15");
        assert_eq!(timecode(61_000, 24, 0), "00:01:01:00");
        assert_eq!(timecode(3_600_000, 25, 1), "02:00:00:00");
    }

    #[test]
    fn a_cut_between_footage_clips_is_two_cut_events_with_source_and_record_times() {
        let (s, _) = two_clip_video(json!({"kind": "cut", "duration_ms": 0}));
        let doc = s.document();
        let edl = edl(doc, &doc.videos[0], &|f| f == "media/a.mp4");
        assert_eq!(edl.events, 2);
        assert!(edl.warnings.is_empty(), "{:?}", edl.warnings);
        let lines: Vec<&str> = edl.text.lines().collect();
        assert_eq!(lines[0], "TITLE: Launch");
        assert_eq!(lines[1], "FCM: NON-DROP FRAME");
        assert!(lines[3].starts_with("001  AX       B     C"));
        assert!(
            lines[3].ends_with("00:00:01:00 00:00:04:00 01:00:00:00 01:00:03:00"),
            "{}",
            lines[3]
        );
        assert_eq!(lines[4], "* FROM CLIP NAME: a.mp4");
        assert_eq!(lines[5], "* SOURCE FILE: media/a.mp4");
        assert!(lines[6].starts_with("002  AX       V     C"));
        assert!(
            lines[6].ends_with("00:00:00:00 00:00:02:00 01:00:03:00 01:00:05:00"),
            "{}",
            lines[6]
        );
    }

    #[test]
    fn a_transition_is_a_dissolve_of_its_overlap_that_starts_from_the_outgoing_clip() {
        let (s, _) = two_clip_video(json!({"kind": "slide_left", "duration_ms": 500}));
        let doc = s.document();
        let edl = edl(doc, &doc.videos[0], &|_| false);
        let lines: Vec<&str> = edl.text.lines().collect();
        // clip 1 ends where the dissolve starts: 3000 - 500 ms
        assert!(
            lines[3].ends_with("00:00:01:00 00:00:03:15 01:00:00:00 01:00:02:15"),
            "{}",
            lines[3]
        );
        let to = lines
            .iter()
            .position(|l| l.starts_with("002  AX       V     C"))
            .unwrap();
        assert!(
            lines[to].ends_with("00:00:03:15 00:00:03:15 01:00:02:15 01:00:02:15"),
            "{}",
            lines[to]
        );
        assert!(
            lines[to + 1].starts_with("002  AX       V     D    015"),
            "{}",
            lines[to + 1]
        );
        assert!(
            lines[to + 1].ends_with("00:00:00:00 00:00:02:00 01:00:02:15 01:00:04:15"),
            "{}",
            lines[to + 1]
        );
        assert_eq!(edl.events, 2);
        assert!(
            edl.warnings
                .iter()
                .any(|w| w.contains("SlideLeft") && w.contains("dissolve"))
        );
    }

    #[test]
    fn what_an_edl_cannot_hold_is_reported_not_dropped() {
        let mut s = Session::new(Document::new("edl"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        apply(
            &mut s,
            json!([{"op": "create_video", "name": "V", "fps": 30, "width": 400, "height": 300}]),
        );
        let video = s.document().videos[0].id.to_string();
        let frame = s
            .apply(
                Actor::User,
                None,
                &serde_json::from_value::<Vec<crate::Operation>>(json!([{"op": "create",
                    "parent": root.as_str(), "node": {"kind": "frame", "name": "Title", "width": 400, "height": 300}}]))
                .unwrap(),
            )
            .unwrap()
            .created[0]
            .to_string();
        apply(
            &mut s,
            json!([
                {"op": "add_clip", "video": video, "frame": frame, "duration_ms": 1000},
                {"op": "add_clip", "video": video, "footage": {"file": "media/a.mp4", "in_ms": 0},
                 "duration_ms": 1000}
            ]),
        );
        let clip = s.document().videos[0].clips[1].id.to_string();
        apply(
            &mut s,
            json!([{"op": "set_clip", "video": video, "clip": clip,
                    "prop": {"name": "speed", "value": 2.0}}]),
        );
        let doc = s.document();
        let edl = edl(doc, &doc.videos[0], &|_| false);
        assert!(edl.text.contains("* FROM CLIP NAME: Title"));
        assert!(edl.text.contains("a design frame, no source file"));
        assert!(edl.text.contains("M2   AX       60.0"), "{}", edl.text);
        assert!(
            edl.text
                .contains("00:00:00:00 00:00:02:00 01:00:01:00 01:00:02:00"),
            "{}",
            edl.text
        );
        assert!(edl.warnings.iter().any(|w| w.contains("design frames")));
    }

    fn cue(start_ms: u32, end_ms: u32, text: &str) -> Cue {
        Cue {
            id: NodeId::parse("c1").unwrap(),
            start_ms,
            end_ms,
            text: text.into(),
            words: Vec::new(),
        }
    }

    #[test]
    fn subtitles_are_written_in_srt_and_webvtt_and_a_blank_line_cannot_end_a_cue_early() {
        let captions = Captions {
            cues: vec![
                cue(1_000, 3_500, "Hello\n\nworld"),
                cue(3_723_004, 3_725_000, "a --> b"),
            ],
            ..Captions::default()
        };
        assert_eq!(
            srt(&captions),
            "1\n00:00:01,000 --> 00:00:03,500\nHello\nworld\n\n2\n01:02:03,004 --> 01:02:05,000\na --> b\n\n"
        );
        assert_eq!(
            vtt(&captions),
            "WEBVTT\n\n00:00:01.000 --> 00:00:03.500\nHello\nworld\n\n01:02:03.004 --> 01:02:05.000\na \u{2192} b\n\n"
        );
        assert_eq!(srt(&Captions::default()), "");
    }
}

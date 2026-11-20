//! The sound under a video's preview: the footage clips' own sound and the sound track, mixed once
//! into a WAV in a folder the caller owns (`od_media::mix_wav`, so what is heard is what
//! exports). The WAV is named by what went into it, so an edit that leaves the sound alone does
//! not mix again.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use od_core::{NodeId, Video};
use od_media::{AudioPiece, Ffmpeg};

use crate::tools::ToolError;
use crate::video_export::{Source, footage_piece, piece_of, retime_clip_sound};

/// The pieces of sound `video` plays: each footage clip's sound (retimed clips through a WAV
/// named by what went into it, in `dir`), at its place on the timeline, and the sound tracks cut
/// at the end of the video.
fn pieces(
    src: &Source,
    video: &Video,
    ffmpeg: &Ffmpeg,
    dir: &Path,
) -> Result<Vec<AudioPiece>, ToolError> {
    let mut out = Vec::new();
    for (clip, placed) in video.clips.iter().zip(video.placed()) {
        let Some(footage) = &clip.footage else {
            continue;
        };
        let project = src
            .dir()
            .ok_or_else(|| ToolError("the project has no folder, so no footage".into()))?;
        let file =
            od_media::footage_path(project, &footage.file).map_err(|e| ToolError(e.to_string()))?;
        let info = ffmpeg
            .probe_file(&file)
            .map_err(|e| ToolError(e.to_string()))?;
        if !info.has_audio {
            continue;
        }
        let length = u64::from(clip.duration_ms);
        if clip.retimed() {
            let wav = retimed_wav(ffmpeg, clip, &file, footage.in_ms, dir)?;
            out.push(footage_piece(wav, 0, placed.start_ms, length));
        } else {
            out.push(footage_piece(file, footage.in_ms, placed.start_ms, length));
        }
    }
    for track in video.sound_tracks() {
        let project = src
            .dir()
            .ok_or_else(|| ToolError("the project has no folder, so no sound track".into()))?;
        let file =
            od_media::audio_path(project, &track.file).map_err(|e| ToolError(e.to_string()))?;
        let length = ffmpeg
            .probe_duration(&file)
            .map_err(|e| ToolError(e.to_string()))?;
        let start = u64::from(track.start_ms);
        let room = video.duration_ms().saturating_sub(start);
        let played = track.played_ms(length, room);
        if played > 0 {
            out.push(piece_of(&track, file, start, played));
        }
    }
    Ok(out)
}

fn retimed_wav(
    ffmpeg: &Ffmpeg,
    clip: &od_core::Clip,
    file: &Path,
    in_ms: u64,
    dir: &Path,
) -> Result<PathBuf, ToolError> {
    let mut hasher = DefaultHasher::new();
    (
        file,
        in_ms,
        clip.duration_ms,
        format!("{:?}{:?}", clip.speed, clip.speed_ramp),
    )
        .hash(&mut hasher);
    let wav = dir.join(format!("retimed-{:016x}.wav", hasher.finish()));
    if !wav.is_file() {
        retime_clip_sound(ffmpeg, clip, file, in_ms, &wav)?;
    }
    Ok(wav)
}

/// Whether `video` can have sound at all: a sound track, or footage that may carry its own.
pub fn may_have_sound(src: &Source, video: &NodeId) -> bool {
    src.doc.video(video).is_some_and(|v| {
        !v.sound_tracks().is_empty() || v.clips.iter().any(|c| c.footage.is_some())
    })
}

/// What the mix is made from: the pieces and their files' sizes and times.
fn mix_key(pieces: &[AudioPiece], duration_ms: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    duration_ms.hash(&mut hasher);
    for p in pieces {
        let stamp = std::fs::metadata(&p.file).ok().map(|m| {
            (
                m.len(),
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos()),
            )
        });
        (
            &p.file,
            p.in_ms,
            p.start_ms,
            p.duration_ms,
            p.gain.to_bits(),
            p.fade_in_ms,
            p.fade_out_ms,
            format!("{:?}{:?}", p.role, p.duck),
            p.pan.to_bits(),
            stamp,
        )
            .hash(&mut hasher);
    }
    hasher.finish()
}

/// Mixes the sound of `video` into `dir` and returns the WAV, or `None` when the video is silent.
/// A mix already there for the same sound is returned as it is.
pub fn prepare(
    src: &Source,
    video: &NodeId,
    ffmpeg: &Ffmpeg,
    dir: &Path,
) -> Result<Option<PathBuf>, ToolError> {
    let v = src
        .doc
        .video(video)
        .ok_or_else(|| ToolError(format!("video {video} does not exist")))?;
    let pieces = pieces(src, v, ffmpeg, dir)?;
    if pieces.is_empty() {
        return Ok(None);
    }
    let wav = dir.join(format!(
        "preview-{:016x}.wav",
        mix_key(&pieces, v.duration_ms())
    ));
    if !wav.is_file() {
        od_media::mix_wav(ffmpeg, &pieces, v.duration_ms(), &wav)
            .map_err(|e| ToolError(e.to_string()))?;
    }
    Ok(Some(wav))
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::{Actor, Document, Operation, Session};

    fn silent_video() -> (Source, NodeId) {
        let mut s = Session::new(Document::new("a"));
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "a".into(),
                    fps: 10,
                    width: 100,
                    height: 100,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        (
            Source {
                doc: s.document().clone(),
                dir: None,
            },
            v,
        )
    }

    #[test]
    fn a_silent_video_has_no_mix_and_a_missing_video_is_an_error() {
        let Ok(ff) = Ffmpeg::locate() else {
            eprintln!("skipped: no ffmpeg on this machine");
            return;
        };
        let tmp = tempfile::tempdir().unwrap();
        let (src, v) = silent_video();
        assert_eq!(prepare(&src, &v, &ff, tmp.path()).unwrap(), None);
        let missing = NodeId::parse("v_999").unwrap();
        assert!(prepare(&src, &missing, &ff, tmp.path()).is_err());
    }

    #[test]
    fn the_mix_key_follows_the_sound_and_nothing_else() {
        let piece = AudioPiece {
            role: od_media::PieceRole::Music,
            pan: 0.0,
            duck: None,
            file: PathBuf::from("/nowhere/a.wav"),
            in_ms: 0,
            start_ms: 0,
            duration_ms: 1000,
            gain: 1.0,
            fade_in_ms: 0,
            fade_out_ms: 0,
        };
        let same = mix_key(std::slice::from_ref(&piece), 3000);
        assert_eq!(same, mix_key(std::slice::from_ref(&piece), 3000));
        let louder = AudioPiece {
            gain: 0.5,
            ..piece.clone()
        };
        assert_ne!(same, mix_key(&[louder], 3000));
        assert_ne!(same, mix_key(&[piece], 4000), "a longer video is a new mix");
    }
}

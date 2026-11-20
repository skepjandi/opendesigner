//! Several sound tracks under a video's picture: music, voice-over and effects, each with its own
//! start, trim, volume, fades and pan, and music that ducks under the voice
//! (`docs/wiki/video-editing.md#sound-tracks`).
//!
//! The older single `Video.audio` stays valid and is read as the first track (`audio`). Tracks are
//! data like everything else; they change as a whole through `set_video` `tracks`, and the
//! helpers here compute the new list for one add, change or removal so each is still one
//! attributed, undoable operation.

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::OpError;
use crate::model::NodeId;
use crate::video::{AudioTrack, MAX_IN_MS, Video, check_audio};

/// Tracks a video may have besides the legacy `audio`.
pub const MAX_TRACKS: usize = 8;
pub const MAX_DUCK_ATTACK_MS: u32 = 2_000;
pub const MAX_DUCK_RELEASE_MS: u32 = 5_000;
/// The id the legacy `Video.audio` goes by when it is read as a track.
pub const LEGACY_TRACK_ID: &str = "audio";

/// What a track is for. Music ducks under voice; voice and effects never do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrackRole {
    #[default]
    Music,
    Voice,
    Sfx,
}

impl TrackRole {
    pub const NAMES: [&'static str; 3] = ["music", "voice", "sfx"];

    pub fn name(self) -> &'static str {
        match self {
            TrackRole::Music => "music",
            TrackRole::Voice => "voice",
            TrackRole::Sfx => "sfx",
        }
    }
}

/// How a music track gets out of the way of the voice: the compressor of its mix listens to the
/// voice tracks and pulls the music down while they speak.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Duck {
    /// 0 is no ducking, 1 is as much as there is: the compressor ratio runs from 1:1 to 20:1.
    #[serde(default = "default_depth")]
    pub depth: f32,
    /// How fast the music goes down when the voice starts.
    #[serde(default = "default_attack")]
    pub attack_ms: u32,
    /// How slowly it comes back after the voice stops.
    #[serde(default = "default_release")]
    pub release_ms: u32,
}

fn default_depth() -> f32 {
    0.7
}
fn default_attack() -> u32 {
    40
}
fn default_release() -> u32 {
    500
}

impl Default for Duck {
    fn default() -> Duck {
        Duck {
            depth: default_depth(),
            attack_ms: default_attack(),
            release_ms: default_release(),
        }
    }
}

fn gain_one() -> f32 {
    1.0
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

fn is_unity(v: &f32) -> bool {
    (*v - 1.0).abs() < f32::EPSILON
}

/// One piece of sound on the timeline: a stretch of an audio file in `media/` placed at a time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundTrack {
    pub id: NodeId,
    #[serde(default)]
    pub role: TrackRole,
    /// `media/<name>.mp3|wav|m4a|aac|ogg`
    pub file: String,
    /// When it starts, counted from the beginning of the video.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub start_ms: u32,
    /// Where in the file it starts playing (trim in).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub in_ms: u32,
    /// Where in the file it stops (trim out); none plays to the end of the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_ms: Option<u32>,
    #[serde(default = "gain_one", skip_serializing_if = "is_unity")]
    pub gain: f32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub fade_in_ms: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub fade_out_ms: u32,
    /// -1 is all left, 1 all right, 0 as recorded.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pan: f32,
    /// Music only: lower under the voice tracks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duck: Option<Duck>,
}

impl SoundTrack {
    /// The legacy single track of a video, read as a music track named `audio`.
    pub fn from_legacy(a: &AudioTrack) -> Result<SoundTrack, OpError> {
        Ok(SoundTrack {
            id: NodeId::parse(LEGACY_TRACK_ID)?,
            role: TrackRole::Music,
            file: a.file.clone(),
            start_ms: a.start_ms,
            in_ms: 0,
            out_ms: None,
            gain: a.gain,
            fade_in_ms: a.fade_in_ms,
            fade_out_ms: a.fade_out_ms,
            pan: 0.0,
            duck: None,
        })
    }

    /// How long it plays when the file lasts `file_ms` and the video leaves `room_ms` after its
    /// start: the trimmed stretch, cut at the end of the video.
    pub fn played_ms(&self, file_ms: u64, room_ms: u64) -> u64 {
        let end = self.out_ms.map_or(file_ms, |o| u64::from(o).min(file_ms));
        end.saturating_sub(u64::from(self.in_ms)).min(room_ms)
    }
}

fn bad(what: &'static str, detail: String) -> OpError {
    OpError::OutOfBounds { what, detail }
}

/// A track names a plain audio file, starts inside the limits, trims to a real stretch, plays at a
/// volume between silence and twice the recording, pans between the speakers, and only music ducks.
pub fn check_track(t: &SoundTrack) -> Result<(), OpError> {
    check_audio(&AudioTrack {
        file: t.file.clone(),
        start_ms: t.start_ms,
        gain: t.gain,
        fade_in_ms: t.fade_in_ms,
        fade_out_ms: t.fade_out_ms,
    })?;
    if u64::from(t.in_ms) > MAX_IN_MS {
        return Err(bad(
            "track in_ms",
            format!("{} is later than {MAX_IN_MS} ms", t.in_ms),
        ));
    }
    if let Some(out) = t.out_ms
        && (out <= t.in_ms || u64::from(out) > MAX_IN_MS)
    {
        return Err(bad(
            "track out_ms",
            format!(
                "{out} must be after in_ms ({}) and within {MAX_IN_MS} ms",
                t.in_ms
            ),
        ));
    }
    if !t.pan.is_finite() || !(-1.0..=1.0).contains(&t.pan) {
        return Err(bad(
            "track pan",
            format!("{} is not a number from -1 to 1", t.pan),
        ));
    }
    if let Some(d) = &t.duck {
        check_duck(t.role, d)?;
    }
    Ok(())
}

fn check_duck(role: TrackRole, d: &Duck) -> Result<(), OpError> {
    if role != TrackRole::Music {
        return Err(bad(
            "track duck",
            "only a music track ducks under the voice".into(),
        ));
    }
    if !d.depth.is_finite() || !(0.0..=1.0).contains(&d.depth) {
        return Err(bad(
            "duck depth",
            format!("{} is not a number from 0 to 1", d.depth),
        ));
    }
    if !(1..=MAX_DUCK_ATTACK_MS).contains(&d.attack_ms) {
        return Err(bad(
            "duck attack_ms",
            format!("{} is not from 1 to {MAX_DUCK_ATTACK_MS}", d.attack_ms),
        ));
    }
    if !(10..=MAX_DUCK_RELEASE_MS).contains(&d.release_ms) {
        return Err(bad(
            "duck release_ms",
            format!("{} is not from 10 to {MAX_DUCK_RELEASE_MS}", d.release_ms),
        ));
    }
    Ok(())
}

/// The list of tracks of one video: at most [`MAX_TRACKS`], unique ids, each valid.
pub fn check_tracks(tracks: &[SoundTrack]) -> Result<(), OpError> {
    if tracks.len() > MAX_TRACKS {
        return Err(OpError::Limit(format!(
            "at most {MAX_TRACKS} sound tracks per video"
        )));
    }
    for (n, t) in tracks.iter().enumerate() {
        if t.id.as_str() == LEGACY_TRACK_ID || tracks[..n].iter().any(|p| p.id == t.id) {
            return Err(OpError::DuplicateId(t.id.to_string()));
        }
        check_track(t)?;
    }
    Ok(())
}

/// A new track: everything but the id, which is the first free `a1`, `a2`, ...
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewTrack {
    #[serde(default)]
    pub role: TrackRole,
    pub file: String,
    #[serde(default)]
    pub start_ms: u32,
    #[serde(default)]
    pub in_ms: u32,
    #[serde(default)]
    pub out_ms: Option<u32>,
    #[serde(default = "gain_one")]
    pub gain: f32,
    #[serde(default)]
    pub fade_in_ms: u32,
    #[serde(default)]
    pub fade_out_ms: u32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub duck: Option<Duck>,
}

/// A change to one track: a field left out stays as it is; `out_ms` and `duck` may be cleared
/// with an explicit `null`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackPatch {
    #[serde(default)]
    pub role: Option<TrackRole>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub start_ms: Option<u32>,
    #[serde(default)]
    pub in_ms: Option<u32>,
    #[serde(default, deserialize_with = "some_or_null")]
    pub out_ms: Option<Option<u32>>,
    #[serde(default)]
    pub gain: Option<f32>,
    #[serde(default)]
    pub fade_in_ms: Option<u32>,
    #[serde(default)]
    pub fade_out_ms: Option<u32>,
    #[serde(default)]
    pub pan: Option<f32>,
    #[serde(default, deserialize_with = "some_or_null")]
    pub duck: Option<Option<Duck>>,
}

/// Tells an absent field (leave alone) from an explicit `null` (clear it).
fn some_or_null<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

impl TrackPatch {
    fn apply(&self, t: &mut SoundTrack) {
        if let Some(v) = self.role {
            t.role = v;
        }
        if let Some(v) = &self.file {
            t.file = v.clone();
        }
        if let Some(v) = self.start_ms {
            t.start_ms = v;
        }
        if let Some(v) = self.in_ms {
            t.in_ms = v;
        }
        if let Some(v) = self.out_ms {
            t.out_ms = v;
        }
        if let Some(v) = self.gain {
            t.gain = v;
        }
        if let Some(v) = self.fade_in_ms {
            t.fade_in_ms = v;
        }
        if let Some(v) = self.fade_out_ms {
            t.fade_out_ms = v;
        }
        if let Some(v) = self.pan {
            t.pan = v;
        }
        if let Some(v) = self.duck {
            t.duck = v;
        }
    }
}

impl Video {
    /// Every sound track in play order: the legacy `audio` first, then `tracks`.
    pub fn sound_tracks(&self) -> Vec<SoundTrack> {
        self.audio
            .iter()
            .filter_map(|a| SoundTrack::from_legacy(a).ok())
            .chain(self.tracks.iter().cloned())
            .collect()
    }

    fn free_track_id(&self) -> Result<NodeId, OpError> {
        (1..)
            .map(|n| format!("a{n}"))
            .find(|id| self.tracks.iter().all(|t| t.id.as_str() != id))
            .map_or_else(
                || Err(OpError::Limit("no free track id".into())),
                |id| NodeId::parse(&id),
            )
    }

    /// The `tracks` after adding `new`, and the id the new track got. Apply the list with
    /// `set_video` `tracks`.
    pub fn tracks_with_added(&self, new: NewTrack) -> Result<(Vec<SoundTrack>, NodeId), OpError> {
        let id = self.free_track_id()?;
        let mut tracks = self.tracks.clone();
        tracks.push(SoundTrack {
            id: id.clone(),
            role: new.role,
            file: new.file,
            start_ms: new.start_ms,
            in_ms: new.in_ms,
            out_ms: new.out_ms,
            gain: new.gain,
            fade_in_ms: new.fade_in_ms,
            fade_out_ms: new.fade_out_ms,
            pan: new.pan,
            duck: new.duck,
        });
        check_tracks(&tracks)?;
        Ok((tracks, id))
    }

    /// The `tracks` after changing track `id` by `patch`.
    pub fn tracks_with_changed(
        &self,
        id: &NodeId,
        patch: &TrackPatch,
    ) -> Result<Vec<SoundTrack>, OpError> {
        let mut tracks = self.tracks.clone();
        let track = tracks
            .iter_mut()
            .find(|t| &t.id == id)
            .ok_or_else(|| no_such_track(id))?;
        patch.apply(track);
        check_tracks(&tracks)?;
        Ok(tracks)
    }

    /// The `tracks` after removing track `id`.
    pub fn tracks_with_removed(&self, id: &NodeId) -> Result<Vec<SoundTrack>, OpError> {
        let at = self
            .tracks
            .iter()
            .position(|t| &t.id == id)
            .ok_or_else(|| no_such_track(id))?;
        let mut tracks = self.tracks.clone();
        tracks.remove(at);
        Ok(tracks)
    }
}

fn no_such_track(id: &NodeId) -> OpError {
    let legacy = if id.as_str() == LEGACY_TRACK_ID {
        "; the older single sound track is changed with set_video audio"
    } else {
        ""
    };
    bad("track", format!("video has no sound track {id}{legacy}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str, file: &str) -> SoundTrack {
        SoundTrack {
            id: NodeId::parse(id).unwrap(),
            role: TrackRole::Music,
            file: file.into(),
            start_ms: 0,
            in_ms: 0,
            out_ms: None,
            gain: 1.0,
            fade_in_ms: 0,
            fade_out_ms: 0,
            pan: 0.0,
            duck: None,
        }
    }

    #[test]
    fn a_track_with_defaults_serializes_to_its_id_role_and_file() {
        let json = serde_json::to_value(track("a1", "media/x.mp3")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"id": "a1", "role": "music", "file": "media/x.mp3"})
        );
        let back: SoundTrack = serde_json::from_value(json).unwrap();
        assert_eq!(back, track("a1", "media/x.mp3"));
    }

    #[test]
    fn bounds_name_the_field_that_breaks_them() {
        let ok = track("a1", "media/x.mp3");
        assert!(check_track(&ok).is_ok());
        let cases: Vec<(SoundTrack, &str)> = vec![
            (track("a1", "x.mp3"), "audio"),
            (track("a1", "media/x.png"), "audio"),
            (
                SoundTrack {
                    gain: 2.5,
                    ..ok.clone()
                },
                "audio gain",
            ),
            (
                SoundTrack {
                    pan: 1.5,
                    ..ok.clone()
                },
                "track pan",
            ),
            (
                SoundTrack {
                    in_ms: 5_000,
                    out_ms: Some(5_000),
                    ..ok.clone()
                },
                "track out_ms",
            ),
            (
                SoundTrack {
                    duck: Some(Duck::default()),
                    role: TrackRole::Voice,
                    ..ok.clone()
                },
                "track duck",
            ),
            (
                SoundTrack {
                    duck: Some(Duck {
                        depth: 1.5,
                        ..Duck::default()
                    }),
                    ..ok.clone()
                },
                "duck depth",
            ),
            (
                SoundTrack {
                    duck: Some(Duck {
                        attack_ms: 0,
                        ..Duck::default()
                    }),
                    ..ok
                },
                "duck attack_ms",
            ),
        ];
        for (t, what) in cases {
            let err = check_track(&t).unwrap_err().to_string();
            assert!(err.contains(what), "{what}: {err}");
        }
    }

    #[test]
    fn the_list_is_capped_and_ids_are_unique_and_not_the_legacy_one() {
        let many: Vec<SoundTrack> = (0..=MAX_TRACKS)
            .map(|n| track(&format!("a{n}"), "media/x.mp3"))
            .collect();
        assert!(matches!(check_tracks(&many), Err(OpError::Limit(_))));
        let twice = vec![track("a1", "media/x.mp3"), track("a1", "media/y.mp3")];
        assert!(matches!(check_tracks(&twice), Err(OpError::DuplicateId(_))));
        let legacy = vec![track(LEGACY_TRACK_ID, "media/x.mp3")];
        assert!(check_tracks(&legacy).is_err());
    }

    #[test]
    fn a_patch_changes_only_what_it_names_and_null_clears() {
        let mut t = track("a1", "media/x.mp3");
        t.out_ms = Some(9_000);
        t.duck = Some(Duck::default());
        let patch: TrackPatch =
            serde_json::from_str(r#"{"gain": 0.3, "out_ms": null, "duck": null}"#).unwrap();
        patch.apply(&mut t);
        assert_eq!((t.gain, t.out_ms, t.duck), (0.3, None, None));
        assert_eq!(t.file, "media/x.mp3");
        let keep: TrackPatch = serde_json::from_str(r#"{"pan": 0.5}"#).unwrap();
        t.out_ms = Some(4_000);
        keep.apply(&mut t);
        assert_eq!((t.pan, t.out_ms), (0.5, Some(4_000)));
    }

    #[test]
    fn played_ms_is_the_trimmed_stretch_cut_at_the_end_of_the_video() {
        let mut t = track("a1", "media/x.mp3");
        t.in_ms = 1_000;
        t.out_ms = Some(6_000);
        assert_eq!(t.played_ms(10_000, 60_000), 5_000);
        assert_eq!(t.played_ms(10_000, 2_000), 2_000);
        assert_eq!(t.played_ms(4_000, 60_000), 3_000);
        t.out_ms = None;
        assert_eq!(t.played_ms(10_000, 60_000), 9_000);
    }
}

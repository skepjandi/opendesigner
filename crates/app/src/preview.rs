//! The Video tab's playback, without Qt: the sound mixed for the preview and how the playhead and
//! the sound player are kept together.
//!
//! The sound is mixed once per version of the video into a WAV in a temp folder (the same mix the
//! export makes) and QML plays it with Qt Multimedia. Two clocks then run, the preview's wall
//! clock and the player's position. [`step`] decides, each tick, where the playhead goes: the
//! player's position is the reference, and the playhead leans toward it by a small share of each
//! tick, so it neither staircases with a player that reports its position in coarse steps nor
//! drifts away from the sound. After a stall, a seek or a loop the two are far apart: the
//! playhead is right (it is where the user or the pictures are) and the player is told to jump.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use od_agent::video_export::Source;
use od_core::NodeId;
use od_media::Ffmpeg;
use serde_json::{Value, json};

/// Farther apart than this, the sound is moved to the playhead instead of the playhead leaning
/// toward the sound: a few frames of difference is drift, more is a jump.
pub const JUMP_LIMIT_MS: f64 = 150.0;
/// The share of a tick the playhead may lean toward the sound.
pub const LEAN: f64 = 0.1;

/// Where the playhead goes after one tick, and whether the sound must jump.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    pub playhead_ms: f64,
    /// Move the player to this position (milliseconds).
    pub seek_sound_ms: Option<f64>,
}

/// One tick of playback: `dt_ms` of wall clock passed. `sound_ms` is the player's position, or
/// `None` while no sound plays (no sound, still loading, or it ended before the video).
pub fn step(playhead_ms: f64, dt_ms: f64, sound_ms: Option<f64>) -> Step {
    let ahead = playhead_ms + dt_ms;
    let Some(sound) = sound_ms else {
        return Step {
            playhead_ms: ahead,
            seek_sound_ms: None,
        };
    };
    let drift = sound - ahead;
    if drift.abs() > JUMP_LIMIT_MS {
        return Step {
            playhead_ms: ahead,
            seek_sound_ms: Some(ahead.max(0.0)),
        };
    }
    let lean = LEAN * dt_ms;
    Step {
        playhead_ms: ahead + drift.clamp(-lean, lean),
        seek_sound_ms: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mix {
    /// Nothing asked for yet.
    Idle,
    Mixing,
    Ready(String),
    /// The video has no sound.
    Silent,
    Failed(String),
}

/// The sound of the video the Video tab shows, mixed on a worker thread.
/// A folder of our own in the system's temporary folder, removed when it is dropped.
#[derive(Debug)]
struct MixDir(PathBuf);

impl MixDir {
    fn new() -> std::io::Result<MixDir> {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "opendesigner-preview-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)?;
        Ok(MixDir(path))
    }
}

impl Drop for MixDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
pub struct AudioSlot {
    dir: Option<MixDir>,
    mix: Arc<Mutex<Option<Mix>>>,
}

impl AudioSlot {
    /// Starts mixing the sound of `video` from the copy `src`; a result of an earlier start that
    /// has not come in yet is dropped.
    pub fn start(&mut self, src: Source, video: &NodeId) -> Result<(), String> {
        if self.dir.is_none() {
            self.dir = Some(MixDir::new().map_err(|e| e.to_string())?);
        }
        let dir = self.dir.as_ref().map(|d| d.0.clone()).unwrap_or_default();
        let mix = Arc::new(Mutex::new(Some(Mix::Mixing)));
        self.mix = mix.clone();
        let video = video.clone();
        std::thread::Builder::new()
            .name("preview-audio".into())
            .spawn(move || {
                let result = mix_into(&src, &video, &dir);
                if let Ok(mut slot) = mix.lock() {
                    *slot = Some(result);
                }
            })
            .map(drop)
            .map_err(|e| e.to_string())
    }

    pub fn stop(&mut self) {
        self.mix = Arc::default();
    }

    /// `{"state": "none" | "mixing" | "ready" | "silent" | "error", "url": ..., "error": ...}`.
    pub fn status(&self) -> Value {
        let state = self.mix.lock().ok().and_then(|m| m.clone());
        match state.unwrap_or(Mix::Idle) {
            Mix::Idle => json!({"state": "none"}),
            Mix::Mixing => json!({"state": "mixing"}),
            Mix::Silent => json!({"state": "silent"}),
            Mix::Ready(url) => json!({"state": "ready", "url": url}),
            Mix::Failed(e) => json!({"state": "error", "error": e}),
        }
    }

    /// Waits for the mix (tests).
    #[cfg(test)]
    fn wait(&self) -> Value {
        for _ in 0..2000 {
            let status = self.status();
            if status["state"] != "mixing" {
                return status;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        self.status()
    }
}

fn mix_into(src: &Source, video: &NodeId, dir: &Path) -> Mix {
    if !od_agent::preview_audio::may_have_sound(src, video) {
        return Mix::Silent;
    }
    let ffmpeg = match Ffmpeg::locate() {
        Ok(f) => f,
        Err(e) => return Mix::Failed(e.to_string()),
    };
    match od_agent::preview_audio::prepare(src, video, &ffmpeg, dir) {
        Ok(Some(wav)) => {
            forget_older_mixes(dir, &wav);
            Mix::Ready(file_url(&wav))
        }
        Ok(None) => Mix::Silent,
        Err(e) => Mix::Failed(e.0),
    }
}

/// Deletes the earlier mixes of this folder: only the newest is played.
fn forget_older_mixes(dir: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path != keep && path.extension().is_some_and(|e| e == "wav") {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// `file://` URL of an absolute path (what a QML media player takes).
fn file_url(path: &Path) -> String {
    let mut url = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                url.push(char::from(byte));
            }
            other => url.push_str(&format!("%{other:02X}")),
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_sound_the_playhead_follows_the_clock() {
        assert_eq!(
            step(1000.0, 66.0, None),
            Step {
                playhead_ms: 1066.0,
                seek_sound_ms: None
            }
        );
    }

    #[test]
    fn a_sound_a_little_off_pulls_the_playhead_gently_and_never_jumps_it() {
        let ahead = step(1000.0, 66.0, Some(1120.0));
        assert!(ahead.playhead_ms > 1066.0 && ahead.playhead_ms <= 1066.0 + LEAN * 66.0 + 1e-9);
        assert_eq!(ahead.seek_sound_ms, None);
        let behind = step(1000.0, 66.0, Some(1000.0));
        assert!(behind.playhead_ms < 1066.0 && behind.playhead_ms >= 1066.0 - LEAN * 66.0 - 1e-9);
        // already together: nothing to correct
        let level = step(1000.0, 66.0, Some(1066.0));
        assert_eq!(level.playhead_ms, 1066.0);
    }

    #[test]
    fn a_sound_far_away_is_moved_to_the_playhead() {
        let s = step(5000.0, 66.0, Some(200.0));
        assert_eq!(s.playhead_ms, 5066.0);
        assert_eq!(s.seek_sound_ms, Some(5066.0));
        let after_loop = step(0.0, 66.0, Some(9000.0));
        assert_eq!(after_loop.seek_sound_ms, Some(66.0));
    }

    #[test]
    fn leaning_converges_on_the_sound_clock() {
        // the sound runs 8% slow against the wall clock; after many ticks the playhead stays
        // within the jump limit of it and never asks for a seek
        let (mut playhead, mut sound) = (0.0_f64, 0.0_f64);
        for _ in 0..600 {
            sound += 66.0 * 0.92;
            let s = step(playhead, 66.0, Some(sound));
            assert_eq!(s.seek_sound_ms, None);
            playhead = s.playhead_ms;
        }
        assert!(
            (playhead - sound).abs() < JUMP_LIMIT_MS,
            "{playhead} {sound}"
        );
    }

    #[test]
    fn file_urls_escape_what_a_url_cannot_hold() {
        assert_eq!(
            file_url(Path::new("/tmp/a b/é.wav")),
            "file:///tmp/a%20b/%C3%A9.wav"
        );
    }

    #[test]
    fn a_silent_video_is_silent_without_needing_ffmpeg_and_a_stopped_slot_has_none() {
        let mut slot = AudioSlot::default();
        assert_eq!(slot.status()["state"], "none");
        let mut s = od_core::Session::new(od_core::Document::new("a"));
        let v = s
            .apply(
                od_core::Actor::User,
                None,
                &[od_core::Operation::CreateVideo {
                    name: "a".into(),
                    fps: 10,
                    width: 100,
                    height: 100,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        let src = Source::of_document(s.document().clone());
        slot.start(src, &v).unwrap();
        assert_eq!(slot.wait()["state"], "silent");
        slot.stop();
        assert_eq!(slot.status()["state"], "none");
    }
}

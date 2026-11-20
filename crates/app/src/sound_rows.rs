//! What the Video tab's sound rows need that the project does not store: every sound track in
//! one list (the older single `audio` first) with the file's length and its waveform peaks. The
//! peaks come from `ffmpeg` (`od_agent::audio_tools::waveform_for`) and are kept until the file
//! changes; without ffmpeg a row simply has no waveform. Qt-free, so it is tested in plain Rust.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::SystemTime;

use serde_json::{Value, json};

/// Peaks per file: enough to draw a few minutes of sound at the timeline's widest zoom.
const BUCKETS: usize = 1_000;
/// Files larger than this are not decoded for a picture (the row is drawn without one).
const MAX_WAVEFORM_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
struct Waveform {
    file_ms: u64,
    peaks: Vec<f32>,
}

struct Entry {
    stamp: (u64, Option<SystemTime>),
    waveform: Option<Waveform>,
}

static CACHE: LazyLock<Mutex<HashMap<PathBuf, Entry>>> = LazyLock::new(Default::default);

fn stamp(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

/// The waveform of `reference` (`media/<file>`) in the project at `dir`, from the cache while the
/// file is unchanged.
fn waveform(dir: &Path, reference: &str) -> Option<Waveform> {
    let path = dir.join(reference);
    let stamp = stamp(&path)?;
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(entry) = cache.get(&path).filter(|e| e.stamp == stamp) {
        return entry.waveform.clone();
    }
    let computed = (stamp.0 <= MAX_WAVEFORM_FILE_BYTES)
        .then(|| compute(dir, reference))
        .flatten();
    cache.insert(
        path,
        Entry {
            stamp,
            waveform: computed.clone(),
        },
    );
    computed
}

fn compute(dir: &Path, reference: &str) -> Option<Waveform> {
    let (file_ms, peaks) = od_agent::audio_tools::waveform_of(dir, reference, BUCKETS).ok()?;
    let peaks = peaks
        .into_iter()
        .map(|p| (p * 100.0).round() / 100.0)
        .collect();
    Some(Waveform { file_ms, peaks })
}

fn row(dir: Option<&Path>, track: &Value, legacy: bool) -> Value {
    let mut row = track.clone();
    let file = track["file"].as_str().unwrap_or("");
    let wave = dir.and_then(|d| waveform(d, file));
    if let Some(obj) = row.as_object_mut() {
        obj.entry("id").or_insert_with(|| json!("audio"));
        obj.entry("role").or_insert_with(|| json!("music"));
        obj.insert("legacy".into(), json!(legacy));
        obj.insert("file_ms".into(), json!(wave.as_ref().map(|w| w.file_ms)));
        obj.insert(
            "peaks".into(),
            json!(wave.map(|w| w.peaks).unwrap_or_default()),
        );
    }
    row
}

/// Adds `sound_rows` to every video of `videos` (the `view::videos` JSON): the older `audio`
/// first, then `tracks`, each with `legacy`, `file_ms` and `peaks`.
pub fn annotate(videos: &mut Value, dir: Option<&Path>) {
    for video in videos.as_array_mut().into_iter().flatten() {
        let legacy = video.get("audio").filter(|a| !a.is_null()).cloned();
        let tracks = video
            .get("tracks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let rows: Vec<Value> = legacy
            .iter()
            .map(|a| row(dir, a, true))
            .chain(tracks.iter().map(|t| row(dir, t, false)))
            .collect();
        if let Some(obj) = video.as_object_mut() {
            obj.insert("sound_rows".into(), Value::Array(rows));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_json() -> Value {
        json!([{
            "id": "v_1",
            "audio": {"file": "media/old.wav", "start_ms": 0, "gain": 1.0, "fade_in_ms": 0, "fade_out_ms": 0},
            "tracks": [{"id": "a1", "role": "voice", "file": "media/vo.wav", "start_ms": 500}],
        }])
    }

    #[test]
    fn the_older_track_comes_first_and_every_row_says_what_it_is() {
        let mut videos = video_json();
        annotate(&mut videos, None);
        let rows = videos[0]["sound_rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (rows[0]["id"].clone(), rows[0]["legacy"].clone()),
            (json!("audio"), json!(true))
        );
        assert_eq!(rows[0]["role"], "music");
        assert_eq!(
            (rows[1]["id"].clone(), rows[1]["role"].clone()),
            (json!("a1"), json!("voice"))
        );
        assert!(rows[1]["peaks"].as_array().unwrap().is_empty());
        assert!(rows[1]["file_ms"].is_null());
    }

    #[test]
    fn a_video_with_no_sound_has_no_rows() {
        let mut videos = json!([{"id": "v_1", "audio": null, "tracks": []}]);
        annotate(&mut videos, None);
        assert!(videos[0]["sound_rows"].as_array().unwrap().is_empty());
    }

    #[test]
    fn peaks_and_length_come_from_the_file_when_ffmpeg_is_there_and_are_kept() {
        let tmp = tempfile::tempdir().unwrap();
        if od_agent::audio_tools::waveform_for(tmp.path(), "media/missing.wav", 100).is_ok() {
            return;
        }
        std::fs::create_dir_all(tmp.path().join("media")).unwrap();
        let spec = od_agent::synth::Spec {
            kind: od_agent::synth::Kind::Hit,
            duration_ms: 900,
            seed: 1,
            pitch_hz: None,
            bpm: None,
            bars: None,
            stereo: false,
        };
        let made = od_agent::synth::make(tmp.path(), &spec, Some("hit")).unwrap();
        let mut videos = json!([{
            "id": "v_1", "audio": null,
            "tracks": [{"id": "a1", "role": "sfx", "file": made.file}],
        }]);
        annotate(&mut videos, Some(tmp.path()));
        let row = &videos[0]["sound_rows"][0];
        assert_eq!(row["peaks"].as_array().unwrap().len(), BUCKETS);
        assert!((800..=1000).contains(&row["file_ms"].as_u64().unwrap()));
        assert!(
            row["peaks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p.as_f64().unwrap() > 0.3)
        );

        let again = {
            let mut v = videos.clone();
            std::fs::remove_file(tmp.path().join(&made.file)).unwrap();
            annotate(&mut v, Some(tmp.path()));
            v
        };
        assert!(
            again[0]["sound_rows"][0]["peaks"]
                .as_array()
                .unwrap()
                .is_empty(),
            "a missing file has no picture"
        );
    }
}

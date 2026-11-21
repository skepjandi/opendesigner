#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Mixing, pause detection and waveforms against a real `ffmpeg`. Skipped when there is none.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use od_media::{AudioPiece, DuckSettings, Ffmpeg, Loudness, PieceRole, add_audio};

fn ffmpeg() -> Option<Ffmpeg> {
    Ffmpeg::locate().ok()
}

fn make(ff: &Ffmpeg, args: &[&str], out: &Path) {
    let status = Command::new(&ff.path)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .arg(out)
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg {args:?}");
}

fn tone(ff: &Ffmpeg, dir: &Path, name: &str, hz: u32, secs: u32, volume: f32) -> PathBuf {
    let path = dir.join(name);
    let source = format!("sine=f={hz}:d={secs}:r=48000");
    make(
        ff,
        &[
            "-f",
            "lavfi",
            "-i",
            &source,
            "-af",
            &format!("volume={volume}"),
        ],
        &path,
    );
    path
}

fn picture(ff: &Ffmpeg, dir: &Path, secs: u32) -> PathBuf {
    let path = dir.join("picture.mp4");
    let source = format!("color=c=black:s=64x64:d={secs}:r=10");
    make(ff, &["-f", "lavfi", "-i", &source, "-c:v", "mpeg4"], &path);
    path
}

fn piece(file: PathBuf, role: PieceRole, start_ms: u64, duration_ms: u64) -> AudioPiece {
    AudioPiece {
        file,
        role,
        in_ms: 0,
        start_ms,
        duration_ms,
        gain: 1.0,
        fade_in_ms: 0,
        fade_out_ms: 0,
        pan: 0.0,
        duck: None,
    }
}

/// The root-mean-square level (0 to 1) of `from_ms..to_ms` of the low band (under 600 Hz) of the
/// sound in `file`: the music, not the voice, in the tests below.
fn low_band_level(ff: &Ffmpeg, file: &Path, from_ms: u64, to_ms: u64) -> f64 {
    let out = Command::new(&ff.path)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(file)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            "8000",
            "-af",
            "lowpass=f=600",
            "-f",
            "s16le",
            "-",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let samples: Vec<f64> = out
        .stdout
        .chunks_exact(2)
        .map(|b| f64::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
        .collect();
    let slice = &samples[(from_ms * 8) as usize..(to_ms * 8) as usize];
    (slice.iter().map(|s| s * s).sum::<f64>() / slice.len() as f64).sqrt()
}

fn mix_with_voice(ff: &Ffmpeg, dir: &Path, name: &str, duck: Option<DuckSettings>) -> PathBuf {
    let music = tone(ff, dir, "music.wav", 220, 6, 3.0);
    let voice = tone(ff, dir, "voice.wav", 1700, 2, 4.0);
    let mut m = piece(music, PieceRole::Music, 0, 6000);
    m.duck = duck;
    let v = piece(voice, PieceRole::Voice, 2000, 2000);
    let out = dir.join(name);
    add_audio(ff, &picture(ff, dir, 6), &[m, v], None, &out).unwrap();
    out
}

#[test]
fn the_mix_has_the_pictures_length_and_music_ducks_under_the_voice() {
    let Some(ff) = ffmpeg() else { return };
    let dir = tempfile::tempdir().unwrap();
    let duck = DuckSettings {
        depth: 1.0,
        attack_ms: 20,
        release_ms: 200,
    };
    let ducked = mix_with_voice(&ff, dir.path(), "ducked.mp4", Some(duck));
    let plain = mix_with_voice(&ff, dir.path(), "plain.mp4", None);

    let length = ff.probe_duration(&ducked).unwrap();
    assert!((5800..=6400).contains(&length), "{length} ms");

    let (beside, under) = (
        low_band_level(&ff, &ducked, 500, 1500),
        low_band_level(&ff, &ducked, 2500, 3500),
    );
    assert!(beside > 0.1, "the music alone is audible: {beside}");
    assert!(
        under < beside * 0.5,
        "music under the voice {under} vs alone {beside}"
    );
    let after = low_band_level(&ff, &ducked, 5000, 5800);
    assert!(after > beside * 0.8, "the music comes back: {after}");

    let undisturbed = low_band_level(&ff, &plain, 2500, 3500);
    assert!(
        undisturbed > beside * 0.8,
        "without ducking it stays: {undisturbed}"
    );
}

#[test]
fn the_picture_is_copied_and_the_mix_survives_pan_trim_and_fades() {
    let Some(ff) = ffmpeg() else { return };
    let dir = tempfile::tempdir().unwrap();
    let pic = picture(&ff, dir.path(), 3);
    let sound = tone(&ff, dir.path(), "a.wav", 440, 5, 0.8);
    let mut p = piece(sound, PieceRole::Sfx, 500, 2000);
    (p.in_ms, p.pan, p.gain, p.fade_in_ms, p.fade_out_ms) = (1000, -0.5, 0.7, 200, 200);
    let out = dir.path().join("out.mp4");
    add_audio(&ff, &pic, &[p], None, &out).unwrap();
    let info = ff.probe_file(&out).unwrap();
    assert!(info.has_audio);
    assert_eq!((info.width, info.height), (64, 64));
    let log = Command::new(&ff.path)
        .args(["-hide_banner", "-i"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&log.stderr).contains("mpeg4"),
        "the picture was re-encoded"
    );
}

#[test]
fn pauses_are_found_between_sounds() {
    let Some(ff) = ffmpeg() else { return };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("speech.wav");
    make(
        &ff,
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=f=300:d=1:r=48000",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=mono:d=1",
            "-f",
            "lavfi",
            "-i",
            "sine=f=300:d=1:r=48000",
            "-filter_complex",
            "[0:a][1:a][2:a]concat=n=3:v=0:a=1",
        ],
        &path,
    );
    let pauses = ff.detect_pauses(&path, -30.0, 300).unwrap();
    assert_eq!(pauses.len(), 1, "{pauses:?}");
    assert!((900..=1100).contains(&pauses[0].start_ms), "{pauses:?}");
    assert!((1900..=2100).contains(&pauses[0].end_ms), "{pauses:?}");
    assert!(
        ff.detect_pauses(&dir.path().join("missing.wav"), -30.0, 300)
            .is_err()
    );

    let peaks = ff.waveform(&path, 300).unwrap();
    assert_eq!(peaks.len(), 300);
    assert!(
        peaks[50] > 0.1 && peaks[150] < 0.01 && peaks[250] > 0.1,
        "{:?}",
        (peaks[50], peaks[150], peaks[250])
    );
    assert!(ff.waveform(&path, 10).is_err());
}

fn measured_after(ff: &Ffmpeg, dir: &Path, level: f32, target: Loudness) -> f64 {
    let pic = picture(ff, dir, 6);
    let source = tone(ff, dir, "level.wav", 440, 5, level);
    let out = dir.join(format!("loud{level}.mp4"));
    let p = piece(source, PieceRole::Music, 0, 5000);
    add_audio(ff, &pic, &[p], Some(target), &out).unwrap();
    ff.measure_loudness(&out).unwrap()
}

#[test]
fn each_loudness_target_is_reached_within_one_and_a_half_lu() {
    let Some(ff) = ffmpeg() else { return };
    let d = tempfile::tempdir().unwrap();
    for (target, level) in [
        (Loudness::Social, 0.05),
        (Loudness::Social, 0.9),
        (Loudness::Podcast, 0.2),
        (Loudness::Broadcast, 0.5),
    ] {
        let got = measured_after(&ff, d.path(), level, target);
        assert!(
            (got - target.lufs()).abs() <= 1.5,
            "{target:?} from level {level}: measured {got} LUFS"
        );
    }
}

#[test]
fn without_a_target_the_level_is_left_as_mixed() {
    let Some(ff) = ffmpeg() else { return };
    let d = tempfile::tempdir().unwrap();
    let pic = picture(&ff, d.path(), 6);
    let source = tone(&ff, d.path(), "quiet.wav", 440, 5, 0.05);
    let out = d.path().join("plain.mp4");
    let p = piece(source, PieceRole::Music, 0, 5000);
    add_audio(&ff, &pic, &[p], None, &out).unwrap();
    assert!(ff.measure_loudness(&out).unwrap() < -25.0);
}

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_media::{EncodeSettings, Ffmpeg, MediaError, VideoWriter};

/// A stand-in for ffmpeg: it answers `-version` and `-encoders`, and otherwise copies its input
/// to the last argument, so the plumbing is tested without a real encoder.
#[cfg(unix)]
fn fake_ffmpeg(dir: &std::path::Path, encoders: &str, exit: i32) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("ffmpeg");
    let script = format!(
        r#"#!/bin/sh
case "$*" in
  *-version*) echo "ffmpeg version 99.0 fake"; echo "configuration: --disable-gpl";;
  *-encoders*) printf ' V....D %s  encoder\n' {encoders};;
  *) for last; do :; done
     if [ {exit} -ne 0 ]; then echo "boom: bad pixel format" >&2; exit {exit}; fi
     cat > "$last";;
esac
"#
    );
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn settings(dir: &std::path::Path) -> EncodeSettings {
    EncodeSettings {
        width: 4,
        height: 2,
        fps: 10,
        output: dir.join("out.mp4"),
    }
}

#[cfg(unix)]
#[test]
fn frames_reach_the_encoder_in_order_and_the_file_appears_only_when_it_finishes() {
    let tmp = tempfile::tempdir().unwrap();
    let ff = Ffmpeg::probe(&fake_ffmpeg(tmp.path(), "h264_videotoolbox libx264", 0)).unwrap();
    assert!(!ff.gpl);
    assert_eq!(
        ff.h264_encoder().unwrap(),
        "h264_videotoolbox",
        "the OS encoder is preferred"
    );

    let mut w = VideoWriter::start(&ff, &settings(tmp.path())).unwrap();
    w.write_frame(&[1; 32]).unwrap();
    w.write_frame(&[2; 32]).unwrap();
    assert!(
        !tmp.path().join("out.mp4").exists(),
        "not there until the end"
    );
    let bytes = w.finish().unwrap();
    assert_eq!(bytes, 64);
    let written = std::fs::read(tmp.path().join("out.mp4")).unwrap();
    assert_eq!(&written[..32], &[1; 32]);
    assert_eq!(&written[32..], &[2; 32]);
    assert!(!tmp.path().join("out.mp4.part").exists());
}

#[cfg(unix)]
#[test]
fn a_wrong_sized_frame_is_refused_and_an_encoder_failure_says_what_ffmpeg_said() {
    let tmp = tempfile::tempdir().unwrap();
    let ff = Ffmpeg::probe(&fake_ffmpeg(tmp.path(), "libx264", 0)).unwrap();
    let mut w = VideoWriter::start(&ff, &settings(tmp.path())).unwrap();
    assert!(w.write_frame(&[0; 5]).is_err());
    w.abort();
    assert!(
        !tmp.path().join("out.mp4.part").exists(),
        "abort leaves nothing behind"
    );

    let bad = tempfile::tempdir().unwrap();
    let ff = Ffmpeg::probe(&fake_ffmpeg(bad.path(), "libx264", 1)).unwrap();
    let mut w = VideoWriter::start(&ff, &settings(bad.path())).unwrap();
    let _ = w.write_frame(&[0; 32]);
    let err = w.finish().unwrap_err();
    assert!(matches!(err, MediaError::Failed(_)));
    assert!(err.to_string().contains("bad pixel format"), "{err}");
    assert!(!bad.path().join("out.mp4").exists());
    assert!(!bad.path().join("out.mp4.part").exists());
}

#[cfg(unix)]
#[test]
fn an_ffmpeg_without_an_h264_encoder_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let ff = Ffmpeg::probe(&fake_ffmpeg(tmp.path(), "libvpx mpeg4", 0)).unwrap();
    let err = VideoWriter::start(&ff, &settings(tmp.path()))
        .err()
        .unwrap();
    assert!(matches!(err, MediaError::NoEncoder(_)), "{err:?}");
}

/// With a real ffmpeg on this machine: encode ten frames and have ffmpeg read them back.
#[test]
fn a_real_ffmpeg_makes_an_mp4_with_every_frame() {
    let Ok(ff) = Ffmpeg::locate() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    let Ok(_) = ff.h264_encoder() else {
        eprintln!("skipped: {} has no H.264 encoder", ff.version);
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let s = EncodeSettings {
        width: 64,
        height: 48,
        fps: 10,
        output: tmp.path().join("real.mp4"),
    };
    let mut w = VideoWriter::start(&ff, &s).unwrap();
    for i in 0..10u8 {
        let frame: Vec<u8> = (0..64 * 48)
            .flat_map(|_| [i * 20, 0, 255 - i * 20, 255])
            .collect();
        w.write_frame(&frame).unwrap();
    }
    let bytes = w.finish().unwrap();
    assert!(bytes > 100);
    let data = std::fs::read(&s.output).unwrap();
    assert_eq!(&data[4..8], b"ftyp", "an MP4 container");
    let out = std::process::Command::new(&ff.path)
        .args(["-hide_banner", "-i"])
        .arg(&s.output)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr);
    assert!(log.contains("h264"), "{log}");
    assert!(log.contains("64x48"), "{log}");
    assert!(
        log.lines()
            .rev()
            .any(|l| l.contains("frame=") && l.contains("10")),
        "{log}"
    );
}

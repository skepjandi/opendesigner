//! A part of a video, every Nth frame, and the GIF, WebM and ProRes files come out of
//! `export_video` as asked, with transparency kept. The tests need `ffmpeg`; without one they say
//! so and pass.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::{Command, Stdio};

use od_agent::video_export::{self, Source, VideoOptions};
use od_core::{
    Actor, AudioTrack, NewNode, NodeId, NodeKind, Operation, Paint, VideoProperty, Workspace,
};
use od_media::Ffmpeg;

const W: usize = 64;
const H: usize = 48;

struct Project {
    _dir: tempfile::TempDir,
    ws: Workspace,
    video: NodeId,
}

/// A 1.5 s video at 10 fps whose only clip shows a frame with no fill and a red rectangle over
/// its left half, plus a two second tone as the video's sound.
fn project() -> Project {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.od");
    let mut ws = Workspace::create(&path, "Formats").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.name = Some("clear".into());
    frame.width = W as f64;
    frame.height = H as f64;
    let mut half = NewNode::new(NodeKind::Rect);
    half.width = (W / 2) as f64;
    half.height = H as f64;
    half.fill = Paint::parse("#ff0000").ok();
    frame.children.push(half);
    let frame = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: frame,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    let video = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "V".into(),
                fps: 10,
                width: W as u32,
                height: H as u32,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: video.clone(),
                frame: Some(frame),
                footage: None,
                duration_ms: 1500,
                index: None,
                transition: None,
                motion: None,
            }],
        )
        .unwrap();
    std::fs::create_dir_all(path.join("media")).unwrap();
    Project {
        _dir: dir,
        ws,
        video,
    }
}

fn add_tone(p: &mut Project, ff: &Ffmpeg) {
    let dir = p.ws.dir().unwrap().to_path_buf();
    let status = Command::new(&ff.path)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=f=440:d=2:r=16000",
        ])
        .arg(dir.join("media/tone.wav"))
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    p.ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::SetVideo {
                id: p.video.clone(),
                prop: VideoProperty::Audio(Some(AudioTrack {
                    file: "media/tone.wav".into(),
                    start_ms: 0,
                    gain: 1.0,
                    fade_in_ms: 0,
                    fade_out_ms: 0,
                })),
            }],
        )
        .unwrap();
}

fn export(
    p: &Project,
    ff: &Ffmpeg,
    file: &str,
    options: &VideoOptions,
) -> Result<video_export::VideoOutcome, String> {
    video_export::export_video_custom(
        &Source::of(&p.ws),
        &p.video,
        file,
        None,
        None,
        ff,
        2,
        None,
        options,
        &mut |_, _| true,
    )
    .map_err(|e| e.0)
}

fn rgba_first_frame(ff: &Ffmpeg, file: &Path, width: usize, height: usize) -> Vec<u8> {
    let out = Command::new(&ff.path)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(file)
        .args(["-frames:v", "1", "-pix_fmt", "rgba", "-f", "rawvideo", "-"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        out.stdout.len(),
        width * height * 4,
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn alpha_at(rgba: &[u8], width: usize, height: usize, x: usize) -> u8 {
    rgba[(height / 2 * width + x) * 4 + 3]
}

fn ffmpeg() -> Option<Ffmpeg> {
    let ff = Ffmpeg::locate().ok();
    if ff.is_none() {
        eprintln!("skipped: no ffmpeg on this machine");
    }
    ff
}

#[test]
fn a_transparent_gif_of_every_other_frame_is_scaled_and_keeps_its_clear_half() {
    let Some(ff) = ffmpeg() else { return };
    let p = project();
    let out = export(
        &p,
        &ff,
        "loop.gif",
        &VideoOptions {
            transparent: true,
            every_nth_frame: Some(2),
            width: Some(32),
            ..VideoOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        (out.format, out.transparent, out.every_nth_frame),
        ("gif", true, 2)
    );
    assert_eq!((out.width, out.height, out.frames), (32, 24, 8));
    let file = p.ws.dir().unwrap().join("exports/loop.gif");
    let picture = rgba_first_frame(&ff, &file, 32, 24);
    assert_eq!(
        (
            alpha_at(&picture, 32, 24, 3),
            alpha_at(&picture, 32, 24, 29)
        ),
        (255, 0)
    );
    let log = Command::new(&ff.path)
        .args(["-hide_banner", "-i"])
        .arg(&file)
        .args(["-f", "null", "-"])
        .output()
        .unwrap()
        .stderr;
    let log = String::from_utf8_lossy(&log);
    let frames: u64 = log
        .lines()
        .rev()
        .find_map(|l| {
            l.split("frame=")
                .nth(1)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
        .unwrap_or(0);
    assert_eq!(frames, 8, "{log}");
}

#[test]
fn a_transparent_prores_mov_and_a_webm_keep_their_alpha() {
    let Some(ff) = ffmpeg() else { return };
    let p = project();
    let transparent = VideoOptions {
        transparent: true,
        ..VideoOptions::default()
    };
    if ff.has_encoder("prores_ks") {
        export(&p, &ff, "keyed.mov", &transparent).unwrap();
        let file = p.ws.dir().unwrap().join("exports/keyed.mov");
        let picture = rgba_first_frame(&ff, &file, W, H);
        assert!(alpha_at(&picture, W, H, 4) > 250 && alpha_at(&picture, W, H, 60) < 5);
    }
    if ff.has_encoder("libvpx-vp9") {
        export(&p, &ff, "keyed.webm", &transparent).unwrap();
        let file = p.ws.dir().unwrap().join("exports/keyed.webm");
        let out = Command::new(&ff.path)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-c:v",
                "libvpx-vp9",
                "-i",
            ])
            .arg(&file)
            .args(["-frames:v", "1", "-pix_fmt", "rgba", "-f", "rawvideo", "-"])
            .output()
            .unwrap();
        assert!(alpha_at(&out.stdout, W, H, 4) > 250 && alpha_at(&out.stdout, W, H, 60) < 5);
    }
}

#[test]
fn a_part_of_the_video_is_written_with_the_part_of_the_sound_under_it() {
    let Some(ff) = ffmpeg() else { return };
    if ff.h264_encoder().is_err() {
        eprintln!("skipped: no H.264 encoder");
        return;
    }
    let mut p = project();
    add_tone(&mut p, &ff);
    let out = export(
        &p,
        &ff,
        "part.mp4",
        &VideoOptions {
            from_ms: Some(300),
            to_ms: Some(800),
            ..VideoOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        (out.from_ms, out.to_ms, out.frames, out.audio_pieces),
        (300, 800, 5, 1)
    );
    let info = ff
        .probe_file(&p.ws.dir().unwrap().join("exports/part.mp4"))
        .unwrap();
    assert!(info.has_audio);
    assert!(
        info.duration_ms.abs_diff(500) <= 60,
        "{} ms",
        info.duration_ms
    );

    let sped = export(
        &p,
        &ff,
        "fast.mp4",
        &VideoOptions {
            every_nth_frame: Some(3),
            ..VideoOptions::default()
        },
    )
    .unwrap();
    assert_eq!(sped.audio_pieces, 0);
    assert!(
        sped.warnings
            .iter()
            .any(|w| w.contains("sound was left out"))
    );
    let sped_info = ff
        .probe_file(&p.ws.dir().unwrap().join("exports/fast.mp4"))
        .unwrap();
    assert!(!sped_info.has_audio);
    assert!(
        sped_info.duration_ms.abs_diff(1500) <= 120,
        "{}",
        sped_info.duration_ms
    );
}

#[test]
fn what_cannot_be_written_is_refused_before_anything_is() {
    let Some(ff) = ffmpeg() else { return };
    let p = project();
    for (file, options, expect) in [
        (
            "a.mp4",
            VideoOptions {
                transparent: true,
                ..VideoOptions::default()
            },
            "cannot hold transparency",
        ),
        (
            "a.mp4",
            VideoOptions {
                width: Some(32),
                ..VideoOptions::default()
            },
            "width is for",
        ),
        (
            "a.mp4",
            VideoOptions {
                gif_loops: Some(1),
                ..VideoOptions::default()
            },
            "gif_loops is for a .gif",
        ),
        (
            "a.gif",
            VideoOptions {
                width: Some(500),
                ..VideoOptions::default()
            },
            "width is 16 to 64",
        ),
        (
            "a.mp4",
            VideoOptions {
                from_ms: Some(900),
                to_ms: Some(900),
                ..VideoOptions::default()
            },
            "empty",
        ),
        (
            "a.mp4",
            VideoOptions {
                from_ms: Some(5000),
                ..VideoOptions::default()
            },
            "empty",
        ),
        (
            "a.mp4",
            VideoOptions {
                every_nth_frame: Some(0),
                ..VideoOptions::default()
            },
            "every_nth_frame is 1 to 60",
        ),
        ("a.avi", VideoOptions::default(), "plain name"),
        ("../a.gif", VideoOptions::default(), "plain name"),
    ] {
        let err = export(&p, &ff, file, &options).unwrap_err();
        assert!(err.contains(expect), "{file}: {err}");
    }
    assert!(
        !p.ws.dir().unwrap().join("exports").exists()
            || std::fs::read_dir(p.ws.dir().unwrap().join("exports"))
                .unwrap()
                .next()
                .is_none()
    );
}

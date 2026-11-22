//! A transparent picture written as a GIF, a WebM and a MOV keeps its transparency, and every Nth
//! frame plays at the lower rate. The tests need `ffmpeg`; without one (or without the encoder)
//! they say so and pass.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::{Command, Stdio};

use od_media::{Container, EncodeSettings, Ffmpeg, VideoWriter};

const W: usize = 64;
const H: usize = 48;

/// Left half opaque red, right half clear, as straight RGBA.
fn frame() -> Vec<u8> {
    (0..W * H)
        .flat_map(|i| {
            if i % W < W / 2 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 0]
            }
        })
        .collect()
}

fn write(
    ff: &Ffmpeg,
    dir: &Path,
    name: &str,
    container: Container,
    alpha: bool,
    frames: usize,
    nth: u32,
) -> std::path::PathBuf {
    let settings = EncodeSettings {
        width: W as u32,
        height: H as u32,
        fps: 10,
        output: dir.join(name),
        container,
        every_nth: nth,
        alpha,
        ..EncodeSettings::default()
    };
    let mut writer = VideoWriter::start(ff, &settings).unwrap();
    for _ in 0..frames {
        writer.write_frame(&frame()).unwrap();
    }
    writer.finish().unwrap();
    settings.output
}

/// The first picture of `file` as RGBA, decoded with `decoder` when one is named.
fn first_picture(ff: &Ffmpeg, file: &Path, decoder: Option<&str>) -> Vec<u8> {
    let mut command = Command::new(&ff.path);
    command.args(["-hide_banner", "-loglevel", "error"]);
    if let Some(d) = decoder {
        command.args(["-c:v", d]);
    }
    let out = command
        .arg("-i")
        .arg(file)
        .args(["-frames:v", "1", "-pix_fmt", "rgba", "-f", "rawvideo", "-"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        out.stdout.len(),
        W * H * 4,
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn alpha(picture: &[u8], x: usize) -> u8 {
    picture[(H / 2 * W + x) * 4 + 3]
}

fn frame_count(ff: &Ffmpeg, file: &Path) -> (u64, String) {
    let out = Command::new(&ff.path)
        .args(["-hide_banner", "-i"])
        .arg(file)
        .args(["-f", "null", "-"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr).into_owned();
    let n = log
        .lines()
        .rev()
        .find_map(|l| {
            let rest = l.split("frame=").nth(1)?;
            rest.split_whitespace().next()?.parse().ok()
        })
        .unwrap_or(0);
    (n, log)
}

fn ffmpeg_with(encoder: &str) -> Option<Ffmpeg> {
    let ff = Ffmpeg::locate().ok()?;
    if ff.has_encoder(encoder) {
        Some(ff)
    } else {
        eprintln!("skipped: {} has no {encoder}", ff.version);
        None
    }
}

#[test]
fn a_transparent_gif_keeps_its_clear_half_and_plays_at_a_slower_rate_for_every_nth_frame() {
    let Some(ff) = ffmpeg_with("gif") else { return };
    let tmp = tempfile::tempdir().unwrap();
    let file = write(&ff, tmp.path(), "t.gif", Container::Gif, true, 6, 2);
    let picture = first_picture(&ff, &file, None);
    assert_eq!((alpha(&picture, 4), alpha(&picture, 60)), (255, 0));
    assert_eq!(&picture[(H / 2 * W + 4) * 4..][..3], [255, 0, 0]);
    let (frames, log) = frame_count(&ff, &file);
    assert_eq!(frames, 6, "{log}");
    assert!(log.contains("5 fps") || log.contains("5 tbr"), "{log}");
}

#[test]
fn a_transparent_webm_keeps_its_alpha_channel() {
    let Some(ff) = ffmpeg_with("libvpx-vp9") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let file = write(&ff, tmp.path(), "t.webm", Container::Webm, true, 5, 1);
    let picture = first_picture(&ff, &file, Some("libvpx-vp9"));
    assert!(alpha(&picture, 4) > 250, "{}", alpha(&picture, 4));
    assert!(alpha(&picture, 60) < 5, "{}", alpha(&picture, 60));
    assert!(picture[(H / 2 * W + 4) * 4] > 200, "red survives the codec");
}

#[test]
fn a_transparent_prores_mov_keeps_its_alpha_channel() {
    let Some(ff) = ffmpeg_with("prores_ks") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let file = write(&ff, tmp.path(), "t.mov", Container::Mov, true, 5, 1);
    let picture = first_picture(&ff, &file, None);
    assert!(alpha(&picture, 4) > 250 && alpha(&picture, 60) < 5);
    let (_, log) = frame_count(&ff, &file);
    assert!(log.contains("prores"), "{log}");
}

#[test]
fn a_picture_is_scaled_to_the_asked_width_while_it_is_written() {
    let Some(ff) = ffmpeg_with("gif") else { return };
    let tmp = tempfile::tempdir().unwrap();
    let settings = EncodeSettings {
        width: W as u32,
        height: H as u32,
        fps: 10,
        output: tmp.path().join("s.gif"),
        container: Container::Gif,
        scale_width: Some(32),
        ..EncodeSettings::default()
    };
    let mut writer = VideoWriter::start(&ff, &settings).unwrap();
    for _ in 0..3 {
        writer.write_frame(&frame()).unwrap();
    }
    writer.finish().unwrap();
    let (_, log) = frame_count(&ff, &settings.output);
    assert!(log.contains("32x24"), "{log}");
}

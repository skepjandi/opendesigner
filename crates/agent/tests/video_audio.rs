//! A video with a sound track exports to an MP4 that has sound, and the sound is cut at the end
//! of the video. The test needs `ffmpeg`; without one it says so and passes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use od_agent::video_export::{self, Source};
use od_core::{Actor, AudioTrack, NewNode, NodeKind, Operation, VideoProperty, Workspace};
use od_media::Ffmpeg;

/// A short tone as a WAV file: a header and a sine wave.
fn tone(path: &Path, ms: u32) {
    let rate = 8_000u32;
    let samples = (rate * ms / 1000) as usize;
    let data = (samples * 2) as u32;
    let mut bytes: Vec<u8> = Vec::with_capacity(44 + data as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for n in 0..samples {
        let value = ((n as f64 / 25.0).sin() * 8_000.0) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn a_video_with_a_sound_track_exports_to_an_mp4_with_sound() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("skipped the export: no ffmpeg on this machine");
        return;
    };
    if ffmpeg.h264_encoder().is_err() {
        eprintln!("skipped the export: this ffmpeg has no H.264 encoder");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("p.od");
    let mut ws = Workspace::create(&project, "Video").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let mut node = NewNode::new(NodeKind::Frame);
    node.name = Some("One".into());
    node.width = 100.0;
    node.height = 200.0;
    let frame = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node,
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
                name: "Teaser".into(),
                fps: 10,
                width: 100,
                height: 200,
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
                duration_ms: 1000,
                index: None,
                transition: None,
                motion: None,
            }],
        )
        .unwrap();

    // A sound in the project: two seconds of a tone, longer than the video.
    let media = project.join("media");
    std::fs::create_dir_all(&media).unwrap();
    tone(&media.join("tone.wav"), 2000);
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::SetVideo {
                id: video.clone(),
                prop: VideoProperty::Audio(Some(AudioTrack {
                    file: "media/tone.wav".into(),
                    start_ms: 0,
                    gain: 0.5,
                    fade_in_ms: 100,
                    fade_out_ms: 200,
                })),
            }],
        )
        .unwrap();
    ws.save().unwrap();

    let source = Source::of(&ws);
    let outcome = video_export::export_video(
        &source,
        &video,
        "out.mp4",
        None,
        None,
        &ffmpeg,
        &mut |_done, _total| true,
    )
    .unwrap();
    assert_eq!(outcome.audio_pieces, 1, "the sound track was not mixed in");

    let file = project.join("exports/out.mp4");
    assert!(file.exists(), "the export wrote no file");
    let info = ffmpeg.probe_file(&file).unwrap();
    assert!(info.has_audio, "the export has no sound: {file:?}");
    assert_eq!(
        info.duration_ms, 1000,
        "the sound must not lengthen the video"
    );
}

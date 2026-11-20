//! The sound-track, pause, waveform and caption tools over the MCP handler. Tests that need
//! `ffmpeg` say so and pass without one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use od_agent::Handler;
use od_core::Workspace;
use serde_json::{Value, json};

struct Fixture {
    handler: Handler,
    ws: Arc<Mutex<Workspace>>,
    session: String,
    dir: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let ws = Workspace::create(&tmp.path().join("p.od"), "Audio").unwrap();
    let dir = ws.dir().unwrap().to_path_buf();
    let ws = Arc::new(Mutex::new(ws));
    let handler = Handler::new(ws.clone());
    let out = handler.handle(
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"claude-code","version":"1"}}}),
    );
    Fixture {
        handler,
        ws,
        session: out.session_id.unwrap(),
        dir,
        _tmp: tmp,
    }
}

impl Fixture {
    fn tool(&self, name: &str, args: Value) -> (bool, String) {
        let out = self.handler.handle(
            Some(&self.session),
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name": name, "arguments": args}}),
        );
        let result = &out.body.unwrap()["result"];
        (
            result["isError"].as_bool().unwrap(),
            result["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        )
    }

    fn ok(&self, name: &str, args: Value) -> Value {
        let (err, text) = self.tool(name, args);
        assert!(!err, "{name} failed: {text}");
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    }

    fn err(&self, name: &str, args: Value) -> String {
        let (err, text) = self.tool(name, args);
        assert!(err, "{name} should have failed: {text}");
        text
    }

    /// A 4 s video of one frame; returns its id.
    fn video(&self) -> String {
        let root = self.ws.lock().unwrap().session.document().pages[0]
            .root
            .id
            .to_string();
        let frame = self.ok(
            "apply_ops",
            json!({"ops": [{"op":"create","parent":root,"node":{"kind":"frame","name":"F","width":200,"height":400,"fill":"#102030"}}]}),
        )["created"][0]
            .as_str()
            .unwrap()
            .to_string();
        let video = self.ok(
            "apply_ops",
            json!({"ops": [{"op":"create_video","name":"V","fps":10,"width":200,"height":400}]}),
        )["created"][0]
            .as_str()
            .unwrap()
            .to_string();
        self.ok(
            "apply_ops",
            json!({"ops": [{"op":"add_clip","video":video,"frame":frame,"duration_ms":4000}]}),
        );
        video
    }

    fn video_row(&self, video: &str) -> Value {
        let all = self.ok("list_videos", json!({}));
        all.as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == video)
            .unwrap()
            .clone()
    }

    fn sound(&self, name: &str) -> String {
        let made = self.ok(
            "make_sound",
            json!({"kind": "drone", "duration_ms": 3000, "name": name}),
        );
        made["file"].as_str().unwrap().to_string()
    }
}

#[test]
fn the_tools_are_listed_and_the_transcript_tools_say_speech_is_not_built_in() {
    let f = fixture();
    let out = f.handler.handle(
        Some(&f.session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let body = out.body.unwrap();
    let tools = body["result"]["tools"].as_array().unwrap();
    for name in od_agent::audio_tools::NAMES {
        assert!(tools.iter().any(|t| t["name"] == name), "{name}");
    }
    let describe = |n: &str| {
        tools.iter().find(|t| t["name"] == n).unwrap()["description"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(describe("import_captions").contains("no speech-to-text"));
    assert!(describe("captions_from_script").contains("ESTIMATE"));
    assert!(describe("add_sound_track").contains("speech MCP server"));
}

#[test]
fn sound_tracks_are_added_changed_and_removed_and_the_story_tells_them() {
    let f = fixture();
    let video = f.video();
    let music = f.sound("bed");
    let voice = f.sound("vo");

    let added = f.ok(
        "add_sound_track",
        json!({"video": video, "role": "voice", "file": voice, "start_ms": 500}),
    );
    assert_eq!(added["track"], "a1");
    assert!(added["checkpoint_before"].is_number());
    let bed = f.ok(
        "add_sound_track",
        json!({"video": video, "file": music, "gain": 0.3, "duck": {"depth": 0.8}, "fade_out_ms": 400}),
    );
    assert_eq!(bed["track"], "a2");

    let row = f.video_row(&video);
    assert_eq!(row["tracks"].as_array().unwrap().len(), 2);
    assert!((row["tracks"][1]["duck"]["depth"].as_f64().unwrap() - 0.8).abs() < 1e-6);
    let story = row["story"].as_array().unwrap();
    assert!(
        story
            .iter()
            .any(|s| s.as_str().unwrap().starts_with("voice track a1")),
        "{story:?}"
    );
    assert!(
        story
            .iter()
            .any(|s| s.as_str().unwrap().contains("ducking under the voice")),
        "{story:?}"
    );

    f.ok(
        "set_sound_track",
        json!({"video": video, "track": "a2", "gain": 0.2, "duck": null, "pan": -0.5}),
    );
    let track = &f.video_row(&video)["tracks"][1];
    assert!((track["gain"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    assert!(track.get("duck").is_none());
    assert_eq!(track["pan"].as_f64(), Some(-0.5));

    let late = f.ok(
        "set_sound_track",
        json!({"video": video, "track": "a1", "start_ms": 9000}),
    );
    assert!(
        late["warnings"][0]
            .as_str()
            .unwrap()
            .contains("after the video ends")
    );

    f.ok("remove_sound_track", json!({"video": video, "track": "a1"}));
    assert_eq!(f.video_row(&video)["tracks"].as_array().unwrap().len(), 1);
}

#[test]
fn bad_tracks_are_refused_with_a_reason_and_change_nothing() {
    let f = fixture();
    let video = f.video();
    let sound = f.sound("bed");
    assert!(
        f.err(
            "add_sound_track",
            json!({"video": video, "file": "media/missing.wav"})
        )
        .contains("missing.wav")
    );
    assert!(
        f.err(
            "add_sound_track",
            json!({"video": video, "file": "../x.wav"})
        )
        .contains("audio")
    );
    assert!(
        f.err(
            "add_sound_track",
            json!({"video": video, "file": sound, "gain": 5})
        )
        .contains("gain")
    );
    assert!(
        f.err(
            "add_sound_track",
            json!({"video": video, "file": sound, "role": "voice", "duck": {}})
        )
        .contains("duck")
    );
    assert!(
        f.err(
            "add_sound_track",
            json!({"video": video, "file": sound, "loud": true})
        )
        .contains("invalid arguments")
    );
    assert!(
        f.err(
            "set_sound_track",
            json!({"video": video, "track": "a9", "gain": 1})
        )
        .contains("no sound track")
    );
    assert!(
        f.err(
            "set_sound_track",
            json!({"video": video, "track": "audio", "gain": 1})
        )
        .contains("set_video audio")
    );
    assert!(f.video_row(&video)["tracks"].as_array().unwrap().is_empty());
}

#[test]
fn captions_come_from_srt_vtt_words_or_a_script_and_every_change_undoes() {
    let f = fixture();
    let video = f.video();
    let srt = "1\n00:00:00,500 --> 00:00:01,500\nHello there\n\n2\n00:00:02,000 --> 00:00:09,000\nThe end\n";
    let r = f.ok(
        "import_captions",
        json!({"video": video, "format": "srt", "text": srt}),
    );
    assert_eq!(r["cues"], 2);
    let row = f.video_row(&video);
    assert_eq!(row["captions"]["cues"][0]["text"], "Hello there");
    assert_eq!(
        row["captions"]["cues"][1]["end_ms"], 4000,
        "cut at the end of the video"
    );
    assert!(
        row["story"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().starts_with("captions: 2 cues"))
    );

    let shifted = f.ok("import_captions", json!({"video": video, "format": "vtt", "text": "WEBVTT\n\n00:01.000 --> 00:02.000\nlate\n\n00:09.000 --> 00:10.000\ngone\n", "offset_ms": 500}));
    assert_eq!(
        (
            shifted["cues"].clone(),
            shifted["dropped_outside_the_video"].clone()
        ),
        (json!(1), json!(1))
    );

    let words =
        json!([{"text":"One","start_ms":0,"end_ms":300},{"word":"two","start":0.3,"end":0.7}]);
    let r = f.ok(
        "import_captions",
        json!({"video": video, "format": "words_json", "text": words.to_string()}),
    );
    assert_eq!(r["with_word_timings"], 1);

    let r = f.ok("captions_from_script", json!({"video": video, "script": "Meet the new way to design. Fast, local and yours.", "words_per_cue": 5}));
    assert!(r["estimate"].as_str().unwrap().contains("not recognized"));
    let cues = f.video_row(&video)["captions"]["cues"]
        .as_array()
        .unwrap()
        .len();
    assert!(cues >= 2);

    // one manual cue at a time, in time order
    f.ok("clear_captions", json!({"video": video}));
    assert!(
        f.video_row(&video)["captions"]["cues"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let late = f.ok(
        "add_caption_cue",
        json!({"video": video, "start_ms": 2000, "end_ms": 3000, "text": "late"}),
    );
    let early = f.ok(
        "add_caption_cue",
        json!({"video": video, "start_ms": 0, "end_ms": 1000, "text": "so early",
               "words": [{"text": "so", "start_ms": 0, "end_ms": 400}, {"text": "early", "start_ms": 400, "end_ms": 1000}]}),
    );
    let texts: Vec<Value> = f.video_row(&video)["captions"]["cues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["text"].clone())
        .collect();
    assert_eq!(texts, [json!("so early"), json!("late")]);
    assert!(
        f.err(
            "add_caption_cue",
            json!({"video": video, "start_ms": 500, "end_ms": 2500, "text": "overlap"})
        )
        .contains("overlap")
    );
    assert!(
        f.err(
            "add_caption_cue",
            json!({"video": video, "start_ms": 3500, "end_ms": 5000, "text": "after"})
        )
        .contains("after the video")
    );
    assert!(
        f.err(
            "add_caption_cue",
            json!({"video": video, "start_ms": 100, "end_ms": 100, "text": "x"})
        )
        .contains("caption")
    );
    assert!(
        f.err(
            "add_caption_cue",
            json!({"video": video, "start_ms": 1000, "end_ms": 1500, "text": "x".repeat(401)})
        )
        .contains("caption text")
    );
    f.ok(
        "remove_caption_cue",
        json!({"video": video, "cue": late["cue"]}),
    );
    assert!(
        f.err(
            "remove_caption_cue",
            json!({"video": video, "cue": early["cue"].as_str().unwrap().to_string() + "9"})
        )
        .contains("no caption cue")
    );
    assert_eq!(
        f.video_row(&video)["captions"]["cues"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn the_style_is_a_preset_with_overrides_and_bad_values_are_refused() {
    let f = fixture();
    let video = f.video();
    f.ok(
        "set_caption_style",
        json!({"video": video, "preset": "bold_pop", "color": "#ff00ff"}),
    );
    let style = f.video_row(&video)["captions"]["style"].clone();
    assert_eq!(
        (style["uppercase"].clone(), style["color"].clone()),
        (json!(true), json!("#ff00ff"))
    );
    assert_eq!(style["highlight"], "#ffd400");
    f.ok(
        "set_caption_style",
        json!({"video": video, "highlight": null}),
    );
    assert!(
        f.video_row(&video)["captions"]["style"]
            .get("highlight")
            .is_none()
    );
    for bad in [
        json!({"size_pct": 50}),
        json!({"color": "red"}),
        json!({"max_lines": 9}),
        json!({"preset": "neon"}),
        json!({"font": "x"}),
    ] {
        let mut args = bad;
        args["video"] = json!(video);
        f.err("set_caption_style", args);
    }
    assert_eq!(f.video_row(&video)["captions"]["style"]["color"], "#ff00ff");
}

#[test]
fn a_transcript_that_is_too_large_or_malformed_changes_nothing() {
    let f = fixture();
    let video = f.video();
    assert!(f.err("import_captions", json!({"video": video, "format": "srt", "text": "1\n00:00:03,000 --> 00:00:01,000\nx\n"})).contains("subtitles"));
    assert!(
        f.err(
            "import_captions",
            json!({"video": video, "format": "srt", "text": "x".repeat(2_000_001)})
        )
        .contains("longer")
    );
    assert!(
        f.err(
            "import_captions",
            json!({"video": video, "format": "words_json", "text": "{}"})
        )
        .contains("word timings")
    );
    assert!(
        f.err(
            "import_captions",
            json!({"video": video, "format": "ass", "text": "x"})
        )
        .contains("srt, vtt or words_json")
    );
    assert!(
        f.err(
            "captions_from_script",
            json!({"video": video, "script": "   "})
        )
        .contains("no words")
    );
    assert!(
        f.video_row(&video)["captions"]["cues"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

// ---- needs ffmpeg ----

fn ffmpeg() -> Option<od_media::Ffmpeg> {
    od_media::Ffmpeg::locate().ok()
}

fn make(ff: &od_media::Ffmpeg, args: &[&str], out: &Path) {
    let status = Command::new(&ff.path)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .arg(out)
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg {args:?}");
}

/// Footage `talk.mp4`: 1 s of tone, 1 s of silence, 1 s of tone, with a picture.
fn talk(ff: &od_media::Ffmpeg, dir: &Path) {
    std::fs::create_dir_all(dir.join("media")).unwrap();
    make(
        ff,
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=gray:s=64x64:d=3:r=10",
            "-f",
            "lavfi",
            "-i",
            "sine=f=330:d=1:r=48000",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=mono:d=1",
            "-f",
            "lavfi",
            "-i",
            "sine=f=330:d=1:r=48000",
            "-filter_complex",
            "[1:a][2:a][3:a]concat=n=3:v=0:a=1[a]",
            "-map",
            "0:v",
            "-map",
            "[a]",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
            "-shortest",
        ],
        &dir.join("media/talk.mp4"),
    );
}

#[test]
fn pauses_are_reported_waveforms_drawn_and_a_clip_is_cut_at_its_pause_in_one_step() {
    let Some(ff) = ffmpeg() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    let f = fixture();
    talk(&ff, &f.dir);

    let found = f.ok(
        "detect_pauses",
        json!({"file": "media/talk.mp4", "min_ms": 300}),
    );
    let pauses = found["pauses"].as_array().unwrap();
    assert_eq!(pauses.len(), 1, "{found}");
    assert!((900..=1100).contains(&pauses[0]["start_ms"].as_u64().unwrap()));
    assert_eq!(found["sound"].as_array().unwrap().len(), 2);

    let wave = f.ok(
        "get_waveform",
        json!({"file": "media/talk.mp4", "buckets": 90}),
    );
    let peaks = wave["peaks"].as_array().unwrap();
    assert_eq!(peaks.len(), 90);
    assert!(peaks[15].as_f64().unwrap() > 0.05 && peaks[45].as_f64().unwrap() < 0.01);
    assert!(
        f.err(
            "get_waveform",
            json!({"file": "media/talk.mp4", "buckets": 5})
        )
        .contains("buckets")
    );
    assert!(
        f.err("get_waveform", json!({"file": "media/nope.wav"}))
            .contains("nope.wav")
    );

    let root = f.ws.lock().unwrap().session.document().pages[0]
        .root
        .id
        .to_string();
    let _ = root;
    let video = f.ok(
        "apply_ops",
        json!({"ops": [{"op":"create_video","name":"T","fps":10,"width":64,"height":64}]}),
    )["created"][0]
        .as_str()
        .unwrap()
        .to_string();
    f.ok(
        "apply_ops",
        json!({"ops": [{"op":"add_clip","video":video,"footage":{"file":"media/talk.mp4","in_ms":0},"duration_ms":3000}]}),
    );
    let clip = f.video_row(&video)["clips"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let cut = f.ok(
        "remove_pauses",
        json!({"video": video, "clip": clip, "min_ms": 300, "keep_ms": 100}),
    );
    assert_eq!(cut["clips"], 2);
    let row = f.video_row(&video);
    let duration = row["duration_ms"].as_u64().unwrap();
    assert!((1900..=2600).contains(&duration), "{duration}");
    assert_eq!(row["clips"].as_array().unwrap().len(), 2);

    f.ok(
        "restore_checkpoint",
        json!({"checkpoint": cut["checkpoint_before"]}),
    );
    assert_eq!(f.video_row(&video)["clips"].as_array().unwrap().len(), 1);
    assert!(
        f.err("remove_pauses", json!({"video": video, "clip": "c_999"}))
            .contains("does not exist")
    );
}

#[test]
fn a_video_with_voice_music_and_captions_exports_with_both_tracks_mixed() {
    let Some(ff) = ffmpeg() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    if ff.h264_encoder().is_err() {
        eprintln!("skipped: this ffmpeg has no H.264 encoder");
        return;
    }
    let f = fixture();
    let video = f.video();
    let (music, voice) = (f.sound("bed"), f.sound("vo"));
    f.ok(
        "add_sound_track",
        json!({"video": video, "role": "voice", "file": voice, "start_ms": 500, "out_ms": 2000}),
    );
    f.ok(
        "add_sound_track",
        json!({"video": video, "file": music, "gain": 0.4, "duck": {"depth": 0.8}}),
    );
    f.ok(
        "captions_from_script",
        json!({"video": video, "script": "Hello from the export", "start_ms": 500, "end_ms": 3000}),
    );
    f.ok(
        "set_caption_style",
        json!({"video": video, "preset": "karaoke"}),
    );

    let out = f.ok("export_video", json!({"video": video, "file": "mixed.mp4"}));
    assert_eq!(out["footage_sounds_mixed"], 2, "{out}");
    let info = ff.probe_file(&f.dir.join("exports/mixed.mp4")).unwrap();
    assert!(info.has_audio);
    assert_eq!(info.duration_ms, 4000);
}

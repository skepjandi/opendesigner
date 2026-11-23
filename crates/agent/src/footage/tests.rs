#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Stdio};

use od_core::{Actor, Workspace};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::*;
use crate::tools::Turns;

struct Fixture {
    _dir: TempDir,
    ws: Workspace,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::create(&dir.path().join("p.od"), "Footage").unwrap();
    Fixture { _dir: dir, ws }
}

fn run(ws: &mut Workspace, name: &str, args: Value) -> Result<Output, String> {
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws,
        actor: Actor::agent("tester").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    crate::tools::call(&mut ctx, name, args).map_err(|e| e.0)
}

fn call(ws: &mut Workspace, name: &str, args: Value) -> Result<Value, String> {
    run(ws, name, args).map(|out| match out.content.first() {
        Some(Content::Text(t)) => serde_json::from_str(t).unwrap(),
        _ => panic!("no text"),
    })
}

impl Fixture {
    fn dir(&self) -> PathBuf {
        self.ws.dir().unwrap().to_path_buf()
    }

    /// An index written the way the tool writes it, for a footage file of `bytes` bytes.
    fn write_index(&self, file: &str, bytes: u64, shots: Vec<ShotEntry>, words: Vec<WordEntry>) {
        let index = Index {
            version: INDEX_VERSION,
            file: file.to_string(),
            bytes,
            duration_ms: shots.last().map_or(0, |s| s.end_ms),
            threshold: DEFAULT_THRESHOLD,
            min_shot_ms: DEFAULT_MIN_SHOT_MS,
            shots,
            language: None,
            words,
        };
        store(&self.dir(), &index).unwrap();
    }

    fn footage_file(&self, name: &str, content: &[u8]) {
        std::fs::create_dir_all(self.dir().join("media")).unwrap();
        std::fs::write(self.dir().join("media").join(name), content).unwrap();
    }
}

fn shot(start_ms: u64, end_ms: u64, note: Option<&str>, tags: &[&str]) -> ShotEntry {
    ShotEntry {
        start_ms,
        end_ms,
        note: note.map(str::to_string),
        tags: tags.iter().map(ToString::to_string).collect(),
    }
}

fn word(text: &str, start_ms: u64) -> WordEntry {
    WordEntry {
        text: text.into(),
        start_ms,
        end_ms: start_ms + 300,
    }
}

#[test]
fn pictures_are_taken_from_the_middle_of_equal_slices() {
    assert_eq!(even_times(0, 1000, 4), [125, 375, 625, 875]);
    assert_eq!(even_times(2000, 3000, 1), [2500]);
    assert!(
        even_times(0, 10_000, 10)
            .iter()
            .all(|t| *t > 0 && *t < 10_000)
    );
}

#[test]
fn a_note_beats_a_spoken_word_and_a_phrase_beats_scattered_words() {
    let index = Index {
        version: INDEX_VERSION,
        file: "media/a.mp4".into(),
        bytes: 1,
        duration_ms: 9000,
        threshold: 0.3,
        min_shot_ms: 500,
        shots: vec![
            shot(0, 3000, Some("Founder laughing at the desk"), &["office"]),
            shot(
                3000,
                6000,
                Some("A laptop on a desk, a founder walks away"),
                &[],
            ),
            shot(6000, 9000, None, &[]),
        ],
        language: None,
        words: vec![word("founder", 6100), word("laughing", 6400)],
    };
    let query = tokens("founder laughing");
    let phrase = query.join(" ");
    let scores: Vec<u32> = index
        .shots
        .iter()
        .map(|s| score(&query, &phrase, &index, s).0)
        .collect();
    assert_eq!(scores, [8, 3, 4]);
    let (none, matched) = score(&tokens("submarine"), "submarine", &index, &index.shots[0]);
    assert_eq!((none, matched.len()), (0, 0));
}

#[test]
fn notes_move_to_the_shot_that_now_starts_where_the_old_one_did() {
    let old = [
        shot(0, 2000, Some("intro"), &["a"]),
        shot(2000, 5000, Some("middle"), &[]),
        shot(5000, 6000, Some("gone"), &[]),
    ];
    let mut new = [shot(0, 2100, None, &[]), shot(2100, 4000, None, &[])];
    assert_eq!(carry_notes(&old, &mut new), (2, 1));
    assert_eq!(new[0].note.as_deref(), Some("intro"));
    assert_eq!(new[0].tags, ["a"]);
    assert_eq!(new[1].note.as_deref(), Some("middle"));
}

#[test]
fn search_finds_moments_by_note_tag_and_transcript_and_offers_a_clip() {
    let mut f = fixture();
    f.footage_file("interview.mp4", b"0123456789");
    f.write_index(
        "media/interview.mp4",
        10,
        vec![
            shot(0, 4000, Some("Host welcomes the guest"), &["Studio"]),
            shot(
                4000,
                20_000,
                Some("Close-up of the guest explaining pricing"),
                &[],
            ),
        ],
        vec![word("budget", 4500), word("pricing", 4900)],
    );

    let by_note = call(
        &mut f.ws,
        "search_footage",
        json!({"query": "guest pricing"}),
    )
    .unwrap();
    assert_eq!(by_note["matches"], 2);
    let top = &by_note["results"][0];
    assert_eq!(top["shot"], 1);
    assert_eq!(top["start_ms"], 4000);
    assert_eq!(
        top["clip"],
        json!({"footage": {"file": "media/interview.mp4", "in_ms": 4000}, "duration_ms": 10_000})
    );
    assert_eq!(top["said"], "budget pricing");
    assert_eq!(top["stale"], false);

    let by_tag = call(&mut f.ws, "search_footage", json!({"query": "studio"})).unwrap();
    assert_eq!(by_tag["results"][0]["shot"], 0);

    let spoken = call(&mut f.ws, "search_footage", json!({"query": "budget"})).unwrap();
    assert_eq!(spoken["results"][0]["shot"], 1);

    let none = call(&mut f.ws, "search_footage", json!({"query": "submarine"})).unwrap();
    assert_eq!(none["matches"], 0);
    assert!(none["hint"].as_str().unwrap().contains("watch_footage"));

    let overview = call(&mut f.ws, "search_footage", json!({})).unwrap();
    assert_eq!(overview["indexed"][0]["with_notes"], 2);
    assert_eq!(overview["indexed"][0]["transcript_words"], 2);
}

#[test]
fn a_file_that_changed_since_it_was_indexed_is_marked_stale() {
    let mut f = fixture();
    f.footage_file("a.mp4", b"0123456789");
    f.write_index(
        "media/a.mp4",
        4,
        vec![shot(0, 1000, Some("a dog"), &[])],
        vec![],
    );
    let found = call(&mut f.ws, "search_footage", json!({"query": "dog"})).unwrap();
    assert_eq!(found["results"][0]["stale"], true);
    assert!(found["warning"].as_str().unwrap().contains("index_footage"));
}

#[test]
fn notes_are_checked_before_anything_is_written() {
    let mut f = fixture();
    f.footage_file("a.mp4", b"0123456789");
    f.write_index("media/a.mp4", 10, vec![shot(0, 1000, None, &[])], vec![]);
    let before = std::fs::read(index_path(&f.dir(), "media/a.mp4")).unwrap();
    // an unchanged file with the stored threshold needs no ffmpeg: only the notes change
    let err = call(
        &mut f.ws,
        "index_footage",
        json!({"file": "media/a.mp4", "notes": [
            {"shot": 0, "note": "fine"}, {"shot": 3, "note": "no such shot"}]}),
    )
    .unwrap_err();
    assert!(err.contains("shot 3 does not exist"), "{err}");
    assert_eq!(
        std::fs::read(index_path(&f.dir(), "media/a.mp4")).unwrap(),
        before
    );

    let long = "x".repeat(MAX_NOTE_CHARS + 1);
    let err = call(
        &mut f.ws,
        "index_footage",
        json!({"file": "media/a.mp4", "notes": [{"shot": 0, "note": long}]}),
    )
    .unwrap_err();
    assert!(err.contains("at most"), "{err}");

    let ok = call(
        &mut f.ws,
        "index_footage",
        json!({"file": "media/a.mp4", "notes": [
            {"shot": 0, "note": "  A  dog\nruns ", "tags": ["Dog", "dog", "Park"]}]}),
    )
    .unwrap();
    assert_eq!(ok["scanned"], false);
    assert_eq!(ok["shots"][0]["note"], "A dog runs");
    assert_eq!(ok["shots"][0]["tags"], json!(["dog", "park"]));
    assert_eq!(ok["unnoted_shots"], json!([]));
}

#[test]
fn only_plain_footage_files_are_read_and_the_index_is_never_written_through_a_link() {
    let mut f = fixture();
    for bad in ["../x.mp4", "media/../x.mp4", "media/a.txt", "a.mp4"] {
        let err = call(&mut f.ws, "index_footage", json!({"file": bad})).unwrap_err();
        assert!(!err.is_empty(), "{bad}");
        assert!(
            call(
                &mut f.ws,
                "search_footage",
                json!({"query": "x", "file": bad})
            )
            .is_err()
        );
    }
    let err = call(
        &mut f.ws,
        "search_footage",
        json!({"query": "x", "file": "media/none.mp4"}),
    )
    .unwrap_err();
    assert!(err.contains("none.mp4"), "{err}");

    #[cfg(unix)]
    {
        f.footage_file("a.mp4", b"0123456789");
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), f.dir().join(INDEX_DIR)).unwrap();
        let err = call(
            &mut f.ws,
            "index_footage",
            json!({"file": "media/a.mp4", "notes": []}),
        )
        .unwrap_err();
        assert!(err.contains("link"), "{err}");
        assert!(
            std::fs::read_dir(elsewhere.path())
                .unwrap()
                .next()
                .is_none()
        );
    }
}

#[test]
fn a_name_with_a_space_is_kept_under_a_plain_file_name() {
    assert_eq!(
        index_path(Path::new("/p.od"), "media/My clip.mp4"),
        Path::new("/p.od/footage-index/My_clip.mp4.json")
    );
}

// ---- needs ffmpeg ----

fn ffmpeg_here() -> Option<Ffmpeg> {
    Ffmpeg::locate().ok()
}

/// `shots.mp4`: two seconds of red, then two seconds of blue, 10 frames a second.
fn two_shots(ff: &Ffmpeg, dir: &Path) {
    std::fs::create_dir_all(dir.join("media")).unwrap();
    let status = Command::new(&ff.path)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i", "color=c=red:s=96x54:d=2:r=10"])
        .args(["-f", "lavfi", "-i", "color=c=blue:s=96x54:d=2:r=10"])
        .args([
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0[v]",
            "-map",
            "[v]",
        ])
        .args(["-c:v", "mpeg4"])
        .arg(dir.join("media/shots.mp4"))
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn footage_is_cut_into_shots_noted_searched_and_watched() {
    let Some(ff) = ffmpeg_here() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    let mut f = fixture();
    two_shots(&ff, &f.dir());

    let first = call(
        &mut f.ws,
        "index_footage",
        json!({"file": "media/shots.mp4", "notes": [{"shot": 0, "note": "A red card"}]}),
    )
    .unwrap();
    assert_eq!(first["scanned"], true);
    assert_eq!(first["shot_count"], 2, "{first}");
    let cut = first["shots"][1]["start_ms"].as_u64().unwrap();
    assert!(cut.abs_diff(2000) <= 100, "cut at {cut}");
    assert_eq!(first["unnoted_shots"], json!([1]));
    assert!(first["next"].as_str().unwrap().contains("watch_footage"));

    let again = call(
        &mut f.ws,
        "index_footage",
        json!({"file": "media/shots.mp4", "notes": [{"shot": 1, "note": "A blue card"}]}),
    )
    .unwrap();
    assert_eq!(again["scanned"], false);
    assert_eq!(again["shots"][0]["note"], "A red card");

    let rescanned = call(
        &mut f.ws,
        "index_footage",
        json!({"file": "media/shots.mp4", "threshold": 0.2}),
    )
    .unwrap();
    assert_eq!(rescanned["scanned"], true);
    assert_eq!(rescanned["notes_carried"], 2);
    assert_eq!(rescanned["shots"][1]["note"], "A blue card");

    let found = call(&mut f.ws, "search_footage", json!({"query": "blue"})).unwrap();
    assert_eq!(found["results"][0]["clip"]["footage"]["in_ms"], cut);
    assert_eq!(found["results"][0]["stale"], false);

    let out = run(
        &mut f.ws,
        "watch_footage",
        json!({"file": "media/shots.mp4", "frames": 4, "question": "which colours show?"}),
    )
    .unwrap();
    assert!(matches!(out.content[0], Content::Image { .. }));
    let Content::Text(note) = &out.content[1] else {
        panic!("no note")
    };
    assert!(
        note.contains("4 pictures") && note.contains("which colours show?"),
        "{note}"
    );
    let Content::Image { data, .. } = &out.content[0] else {
        panic!("no image")
    };
    let png = STANDARD.decode(data).unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(&png))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    let rgb: Vec<[u8; 3]> = pixels[..info.buffer_size()]
        .chunks_exact(3)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    assert!(
        rgb.iter().any(|p| p[0] > 200 && p[1] < 60 && p[2] < 60),
        "no red in the sheet"
    );
    assert!(
        rgb.iter().any(|p| p[2] > 200 && p[0] < 60 && p[1] < 60),
        "no blue in the sheet"
    );

    let err = call(
        &mut f.ws,
        "watch_footage",
        json!({"file": "media/shots.mp4", "frames": 11}),
    )
    .unwrap_err();
    assert!(err.contains("1 to 10"), "{err}");
    let err = call(
        &mut f.ws,
        "watch_footage",
        json!({"file": "media/shots.mp4", "from_ms": 3000, "to_ms": 3000}),
    )
    .unwrap_err();
    assert!(err.contains("empty"), "{err}");
}

//! RV013 to RV019 and the overlap pass, each on a constructed document that has the fault and
//! one that does not.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use od_core::{Actor, Operation, Session, SoundTrack};

struct Project {
    session: Session,
    root: NodeId,
    video: NodeId,
    size: (u32, u32),
}

fn apply(s: &mut Session, ops: Value) -> od_core::TxReceipt {
    let ops: Vec<Operation> = serde_json::from_value(ops).unwrap();
    s.apply(Actor::User, None, &ops).unwrap()
}

fn project(width: u32, height: u32) -> Project {
    let mut session = Session::new(Document::new("more"));
    let page = session.add_page("p").unwrap();
    let root = session.document().page(&page).unwrap().root.id.clone();
    apply(
        &mut session,
        json!([{"op": "create_video", "name": "V", "fps": 30, "width": width, "height": height}]),
    );
    let video = session.document().videos[0].id.clone();
    Project {
        session,
        root,
        video,
        size: (width, height),
    }
}

impl Project {
    fn frame(&mut self) -> NodeId {
        let (w, h) = self.size;
        apply(
            &mut self.session,
            json!([{"op": "create", "parent": self.root.as_str(), "node":
                {"kind": "frame", "name": "scene", "width": w, "height": h, "fill": "#101830"}}]),
        )
        .created[0]
            .clone()
    }

    fn shape(
        &mut self,
        parent: &NodeId,
        kind: &str,
        name: &str,
        at: (f64, f64, f64, f64),
    ) -> NodeId {
        apply(
            &mut self.session,
            json!([{"op": "create", "parent": parent.as_str(), "node":
                {"kind": kind, "name": name, "x": at.0, "y": at.1, "width": at.2, "height": at.3,
                 "fill": "#ff7a45"}}]),
        )
        .created[0]
            .clone()
    }

    fn text(&mut self, parent: &NodeId, text: &str, y: f64) -> NodeId {
        apply(
            &mut self.session,
            json!([{"op": "create", "parent": parent.as_str(), "node":
                {"kind": "text", "name": "Words", "x": 40, "y": y, "width": 300, "height": 60,
                 "text": text, "font_size": 30, "fill": "#ffffff"}}]),
        )
        .created[0]
            .clone()
    }

    fn clip(&mut self, frame: &NodeId, ms: u32) {
        apply(
            &mut self.session,
            json!([{"op": "add_clip", "video": self.video.as_str(), "frame": frame.as_str(), "duration_ms": ms}]),
        );
    }

    fn animate(&mut self, node: &NodeId, prop: &str, keys: Value, repeat: Option<&str>) {
        let mut op = json!({"op": "animate", "video": self.video.as_str(), "node": node.as_str(),
            "prop": prop, "keys": keys});
        if let Some(r) = repeat {
            op["repeat"] = json!(r);
        }
        apply(&mut self.session, json!([op]));
    }

    fn source(&self) -> Source {
        Source::of_document(self.session.document().clone())
    }

    fn found(&self) -> Report {
        review(&self.source(), None, None).unwrap()
    }
}

fn codes(r: &Report) -> Vec<&'static str> {
    r.findings.iter().map(|f| f.code).collect()
}

/// A full-frame white card whose opacity goes `0, 1, 0, 1 ...` every `half_ms`.
fn strobe(half_ms: u64, flips: u64) -> Project {
    let mut p = project(400, 300);
    let f = p.frame();
    let card = p.shape(&f, "rect", "Card", (0.0, 0.0, 400.0, 300.0));
    p.clip(&f, 2_000);
    let keys: Vec<Value> = (0..=flips)
        .map(|i| json!({"at_ms": i * half_ms, "value": (i % 2) as f64, "ease": "linear"}))
        .collect();
    p.animate(&card, "opacity", Value::Array(keys), None);
    p
}

#[test]
fn a_strobing_video_is_an_error_and_a_calm_one_is_not() {
    let report = strobe(100, 15).found();
    assert!(codes(&report).contains(&"RV013"), "{:?}", codes(&report));
    let finding = report.findings.iter().find(|f| f.code == "RV013").unwrap();
    assert_eq!(finding.severity, "error");
    assert!(finding.message.contains("seizures"), "{}", finding.message);
    assert!(report.score <= ERROR_SCORE_CAP, "{}", report.score);
    assert!(
        report.summary().contains("not ready"),
        "{}",
        report.summary()
    );
    let listed = report.to_json();
    let json_finding = listed["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "RV013")
        .unwrap();
    assert_eq!(json_finding["severity"], json!("error"));
    assert!(
        report.metrics["flash_changes_per_second_max"]
            .as_u64()
            .unwrap()
            > 3
    );

    // one flash per second and slow fades are fine
    for calm in [strobe(500, 3), strobe(1_000, 1)] {
        let report = calm.found();
        assert!(!codes(&report).contains(&"RV013"), "{:?}", codes(&report));
        assert!(
            report.metrics["flash_changes_per_second_max"]
                .as_u64()
                .unwrap()
                <= 3
        );
    }
}

#[test]
fn flash_changes_are_runs_not_single_steps() {
    // a fade over 10 pictures is one change, a cut to white and back is two
    let fade: Vec<(u64, f64)> = (0..=10).map(|i| (i * 66, i as f64 / 10.0)).collect();
    assert_eq!(flash::large_changes(&fade), vec![660]);
    let cut = [(0, 0.0), (66, 0.0), (132, 1.0), (198, 1.0), (264, 0.0)];
    assert_eq!(flash::large_changes(&cut), vec![132, 264]);
    // wiggles under the threshold, and changes among bright pictures, do not count
    let small = [(0, 0.5), (66, 0.55), (132, 0.5), (198, 0.55)];
    assert!(flash::large_changes(&small).is_empty());
    let bright = [(0, 0.85), (66, 0.99), (132, 0.85)];
    assert!(flash::large_changes(&bright).is_empty());
    let scan = flash::FlashScan {
        changes: vec![0, 200, 400, 600, 5_000],
        scanned_ms: 6_000,
    };
    assert_eq!(scan.worst_window(), (4, 0));
    assert!(flash::draft(&scan).is_some());
    let three = flash::FlashScan {
        changes: vec![0, 200, 400],
        scanned_ms: 6_000,
    };
    assert!(flash::draft(&three).is_none());
}

fn cuts_of(durations: &[u32]) -> Project {
    let mut p = project(400, 300);
    for _ in durations {
        let f = p.frame();
        p.shape(&f, "rect", "a", (10.0, 10.0, 20.0, 20.0));
    }
    let frames: Vec<NodeId> = p.session.document().pages[0]
        .root
        .children
        .iter()
        .map(|n| n.id.clone())
        .collect();
    for (f, ms) in frames.iter().zip(durations) {
        p.clip(f, *ms);
    }
    p
}

#[test]
fn cuts_off_the_beat_need_a_tempo_and_a_miss() {
    let off = cuts_of(&[1_100, 1_300, 1_000]);
    let on = cuts_of(&[2_000, 1_500, 2_500]);
    let at = |p: &Project, bpm| review_at_tempo(&p.source(), None, None, bpm).unwrap();
    let found = at(&off, Some(120.0));
    assert!(codes(&found).contains(&"RV014"), "{:?}", codes(&found));
    assert!(
        found
            .findings
            .iter()
            .any(|f| f.message.contains("2 of 2 cuts"))
    );
    assert!(!codes(&at(&on, Some(120.0))).contains(&"RV014"));
    // no tempo, no beat check
    assert!(!codes(&at(&off, None)).contains(&"RV014"));
    // the grid starts with the music: a bed that starts 100 ms late puts both cuts on it
    let late = cuts_of(&[1_100, 1_500, 1_000]);
    let mut src = late.source();
    src.doc.videos[0].tracks = vec![SoundTrack {
        id: NodeId::parse("t_bed").unwrap(),
        role: od_core::TrackRole::Music,
        file: "media/bed.wav".into(),
        start_ms: 100,
        in_ms: 0,
        out_ms: None,
        gain: 0.3,
        fade_in_ms: 0,
        fade_out_ms: 0,
        pan: 0.0,
        duck: None,
    }];
    let report = review_at_tempo(&src, None, None, Some(120.0)).unwrap();
    assert!(!codes(&report).contains(&"RV014"), "{:?}", codes(&report));
    for bad in [0.0, 29.0, 301.0, f64::NAN] {
        assert!(
            review_at_tempo(&on.source(), None, None, Some(bad)).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn text_that_leaves_before_it_can_be_read_is_found() {
    let sentence = "Hello brave new world today";
    let build = |ms: u32, late: bool| {
        let mut p = project(400, 300);
        let f = p.frame();
        let t = p.text(&f, sentence, 100.0);
        p.clip(&f, ms);
        if late {
            let keys = json!([{"at_ms": 0, "value": 0.0, "ease": "linear"},
                {"at_ms": u64::from(ms) - 500, "value": 0.0, "ease": "linear"},
                {"at_ms": u64::from(ms) - 400, "value": 1.0, "ease": "linear"}]);
            p.animate(&t, "opacity", keys, None);
        }
        p.found()
    };
    // five short words need 1 s + 5 x 0.4 s
    let brief = build(2_500, false);
    assert!(codes(&brief).contains(&"RV015"), "{:?}", codes(&brief));
    assert!(
        brief
            .findings
            .iter()
            .any(|f| f.message.contains("needs 3.0 s"))
    );
    assert!(codes(&build(6_000, true)).contains(&"RV015"));
    assert!(!codes(&build(3_200, false)).contains(&"RV015"));
    assert_eq!(checks::reading_ms("Black holes"), 1_800);
    assert_eq!(checks::reading_ms("extraordinarily"), 1_000 + 3 * 400);
}

#[test]
fn text_under_a_phones_interface_is_found_in_vertical_video_only() {
    let build = |size: (u32, u32), y: f64| {
        let mut p = project(size.0, size.1);
        let f = p.frame();
        p.text(&f, "Subscribe", y);
        p.clip(&f, 5_000);
        p.found()
    };
    // 1080 x 1920: the top 150 px and the bottom 250 px
    let top = build((1080, 1920), 40.0);
    assert!(codes(&top).contains(&"RV016"), "{:?}", codes(&top));
    assert!(codes(&build((1080, 1920), 1_800.0)).contains(&"RV016"));
    assert!(!codes(&build((1080, 1920), 900.0)).contains(&"RV016"));
    assert!(!codes(&build((1920, 1080), 20.0)).contains(&"RV016"));
    // the zones scale with the height
    assert!(codes(&build((540, 960), 20.0)).contains(&"RV016"));
    assert!(!codes(&build((540, 960), 100.0)).contains(&"RV016"));
}

fn busy(layers: usize) -> Project {
    let mut p = project(400, 300);
    let f = p.frame();
    for i in 0..layers {
        let n = p.shape(
            &f,
            "ellipse",
            &format!("blob {i}"),
            (20.0 * i as f64, 40.0, 30.0, 30.0),
        );
        let span = 1_000 + 300 * i as u64;
        let keys = json!([{"at_ms": 0, "value": 40.0}, {"at_ms": span / 2, "value": 90.0}, {"at_ms": span, "value": 40.0}]);
        p.animate(&n, "y", keys, Some("loop"));
    }
    p.clip(&f, 6_000);
    p
}

#[test]
fn two_seconds_with_fewer_than_three_moving_layers_are_found() {
    let thin = busy(2).found();
    assert!(codes(&thin).contains(&"RV017"), "{:?}", codes(&thin));
    assert_eq!(
        thin.findings
            .iter()
            .find(|f| f.code == "RV017")
            .unwrap()
            .scenes,
        vec![0]
    );
    assert!(!codes(&busy(4).found()).contains(&"RV017"));
    // the camera is a layer
    let mut with_camera = busy(2);
    let video = with_camera.video.clone();
    with_camera.animate(
        &video,
        "camera_zoom",
        json!([{"at_ms": 0, "value": 1.0}, {"at_ms": 6000, "value": 1.2}]),
        None,
    );
    assert!(!codes(&with_camera.found()).contains(&"RV017"));
}

#[test]
fn the_same_scene_twice_is_found_and_two_different_scenes_are_not() {
    let mut twice = project(400, 300);
    let f = twice.frame();
    for i in 0..3 {
        twice.shape(&f, "rect", "r", (10.0 + 50.0 * i as f64, 10.0, 40.0, 40.0));
    }
    twice.clip(&f, 1_500);
    twice.clip(&f, 1_500);
    let found = twice.found();
    assert!(codes(&found).contains(&"RV018"), "{:?}", codes(&found));
    assert_eq!(
        found
            .findings
            .iter()
            .find(|f| f.code == "RV018")
            .unwrap()
            .scenes,
        vec![0, 1]
    );

    let mut different = project(400, 300);
    for shift in [0.0, 25.0] {
        let f = different.frame();
        for i in 0..3 {
            different.shape(
                &f,
                "rect",
                "r",
                (10.0 + 50.0 * i as f64 + shift, 10.0, 40.0, 40.0),
            );
        }
        different.clip(&f, 1_500);
    }
    assert!(!codes(&different.found()).contains(&"RV018"));
}

#[test]
fn a_video_that_never_holds_its_last_picture_is_found() {
    let build = |slide_until: u64| {
        let mut p = project(400, 300);
        let f = p.frame();
        let n = p.shape(&f, "rect", "slider", (10.0, 10.0, 40.0, 40.0));
        let ambient = p.shape(&f, "ellipse", "ambient", (200.0, 10.0, 40.0, 40.0));
        p.clip(&f, 4_000);
        p.animate(
            &n,
            "x",
            json!([{"at_ms": 0, "value": 10.0}, {"at_ms": slide_until, "value": 300.0}]),
            None,
        );
        p.animate(&ambient, "y", json!([{"at_ms": 0, "value": 10.0}, {"at_ms": 500, "value": 30.0}, {"at_ms": 1000, "value": 10.0}]), Some("loop"));
        p.found()
    };
    let moving = build(4_000);
    assert!(codes(&moving).contains(&"RV019"), "{:?}", codes(&moving));
    // landing at 3 s holds for a second, and the ambient loop under it does not matter
    assert!(!codes(&build(3_000)).contains(&"RV019"));
    // a slow zoom to the very end is not a hold either
    let mut zoom = project(400, 300);
    let f = zoom.frame();
    zoom.clip(&f, 3_000);
    let src = {
        let mut s = zoom.source();
        s.doc.videos[0].clips[0].motion.scale_to = 1.2;
        s
    };
    assert!(codes(&review(&src, None, None).unwrap()).contains(&"RV019"));
}

#[test]
fn overlaps_list_the_layers_back_to_front_and_skip_the_backdrop() {
    let mut p = project(400, 300);
    let f = p.frame();
    p.shape(&f, "rect", "Sky", (0.0, 0.0, 400.0, 300.0));
    p.shape(&f, "rect", "Hills", (0.0, 100.0, 300.0, 150.0));
    p.shape(&f, "ellipse", "Cloud", (100.0, 50.0, 100.0, 100.0));
    p.shape(&f, "rect", "Far", (350.0, 250.0, 20.0, 20.0));
    p.clip(&f, 3_000);
    let report = p.found();
    assert_eq!(report.overlap_total, 1);
    let region = &report.overlap[0];
    assert_eq!(region["back_to_front"], json!(["Hills", "Cloud"]));
    assert_eq!(
        region["rect"],
        json!({"x": 100.0, "y": 100.0, "width": 100.0, "height": 50.0})
    );
    assert_eq!(region["scene"], json!(1));
    assert!(region["check"].as_str().unwrap().contains("video_crop"));
    let json = report.to_json();
    assert_eq!(json["overlap"]["total"], json!(1));
    assert!(report.text().contains("overlap regions"));
    // informational: no finding mentions it, and nothing overlapping gives an empty list
    assert!(report.findings.iter().all(|f| !f.name.contains("overlap")));
    let mut apart = project(400, 300);
    let f = apart.frame();
    apart.shape(&f, "rect", "A", (0.0, 0.0, 100.0, 100.0));
    apart.shape(&f, "rect", "B", (200.0, 150.0, 100.0, 100.0));
    apart.clip(&f, 3_000);
    assert!(apart.found().overlap.is_empty());
}

#[test]
fn the_tool_description_lists_the_new_codes_and_the_tempo() {
    let defs = definitions();
    let text = defs[0]["description"].as_str().unwrap();
    for code in [
        "RV013", "RV014", "RV015", "RV016", "RV017", "RV018", "RV019", "overlap",
    ] {
        assert!(text.contains(code), "{code}");
    }
    assert!(defs[0]["inputSchema"]["properties"]["bpm"].is_object());
}

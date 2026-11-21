#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::lint::{
    ExportTarget, Finding, LintOptions, RULES, Severity, TextMeasure, TextOverflow, lint,
};
use od_core::{Actor, Document, Node, NodeId, Operation, Session, Workspace};
use serde_json::{Value, json};

struct Fixture {
    ws: Workspace,
    root: NodeId,
}

impl Fixture {
    fn new() -> Fixture {
        let mut doc = Document::new("Lint");
        doc.add_page("Page 1").unwrap();
        let root = doc.pages[0].root.id.clone();
        Fixture {
            ws: Workspace::new(Session::new(doc)),
            root,
        }
    }

    fn apply(&mut self, v: Value) -> od_core::TxReceipt {
        let ops: Vec<Operation> = serde_json::from_value(v).unwrap();
        self.ws.session.apply(Actor::User, None, &ops).unwrap()
    }

    fn create(&mut self, parent: &NodeId, node: Value) -> NodeId {
        let made = self.apply(json!([{"op": "create", "parent": parent.as_str(), "node": node}]));
        made.created[0].clone()
    }

    fn frame(&mut self, w: u32, h: u32) -> NodeId {
        let root = self.root.clone();
        self.create(
            &root,
            json!({"kind": "frame", "name": "F", "width": w, "height": h, "fill": "#ffffff"}),
        )
    }

    fn doc(&self) -> Document {
        self.ws.session.document().clone()
    }
}

fn codes(findings: &[Finding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.code).collect()
}

fn run(doc: &Document) -> Vec<Finding> {
    lint(doc, &LintOptions::default())
}

fn has(findings: &[Finding], code: &str, node: &NodeId) -> bool {
    findings
        .iter()
        .any(|f| f.code == code && f.node.as_ref() == Some(node))
}

fn text(x: f64, y: f64, w: f64, h: f64, body: &str, size: f64) -> Value {
    json!({"kind": "text", "name": "T", "x": x, "y": y, "width": w, "height": h,
           "text": body, "font_size": size, "fill": "#000000"})
}

#[test]
fn a_clean_design_has_no_findings() {
    let mut f = Fixture::new();
    let frame = f.frame(1000, 1000);
    f.create(
        &frame,
        text(100.0, 100.0, 400.0, 100.0, "Hold for everything", 60.0),
    );
    assert_eq!(run(&f.doc()), vec![]);
}

#[test]
fn od001_node_entirely_outside_its_frame() {
    let mut f = Fixture::new();
    let frame = f.frame(100, 100);
    let outside = f.create(
        &frame,
        json!({"kind": "rect", "x": 200, "y": 0, "width": 10, "height": 10}),
    );
    let inside = f.create(
        &frame,
        json!({"kind": "rect", "x": 10, "y": 10, "width": 10, "height": 10}),
    );
    let found = run(&f.doc());
    assert!(has(&found, "OD001", &outside));
    assert!(!found.iter().any(|x| x.node.as_ref() == Some(&inside)));
    let finding = found.iter().find(|x| x.code == "OD001").unwrap();
    assert_eq!(finding.frame.as_ref(), Some(&frame));
    assert_eq!(finding.severity, Severity::Warn);
}

#[test]
fn od002_and_od003_a_bleed_is_info_but_cut_off_text_is_a_warning() {
    let mut f = Fixture::new();
    let frame = f.frame(100, 100);
    let bleed = f.create(
        &frame,
        json!({"kind": "rect", "x": 80, "y": 0, "width": 50, "height": 10}),
    );
    let cut = f.create(&frame, text(80.0, 40.0, 50.0, 30.0, "Cut", 20.0));
    let found = run(&f.doc());
    assert!(has(&found, "OD002", &bleed));
    assert!(has(&found, "OD003", &cut));
    assert_eq!(
        found.iter().find(|x| x.code == "OD002").unwrap().severity,
        Severity::Info
    );
    assert!(!has(&found, "OD001", &bleed));
}

#[test]
fn od001_follows_nested_groups_to_the_frame() {
    let mut f = Fixture::new();
    let frame = f.frame(100, 100);
    let group = f.create(&frame, json!({"kind": "group", "x": 90, "y": 0}));
    let rect = f.create(
        &group,
        json!({"kind": "rect", "x": 20, "y": 0, "width": 5, "height": 5}),
    );
    assert!(has(&run(&f.doc()), "OD001", &rect));
}

struct Tight;

impl TextMeasure for Tight {
    fn overflow(&self, node: &Node) -> Option<TextOverflow> {
        (node.name == "Tall").then_some(TextOverflow {
            needed: 90,
            available: 30,
        })
    }
}

#[test]
fn od010_text_overflow_comes_from_the_measurer_and_is_skipped_without_one() {
    let mut f = Fixture::new();
    let frame = f.frame(1000, 1000);
    let tall = f.create(
        &frame,
        json!({"kind": "text", "name": "Tall", "x": 10, "y": 10,
                                      "width": 300, "height": 30, "text": "Long", "font_size": 40}),
    );
    let ok = f.create(&frame, text(10.0, 100.0, 300.0, 100.0, "Fits", 40.0));
    let doc = f.doc();
    let with = lint(
        &doc,
        &LintOptions {
            text: Some(&Tight),
            ..LintOptions::default()
        },
    );
    assert!(has(&with, "OD010", &tall));
    assert!(!has(&with, "OD010", &ok));
    assert!(!codes(&run(&doc)).contains(&"OD010"));
}

#[test]
fn od011_empty_text() {
    let mut f = Fixture::new();
    let frame = f.frame(1000, 1000);
    let empty = f.create(&frame, text(0.0, 0.0, 300.0, 50.0, "   ", 40.0));
    let full = f.create(&frame, text(0.0, 100.0, 300.0, 50.0, "Hi", 40.0));
    let found = run(&f.doc());
    assert!(has(&found, "OD011", &empty));
    assert!(!has(&found, "OD011", &full));
}

#[test]
fn od012_text_too_small_for_the_frame() {
    let mut f = Fixture::new();
    let frame = f.frame(1000, 1000);
    let small = f.create(&frame, text(0.0, 0.0, 300.0, 50.0, "Tiny", 8.0));
    let fine = f.create(&frame, text(0.0, 100.0, 300.0, 50.0, "Fine", 14.0));
    let found = run(&f.doc());
    assert!(has(&found, "OD012", &small));
    assert!(!has(&found, "OD012", &fine));
}

#[test]
fn od013_low_contrast_against_the_background_fill() {
    let mut f = Fixture::new();
    let frame = f.frame(1000, 1000);
    let panel = f.create(
        &frame,
        json!({"kind": "frame", "x": 0, "y": 500, "width": 1000, "height": 500,
                                       "fill": "#101010"}),
    );
    let faint = f.create(
        &frame,
        json!({"kind": "text", "x": 10, "y": 10, "width": 300, "height": 50,
                                       "text": "Faint", "font_size": 20, "fill": "#cccccc"}),
    );
    let on_dark = f.create(
        &panel,
        json!({"kind": "text", "x": 10, "y": 10, "width": 300, "height": 50,
                                         "text": "Bright", "font_size": 20, "fill": "#ffffff"}),
    );
    let invisible_on_dark = f.create(&frame, json!({"kind": "text", "x": 10, "y": 600, "width": 300,
                                       "height": 50, "text": "Lost", "font_size": 20, "fill": "#111111"}));
    let found = run(&f.doc());
    assert!(has(&found, "OD013", &faint));
    assert!(has(&found, "OD013", &invisible_on_dark));
    assert!(!has(&found, "OD013", &on_dark));
}

#[test]
fn od013_large_text_has_the_lower_threshold_and_pictures_are_not_guessed() {
    let mut f = Fixture::new();
    let frame = f.frame(1000, 1000);
    // #959595 on white is about 3:1: fine for large text, not for body text
    let large = f.create(
        &frame,
        json!({"kind": "text", "x": 0, "y": 0, "width": 500, "height": 80,
                                       "text": "Big", "font_size": 48, "fill": "#8a8a8a"}),
    );
    let body = f.create(
        &frame,
        json!({"kind": "text", "x": 0, "y": 200, "width": 500, "height": 40,
                                      "text": "Body", "font_size": 20, "fill": "#8a8a8a"}),
    );
    f.create(
        &frame,
        json!({"kind": "image", "x": 0, "y": 400, "width": 500, "height": 200,
                           "image": "media/photo.png"}),
    );
    let over_photo = f.create(&frame, json!({"kind": "text", "x": 10, "y": 410, "width": 400,
                                            "height": 40, "text": "Over", "font_size": 20, "fill": "#ffffff"}));
    let found = run(&f.doc());
    assert!(!has(&found, "OD013", &large), "{found:?}");
    assert!(has(&found, "OD013", &body));
    assert!(!has(&found, "OD013", &over_photo));
}

#[test]
fn od020_missing_media_is_found_only_when_the_project_folder_is_known() {
    let mut f = Fixture::new();
    let frame = f.frame(1000, 1000);
    let img = f.create(
        &frame,
        json!({"kind": "image", "x": 0, "y": 0, "width": 100, "height": 100,
                                     "image": "media/logo.png"}),
    );
    let doc = f.doc();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("media")).unwrap();
    let opts = LintOptions {
        project_dir: Some(tmp.path()),
        ..LintOptions::default()
    };
    assert!(has(&lint(&doc, &opts), "OD020", &img));
    assert!(!codes(&run(&doc)).contains(&"OD020"));
    std::fs::write(tmp.path().join("media/logo.png"), b"x").unwrap();
    assert!(!codes(&lint(&doc, &opts)).contains(&"OD020"));
}

#[test]
fn od021_an_undefined_token() {
    let mut f = Fixture::new();
    let frame = f.frame(1000, 1000);
    let mut doc = f.doc();
    let rect: Node = serde_json::from_value(json!({"id": "n_900", "kind": "rect", "name": "R",
        "x": 0, "y": 0, "width": 10, "height": 10, "fill": "{brand.missing}"}))
    .unwrap();
    doc.next_id = 1000;
    let loc = doc.locate(&frame).unwrap();
    doc.node_at_mut(&loc).unwrap().children.push(rect);
    let found = run(&doc);
    assert!(has(&found, "OD021", &NodeId::parse("n_900").unwrap()));
    assert_eq!(
        found.iter().find(|x| x.code == "OD021").unwrap().severity,
        Severity::Error
    );
    doc.tokens.insert("brand.missing".into(), "#ff0000".into());
    assert!(!codes(&run(&doc)).contains(&"OD021"));
}

fn video_fixture(f: &mut Fixture, frame: &NodeId) -> NodeId {
    f.apply(json!([{"op": "create_video", "name": "V", "fps": 30, "width": 640, "height": 360}]));
    let video = f.doc().videos[0].id.clone();
    f.apply(
        json!([{"op": "add_clip", "video": video.as_str(), "frame": frame.as_str(),
                    "duration_ms": 1000}]),
    );
    video
}

#[test]
fn od030_an_animated_node_that_is_hidden() {
    let mut f = Fixture::new();
    let frame = f.frame(640, 360);
    let hidden = f.create(
        &frame,
        json!({"kind": "rect", "x": 0, "y": 0, "width": 10, "height": 10,
                                        "visible": false}),
    );
    let fade = f.create(
        &frame,
        json!({"kind": "rect", "x": 0, "y": 0, "width": 10, "height": 10,
                                      "opacity": 0}),
    );
    let stuck = f.create(
        &frame,
        json!({"kind": "rect", "x": 0, "y": 0, "width": 10, "height": 10,
                                       "opacity": 0}),
    );
    let video = video_fixture(&mut f, &frame);
    for (node, prop) in [(&hidden, "x"), (&fade, "opacity"), (&stuck, "x")] {
        f.apply(
            json!([{"op": "animate", "video": video.as_str(), "node": node.as_str(), "prop": prop,
                        "keys": [{"at_ms": 0, "value": 1.0}, {"at_ms": 500, "value": 1.0}]}]),
        );
    }
    let found = run(&f.doc());
    // hidden nodes are skipped by the tree walk, so the video rule is what reports them
    assert!(has(&found, "OD030", &hidden));
    assert!(has(&found, "OD030", &stuck));
    assert!(!has(&found, "OD030", &fade));
}

#[test]
fn od031_a_frame_shown_in_a_video_that_is_invisible() {
    let mut f = Fixture::new();
    let frame = f.frame(640, 360);
    let ghost = f.frame(640, 360);
    video_fixture(&mut f, &frame);
    let video = f.doc().videos[0].id.clone();
    f.apply(json!([{"op": "set", "id": ghost.as_str(), "prop": {"name": "visible", "value": false}},
                   {"op": "add_clip", "video": video.as_str(), "frame": ghost.as_str(), "duration_ms": 500}]));
    let found = run(&f.doc());
    assert!(has(&found, "OD031", &ghost));
    assert!(!has(&found, "OD031", &frame));
}

#[test]
fn od040_overlapping_siblings() {
    let mut f = Fixture::new();
    let frame = f.frame(1000, 1000);
    f.create(&frame, text(0.0, 0.0, 300.0, 100.0, "One", 40.0));
    let two = f.create(&frame, text(100.0, 20.0, 300.0, 100.0, "Two", 40.0));
    let apart = f.create(&frame, text(0.0, 500.0, 300.0, 100.0, "Apart", 40.0));
    let found = run(&f.doc());
    assert!(has(&found, "OD040", &two));
    assert!(!has(&found, "OD040", &apart));
}

#[test]
fn od041_frame_size_against_the_chosen_export_targets() {
    let mut f = Fixture::new();
    let wrong = f.frame(100, 100);
    let right = f.frame(1290, 2796);
    let doc = f.doc();
    let target = || ExportTarget {
        id: "apple-iphone-6.9".into(),
        accepts: Box::new(|w, h| (w, h) == (1290, 2796)),
    };
    let found = lint(
        &doc,
        &LintOptions {
            export_targets: vec![target()],
            ..LintOptions::default()
        },
    );
    assert!(has(&found, "OD041", &wrong));
    assert!(!has(&found, "OD041", &right));
    assert!(!codes(&run(&doc)).contains(&"OD041"));
}

#[test]
fn od050_a_clip_showing_a_missing_frame() {
    let mut f = Fixture::new();
    let frame = f.frame(640, 360);
    let video = video_fixture(&mut f, &frame);
    let mut doc = f.doc();
    assert!(!codes(&run(&doc)).contains(&"OD050"));
    doc.videos[0].clips[0].frame = Some(NodeId::parse("n_999").unwrap());
    let found = run(&doc);
    let finding = found.iter().find(|x| x.code == "OD050").unwrap();
    assert_eq!(finding.video.as_ref(), Some(&video));
}

#[test]
fn od051_and_od054_footage_and_sound_files_that_are_missing() {
    let mut f = Fixture::new();
    let frame = f.frame(640, 360);
    let video = video_fixture(&mut f, &frame);
    f.apply(json!([
        {"op": "add_clip", "video": video.as_str(),
         "footage": {"file": "media/clip.mp4", "in_ms": 0}, "duration_ms": 500},
        {"op": "set_video", "id": video.as_str(),
         "prop": {"name": "audio", "value": {"file": "media/song.mp3"}}},
    ]));
    let doc = f.doc();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("media")).unwrap();
    let opts = LintOptions {
        project_dir: Some(tmp.path()),
        ..LintOptions::default()
    };
    let found = lint(&doc, &opts);
    assert!(codes(&found).contains(&"OD051"));
    assert!(codes(&found).contains(&"OD054"));
    std::fs::write(tmp.path().join("media/clip.mp4"), b"x").unwrap();
    std::fs::write(tmp.path().join("media/song.mp3"), b"x").unwrap();
    let found = lint(&doc, &opts);
    assert!(!codes(&found).contains(&"OD051") && !codes(&found).contains(&"OD054"));
}

#[test]
fn od052_an_overlay_outside_the_videos_time() {
    let mut f = Fixture::new();
    let frame = f.frame(640, 360);
    let title = f.frame(640, 100);
    let video = video_fixture(&mut f, &frame);
    let add = |start: u64, dur: u32| {
        json!([{"op": "add_overlay", "video": video.as_str(), "frame": title.as_str(),
                "start_ms": start, "duration_ms": dur}])
    };
    f.apply(add(100, 500));
    assert!(!codes(&run(&f.doc())).contains(&"OD052"));
    f.apply(add(800, 500));
    f.apply(add(5000, 500));
    let found: Vec<_> = run(&f.doc())
        .into_iter()
        .filter(|x| x.code == "OD052")
        .collect();
    assert_eq!(found.len(), 2);
}

#[test]
fn od053_animation_keys_beyond_the_video() {
    let mut f = Fixture::new();
    let frame = f.frame(640, 360);
    let rect = f.create(
        &frame,
        json!({"kind": "rect", "x": 0, "y": 0, "width": 10, "height": 10}),
    );
    let video = video_fixture(&mut f, &frame);
    let animate = |end: u64| {
        json!([{"op": "animate", "video": video.as_str(), "node": rect.as_str(), "prop": "x",
                "keys": [{"at_ms": 0, "value": 0.0}, {"at_ms": end, "value": 50.0}]}])
    };
    f.apply(animate(900));
    assert!(!codes(&run(&f.doc())).contains(&"OD053"));
    f.apply(animate(4000));
    assert!(codes(&run(&f.doc())).contains(&"OD053"));
}

#[test]
fn od055_an_empty_video() {
    let mut f = Fixture::new();
    f.apply(json!([{"op": "create_video", "name": "V", "fps": 30, "width": 640, "height": 360}]));
    let found = run(&f.doc());
    assert!(codes(&found).contains(&"OD055"));
}

fn variable_fixture(f: &mut Fixture) -> (NodeId, NodeId) {
    let frame = f.frame(1000, 1000);
    let t = f.create(&frame, text(0.0, 0.0, 300.0, 100.0, "Hi", 40.0));
    f.apply(
        json!([{"op": "declare_variable", "variable": {"id": "title", "kind": "string",
        "label": "Title", "default": "Welcome", "binds": [{"node": t.as_str(), "prop": "text"}]}}]),
    );
    (frame, t)
}

#[test]
fn od060_a_binding_to_a_deleted_node() {
    let mut f = Fixture::new();
    let (_frame, t) = variable_fixture(&mut f);
    assert!(!codes(&run(&f.doc())).contains(&"OD060"));
    f.apply(json!([{"op": "delete", "id": t.as_str()}]));
    assert!(has(&run(&f.doc()), "OD060", &t));
}

#[test]
fn od061_a_binding_the_node_cannot_take() {
    let mut f = Fixture::new();
    let (frame, _t) = variable_fixture(&mut f);
    let mut doc = f.doc();
    assert!(!codes(&run(&doc)).contains(&"OD061"));
    doc.variables[0].binds[0].node = frame;
    assert!(codes(&run(&doc)).contains(&"OD061"));
}

#[test]
fn od062_translations_shadow_a_bound_text() {
    let mut f = Fixture::new();
    let (_frame, t) = variable_fixture(&mut f);
    assert!(!codes(&run(&f.doc())).contains(&"OD062"));
    f.apply(
        json!([{"op": "set_localized_text", "locale": "de", "id": t.as_str(), "text": "Hallo"}]),
    );
    let found = run(&f.doc());
    assert_eq!(
        found.iter().find(|x| x.code == "OD062").unwrap().severity,
        Severity::Info
    );
}

#[test]
fn od063_two_variables_writing_one_property() {
    let mut f = Fixture::new();
    let (_frame, t) = variable_fixture(&mut f);
    assert!(!codes(&run(&f.doc())).contains(&"OD063"));
    f.apply(
        json!([{"op": "declare_variable", "variable": {"id": "other", "kind": "string",
        "label": "Other", "default": "x", "binds": [{"node": t.as_str(), "prop": "text"}]}}]),
    );
    assert!(has(&run(&f.doc()), "OD063", &t));
}

#[test]
fn every_rule_has_a_unique_stable_code_and_a_row_in_the_wiki_table() {
    let wiki = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/wiki/lint-rules.md"
    ))
    .unwrap();
    let mut seen = std::collections::HashSet::new();
    for rule in RULES {
        assert!(seen.insert(rule.code), "{} is listed twice", rule.code);
        assert!(
            rule.code.starts_with("OD") && rule.code.len() == 5,
            "{}",
            rule.code
        );
        let row = format!(
            "| `{}` | {} | {} |",
            rule.code,
            rule.name,
            rule.severity.as_str()
        );
        assert!(
            wiki.contains(&row),
            "docs/wiki/lint-rules.md lacks the row {row}"
        );
    }
}

fn scene_fixture(w: u32, h: u32) -> (Fixture, NodeId, NodeId) {
    let mut f = Fixture::new();
    let frame = f.frame(w, h);
    f.apply(json!([{"op": "create_video", "name": "V", "fps": 30, "width": w, "height": h}]));
    let video = f.ws.session.document().videos[0].id.clone();
    f.apply(json!([{"op": "add_clip", "video": video.as_str(), "frame": frame.as_str(), "duration_ms": 3000}]));
    (f, frame, video)
}

fn fade(f: &mut Fixture, video: &NodeId, node: &NodeId) {
    f.apply(
        json!([{"op": "animate", "video": video.as_str(), "node": node.as_str(), "prop": "opacity",
        "keys": [{"at_ms": 0, "value": 0.2}, {"at_ms": 800, "value": 1.0}]}]),
    );
}

#[test]
fn a_picture_with_animated_text_is_not_an_animated_scene() {
    let (mut f, frame, video) = scene_fixture(1000, 1000);
    f.create(
        &frame,
        json!({"kind": "image", "name": "Photo", "x": 0, "y": 0, "width": 1000, "height": 1000, "image": "media/photo.png"}),
    );
    let title = f.create(&frame, text(100.0, 100.0, 600.0, 100.0, "Hello", 80.0));
    fade(&mut f, &video, &title);
    let found = run(&f.doc());
    assert!(has(&found, "OD090", &frame), "{found:?}");
    assert!(!codes(&found).contains(&"OD091"), "OD090 already says it");

    let star = f.create(
        &frame,
        json!({"kind": "ellipse", "name": "Star", "x": 800, "y": 100, "width": 40, "height": 40, "fill": "#ffcc00"}),
    );
    fade(&mut f, &video, &star);
    let found = run(&f.doc());
    assert!(!codes(&found).contains(&"OD090"), "{found:?}");
    assert!(!codes(&found).contains(&"OD091"), "{found:?}");
}

#[test]
fn a_paper_grain_overlay_is_not_the_backdrop() {
    let (mut f, frame, video) = scene_fixture(1000, 1000);
    f.create(
        &frame,
        json!({"kind": "image", "name": "block:paper_grain#1/grain_a", "x": 0, "y": 0, "width": 1000, "height": 1000, "image": "media/grain.png"}),
    );
    let star = f.create(
        &frame,
        json!({"kind": "ellipse", "name": "Star", "x": 800, "y": 100, "width": 40, "height": 40, "fill": "#ffcc00"}),
    );
    fade(&mut f, &video, &star);
    let found = run(&f.doc());
    assert!(!codes(&found).contains(&"OD090"), "{found:?}");
}

#[test]
fn a_scene_where_only_text_and_rectangles_move_is_flagged() {
    let (mut f, frame, video) = scene_fixture(1920, 1080);
    let title = f.create(&frame, text(100.0, 100.0, 600.0, 100.0, "Hello", 80.0));
    let band = f.create(
        &frame,
        json!({"kind": "rect", "name": "Band", "x": 0, "y": 800, "width": 1920, "height": 200, "fill": "#222222"}),
    );
    fade(&mut f, &video, &title);
    fade(&mut f, &video, &band);
    assert!(has(&run(&f.doc()), "OD091", &frame));
    let moon = f.create(
        &frame,
        json!({"kind": "path", "name": "Moon", "x": 800, "y": 100, "width": 80, "height": 80,
               "path": "M0 0 L10 0 L5 10 Z", "fill": "#ffeeaa"}),
    );
    fade(&mut f, &video, &moon);
    assert!(!codes(&run(&f.doc())).contains(&"OD091"));
}

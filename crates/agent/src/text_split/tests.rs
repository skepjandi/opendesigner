#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::animation::AnimProp;
use od_core::{Actor, Document, Node, NodeId, NodeKind, Operation, Workspace};
use od_render::{PlacedGlyph, PlacedLine};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::pieces::{By, default_by, split};
use super::reveal::hold_ms;
use crate::tools::{CallCtx, Content, Turns};

const COPY: &str = "Hello brave new world of kinetic type";

struct Fixture {
    _dir: TempDir,
    ws: Workspace,
    frame: NodeId,
    text: NodeId,
    video: NodeId,
}

fn fixture_with(text: &str, extra: Value) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Split").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let frame = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: serde_json::from_value(json!({
                    "kind": "frame", "name": "Slide", "width": 400, "height": 300, "fill": "#ffffff"
                }))
                .unwrap(),
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    let mut spec = json!({
        "kind": "text", "name": "Headline", "x": 20, "y": 30, "width": 210, "height": 200,
        "text": text, "font_size": 28, "font_weight": 700, "fill": "#101010"
    });
    if let (Some(base), Some(more)) = (spec.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    let text = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: frame.clone(),
                index: None,
                node: serde_json::from_value(spec).unwrap(),
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "Cut".into(),
                fps: 30,
                width: 400,
                height: 300,
            }],
        )
        .unwrap();
    let video = ws.session.document().videos[0].id.clone();
    Fixture {
        _dir: dir,
        ws,
        frame,
        text,
        video,
    }
}

fn fixture() -> Fixture {
    fixture_with(COPY, json!({}))
}

fn call(ws: &mut Workspace, name: &str, args: Value) -> Result<Value, String> {
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws,
        actor: Actor::agent("tester").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    match crate::tools::call(&mut ctx, name, args) {
        Ok(out) => match out.content.first() {
            Some(Content::Text(t)) => Ok(serde_json::from_str(t).unwrap()),
            _ => Err("no text".into()),
        },
        Err(e) => Err(e.0),
    }
}

impl Fixture {
    fn split(&mut self, by: &str, extra: Value) -> Result<Value, String> {
        let args = split_args(self, by, extra);
        call(&mut self.ws, "split_text", args)
    }

    fn reveal(&mut self, style: &str, extra: Value) -> Result<Value, String> {
        let args = reveal_args(self, style, extra);
        call(&mut self.ws, "text_reveal", args)
    }

    fn doc(&self) -> &Document {
        self.ws.session.document()
    }

    fn text_node(&self) -> &Node {
        self.doc().node(&self.text).unwrap()
    }

    fn rows(&self) -> Vec<PlacedLine> {
        od_render::text_lines(self.text_node()).unwrap()
    }

    fn pixels(&self) -> Vec<u8> {
        od_render::render_frame(self.doc(), &self.frame, 1.0, None)
            .unwrap()
            .pixmap
            .data()
            .to_vec()
    }

    fn group(&self, report: &Value) -> &Node {
        let id = NodeId::parse(report["group"].as_str().unwrap()).unwrap();
        self.doc().node(&id).unwrap()
    }
}

fn ink(data: &[u8]) -> usize {
    data.chunks(4)
        .filter(|p| p[0] < 200 || p[1] < 200 || p[2] < 200)
        .count()
}

/// The box the ink covers, in pixels.
fn ink_box(data: &[u8]) -> (usize, usize, usize, usize) {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (i, p) in data.chunks(4).enumerate() {
        if p[0] < 200 || p[1] < 200 || p[2] < 200 {
            let (x, y) = (i % 400, i / 400);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0, y0, x1, y1)
}

fn differing(a: &[u8], b: &[u8], threshold: i32) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(p, q)| {
            p.iter()
                .zip(q.iter())
                .any(|(x, y)| (i32::from(*x) - i32::from(*y)).abs() > threshold)
        })
        .count()
}

fn split_args(f: &Fixture, by: &str, extra: Value) -> Value {
    let mut args = json!({"node": f.text.to_string(), "by": by});
    if let (Some(base), Some(more)) = (args.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    args
}

fn nodes_below(node: &Node) -> Vec<&Node> {
    let mut out = Vec::new();
    for c in &node.children {
        out.push(c);
        out.extend(nodes_below(c));
    }
    out
}

fn texts_of(group: &Node) -> Vec<&Node> {
    nodes_below(group)
        .into_iter()
        .filter(|n| n.kind == NodeKind::Text)
        .collect()
}

#[test]
fn the_text_wraps_to_several_rows_in_the_fixture() {
    let f = fixture();
    assert!(f.rows().len() >= 3, "{:?}", f.rows().len());
}

#[test]
fn lines_are_the_rows_the_renderer_wrapped() {
    let mut f = fixture();
    let rows = f.rows();
    let report = f.split("lines", json!({})).unwrap();
    let group = f.group(&report);
    let texts = texts_of(group);
    assert_eq!(texts.len(), rows.len());
    for (node, row) in texts.iter().zip(&rows) {
        let wanted = row.text[row.glyphs.iter().map(|g| g.start).min().unwrap()
            ..row.glyphs.iter().map(|g| g.end).max().unwrap()]
            .trim();
        assert_eq!(node.text.as_deref(), Some(wanted));
    }
    let names: Vec<&str> = texts.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names[0], "Headline.l0");
    assert_eq!(names[1], "Headline.l1");
    assert_eq!(report["count"], rows.len());
}

#[test]
fn words_come_in_reading_order_one_per_word() {
    let mut f = fixture();
    let report = f.split("words", json!({})).unwrap();
    let group = f.group(&report);
    let texts = texts_of(group);
    let joined: Vec<&str> = texts.iter().filter_map(|n| n.text.as_deref()).collect();
    assert_eq!(joined, COPY.split(' ').collect::<Vec<_>>());
    assert_eq!(texts[0].name, "Headline.w0");
    assert_eq!(texts[6].name, "Headline.w6");
    let ids: Vec<String> = report["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let in_doc: Vec<String> = texts.iter().map(|n| n.id.to_string()).collect();
    assert_eq!(ids, in_doc);
}

#[test]
fn chars_skip_spaces_and_keep_the_original_style() {
    let mut f = fixture();
    let fill = f.text_node_fill();
    let report = f.split("chars", json!({})).unwrap();
    let group = f.group(&report);
    let texts = texts_of(group);
    let letters: String = COPY.chars().filter(|c| !c.is_whitespace()).collect();
    let got: String = texts.iter().filter_map(|n| n.text.as_deref()).collect();
    assert_eq!(got, letters);
    assert_eq!(texts[0].name, "Headline.c0");
    for t in &texts {
        assert_eq!(t.font_size, Some(28.0));
        assert_eq!(t.font_weight, Some(700.0));
        assert_eq!(t.fill, fill);
    }
}

impl Fixture {
    fn text_node_fill(&self) -> Option<od_core::Paint> {
        self.doc()
            .node(&self.text)
            .map(|n| n.fill.clone())
            .unwrap_or(None)
    }
}

#[test]
fn pieces_sit_inside_the_rows_and_the_box_of_the_original() {
    let f = fixture();
    let rows = f.rows();
    let original = f.text_node().clone();
    for by in ["chars", "words", "lines"] {
        let mut g = fixture();
        let report = g.split(by, json!({})).unwrap();
        let group = g.group(&report);
        assert_eq!((group.x, group.y), (original.x, original.y));
        for t in texts_of(group) {
            let centre_y = t.y + t.height / 2.0;
            assert!(
                rows.iter().any(
                    |r| centre_y >= f64::from(r.top) && centre_y <= f64::from(r.top + r.height)
                ),
                "{by}: {} centre {centre_y} is in no row",
                t.name
            );
            assert!(
                t.x >= 0.0 && t.x + t.width <= original.width + 2.0,
                "{by}: {}",
                t.name
            );
        }
    }
}

fn rest_difference(by: &str, wrap: bool) -> (usize, usize) {
    let mut f = fixture();
    let before = f.pixels();
    f.split(by, json!({"wrap": wrap})).unwrap();
    let after = f.pixels();
    (differing(&before, &after, 96), ink(&before))
}

#[test]
fn a_split_renders_the_same_at_rest() {
    for (by, wrap, share) in [
        ("lines", false, 0.05),
        ("words", false, 0.10),
        ("chars", false, 0.20),
        ("words", true, 0.10),
        ("chars", true, 0.20),
    ] {
        let (changed, inked) = rest_difference(by, wrap);
        assert!(inked > 500, "the fixture draws something");
        let ratio = changed as f64 / inked as f64;
        assert!(
            ratio <= share,
            "{by} wrap={wrap}: {changed} of {inked} ink pixels differ ({ratio:.3})"
        );
        let mut f = fixture();
        let before = f.pixels();
        f.split(by, json!({"wrap": wrap})).unwrap();
        let (x0, y0, x1, y1) = ink_box(&before);
        let (a0, b0, a1, b1) = ink_box(&f.pixels());
        for (now, was, side) in [
            (a0, x0, "left"),
            (b0, y0, "top"),
            (a1, x1, "right"),
            (b1, y1, "bottom"),
        ] {
            assert!(
                now.abs_diff(was) <= 1,
                "{by} wrap={wrap}: the ink moved at the {side} ({was} to {now})"
            );
        }
    }
}

#[test]
fn centered_text_splits_where_it_was_drawn() {
    let mut f = fixture_with(COPY, json!({"text_align": "center"}));
    let before = f.pixels();
    f.split("words", json!({})).unwrap();
    let after = f.pixels();
    let (x0, y0, x1, y1) = ink_box(&before);
    let (a0, b0, a1, b1) = ink_box(&after);
    assert!(
        a0.abs_diff(x0) <= 2
            && b0.abs_diff(y0) <= 2
            && a1.abs_diff(x1) <= 2
            && b1.abs_diff(y1) <= 2,
        "the ink moved: {x0},{y0}..{x1},{y1} became {a0},{b0}..{a1},{b1}"
    );
}

#[test]
fn one_undo_puts_the_original_back() {
    let mut f = fixture();
    let original = f.text_node().clone();
    let before = f.pixels();
    f.split("chars", json!({})).unwrap();
    assert!(
        f.doc().node(&f.text).is_none(),
        "replace removes the original"
    );
    f.ws.session.undo(Actor::User).unwrap();
    assert_eq!(f.doc().node(&f.text).unwrap(), &original);
    assert_eq!(f.pixels(), before);
    assert_eq!(f.doc().node_count(), 3, "root, frame, text");
}

#[test]
fn keep_hides_the_original_and_undo_still_restores_it() {
    let mut f = fixture();
    let report = f.split("words", json!({"mode": "keep"})).unwrap();
    assert!(!f.text_node().visible);
    assert_eq!(f.group(&report).name, "Headline split");
    let before_undo = f.doc().node_count();
    assert!(before_undo > 3);
    f.ws.session.undo(Actor::User).unwrap();
    assert!(f.text_node().visible);
    assert_eq!(f.doc().node_count(), 3);
}

#[test]
fn wrap_puts_each_piece_in_a_clipping_mask() {
    let mut f = fixture();
    let report = f.split("words", json!({"wrap": true})).unwrap();
    let group = f.group(&report);
    assert_eq!(group.children.len(), 7);
    for (i, mask) in group.children.iter().enumerate() {
        assert_eq!(mask.kind, NodeKind::Group);
        assert!(mask.clip);
        assert_eq!(mask.name, format!("Headline.w{i}.mask"));
        assert_eq!(mask.children.len(), 1);
        assert_eq!(mask.children[0].kind, NodeKind::Text);
        assert_eq!(mask.children[0].name, format!("Headline.w{i}"));
    }
    assert_eq!(report["masks"].as_array().unwrap().len(), 7);
    assert_eq!(report["ids"].as_array().unwrap().len(), 7);
}

#[test]
fn too_many_pieces_are_refused() {
    let long = "abcdefghij ".repeat(40);
    let mut f = fixture_with(&long, json!({"width": 400, "font_size": 12}));
    let err = f.split("chars", json!({})).unwrap_err();
    assert!(err.contains("300"), "{err}");
    assert!(f.text_node().visible);
    f.split("words", json!({})).unwrap();
}

#[test]
fn only_text_can_be_split() {
    let mut f = fixture();
    let frame = f.frame.to_string();
    let err = call(
        &mut f.ws,
        "split_text",
        json!({"node": frame, "by": "words"}),
    )
    .unwrap_err();
    assert!(err.contains("only a text node"), "{err}");
}

fn reveal_args(f: &Fixture, style: &str, extra: Value) -> Value {
    let mut args =
        json!({"video": f.video.to_string(), "node": f.text.to_string(), "style": style});
    if let (Some(base), Some(more)) = (args.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    args
}

fn track<'a>(f: &'a Fixture, node: &str, prop: AnimProp) -> &'a od_core::Animation {
    let id = NodeId::parse(node).unwrap();
    f.doc().videos[0]
        .animations
        .iter()
        .find(|a| a.node == id && a.prop == prop)
        .unwrap()
}

#[test]
fn rise_defaults_to_words_masked_with_stagger_90_and_450_ms() {
    let mut f = fixture();
    let report = f.reveal("rise", json!({"at_ms": 200})).unwrap();
    assert_eq!(report["by"], "words");
    assert_eq!(report["wrap"], true);
    assert_eq!(report["each_ms"], 90);
    assert_eq!(report["duration_ms"], 450);
    assert_eq!(report["ease"], "power4_out");
    let pieces = report["pieces"].as_array().unwrap();
    assert_eq!(pieces.len(), 7);
    for (i, p) in pieces.iter().enumerate() {
        assert_eq!(p["start_ms"], 200 + 90 * i as u64);
        assert_eq!(p["end_ms"], 650 + 90 * i as u64);
    }
    assert_eq!(report["ends_at_ms"], 650 + 90 * 6);
    let first = pieces[0]["id"].as_str().unwrap();
    let y = track(&f, first, AnimProp::Y);
    let height = f.doc().node(&NodeId::parse(first).unwrap()).unwrap().height;
    assert_eq!(y.value_at(0.0), Some(height));
    assert!(
        y.value_at(650.0).is_some_and(|v| v.abs() < 1e-3),
        "settled by its end"
    );
    assert_eq!(y.value_at(5000.0), Some(0.0));
    assert!(report["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn a_short_headline_defaults_to_characters_at_40_ms() {
    let mut f = fixture_with("Hi there", json!({"width": 300}));
    let report = f.reveal("fade", json!({})).unwrap();
    assert_eq!(report["by"], "chars");
    assert_eq!(report["each_ms"], 40);
    let starts: Vec<u64> = report["pieces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["start_ms"].as_u64().unwrap())
        .collect();
    assert_eq!(starts, vec![0, 40, 80, 120, 160, 200, 240]);
    assert_eq!(report["wrap"], false);
}

#[test]
fn typewriter_makes_each_piece_appear_at_once_33_ms_apart() {
    let mut f = fixture_with("Hi there", json!({"width": 300}));
    let report = f.reveal("typewriter", json!({"at_ms": 100})).unwrap();
    assert_eq!(report["by"], "chars");
    assert_eq!(report["each_ms"], 33);
    let second = report["pieces"][1]["id"].as_str().unwrap();
    let opacity = track(&f, second, AnimProp::Opacity);
    assert_eq!(opacity.value_at(100.0 + 33.0 - 1.0), Some(0.0));
    assert_eq!(opacity.value_at(100.0 + 33.0 + 60.0), Some(1.0));
}

#[test]
fn pop_scales_from_half_and_fade_slide_have_their_tracks() {
    for (style, expect) in [("pop", "scale"), ("slide", "x"), ("fade", "opacity")] {
        let mut f = fixture();
        let report = f.reveal(style, json!({})).unwrap();
        let tracks: Vec<&str> = report["tracks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(tracks.contains(&expect), "{style}: {tracks:?}");
    }
}

#[test]
fn chars_on_a_long_text_warn_and_a_slow_stagger_warns() {
    let mut f = fixture();
    let report = f
        .reveal("fade", json!({"by": "chars", "each_ms": 200}))
        .unwrap();
    let warnings = report["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
}

#[test]
fn the_hold_follows_the_readability_rule() {
    assert_eq!(hold_ms("Hello world"), 1_800);
    assert_eq!(
        hold_ms("extraordinary"),
        2_200,
        "a long word counts as three short ones"
    );
    assert_eq!(hold_ms("one two three"), 2_200);
    let mut f = fixture();
    let report = f.reveal("fade", json!({})).unwrap();
    assert_eq!(report["hold_ms"], 1_000 + 8 * 400);
    assert_eq!(
        report["leave_no_earlier_than_ms"].as_u64().unwrap(),
        report["ends_at_ms"].as_u64().unwrap() + report["hold_ms"].as_u64().unwrap()
    );
}

#[test]
fn one_undo_removes_the_split_and_its_keys() {
    let mut f = fixture();
    let original = f.text_node().clone();
    f.reveal("rise", json!({})).unwrap();
    assert!(!f.doc().videos[0].animations.is_empty());
    f.ws.session.undo(Actor::User).unwrap();
    assert!(f.doc().videos[0].animations.is_empty());
    assert_eq!(f.doc().node(&f.text).unwrap(), &original);
}

#[test]
fn a_group_split_earlier_is_animated_as_it_is() {
    let mut f = fixture();
    let split_report = f.split("words", json!({"wrap": true})).unwrap();
    let group = split_report["group"].as_str().unwrap().to_string();
    let args = json!({"video": f.video.to_string(), "node": group, "style": "rise"});
    let report = call(&mut f.ws, "text_reveal", args).unwrap();
    assert_eq!(report["ids"], split_report["ids"]);
    assert!(report["split"].is_string());
    assert_eq!(f.doc().videos[0].animations.len(), 7);
}

#[test]
fn an_unknown_video_or_a_late_reveal_is_refused() {
    let mut f = fixture();
    let mut args = reveal_args(&f, "fade", json!({}));
    args["video"] = json!("v_99");
    assert!(call(&mut f.ws, "text_reveal", args).is_err());
    let late = reveal_args(&f, "fade", json!({"at_ms": 599_900, "each_ms": 1000}));
    let err = call(&mut f.ws, "text_reveal", late).unwrap_err();
    assert!(err.contains("at most"), "{err}");
    assert!(f.text_node().visible && f.doc().videos[0].animations.is_empty());
}

fn row(text: &str, glyphs: &[(usize, usize, f32, f32)], top: f32) -> PlacedLine {
    PlacedLine {
        paragraph: 0,
        text: text.to_string(),
        top,
        baseline: top + 20.0,
        height: 24.0,
        width: 100.0,
        rtl: false,
        glyphs: glyphs
            .iter()
            .map(|&(start, end, x, width)| PlacedGlyph {
                start,
                end,
                x,
                y: 0.0,
                width,
            })
            .collect(),
    }
}

#[test]
fn spaces_are_never_pieces_and_punctuation_sticks_to_its_word() {
    let rows = [row(
        "Hi, you",
        &[
            (0, 1, 0.0, 10.0),
            (1, 2, 10.0, 5.0),
            (2, 3, 15.0, 4.0),
            (3, 4, 19.0, 4.0),
            (4, 5, 23.0, 8.0),
            (5, 6, 31.0, 8.0),
            (6, 7, 39.0, 8.0),
        ],
        0.0,
    )];
    let chars = split(&rows, By::Chars);
    assert_eq!(
        chars.iter().map(|p| p.text.as_str()).collect::<String>(),
        "Hi,you"
    );
    let words = split(&rows, By::Words);
    assert_eq!(
        words.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(),
        ["Hi,", "you"]
    );
    assert_eq!(words[0].edges(), (0.0, 19.0));
    assert_eq!(words[1].edges(), (23.0, 47.0));
    let lines = split(&rows, By::Lines);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "Hi, you");
}

#[test]
fn blank_rows_are_skipped_and_line_numbers_count_the_rest() {
    let rows = [
        row("ab", &[(0, 1, 0.0, 8.0), (1, 2, 8.0, 8.0)], 0.0),
        row("", &[], 24.0),
        row("c", &[(0, 1, 0.0, 8.0)], 48.0),
    ];
    let words = split(&rows, By::Words);
    assert_eq!(words.iter().map(|p| p.line).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(words[1].top, 48.0);
}

#[test]
fn the_default_cut_follows_the_length_of_the_text() {
    let one = [row(
        "Hello",
        &[
            (0, 1, 0.0, 8.0),
            (1, 2, 8.0, 8.0),
            (2, 3, 16.0, 8.0),
            (3, 4, 24.0, 8.0),
            (4, 5, 32.0, 8.0),
        ],
        0.0,
    )];
    assert_eq!(default_by(&one), By::Chars);
    let long: String = "word ".repeat(5);
    let glyphs: Vec<(usize, usize, f32, f32)> = (0..long.len())
        .map(|i| (i, i + 1, i as f32 * 8.0, 8.0))
        .collect();
    assert_eq!(default_by(&[row(&long, &glyphs, 0.0)]), By::Words);
}

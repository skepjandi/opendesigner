#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{
    Actor, CaptionPosition, CaptionPreset, CaptionStyle, Cue, Document, NewNode, NodeId, NodeKind,
    Operation, Paint, Session, VideoProperty, Word,
};
use od_render::Pixmap;
use od_render::video::VideoRenderer;

const W: u32 = 300;
const H: u32 = 400;

/// A 4 s video of one `fill` frame with the cue "alpha beta gamma" from 1 s to 4 s, each word 1 s.
fn video(fill: &str, style: CaptionStyle) -> (Session, NodeId) {
    let mut s = Session::new(Document::new("v"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut node = NewNode::new(NodeKind::Frame);
    (node.width, node.height) = (f64::from(W), f64::from(H));
    node.fill = Paint::parse(fill).ok();
    let frame = op(
        &mut s,
        Operation::Create {
            parent: root,
            index: None,
            node,
        },
    )
    .created[0]
        .clone();
    let v = op(
        &mut s,
        Operation::CreateVideo {
            name: "v".into(),
            fps: 10,
            width: W,
            height: H,
        },
    )
    .created[0]
        .clone();
    op(
        &mut s,
        Operation::AddClip {
            video: v.clone(),
            frame: Some(frame),
            footage: None,
            duration_ms: 4000,
            index: None,
            transition: None,
            motion: None,
        },
    );
    let words = ["alpha", "beta", "gamma"]
        .iter()
        .enumerate()
        .map(|(n, t)| Word {
            text: (*t).into(),
            start_ms: 1000 + 1000 * n as u32,
            end_ms: 2000 + 1000 * n as u32,
        })
        .collect();
    let cue = Cue {
        id: NodeId::parse("q1").unwrap(),
        start_ms: 1000,
        end_ms: 4000,
        text: "alpha beta gamma".into(),
        words,
    };
    let captions = {
        let video = s.document().video(&v).unwrap();
        let mut c = video.captions_with_cues(vec![cue]).unwrap();
        c.style = style;
        c
    };
    op(
        &mut s,
        Operation::SetVideo {
            id: v.clone(),
            prop: VideoProperty::Captions(captions),
        },
    );
    (s, v)
}

fn op(s: &mut Session, o: Operation) -> od_core::TxReceipt {
    s.apply(Actor::User, None, &[o]).unwrap()
}

fn frame(s: &Session, v: &NodeId, ms: u64) -> Pixmap {
    let doc = s.document();
    VideoRenderer::new(doc, doc.video(v).unwrap(), None)
        .frame_at(ms)
        .unwrap()
}

fn rgb(p: &Pixmap, x: u32, y: u32) -> [u8; 3] {
    let c = p.pixel(x, y).unwrap();
    [c.red(), c.green(), c.blue()]
}

/// Rows `y0..y1` of the picture as bytes, to compare regions.
fn rows(p: &Pixmap, y0: u32, y1: u32) -> Vec<u8> {
    p.data()[(y0 * W * 4) as usize..(y1 * W * 4) as usize].to_vec()
}

fn count(p: &Pixmap, near: [u8; 3]) -> usize {
    (0..H)
        .flat_map(|y| (0..W).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let c = rgb(p, x, y);
            c.iter().zip(near).all(|(a, b)| a.abs_diff(b) < 40)
        })
        .count()
}

/// The first and last row holding a pixel that is not the plain background.
fn ink_rows(p: &Pixmap, background: [u8; 3]) -> Option<(u32, u32)> {
    let busy: Vec<u32> = (0..H)
        .filter(|&y| (0..W).any(|x| rgb(p, x, y) != background))
        .collect();
    Some((*busy.first()?, *busy.last()?))
}

#[test]
fn nothing_is_drawn_outside_a_cue_and_the_caption_sits_in_the_lower_part() {
    let (s, v) = video("#000000", CaptionStyle::default());
    assert_eq!(ink_rows(&frame(&s, &v, 500), [0, 0, 0]), None);
    let (top, bottom) = ink_rows(&frame(&s, &v, 1500), [0, 0, 0]).unwrap();
    assert!(top > H / 2 && bottom < H, "{top}..{bottom}");
}

#[test]
fn the_active_word_changes_colour_as_the_time_moves_between_words() {
    let style = CaptionStyle::preset(CaptionPreset::Karaoke);
    let (s, v) = video(
        "#000000",
        CaptionStyle {
            position: CaptionPosition::Bottom,
            ..style
        },
    );
    let yellow = [0x35, 0xe0, 0xa1];
    let (first, third) = (frame(&s, &v, 1500), frame(&s, &v, 3500));
    let lit_first = count(&first, yellow);
    let lit_third = count(&third, yellow);
    assert!(lit_first > 30 && lit_third > 30, "{lit_first} {lit_third}");
    assert_ne!(
        first.data(),
        third.data(),
        "the picture moves with the words"
    );

    let (top, bottom) = ink_rows(&first, [0, 0, 0]).unwrap();
    assert_ne!(rows(&first, top, bottom + 1), rows(&third, top, bottom + 1));

    let leftmost_lit = |p: &Pixmap| {
        (0..W)
            .find(|&x| {
                (0..H).any(|y| {
                    rgb(p, x, y)
                        .iter()
                        .zip(yellow)
                        .all(|(a, b)| a.abs_diff(b) < 40)
                })
            })
            .unwrap()
    };
    assert!(
        leftmost_lit(&first) + 40 < leftmost_lit(&third),
        "the highlight moved right"
    );
}

#[test]
fn without_a_highlight_every_word_keeps_one_colour_and_the_signature_holds_still() {
    let (s, v) = video("#000000", CaptionStyle::default());
    let doc = s.document();
    let renderer = VideoRenderer::new(doc, doc.video(&v).unwrap(), None);
    assert_eq!(renderer.signature(1500), renderer.signature(3500));
    assert_ne!(renderer.signature(500), renderer.signature(1500));
    assert_eq!(frame(&s, &v, 1500).data(), frame(&s, &v, 3500).data());

    let (s, v) = video("#000000", CaptionStyle::preset(CaptionPreset::BoldPop));
    let doc = s.document();
    let renderer = VideoRenderer::new(doc, doc.video(&v).unwrap(), None);
    assert_ne!(renderer.signature(1500), renderer.signature(3500));
}

#[test]
fn a_boxed_style_darkens_the_picture_behind_the_lines() {
    let (plain, v) = video(
        "#ffffff",
        CaptionStyle {
            color: "#ffffff".into(),
            ..CaptionStyle::default()
        },
    );
    let (boxed, w) = video("#ffffff", CaptionStyle::preset(CaptionPreset::Boxed));
    let (a, b) = (frame(&plain, &v, 1500), frame(&boxed, &w, 1500));
    let (top, bottom) = ink_rows(&b, [255, 255, 255]).unwrap();
    let y = (top + bottom) / 2;
    let dark = (0..W).filter(|&x| rgb(&b, x, y)[0] < 140).count();
    assert!(dark > 60, "a dark box under the text: {dark}");
    assert_eq!(
        ink_rows(&a, [255, 255, 255]),
        None,
        "white text on white shows nothing"
    );
}

#[test]
fn a_long_cue_is_wrapped_to_the_lines_the_style_allows() {
    let long =
        "the quick brown fox jumps over the lazy dog and keeps running far away from home today";
    let (mut s, v) = video(
        "#000000",
        CaptionStyle {
            size_pct: 6.0,
            max_lines: 2,
            ..CaptionStyle::default()
        },
    );
    let cue = Cue {
        id: NodeId::parse("q1").unwrap(),
        start_ms: 1000,
        end_ms: 4000,
        text: long.into(),
        words: Vec::new(),
    };
    let captions = {
        let video = s.document().video(&v).unwrap();
        let mut c = video.captions_with_cues(vec![cue]).unwrap();
        c.style = video.captions.style.clone();
        c
    };
    op(
        &mut s,
        Operation::SetVideo {
            id: v.clone(),
            prop: VideoProperty::Captions(captions),
        },
    );
    let p = frame(&s, &v, 2000);
    let (top, bottom) = ink_rows(&p, [0, 0, 0]).unwrap();
    let line = 6.0 / 100.0 * H as f32 * 1.25;
    assert!(
        (bottom - top) as f32 <= line * 2.0 + 2.0,
        "{top}..{bottom} for two lines of {line}"
    );
    let left = (0..W)
        .find(|&x| (0..H).any(|y| rgb(&p, x, y) != [0, 0, 0]))
        .unwrap();
    assert!(
        left as f32 >= W as f32 * 0.03,
        "text keeps off the left edge: {left}"
    );
}

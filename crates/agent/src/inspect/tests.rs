#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use od_core::{Actor, Document, Operation, Session};

const W: u32 = 400;
const H: u32 = 300;

fn apply(s: &mut Session, ops: Value) -> od_core::TxReceipt {
    let ops: Vec<Operation> = serde_json::from_value(ops).unwrap();
    s.apply(Actor::User, None, &ops).unwrap()
}

/// A black 400 x 300 scene with a red 50 x 50 square at (100, 100) that slides right to x 200
/// over the first second of a two second clip, at 10 frames a second.
struct Scene {
    session: Session,
    video: String,
    square: String,
    frame: String,
}

fn scene() -> Scene {
    let mut s = Session::new(Document::new("inspect"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    apply(
        &mut s,
        json!([{"op": "create_video", "name": "V", "fps": 10, "width": W, "height": H}]),
    );
    let video = s.document().videos[0].id.to_string();
    let frame = apply(
        &mut s,
        json!([{"op": "create", "parent": root.as_str(), "node":
            {"kind": "frame", "name": "scene", "width": W, "height": H, "fill": "#000000"}}]),
    )
    .created[0]
        .to_string();
    let square = apply(
        &mut s,
        json!([{"op": "create", "parent": frame, "node":
            {"kind": "rect", "name": "Square", "x": 100, "y": 100, "width": 50, "height": 50,
             "fill": "#ff0000"}}]),
    )
    .created[0]
        .to_string();
    apply(
        &mut s,
        json!([
            {"op": "add_clip", "video": video, "frame": frame, "duration_ms": 2000},
            {"op": "animate", "video": video, "node": square, "prop": "x", "keys": [
                {"at_ms": 0, "value": 100.0, "ease": "linear"},
                {"at_ms": 1000, "value": 200.0, "ease": "linear"}]}
        ]),
    );
    Scene {
        session: s,
        video,
        square,
        frame,
    }
}

fn source(s: &Scene) -> Source {
    Source::of_document(s.session.document().clone())
}

fn pixels(png_bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    (info.width, info.height, buf)
}

fn at(rgb: &(u32, u32, Vec<u8>), x: u32, y: u32) -> [u8; 3] {
    let i = ((y * rgb.0 + x) * 3) as usize;
    [rgb.2[i], rgb.2[i + 1], rgb.2[i + 2]]
}

fn is_red(p: [u8; 3]) -> bool {
    p[0] > 240 && p[1] < 15 && p[2] < 15
}

fn error(r: Result<Picture, ToolError>) -> String {
    match r {
        Ok(_) => panic!("expected an error"),
        Err(e) => e.0,
    }
}

#[test]
fn a_strip_shows_every_frame_in_a_labeled_grid() {
    let s = scene();
    let pic = run(
        &source(&s),
        "video_strip",
        json!({"video": s.video, "from_ms": 0, "to_ms": 500, "cell_side": 100}),
    )
    .unwrap();
    // 6 frames at 10 fps, 100 x 75 each, 6 to a row, 6 px gaps
    assert_eq!((pic.width, pic.height), (6 * 100 + 7 * 6, 75 + 2 * 6));
    assert!(pic.note.contains("6 frames"), "{}", pic.note);
    let rgb = pixels(&pic.png);
    // the square sits at x 25 to 37 (of 100) in the first cell, and has moved on by the last
    assert!(is_red(at(&rgb, 6 + 31, 6 + 31)));
    assert!(!is_red(at(&rgb, 6 + 5 * 106 + 31, 6 + 31)));
    assert!(is_red(at(&rgb, 6 + 5 * 106 + 44, 6 + 31)));
    // the time label is drawn in the corner of the cell: some white pixels, none in the next cell's body
    let white = (0..30)
        .flat_map(|y| (0..40).map(move |x| (x, y)))
        .filter(|(x, y)| at(&rgb, 6 + x, 6 + y) == [255, 255, 255])
        .count();
    assert!(white > 5, "{white}");
}

#[test]
fn every_nth_frame_is_taken_and_the_end_is_checked() {
    let s = scene();
    let src = source(&s);
    let every2 = run(
        &src,
        "video_strip",
        json!({"video": s.video, "from_ms": 0, "to_ms": 5000 - 1, "every": 2, "cell_side": 64}),
    );
    // 2000 ms clip: the end is past it
    assert!(error(every2).contains("past the end"));
    let ok = run(
        &src,
        "video_strip",
        json!({"video": s.video, "from_ms": 0, "to_ms": 1990, "every": 2, "cell_side": 64}),
    )
    .unwrap();
    assert!(ok.note.contains("10 frames"), "{}", ok.note);
    let all = run(
        &src,
        "video_strip",
        json!({"video": s.video, "from_ms": 0, "to_ms": 1990, "cell_side": 64}),
    )
    .unwrap();
    assert!(
        all.note.contains("20 frames") && all.note.contains("every 1 frame "),
        "{}",
        all.note
    );
}

#[test]
fn a_strip_of_more_than_36_frames_is_refused_with_the_every_that_fits() {
    let mut s = scene();
    apply(
        &mut s.session,
        json!([{"op": "add_clip", "video": s.video, "frame": s.frame, "duration_ms": 6000}]),
    );
    let src = source(&s);
    let message = error(run(
        &src,
        "video_strip",
        json!({"video": s.video, "from_ms": 0, "to_ms": 7000}),
    ));
    assert!(
        message.contains("71 frames") && message.contains("every to 2 or more"),
        "{message}"
    );
    let fine = run(
        &src,
        "video_strip",
        json!({"video": s.video, "from_ms": 0, "to_ms": 7000, "every": 2, "cell_side": 64}),
    )
    .unwrap();
    assert!(fine.note.contains("36 frames"), "{}", fine.note);
    let backwards = error(run(
        &src,
        "video_strip",
        json!({"video": s.video, "from_ms": 900, "to_ms": 100}),
    ));
    assert!(backwards.contains("after"), "{backwards}");
}

#[test]
fn a_region_crop_is_full_resolution_and_can_be_enlarged() {
    let s = scene();
    let src = source(&s);
    let region =
        json!({"video": s.video, "time_ms": 0, "x": 100, "y": 100, "width": 50, "height": 50});
    let pic = run(&src, "video_crop", region).unwrap();
    assert_eq!((pic.width, pic.height), (50, 50));
    let rgb = pixels(&pic.png);
    assert!(is_red(at(&rgb, 0, 0)) && is_red(at(&rgb, 49, 49)));
    let big = run(
        &src,
        "video_crop",
        json!({"video": s.video, "time_ms": 0, "x": 90, "y": 90, "width": 20, "height": 20, "zoom": 3}),
    )
    .unwrap();
    assert_eq!((big.width, big.height), (60, 60));
    let rgb = pixels(&big.png);
    // 10 source pixels of black, then red: 30 enlarged pixels
    assert!(!is_red(at(&rgb, 29, 29)) && is_red(at(&rgb, 30, 30)));
}

#[test]
fn crop_errors_say_what_to_change() {
    let s = scene();
    let src = source(&s);
    let call = |args: Value| error(run(&src, "video_crop", args));
    let v = &s.video;
    assert!(call(json!({"video": v, "time_ms": 0})).contains("x, y, width and height, or a node"));
    assert!(
        call(json!({"video": v, "time_ms": 0, "x": 500, "y": 0, "width": 10, "height": 10}))
            .contains("outside")
    );
    assert!(
        call(json!({"video": v, "time_ms": 99999, "x": 0, "y": 0, "width": 4, "height": 4}))
            .contains("past the end")
    );
    assert!(call(json!({"video": v, "time_ms": 0, "node": s.square, "x": 1, "y": 1, "width": 4, "height": 4})).contains("not both"));
    assert!(call(json!({"video": v, "time_ms": 0, "node": "n_nope"})).contains("not on screen"));
    assert!(
        call(json!({"video": "v_nope", "time_ms": 0, "x": 0, "y": 0, "width": 4, "height": 4}))
            .contains("does not exist")
    );
}

#[test]
fn a_crop_over_2048_pixels_on_a_side_is_refused() {
    let mut s = scene();
    apply(
        &mut s.session,
        json!([{"op": "create_video", "name": "Big", "fps": 10, "width": 1280, "height": 720}]),
    );
    let big = s.session.document().videos[1].id.to_string();
    apply(
        &mut s.session,
        json!([{"op": "add_clip", "video": big, "frame": s.frame, "duration_ms": 1000}]),
    );
    let src = source(&s);
    let args = |zoom: u32| json!({"video": big, "time_ms": 0, "x": 0, "y": 0, "width": 600, "height": 400, "zoom": zoom});
    assert!(error(run(&src, "video_crop", args(4))).contains("smaller region or a lower zoom"));
    assert_eq!(run(&src, "video_crop", args(3)).unwrap().width, 1800);
}

#[test]
fn a_node_crop_follows_the_node_as_it_moves() {
    let s = scene();
    let src = source(&s);
    let crop = |t: u64, padding: u32| {
        run(
            &src,
            "video_crop",
            json!({"video": s.video, "time_ms": t, "node": s.square, "padding": padding}),
        )
        .unwrap()
    };
    // at 500 ms the square has slid from 100 to 150: no padding is just the square
    let tight = crop(500, 0);
    assert_eq!((tight.width, tight.height), (50, 50));
    assert!(
        pixels(&tight.png)
            .2
            .chunks(3)
            .all(|p| is_red([p[0], p[1], p[2]]))
    );
    assert!(tight.note.contains("150,100"), "{}", tight.note);
    // with padding the square sits in the middle of the crop
    let padded = pixels(&crop(500, 10).png);
    assert_eq!((padded.0, padded.1), (70, 70));
    assert!(is_red(at(&padded, 35, 35)) && !is_red(at(&padded, 3, 3)));
    // after the slide it stays at 200
    assert!(crop(1500, 0).note.contains("200,100"));
}

#[test]
fn a_node_crop_follows_the_node_through_the_camera() {
    let mut s = scene();
    apply(
        &mut s.session,
        json!([
            {"op": "animate", "video": s.video, "node": s.video, "prop": "camera_zoom",
             "keys": [{"at_ms": 0, "value": 2.0}, {"at_ms": 100, "value": 2.0}]},
            {"op": "animate", "video": s.video, "node": s.video, "prop": "camera_x",
             "keys": [{"at_ms": 0, "value": 20.0}, {"at_ms": 100, "value": 20.0}]}
        ]),
    );
    let src = source(&s);
    // zoom 2 about the center (200, 150) after panning the scene 20 left: the square at
    // (100, 100) 50 x 50 lands on x 2*(100-20-200)+200 = -40 .. 60 (cut to 0), y 50 .. 150
    let rect = node_rect(
        &src.doc,
        src.doc.video(&node_id(&s.video).unwrap()).unwrap(),
        &node_id(&s.square).unwrap(),
        0,
    )
    .unwrap();
    assert!(
        (rect.x - -40.0).abs() < 1e-6 && (rect.width - 100.0).abs() < 1e-6,
        "{rect:?}"
    );
    assert!(
        (rect.y - 50.0).abs() < 1e-6 && (rect.height - 100.0).abs() < 1e-6,
        "{rect:?}"
    );
    let pic = run(
        &src,
        "video_crop",
        json!({"video": s.video, "time_ms": 0, "node": s.square, "padding": 0}),
    )
    .unwrap();
    // the part of the box inside the picture: 60 x 100, all red
    assert_eq!((pic.width, pic.height), (60, 100));
    assert!(
        pixels(&pic.png)
            .2
            .chunks(3)
            .all(|p| is_red([p[0], p[1], p[2]]))
    );
}

#[test]
fn a_hidden_node_has_no_place_on_screen() {
    let mut s = scene();
    apply(
        &mut s.session,
        json!([{"op": "set", "id": s.square, "prop": {"name": "visible", "value": false}}]),
    );
    let message = error(run(
        &source(&s),
        "video_crop",
        json!({"video": s.video, "time_ms": 0, "node": s.square}),
    ));
    assert!(message.contains("not on screen"), "{message}");
}

#[test]
fn the_grid_draws_labeled_lines_at_round_distances() {
    assert_eq!(auto_spacing(400), 50);
    assert_eq!(auto_spacing(1920), 200);
    assert_eq!(auto_spacing(1080), 100);
    let s = scene();
    let src = source(&s);
    let pic = run(&src, "frame_grid", json!({"video": s.video, "time_ms": 0})).unwrap();
    assert_eq!((pic.width, pic.height), (W, H));
    assert!(pic.note.contains("every 50 video pixels"), "{}", pic.note);
    let rgb = pixels(&pic.png);
    // lines at x 50 and y 250 (white at 55% over black), black between, the square untouched
    assert!(at(&rgb, 50, 280)[0] > 120 && at(&rgb, 75, 280) == [0, 0, 0]);
    assert!(at(&rgb, 75, 250)[0] > 120);
    assert!(is_red(at(&rgb, 125, 125)));
    // numbers are drawn beside the lines on the top edge
    let labeled = (0..12)
        .flat_map(|y| (53..80).map(move |x| (x, y)))
        .any(|(x, y)| at(&rgb, x, y) == [255, 255, 255]);
    assert!(labeled);
    let coarse = run(
        &src,
        "frame_grid",
        json!({"video": s.video, "time_ms": 0, "spacing": 100}),
    )
    .unwrap();
    let rgb = pixels(&coarse.png);
    assert_eq!(at(&rgb, 50, 280), [0, 0, 0]);
    assert!(at(&rgb, 100, 280)[0] > 120);
    // a smaller picture still reads in video pixels
    let small = run(
        &src,
        "frame_grid",
        json!({"video": s.video, "time_ms": 0, "max_side": 256}),
    )
    .unwrap();
    assert_eq!((small.width, small.height), (256, 192));
    assert!(small.note.contains("400x300 in all"));
}

#[test]
fn a_diff_highlights_what_changed_between_two_times() {
    let s = scene();
    let src = source(&s);
    let overlay = run(
        &src,
        "video_diff",
        json!({"video": s.video, "time_ms": 0, "other_time_ms": 1000, "mode": "overlay", "max_side": 400}),
    )
    .unwrap();
    assert_eq!((overlay.width, overlay.height), (W, H));
    let rgb = pixels(&overlay.png);
    // where the square was, and where it went: changed; everywhere else: dimmed black
    assert_eq!(at(&rgb, 125, 125), [255, 0, 170]);
    assert_eq!(at(&rgb, 225, 125), [255, 0, 170]);
    assert!(at(&rgb, 10, 10)[0] < 40);
    assert!(
        overlay.note.contains("changes lie in 100,100 150x50"),
        "{}",
        overlay.note
    );
    assert!(overlay.note.contains("4.17%"), "{}", overlay.note);
    let sheet = run(
        &src,
        "video_diff",
        json!({"video": s.video, "time_ms": 0, "other_time_ms": 1000, "max_side": 200}),
    )
    .unwrap();
    assert_eq!((sheet.width, sheet.height), (3 * 200 + 4 * 6, 150 + 12));
    let rgb = pixels(&sheet.png);
    // first the picture at 0 ms (square at 50), then the one at 1000 ms (square at 100)
    assert!(is_red(at(&rgb, 6 + 55, 6 + 55)));
    assert!(is_red(at(&rgb, 6 + 206 + 105, 6 + 55)));
    assert!(!is_red(at(&rgb, 6 + 206 + 55, 6 + 55)));
}

#[test]
fn a_diff_of_two_videos_needs_two_pictures_of_one_size() {
    let mut s = scene();
    let src = source(&s);
    let same = error(run(
        &src,
        "video_diff",
        json!({"video": s.video, "time_ms": 100}),
    ));
    assert!(same.contains("same picture"), "{same}");

    apply(
        &mut s.session,
        json!([{"op": "create_video", "name": "Twin", "fps": 10, "width": W, "height": H},]),
    );
    let twin = s.session.document().videos[1].id.to_string();
    apply(
        &mut s.session,
        json!([{"op": "add_clip", "video": twin, "frame": s.frame, "duration_ms": 2000}]),
    );
    // the twin does not animate the square: at 1000 ms it is at 100, in the first video at 200
    let src = source(&s);
    let pic = run(
        &src,
        "video_diff",
        json!({"video": s.video, "time_ms": 1000, "other_video": twin, "mode": "overlay", "max_side": 400}),
    )
    .unwrap();
    assert!(
        pic.note.contains("changes lie in 100,100 150x50"),
        "{}",
        pic.note
    );
    let still = run(
        &src,
        "video_diff",
        json!({"video": s.video, "time_ms": 0, "other_video": twin, "max_side": 100}),
    )
    .unwrap();
    assert!(
        still.note.contains("nothing changed") && still.note.contains("0.00%"),
        "{}",
        still.note
    );

    apply(
        &mut s.session,
        json!([{"op": "create_video", "name": "Small", "fps": 10, "width": 200, "height": 100}]),
    );
    let small = s.session.document().videos[2].id.to_string();
    apply(
        &mut s.session,
        json!([{"op": "add_clip", "video": small, "frame": s.frame, "duration_ms": 1000}]),
    );
    let message = error(run(
        &source(&s),
        "video_diff",
        json!({"video": s.video, "time_ms": 0, "other_video": small}),
    ));
    assert!(message.contains("one size"), "{message}");
}

#[test]
fn the_tools_are_listed_read_only_with_closed_schemas() {
    let defs = definitions();
    let names: Vec<&str> = defs.iter().filter_map(|d| d["name"].as_str()).collect();
    assert_eq!(names, NAMES);
    for d in &defs {
        assert_eq!(d["annotations"]["readOnlyHint"], json!(true));
        assert_eq!(d["inputSchema"]["additionalProperties"], json!(false));
    }
    let listed = crate::tools::definitions();
    for name in NAMES {
        assert!(listed.iter().any(|d| d["name"] == name), "{name}");
    }
    assert!(run(&source(&scene()), "video_nothing", json!({})).is_err());
    let unknown = run(
        &source(&scene()),
        "frame_grid",
        json!({"video": "v_1", "time_ms": 0, "extra": 1}),
    );
    assert!(unknown.is_err());
}

#[test]
fn exports_take_plain_names_and_refuse_links() {
    let dir = tempfile::tempdir().unwrap();
    let written = write_export(dir.path(), "strip-1.png", b"png").unwrap();
    assert_eq!(written, "exports/strip-1.png");
    assert_eq!(
        std::fs::read(dir.path().join("exports/strip-1.png")).unwrap(),
        b"png"
    );
    for bad in ["../x.png", "a/b.png", ".hidden.png", "x.jpg", "x y.png"] {
        assert!(write_export(dir.path(), bad, b"png").is_err(), "{bad}");
    }
    let linked = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), linked.path().join("exports")).unwrap();
    assert!(write_export(linked.path(), "x.png", b"png").is_err());
    assert!(
        std::fs::read_dir(elsewhere.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn drawing_helpers_cut_enlarge_and_compare_exactly() {
    let mut a = Pixmap::new(4, 3).unwrap();
    a.data_mut()
        .chunks_mut(4)
        .for_each(|p| p.copy_from_slice(&[10, 20, 30, 255]));
    let mut b = a.clone();
    b.data_mut()[(2 * 4 + 1) * 4] = 200;
    let (marked, changes) = draw::highlight(&a, &b, 12).unwrap();
    assert_eq!((changes.changed, changes.total), (1, 12));
    assert_eq!(
        changes.bounds,
        Some(PixelRect {
            x: 1,
            y: 2,
            width: 1,
            height: 1
        })
    );
    assert_eq!(
        &marked.data()[(2 * 4 + 1) * 4..(2 * 4 + 1) * 4 + 3],
        &[255, 0, 170]
    );
    // under the threshold nothing counts
    assert_eq!(draw::highlight(&a, &b, 250).unwrap().1.changed, 0);
    assert!(draw::highlight(&a, &Pixmap::new(5, 3).unwrap(), 12).is_none());

    let piece = draw::crop(
        &b,
        PixelRect {
            x: 1,
            y: 2,
            width: 2,
            height: 1,
        },
    )
    .unwrap();
    assert_eq!(
        (piece.width(), piece.height(), piece.data()[0]),
        (2, 1, 200)
    );
    assert!(
        draw::crop(
            &b,
            PixelRect {
                x: 3,
                y: 0,
                width: 2,
                height: 1
            }
        )
        .is_none()
    );
    assert!(
        draw::crop(
            &b,
            PixelRect {
                x: 0,
                y: 0,
                width: 0,
                height: 1
            }
        )
        .is_none()
    );
    let big = draw::enlarge(&piece, 3).unwrap();
    assert_eq!((big.width(), big.height()), (6, 3));
    assert_eq!(big.data()[2 * 4], 200);
    assert_eq!(big.data()[3 * 4], 10);
}

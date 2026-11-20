//! Tests for the 3D devices: golden pictures of the five models and the properties a device
//! must keep whatever its pose (the screen stays square-on, the picture is the same however many
//! threads drew it, a spin never touches the edge of the viewport).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{Device3d, DeviceModel, Rgba};
use od_render::three_d::{
    Scene, render_device, render_device_with_threads, screen_center_px, screen_texture_size,
};

const GOLDEN_DIR: &str = "tests/golden";
/// How far a golden pixel may differ from the rendered one, per channel.
const TOLERANCE: u8 = 2;

/// A picture with flat bands and a mark in the middle, so a wrong or shifted screen shows up.
fn screen_picture(w: u32, h: u32) -> tiny_skia::Pixmap {
    let mut pix = tiny_skia::Pixmap::new(w, h).expect("screen picture");
    let data = pix.data_mut();
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 4;
            let band = (y * 8 / h as usize) % 2 == 0;
            let centre = x.abs_diff(w as usize / 2) < w as usize / 12
                && y.abs_diff(h as usize / 2) < h as usize / 12;
            let (r, g, b) = if centre {
                (255, 255, 255)
            } else if band {
                (220, 40, 30)
            } else {
                (20, 60, 200)
            };
            data[i] = r;
            data[i + 1] = g;
            data[i + 2] = b;
            data[i + 3] = 255;
        }
    }
    pix
}

fn device(model: DeviceModel) -> Device3d {
    Device3d {
        model,
        rotate_x: 8.0,
        rotate_y: -22.0,
        ..Device3d::default()
    }
}

fn body() -> Rgba {
    Rgba {
        r: 58,
        g: 61,
        b: 68,
        a: 255,
    }
}

fn render(d: &Device3d, w: u32, h: u32) -> tiny_skia::Pixmap {
    let (tw, th) = screen_texture_size(d, w, h);
    let screen = screen_picture(tw, th);
    let scene = Scene {
        device: d,
        body: body(),
        screen: Some(&screen),
        width: w,
        height: h,
    };
    render_device(&scene).expect("device")
}

fn alpha(pix: &tiny_skia::Pixmap, x: u32, y: u32) -> u8 {
    let i = ((y * pix.width() + x) * 4 + 3) as usize;
    pix.data()[i]
}

fn rgb(pix: &tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * pix.width() + x) * 4) as usize;
    [pix.data()[i], pix.data()[i + 1], pix.data()[i + 2]]
}

/// The golden pictures are regenerated with `OD_UPDATE_GOLDEN=1 cargo test -p opendesigner-render`.
fn check_golden(model: DeviceModel) {
    let d = device(model);
    let (w, h) = (256u32, 384u32);
    let got = render(&d, w, h);
    let path = format!("{GOLDEN_DIR}/{}.png", model.as_str());
    if std::env::var("OD_UPDATE_GOLDEN").is_ok() {
        let img = image::RgbaImage::from_raw(w, h, got.data().to_vec()).expect("png");
        std::fs::create_dir_all(GOLDEN_DIR).expect("golden dir");
        img.save(&path).expect("write golden");
        return;
    }
    let want =
        image::open(&path).expect("golden picture (run with OD_UPDATE_GOLDEN=1 to write it)");
    let want = want.to_rgba8();
    assert_eq!((want.width(), want.height()), (w, h), "golden size");
    let mut worst = 0i32;
    let mut at = (0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let g = rgb(&got, x, y);
            let wv = want.get_pixel(x, y).0;
            let diff = (0..3)
                .map(|c| (i32::from(g[c]) - i32::from(wv[c])).abs())
                .max()
                .unwrap_or(0);
            let a = (i32::from(alpha(&got, x, y)) - i32::from(wv[3])).abs();
            let diff = diff.max(a);
            if diff > worst {
                worst = diff;
                at = (x, y);
            }
        }
    }
    assert!(
        worst <= i32::from(TOLERANCE),
        "{} differs by {worst} at {at:?}",
        model.as_str()
    );
}

#[test]
fn golden_pictures_match() {
    for m in DeviceModel::ALL {
        check_golden(m);
    }
}

#[test]
fn the_screen_centre_is_the_picture_centre_when_the_device_faces_the_viewer() {
    let d = Device3d {
        model: DeviceModel::Phone,
        fov: 3.0,
        ..Device3d::default()
    };
    let (w, h) = (256u32, 384u32);
    let (tw, th) = screen_texture_size(&d, w, h);
    let screen = screen_picture(tw, th);
    let scene = Scene {
        device: &d,
        body: body(),
        screen: Some(&screen),
        width: w,
        height: h,
    };
    let got = render_device(&scene).expect("device");
    let (cx, cy) = screen_center_px(&d, w, h).expect("screen centre");
    let x = cx.round().clamp(0.0, (w - 1) as f32) as u32;
    let y = cy.round().clamp(0.0, (h - 1) as f32) as u32;
    let pixel = rgb(&got, x, y);
    // The middle of the picture is a white mark: the screen shows it where it should.
    let middle = rgb(&screen, tw / 2, th / 2);
    for c in 0..3 {
        assert!(
            (i32::from(pixel[c]) - i32::from(middle[c])).abs() <= 12,
            "screen centre {x},{y} is {pixel:?}, not the picture's {middle:?}"
        );
    }
}

#[test]
fn a_device_without_a_picture_has_a_dark_screen() {
    let mut d = Device3d {
        model: DeviceModel::Phone,
        ..Device3d::default()
    };
    d.rotate_y = -22.0;
    let (w, h) = (192u32, 320u32);
    let scene = Scene {
        device: &d,
        body: body(),
        screen: None,
        width: w,
        height: h,
    };
    let got = render_device(&scene).expect("device");
    let (cx, cy) = screen_center_px(&d, w, h).expect("screen centre");
    let x = cx.round().clamp(0.0, (w - 1) as f32) as u32;
    let y = cy.round().clamp(0.0, (h - 1) as f32) as u32;
    let pixel = rgb(&got, x, y);
    assert!(
        pixel[0] < 120 && pixel[1] < 120 && pixel[2] < 130,
        "a switched-off screen is {pixel:?}, not dark"
    );
}

#[test]
fn a_device_facing_the_viewer_is_left_right_symmetric() {
    let d = Device3d {
        model: DeviceModel::Phone,
        shadow: 0.0,
        ..Device3d::default()
    };
    let (w, h) = (192u32, 320u32);
    let got = render(&d, w, h);
    // How much of the device each column holds: the same on the left and the right.
    let column = |x: u32| -> u64 { (0..h).map(|y| u64::from(alpha(&got, x, y))).sum() };
    let total: u64 = (0..w).map(column).sum();
    for x in 0..w / 2 {
        let (a, b) = (column(x), column(w - 1 - x));
        let diff = a.abs_diff(b);
        assert!(
            diff * 100 <= total.max(1),
            "the outline is not symmetric at column {x}: {a} vs {b}"
        );
    }
    // And the outline itself reaches as far on one side as on the other.
    let edge = |x: u32| (0..h).any(|y| alpha(&got, x, y) > 0);
    let left = (0..w).find(|&x| edge(x)).expect("a left edge");
    let right = (0..w).rev().find(|&x| edge(x)).expect("a right edge");
    assert!(
        left.abs_diff(w - 1 - right) <= 1,
        "the outline runs from {left} to {right}, not from one side to the other"
    );
}

#[test]
fn a_device_turned_around_shows_no_screen() {
    let d = Device3d {
        model: DeviceModel::Phone,
        rotate_y: 180.0,
        ..Device3d::default()
    };
    let (w, h) = (192u32, 320u32);
    let got = render(&d, w, h);
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = rgb(&got, x, y);
            let screenish = r > 180 && b > 180 && g < 110;
            assert!(!screenish, "a screen pixel at {x},{y}: {r},{g},{b}");
        }
    }
}

#[test]
fn the_corners_are_clear() {
    let d = device(DeviceModel::Phone);
    let (w, h) = (192u32, 320u32);
    let got = render(&d, w, h);
    for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
        assert_eq!(alpha(&got, x, y), 0, "corner {x},{y} is not clear");
    }
}

#[test]
fn the_shadow_only_falls_outside_the_device() {
    let mut d = device(DeviceModel::Phone);
    d.shadow = 0.0;
    let (w, h) = (192u32, 320u32);
    let plain = render(&d, w, h);
    d.shadow = 0.35;
    let shadowed = render(&d, w, h);
    let mut changed = 0;
    for y in 0..h {
        for x in 0..w {
            let same = rgb(&plain, x, y) == rgb(&shadowed, x, y)
                && alpha(&plain, x, y) == alpha(&shadowed, x, y);
            if same {
                continue;
            }
            changed += 1;
            assert!(
                alpha(&plain, x, y) < 255,
                "the shadow changed the device itself at {x},{y}"
            );
        }
    }
    assert!(
        changed > 100,
        "the shadow changed nothing ({changed} pixels)"
    );
}

#[test]
fn the_picture_does_not_depend_on_the_thread_count() {
    let d = device(DeviceModel::Laptop);
    let (w, h) = (192u32, 320u32);
    let (tw, th) = screen_texture_size(&d, w, h);
    let screen = screen_picture(tw, th);
    let one = render_device_with_threads(
        &Scene {
            device: &d,
            body: body(),
            screen: Some(&screen),
            width: w,
            height: h,
        },
        1,
    )
    .expect("device");
    let many = render_device_with_threads(
        &Scene {
            device: &d,
            body: body(),
            screen: Some(&screen),
            width: w,
            height: h,
        },
        7,
    )
    .expect("device");
    assert_eq!(
        one.data(),
        many.data(),
        "the thread count changed the picture"
    );
    let again = render(&d, w, h);
    assert_eq!(one.data(), again.data(), "the picture is not reproducible");
}

/// The device fills its box: the silhouette reaches nearly the whole of the smaller side.
#[test]
fn the_device_fills_its_viewport() {
    let d = Device3d {
        model: DeviceModel::Phone,
        rotate_x: 8.0,
        rotate_y: -18.0,
        ..Device3d::default()
    };
    let (w, h) = (420u32, 913u32);
    let got = render(&d, w, h);
    let (mut x0, mut x1, mut y0, mut y1) = (w, 0u32, h, 0u32);
    let mut area = 0u64;
    for y in 0..h {
        for x in 0..w {
            if alpha(&got, x, y) > 0 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
                area += 1;
            }
        }
    }
    let tall = (y1 - y0 + 1) as f64 / f64::from(h);
    let wide = (x1 - x0 + 1) as f64 / f64::from(w);
    println!(
        "the device covers {tall:.3} of the height and {wide:.3} of the width ({:.3} of the area)",
        area as f64 / f64::from(w * h)
    );
    assert!(tall > 0.85, "the device only fills {tall} of the viewport");
    assert!(wide > 0.85, "the device only fills {wide} of the viewport");
    assert!(
        x0 > 0 && y0 > 0 && x1 < w - 1 && y1 < h - 1,
        "the device touches the edge"
    );
}

/// A turn and a tilt together throw the silhouette wider than the device's own box; the camera
/// steps back for those, so the device still stays in its viewport.
#[test]
fn a_turned_and_tilted_device_stays_in_its_viewport() {
    let (w, h) = (160u32, 260u32);
    for turn in 0..12 {
        for tilt in 0..6 {
            let d = Device3d {
                model: DeviceModel::Phone,
                rotate_y: f64::from(turn) * 30.0,
                rotate_x: f64::from(tilt) * 20.0,
                ..Device3d::default()
            };
            let got = render(&d, w, h);
            for x in 0..w {
                assert_eq!(alpha(&got, x, 0), 0, "top edge at {turn},{tilt}");
                assert_eq!(alpha(&got, x, h - 1), 0, "bottom edge at {turn},{tilt}");
            }
            for y in 0..h {
                assert_eq!(alpha(&got, 0, y), 0, "left edge at {turn},{tilt}");
                assert_eq!(alpha(&got, w - 1, y), 0, "right edge at {turn},{tilt}");
            }
        }
    }
}

#[test]
fn a_spin_never_touches_the_edge_of_the_viewport() {
    let d = Device3d {
        model: DeviceModel::Phone,
        ..Device3d::default()
    };
    let (w, h) = (160u32, 260u32);
    for step in 0..24 {
        let mut d = d.clone();
        d.rotate_y = f64::from(step) * 15.0;
        let got = render(&d, w, h);
        for x in 0..w {
            assert_eq!(alpha(&got, x, 0), 0, "top edge at step {step}");
            assert_eq!(alpha(&got, x, h - 1), 0, "bottom edge at step {step}");
        }
        for y in 0..h {
            assert_eq!(alpha(&got, 0, y), 0, "left edge at step {step}");
            assert_eq!(alpha(&got, w - 1, y), 0, "right edge at step {step}");
        }
    }
}

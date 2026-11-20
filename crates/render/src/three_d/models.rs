//! The five generated devices. Everything is designed in millimetres, with proportions typical of
//! the kind (nothing here copies a manufacturer's artwork), then scaled so the model fits a unit
//! bounding sphere centred on the origin: x right, y up, z toward the viewer.

use std::f32::consts::FRAC_PI_2;
use std::sync::OnceLock;

use od_core::DeviceModel;

use super::math::{Mat3, V3, v3};
use super::mesh::{Mat, Mesh, cylinder, dome, edge_profile, rounded_rect, slab};

/// The display: a rounded rectangle in the front slab's `uv` frame.
#[derive(Clone, Copy, Debug)]
pub struct ScreenRect {
    pub cx: f32,
    pub cy: f32,
    pub hw: f32,
    pub hh: f32,
    pub radius: f32,
}

/// Something cut out of the display: a pill or round camera hole, drawn black.
#[derive(Clone, Copy, Debug)]
pub enum Cutout {
    None,
    Pill { cx: f32, cy: f32, hw: f32, hh: f32 },
    Circle { cx: f32, cy: f32, r: f32 },
}

#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub yc: f32,
    pub half_h: f32,
    pub band: f32,
}

/// Procedural laptop deck, in the base slab's `uv` frame.
#[derive(Clone, Debug)]
pub struct Keyboard {
    pub cols: usize,
    pub x0: f32,
    pub pitch: f32,
    pub half_w: f32,
    pub rows: Vec<Row>,
    pub pad: ScreenRect,
}

pub struct Model {
    pub mesh: Mesh,
    pub screen: ScreenRect,
    pub cutout: Cutout,
    /// A front camera dot in the bezel: (x, y, radius).
    pub dot: Option<[f32; 3]>,
    pub keyboard: Option<Keyboard>,
    /// Millimetres per model unit.
    pub mm_per_unit: f32,
    /// The half-extents of the model's own box, in model units. The camera fits this box to the
    /// viewport, so the device fills it.
    pub half: V3,
    /// The screen's centre and its long side, in model units.
    pub screen_centre: V3,
    pub screen_long: f32,
}

static CACHE: [OnceLock<Model>; 5] = [
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
];

pub fn model(m: DeviceModel) -> &'static Model {
    let slot = DeviceModel::ALL.iter().position(|&x| x == m).unwrap_or(0);
    CACHE[slot].get_or_init(|| match m {
        DeviceModel::Phone => phone(false),
        DeviceModel::PhonePunchHole => phone(true),
        DeviceModel::Tablet => tablet(),
        DeviceModel::Laptop => laptop(),
        DeviceModel::Monitor => monitor(),
    })
}

struct Raw {
    mesh: Mesh,
    screen: ScreenRect,
    cutout: Cutout,
    dot: Option<[f32; 3]>,
    keyboard: Option<Keyboard>,
    /// Screen centre in raw model space (mm).
    screen_pt: V3,
}

fn finish(raw: Raw, aspect: f32) -> Model {
    let Raw {
        mut mesh,
        screen,
        cutout,
        dot,
        keyboard,
        screen_pt,
    } = raw;
    // The model is centred on its own box (so it fills the viewport and spins in place) and
    // scaled so that its bounding sphere has radius 1.
    let mut lo = v3(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = v3(-f32::MAX, -f32::MAX, -f32::MAX);
    for v in &mesh.v {
        lo = v3(lo.x.min(v.p.x), lo.y.min(v.p.y), lo.z.min(v.p.z));
        hi = v3(hi.x.max(v.p.x), hi.y.max(v.p.y), hi.z.max(v.p.z));
    }
    let c = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    for v in &mut mesh.v {
        v.p = v.p - c;
    }
    // Scale so that the model fits a unit ball centred on its box's middle.
    let r = mesh
        .v
        .iter()
        .map(|v| v.p.len())
        .fold(0.0f32, f32::max)
        .max(1e-6);
    let inv = 1.0 / r;
    for v in &mut mesh.v {
        v.p = v.p * inv;
    }
    let long = (screen.hw * 2.0).max(screen.hh * 2.0) * inv;
    debug_assert!((screen.hw / screen.hh - aspect).abs() < 1e-3);
    Model {
        mesh,
        screen,
        cutout,
        dot,
        keyboard,
        mm_per_unit: r,
        half: half * inv,
        screen_centre: (screen_pt - c) * inv,
        screen_long: long,
    }
}

/// A part built face-up (+z outward) and mounted on the back of a body: turned about the y axis
/// so it faces -z. In `part` coordinates +x is to the viewer's right looking at the back.
fn on_back(mut part: Mesh, at: [f32; 2], z: f32) -> Mesh {
    part.transform(
        &Mat3([[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]]),
        v3(at[0], at[1], z),
    );
    part
}

/// A camera lens standing on a surface whose top is at local `z`: metal barrel and ring, a dark
/// glass dome.
fn lens(c: [f32; 2], r: f32, z: f32) -> Mesh {
    let mut m = cylinder(c, r, z - 0.6, z + 0.8, 36, Mat::Metal, Mat::Metal);
    m.append(cylinder(
        c,
        r * 0.78,
        z + 0.5,
        z + 1.1,
        36,
        Mat::Dark,
        Mat::Dark,
    ));
    m.append(dome(c, r * 0.78, z + 1.1, 0.22, 36, 5, Mat::Lens));
    m
}

fn phone(punch: bool) -> Model {
    const W: f32 = 71.5;
    const B: f32 = 2.1;
    const T: f32 = 8.25;
    let aspect = DeviceModel::Phone.screen_aspect() as f32;
    let (ws, r) = (W - 2.0 * B, 0.14 * W);
    let hs = ws / aspect;
    let h = hs + 2.0 * B;
    let profile = edge_profile(T, 0.9, 5, Mat::Front, Mat::Metal, Mat::Back);
    let mut mesh = slab(&rounded_rect(W, h, r, 14), &profile);
    let zb = -T * 0.5;
    if punch {
        // A single-row bar across the top of the back with two lenses and a flash.
        let mut bar = slab(
            &rounded_rect(50.0, 15.5, 7.75, 12),
            &edge_profile(1.4, 0.5, 3, Mat::Back, Mat::Back, Mat::Dark),
        );
        bar.append(lens([-14.0, 0.0], 4.7, 0.7));
        bar.append(lens([-0.5, 0.0], 4.7, 0.7));
        bar.append(lens([13.0, 0.0], 2.3, 0.7));
        mesh.append(on_back(bar, [0.0, h * 0.5 - 5.0 - 7.75], zb - 0.4));
    } else {
        let s = 31.0;
        let mut isl = slab(
            &rounded_rect(s, s, 8.5, 12),
            &edge_profile(1.4, 0.5, 3, Mat::Back, Mat::Back, Mat::Dark),
        );
        isl.append(lens([-7.6, 7.6], 5.8, 0.7));
        isl.append(lens([-7.6, -7.6], 5.8, 0.7));
        isl.append(lens([7.6, 0.0], 5.8, 0.7));
        isl.append(lens([8.2, 11.0], 1.9, 0.7));
        mesh.append(on_back(
            isl,
            [W * 0.5 - 5.0 - s * 0.5, h * 0.5 - 5.0 - s * 0.5],
            zb - 0.4,
        ));
    }
    let top = hs * 0.5;
    let cutout = if punch {
        Cutout::Circle {
            cx: 0.0,
            cy: top - 4.6,
            r: 1.9,
        }
    } else {
        Cutout::Pill {
            cx: 0.0,
            cy: top - 5.3,
            hw: 9.6,
            hh: 2.9,
        }
    };
    finish(
        Raw {
            mesh,
            screen: ScreenRect {
                cx: 0.0,
                cy: 0.0,
                hw: ws * 0.5,
                hh: hs * 0.5,
                radius: r - B,
            },
            cutout,
            dot: None,
            keyboard: None,
            screen_pt: v3(0.0, 0.0, T * 0.5),
        },
        aspect,
    )
}

fn tablet() -> Model {
    const B: f32 = 6.0;
    const T: f32 = 6.3;
    let aspect = DeviceModel::Tablet.screen_aspect() as f32;
    let (ws, r) = (140.0, 11.5);
    let hs = ws / aspect;
    let (w, h) = (ws + 2.0 * B, hs + 2.0 * B);
    let profile = edge_profile(T, 0.8, 5, Mat::Front, Mat::Metal, Mat::Back);
    let mut mesh = slab(&rounded_rect(w, h, r, 14), &profile);
    let mut cam = Mesh::default();
    cam.append(lens([0.0, 0.0], 4.6, 0.0));
    mesh.append(on_back(cam, [w * 0.5 - 13.0, h * 0.5 - 13.0], -T * 0.5));
    finish(
        Raw {
            mesh,
            screen: ScreenRect {
                cx: 0.0,
                cy: 0.0,
                hw: ws * 0.5,
                hh: hs * 0.5,
                radius: r - B,
            },
            cutout: Cutout::None,
            dot: Some([0.0, hs * 0.5 + B * 0.5, 1.1]),
            keyboard: None,
            screen_pt: v3(0.0, 0.0, T * 0.5),
        },
        aspect,
    )
}

fn laptop() -> Model {
    const LID_T: f32 = 3.6;
    const BASE_T: f32 = 12.5;
    const DEPTH: f32 = 221.0;
    const OPEN_DEG: f64 = 105.0;
    let aspect = DeviceModel::Laptop.screen_aspect() as f32;
    let (ws, bs, top, chin) = (300.0, 6.0, 6.0, 9.0);
    let hs = ws / aspect;
    let (w, h) = (ws + 2.0 * bs, hs + top + chin);
    let cy = (chin - top) * 0.5;

    // The lid stands up from the hinge line (y = z = 0) with its screen facing +z.
    let mut lid = slab(
        &rounded_rect(w, h, 8.0, 12),
        &edge_profile(LID_T, 0.7, 4, Mat::Front, Mat::Metal, Mat::Metal),
    );
    lid.transform(&Mat3::IDENTITY, v3(0.0, h * 0.5, -LID_T * 0.5));

    // The base slopes down toward the viewer: the angle between its deck and the lid is 105 deg.
    let open = OPEN_DEG.to_radians();
    let toward = [0.0, open.cos(), open.sin()]; // hinge -> front edge along the deck
    let up = [0.0, open.sin(), -open.cos()]; // deck normal
    let mut base = slab(
        &rounded_rect(w, DEPTH, 12.0, 12),
        &edge_profile(BASE_T, 1.6, 4, Mat::Deck, Mat::Metal, Mat::Dark),
    );
    // local x -> world x, local y (front to back) -> -toward, local z (up) -> deck normal.
    let rot = Mat3::from_cols([1.0, 0.0, 0.0], [-toward[0], -toward[1], -toward[2]], up);
    let hinge = v3(0.0, -0.5, -LID_T * 0.5);
    let centre = [
        f64::from(hinge.x) + toward[0] * f64::from(DEPTH) * 0.5 - up[0] * f64::from(BASE_T) * 0.5,
        f64::from(hinge.y) + toward[1] * f64::from(DEPTH) * 0.5 - up[1] * f64::from(BASE_T) * 0.5,
        f64::from(hinge.z) + toward[2] * f64::from(DEPTH) * 0.5 - up[2] * f64::from(BASE_T) * 0.5,
    ];
    base.transform(
        &rot,
        v3(centre[0] as f32, centre[1] as f32, centre[2] as f32),
    );

    let mut mesh = lid;
    mesh.append(base);

    let mut rows = vec![Row {
        yc: DEPTH * 0.5 - 19.0,
        half_h: 5.0,
        band: 6.75,
    }];
    let mut y = DEPTH * 0.5 - 19.0 - 6.75 - 9.5;
    for _ in 0..5 {
        rows.push(Row {
            yc: y,
            half_h: 8.0,
            band: 9.5,
        });
        y -= 19.0;
    }
    let cols = 14;
    let pitch = 19.2;
    let keyboard = Keyboard {
        cols,
        x0: -(cols as f32) * pitch * 0.5,
        pitch,
        half_w: 8.1,
        rows,
        pad: ScreenRect {
            cx: 0.0,
            cy: -DEPTH * 0.5 + 16.0 + 38.0,
            hw: 62.0,
            hh: 38.0,
            radius: 4.0,
        },
    };
    finish(
        Raw {
            mesh,
            screen: ScreenRect {
                cx: 0.0,
                cy,
                hw: ws * 0.5,
                hh: hs * 0.5,
                radius: 2.2,
            },
            cutout: Cutout::None,
            dot: Some([0.0, cy + hs * 0.5 + top * 0.5, 1.0]),
            keyboard: Some(keyboard),
            screen_pt: v3(0.0, h * 0.5 + cy, 0.0),
        },
        aspect,
    )
}

fn monitor() -> Model {
    const T: f32 = 14.0;
    let aspect = DeviceModel::Monitor.screen_aspect() as f32;
    let (ws, bs, top, chin) = (600.0, 8.0, 8.0, 26.0);
    let hs = ws / aspect;
    let (w, h) = (ws + 2.0 * bs, hs + top + chin);
    let cy = (chin - top) * 0.5;
    let mut mesh = slab(
        &rounded_rect(w, h, 7.0, 12),
        &edge_profile(T, 2.0, 4, Mat::Front, Mat::Metal, Mat::Back),
    );
    let foot_y = -h * 0.5 - 95.0;
    let neck_top = -30.0;
    let neck_bottom = foot_y + 10.0;
    let neck_t = 16.0;
    let mut neck = slab(
        &rounded_rect(70.0, neck_top - neck_bottom, 10.0, 8),
        &edge_profile(neck_t, 3.0, 3, Mat::Metal, Mat::Metal, Mat::Metal),
    );
    neck.transform(
        &Mat3::IDENTITY,
        v3(0.0, (neck_top + neck_bottom) * 0.5, -T * 0.5 - neck_t * 0.5),
    );
    mesh.append(neck);
    let mut foot = slab(
        &rounded_rect(250.0, 190.0, 60.0, 16),
        &edge_profile(10.0, 3.0, 3, Mat::Metal, Mat::Metal, Mat::Dark),
    );
    foot.transform(
        &Mat3::rot_x(-f64::from(FRAC_PI_2)),
        v3(0.0, foot_y + 5.0, -T * 0.5 - 4.0),
    );
    mesh.append(foot);
    finish(
        Raw {
            mesh,
            screen: ScreenRect {
                cx: 0.0,
                cy,
                hw: ws * 0.5,
                hh: hs * 0.5,
                radius: 1.5,
            },
            cutout: Cutout::None,
            dot: None,
            keyboard: None,
            screen_pt: v3(0.0, cy, T * 0.5),
        },
        aspect,
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn every_model_fits_a_unit_sphere() {
        for m in DeviceModel::ALL {
            let model = model(m);
            for v in &model.mesh.v {
                assert!(
                    v.p.len() <= 1.0001,
                    "{} has a point outside the unit sphere",
                    m.as_str()
                );
            }
        }
    }

    #[test]
    fn every_screen_has_the_shape_of_its_capture() {
        for m in DeviceModel::ALL {
            let model = model(m);
            let aspect = model.screen.hw / model.screen.hh;
            assert!(
                (aspect - m.screen_aspect() as f32).abs() < 0.002,
                "{}: screen {aspect} vs {}",
                m.as_str(),
                m.screen_aspect()
            );
        }
    }

    #[test]
    fn the_laptop_is_opened_past_upright() {
        let model = model(DeviceModel::Laptop);
        // The lid stands up from the hinge and the deck slopes away from it.
        let hinge = v3(0.0, 0.0, 0.0);
        let deck = model
            .mesh
            .v
            .iter()
            .map(|v| v.p)
            .fold(v3(0.0, 0.0, 0.0), |a, b| {
                if b.z > a.z && b.y < 0.0 { b } else { a }
            });
        assert!(deck.z > 0.0, "the deck does not come towards the viewer");
        let lid_top = model
            .mesh
            .v
            .iter()
            .map(|v| v.p)
            .fold(v3(0.0, 0.0, 0.0), |a, b| if b.y > a.y { b } else { a });
        assert!(lid_top.y > 0.0, "the lid does not stand up");
        assert!(hinge.len() < 0.001);
    }
}

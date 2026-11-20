//! Projection, near-plane clipping and a depth-buffered, perspective-correct triangle rasterizer
//! that works a band of rows at a time. Samples are exact functions of their position, so the
//! picture is the same however the bands are spread across threads.

use od_core::Device3d;

use super::math::{Mat3, V3, v3};
use super::mesh::Mesh;

pub const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Distance from the eye to the model's centre, in model units (the model's radius is 1).
    pub d: f64,
    /// Pixels per unit of x/w.
    pub f: f64,
    pub cx: f64,
    pub cy: f64,
    pub near: f64,
}

/// How much of the viewport the device may fill: the rest is the margin, so a turn or a shadow
/// never runs into the edge.
pub const MARGIN: f64 = 0.95;

/// The camera for a device: the model's own box is fitted to the `w` x `h` viewport (so the
/// device fills it), and the distance is pulled back when the pose throws the silhouette wider
/// than its box (a turn and a tilt together do).
pub fn camera(device: &Device3d, half: V3, w: u32, h: u32) -> Camera {
    let fov = device.fov.clamp(1.0, 100.0).to_radians() * 0.5;
    let f = f64::from(h) * 0.5 / fov.tan();
    let zoom = device.zoom.clamp(0.05, 10.0);
    // The box seen from the front: its width and height must fit the viewport.
    let wide = 2.0 * f64::from(half.x);
    let tall = 2.0 * f64::from(half.y);
    let mut d = f * (wide / f64::from(w)).max(tall / f64::from(h)) / MARGIN / zoom;
    let rot = pose(device.rotate_x, device.rotate_y, device.rotate_z);
    let mut cam = Camera {
        d,
        f,
        cx: f64::from(w) * 0.5,
        cy: f64::from(h) * 0.5,
        near: 0.0,
    };
    for _ in 0..16 {
        cam.d = d;
        cam.near = (d * 0.01).min(0.05);
        let need = overflow(&cam, &rot, half, w, h);
        if need <= 1.0 {
            break;
        }
        d *= need;
    }
    cam.d = d;
    cam.near = (d * 0.01).min(0.05);
    cam
}

/// How much further away the camera must be for the turned box to fit the viewport with its
/// margin (1 when it already fits).
fn overflow(cam: &Camera, rot: &Mat3, half: V3, w: u32, h: u32) -> f64 {
    let mut need = 1.0f64;
    for sx in [-1.0f64, 1.0] {
        for sy in [-1.0f64, 1.0] {
            for sz in [-1.0f64, 1.0] {
                let p = v3(
                    (sx * f64::from(half.x)) as f32,
                    (sy * f64::from(half.y)) as f32,
                    (sz * f64::from(half.z)) as f32,
                );
                let Some((x, y)) = project(cam, rot, p) else {
                    return 2.0;
                };
                need = need
                    .max((x - cam.cx).abs() / (MARGIN * f64::from(w) * 0.5))
                    .max((y - cam.cy).abs() / (MARGIN * f64::from(h) * 0.5));
            }
        }
    }
    need * 1.000_5
}

/// Model -> world: roll first, then tilt, then turn (Ry * Rx * Rz).
pub fn pose(rotate_x: f64, rotate_y: f64, rotate_z: f64) -> Mat3 {
    // Positive tilt leans the top away (-z), positive turn takes the right edge away, positive
    // roll is clockwise as the viewer sees it.
    Mat3::rot_y(rotate_y.to_radians())
        .mul(&Mat3::rot_x(-rotate_x.to_radians()))
        .mul(&Mat3::rot_z(-rotate_z.to_radians()))
}

/// Where a model-space point lands, in output pixels (None behind the eye).
pub fn project(cam: &Camera, rot: &Mat3, p: V3) -> Option<(f64, f64)> {
    let w = pw(cam, rot, p);
    let r = rot.apply([f64::from(p.x), f64::from(p.y), f64::from(p.z)]);
    (w.1 > 1e-9).then(|| (cam.cx + cam.f * r[0] / w.1, cam.cy - cam.f * r[1] / w.1))
}

fn pw(cam: &Camera, rot: &Mat3, p: V3) -> (f64, f64) {
    let r = rot.apply([f64::from(p.x), f64::from(p.y), f64::from(p.z)]);
    (r[2], cam.d - r[2])
}

/// A vertex ready for shading: camera-relative position and world normal.
#[derive(Clone, Copy)]
pub struct ShadeVert {
    pub p: V3,
    pub n: V3,
    pub uv: [f32; 2],
}

pub struct Prim {
    iw: [f64; 3],
    /// Barycentric weights of each corner in the mesh triangle it was cut from.
    bary: [[f32; 3]; 3],
    tri: u32,
    /// Edge `k` (opposite corner `k`) as `a x + b y + c`, sign-normalised to be >= 0 inside.
    e: [[f64; 3]; 3],
    inv_area: f64,
    pub y_min: f64,
    pub y_max: f64,
}

pub struct Prepared {
    pub verts: Vec<ShadeVert>,
    pub prims: Vec<Prim>,
}

fn edge(a: (f64, f64), b: (f64, f64)) -> [f64; 3] {
    let (flip, a, b) = if a.0 > b.0 || (a.0 == b.0 && a.1 > b.1) {
        (-1.0, b, a)
    } else {
        (1.0, a, b)
    };
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    [-dy * flip, dx * flip, (-dx * a.1 + dy * a.0) * flip]
}

#[derive(Clone, Copy)]
struct CV {
    x: f64,
    y: f64,
    w: f64,
    bary: [f32; 3],
}

/// Transforms the mesh, culls back faces, clips against the near plane and sets up the triangles
/// at `ss` samples per output pixel.
pub fn prepare(mesh: &Mesh, rot: &Mat3, cam: &Camera, ss: usize) -> Prepared {
    let mut view = Vec::with_capacity(mesh.v.len());
    let mut verts = Vec::with_capacity(mesh.v.len());
    for v in &mesh.v {
        let r = rot.apply([f64::from(v.p.x), f64::from(v.p.y), f64::from(v.p.z)]);
        let p = [r[0], r[1], r[2] - cam.d];
        view.push(p);
        verts.push(ShadeVert {
            p: v3(p[0] as f32, p[1] as f32, p[2] as f32),
            n: rot.apply_v3(v.n).norm(),
            uv: v.uv,
        });
    }
    let mut prims = Vec::with_capacity(mesh.t.len());
    let s = ss as f64;
    for (ti, t) in mesh.t.iter().enumerate() {
        let [a, b, c] = [
            view[t.i[0] as usize],
            view[t.i[1] as usize],
            view[t.i[2] as usize],
        ];
        let (e1, e2) = (
            [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
            [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
        );
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        if n[0] * a[0] + n[1] * a[1] + n[2] * a[2] >= 0.0 {
            continue;
        }
        let corner = |p: [f64; 3], k: usize| {
            let mut bary = [0.0; 3];
            bary[k] = 1.0;
            CV {
                x: p[0],
                y: p[1],
                w: -p[2],
                bary,
            }
        };
        let poly = [corner(a, 0), corner(b, 1), corner(c, 2)];
        if poly.iter().all(|v| v.w >= cam.near) {
            push_prim(&mut prims, &poly, ti as u32, cam, s);
        } else if poly.iter().any(|v| v.w >= cam.near) {
            let clipped = clip_near(&poly, cam.near);
            for k in 1..clipped.len().saturating_sub(1) {
                push_prim(
                    &mut prims,
                    &[clipped[0], clipped[k], clipped[k + 1]],
                    ti as u32,
                    cam,
                    s,
                );
            }
        }
    }
    Prepared { verts, prims }
}

fn clip_near(poly: &[CV; 3], near: f64) -> Vec<CV> {
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let (a, b) = (poly[i], poly[(i + 1) % 3]);
        let (ain, bin) = (a.w >= near, b.w >= near);
        if ain {
            out.push(a);
        }
        if ain != bin {
            let t = (near - a.w) / (b.w - a.w);
            let lerp = |p: f64, q: f64| p + (q - p) * t;
            let tf = t as f32;
            out.push(CV {
                x: lerp(a.x, b.x),
                y: lerp(a.y, b.y),
                w: near,
                bary: [
                    a.bary[0] + (b.bary[0] - a.bary[0]) * tf,
                    a.bary[1] + (b.bary[1] - a.bary[1]) * tf,
                    a.bary[2] + (b.bary[2] - a.bary[2]) * tf,
                ],
            });
        }
    }
    out
}

fn push_prim(prims: &mut Vec<Prim>, v: &[CV; 3], tri: u32, cam: &Camera, s: f64) {
    let pt = |c: &CV| {
        (
            (cam.cx + cam.f * c.x / c.w) * s,
            (cam.cy - cam.f * c.y / c.w) * s,
        )
    };
    let p = [pt(&v[0]), pt(&v[1]), pt(&v[2])];
    let mut e = [edge(p[1], p[2]), edge(p[2], p[0]), edge(p[0], p[1])];
    // Area from the same canonical edge arithmetic: edge 2 evaluated at corner 2.
    let area = e[2][0] * p[2].0 + e[2][1] * p[2].1 + e[2][2];
    if !area.is_finite() || area.abs() < 1e-9 {
        return;
    }
    let sign = area.signum();
    for k in &mut e {
        for c in k.iter_mut() {
            *c *= sign;
        }
    }
    let ys = [p[0].1, p[1].1, p[2].1];
    prims.push(Prim {
        iw: [1.0 / v[0].w, 1.0 / v[1].w, 1.0 / v[2].w],
        bary: [v[0].bary, v[1].bary, v[2].bary],
        tri,
        e,
        inv_area: 1.0 / area.abs(),
        y_min: ys[0].min(ys[1]).min(ys[2]),
        y_max: ys[0].max(ys[1]).max(ys[2]),
    });
}

/// Visibility of a band's samples: depth as 1/w (larger is nearer), the mesh triangle, and the
/// weights of its first two corners.
pub struct Vis {
    pub depth: Vec<f32>,
    pub tri: Vec<u32>,
    pub wb: Vec<[f32; 2]>,
}

impl Vis {
    pub fn new(samples: usize) -> Vis {
        Vis {
            depth: vec![0.0; samples],
            tri: vec![NONE; samples],
            wb: vec![[0.0; 2]; samples],
        }
    }

    pub fn clear(&mut self, samples: usize) {
        self.depth[..samples].fill(0.0);
        self.tri[..samples].fill(NONE);
    }
}

/// Rasterizes `ids` into sample rows `row0 .. row0 + rows` of a `width`-sample-wide frame.
pub fn raster_band(
    prims: &[Prim],
    ids: &[u32],
    row0: usize,
    rows: usize,
    width: usize,
    vis: &mut Vis,
) {
    for &id in ids {
        let p = &prims[id as usize];
        let r_lo = ((p.y_min - 0.5).ceil() as i64 - 1).max(row0 as i64);
        let r_hi = ((p.y_max - 0.5).floor() as i64 + 1).min((row0 + rows) as i64 - 1);
        let mut r = r_lo;
        while r <= r_hi {
            raster_row(p, r as usize, row0, width, vis);
            r += 1;
        }
    }
}

fn raster_row(p: &Prim, row: usize, row0: usize, width: usize, vis: &mut Vis) {
    let yc = row as f64 + 0.5;
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for e in &p.e {
        let k = e[1] * yc + e[2];
        if e[0] > 1e-12 {
            lo = lo.max(-k / e[0]);
        } else if e[0] < -1e-12 {
            hi = hi.min(-k / e[0]);
        } else if k < 0.0 {
            return;
        }
    }
    if lo > hi + 2.0 {
        return;
    }
    let x0 = (lo - 0.5).floor().max(-1.0) as i64 - 1;
    let x1 = (hi - 0.5).ceil().min(width as f64) as i64 + 1;
    let (x0, x1) = (x0.max(0) as usize, x1.min(width as i64 - 1));
    if x1 < 0 {
        return;
    }
    let base = (row - row0) * width;
    for x in x0..=(x1 as usize) {
        let xc = x as f64 + 0.5;
        let e0 = p.e[0][0] * xc + p.e[0][1] * yc + p.e[0][2];
        let e1 = p.e[1][0] * xc + p.e[1][1] * yc + p.e[1][2];
        let e2 = p.e[2][0] * xc + p.e[2][1] * yc + p.e[2][2];
        if e0 < 0.0 || e1 < 0.0 || e2 < 0.0 {
            continue;
        }
        let (l0, l1, l2) = (e0 * p.inv_area, e1 * p.inv_area, e2 * p.inv_area);
        let (q0, q1, q2) = (l0 * p.iw[0], l1 * p.iw[1], l2 * p.iw[2]);
        let iw = q0 + q1 + q2;
        let i = base + x;
        if (iw as f32) <= vis.depth[i] {
            continue;
        }
        let inv = 1.0 / iw;
        let (c0, c1, c2) = ((q0 * inv) as f32, (q1 * inv) as f32, (q2 * inv) as f32);
        vis.depth[i] = iw as f32;
        vis.tri[i] = p.tri;
        vis.wb[i] = [
            c0 * p.bary[0][0] + c1 * p.bary[1][0] + c2 * p.bary[2][0],
            c0 * p.bary[0][1] + c1 * p.bary[1][1] + c2 * p.bary[2][1],
        ];
    }
}

impl Prim {
    pub fn rows(&self, band_rows: usize, bands: usize) -> (usize, usize) {
        let lo = ((self.y_min - 0.5).ceil() as i64 - 1).max(0) as usize / band_rows;
        let hi = (((self.y_max - 0.5).floor() as i64 + 1).max(0) as usize / band_rows)
            .min(bands.saturating_sub(1));
        (lo.min(bands.saturating_sub(1)), hi)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    /// The pose conventions: a positive tilt leans the top away, a positive turn takes the right
    /// edge away, a positive roll turns the picture clockwise.
    #[test]
    fn the_pose_follows_the_documented_directions() {
        let tilt = pose(10.0, 0.0, 0.0).apply([0.0, 1.0, 0.0]);
        assert!(tilt[2] < 0.0, "the top did not lean away");
        let turn = pose(0.0, 10.0, 0.0).apply([1.0, 0.0, 0.0]);
        assert!(turn[2] < 0.0, "the right edge did not turn away");
        let roll = pose(0.0, 0.0, 10.0).apply([0.0, 1.0, 0.0]);
        assert!(roll[0] > 0.0, "the roll was not clockwise");
    }

    #[test]
    fn the_camera_holds_the_model_whatever_the_turn() {
        let (w, h) = (200u32, 300u32);
        // A phone-ish box: the device is fitted by its own size, not by a sphere around it.
        let half = v3(0.85, 0.9, 0.05);
        for step in 0..24 {
            let device = Device3d {
                rotate_x: f64::from(step) * 15.0,
                rotate_y: f64::from(step) * 7.0,
                rotate_z: f64::from(step) * 3.0,
                ..Device3d::default()
            };
            let cam = camera(&device, half, w, h);
            let rot = pose(device.rotate_x, device.rotate_y, device.rotate_z);
            for sx in [-1.0f32, 1.0] {
                for sy in [-1.0f32, 1.0] {
                    for sz in [-1.0f32, 1.0] {
                        let p = v3(sx * half.x, sy * half.y, sz * half.z);
                        let (x, y) = project(&cam, &rot, p).expect("in front of the eye");
                        assert!(
                            x >= 0.0 && x <= f64::from(w) && y >= 0.0 && y <= f64::from(h),
                            "the model leaves the viewport at step {step}: {x},{y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_device_fills_the_viewport() {
        let (w, h) = (200u32, 300u32);
        let half = v3(0.85, 0.9, 0.05);
        let device = Device3d::default();
        let cam = camera(&device, half, w, h);
        let rot = pose(0.0, 0.0, 0.0);
        let (x, _) = project(&cam, &rot, v3(half.x, 0.0, 0.0)).expect("in front");
        let fill = (x - cam.cx) * 2.0 / f64::from(w);
        assert!(
            (0.90..=0.95).contains(&fill),
            "the device fills {fill} of the width"
        );
    }

    #[test]
    fn a_long_lens_is_almost_flat() {
        let half = v3(0.85, 0.9, 0.05);
        let long = Device3d {
            fov: 1.0,
            ..Device3d::default()
        };
        let wide = Device3d {
            fov: 100.0,
            ..Device3d::default()
        };
        let a = camera(&long, half, 200, 300);
        let b = camera(&wide, half, 200, 300);
        assert!(a.d > b.d * 10.0, "a 1 degree lens is not almost flat");
    }
}

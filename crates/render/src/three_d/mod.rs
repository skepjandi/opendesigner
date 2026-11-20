//! 3D devices: generated models of a phone, tablet, laptop or monitor, drawn in perspective by a
//! deterministic CPU rasterizer with a capture on the screen
//! (`docs/wiki/document-format.md#3d-devices`).
//!
//! A model is a triangle mesh in device space (x right, y up, z toward the viewer), centred on its
//! own box and scaled so that it fits a unit ball. The pose is `Ry(turn) * Rx(tilt) * Rz(roll)`;
//! the camera looks at the origin from the distance that fits the device's own box in the
//! viewport with a margin (so the device fills its box), stepping back for the poses where a
//! turn and a tilt together throw the silhouette wider than the box. The zoom moves that
//! distance. The picture is rasterized by bands of rows into a visibility buffer (supersampled,
//! depth tested), shaded once per visible sample, and averaged down; the soft shadow is added
//! under the device last. Nothing depends on the thread count.

mod math;
mod mesh;
mod models;
mod raster;
mod shade;
mod shadow;
mod texture;

use od_core::{Device3d, Rgba};
use tiny_skia::Pixmap;

use math::{V3, v3};
use raster::{Camera, NONE, Prepared, Vis};
use shade::{Frag, Shader};
use texture::{Texture, srgb_encode};

/// Everything one device picture needs.
pub struct Scene<'a> {
    pub device: &'a Device3d,
    /// The body's finish (the node's fill, or the default).
    pub body: Rgba,
    /// What the screen shows, premultiplied RGBA with the screen's aspect (from
    /// [`screen_texture_size`]), top row first. `None` is a screen that is off.
    pub screen: Option<&'a Pixmap>,
    pub width: u32,
    pub height: u32,
}

/// Output rows per band: the unit of parallel work (fixed, so results never depend on threads).
const BAND: usize = 8;
const MAX_SIDE: u32 = 16_384;

/// The size to prepare the screen picture at for a device drawn at `width` x `height`: the
/// screen's own aspect, and at least as many pixels as the screen covers on output at any
/// rotation (perspective included), capped at 4096 a side.
pub fn screen_texture_size(device: &Device3d, width: u32, height: u32) -> (u32, u32) {
    let aspect = device.model.screen_aspect();
    let (w, h) = (width.max(1), height.max(1));
    let m = models::model(device.model);
    let cam = raster::camera(device, m.half, w, h);
    let len = f64::from(m.screen_long);
    let half = len * 0.5;
    // A segment of the screen's long side turned by `t` about its centre: its perspective image.
    let mut best = 0.0f64;
    for deg in 0..90 {
        let t = f64::from(deg).to_radians();
        let s = half * t.sin();
        let denom = (cam.d * cam.d - s * s).max(0.05 * cam.d * cam.d);
        best = best.max(cam.f * len * cam.d * t.cos() / denom);
    }
    let side = best.ceil().clamp(16.0, 4096.0);
    if aspect < 1.0 {
        (((side * aspect).round() as u32).max(1), side as u32)
    } else {
        (side as u32, ((side / aspect).round() as u32).max(1))
    }
}

/// Where the middle of the screen lands in a `width` x `height` picture of `device`, in pixels
/// (`None` when it is behind the eye).
pub fn screen_center_px(device: &Device3d, width: u32, height: u32) -> Option<(f32, f32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let m = models::model(device.model);
    let cam = raster::camera(device, m.half, width, height);
    let rot = raster::pose(device.rotate_x, device.rotate_y, device.rotate_z);
    raster::project(&cam, &rot, m.screen_centre).map(|(x, y)| (x as f32, y as f32))
}

/// The device on a transparent `width` x `height` pixmap (premultiplied), with its shadow.
/// `None` when the size is empty or too large to allocate.
pub fn render_device(scene: &Scene) -> Option<Pixmap> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(16));
    render_device_with_threads(scene, threads)
}

/// [`render_device`] on exactly `threads` threads; the picture is identical for any count.
pub fn render_device_with_threads(scene: &Scene, threads: usize) -> Option<Pixmap> {
    let (w, h) = (scene.width, scene.height);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let mut pix = Pixmap::new(w, h)?;
    let threads = threads.max(1);
    let d = scene.device;
    let model = models::model(d.model);
    let ss = supersample(w, h);
    let cam = raster::camera(d, model.half, w, h);
    let rot = raster::pose(d.rotate_x, d.rotate_y, d.rotate_z);
    let prepared = raster::prepare(&model.mesh, &rot, &cam, ss);

    let (wu, hu) = (w as usize, h as usize);
    let bands = hu.div_ceil(BAND);
    let mut bins: Vec<Vec<u32>> = vec![Vec::new(); bands];
    for (i, p) in prepared.prims.iter().enumerate() {
        let (lo, hi) = p.rows(BAND * ss, bands);
        for bin in &mut bins[lo..=hi] {
            bin.push(i as u32);
        }
    }

    let texture = scene
        .screen
        .map(|s| Texture::new(s, mips_needed(s, model, &cam, &rot, ss), threads));
    let shader = Shader::new(
        model,
        d.light_angle,
        [scene.body.r, scene.body.g, scene.body.b],
        d.glare.clamp(0.0, 1.0),
        texture,
    );
    let ctx = Ctx {
        prepared: &prepared,
        shader: &shader,
        bins: &bins,
        cam,
        ss,
        w: wu,
        mm_per_unit: model.mm_per_unit,
        mesh: &model.mesh,
    };

    let workers = threads.min(bands);
    {
        let mut groups: Vec<Vec<(usize, &mut [u8])>> = (0..workers).map(|_| Vec::new()).collect();
        for (i, chunk) in pix.data_mut().chunks_mut(BAND * wu * 4).enumerate() {
            groups[i % workers].push((i, chunk));
        }
        let run = |group: Vec<(usize, &mut [u8])>| {
            let mut vis = Vis::new(BAND * ss * wu * ss);
            for (band, out) in group {
                render_band(&ctx, band, out, &mut vis);
            }
        };
        if workers == 1 {
            for g in groups {
                run(g);
            }
        } else {
            std::thread::scope(|s| {
                for g in groups {
                    let run = &run;
                    s.spawn(move || run(g));
                }
            });
        }
    }
    shadow::apply(
        pix.data_mut(),
        wu,
        hu,
        d.shadow.clamp(0.0, 1.0) as f32,
        d.light_angle,
        threads,
    );
    Some(pix)
}

/// Samples per output pixel along each axis: more for small pictures, which can afford it.
fn supersample(w: u32, h: u32) -> usize {
    let px = u64::from(w) * u64::from(h);
    if px <= 16_384 {
        4
    } else if px <= 100_000 {
        3
    } else {
        2
    }
}

/// How many mip levels the screen picture needs: enough that its smallest projected edge is
/// not drawn with more than one texel per sample.
fn mips_needed(
    tex: &Pixmap,
    m: &models::Model,
    cam: &Camera,
    rot: &math::Mat3,
    ss: usize,
) -> usize {
    let (hw, hh) = (m.screen.hw / m.mm_per_unit, m.screen.hh / m.mm_per_unit);
    let c = m.screen_centre;
    let corner = |sx: f32, sy: f32| raster::project(cam, rot, c + v3(sx * hw, sy * hh, 0.0));
    let (Some(tl), Some(tr), Some(bl), Some(br)) = (
        corner(-1.0, 1.0),
        corner(1.0, 1.0),
        corner(-1.0, -1.0),
        corner(1.0, -1.0),
    ) else {
        return 12;
    };
    let dist = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) * ss as f64;
    let across = dist(tl, tr).min(dist(bl, br)).max(1e-3);
    let down = dist(tl, bl).min(dist(tr, br)).max(1e-3);
    let ratio = (f64::from(tex.width()) / across).max(f64::from(tex.height()) / down);
    if ratio <= 1.0 {
        0
    } else {
        (ratio.log2().ceil() as usize).min(12)
    }
}

struct Ctx<'a> {
    prepared: &'a Prepared,
    shader: &'a Shader<'a>,
    bins: &'a [Vec<u32>],
    cam: Camera,
    ss: usize,
    w: usize,
    mm_per_unit: f32,
    mesh: &'a mesh::Mesh,
}

impl Ctx<'_> {
    fn corners(&self, tri: u32) -> [usize; 3] {
        let i = self.mesh.t[tri as usize].i;
        [i[0] as usize, i[1] as usize, i[2] as usize]
    }

    fn uv_at(&self, tri: u32, wb: [f32; 2]) -> [f32; 2] {
        let [a, b, c] = self.corners(tri);
        let v = &self.prepared.verts;
        let w2 = 1.0 - wb[0] - wb[1];
        [
            wb[0] * v[a].uv[0] + wb[1] * v[b].uv[0] + w2 * v[c].uv[0],
            wb[0] * v[a].uv[1] + wb[1] * v[b].uv[1] + w2 * v[c].uv[1],
        ]
    }

    fn shade_sample(&self, vis: &Vis, i: usize, ws: usize, rows: usize) -> V3 {
        let tri = vis.tri[i];
        let wb = vis.wb[i];
        let [a, b, c] = self.corners(tri);
        let v = &self.prepared.verts;
        let w2 = 1.0 - wb[0] - wb[1];
        let p = v[a].p * wb[0] + v[b].p * wb[1] + v[c].p * w2;
        let n = (v[a].n * wb[0] + v[b].n * wb[1] + v[c].n * w2).norm();
        let uv = self.uv_at(tri, wb);
        let mat = self.mesh.t[tri as usize].mat;
        let mut fp = 0.0f32;
        if matches!(mat, mesh::Mat::Front | mesh::Mat::Deck) {
            let (x, y) = (i % ws, i / ws);
            let nx = if x + 1 < ws { i + 1 } else { i - 1 };
            let ny = if (y + 1) < rows { i + ws } else { i - ws };
            for j in [nx, ny] {
                let t = vis.tri[j];
                if t != NONE
                    && self.mesh.t[t as usize].mat == mat
                    && (vis.depth[j] / vis.depth[i] - 1.0).abs() < 0.05
                {
                    let o = self.uv_at(t, vis.wb[j]);
                    fp = fp.max((o[0] - uv[0]).hypot(o[1] - uv[1]));
                }
            }
            if fp == 0.0 {
                fp = -p.z / (self.cam.f as f32 * self.ss as f32) * self.mm_per_unit;
            }
        }
        self.shader.shade(&Frag { p, n, uv, fp, mat })
    }
}

fn render_band(ctx: &Ctx, band: usize, out: &mut [u8], vis: &mut Vis) {
    let ss = ctx.ss;
    let rows = out.len() / (ctx.w * 4);
    let (rs, ws) = (rows * ss, ctx.w * ss);
    vis.clear(rs * ws);
    raster::raster_band(
        &ctx.prepared.prims,
        &ctx.bins[band],
        band * BAND * ss,
        rs,
        ws,
        vis,
    );
    let inv = 1.0 / (ss * ss) as f32;
    for yo in 0..rows {
        for xo in 0..ctx.w {
            let mut acc = V3::ZERO;
            let mut cov = 0u32;
            for sy in 0..ss {
                for sx in 0..ss {
                    let i = (yo * ss + sy) * ws + xo * ss + sx;
                    if vis.tri[i] != NONE {
                        acc += ctx.shade_sample(vis, i, ws, rs);
                        cov += 1;
                    }
                }
            }
            if cov == 0 {
                continue;
            }
            let alpha = cov as f32 * inv;
            let straight = acc * (1.0 / cov as f32);
            let px = &mut out[(yo * ctx.w + xo) * 4..][..4];
            let enc = |l: f32| (srgb_encode(l) * alpha * 255.0).round() as u8;
            px[0] = enc(straight.x);
            px[1] = enc(straight.y);
            px[2] = enc(straight.z);
            px[3] = (alpha * 255.0).round() as u8;
        }
    }
}

//! The fragment stage: a studio (soft boxes over a gradient) lights and is reflected by every
//! surface. Metal is brushed and tinted by the body colour, the front is black glass with the
//! screen picture emissive under a Fresnel reflection, camera lenses are dark glossy glass, and a
//! laptop deck is drawn procedurally (keys, trackpad).

use std::sync::{Arc, Mutex, OnceLock};

use super::math::{V3, mix, pow16, pow64, reflect, saturate, smoothstep, v3};
use super::mesh::Mat;
use super::models::{Cutout, Model, ScreenRect};
use super::texture::{Texture, srgb_decode};

/// 1 inside the box, fading to 0 at `edge` and beyond `full`.
fn ramp(x: f32, edge: f32, full: f32) -> f32 {
    if x >= full {
        return 0.0;
    }
    let t = ((full - x) / (full - edge + 1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

struct SoftBox {
    c: V3,
    ux: V3,
    uy: V3,
    hu: f32,
    hv: f32,
    soft: f32,
    power: V3,
}

impl SoftBox {
    fn new(dir: V3, half: [f32; 2], soft: f32, power: V3, horizontal_ref: bool) -> SoftBox {
        let c = dir.norm();
        let reference = if horizontal_ref || c.y.abs() > 0.9 {
            v3(0.0, 0.0, 1.0)
        } else {
            v3(0.0, 1.0, 0.0)
        };
        let ux = reference.cross(c).norm();
        let uy = c.cross(ux);
        SoftBox {
            c,
            ux,
            uy,
            hu: half[0],
            hv: half[1],
            soft,
            power,
        }
    }

    fn eval(&self, d: V3, rough: f32) -> V3 {
        let z = d.dot(self.c);
        if z <= 0.02 {
            return V3::ZERO;
        }
        let soft = (self.soft + 1.1 * rough).min(0.95);
        let grow = 1.0 + 0.6 * rough;
        // The box is a rectangle in the plane at its axis: the falloff is measured against the
        // rectangle's edges, without a division per axis.
        let (hu, hv) = (self.hu * grow * z, self.hv * grow * z);
        let (eu, ev) = (hu * soft, hv * soft);
        let a = ramp(d.dot(self.ux).abs(), eu, hu);
        let b = ramp(d.dot(self.uy).abs(), ev, hv);
        self.power * (a * b / (1.0 + 2.0 * rough))
    }
}

/// The lighting environment, built from the key light's angle.
struct Studio {
    boxes: [SoftBox; 7],
    key: V3,
    fill: V3,
}

impl Studio {
    fn new(light_angle_deg: f64) -> Studio {
        let a = light_angle_deg.to_radians() as f32;
        let (kx, ky) = (a.sin(), a.cos());
        let key = v3(kx * 0.9, ky * 0.9, 0.45).norm();
        let fill = v3(-kx * 0.8, -ky * 0.3 + 0.1, 0.55).norm();
        Studio {
            boxes: [
                SoftBox::new(key, [0.62, 0.42], 0.22, v3(3.3, 3.2, 3.1), false),
                SoftBox::new(fill, [0.85, 0.6], 0.5, v3(0.8, 0.85, 0.92), false),
                SoftBox::new(
                    v3(-0.95, 0.15, -0.35),
                    [0.13, 1.6],
                    0.35,
                    v3(2.0, 2.1, 2.3),
                    false,
                ),
                SoftBox::new(
                    v3(0.95, 0.15, -0.35),
                    [0.13, 1.6],
                    0.35,
                    v3(2.3, 2.2, 2.1),
                    false,
                ),
                SoftBox::new(
                    v3(0.0, 1.0, -0.15),
                    [1.6, 0.14],
                    0.4,
                    v3(1.7, 1.7, 1.8),
                    true,
                ),
                // The large soft box in front of the subject: what a screen and a metal edge
                // reflect in a studio shot.
                SoftBox::new(
                    v3(-0.4, 0.5, 1.0),
                    [0.5, 0.4],
                    0.4,
                    v3(2.3, 2.3, 2.4),
                    false,
                ),
                SoftBox::new(
                    v3(0.7, -0.35, 1.0),
                    [0.42, 0.5],
                    0.5,
                    v3(1.0, 1.05, 1.15),
                    false,
                ),
            ],
            key,
            fill,
        }
    }

    fn env(&self, d: V3, rough: f32) -> V3 {
        let sky = 0.09 + 0.26 * smoothstep(-0.35, 1.0, d.y);
        let toward_camera = 0.3 + 0.7 * smoothstep(0.95, -0.25, d.z);
        let mut c = v3(0.96, 0.98, 1.0) * (sky * toward_camera);
        for b in &self.boxes {
            c += b.eval(d, rough);
        }
        c
    }

    fn irradiance(&self, n: V3) -> V3 {
        let amb = 0.2 + 0.14 * n.y;
        v3(1.0, 0.98, 0.95) * (n.dot(self.key).max(0.0) * 1.5)
            + v3(0.92, 0.96, 1.0) * (n.dot(self.fill).max(0.0) * 0.3)
            + V3::splat(amb)
    }
}

/// The cube map of a studio, kept between frames: the light rarely moves.
fn env_map(light_angle: f64, studio: &Studio) -> Arc<EnvMap> {
    type Cache = Mutex<Option<(u64, Arc<EnvMap>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let key = light_angle.to_bits();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, map)) = guard.as_ref() {
        if *k == key {
            return Arc::clone(map);
        }
    }
    let map = Arc::new(EnvMap::build(studio, 32));
    *guard = Some((key, Arc::clone(&map)));
    map
}

/// The studio, baked into a cube map: reflections look the map up instead of working out every
/// soft box again for every sample. Two levels, a sharp one and a blurred one for rough metal.
type Faces = [Vec<V3>; 6];

struct EnvMap {
    size: usize,
    sharp: Faces,
    soft: Faces,
}

fn cube_uv(d: V3) -> (usize, f32, f32) {
    let (ax, ay, az) = (d.x.abs(), d.y.abs(), d.z.abs());
    let (face, sc, tc, ma) = if ax >= ay && ax >= az {
        if d.x > 0.0 {
            (0, -d.z, -d.y, d.x)
        } else {
            (1, d.z, -d.y, -d.x)
        }
    } else if ay >= az {
        if d.y > 0.0 {
            (2, d.x, d.z, d.y)
        } else {
            (3, d.x, -d.z, -d.y)
        }
    } else if d.z > 0.0 {
        (4, d.x, -d.y, d.z)
    } else {
        (5, -d.x, -d.y, -d.z)
    };
    let inv = 0.5 / ma.abs().max(1e-6);
    (
        face,
        (sc * inv + 0.5).clamp(0.0, 1.0),
        (tc * inv + 0.5).clamp(0.0, 1.0),
    )
}

fn cube_dir(face: usize, u: f32, v: f32) -> V3 {
    let (a, b) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
    let d = match face {
        0 => v3(1.0, -b, -a),
        1 => v3(-1.0, -b, a),
        2 => v3(a, 1.0, b),
        3 => v3(a, -1.0, -b),
        4 => v3(a, -b, 1.0),
        _ => v3(-a, -b, -1.0),
    };
    d.norm()
}

impl EnvMap {
    fn build(studio: &Studio, size: usize) -> EnvMap {
        let mut sharp: Faces = Default::default();
        for (f, face) in sharp.iter_mut().enumerate() {
            face.reserve(size * size);
            for j in 0..size {
                for i in 0..size {
                    let u = (i as f32 + 0.5) / size as f32;
                    let v = (j as f32 + 0.5) / size as f32;
                    face.push(studio.env(cube_dir(f, u, v), 0.0));
                }
            }
        }
        let mut soft: Faces = Default::default();
        for (f, face) in sharp.iter().enumerate() {
            soft[f] = blur_face(face, size);
        }
        EnvMap { size, sharp, soft }
    }

    fn sample(&self, d: V3, rough: f32) -> V3 {
        let (face, u, v) = cube_uv(d);
        let map = if rough < 0.3 { &self.sharp } else { &self.soft };
        let texels = &map[face];
        let n = self.size as f32;
        let x = u * n - 0.5;
        let y = v * n - 0.5;
        let (fx, fy) = (x.floor(), y.floor());
        let (tx, ty) = (x - fx, y - fy);
        let at = |ix: i64, iy: i64| -> V3 {
            let ix = ix.clamp(0, self.size as i64 - 1) as usize;
            let iy = iy.clamp(0, self.size as i64 - 1) as usize;
            texels[iy * self.size + ix]
        };
        let top = at(fx as i64, fy as i64) * (1.0 - tx) + at(fx as i64 + 1, fy as i64) * tx;
        let bot = at(fx as i64, fy as i64 + 1) * (1.0 - tx) + at(fx as i64 + 1, fy as i64 + 1) * tx;
        top * (1.0 - ty) + bot * ty
    }
}

/// A small blur over one face of the cube map: the reflection of brushed metal.
fn blur_face(face: &[V3], size: usize) -> Vec<V3> {
    let mut out = vec![V3::ZERO; face.len()];
    for j in 0..size {
        for i in 0..size {
            let mut acc = V3::ZERO;
            for dj in -1i64..=1 {
                for di in -1i64..=1 {
                    let x = (i as i64 + di).clamp(0, size as i64 - 1) as usize;
                    let y = (j as i64 + dj).clamp(0, size as i64 - 1) as usize;
                    acc += face[y * size + x];
                }
            }
            out[j * size + i] = acc * (1.0 / 9.0);
        }
    }
    out
}

/// One visible surface point.
pub struct Frag {
    /// Position relative to the eye.
    pub p: V3,
    pub n: V3,
    pub uv: [f32; 2],
    /// The size of one sample in `uv` units, for filtering the procedural parts.
    pub fp: f32,
    pub mat: Mat,
}

pub struct Shader<'a> {
    model: &'static Model,
    studio: Studio,
    env: Arc<EnvMap>,
    body: V3,
    glare: f32,
    texture: Option<Texture<'a>>,
}

fn rbox_sdf(p: [f32; 2], hw: f32, hh: f32, r: f32) -> f32 {
    let r = r.min(hw).min(hh);
    let q = [p[0].abs() - (hw - r), p[1].abs() - (hh - r)];
    let out = (q[0].max(0.0).powi(2) + q[1].max(0.0).powi(2)).sqrt();
    out + q[0].max(q[1]).min(0.0) - r
}

fn soft_clip(x: f32) -> f32 {
    if x < 0.85 {
        x
    } else {
        0.85 + 0.15 * (1.0 - (-(x - 0.85) / 0.15).exp())
    }
}

impl<'a> Shader<'a> {
    pub fn new(
        model: &'static Model,
        light_angle: f64,
        body: [u8; 3],
        glare: f64,
        texture: Option<Texture<'a>>,
    ) -> Shader<'a> {
        let lin = |c: u8| srgb_decode(f32::from(c) / 255.0);
        let studio = Studio::new(light_angle);
        let env = env_map(light_angle, &studio);
        Shader {
            model,
            studio,
            env,
            body: v3(lin(body[0]), lin(body[1]), lin(body[2])),
            glare: glare as f32,
            texture,
        }
    }

    /// Linear RGB for the point, 0..1 (not yet clamped for emissive screens).
    pub fn shade(&self, f: &Frag) -> V3 {
        let v = (-f.p).norm();
        let n = f.n;
        match f.mat {
            Mat::Front => self.front(f, n, v),
            Mat::Metal => {
                // A brushed finish: the roughness drifts along the frame, so the reflection has
                // the soft streaks of brushed metal instead of a perfect mirror.
                let brush = 0.05 * (f.uv[1] * 0.06).sin() + 0.02 * (f.uv[0] * 0.05).sin();
                self.metal(n, v, self.body, (0.38 + brush).max(0.05))
                    .map(soft_clip)
            }
            Mat::Back => self.back(n, v).map(soft_clip),
            Mat::Lens => self.lens(f, n, v).map(soft_clip),
            Mat::Deck => self.deck(f, n, v).map(soft_clip),
            Mat::Dark => {
                let r = reflect(v, n);
                (V3::splat(0.02)
                    + self.env.sample(r, 0.5) * (0.03 * (1.0 - n.dot(v).max(0.0) + 0.3)))
                    .map(soft_clip)
            }
        }
    }

    fn metal(&self, n: V3, v: V3, albedo: V3, rough: f32) -> V3 {
        let nv = n.dot(v).max(0.0);
        let r = reflect(v, n);
        // Anodised metal: a tinted diffuse base under a broad, slightly rough reflection.
        let f0 = albedo * 0.8 + V3::splat(0.05);
        let fr = (1.0 - nv).powi(5);
        let f = f0 + (V3::splat(1.0) - f0) * fr;
        let spec = f * self.env.sample(r, rough);
        let diff = albedo * self.studio.irradiance(n) * 0.34;
        let h = (self.studio.key + v).norm();
        let blinn = pow64(n.dot(h).max(0.0));
        diff + spec + f * (blinn * 1.2)
    }

    fn back(&self, n: V3, v: V3) -> V3 {
        let nv = n.dot(v).max(0.0);
        let r = reflect(v, n);
        let fr = 0.05 + 0.95 * (1.0 - nv).powi(5);
        let albedo = self.body * 0.6 + V3::splat(0.015);
        let h = (self.studio.key + v).norm();
        albedo * self.studio.irradiance(n) * 0.75
            + self.env.sample(r, 0.42) * (fr * 1.1)
            + V3::splat(pow16(n.dot(h).max(0.0)) * 0.07)
    }

    /// Fresnel reflection of the studio in glass.
    fn glass_reflection(&self, n: V3, v: V3, rough: f32) -> V3 {
        let nv = n.dot(v).max(0.0);
        let f = 0.04 + 0.96 * (1.0 - nv).powi(5);
        self.env.sample(reflect(v, n), rough) * f
    }

    fn lens(&self, f: &Frag, n: V3, v: V3) -> V3 {
        let rad = (f.uv[0] * f.uv[0] + f.uv[1] * f.uv[1]).sqrt();
        let nv = n.dot(v).max(0.0);
        let coat = v3(0.004, 0.006, 0.014)
            + v3(0.012, 0.004, 0.03) * ((1.0 - nv) * smoothstep(0.2, 0.9, rad));
        let f0 = 0.07 + 0.93 * (1.0 - nv).powi(5);
        coat + self.env.sample(reflect(v, n), 0.02) * (f0 * 1.8)
    }

    fn front(&self, f: &Frag, n: V3, v: V3) -> V3 {
        let m = self.model;
        let s: &ScreenRect = &m.screen;
        let p = [f.uv[0] - s.cx, f.uv[1] - s.cy];
        let mut sdf = rbox_sdf(p, s.hw, s.hh, s.radius);
        match m.cutout {
            Cutout::None => {}
            Cutout::Pill { cx, cy, hw, hh } => {
                sdf = sdf.max(-rbox_sdf([f.uv[0] - cx, f.uv[1] - cy], hw, hh, hh));
            }
            Cutout::Circle { cx, cy, r } => {
                let d = ((f.uv[0] - cx).powi(2) + (f.uv[1] - cy).powi(2)).sqrt() - r;
                sdf = sdf.max(-d);
            }
        }
        let fp = f.fp.max(1e-5);
        let cover = saturate(0.5 - sdf / fp);
        let bezel = V3::splat(0.006) + self.glass_reflection(n, v, 0.03) * 1.1;
        let mut col = bezel;
        if cover > 0.0 {
            let screen = match &self.texture {
                Some(t) => {
                    let u = (p[0] + s.hw) / (2.0 * s.hw);
                    let w = (s.hh - p[1]) / (2.0 * s.hh);
                    let per_uv = t.size().0 as f32 / (2.0 * s.hw);
                    let lod = (fp * per_uv).max(1e-4).log2();
                    let tex = t.sample(u, w, lod);
                    let bright = 0.9 + 0.1 * n.dot(v).max(0.0);
                    tex * bright + self.glass_reflection(n, v, 0.1) * (0.1 + self.glare * 1.5)
                }
                None => {
                    V3::splat(0.012) + self.glass_reflection(n, v, 0.1) * (1.0 + self.glare * 1.5)
                }
            };
            col = mix(col, screen, cover);
        }
        if let Some([x, y, r]) = m.dot {
            let d = ((f.uv[0] - x).powi(2) + (f.uv[1] - y).powi(2)).sqrt() - r;
            let a = saturate(0.5 - d / fp);
            if a > 0.0 {
                let lens = v3(0.004, 0.006, 0.02) + self.glass_reflection(n, v, 0.02) * 1.5;
                col = mix(col, lens, a);
            }
        }
        col
    }

    fn deck(&self, f: &Frag, n: V3, v: V3) -> V3 {
        let base = self.metal(n, v, self.body, 0.45);
        let Some(k) = &self.model.keyboard else {
            return base;
        };
        let fp = f.fp.max(1e-5);
        let (x, y) = (f.uv[0], f.uv[1]);
        let mut key_sdf = 1.0e3f32;
        for row in &k.rows {
            if (y - row.yc).abs() <= row.band {
                let col = ((x - k.x0) / k.pitch).floor();
                if col >= 0.0 && (col as usize) < k.cols {
                    let cx = k.x0 + (col + 0.5) * k.pitch;
                    key_sdf = rbox_sdf([x - cx, y - row.yc], k.half_w, row.half_h, 2.2);
                }
                break;
            }
        }
        let cover = saturate(0.5 - key_sdf / fp);
        let mut col = base;
        if cover > 0.0 {
            let r = reflect(v, n);
            let cap = V3::splat(0.011)
                + self.env.sample(r, 0.4) * 0.05
                + V3::splat(0.012 * smoothstep(-1.2, 0.0, key_sdf));
            col = mix(col, cap, cover);
        }
        let pad = &k.pad;
        let ps = rbox_sdf([x - pad.cx, y - pad.cy], pad.hw, pad.hh, pad.radius);
        let groove = saturate(0.5 - (ps.abs() - 0.16) / fp);
        let inside = saturate(0.5 - ps / fp);
        col = mix(
            col,
            col * 0.97 + self.glass_reflection(n, v, 0.2) * 0.25,
            inside,
        );
        mix(col, col * 0.35, groove)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn a_rounded_box_is_measured_from_its_edge() {
        assert!(rbox_sdf([0.0, 0.0], 10.0, 20.0, 3.0) < 0.0);
        assert!(rbox_sdf([11.0, 0.0], 10.0, 20.0, 3.0) > 0.0);
        assert!((rbox_sdf([10.0, 0.0], 10.0, 20.0, 3.0) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn a_soft_box_lights_only_its_own_side() {
        let box_ = SoftBox::new(v3(0.0, 1.0, 0.0), [0.5, 0.5], 0.2, V3::splat(1.0), true);
        assert!(box_.eval(v3(0.0, 1.0, 0.0), 0.0).x > 0.5);
        assert_eq!(box_.eval(v3(0.0, -1.0, 0.0), 0.0), V3::ZERO);
    }
}

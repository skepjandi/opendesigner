//! Triangle meshes and the generators the device models are assembled from: swept rounded slabs
//! (bodies, lids, panels), cylinders and domes (camera lenses).

use std::f32::consts::{FRAC_PI_2, TAU};

use super::math::{Mat3, V3, v3};

/// What a triangle is made of; the fragment stage shades each differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mat {
    /// Anodised metal frame and lid, tinted by the body colour.
    Metal,
    /// Frosted glass back, tinted by the body colour.
    Back,
    /// Front glass: screen, black bezel and cut-outs, decided per fragment from `uv`.
    Front,
    /// Dark glossy camera glass; `uv` is the position over the lens radius.
    Lens,
    /// A laptop deck: keys and trackpad, drawn per fragment from `uv`.
    Deck,
    /// Dark rubber and plastic.
    Dark,
}

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub p: V3,
    pub n: V3,
    /// Planar coordinates in the generating slab's own frame (millimetres), or the radial
    /// position over a lens.
    pub uv: [f32; 2],
}

#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub i: [u32; 3],
    pub mat: Mat,
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub v: Vec<Vertex>,
    pub t: Vec<Tri>,
}

impl Mesh {
    /// `p -> rot * p + offset`; normals are rotated only. Rotations must be proper (no mirror).
    pub fn transform(&mut self, rot: &Mat3, offset: V3) {
        for v in &mut self.v {
            v.p = rot.apply_v3(v.p) + offset;
            v.n = rot.apply_v3(v.n).norm();
        }
    }

    pub fn append(&mut self, other: Mesh) {
        let base = self.v.len() as u32;
        self.v.extend(other.v);
        self.t.extend(other.t.into_iter().map(|t| Tri {
            i: [t.i[0] + base, t.i[1] + base, t.i[2] + base],
            mat: t.mat,
        }));
    }

    fn push(&mut self, p: V3, n: V3, uv: [f32; 2]) -> u32 {
        self.v.push(Vertex { p, n, uv });
        (self.v.len() - 1) as u32
    }

    fn tri(&mut self, a: u32, b: u32, c: u32, mat: Mat) {
        self.t.push(Tri { i: [a, b, c], mat });
    }
}

/// A point of a closed 2D outline and its outward normal.
#[derive(Clone, Copy)]
pub struct OutlinePoint {
    pub p: [f32; 2],
    pub n: [f32; 2],
}

/// A rounded rectangle centred on the origin, counter-clockwise, `seg + 1` points per corner.
pub fn rounded_rect(w: f32, h: f32, r: f32, seg: usize) -> Vec<OutlinePoint> {
    let r = r.clamp(0.01, w.min(h) * 0.5);
    let (cx, cy) = (w * 0.5 - r, h * 0.5 - r);
    let mut out = Vec::with_capacity(4 * (seg + 1));
    for (corner, (sx, sy)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
        .into_iter()
        .enumerate()
    {
        for k in 0..=seg {
            let a = (corner as f32 + k as f32 / seg as f32) * FRAC_PI_2;
            let (s, c) = a.sin_cos();
            out.push(OutlinePoint {
                p: [sx * cx + r * c, sy * cy + r * s],
                n: [c, s],
            });
        }
    }
    out
}

/// One ring of a slab's cross-section: how far inside the outline, the height, and the normal as
/// (outward component, z component).
#[derive(Clone, Copy)]
pub struct Ring {
    pub inset: f32,
    pub z: f32,
    pub n_out: f32,
    pub n_z: f32,
}

pub struct Profile {
    pub rings: Vec<Ring>,
    /// Material of each strip between consecutive rings.
    pub strips: Vec<Mat>,
    pub front: Mat,
    pub back: Mat,
}

/// Flat faces, a rounded edge of radius `e`, a straight wall: the section of a phone or panel
/// of thickness `t`.
pub fn edge_profile(t: f32, e: f32, arc: usize, front: Mat, side: Mat, back: Mat) -> Profile {
    let h = t * 0.5;
    let e = e.min(h * 0.999);
    let mut rings = vec![Ring {
        inset: e,
        z: h,
        n_out: 0.0,
        n_z: 1.0,
    }];
    for k in 1..=arc {
        let a = k as f32 / arc as f32 * FRAC_PI_2;
        rings.push(Ring {
            inset: e - e * a.sin(),
            z: h - e + e * a.cos(),
            n_out: a.sin(),
            n_z: a.cos(),
        });
    }
    rings.push(Ring {
        inset: 0.0,
        z: -(h - e),
        n_out: 1.0,
        n_z: 0.0,
    });
    for k in 1..=arc {
        let a = k as f32 / arc as f32 * FRAC_PI_2;
        rings.push(Ring {
            inset: e - e * a.cos(),
            z: -(h - e) - e * a.sin(),
            n_out: a.cos(),
            n_z: -a.sin(),
        });
    }
    let strips = vec![side; rings.len() - 1];
    Profile {
        rings,
        strips,
        front,
        back,
    }
}

/// Sweeps `profile` around `outline`: a closed solid, front cap +z, centred on the origin.
pub fn slab(outline: &[OutlinePoint], profile: &Profile) -> Mesh {
    let mut m = Mesh::default();
    let n = outline.len();
    let rings = profile.rings.len();
    for ring in &profile.rings {
        for o in outline {
            let p = [o.p[0] - o.n[0] * ring.inset, o.p[1] - o.n[1] * ring.inset];
            m.push(
                v3(p[0], p[1], ring.z),
                v3(o.n[0] * ring.n_out, o.n[1] * ring.n_out, ring.n_z).norm(),
                p,
            );
        }
    }
    let at = |j: usize, i: usize| (j * n + i % n) as u32;
    for (j, &mat) in profile.strips.iter().enumerate() {
        for i in 0..n {
            let (a, b, c, d) = (at(j, i), at(j, i + 1), at(j + 1, i + 1), at(j + 1, i));
            m.tri(a, c, b, mat);
            m.tri(a, d, c, mat);
        }
    }
    let top = &profile.rings[0];
    let bot = &profile.rings[rings - 1];
    let cf = m.push(v3(0.0, 0.0, top.z), v3(0.0, 0.0, 1.0), [0.0, 0.0]);
    let cb = m.push(v3(0.0, 0.0, bot.z), v3(0.0, 0.0, -1.0), [0.0, 0.0]);
    for i in 0..n {
        m.tri(cf, at(0, i), at(0, i + 1), profile.front);
        m.tri(cb, at(rings - 1, i + 1), at(rings - 1, i), profile.back);
    }
    m
}

/// A cylinder on the xy plane from `z0` to `z1` with a flat top: lens barrels. `uv` is the radial
/// position (so a lens top can be shaded by radius).
pub fn cylinder(c: [f32; 2], r: f32, z0: f32, z1: f32, seg: usize, top: Mat, side: Mat) -> Mesh {
    let mut m = Mesh::default();
    for k in 0..seg {
        let a = k as f32 / seg as f32 * TAU;
        let (s, co) = a.sin_cos();
        let (x, y) = (c[0] + r * co, c[1] + r * s);
        m.push(v3(x, y, z0), v3(co, s, 0.0), [co, s]);
        m.push(v3(x, y, z1), v3(co, s, 0.0), [co, s]);
    }
    for k in 0..seg {
        let (a, b) = ((2 * k) as u32, (2 * ((k + 1) % seg)) as u32);
        m.tri(a, b, b + 1, side);
        m.tri(a, b + 1, a + 1, side);
    }
    let c1 = m.push(v3(c[0], c[1], z1), v3(0.0, 0.0, 1.0), [0.0, 0.0]);
    for k in 0..seg {
        let (a, b) = (2 * k as u32 + 1, 2 * ((k + 1) % seg) as u32 + 1);
        m.tri(c1, a, b, top);
    }
    m
}

/// A shallow dome of radius `r` and height `h` over `z`: a lens surface, with the analytic
/// normals of its paraboloid so it catches the studio's reflections as a curved lens does.
pub fn dome(c: [f32; 2], r: f32, z: f32, h: f32, seg: usize, rings: usize, mat: Mat) -> Mesh {
    let mut m = Mesh::default();
    let apex = m.push(v3(c[0], c[1], z + h), v3(0.0, 0.0, 1.0), [0.0, 0.0]);
    for j in 1..=rings {
        let rho = j as f32 / rings as f32;
        for k in 0..seg {
            let a = k as f32 / seg as f32 * TAU;
            let (s, co) = a.sin_cos();
            let dz = h * (1.0 - rho * rho);
            // z = h (1 - rho^2): the slope along the radius is -2 h rho / r.
            let slope = 2.0 * h * rho / r;
            m.push(
                v3(c[0] + r * rho * co, c[1] + r * rho * s, z + dz),
                v3(slope * co, slope * s, 1.0).norm(),
                [rho * co, rho * s],
            );
        }
    }
    let at = |j: usize, k: usize| (1 + (j - 1) * seg + k % seg) as u32;
    for k in 0..seg {
        m.tri(apex, at(1, k), at(1, k + 1), mat);
    }
    for j in 1..rings {
        for k in 0..seg {
            let (a, b, c2, d) = (at(j, k), at(j, k + 1), at(j + 1, k + 1), at(j + 1, k));
            m.tri(a, d, c2, mat);
            m.tri(a, c2, b, mat);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    /// Every edge of a closed solid is used once in each direction.
    fn is_closed(m: &Mesh) -> bool {
        let mut edges: Vec<(u32, u32)> = Vec::new();
        for t in &m.t {
            for k in 0..3 {
                edges.push((t.i[k], t.i[(k + 1) % 3]));
            }
        }
        edges.sort_unstable();
        let mut ok = true;
        let mut i = 0;
        while i < edges.len() {
            let (a, b) = edges[i];
            let j = edges[i..].iter().position(|e| *e == (b, a));
            match j {
                Some(j) if j > 0 => i += j + 1,
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        ok
    }

    #[test]
    fn a_rounded_rectangle_is_the_size_it_says() {
        let o = rounded_rect(10.0, 20.0, 3.0, 8);
        for p in &o {
            assert!(p.p[0].abs() <= 5.0 + 1e-4 && p.p[1].abs() <= 10.0 + 1e-4);
            assert!((p.n[0] * p.n[0] + p.n[1] * p.n[1] - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn a_slab_is_a_closed_solid() {
        let m = slab(
            &rounded_rect(10.0, 20.0, 2.0, 6),
            &edge_profile(2.0, 0.4, 3, Mat::Front, Mat::Metal, Mat::Back),
        );
        assert!(is_closed(&m), "the slab is not closed");
        assert!(m.t.iter().all(|t| t.mat != Mat::Front || t.i[0] != t.i[1]));
    }
}

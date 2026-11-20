//! Small vector and matrix helpers for the 3D renderer.

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    pub const ZERO: V3 = v3(0.0, 0.0, 0.0);

    pub const fn splat(v: f32) -> V3 {
        v3(v, v, v)
    }

    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: V3) -> V3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// Unit length; the zero vector stays zero.
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l > 1e-20 {
            self * (1.0 / l)
        } else {
            V3::ZERO
        }
    }

    pub fn map(self, f: impl Fn(f32) -> f32) -> V3 {
        v3(f(self.x), f(self.y), f(self.z))
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl AddAssign for V3 {
    fn add_assign(&mut self, o: V3) {
        *self = *self + o;
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, s: f32) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Mul<V3> for V3 {
    type Output = V3;
    fn mul(self, o: V3) -> V3 {
        v3(self.x * o.x, self.y * o.y, self.z * o.z)
    }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// `x` to the 64th power by repeated squaring: a sharp highlight without a `powf` call.
pub fn pow64(x: f32) -> f32 {
    let x2 = x * x;
    let x4 = x2 * x2;
    let x8 = x4 * x4;
    let x16 = x8 * x8;
    let x32 = x16 * x16;
    x32 * x32
}

/// `x` to the 16th power by repeated squaring.
pub fn pow16(x: f32) -> f32 {
    let x2 = x * x;
    let x4 = x2 * x2;
    let x8 = x4 * x4;
    x8 * x8
}

pub fn mix(a: V3, b: V3, t: f32) -> V3 {
    a + (b - a) * t
}

pub fn saturate(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = saturate((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

/// `n` reflected about the surface normal for an incident direction `-v` (v points at the eye).
pub fn reflect(v: V3, n: V3) -> V3 {
    n * (2.0 * n.dot(v)) - v
}

/// Row-major 3x3 rotation, in f64 because poses are accumulated from degrees.
#[derive(Clone, Copy, Debug)]
pub struct Mat3(pub [[f64; 3]; 3]);

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    pub fn rot_x(rad: f64) -> Mat3 {
        let (s, c) = rad.sin_cos();
        Mat3([[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]])
    }
    pub fn rot_y(rad: f64) -> Mat3 {
        let (s, c) = rad.sin_cos();
        Mat3([[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]])
    }
    pub fn rot_z(rad: f64) -> Mat3 {
        let (s, c) = rad.sin_cos();
        Mat3([[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]])
    }

    /// From three column vectors.
    pub fn from_cols(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Mat3 {
        Mat3([[a[0], b[0], c[0]], [a[1], b[1], c[1]], [a[2], b[2], c[2]]])
    }

    pub fn mul(&self, o: &Mat3) -> Mat3 {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.0[i][k] * o.0[k][j]).sum();
            }
        }
        Mat3(r)
    }

    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let m = &self.0;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2],
        ]
    }

    pub fn apply_v3(&self, p: V3) -> V3 {
        let r = self.apply([f64::from(p.x), f64::from(p.y), f64::from(p.z)]);
        v3(r[0] as f32, r[1] as f32, r[2] as f32)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn a_reflection_keeps_its_length() {
        let n = v3(0.0, 0.0, 1.0);
        let v = v3(0.3, -0.2, 1.0).norm();
        let r = reflect(v, n);
        assert!((r.len() - 1.0).abs() < 1e-5);
        // The part along the normal stays, the part across it is mirrored.
        let along = |u: V3| u - n * n.dot(u);
        assert!(
            (along(r) + along(v)).len() < 1e-5,
            "the reflection is not mirrored"
        );
        assert!(
            r.z > 0.0,
            "the reflection does not come back towards the eye"
        );
    }

    #[test]
    fn rotations_keep_length() {
        for m in [Mat3::rot_x(0.7), Mat3::rot_y(-1.2), Mat3::rot_z(2.4)] {
            let p = [0.3, -0.7, 0.5];
            let r = m.apply(p);
            let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
            let want = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((l - want).abs() < 1e-9, "rotation changed the length");
        }
    }
}

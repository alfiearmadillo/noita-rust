//! `b2Math.h` / `b2Math.cpp`. Operation order follows the C++ so Real results match.

use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

use crate::settings::EPSILON;

/// Noita's Box2D build uses double precision for loat32; float literals stay float-valued.
pub type Real = f64;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: Real,
    pub y: Real,
}

#[inline]
pub const fn vec2(x: Real, y: Real) -> Vec2 {
    Vec2 { x, y }
}

impl Vec2 {
    pub const ZERO: Vec2 = vec2(0.0, 0.0);

    pub fn length(self) -> Real {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn length_squared(self) -> Real {
        self.x * self.x + self.y * self.y
    }

    /// Returns the length before normalizing; a vector shorter than epsilon is left unchanged.
    pub fn normalize(&mut self) -> Real {
        let length = self.length();
        if length < EPSILON {
            return 0.0;
        }
        let inv = 1.0 / length;
        self.x *= inv;
        self.y *= inv;
        length
    }

    pub fn skew(self) -> Vec2 {
        vec2(-self.y, self.x)
    }

    pub fn is_valid(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Component `i` (0 = x, 1 = y).
    pub fn get(self, i: usize) -> Real {
        if i == 0 { self.x } else { self.y }
    }

    pub fn set(&mut self, i: usize, v: Real) {
        if i == 0 {
            self.x = v;
        } else {
            self.y = v;
        }
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        vec2(-self.x, -self.y)
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        vec2(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        vec2(self.x - o.x, self.y - o.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        self.x += o.x;
        self.y += o.y;
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, o: Vec2) {
        self.x -= o.x;
        self.y -= o.y;
    }
}

/// `s * v`
impl Mul<Vec2> for Real {
    type Output = Vec2;
    fn mul(self, a: Vec2) -> Vec2 {
        vec2(self * a.x, self * a.y)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: Real,
    pub y: Real,
    pub z: Real,
}

#[inline]
pub const fn vec3(x: Real, y: Real, z: Real) -> Vec3 {
    Vec3 { x, y, z }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        vec3(-self.x, -self.y, -self.z)
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        vec3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        vec3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        self.x += o.x;
        self.y += o.y;
        self.z += o.z;
    }
}

impl Mul<Vec3> for Real {
    type Output = Vec3;
    fn mul(self, a: Vec3) -> Vec3 {
        vec3(self * a.x, self * a.y, self * a.z)
    }
}

/// 2x2 matrix, columns `ex` and `ey`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mat22 {
    pub ex: Vec2,
    pub ey: Vec2,
}

impl Mat22 {
    pub fn from_cols(ex: Vec2, ey: Vec2) -> Mat22 {
        Mat22 { ex, ey }
    }

    pub fn new(a11: Real, a12: Real, a21: Real, a22: Real) -> Mat22 {
        Mat22 {
            ex: vec2(a11, a21),
            ey: vec2(a12, a22),
        }
    }

    pub fn inverse(&self) -> Mat22 {
        let (a, b, c, d) = (self.ex.x, self.ey.x, self.ex.y, self.ey.y);
        let mut det = a * d - b * c;
        if det != 0.0 {
            det = 1.0 / det;
        }
        Mat22 {
            ex: vec2(det * d, -det * c),
            ey: vec2(-det * b, det * a),
        }
    }

    pub fn solve(&self, b: Vec2) -> Vec2 {
        let (a11, a12, a21, a22) = (self.ex.x, self.ey.x, self.ex.y, self.ey.y);
        let mut det = a11 * a22 - a12 * a21;
        if det != 0.0 {
            det = 1.0 / det;
        }
        vec2(det * (a22 * b.x - a12 * b.y), det * (a11 * b.y - a21 * b.x))
    }
}

/// 3x3 matrix, columns `ex`, `ey`, `ez`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mat33 {
    pub ex: Vec3,
    pub ey: Vec3,
    pub ez: Vec3,
}

impl Mat33 {
    pub fn solve33(&self, b: Vec3) -> Vec3 {
        let mut det = dot3(self.ex, cross3(self.ey, self.ez));
        if det != 0.0 {
            det = 1.0 / det;
        }
        vec3(
            det * dot3(b, cross3(self.ey, self.ez)),
            det * dot3(self.ex, cross3(b, self.ez)),
            det * dot3(self.ex, cross3(self.ey, b)),
        )
    }

    pub fn solve22(&self, b: Vec2) -> Vec2 {
        let (a11, a12, a21, a22) = (self.ex.x, self.ey.x, self.ex.y, self.ey.y);
        let mut det = a11 * a22 - a12 * a21;
        if det != 0.0 {
            det = 1.0 / det;
        }
        vec2(det * (a22 * b.x - a12 * b.y), det * (a11 * b.y - a21 * b.x))
    }

    pub fn inverse22(&self) -> Mat33 {
        let (a, b, c, d) = (self.ex.x, self.ey.x, self.ex.y, self.ey.y);
        let mut det = a * d - b * c;
        if det != 0.0 {
            det = 1.0 / det;
        }
        Mat33 {
            ex: vec3(det * d, -det * c, 0.0),
            ey: vec3(-det * b, det * a, 0.0),
            ez: vec3(0.0, 0.0, 0.0),
        }
    }

    pub fn sym_inverse33(&self) -> Mat33 {
        let mut det = dot3(self.ex, cross3(self.ey, self.ez));
        if det != 0.0 {
            det = 1.0 / det;
        }
        let (a11, a12, a13) = (self.ex.x, self.ey.x, self.ez.x);
        let (a22, a23) = (self.ey.y, self.ez.y);
        let a33 = self.ez.z;
        let mut m = Mat33::default();
        m.ex.x = det * (a22 * a33 - a23 * a23);
        m.ex.y = det * (a13 * a23 - a12 * a33);
        m.ex.z = det * (a12 * a23 - a13 * a22);
        m.ey.x = m.ex.y;
        m.ey.y = det * (a11 * a33 - a13 * a13);
        m.ey.z = det * (a13 * a12 - a11 * a23);
        m.ez.x = m.ex.z;
        m.ez.y = m.ey.z;
        m.ez.z = det * (a11 * a22 - a12 * a12);
        m
    }
}

/// Rotation stored as sine and cosine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rot {
    pub s: Real,
    pub c: Real,
}

impl Default for Rot {
    fn default() -> Self {
        Rot::IDENTITY
    }
}

impl Rot {
    pub const IDENTITY: Rot = Rot { s: 0.0, c: 1.0 };

    pub fn new(angle: Real) -> Rot {
        Rot {
            s: angle.sin(),
            c: angle.cos(),
        }
    }

    pub fn angle(&self) -> Real {
        self.s.atan2(self.c)
    }

    pub fn x_axis(&self) -> Vec2 {
        vec2(self.c, self.s)
    }

    pub fn y_axis(&self) -> Vec2 {
        vec2(-self.s, self.c)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transform {
    pub p: Vec2,
    pub q: Rot,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        p: Vec2::ZERO,
        q: Rot::IDENTITY,
    };

    pub fn new(p: Vec2, angle: Real) -> Transform {
        Transform {
            p,
            q: Rot::new(angle),
        }
    }
}

/// Motion of a body for TOI queries (`b2Sweep`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sweep {
    pub local_center: Vec2,
    pub c0: Vec2,
    pub c: Vec2,
    pub a0: Real,
    pub a: Real,
    pub alpha0: Real,
}

impl Sweep {
    pub fn transform(&self, beta: Real) -> Transform {
        let p = (1.0 - beta) * self.c0 + beta * self.c;
        let angle = (1.0 - beta) * self.a0 + beta * self.a;
        let q = Rot::new(angle);
        Transform {
            p: p - mul_rot(q, self.local_center),
            q,
        }
    }

    pub fn advance(&mut self, alpha: Real) {
        debug_assert!(self.alpha0 < 1.0);
        let beta = (alpha - self.alpha0) / (1.0 - self.alpha0);
        self.c0 += beta * (self.c - self.c0);
        self.a0 += beta * (self.a - self.a0);
        self.alpha0 = alpha;
    }

    pub fn normalize(&mut self) {
        // Noita normalizes with the double 2 * 3.14159265359, not the float pi.
        #[allow(clippy::approx_constant)]
        let two_pi = 2.0 * 3.14159265359;
        let d = two_pi * (self.a0 / two_pi).floor();
        self.a0 -= d;
        self.a -= d;
    }
}

#[inline]
pub fn dot(a: Vec2, b: Vec2) -> Real {
    a.x * b.x + a.y * b.y
}

/// 2D cross product of two vectors (a scalar).
#[inline]
pub fn cross(a: Vec2, b: Vec2) -> Real {
    a.x * b.y - a.y * b.x
}

/// `a x s`
#[inline]
pub fn cross_vs(a: Vec2, s: Real) -> Vec2 {
    vec2(s * a.y, -s * a.x)
}

/// `s x a`
#[inline]
pub fn cross_sv(s: Real, a: Vec2) -> Vec2 {
    vec2(-s * a.y, s * a.x)
}

#[inline]
pub fn dot3(a: Vec3, b: Vec3) -> Real {
    a.x * b.x + a.y * b.y + a.z * b.z
}

#[inline]
pub fn cross3(a: Vec3, b: Vec3) -> Vec3 {
    vec3(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

#[inline]
pub fn mul_mat22(a: &Mat22, v: Vec2) -> Vec2 {
    vec2(a.ex.x * v.x + a.ey.x * v.y, a.ex.y * v.x + a.ey.y * v.y)
}

#[inline]
pub fn mul_t_mat22(a: &Mat22, v: Vec2) -> Vec2 {
    vec2(dot(v, a.ex), dot(v, a.ey))
}

/// `A * B`
pub fn mul_mm(a: &Mat22, b: &Mat22) -> Mat22 {
    Mat22::from_cols(mul_mat22(a, b.ex), mul_mat22(a, b.ey))
}

/// `A^T * B`
pub fn mul_t_mm(a: &Mat22, b: &Mat22) -> Mat22 {
    Mat22::from_cols(
        vec2(dot(a.ex, b.ex), dot(a.ey, b.ex)),
        vec2(dot(a.ex, b.ey), dot(a.ey, b.ey)),
    )
}

#[inline]
pub fn mul_mat33(a: &Mat33, v: Vec3) -> Vec3 {
    v.x * a.ex + v.y * a.ey + v.z * a.ez
}

#[inline]
pub fn mul22_mat33(a: &Mat33, v: Vec2) -> Vec2 {
    vec2(a.ex.x * v.x + a.ey.x * v.y, a.ex.y * v.x + a.ey.y * v.y)
}

/// `q * r`
pub fn mul_rr(q: Rot, r: Rot) -> Rot {
    Rot {
        s: q.s * r.c + q.c * r.s,
        c: q.c * r.c - q.s * r.s,
    }
}

/// `q^T * r`
pub fn mul_t_rr(q: Rot, r: Rot) -> Rot {
    Rot {
        s: q.c * r.s - q.s * r.c,
        c: q.c * r.c + q.s * r.s,
    }
}

#[inline]
pub fn mul_rot(q: Rot, v: Vec2) -> Vec2 {
    vec2(q.c * v.x - q.s * v.y, q.s * v.x + q.c * v.y)
}

#[inline]
pub fn mul_t_rot(q: Rot, v: Vec2) -> Vec2 {
    vec2(q.c * v.x + q.s * v.y, -q.s * v.x + q.c * v.y)
}

#[inline]
pub fn mul_xf(t: &Transform, v: Vec2) -> Vec2 {
    let x = (t.q.c * v.x - t.q.s * v.y) + t.p.x;
    let y = (t.q.s * v.x + t.q.c * v.y) + t.p.y;
    vec2(x, y)
}

#[inline]
pub fn mul_t_xf(t: &Transform, v: Vec2) -> Vec2 {
    let px = v.x - t.p.x;
    let py = v.y - t.p.y;
    vec2(t.q.c * px + t.q.s * py, -t.q.s * px + t.q.c * py)
}

/// `A * B` for transforms.
pub fn mul_xx(a: &Transform, b: &Transform) -> Transform {
    Transform {
        q: mul_rr(a.q, b.q),
        p: mul_rot(a.q, b.p) + a.p,
    }
}

/// `A^T * B` for transforms.
pub fn mul_t_xx(a: &Transform, b: &Transform) -> Transform {
    Transform {
        q: mul_t_rr(a.q, b.q),
        p: mul_t_rot(a.q, b.p - a.p),
    }
}

#[inline]
pub fn distance(a: Vec2, b: Vec2) -> Real {
    (a - b).length()
}

#[inline]
pub fn distance_squared(a: Vec2, b: Vec2) -> Real {
    let c = a - b;
    dot(c, c)
}

/// `b2Abs` for vectors.
pub fn abs2(a: Vec2) -> Vec2 {
    vec2(abs(a.x), abs(a.y))
}

/// `b2Abs` for floats: `a > 0 ? a : -a`.
#[inline]
pub fn abs(a: Real) -> Real {
    if a > 0.0 { a } else { -a }
}

/// `a < b ? a : b`, as the C++ template (not `Real::min`, which differs on NaN).
#[inline]
pub fn min(a: Real, b: Real) -> Real {
    if a < b { a } else { b }
}

/// `a > b ? a : b`.
#[inline]
pub fn max(a: Real, b: Real) -> Real {
    if a > b { a } else { b }
}

pub fn min2(a: Vec2, b: Vec2) -> Vec2 {
    vec2(min(a.x, b.x), min(a.y, b.y))
}

pub fn max2(a: Vec2, b: Vec2) -> Vec2 {
    vec2(max(a.x, b.x), max(a.y, b.y))
}

/// `b2Max(low, b2Min(a, high))`
#[inline]
pub fn clamp(a: Real, low: Real, high: Real) -> Real {
    max(low, min(a, high))
}

pub fn clamp2(a: Vec2, low: Vec2, high: Vec2) -> Vec2 {
    max2(low, min2(a, high))
}

pub fn next_power_of_two(mut x: u32) -> u32 {
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    x.wrapping_add(1)
}

pub fn is_power_of_two(x: u32) -> bool {
    x > 0 && (x & (x - 1)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_round_trips_and_transforms_compose() {
        let q = Rot::new(0.7);
        let v = vec2(1.5, -2.0);
        let r = mul_t_rot(q, mul_rot(q, v));
        assert!((r.x - v.x).abs() < 1e-6 && (r.y - v.y).abs() < 1e-6);

        let a = Transform::new(vec2(1.0, 2.0), 0.3);
        let b = Transform::new(vec2(-0.5, 0.25), -1.1);
        let c = mul_xx(&a, &b);
        let direct = mul_xf(&a, mul_xf(&b, v));
        let composed = mul_xf(&c, v);
        assert!((direct.x - composed.x).abs() < 1e-5 && (direct.y - composed.y).abs() < 1e-5);
    }

    #[test]
    fn solves_match_inverses() {
        let m = Mat22::new(2.0, 1.0, 0.5, 3.0);
        let b = vec2(1.0, -1.0);
        let x = m.solve(b);
        let back = mul_mat22(&m, x);
        assert!((back.x - b.x).abs() < 1e-6 && (back.y - b.y).abs() < 1e-6);
        let inv = m.inverse();
        let y = mul_mat22(&inv, b);
        assert!((x.x - y.x).abs() < 1e-6 && (x.y - y.y).abs() < 1e-6);
    }

    #[test]
    fn normalize_ignores_tiny_vectors() {
        let mut v = vec2(1e-9, 0.0);
        assert_eq!(v.normalize(), 0.0);
        let mut w = vec2(3.0, 4.0);
        assert_eq!(w.normalize(), 5.0);
        assert!((w.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn power_of_two_helpers() {
        assert_eq!(next_power_of_two(5), 8);
        assert!(is_power_of_two(64) && !is_power_of_two(0) && !is_power_of_two(12));
    }
}

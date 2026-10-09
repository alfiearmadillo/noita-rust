//! Shapes and AABBs: `b2Shape`, `b2CircleShape`, `b2PolygonShape`, `b2AABB` (Box2D 2.3.1).

use crate::math::*;
use crate::settings::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MassData {
    pub mass: Real,
    pub center: Vec2,
    /// Rotational inertia about the local origin.
    pub i: Real,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aabb {
    pub lower_bound: Vec2,
    pub upper_bound: Vec2,
}

impl Aabb {
    pub fn is_valid(&self) -> bool {
        let d = self.upper_bound - self.lower_bound;
        d.x >= 0.0 && d.y >= 0.0 && self.lower_bound.is_valid() && self.upper_bound.is_valid()
    }

    pub fn center(&self) -> Vec2 {
        0.5 * (self.lower_bound + self.upper_bound)
    }

    pub fn extents(&self) -> Vec2 {
        0.5 * (self.upper_bound - self.lower_bound)
    }

    pub fn perimeter(&self) -> Real {
        let wx = self.upper_bound.x - self.lower_bound.x;
        let wy = self.upper_bound.y - self.lower_bound.y;
        2.0 * (wx + wy)
    }

    pub fn combine(&mut self, other: &Aabb) {
        self.lower_bound = min2(self.lower_bound, other.lower_bound);
        self.upper_bound = max2(self.upper_bound, other.upper_bound);
    }

    pub fn combined(a: &Aabb, b: &Aabb) -> Aabb {
        Aabb {
            lower_bound: min2(a.lower_bound, b.lower_bound),
            upper_bound: max2(a.upper_bound, b.upper_bound),
        }
    }

    pub fn contains(&self, o: &Aabb) -> bool {
        self.lower_bound.x <= o.lower_bound.x
            && self.lower_bound.y <= o.lower_bound.y
            && o.upper_bound.x <= self.upper_bound.x
            && o.upper_bound.y <= self.upper_bound.y
    }

    /// Slab test (Real-Time Collision Detection p179).
    pub fn ray_cast(&self, input: &RayCastInput) -> Option<RayCastOutput> {
        let mut tmin = -FLOAT_MAX;
        let mut tmax = FLOAT_MAX;
        let p = input.p1;
        let d = input.p2 - input.p1;
        let abs_d = abs2(d);
        let mut normal = Vec2::ZERO;
        for i in 0..2 {
            if abs_d.get(i) < EPSILON {
                if p.get(i) < self.lower_bound.get(i) || self.upper_bound.get(i) < p.get(i) {
                    return None;
                }
            } else {
                let inv_d = 1.0 / d.get(i);
                let mut t1 = (self.lower_bound.get(i) - p.get(i)) * inv_d;
                let mut t2 = (self.upper_bound.get(i) - p.get(i)) * inv_d;
                let mut s = -1.0;
                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                    s = 1.0;
                }
                if t1 > tmin {
                    normal = Vec2::ZERO;
                    normal.set(i, s);
                    tmin = t1;
                }
                tmax = min(tmax, t2);
                if tmin > tmax {
                    return None;
                }
            }
        }
        if tmin < 0.0 || input.max_fraction < tmin {
            return None;
        }
        Some(RayCastOutput {
            normal,
            fraction: tmin,
        })
    }
}

pub fn test_overlap_aabb(a: &Aabb, b: &Aabb) -> bool {
    let d1 = b.lower_bound - a.upper_bound;
    let d2 = a.lower_bound - b.upper_bound;
    !(d1.x > 0.0 || d1.y > 0.0 || d2.x > 0.0 || d2.y > 0.0)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RayCastInput {
    pub p1: Vec2,
    pub p2: Vec2,
    pub max_fraction: Real,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RayCastOutput {
    pub normal: Vec2,
    pub fraction: Real,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleShape {
    pub radius: Real,
    pub p: Vec2,
}

impl CircleShape {
    pub fn new(radius: Real) -> Self {
        CircleShape {
            radius,
            p: Vec2::ZERO,
        }
    }

    pub fn test_point(&self, xf: &Transform, p: Vec2) -> bool {
        let center = xf.p + mul_rot(xf.q, self.p);
        let d = p - center;
        dot(d, d) <= self.radius * self.radius
    }

    pub fn ray_cast(&self, input: &RayCastInput, xf: &Transform) -> Option<RayCastOutput> {
        let position = xf.p + mul_rot(xf.q, self.p);
        let s = input.p1 - position;
        let b = dot(s, s) - self.radius * self.radius;
        let r = input.p2 - input.p1;
        let c = dot(s, r);
        let rr = dot(r, r);
        let sigma = c * c - rr * b;
        if sigma < 0.0 || rr < EPSILON {
            return None;
        }
        let mut a = -(c + sigma.sqrt());
        if 0.0 <= a && a <= input.max_fraction * rr {
            a /= rr;
            let mut normal = s + a * r;
            normal.normalize();
            return Some(RayCastOutput {
                normal,
                fraction: a,
            });
        }
        None
    }

    pub fn compute_aabb(&self, xf: &Transform) -> Aabb {
        let p = xf.p + mul_rot(xf.q, self.p);
        Aabb {
            lower_bound: vec2(p.x - self.radius, p.y - self.radius),
            upper_bound: vec2(p.x + self.radius, p.y + self.radius),
        }
    }

    pub fn compute_mass(&self, density: Real) -> MassData {
        let mass = density * PI * self.radius * self.radius;
        MassData {
            mass,
            center: self.p,
            i: mass * (0.5 * self.radius * self.radius + dot(self.p, self.p)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolygonShape {
    pub centroid: Vec2,
    pub vertices: [Vec2; MAX_POLYGON_VERTICES],
    pub normals: [Vec2; MAX_POLYGON_VERTICES],
    pub count: usize,
}

impl Default for PolygonShape {
    fn default() -> Self {
        PolygonShape {
            centroid: Vec2::ZERO,
            vertices: [Vec2::ZERO; MAX_POLYGON_VERTICES],
            normals: [Vec2::ZERO; MAX_POLYGON_VERTICES],
            count: 0,
        }
    }
}

fn compute_centroid(vs: &[Vec2]) -> Vec2 {
    let count = vs.len();
    let mut c = Vec2::ZERO;
    let mut area = 0.0;
    let p_ref = Vec2::ZERO;
    let inv3 = (1.0f32 / 3.0f32) as Real;
    for i in 0..count {
        let p1 = p_ref;
        let p2 = vs[i];
        let p3 = if i + 1 < count { vs[i + 1] } else { vs[0] };
        let e1 = p2 - p1;
        let e2 = p3 - p1;
        let d = cross(e1, e2);
        let triangle_area = 0.5 * d;
        area += triangle_area;
        c += (triangle_area * inv3) * (p1 + p2 + p3);
    }
    // Noita returns the first vertex for a degenerate (area <= FLT_EPSILON) polygon.
    if area > EPSILON {
        (1.0 / area) * c
    } else {
        vs[0]
    }
}

impl PolygonShape {
    pub fn radius(&self) -> Real {
        POLYGON_RADIUS
    }

    pub fn as_box(hx: Real, hy: Real) -> Self {
        let mut s = PolygonShape {
            count: 4,
            ..Default::default()
        };
        s.vertices[0] = vec2(-hx, -hy);
        s.vertices[1] = vec2(hx, -hy);
        s.vertices[2] = vec2(hx, hy);
        s.vertices[3] = vec2(-hx, hy);
        s.normals[0] = vec2(0.0, -1.0);
        s.normals[1] = vec2(1.0, 0.0);
        s.normals[2] = vec2(0.0, 1.0);
        s.normals[3] = vec2(-1.0, 0.0);
        s
    }

    pub fn as_oriented_box(hx: Real, hy: Real, center: Vec2, angle: Real) -> Self {
        let mut s = PolygonShape::as_box(hx, hy);
        s.centroid = center;
        let xf = Transform::new(center, angle);
        for i in 0..s.count {
            s.vertices[i] = mul_xf(&xf, s.vertices[i]);
            s.normals[i] = mul_rot(xf.q, s.normals[i]);
        }
        s
    }

    /// Convex hull by gift wrapping, with welding of near-duplicate points. Fewer than 3 inputs
    /// give a +-1 box; a degenerate hull gives a unit box whose corner is the first point.
    pub fn from_points(points: &[Vec2]) -> Self {
        if points.len() < 3 {
            return PolygonShape::as_box(1.0, 1.0);
        }
        // Squared weld distance, computed in float like the original.
        let weld_sqr = (0.5f32 * 0.005f32 * (0.5f32 * 0.005f32)) as Real;
        let n = points.len().min(MAX_POLYGON_VERTICES);
        let mut ps = [Vec2::ZERO; MAX_POLYGON_VERTICES];
        let mut temp_count = 0;
        for &v in &points[..n] {
            let unique = ps[..temp_count]
                .iter()
                .all(|&p| distance_squared(v, p) >= weld_sqr);
            if unique {
                ps[temp_count] = v;
                temp_count += 1;
            }
        }
        let n = temp_count;
        if n < 3 {
            return PolygonShape::as_box(1.0, 1.0);
        }

        let mut i0 = 0;
        let mut x0 = ps[0].x;
        for (i, p) in ps.iter().enumerate().take(n).skip(1) {
            if p.x > x0 || (p.x == x0 && p.y < ps[i0].y) {
                i0 = i;
                x0 = p.x;
            }
        }

        let mut hull = [0usize; MAX_POLYGON_VERTICES];
        let mut m = 0;
        let mut ih = i0;
        loop {
            hull[m] = ih;
            let mut ie = 0;
            for j in 1..n {
                if ie == ih {
                    ie = j;
                    continue;
                }
                let r = ps[ie] - ps[hull[m]];
                let v = ps[j] - ps[hull[m]];
                let c = cross(r, v);
                if c < 0.0 {
                    ie = j;
                }
                if c == 0.0 && v.length_squared() > r.length_squared() {
                    ie = j;
                }
            }
            m += 1;
            ih = ie;
            if ie == i0 {
                break;
            }
            if m >= MAX_POLYGON_VERTICES {
                break;
            }
        }
        if m < 3 {
            let (x, y) = (ps[0].x, ps[0].y);
            let mut s = PolygonShape {
                count: 4,
                centroid: vec2(x + 0.5, y + 0.5),
                ..Default::default()
            };
            s.vertices[..4].copy_from_slice(&[
                vec2(x, y),
                vec2(x + 1.0, y),
                vec2(x + 1.0, y + 1.0),
                vec2(x, y + 1.0),
            ]);
            s.normals[..4].copy_from_slice(&[
                vec2(0.0, -1.0),
                vec2(1.0, 0.0),
                vec2(0.0, 1.0),
                vec2(-1.0, 0.0),
            ]);
            return s;
        }

        let mut s = PolygonShape {
            count: m,
            ..Default::default()
        };
        for i in 0..m {
            s.vertices[i] = ps[hull[i]];
        }
        for i in 0..m {
            let i2 = if i + 1 < m { i + 1 } else { 0 };
            let edge = s.vertices[i2] - s.vertices[i];
            s.normals[i] = cross_vs(edge, 1.0);
            s.normals[i].normalize();
        }
        s.centroid = compute_centroid(&s.vertices[..m]);
        s
    }

    pub fn vertices(&self) -> &[Vec2] {
        &self.vertices[..self.count]
    }

    pub fn test_point(&self, xf: &Transform, p: Vec2) -> bool {
        let p_local = mul_t_rot(xf.q, p - xf.p);
        (0..self.count).all(|i| dot(self.normals[i], p_local - self.vertices[i]) <= 0.0)
    }

    pub fn ray_cast(&self, input: &RayCastInput, xf: &Transform) -> Option<RayCastOutput> {
        let p1 = mul_t_rot(xf.q, input.p1 - xf.p);
        let p2 = mul_t_rot(xf.q, input.p2 - xf.p);
        let d = p2 - p1;
        let mut lower = 0.0;
        let mut upper = input.max_fraction;
        let mut index: Option<usize> = None;
        for i in 0..self.count {
            let numerator = dot(self.normals[i], self.vertices[i] - p1);
            let denominator = dot(self.normals[i], d);
            if denominator == 0.0 {
                if numerator < 0.0 {
                    return None;
                }
            } else if denominator < 0.0 && numerator < lower * denominator {
                lower = numerator / denominator;
                index = Some(i);
            } else if denominator > 0.0 && numerator < upper * denominator {
                upper = numerator / denominator;
            }
            if upper < lower {
                return None;
            }
        }
        index.map(|i| RayCastOutput {
            fraction: lower,
            normal: mul_rot(xf.q, self.normals[i]),
        })
    }

    pub fn compute_aabb(&self, xf: &Transform) -> Aabb {
        let mut lower = mul_xf(xf, self.vertices[0]);
        let mut upper = lower;
        for i in 1..self.count {
            let v = mul_xf(xf, self.vertices[i]);
            lower = min2(lower, v);
            upper = max2(upper, v);
        }
        let r = vec2(POLYGON_RADIUS, POLYGON_RADIUS);
        Aabb {
            lower_bound: lower - r,
            upper_bound: upper + r,
        }
    }

    /// `b2PolygonShape::ComputeMass` in the operation order of Noita's build (0x9bb9a0): the vertex
    /// sum is unrolled by four and the inertia terms are added as one left-to-right chain.
    pub fn compute_mass(&self, density: Real) -> MassData {
        let n = self.count;
        let v = &self.vertices;
        let (mut sx, mut sy) = (0.0, 0.0);
        let mut i = 0;
        if n > 3 {
            while i + 4 <= n {
                sx += ((v[i + 3].x + v[i + 2].x) + v[i + 1].x) + v[i].x;
                sy += ((v[i + 3].y + v[i + 2].y) + v[i + 1].y) + v[i].y;
                i += 4;
            }
        }
        while i < n {
            sx += v[i].x;
            sy += v[i].y;
            i += 1;
        }
        let inv_count = Real::from(1.0f32 / n as f32);
        let s = vec2(sx * inv_count, sy * inv_count);

        let k_inv3 = Real::from(1.0f32 / 3.0f32);
        let k_inertia = Real::from(0.25f32) * k_inv3;
        let (mut area, mut cx, mut cy, mut inertia) = (0.0, 0.0, 0.0, 0.0);
        for i in 0..n {
            let e1 = v[i] - s;
            let e2 = if i + 1 < n { v[i + 1] - s } else { v[0] - s };
            let d = e2.y * e1.x - e2.x * e1.y;
            let triangle_area = d * 0.5;
            let w = triangle_area * k_inv3;
            area += triangle_area;
            cx += (e2.x + e1.x) * w;
            cy += (e2.y + e1.y) * w;
            let sum =
                e2.y * e1.y + e1.y * e1.y + e2.y * e2.y + e2.x * e1.x + e1.x * e1.x + e2.x * e2.x;
            inertia += sum * d * k_inertia;
        }

        let mass = area * density;
        if area <= EPSILON {
            return MassData {
                mass,
                ..Default::default()
            };
        }
        let inv_area = 1.0 / area;
        let (cx, cy) = (cx * inv_area, cy * inv_area);
        let center = vec2(s.x + cx, s.y + cy);
        let diff = (center.y * center.y + center.x * center.x) - (cy * cy + cx * cx);
        MassData {
            mass,
            center,
            i: diff * area * density + inertia * density,
        }
    }

    /// Convexity check; slow.
    pub fn validate(&self) -> bool {
        for i in 0..self.count {
            let i2 = if i < self.count - 1 { i + 1 } else { 0 };
            let p = self.vertices[i];
            let e = self.vertices[i2] - p;
            for j in 0..self.count {
                if j == i || j == i2 {
                    continue;
                }
                if cross(e, self.vertices[j] - p) < 0.0 {
                    return false;
                }
            }
        }
        true
    }
}

/// `b2EdgeShape`: a segment with optional neighbor vertices for smooth chain collision.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeShape {
    pub vertex0: Vec2,
    pub vertex1: Vec2,
    pub vertex2: Vec2,
    pub vertex3: Vec2,
    pub has_vertex0: bool,
    pub has_vertex3: bool,
}

impl EdgeShape {
    pub fn new(v1: Vec2, v2: Vec2) -> Self {
        EdgeShape {
            vertex1: v1,
            vertex2: v2,
            ..Default::default()
        }
    }

    pub fn radius(&self) -> Real {
        POLYGON_RADIUS
    }

    pub fn ray_cast(&self, input: &RayCastInput, xf: &Transform) -> Option<RayCastOutput> {
        let p1 = mul_t_rot(xf.q, input.p1 - xf.p);
        let p2 = mul_t_rot(xf.q, input.p2 - xf.p);
        let d = p2 - p1;

        let (v1, v2) = (self.vertex1, self.vertex2);
        let e = v2 - v1;
        let mut normal = vec2(e.y, -e.x);
        normal.normalize();

        let numerator = dot(normal, v1 - p1);
        let denominator = dot(normal, d);
        if denominator == 0.0 {
            return None;
        }
        let t = numerator / denominator;
        if t < 0.0 || input.max_fraction < t {
            return None;
        }
        let q = p1 + t * d;
        let r = v2 - v1;
        let rr = dot(r, r);
        if rr == 0.0 {
            return None;
        }
        let s = dot(q - v1, r) / rr;
        if !(0.0..=1.0).contains(&s) {
            return None;
        }
        let n = mul_rot(xf.q, normal);
        Some(RayCastOutput {
            fraction: t,
            normal: if numerator > 0.0 { -n } else { n },
        })
    }

    pub fn compute_aabb(&self, xf: &Transform) -> Aabb {
        let v1 = mul_xf(xf, self.vertex1);
        let v2 = mul_xf(xf, self.vertex2);
        let r = vec2(POLYGON_RADIUS, POLYGON_RADIUS);
        Aabb {
            lower_bound: min2(v1, v2) - r,
            upper_bound: max2(v1, v2) + r,
        }
    }

    pub fn compute_mass(&self) -> MassData {
        MassData {
            mass: 0.0,
            center: 0.5 * (self.vertex1 + self.vertex2),
            i: 0.0,
        }
    }
}

/// `b2ChainShape`: a vertex chain or loop; each segment is a child edge.
#[derive(Clone, Debug, PartialEq)]
pub struct ChainShape {
    pub vertices: std::sync::Arc<[Vec2]>,
    pub prev_vertex: Vec2,
    pub next_vertex: Vec2,
    pub has_prev_vertex: bool,
    pub has_next_vertex: bool,
}

impl ChainShape {
    pub fn create_chain(vertices: &[Vec2]) -> Self {
        ChainShape {
            vertices: vertices.into(),
            prev_vertex: Vec2::ZERO,
            next_vertex: Vec2::ZERO,
            has_prev_vertex: false,
            has_next_vertex: false,
        }
    }

    /// A closed loop: the first vertex is repeated and both ends connect.
    pub fn create_loop(vertices: &[Vec2]) -> Self {
        let mut v = vertices.to_vec();
        v.push(vertices[0]);
        let count = v.len();
        ChainShape {
            prev_vertex: v[count - 2],
            next_vertex: v[1],
            vertices: v.into(),
            has_prev_vertex: true,
            has_next_vertex: true,
        }
    }

    pub fn radius(&self) -> Real {
        POLYGON_RADIUS
    }

    pub fn child_count(&self) -> usize {
        self.vertices.len() - 1
    }

    pub fn child_edge(&self, index: usize) -> EdgeShape {
        let count = self.vertices.len();
        let mut edge = EdgeShape::new(self.vertices[index], self.vertices[index + 1]);
        if index > 0 {
            edge.vertex0 = self.vertices[index - 1];
            edge.has_vertex0 = true;
        } else {
            edge.vertex0 = self.prev_vertex;
            edge.has_vertex0 = self.has_prev_vertex;
        }
        if index + 2 < count {
            edge.vertex3 = self.vertices[index + 2];
            edge.has_vertex3 = true;
        } else {
            edge.vertex3 = self.next_vertex;
            edge.has_vertex3 = self.has_next_vertex;
        }
        edge
    }

    fn segment(&self, index: usize) -> (Vec2, Vec2) {
        let i2 = if index + 1 == self.vertices.len() {
            0
        } else {
            index + 1
        };
        (self.vertices[index], self.vertices[i2])
    }

    pub fn ray_cast(
        &self,
        input: &RayCastInput,
        xf: &Transform,
        child: usize,
    ) -> Option<RayCastOutput> {
        let (v1, v2) = self.segment(child);
        EdgeShape::new(v1, v2).ray_cast(input, xf)
    }

    /// Unlike an edge, the chain AABB is not fattened by the radius.
    pub fn compute_aabb(&self, xf: &Transform, child: usize) -> Aabb {
        let (a, b) = self.segment(child);
        let (v1, v2) = (mul_xf(xf, a), mul_xf(xf, b));
        Aabb {
            lower_bound: min2(v1, v2),
            upper_bound: max2(v1, v2),
        }
    }
}

/// A fixture shape.
#[allow(clippy::large_enum_variant)] // inline vertex storage like the original
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Circle(CircleShape),
    Polygon(PolygonShape),
    Edge(EdgeShape),
    Chain(ChainShape),
}

impl Shape {
    pub fn radius(&self) -> Real {
        match self {
            Shape::Circle(c) => c.radius,
            Shape::Polygon(p) => p.radius(),
            Shape::Edge(e) => e.radius(),
            Shape::Chain(c) => c.radius(),
        }
    }

    pub fn child_count(&self) -> usize {
        match self {
            Shape::Chain(c) => c.child_count(),
            _ => 1,
        }
    }

    pub fn test_point(&self, xf: &Transform, p: Vec2) -> bool {
        match self {
            Shape::Circle(c) => c.test_point(xf, p),
            Shape::Polygon(s) => s.test_point(xf, p),
            Shape::Edge(_) | Shape::Chain(_) => false,
        }
    }

    pub fn ray_cast(
        &self,
        input: &RayCastInput,
        xf: &Transform,
        child: usize,
    ) -> Option<RayCastOutput> {
        match self {
            Shape::Circle(c) => c.ray_cast(input, xf),
            Shape::Polygon(s) => s.ray_cast(input, xf),
            Shape::Edge(e) => e.ray_cast(input, xf),
            Shape::Chain(c) => c.ray_cast(input, xf, child),
        }
    }

    pub fn compute_aabb(&self, xf: &Transform, child: usize) -> Aabb {
        match self {
            Shape::Circle(c) => c.compute_aabb(xf),
            Shape::Polygon(s) => s.compute_aabb(xf),
            Shape::Edge(e) => e.compute_aabb(xf),
            Shape::Chain(c) => c.compute_aabb(xf, child),
        }
    }

    pub fn compute_mass(&self, density: Real) -> MassData {
        match self {
            Shape::Circle(c) => c.compute_mass(density),
            Shape::Polygon(s) => s.compute_mass(density),
            Shape::Edge(e) => e.compute_mass(),
            Shape::Chain(_) => MassData::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_mass_matches_closed_form() {
        let b = PolygonShape::as_box(0.5, 1.0);
        let m = b.compute_mass(2.0);
        assert!((m.mass - 4.0).abs() < 1e-5);
        assert!(m.center.length() < 1e-6);
        // I = m (w^2 + h^2) / 12 with w = 1, h = 2.
        assert!((m.i - 4.0 * 5.0 / 12.0).abs() < 1e-5);
    }

    #[test]
    fn hull_orders_counter_clockwise_and_welds() {
        let pts = [
            vec2(0.0, 0.0),
            vec2(1.0, 1.0),
            vec2(1.0, 0.0),
            vec2(0.0, 1.0),
            vec2(0.5, 0.5),
            vec2(1.0, 1.0),
        ];
        let p = PolygonShape::from_points(&pts);
        assert_eq!(p.count, 4);
        assert!(p.validate());
        assert!((p.centroid.x - 0.5).abs() < 1e-6 && (p.centroid.y - 0.5).abs() < 1e-6);
    }

    #[test]
    fn circle_mass_and_aabb() {
        let c = CircleShape::new(1.0);
        let m = c.compute_mass(1.0);
        assert!((m.mass - PI).abs() < 1e-6);
        let aabb = c.compute_aabb(&Transform::new(vec2(2.0, 3.0), 0.0));
        assert_eq!(aabb.lower_bound, vec2(1.0, 2.0));
        assert_eq!(aabb.upper_bound, vec2(3.0, 4.0));
    }

    #[test]
    fn ray_hits_box_face() {
        let b = PolygonShape::as_box(1.0, 1.0);
        let input = RayCastInput {
            p1: vec2(-3.0, 0.0),
            p2: vec2(3.0, 0.0),
            max_fraction: 1.0,
        };
        let out = b.ray_cast(&input, &Transform::IDENTITY).unwrap();
        assert!((out.fraction - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(out.normal, vec2(-1.0, 0.0));
    }

    #[test]
    fn collinear_points_give_a_unit_box_at_the_first_point() {
        let p = PolygonShape::from_points(&[vec2(2.0, 3.0), vec2(3.0, 3.0), vec2(4.0, 3.0)]);
        assert_eq!(p.count, 4);
        assert_eq!(p.vertices[2], vec2(3.0, 4.0));
        assert_eq!(p.centroid, vec2(2.5, 3.5));
    }

    #[test]
    fn welding_uses_the_squared_half_slop_distance() {
        // Points 0.01 apart are kept; stock Box2D would weld them (0.0001 < 0.0025).
        let p = PolygonShape::from_points(&[
            vec2(0.0, 0.0),
            vec2(1.0, 0.0),
            vec2(0.5, 1.0),
            vec2(0.5, 1.01),
        ]);
        assert_eq!(p.count, 3);
        assert!(p.vertices().contains(&vec2(0.5, 1.01)));
    }
}

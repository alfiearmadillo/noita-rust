//! GJK distance (`b2Distance.cpp`, Box2D 2.3.1).

use crate::math::*;
use crate::settings::*;
use crate::shape::Shape;

/// Copy of the vertices GJK works on.
#[derive(Clone, Copy, Debug)]
pub struct DistanceProxy {
    vertices: [Vec2; MAX_POLYGON_VERTICES],
    pub count: usize,
    pub radius: Real,
}

impl DistanceProxy {
    pub fn from_shape(shape: &Shape, child_index: usize) -> Self {
        let mut vertices = [Vec2::ZERO; MAX_POLYGON_VERTICES];
        match shape {
            Shape::Circle(c) => {
                vertices[0] = c.p;
                DistanceProxy {
                    vertices,
                    count: 1,
                    radius: c.radius,
                }
            }
            Shape::Polygon(p) => {
                vertices[..p.count].copy_from_slice(&p.vertices[..p.count]);
                DistanceProxy {
                    vertices,
                    count: p.count,
                    radius: p.radius(),
                }
            }
            Shape::Edge(e) => {
                vertices[0] = e.vertex1;
                vertices[1] = e.vertex2;
                DistanceProxy {
                    vertices,
                    count: 2,
                    radius: e.radius(),
                }
            }
            Shape::Chain(c) => {
                vertices[0] = c.vertices[child_index];
                vertices[1] = if child_index + 1 < c.vertices.len() {
                    c.vertices[child_index + 1]
                } else {
                    c.vertices[0]
                };
                DistanceProxy {
                    vertices,
                    count: 2,
                    radius: c.radius(),
                }
            }
        }
    }

    /// A proxy over arbitrary points (used by tests and TOI).
    pub fn from_points(points: &[Vec2], radius: Real) -> Self {
        let mut vertices = [Vec2::ZERO; MAX_POLYGON_VERTICES];
        vertices[..points.len()].copy_from_slice(points);
        DistanceProxy {
            vertices,
            count: points.len(),
            radius,
        }
    }

    pub fn vertex(&self, index: usize) -> Vec2 {
        self.vertices[index]
    }

    pub fn support(&self, d: Vec2) -> usize {
        let mut best_index = 0;
        let mut best_value = dot(self.vertices[0], d);
        for i in 1..self.count {
            let value = dot(self.vertices[i], d);
            if value > best_value {
                best_index = i;
                best_value = value;
            }
        }
        best_index
    }

    pub fn support_vertex(&self, d: Vec2) -> Vec2 {
        self.vertices[self.support(d)]
    }
}

/// Warm-start data; set `count` to zero on the first call.
#[derive(Clone, Copy, Debug, Default)]
pub struct SimplexCache {
    pub metric: Real,
    pub count: u16,
    pub index_a: [u8; 3],
    pub index_b: [u8; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct DistanceInput {
    pub proxy_a: DistanceProxy,
    pub proxy_b: DistanceProxy,
    pub transform_a: Transform,
    pub transform_b: Transform,
    pub use_radii: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DistanceOutput {
    pub point_a: Vec2,
    pub point_b: Vec2,
    pub distance: Real,
    pub iterations: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SimplexVertex {
    pub w_a: Vec2,
    pub w_b: Vec2,
    pub w: Vec2,
    pub a: Real,
    pub index_a: usize,
    pub index_b: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Simplex {
    pub v: [SimplexVertex; 3],
    pub count: usize,
}

impl Simplex {
    pub(crate) fn read_cache(
        cache: &SimplexCache,
        proxy_a: &DistanceProxy,
        transform_a: &Transform,
        proxy_b: &DistanceProxy,
        transform_b: &Transform,
    ) -> Simplex {
        let mut s = Simplex {
            count: usize::from(cache.count),
            ..Default::default()
        };
        for i in 0..s.count {
            let v = &mut s.v[i];
            v.index_a = usize::from(cache.index_a[i]);
            v.index_b = usize::from(cache.index_b[i]);
            v.w_a = mul_xf(transform_a, proxy_a.vertex(v.index_a));
            v.w_b = mul_xf(transform_b, proxy_b.vertex(v.index_b));
            v.w = v.w_b - v.w_a;
            v.a = 0.0;
        }
        if s.count > 1 {
            let metric1 = cache.metric;
            let metric2 = s.metric();
            if metric2 < 0.5 * metric1 || 2.0 * metric1 < metric2 || metric2 < EPSILON {
                s.count = 0;
            }
        }
        if s.count == 0 {
            let v = &mut s.v[0];
            v.index_a = 0;
            v.index_b = 0;
            v.w_a = mul_xf(transform_a, proxy_a.vertex(0));
            v.w_b = mul_xf(transform_b, proxy_b.vertex(0));
            v.w = v.w_b - v.w_a;
            v.a = 1.0;
            s.count = 1;
        }
        s
    }

    pub(crate) fn write_cache(&self, cache: &mut SimplexCache) {
        cache.metric = self.metric();
        cache.count = self.count as u16;
        for i in 0..self.count {
            cache.index_a[i] = self.v[i].index_a as u8;
            cache.index_b[i] = self.v[i].index_b as u8;
        }
    }

    pub(crate) fn search_direction(&self) -> Vec2 {
        match self.count {
            1 => -self.v[0].w,
            2 => {
                let e12 = self.v[1].w - self.v[0].w;
                let sgn = cross(e12, -self.v[0].w);
                if sgn > 0.0 {
                    cross_sv(1.0, e12)
                } else {
                    cross_vs(e12, 1.0)
                }
            }
            _ => Vec2::ZERO,
        }
    }

    pub(crate) fn witness_points(&self) -> (Vec2, Vec2) {
        let v = &self.v;
        match self.count {
            1 => (v[0].w_a, v[0].w_b),
            2 => (
                v[0].a * v[0].w_a + v[1].a * v[1].w_a,
                v[0].a * v[0].w_b + v[1].a * v[1].w_b,
            ),
            3 => {
                let p = v[0].a * v[0].w_a + v[1].a * v[1].w_a + v[2].a * v[2].w_a;
                (p, p)
            }
            _ => (Vec2::ZERO, Vec2::ZERO),
        }
    }

    pub(crate) fn metric(&self) -> Real {
        match self.count {
            2 => distance(self.v[0].w, self.v[1].w),
            3 => cross(self.v[1].w - self.v[0].w, self.v[2].w - self.v[0].w),
            _ => 0.0,
        }
    }

    pub(crate) fn solve2(&mut self) {
        let w1 = self.v[0].w;
        let w2 = self.v[1].w;
        let e12 = w2 - w1;

        let d12_2 = -dot(w1, e12);
        if d12_2 <= 0.0 {
            self.v[0].a = 1.0;
            self.count = 1;
            return;
        }
        let d12_1 = dot(w2, e12);
        if d12_1 <= 0.0 {
            self.v[1].a = 1.0;
            self.count = 1;
            self.v[0] = self.v[1];
            return;
        }
        let inv_d12 = 1.0 / (d12_1 + d12_2);
        self.v[0].a = d12_1 * inv_d12;
        self.v[1].a = d12_2 * inv_d12;
        self.count = 2;
    }

    pub(crate) fn solve3(&mut self) {
        let w1 = self.v[0].w;
        let w2 = self.v[1].w;
        let w3 = self.v[2].w;

        let e12 = w2 - w1;
        let w1e12 = dot(w1, e12);
        let w2e12 = dot(w2, e12);
        let d12_1 = w2e12;
        let d12_2 = -w1e12;

        let e13 = w3 - w1;
        let w1e13 = dot(w1, e13);
        let w3e13 = dot(w3, e13);
        let d13_1 = w3e13;
        let d13_2 = -w1e13;

        let e23 = w3 - w2;
        let w2e23 = dot(w2, e23);
        let w3e23 = dot(w3, e23);
        let d23_1 = w3e23;
        let d23_2 = -w2e23;

        let n123 = cross(e12, e13);
        let d123_1 = n123 * cross(w2, w3);
        let d123_2 = n123 * cross(w3, w1);
        let d123_3 = n123 * cross(w1, w2);

        if d12_2 <= 0.0 && d13_2 <= 0.0 {
            self.v[0].a = 1.0;
            self.count = 1;
            return;
        }
        if d12_1 > 0.0 && d12_2 > 0.0 && d123_3 <= 0.0 {
            let inv_d12 = 1.0 / (d12_1 + d12_2);
            self.v[0].a = d12_1 * inv_d12;
            self.v[1].a = d12_2 * inv_d12;
            self.count = 2;
            return;
        }
        if d13_1 > 0.0 && d13_2 > 0.0 && d123_2 <= 0.0 {
            let inv_d13 = 1.0 / (d13_1 + d13_2);
            self.v[0].a = d13_1 * inv_d13;
            self.v[2].a = d13_2 * inv_d13;
            self.count = 2;
            self.v[1] = self.v[2];
            return;
        }
        if d12_1 <= 0.0 && d23_2 <= 0.0 {
            self.v[1].a = 1.0;
            self.count = 1;
            self.v[0] = self.v[1];
            return;
        }
        if d13_1 <= 0.0 && d23_1 <= 0.0 {
            self.v[2].a = 1.0;
            self.count = 1;
            self.v[0] = self.v[2];
            return;
        }
        if d23_1 > 0.0 && d23_2 > 0.0 && d123_1 <= 0.0 {
            let inv_d23 = 1.0 / (d23_1 + d23_2);
            self.v[1].a = d23_1 * inv_d23;
            self.v[2].a = d23_2 * inv_d23;
            self.count = 2;
            self.v[0] = self.v[2];
            return;
        }
        let inv_d123 = 1.0 / (d123_1 + d123_2 + d123_3);
        self.v[0].a = d123_1 * inv_d123;
        self.v[1].a = d123_2 * inv_d123;
        self.v[2].a = d123_3 * inv_d123;
        self.count = 3;
    }
}

/// Closest points between two shapes. `cache` is input and output.
pub fn distance_gjk(cache: &mut SimplexCache, input: &DistanceInput) -> DistanceOutput {
    let proxy_a = &input.proxy_a;
    let proxy_b = &input.proxy_b;
    let transform_a = input.transform_a;
    let transform_b = input.transform_b;

    let mut simplex = Simplex::read_cache(cache, proxy_a, &transform_a, proxy_b, &transform_b);

    const MAX_ITERS: i32 = 20;
    let mut save_a = [0usize; 3];
    let mut save_b = [0usize; 3];

    let mut iter = 0;
    while iter < MAX_ITERS {
        let save_count = simplex.count;
        for i in 0..save_count {
            save_a[i] = simplex.v[i].index_a;
            save_b[i] = simplex.v[i].index_b;
        }

        match simplex.count {
            2 => simplex.solve2(),
            3 => simplex.solve3(),
            _ => {}
        }

        if simplex.count == 3 {
            break;
        }

        // The original computes the closest point here only for a disabled progress check.
        let d = simplex.search_direction();
        if d.length_squared() < EPSILON * EPSILON {
            break;
        }

        let idx = simplex.count;
        let v = &mut simplex.v[idx];
        v.index_a = proxy_a.support(mul_t_rot(transform_a.q, -d));
        v.w_a = mul_xf(&transform_a, proxy_a.vertex(v.index_a));
        v.index_b = proxy_b.support(mul_t_rot(transform_b.q, d));
        v.w_b = mul_xf(&transform_b, proxy_b.vertex(v.index_b));
        v.w = v.w_b - v.w_a;

        iter += 1;

        let duplicate = (0..save_count).any(|i| v.index_a == save_a[i] && v.index_b == save_b[i]);
        if duplicate {
            break;
        }
        simplex.count += 1;
    }

    let (point_a, point_b) = simplex.witness_points();
    let mut out = DistanceOutput {
        point_a,
        point_b,
        distance: distance(point_a, point_b),
        iterations: iter,
    };
    simplex.write_cache(cache);

    if input.use_radii {
        let r_a = proxy_a.radius;
        let r_b = proxy_b.radius;
        if out.distance > r_a + r_b && out.distance > EPSILON {
            out.distance -= r_a + r_b;
            let mut normal = out.point_b - out.point_a;
            normal.normalize();
            out.point_a += r_a * normal;
            out.point_b -= r_b * normal;
        } else {
            let p = 0.5 * (out.point_a + out.point_b);
            out.point_a = p;
            out.point_b = p;
            out.distance = 0.0;
        }
    }
    out
}

/// `b2TestOverlap` for shapes: closest distance under 10 epsilon.
pub fn test_overlap_shapes(
    shape_a: &Shape,
    index_a: usize,
    shape_b: &Shape,
    index_b: usize,
    xf_a: &Transform,
    xf_b: &Transform,
) -> bool {
    let input = DistanceInput {
        proxy_a: DistanceProxy::from_shape(shape_a, index_a),
        proxy_b: DistanceProxy::from_shape(shape_b, index_b),
        transform_a: *xf_a,
        transform_b: *xf_b,
        use_radii: true,
    };
    let mut cache = SimplexCache::default();
    let out = distance_gjk(&mut cache, &input);
    out.distance < 10.0 * EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::PolygonShape;

    #[test]
    fn distance_between_separated_boxes() {
        let a = Shape::Polygon(PolygonShape::as_box(1.0, 1.0));
        let input = DistanceInput {
            proxy_a: DistanceProxy::from_shape(&a, 0),
            proxy_b: DistanceProxy::from_shape(&a, 0),
            transform_a: Transform::IDENTITY,
            transform_b: Transform::new(vec2(5.0, 0.0), 0.0),
            use_radii: false,
        };
        let mut cache = SimplexCache::default();
        let out = distance_gjk(&mut cache, &input);
        assert!((out.distance - 3.0).abs() < 1e-5);
        assert!(cache.count >= 1);
    }

    #[test]
    fn overlap_test_detects_contact() {
        let a = Shape::Polygon(PolygonShape::as_box(1.0, 1.0));
        assert!(test_overlap_shapes(
            &a,
            0,
            &a,
            0,
            &Transform::IDENTITY,
            &Transform::new(vec2(1.5, 0.0), 0.0)
        ));
        assert!(!test_overlap_shapes(
            &a,
            0,
            &a,
            0,
            &Transform::IDENTITY,
            &Transform::new(vec2(2.5, 0.0), 0.0)
        ));
    }
}

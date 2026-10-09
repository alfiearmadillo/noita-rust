//! Continuous collision (`b2TimeOfImpact.cpp`, Box2D 2.3.1): conservative advancement along
//! the local separating axis.

use crate::distance::*;
use crate::math::*;
use crate::settings::*;

#[derive(Clone, Copy, Debug)]
pub struct ToiInput {
    pub proxy_a: DistanceProxy,
    pub proxy_b: DistanceProxy,
    pub sweep_a: Sweep,
    pub sweep_b: Sweep,
    /// Defines the sweep interval [0, t_max].
    pub t_max: Real,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToiState {
    Unknown,
    Failed,
    Overlapped,
    Touching,
    Separated,
}

#[derive(Clone, Copy, Debug)]
pub struct ToiOutput {
    pub state: ToiState,
    pub t: Real,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SepType {
    Points,
    FaceA,
    FaceB,
}

struct SeparationFunction<'a> {
    proxy_a: &'a DistanceProxy,
    proxy_b: &'a DistanceProxy,
    sweep_a: Sweep,
    sweep_b: Sweep,
    kind: SepType,
    local_point: Vec2,
    axis: Vec2,
}

impl<'a> SeparationFunction<'a> {
    fn initialize(
        cache: &SimplexCache,
        proxy_a: &'a DistanceProxy,
        sweep_a: &Sweep,
        proxy_b: &'a DistanceProxy,
        sweep_b: &Sweep,
        t1: Real,
    ) -> (Self, Real) {
        let count = cache.count;
        let xf_a = sweep_a.transform(t1);
        let xf_b = sweep_b.transform(t1);
        let mut f = SeparationFunction {
            proxy_a,
            proxy_b,
            sweep_a: *sweep_a,
            sweep_b: *sweep_b,
            kind: SepType::Points,
            local_point: Vec2::ZERO,
            axis: Vec2::ZERO,
        };

        if count == 1 {
            f.kind = SepType::Points;
            let local_point_a = proxy_a.vertex(usize::from(cache.index_a[0]));
            let local_point_b = proxy_b.vertex(usize::from(cache.index_b[0]));
            let point_a = mul_xf(&xf_a, local_point_a);
            let point_b = mul_xf(&xf_b, local_point_b);
            f.axis = point_b - point_a;
            let s = f.axis.normalize();
            (f, s)
        } else if cache.index_a[0] == cache.index_a[1] {
            f.kind = SepType::FaceB;
            let local_point_b1 = proxy_b.vertex(usize::from(cache.index_b[0]));
            let local_point_b2 = proxy_b.vertex(usize::from(cache.index_b[1]));
            f.axis = cross_vs(local_point_b2 - local_point_b1, 1.0);
            f.axis.normalize();
            let normal = mul_rot(xf_b.q, f.axis);
            f.local_point = 0.5 * (local_point_b1 + local_point_b2);
            let point_b = mul_xf(&xf_b, f.local_point);
            let local_point_a = proxy_a.vertex(usize::from(cache.index_a[0]));
            let point_a = mul_xf(&xf_a, local_point_a);
            let mut s = dot(point_a - point_b, normal);
            if s < 0.0 {
                f.axis = -f.axis;
                s = -s;
            }
            (f, s)
        } else {
            f.kind = SepType::FaceA;
            let local_point_a1 = proxy_a.vertex(usize::from(cache.index_a[0]));
            let local_point_a2 = proxy_a.vertex(usize::from(cache.index_a[1]));
            f.axis = cross_vs(local_point_a2 - local_point_a1, 1.0);
            f.axis.normalize();
            let normal = mul_rot(xf_a.q, f.axis);
            f.local_point = 0.5 * (local_point_a1 + local_point_a2);
            let point_a = mul_xf(&xf_a, f.local_point);
            let local_point_b = proxy_b.vertex(usize::from(cache.index_b[0]));
            let point_b = mul_xf(&xf_b, local_point_b);
            let mut s = dot(point_b - point_a, normal);
            if s < 0.0 {
                f.axis = -f.axis;
                s = -s;
            }
            (f, s)
        }
    }

    /// Returns (separation, index_a, index_b); an index of -1 is unused.
    fn find_min_separation(&self, t: Real) -> (Real, i32, i32) {
        let xf_a = self.sweep_a.transform(t);
        let xf_b = self.sweep_b.transform(t);
        match self.kind {
            SepType::Points => {
                let axis_a = mul_t_rot(xf_a.q, self.axis);
                let axis_b = mul_t_rot(xf_b.q, -self.axis);
                let index_a = self.proxy_a.support(axis_a);
                let index_b = self.proxy_b.support(axis_b);
                let point_a = mul_xf(&xf_a, self.proxy_a.vertex(index_a));
                let point_b = mul_xf(&xf_b, self.proxy_b.vertex(index_b));
                (
                    dot(point_b - point_a, self.axis),
                    index_a as i32,
                    index_b as i32,
                )
            }
            SepType::FaceA => {
                let normal = mul_rot(xf_a.q, self.axis);
                let point_a = mul_xf(&xf_a, self.local_point);
                let axis_b = mul_t_rot(xf_b.q, -normal);
                let index_b = self.proxy_b.support(axis_b);
                let point_b = mul_xf(&xf_b, self.proxy_b.vertex(index_b));
                (dot(point_b - point_a, normal), -1, index_b as i32)
            }
            SepType::FaceB => {
                let normal = mul_rot(xf_b.q, self.axis);
                let point_b = mul_xf(&xf_b, self.local_point);
                let axis_a = mul_t_rot(xf_a.q, -normal);
                let index_a = self.proxy_a.support(axis_a);
                let point_a = mul_xf(&xf_a, self.proxy_a.vertex(index_a));
                (dot(point_a - point_b, normal), index_a as i32, -1)
            }
        }
    }

    fn evaluate(&self, index_a: i32, index_b: i32, t: Real) -> Real {
        let xf_a = self.sweep_a.transform(t);
        let xf_b = self.sweep_b.transform(t);
        match self.kind {
            SepType::Points => {
                let point_a = mul_xf(&xf_a, self.proxy_a.vertex(index_a as usize));
                let point_b = mul_xf(&xf_b, self.proxy_b.vertex(index_b as usize));
                dot(point_b - point_a, self.axis)
            }
            SepType::FaceA => {
                let normal = mul_rot(xf_a.q, self.axis);
                let point_a = mul_xf(&xf_a, self.local_point);
                let point_b = mul_xf(&xf_b, self.proxy_b.vertex(index_b as usize));
                dot(point_b - point_a, normal)
            }
            SepType::FaceB => {
                let normal = mul_rot(xf_b.q, self.axis);
                let point_b = mul_xf(&xf_b, self.local_point);
                let point_a = mul_xf(&xf_a, self.proxy_a.vertex(index_a as usize));
                dot(point_a - point_b, normal)
            }
        }
    }
}

pub fn time_of_impact(input: &ToiInput) -> ToiOutput {
    let mut output = ToiOutput {
        state: ToiState::Unknown,
        t: input.t_max,
    };
    let proxy_a = &input.proxy_a;
    let proxy_b = &input.proxy_b;

    let mut sweep_a = input.sweep_a;
    let mut sweep_b = input.sweep_b;
    sweep_a.normalize();
    sweep_b.normalize();

    let t_max = input.t_max;
    let total_radius = proxy_a.radius + proxy_b.radius;
    // 	arget is a float32 local in the original.
    let target = Real::from(max(LINEAR_SLOP, total_radius - 3.0 * LINEAR_SLOP) as f32);
    let tolerance = 0.25 * LINEAR_SLOP;

    let mut t1 = 0.0;
    const MAX_ITERATIONS: i32 = 20;
    let mut iter = 0;

    let mut cache = SimplexCache::default();
    let mut distance_input = DistanceInput {
        proxy_a: *proxy_a,
        proxy_b: *proxy_b,
        transform_a: Transform::IDENTITY,
        transform_b: Transform::IDENTITY,
        use_radii: false,
    };

    loop {
        let xf_a = sweep_a.transform(t1);
        let xf_b = sweep_b.transform(t1);
        distance_input.transform_a = xf_a;
        distance_input.transform_b = xf_b;
        let distance_output = distance_gjk(&mut cache, &distance_input);

        if distance_output.distance <= 0.0 {
            output.state = ToiState::Overlapped;
            output.t = 0.0;
            break;
        }
        if distance_output.distance < target + tolerance {
            output.state = ToiState::Touching;
            output.t = t1;
            break;
        }

        let (fcn, _) =
            SeparationFunction::initialize(&cache, proxy_a, &sweep_a, proxy_b, &sweep_b, t1);

        let mut done = false;
        let mut t2 = t_max;
        let mut push_back_iter = 0;
        loop {
            let (mut s2, index_a, index_b) = fcn.find_min_separation(t2);

            if s2 > target + tolerance {
                output.state = ToiState::Separated;
                output.t = t_max;
                done = true;
                break;
            }
            if s2 > target - tolerance {
                t1 = t2;
                break;
            }

            let mut s1 = fcn.evaluate(index_a, index_b, t1);
            if s1 < target - tolerance {
                output.state = ToiState::Failed;
                output.t = t1;
                done = true;
                break;
            }
            if s1 <= target + tolerance {
                output.state = ToiState::Touching;
                output.t = t1;
                done = true;
                break;
            }

            let mut root_iter_count = 0;
            let mut a1 = t1;
            let mut a2 = t2;
            loop {
                let t = if root_iter_count & 1 != 0 {
                    a1 + (target - s1) * (a2 - a1) / (s2 - s1)
                } else {
                    0.5 * (a1 + a2)
                };
                root_iter_count += 1;
                let s = fcn.evaluate(index_a, index_b, t);
                if abs(s - target) < tolerance {
                    t2 = t;
                    break;
                }
                if s > target {
                    a1 = t;
                    s1 = s;
                } else {
                    a2 = t;
                    s2 = s;
                }
                if root_iter_count == 50 {
                    break;
                }
            }

            push_back_iter += 1;
            if push_back_iter == MAX_POLYGON_VERTICES {
                break;
            }
        }

        iter += 1;
        if done {
            break;
        }
        if iter == MAX_ITERATIONS {
            output.state = ToiState::Failed;
            output.t = t1;
            break;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::PolygonShape;

    fn box_proxy() -> DistanceProxy {
        DistanceProxy::from_points(PolygonShape::as_box(0.5, 0.5).vertices(), POLYGON_RADIUS)
    }

    fn sweep(from: Vec2, to: Vec2) -> Sweep {
        Sweep {
            local_center: Vec2::ZERO,
            c0: from,
            c: to,
            a0: 0.0,
            a: 0.0,
            alpha0: 0.0,
        }
    }

    #[test]
    fn fast_box_stops_at_a_thin_wall() {
        let out = time_of_impact(&ToiInput {
            proxy_a: box_proxy(),
            proxy_b: DistanceProxy::from_points(
                PolygonShape::as_box(0.1, 5.0).vertices(),
                POLYGON_RADIUS,
            ),
            sweep_a: sweep(vec2(-5.0, 0.0), vec2(5.0, 0.0)),
            sweep_b: sweep(Vec2::ZERO, Vec2::ZERO),
            t_max: 1.0,
        });
        assert_eq!(out.state, ToiState::Touching);
        assert!(out.t > 0.3 && out.t < 0.5, "t = {}", out.t);
    }

    #[test]
    fn moving_away_is_separated() {
        let out = time_of_impact(&ToiInput {
            proxy_a: box_proxy(),
            proxy_b: box_proxy(),
            sweep_a: sweep(vec2(-3.0, 0.0), vec2(-5.0, 0.0)),
            sweep_b: sweep(Vec2::ZERO, Vec2::ZERO),
            t_max: 1.0,
        });
        assert_eq!(out.state, ToiState::Separated);
        assert_eq!(out.t, 1.0);
    }
}

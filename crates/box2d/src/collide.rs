//! Contact manifolds: `b2Collision.h/.cpp`, `b2CollideCircle.cpp`, `b2CollidePolygon.cpp` (Box2D 2.3.1).

use crate::math::*;
use crate::settings::*;
use crate::shape::{CircleShape, EdgeShape, PolygonShape};

pub const NULL_FEATURE: u8 = u8::MAX;

pub const FEATURE_VERTEX: u8 = 0;
pub const FEATURE_FACE: u8 = 1;

/// The features that intersect to form a contact point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContactFeature {
    pub index_a: u8,
    pub index_b: u8,
    pub type_a: u8,
    pub type_b: u8,
}

impl ContactFeature {
    /// `b2ContactID::key`: the four bytes in little-endian order.
    pub fn key(&self) -> u32 {
        u32::from_le_bytes([self.index_a, self.index_b, self.type_a, self.type_b])
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ManifoldPoint {
    /// Usage depends on the manifold type.
    pub local_point: Vec2,
    pub normal_impulse: Real,
    pub tangent_impulse: Real,
    pub id: ContactFeature,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ManifoldType {
    #[default]
    Circles,
    FaceA,
    FaceB,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Manifold {
    pub points: [ManifoldPoint; MAX_MANIFOLD_POINTS],
    pub local_normal: Vec2,
    pub local_point: Vec2,
    pub kind: ManifoldType,
    pub point_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldManifold {
    /// World vector pointing from A to B.
    pub normal: Vec2,
    pub points: [Vec2; MAX_MANIFOLD_POINTS],
    /// Negative means overlap, in meters.
    pub separations: [Real; MAX_MANIFOLD_POINTS],
}

impl WorldManifold {
    pub fn initialize(
        manifold: &Manifold,
        xf_a: &Transform,
        radius_a: Real,
        xf_b: &Transform,
        radius_b: Real,
    ) -> WorldManifold {
        let mut w = WorldManifold::default();
        if manifold.point_count == 0 {
            return w;
        }
        match manifold.kind {
            ManifoldType::Circles => {
                w.normal = vec2(1.0, 0.0);
                let point_a = mul_xf(xf_a, manifold.local_point);
                let point_b = mul_xf(xf_b, manifold.points[0].local_point);
                if distance_squared(point_a, point_b) > EPSILON * EPSILON {
                    w.normal = point_b - point_a;
                    w.normal.normalize();
                }
                let c_a = point_a + radius_a * w.normal;
                let c_b = point_b - radius_b * w.normal;
                w.points[0] = 0.5 * (c_a + c_b);
                w.separations[0] = dot(c_b - c_a, w.normal);
            }
            ManifoldType::FaceA => {
                w.normal = mul_rot(xf_a.q, manifold.local_normal);
                let plane_point = mul_xf(xf_a, manifold.local_point);
                for i in 0..manifold.point_count {
                    let clip_point = mul_xf(xf_b, manifold.points[i].local_point);
                    let c_a = clip_point
                        + (radius_a - dot(clip_point - plane_point, w.normal)) * w.normal;
                    let c_b = clip_point - radius_b * w.normal;
                    w.points[i] = 0.5 * (c_a + c_b);
                    w.separations[i] = dot(c_b - c_a, w.normal);
                }
            }
            ManifoldType::FaceB => {
                w.normal = mul_rot(xf_b.q, manifold.local_normal);
                let plane_point = mul_xf(xf_b, manifold.local_point);
                for i in 0..manifold.point_count {
                    let clip_point = mul_xf(xf_a, manifold.points[i].local_point);
                    let c_b = clip_point
                        + (radius_b - dot(clip_point - plane_point, w.normal)) * w.normal;
                    let c_a = clip_point - radius_a * w.normal;
                    w.points[i] = 0.5 * (c_a + c_b);
                    w.separations[i] = dot(c_a - c_b, w.normal);
                }
                w.normal = -w.normal;
            }
        }
        w
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClipVertex {
    pub v: Vec2,
    pub id: ContactFeature,
}

/// Sutherland-Hodgman clipping of a segment against a line.
pub fn clip_segment_to_line(
    v_out: &mut [ClipVertex; 2],
    v_in: &[ClipVertex; 2],
    normal: Vec2,
    offset: Real,
    vertex_index_a: usize,
) -> usize {
    let mut num_out = 0;
    let distance0 = dot(normal, v_in[0].v) - offset;
    let distance1 = dot(normal, v_in[1].v) - offset;
    if distance0 <= 0.0 {
        v_out[num_out] = v_in[0];
        num_out += 1;
    }
    if distance1 <= 0.0 {
        v_out[num_out] = v_in[1];
        num_out += 1;
    }
    if distance0 * distance1 < 0.0 {
        let interp = distance0 / (distance0 - distance1);
        v_out[num_out].v = v_in[0].v + interp * (v_in[1].v - v_in[0].v);
        v_out[num_out].id = ContactFeature {
            index_a: vertex_index_a as u8,
            index_b: v_in[0].id.index_b,
            type_a: FEATURE_VERTEX,
            type_b: FEATURE_FACE,
        };
        num_out += 1;
    }
    num_out
}

pub fn collide_circles(
    circle_a: &CircleShape,
    xf_a: &Transform,
    circle_b: &CircleShape,
    xf_b: &Transform,
) -> Manifold {
    let mut m = Manifold::default();
    let p_a = mul_xf(xf_a, circle_a.p);
    let p_b = mul_xf(xf_b, circle_b.p);
    let d = p_b - p_a;
    let dist_sqr = dot(d, d);
    let radius = circle_a.radius + circle_b.radius;
    if dist_sqr > radius * radius {
        return m;
    }
    m.kind = ManifoldType::Circles;
    m.local_point = circle_a.p;
    m.local_normal = Vec2::ZERO;
    m.point_count = 1;
    m.points[0].local_point = circle_b.p;
    m.points[0].id = ContactFeature::default();
    m
}

pub fn collide_polygon_and_circle(
    polygon_a: &PolygonShape,
    xf_a: &Transform,
    circle_b: &CircleShape,
    xf_b: &Transform,
) -> Manifold {
    let mut m = Manifold::default();
    let c = mul_xf(xf_b, circle_b.p);
    let c_local = mul_t_xf(xf_a, c);

    let mut normal_index = 0;
    let mut separation = -FLOAT_MAX;
    let radius = polygon_a.radius() + circle_b.radius;
    let vertex_count = polygon_a.count;
    let vertices = &polygon_a.vertices;
    let normals = &polygon_a.normals;
    for i in 0..vertex_count {
        let s = dot(normals[i], c_local - vertices[i]);
        if s > radius {
            return m;
        }
        if s > separation {
            separation = s;
            normal_index = i;
        }
    }

    let vert_index1 = normal_index;
    let vert_index2 = if vert_index1 + 1 < vertex_count {
        vert_index1 + 1
    } else {
        0
    };
    let v1 = vertices[vert_index1];
    let v2 = vertices[vert_index2];

    if separation < EPSILON {
        m.point_count = 1;
        m.kind = ManifoldType::FaceA;
        m.local_normal = normals[normal_index];
        m.local_point = 0.5 * (v1 + v2);
        m.points[0].local_point = circle_b.p;
        m.points[0].id = ContactFeature::default();
        return m;
    }

    let u1 = dot(c_local - v1, v2 - v1);
    let u2 = dot(c_local - v2, v1 - v2);
    if u1 <= 0.0 {
        if distance_squared(c_local, v1) > radius * radius {
            return m;
        }
        m.point_count = 1;
        m.kind = ManifoldType::FaceA;
        m.local_normal = c_local - v1;
        m.local_normal.normalize();
        m.local_point = v1;
        m.points[0].local_point = circle_b.p;
        m.points[0].id = ContactFeature::default();
    } else if u2 <= 0.0 {
        if distance_squared(c_local, v2) > radius * radius {
            return m;
        }
        m.point_count = 1;
        m.kind = ManifoldType::FaceA;
        m.local_normal = c_local - v2;
        m.local_normal.normalize();
        m.local_point = v2;
        m.points[0].local_point = circle_b.p;
        m.points[0].id = ContactFeature::default();
    } else {
        let face_center = 0.5 * (v1 + v2);
        let separation = dot(c_local - face_center, normals[vert_index1]);
        if separation > radius {
            return m;
        }
        m.point_count = 1;
        m.kind = ManifoldType::FaceA;
        m.local_normal = normals[vert_index1];
        m.local_point = face_center;
        m.points[0].local_point = circle_b.p;
        m.points[0].id = ContactFeature::default();
    }
    m
}

/// Max separation of `poly2` from the edge normals of `poly1`; returns (separation, edge index).
fn find_max_separation(
    poly1: &PolygonShape,
    xf1: &Transform,
    poly2: &PolygonShape,
    xf2: &Transform,
) -> (Real, usize) {
    let xf = mul_t_xx(xf2, xf1);
    let mut best_index = 0;
    let mut max_separation = -FLOAT_MAX;
    for i in 0..poly1.count {
        let n = mul_rot(xf.q, poly1.normals[i]);
        let v1 = mul_xf(&xf, poly1.vertices[i]);
        let mut si = FLOAT_MAX;
        for j in 0..poly2.count {
            let sij = dot(n, poly2.vertices[j] - v1);
            if sij < si {
                si = sij;
            }
        }
        if si > max_separation {
            max_separation = si;
            best_index = i;
        }
    }
    (max_separation, best_index)
}

fn find_incident_edge(
    poly1: &PolygonShape,
    xf1: &Transform,
    edge1: usize,
    poly2: &PolygonShape,
    xf2: &Transform,
) -> [ClipVertex; 2] {
    let normal1 = mul_t_rot(xf2.q, mul_rot(xf1.q, poly1.normals[edge1]));
    let mut index = 0;
    let mut min_dot = FLOAT_MAX;
    for i in 0..poly2.count {
        let d = dot(normal1, poly2.normals[i]);
        if d < min_dot {
            min_dot = d;
            index = i;
        }
    }
    let i1 = index;
    let i2 = if i1 + 1 < poly2.count { i1 + 1 } else { 0 };
    let make = |i: usize| ClipVertex {
        v: mul_xf(xf2, poly2.vertices[i]),
        id: ContactFeature {
            index_a: edge1 as u8,
            index_b: i as u8,
            type_a: FEATURE_FACE,
            type_b: FEATURE_VERTEX,
        },
    };
    [make(i1), make(i2)]
}

/// The normal points from A to B.
pub fn collide_polygons(
    poly_a: &PolygonShape,
    xf_a: &Transform,
    poly_b: &PolygonShape,
    xf_b: &Transform,
) -> Manifold {
    let mut m = Manifold::default();
    let total_radius = poly_a.radius() + poly_b.radius();

    let (separation_a, edge_a) = find_max_separation(poly_a, xf_a, poly_b, xf_b);
    if separation_a > total_radius {
        return m;
    }
    let (separation_b, edge_b) = find_max_separation(poly_b, xf_b, poly_a, xf_a);
    if separation_b > total_radius {
        return m;
    }

    let k_tol = (0.1f32 as Real) * LINEAR_SLOP;
    let (poly1, poly2, xf1, xf2, edge1, flip);
    if separation_b > separation_a + k_tol {
        (poly1, poly2, xf1, xf2, edge1) = (poly_b, poly_a, xf_b, xf_a, edge_b);
        m.kind = ManifoldType::FaceB;
        flip = true;
    } else {
        (poly1, poly2, xf1, xf2, edge1) = (poly_a, poly_b, xf_a, xf_b, edge_a);
        m.kind = ManifoldType::FaceA;
        flip = false;
    }

    let incident_edge = find_incident_edge(poly1, xf1, edge1, poly2, xf2);

    let count1 = poly1.count;
    let iv1 = edge1;
    let iv2 = if edge1 + 1 < count1 { edge1 + 1 } else { 0 };
    let mut v11 = poly1.vertices[iv1];
    let mut v12 = poly1.vertices[iv2];

    let mut local_tangent = v12 - v11;
    local_tangent.normalize();
    let local_normal = cross_vs(local_tangent, 1.0);
    let plane_point = 0.5 * (v11 + v12);

    let tangent = mul_rot(xf1.q, local_tangent);
    let normal = cross_vs(tangent, 1.0);

    v11 = mul_xf(xf1, v11);
    v12 = mul_xf(xf1, v12);

    let front_offset = dot(normal, v11);
    let side_offset1 = -dot(tangent, v11) + total_radius;
    let side_offset2 = dot(tangent, v12) + total_radius;

    let mut clip_points1 = [ClipVertex::default(); 2];
    let mut clip_points2 = [ClipVertex::default(); 2];
    let np = clip_segment_to_line(
        &mut clip_points1,
        &incident_edge,
        -tangent,
        side_offset1,
        iv1,
    );
    if np < 2 {
        m.point_count = 0;
        return m;
    }
    let np = clip_segment_to_line(&mut clip_points2, &clip_points1, tangent, side_offset2, iv2);
    if np < 2 {
        m.point_count = 0;
        return m;
    }

    m.local_normal = local_normal;
    m.local_point = plane_point;

    let mut point_count = 0;
    for clip in clip_points2.iter().take(MAX_MANIFOLD_POINTS) {
        let separation = dot(normal, clip.v) - front_offset;
        if separation <= total_radius {
            let cp = &mut m.points[point_count];
            cp.local_point = mul_t_xf(xf2, clip.v);
            cp.id = clip.id;
            if flip {
                cp.id = ContactFeature {
                    index_a: clip.id.index_b,
                    index_b: clip.id.index_a,
                    type_a: clip.id.type_b,
                    type_b: clip.id.type_a,
                };
            }
            point_count += 1;
        }
    }
    m.point_count = point_count;
    m
}

/// `b2CollideEdgeAndCircle`: accounts for edge connectivity.
pub fn collide_edge_and_circle(
    edge_a: &EdgeShape,
    xf_a: &Transform,
    circle_b: &CircleShape,
    xf_b: &Transform,
) -> Manifold {
    let mut m = Manifold::default();
    let q = mul_t_xf(xf_a, mul_xf(xf_b, circle_b.p));

    let (a, b) = (edge_a.vertex1, edge_a.vertex2);
    let e = b - a;
    let u = dot(e, b - q);
    let v = dot(e, q - a);
    let radius = edge_a.radius() + circle_b.radius;

    let mut cf = ContactFeature {
        index_b: 0,
        type_b: FEATURE_VERTEX,
        ..Default::default()
    };

    // Region A
    if v <= 0.0 {
        let p = a;
        let d = q - p;
        if dot(d, d) > radius * radius {
            return m;
        }
        if edge_a.has_vertex0 {
            let a1 = edge_a.vertex0;
            let b1 = a;
            let e1 = b1 - a1;
            if dot(e1, b1 - q) > 0.0 {
                return m;
            }
        }
        cf.index_a = 0;
        cf.type_a = FEATURE_VERTEX;
        m.point_count = 1;
        m.kind = ManifoldType::Circles;
        m.local_normal = Vec2::ZERO;
        m.local_point = p;
        m.points[0].id = cf;
        m.points[0].local_point = circle_b.p;
        return m;
    }

    // Region B
    if u <= 0.0 {
        let p = b;
        let d = q - p;
        if dot(d, d) > radius * radius {
            return m;
        }
        if edge_a.has_vertex3 {
            let b2 = edge_a.vertex3;
            let a2 = b;
            let e2 = b2 - a2;
            if dot(e2, q - a2) > 0.0 {
                return m;
            }
        }
        cf.index_a = 1;
        cf.type_a = FEATURE_VERTEX;
        m.point_count = 1;
        m.kind = ManifoldType::Circles;
        m.local_normal = Vec2::ZERO;
        m.local_point = p;
        m.points[0].id = cf;
        m.points[0].local_point = circle_b.p;
        return m;
    }

    // Region AB
    let den = dot(e, e);
    let p = (1.0 / den) * (u * a + v * b);
    let d = q - p;
    if dot(d, d) > radius * radius {
        return m;
    }
    let mut n = vec2(-e.y, e.x);
    if dot(n, q - a) < 0.0 {
        n = vec2(-n.x, -n.y);
    }
    n.normalize();

    cf.index_a = 0;
    cf.type_a = FEATURE_FACE;
    m.point_count = 1;
    m.kind = ManifoldType::FaceA;
    m.local_normal = n;
    m.local_point = a;
    m.points[0].id = cf;
    m.points[0].local_point = circle_b.p;
    m
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EpAxisType {
    Unknown,
    EdgeA,
    EdgeB,
}

#[derive(Clone, Copy)]
struct EpAxis {
    kind: EpAxisType,
    index: i32,
    separation: Real,
}

struct TempPolygon {
    vertices: [Vec2; MAX_POLYGON_VERTICES],
    normals: [Vec2; MAX_POLYGON_VERTICES],
    count: usize,
}

struct ReferenceFace {
    i1: usize,
    i2: usize,
    v1: Vec2,
    v2: Vec2,
    normal: Vec2,
    side_normal1: Vec2,
    side_offset1: Real,
    side_normal2: Vec2,
    side_offset2: Real,
}

struct EpCollider {
    polygon_b: TempPolygon,
    v1: Vec2,
    v2: Vec2,
    normal: Vec2,
    lower_limit: Vec2,
    upper_limit: Vec2,
    radius: Real,
    front: bool,
}

impl EpCollider {
    fn compute_edge_separation(&self) -> EpAxis {
        let mut axis = EpAxis {
            kind: EpAxisType::EdgeA,
            index: if self.front { 0 } else { 1 },
            separation: FLOAT_MAX,
        };
        for i in 0..self.polygon_b.count {
            let s = dot(self.normal, self.polygon_b.vertices[i] - self.v1);
            if s < axis.separation {
                axis.separation = s;
            }
        }
        axis
    }

    fn compute_polygon_separation(&self) -> EpAxis {
        let mut axis = EpAxis {
            kind: EpAxisType::Unknown,
            index: -1,
            separation: -FLOAT_MAX,
        };
        let perp = vec2(-self.normal.y, self.normal.x);
        for i in 0..self.polygon_b.count {
            let n = -self.polygon_b.normals[i];
            let s1 = dot(n, self.polygon_b.vertices[i] - self.v1);
            let s2 = dot(n, self.polygon_b.vertices[i] - self.v2);
            let s = min(s1, s2);
            if s > self.radius {
                axis.kind = EpAxisType::EdgeB;
                axis.index = i as i32;
                axis.separation = s;
                return axis;
            }
            if dot(n, perp) >= 0.0 {
                if dot(n - self.upper_limit, self.normal) < -ANGULAR_SLOP {
                    continue;
                }
            } else if dot(n - self.lower_limit, self.normal) < -ANGULAR_SLOP {
                continue;
            }
            if s > axis.separation {
                axis.kind = EpAxisType::EdgeB;
                axis.index = i as i32;
                axis.separation = s;
            }
        }
        axis
    }
}

/// `b2CollideEdgeAndPolygon` (`b2EPCollider`): accounts for edge adjacency.
pub fn collide_edge_and_polygon(
    edge_a: &EdgeShape,
    xf_a: &Transform,
    polygon_b: &PolygonShape,
    xf_b: &Transform,
) -> Manifold {
    let mut manifold = Manifold::default();
    let xf = mul_t_xx(xf_a, xf_b);
    let centroid_b = mul_xf(&xf, polygon_b.centroid);

    let v0 = edge_a.vertex0;
    let v1 = edge_a.vertex1;
    let v2 = edge_a.vertex2;
    let v3 = edge_a.vertex3;
    let has_vertex0 = edge_a.has_vertex0;
    let has_vertex3 = edge_a.has_vertex3;

    let mut edge1 = v2 - v1;
    edge1.normalize();
    let normal1 = vec2(edge1.y, -edge1.x);
    let offset1 = dot(normal1, centroid_b - v1);
    let mut offset0 = 0.0;
    let mut offset2 = 0.0;
    let mut convex1 = false;
    let mut convex2 = false;
    let mut normal0 = Vec2::ZERO;
    let mut normal2 = Vec2::ZERO;

    if has_vertex0 {
        let mut edge0 = v1 - v0;
        edge0.normalize();
        normal0 = vec2(edge0.y, -edge0.x);
        convex1 = cross(edge0, edge1) >= 0.0;
        offset0 = dot(normal0, centroid_b - v0);
    }
    if has_vertex3 {
        let mut edge2 = v3 - v2;
        edge2.normalize();
        normal2 = vec2(edge2.y, -edge2.x);
        convex2 = cross(edge1, edge2) > 0.0;
        offset2 = dot(normal2, centroid_b - v2);
    }

    let front;
    let normal;
    let lower_limit;
    let upper_limit;
    if has_vertex0 && has_vertex3 {
        if convex1 && convex2 {
            front = offset0 >= 0.0 || offset1 >= 0.0 || offset2 >= 0.0;
            if front {
                normal = normal1;
                lower_limit = normal0;
                upper_limit = normal2;
            } else {
                normal = -normal1;
                lower_limit = -normal1;
                upper_limit = -normal1;
            }
        } else if convex1 {
            front = offset0 >= 0.0 || (offset1 >= 0.0 && offset2 >= 0.0);
            if front {
                normal = normal1;
                lower_limit = normal0;
                upper_limit = normal1;
            } else {
                normal = -normal1;
                lower_limit = -normal2;
                upper_limit = -normal1;
            }
        } else if convex2 {
            front = offset2 >= 0.0 || (offset0 >= 0.0 && offset1 >= 0.0);
            if front {
                normal = normal1;
                lower_limit = normal1;
                upper_limit = normal2;
            } else {
                normal = -normal1;
                lower_limit = -normal1;
                upper_limit = -normal0;
            }
        } else {
            front = offset0 >= 0.0 && offset1 >= 0.0 && offset2 >= 0.0;
            if front {
                normal = normal1;
                lower_limit = normal1;
                upper_limit = normal1;
            } else {
                normal = -normal1;
                lower_limit = -normal2;
                upper_limit = -normal0;
            }
        }
    } else if has_vertex0 {
        if convex1 {
            front = offset0 >= 0.0 || offset1 >= 0.0;
            if front {
                normal = normal1;
                lower_limit = normal0;
                upper_limit = -normal1;
            } else {
                normal = -normal1;
                lower_limit = normal1;
                upper_limit = -normal1;
            }
        } else {
            front = offset0 >= 0.0 && offset1 >= 0.0;
            if front {
                normal = normal1;
                lower_limit = normal1;
                upper_limit = -normal1;
            } else {
                normal = -normal1;
                lower_limit = normal1;
                upper_limit = -normal0;
            }
        }
    } else if has_vertex3 {
        if convex2 {
            front = offset1 >= 0.0 || offset2 >= 0.0;
            if front {
                normal = normal1;
                lower_limit = -normal1;
                upper_limit = normal2;
            } else {
                normal = -normal1;
                lower_limit = -normal1;
                upper_limit = normal1;
            }
        } else {
            front = offset1 >= 0.0 && offset2 >= 0.0;
            if front {
                normal = normal1;
                lower_limit = -normal1;
                upper_limit = normal1;
            } else {
                normal = -normal1;
                lower_limit = -normal2;
                upper_limit = normal1;
            }
        }
    } else {
        front = offset1 >= 0.0;
        if front {
            normal = normal1;
            lower_limit = -normal1;
            upper_limit = -normal1;
        } else {
            normal = -normal1;
            lower_limit = normal1;
            upper_limit = normal1;
        }
    }

    // Polygon B in the frame of A.
    let mut poly = TempPolygon {
        vertices: [Vec2::ZERO; MAX_POLYGON_VERTICES],
        normals: [Vec2::ZERO; MAX_POLYGON_VERTICES],
        count: polygon_b.count,
    };
    for i in 0..polygon_b.count {
        poly.vertices[i] = mul_xf(&xf, polygon_b.vertices[i]);
        poly.normals[i] = mul_rot(xf.q, polygon_b.normals[i]);
    }

    let collider = EpCollider {
        polygon_b: poly,
        v1,
        v2,
        normal,
        lower_limit,
        upper_limit,
        radius: 2.0 * POLYGON_RADIUS,
        front,
    };
    let radius = collider.radius;

    let edge_axis = collider.compute_edge_separation();
    if edge_axis.kind == EpAxisType::Unknown || edge_axis.separation > radius {
        return manifold;
    }
    let polygon_axis = collider.compute_polygon_separation();
    if polygon_axis.kind != EpAxisType::Unknown && polygon_axis.separation > radius {
        return manifold;
    }

    // Hysteresis for jitter reduction.
    let relative_tol = 0.98f32 as Real;
    let absolute_tol = 0.001f32 as Real;
    let primary_axis = if polygon_axis.kind == EpAxisType::Unknown {
        edge_axis
    } else if polygon_axis.separation > relative_tol * edge_axis.separation + absolute_tol {
        polygon_axis
    } else {
        edge_axis
    };

    let poly = &collider.polygon_b;
    let mut ie = [ClipVertex::default(); 2];
    let rf_i1;
    let rf_i2;
    let rf_v1;
    let rf_v2;
    let rf_normal;
    if primary_axis.kind == EpAxisType::EdgeA {
        manifold.kind = ManifoldType::FaceA;

        // The polygon normal most anti-parallel to the edge normal.
        let mut best_index = 0;
        let mut best_value = dot(normal, poly.normals[0]);
        for i in 1..poly.count {
            let value = dot(normal, poly.normals[i]);
            if value < best_value {
                best_value = value;
                best_index = i;
            }
        }
        let i1 = best_index;
        let i2 = if i1 + 1 < poly.count { i1 + 1 } else { 0 };

        ie[0].v = poly.vertices[i1];
        ie[0].id = ContactFeature {
            index_a: 0,
            index_b: i1 as u8,
            type_a: FEATURE_FACE,
            type_b: FEATURE_VERTEX,
        };
        ie[1].v = poly.vertices[i2];
        ie[1].id = ContactFeature {
            index_a: 0,
            index_b: i2 as u8,
            type_a: FEATURE_FACE,
            type_b: FEATURE_VERTEX,
        };

        if front {
            rf_i1 = 0;
            rf_i2 = 1;
            rf_v1 = v1;
            rf_v2 = v2;
            rf_normal = normal1;
        } else {
            rf_i1 = 1;
            rf_i2 = 0;
            rf_v1 = v2;
            rf_v2 = v1;
            rf_normal = -normal1;
        }
    } else {
        manifold.kind = ManifoldType::FaceB;
        let idx = primary_axis.index as usize;
        ie[0].v = v1;
        ie[0].id = ContactFeature {
            index_a: 0,
            index_b: idx as u8,
            type_a: FEATURE_VERTEX,
            type_b: FEATURE_FACE,
        };
        ie[1].v = v2;
        ie[1].id = ContactFeature {
            index_a: 0,
            index_b: idx as u8,
            type_a: FEATURE_VERTEX,
            type_b: FEATURE_FACE,
        };
        rf_i1 = idx;
        rf_i2 = if rf_i1 + 1 < poly.count { rf_i1 + 1 } else { 0 };
        rf_v1 = poly.vertices[rf_i1];
        rf_v2 = poly.vertices[rf_i2];
        rf_normal = poly.normals[rf_i1];
    }

    let mut rf = ReferenceFace {
        i1: rf_i1,
        i2: rf_i2,
        v1: rf_v1,
        v2: rf_v2,
        normal: rf_normal,
        side_normal1: Vec2::ZERO,
        side_offset1: 0.0,
        side_normal2: Vec2::ZERO,
        side_offset2: 0.0,
    };
    rf.side_normal1 = vec2(rf.normal.y, -rf.normal.x);
    rf.side_normal2 = -rf.side_normal1;
    rf.side_offset1 = dot(rf.side_normal1, rf.v1);
    rf.side_offset2 = dot(rf.side_normal2, rf.v2);

    let mut clip_points1 = [ClipVertex::default(); 2];
    let mut clip_points2 = [ClipVertex::default(); 2];
    let np = clip_segment_to_line(
        &mut clip_points1,
        &ie,
        rf.side_normal1,
        rf.side_offset1,
        rf.i1,
    );
    if np < MAX_MANIFOLD_POINTS {
        return manifold;
    }
    let np = clip_segment_to_line(
        &mut clip_points2,
        &clip_points1,
        rf.side_normal2,
        rf.side_offset2,
        rf.i2,
    );
    if np < MAX_MANIFOLD_POINTS {
        return manifold;
    }

    if primary_axis.kind == EpAxisType::EdgeA {
        manifold.local_normal = rf.normal;
        manifold.local_point = rf.v1;
    } else {
        manifold.local_normal = polygon_b.normals[rf.i1];
        manifold.local_point = polygon_b.vertices[rf.i1];
    }

    let mut point_count = 0;
    for cp in clip_points2.iter().take(MAX_MANIFOLD_POINTS) {
        let separation = dot(rf.normal, cp.v - rf.v1);
        if separation <= radius {
            let out = &mut manifold.points[point_count];
            if primary_axis.kind == EpAxisType::EdgeA {
                out.local_point = mul_t_xf(&xf, cp.v);
                out.id = cp.id;
            } else {
                out.local_point = cp.v;
                out.id = ContactFeature {
                    type_a: cp.id.type_b,
                    type_b: cp.id.type_a,
                    index_a: cp.id.index_b,
                    index_b: cp.id.index_a,
                };
            }
            point_count += 1;
        }
    }
    manifold.point_count = point_count;
    manifold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacked_boxes_make_two_points_with_upward_normal() {
        let a = PolygonShape::as_box(1.0, 1.0);
        let b = PolygonShape::as_box(1.0, 1.0);
        let xf_a = Transform::new(vec2(0.0, 0.0), 0.0);
        let xf_b = Transform::new(vec2(0.0, 1.99), 0.0);
        let m = collide_polygons(&a, &xf_a, &b, &xf_b);
        assert_eq!(m.point_count, 2);
        let w = WorldManifold::initialize(&m, &xf_a, a.radius(), &xf_b, b.radius());
        assert!((w.normal.y - 1.0).abs() < 1e-6 && w.normal.x.abs() < 1e-6);
        assert!(w.separations.iter().all(|&s| s < 0.0));
    }

    #[test]
    fn separated_boxes_make_no_manifold() {
        let a = PolygonShape::as_box(1.0, 1.0);
        let m = collide_polygons(
            &a,
            &Transform::IDENTITY,
            &a,
            &Transform::new(vec2(0.0, 2.5), 0.0),
        );
        assert_eq!(m.point_count, 0);
    }

    #[test]
    fn circles_and_polygon_circle_touch() {
        let c = CircleShape::new(0.5);
        let m = collide_circles(
            &c,
            &Transform::IDENTITY,
            &c,
            &Transform::new(vec2(0.9, 0.0), 0.0),
        );
        assert_eq!(m.point_count, 1);
        let p = PolygonShape::as_box(1.0, 1.0);
        let m = collide_polygon_and_circle(
            &p,
            &Transform::IDENTITY,
            &c,
            &Transform::new(vec2(0.0, 1.4), 0.0),
        );
        assert_eq!(m.point_count, 1);
        assert_eq!(m.kind, ManifoldType::FaceA);
        let far = collide_polygon_and_circle(
            &p,
            &Transform::IDENTITY,
            &c,
            &Transform::new(vec2(0.0, 2.0), 0.0),
        );
        assert_eq!(far.point_count, 0);
    }

    #[test]
    fn contact_feature_key_is_little_endian() {
        let f = ContactFeature {
            index_a: 1,
            index_b: 2,
            type_a: 3,
            type_b: 4,
        };
        assert_eq!(f.key(), 0x0403_0201);
    }
}

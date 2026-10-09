//! `b2ContactSolver.cpp` (Box2D 2.3.1): sequential-impulse solver with the 2-point block solver.

use crate::collide::{Manifold, ManifoldType, WorldManifold};
use crate::math::*;
use crate::settings::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Position {
    pub c: Vec2,
    pub a: Real,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity {
    pub v: Vec2,
    pub w: Real,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TimeStep {
    pub dt: Real,
    pub inv_dt: Real,
    pub dt_ratio: Real,
    pub velocity_iterations: i32,
    pub position_iterations: i32,
    pub warm_starting: bool,
}

/// Everything the solver reads from a touching contact and its two bodies.
#[derive(Clone, Copy, Debug)]
pub struct SolverContact {
    pub manifold: Manifold,
    pub friction: Real,
    pub restitution: Real,
    pub tangent_speed: Real,
    /// Island body indices.
    pub index_a: usize,
    pub index_b: usize,
    pub inv_mass_a: Real,
    pub inv_mass_b: Real,
    pub inv_i_a: Real,
    pub inv_i_b: Real,
    pub local_center_a: Vec2,
    pub local_center_b: Vec2,
    pub radius_a: Real,
    pub radius_b: Real,
}

#[derive(Clone, Copy, Debug, Default)]
struct VelocityConstraintPoint {
    r_a: Vec2,
    r_b: Vec2,
    normal_impulse: Real,
    tangent_impulse: Real,
    normal_mass: Real,
    tangent_mass: Real,
    velocity_bias: Real,
}

#[derive(Clone, Copy, Debug, Default)]
struct VelocityConstraint {
    points: [VelocityConstraintPoint; MAX_MANIFOLD_POINTS],
    normal: Vec2,
    normal_mass: Mat22,
    k: Mat22,
    index_a: usize,
    index_b: usize,
    inv_mass_a: Real,
    inv_mass_b: Real,
    inv_i_a: Real,
    inv_i_b: Real,
    friction: Real,
    restitution: Real,
    tangent_speed: Real,
    point_count: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct PositionConstraint {
    local_points: [Vec2; MAX_MANIFOLD_POINTS],
    local_normal: Vec2,
    local_point: Vec2,
    index_a: usize,
    index_b: usize,
    inv_mass_a: Real,
    inv_mass_b: Real,
    local_center_a: Vec2,
    local_center_b: Vec2,
    inv_i_a: Real,
    inv_i_b: Real,
    kind: ManifoldType,
    radius_a: Real,
    radius_b: Real,
    point_count: usize,
}

pub struct ContactSolver {
    step: TimeStep,
    contacts: Vec<SolverContact>,
    position_constraints: Vec<PositionConstraint>,
    velocity_constraints: Vec<VelocityConstraint>,
}

const BLOCK_SOLVE: bool = true;

/// Noita's association of the scalar effective-mass sum: `((t1 + mB) + mA) + t2`.
fn k_sum(t1: Real, m_a: Real, m_b: Real, t2: Real) -> Real {
    ((t1 + m_b) + m_a) + t2
}

impl ContactSolver {
    pub fn new(step: TimeStep, contacts: Vec<SolverContact>) -> Self {
        let mut position_constraints = Vec::with_capacity(contacts.len());
        let mut velocity_constraints = Vec::with_capacity(contacts.len());
        for contact in &contacts {
            let manifold = &contact.manifold;
            let point_count = manifold.point_count;
            let mut vc = VelocityConstraint {
                friction: contact.friction,
                restitution: contact.restitution,
                tangent_speed: contact.tangent_speed,
                index_a: contact.index_a,
                index_b: contact.index_b,
                inv_mass_a: contact.inv_mass_a,
                inv_mass_b: contact.inv_mass_b,
                inv_i_a: contact.inv_i_a,
                inv_i_b: contact.inv_i_b,
                point_count,
                ..Default::default()
            };
            let mut pc = PositionConstraint {
                index_a: contact.index_a,
                index_b: contact.index_b,
                inv_mass_a: contact.inv_mass_a,
                inv_mass_b: contact.inv_mass_b,
                local_center_a: contact.local_center_a,
                local_center_b: contact.local_center_b,
                inv_i_a: contact.inv_i_a,
                inv_i_b: contact.inv_i_b,
                local_normal: manifold.local_normal,
                local_point: manifold.local_point,
                point_count,
                radius_a: contact.radius_a,
                radius_b: contact.radius_b,
                kind: manifold.kind,
                ..Default::default()
            };
            for j in 0..point_count {
                let cp = &manifold.points[j];
                let vcp = &mut vc.points[j];
                if step.warm_starting {
                    vcp.normal_impulse = step.dt_ratio * cp.normal_impulse;
                    vcp.tangent_impulse = step.dt_ratio * cp.tangent_impulse;
                } else {
                    vcp.normal_impulse = 0.0;
                    vcp.tangent_impulse = 0.0;
                }
                pc.local_points[j] = cp.local_point;
            }
            velocity_constraints.push(vc);
            position_constraints.push(pc);
        }
        ContactSolver {
            step,
            contacts,
            position_constraints,
            velocity_constraints,
        }
    }

    pub fn initialize_velocity_constraints(
        &mut self,
        positions: &[Position],
        velocities: &[Velocity],
    ) {
        for i in 0..self.contacts.len() {
            let pc = self.position_constraints[i];
            let manifold = self.contacts[i].manifold;
            let vc = &mut self.velocity_constraints[i];

            let (radius_a, radius_b) = (pc.radius_a, pc.radius_b);
            let (m_a, m_b) = (vc.inv_mass_a, vc.inv_mass_b);
            let (i_a, i_b) = (vc.inv_i_a, vc.inv_i_b);

            let c_a = positions[vc.index_a].c;
            let a_a = positions[vc.index_a].a;
            let v_a = velocities[vc.index_a].v;
            let w_a = velocities[vc.index_a].w;
            let c_b = positions[vc.index_b].c;
            let a_b = positions[vc.index_b].a;
            let v_b = velocities[vc.index_b].v;
            let w_b = velocities[vc.index_b].w;

            let mut xf_a = Transform {
                q: Rot::new(a_a),
                ..Default::default()
            };
            let mut xf_b = Transform {
                q: Rot::new(a_b),
                ..Default::default()
            };
            xf_a.p = c_a - mul_rot(xf_a.q, pc.local_center_a);
            xf_b.p = c_b - mul_rot(xf_b.q, pc.local_center_b);

            let world_manifold =
                WorldManifold::initialize(&manifold, &xf_a, radius_a, &xf_b, radius_b);
            vc.normal = world_manifold.normal;

            for j in 0..vc.point_count {
                let vcp = &mut vc.points[j];
                vcp.r_a = world_manifold.points[j] - c_a;
                vcp.r_b = world_manifold.points[j] - c_b;

                let rn_a = cross(vcp.r_a, vc.normal);
                let rn_b = cross(vcp.r_b, vc.normal);
                let k_normal = k_sum(i_a * rn_a * rn_a, m_a, m_b, i_b * rn_b * rn_b);
                vcp.normal_mass = if k_normal > 0.0 { 1.0 / k_normal } else { 0.0 };

                let tangent = cross_vs(vc.normal, 1.0);
                let rt_a = cross(vcp.r_a, tangent);
                let rt_b = cross(vcp.r_b, tangent);
                let k_tangent = k_sum(i_a * rt_a * rt_a, m_a, m_b, i_b * rt_b * rt_b);
                vcp.tangent_mass = if k_tangent > 0.0 {
                    1.0 / k_tangent
                } else {
                    0.0
                };

                vcp.velocity_bias = 0.0;
                let v_rel = dot(
                    vc.normal,
                    v_b + cross_sv(w_b, vcp.r_b) - v_a - cross_sv(w_a, vcp.r_a),
                );
                if v_rel < -VELOCITY_THRESHOLD {
                    vcp.velocity_bias = -vc.restitution * v_rel;
                }
            }

            if vc.point_count == 2 && BLOCK_SOLVE {
                let (vcp1, vcp2) = (vc.points[0], vc.points[1]);
                let rn1_a = cross(vcp1.r_a, vc.normal);
                let rn1_b = cross(vcp1.r_b, vc.normal);
                let rn2_a = cross(vcp2.r_a, vc.normal);
                let rn2_b = cross(vcp2.r_b, vc.normal);

                let s = m_b + m_a;
                let k11 = (i_a * rn1_a * rn1_a + s) + i_b * rn1_b * rn1_b;
                let k22 = (i_a * rn2_a * rn2_a + s) + i_b * rn2_b * rn2_b;
                let k12 = (i_a * rn1_a * rn2_a + s) + i_b * rn1_b * rn2_b;

                const MAX_CONDITION_NUMBER: Real = 1000.0;
                if k11 * k11 < MAX_CONDITION_NUMBER * (k11 * k22 - k12 * k12) {
                    vc.k.ex = vec2(k11, k12);
                    vc.k.ey = vec2(k12, k22);
                    vc.normal_mass = vc.k.inverse();
                } else {
                    vc.point_count = 1;
                }
            }
        }
    }

    pub fn warm_start(&mut self, velocities: &mut [Velocity]) {
        for vc in &self.velocity_constraints {
            let (index_a, index_b) = (vc.index_a, vc.index_b);
            let (m_a, i_a) = (vc.inv_mass_a, vc.inv_i_a);
            let (m_b, i_b) = (vc.inv_mass_b, vc.inv_i_b);

            let mut v_a = velocities[index_a].v;
            let mut w_a = velocities[index_a].w;
            let mut v_b = velocities[index_b].v;
            let mut w_b = velocities[index_b].w;

            let normal = vc.normal;
            let tangent = cross_vs(normal, 1.0);
            for vcp in &vc.points[..vc.point_count] {
                // Noita zeroes angular velocities at or below float epsilon before each point.
                if w_b.abs() <= EPSILON {
                    w_b = 0.0;
                }
                if w_a.abs() <= EPSILON {
                    w_a = 0.0;
                }
                let p = vcp.normal_impulse * normal + vcp.tangent_impulse * tangent;
                w_a -= i_a * cross(vcp.r_a, p);
                v_a -= m_a * p;
                w_b += i_b * cross(vcp.r_b, p);
                v_b += m_b * p;
            }

            velocities[index_a].v = v_a;
            velocities[index_a].w = w_a;
            velocities[index_b].v = v_b;
            velocities[index_b].w = w_b;
        }
    }

    pub fn solve_velocity_constraints(&mut self, velocities: &mut [Velocity]) {
        for vc in &mut self.velocity_constraints {
            let (index_a, index_b) = (vc.index_a, vc.index_b);
            let (m_a, i_a) = (vc.inv_mass_a, vc.inv_i_a);
            let (m_b, i_b) = (vc.inv_mass_b, vc.inv_i_b);
            let point_count = vc.point_count;

            let mut v_a = velocities[index_a].v;
            let mut w_a = velocities[index_a].w;
            let mut v_b = velocities[index_b].v;
            let mut w_b = velocities[index_b].w;

            let normal = vc.normal;
            let tangent = cross_vs(normal, 1.0);
            let friction = vc.friction;

            // Friction first: non-penetration matters more.
            for j in 0..point_count {
                let vcp = &mut vc.points[j];
                let dv = v_b + cross_sv(w_b, vcp.r_b) - v_a - cross_sv(w_a, vcp.r_a);
                let vt = dot(dv, tangent) - vc.tangent_speed;
                let mut lambda = vcp.tangent_mass * (-vt);

                let max_friction = friction * vcp.normal_impulse;
                let new_impulse = clamp(vcp.tangent_impulse + lambda, -max_friction, max_friction);
                lambda = new_impulse - vcp.tangent_impulse;
                vcp.tangent_impulse = new_impulse;

                let p = lambda * tangent;
                v_a -= m_a * p;
                w_a -= i_a * cross(vcp.r_a, p);
                v_b += m_b * p;
                w_b += i_b * cross(vcp.r_b, p);
            }

            if point_count == 1 || !BLOCK_SOLVE {
                for j in 0..point_count {
                    let vcp = &mut vc.points[j];
                    let dv = v_b + cross_sv(w_b, vcp.r_b) - v_a - cross_sv(w_a, vcp.r_a);
                    let vn = dot(dv, normal);
                    let mut lambda = -vcp.normal_mass * (vn - vcp.velocity_bias);

                    let new_impulse = max(vcp.normal_impulse + lambda, 0.0);
                    lambda = new_impulse - vcp.normal_impulse;
                    vcp.normal_impulse = new_impulse;

                    let p = lambda * normal;
                    v_a -= m_a * p;
                    w_a -= i_a * cross(vcp.r_a, p);
                    v_b += m_b * p;
                    w_b += i_b * cross(vcp.r_b, p);
                }
            } else {
                // Block solver: total enumeration of the two-point LCP.
                let (cp1, cp2) = (vc.points[0], vc.points[1]);
                let a = vec2(cp1.normal_impulse, cp2.normal_impulse);

                let dv1 = v_b + cross_sv(w_b, cp1.r_b) - v_a - cross_sv(w_a, cp1.r_a);
                let dv2 = v_b + cross_sv(w_b, cp2.r_b) - v_a - cross_sv(w_a, cp2.r_a);
                let vn1 = dot(dv1, normal);
                let vn2 = dot(dv2, normal);

                let mut b = vec2(vn1 - cp1.velocity_bias, vn2 - cp2.velocity_bias);
                b -= mul_mat22(&vc.k, a);

                // Applies the new total impulse `x` to the bodies and stores it.
                let mut apply = |x: Vec2, vc: &mut VelocityConstraint| {
                    let d = x - a;
                    let p1 = d.x * normal;
                    let p2 = d.y * normal;
                    v_a -= m_a * (p1 + p2);
                    w_a -= i_a * (cross(cp1.r_a, p1) + cross(cp2.r_a, p2));
                    v_b += m_b * (p1 + p2);
                    w_b += i_b * (cross(cp1.r_b, p1) + cross(cp2.r_b, p2));
                    vc.points[0].normal_impulse = x.x;
                    vc.points[1].normal_impulse = x.y;
                };

                // Case 1: vn = 0 at both points.
                let x = -mul_mat22(&vc.normal_mass, b);
                if x.x >= 0.0 && x.y >= 0.0 {
                    apply(x, vc);
                } else {
                    // Case 2: vn1 = 0 and x2 = 0.
                    let x = vec2(-cp1.normal_mass * b.x, 0.0);
                    let vn2 = vc.k.ex.y * x.x + b.y;
                    if x.x >= 0.0 && vn2 >= 0.0 {
                        apply(x, vc);
                    } else {
                        // Case 3: vn2 = 0 and x1 = 0.
                        let x = vec2(0.0, -cp2.normal_mass * b.y);
                        let vn1 = vc.k.ey.x * x.y + b.x;
                        if x.y >= 0.0 && vn1 >= 0.0 {
                            apply(x, vc);
                        } else if b.x >= 0.0 && b.y >= 0.0 {
                            // Case 4: x1 = 0 and x2 = 0. Otherwise there is no solution.
                            apply(vec2(0.0, 0.0), vc);
                        }
                    }
                }
            }

            velocities[index_a].v = v_a;
            velocities[index_a].w = w_a;
            velocities[index_b].v = v_b;
            velocities[index_b].w = w_b;
        }
    }

    /// Copies the accumulated impulses back to the contact manifolds (same order as the input).
    pub fn store_impulses(&self, manifolds: &mut [Manifold]) {
        for (vc, manifold) in self.velocity_constraints.iter().zip(manifolds.iter_mut()) {
            for j in 0..vc.point_count {
                manifold.points[j].normal_impulse = vc.points[j].normal_impulse;
                manifold.points[j].tangent_impulse = vc.points[j].tangent_impulse;
            }
        }
    }

    pub fn solve_position_constraints(&self, positions: &mut [Position]) -> bool {
        self.solve_positions(positions, BAUMGARTE, -3.0 * LINEAR_SLOP, None)
    }

    pub fn solve_toi_position_constraints(
        &self,
        positions: &mut [Position],
        toi_index_a: usize,
        toi_index_b: usize,
    ) -> bool {
        self.solve_positions(
            positions,
            TOI_BAUMGARTE,
            -1.5 * LINEAR_SLOP,
            Some((toi_index_a, toi_index_b)),
        )
    }

    fn solve_positions(
        &self,
        positions: &mut [Position],
        baumgarte: Real,
        min_allowed: Real,
        toi: Option<(usize, usize)>,
    ) -> bool {
        let mut min_separation = 0.0;
        for pc in &self.position_constraints {
            let (index_a, index_b) = (pc.index_a, pc.index_b);
            let (mut m_a, mut i_a, mut m_b, mut i_b) =
                (pc.inv_mass_a, pc.inv_i_a, pc.inv_mass_b, pc.inv_i_b);
            if let Some((ta, tb)) = toi {
                if !(index_a == ta || index_a == tb) {
                    m_a = 0.0;
                    i_a = 0.0;
                }
                if !(index_b == ta || index_b == tb) {
                    m_b = 0.0;
                    i_b = 0.0;
                }
            }

            let mut c_a = positions[index_a].c;
            let mut a_a = positions[index_a].a;
            let mut c_b = positions[index_b].c;
            let mut a_b = positions[index_b].a;

            for j in 0..pc.point_count {
                let mut xf_a = Transform {
                    q: Rot::new(a_a),
                    ..Default::default()
                };
                let mut xf_b = Transform {
                    q: Rot::new(a_b),
                    ..Default::default()
                };
                xf_a.p = c_a - mul_rot(xf_a.q, pc.local_center_a);
                xf_b.p = c_b - mul_rot(xf_b.q, pc.local_center_b);

                let (normal, point, separation) = position_solver_manifold(pc, &xf_a, &xf_b, j);

                let r_a = point - c_a;
                let r_b = point - c_b;

                min_separation = min(min_separation, separation);

                let c = clamp(
                    baumgarte * (separation + LINEAR_SLOP),
                    -MAX_LINEAR_CORRECTION,
                    0.0,
                );

                let rn_a = cross(r_a, normal);
                let rn_b = cross(r_b, normal);
                let k = (i_a * rn_a * rn_a + (m_b + m_a)) + i_b * rn_b * rn_b;
                // The correction is a float32 local in the original.
                let impulse = if k > 0.0 {
                    -(Real::from(c as f32) / k)
                } else {
                    0.0
                };
                let p = impulse * normal;

                c_a -= m_a * p;
                a_a -= i_a * cross(r_a, p);
                c_b += m_b * p;
                a_b += i_b * cross(r_b, p);
            }

            positions[index_a].c = c_a;
            positions[index_a].a = a_a;
            positions[index_b].c = c_b;
            positions[index_b].a = a_b;
        }
        min_separation >= min_allowed
    }

    pub fn step(&self) -> &TimeStep {
        &self.step
    }
}

/// `b2PositionSolverManifold`: returns (normal, point, separation).
fn position_solver_manifold(
    pc: &PositionConstraint,
    xf_a: &Transform,
    xf_b: &Transform,
    index: usize,
) -> (Vec2, Vec2, Real) {
    match pc.kind {
        ManifoldType::Circles => {
            let point_a = mul_xf(xf_a, pc.local_point);
            let point_b = mul_xf(xf_b, pc.local_points[0]);
            let mut normal = point_b - point_a;
            normal.normalize();
            let point = 0.5 * (point_a + point_b);
            let separation = dot(point_b - point_a, normal) - pc.radius_a - pc.radius_b;
            (normal, point, separation)
        }
        ManifoldType::FaceA => {
            let normal = mul_rot(xf_a.q, pc.local_normal);
            let plane_point = mul_xf(xf_a, pc.local_point);
            let clip_point = mul_xf(xf_b, pc.local_points[index]);
            let separation = dot(clip_point - plane_point, normal) - pc.radius_a - pc.radius_b;
            (normal, clip_point, separation)
        }
        ManifoldType::FaceB => {
            let normal = mul_rot(xf_b.q, pc.local_normal);
            let plane_point = mul_xf(xf_b, pc.local_point);
            let clip_point = mul_xf(xf_a, pc.local_points[index]);
            let separation = dot(clip_point - plane_point, normal) - pc.radius_a - pc.radius_b;
            (-normal, clip_point, separation)
        }
    }
}

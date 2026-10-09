//! Rigid bodies, fixtures, contacts and the world step (`b2Body`, `b2Fixture`, `b2Contact`,
//! `b2ContactManager`, `b2Island`, `b2World`; Box2D 2.3.1).
//!
//! Objects live in arenas addressed by id. The original's intrusive linked lists (world body list,
//! world contact list, per-body fixture and contact lists) keep their exact insert-at-head order,
//! because island building and TOI search iterate them and the order decides the results.
//! Joints, edge shapes and chain shapes are not ported yet.

use crate::broad_phase::BroadPhase;
use crate::collide::*;
use crate::contact_solver::*;
use crate::distance::test_overlap_shapes;
use crate::math::*;
use crate::settings::*;
use crate::shape::{Aabb, MassData, PolygonShape, RayCastInput, Shape};
use crate::toi::{ToiInput, ToiState, time_of_impact};

pub type BodyId = usize;
pub type FixtureId = usize;
pub type ContactId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyType {
    Static,
    Kinematic,
    Dynamic,
}

#[derive(Clone, Copy, Debug)]
pub struct BodyDef {
    pub kind: BodyType,
    pub position: Vec2,
    pub angle: Real,
    pub linear_velocity: Vec2,
    pub angular_velocity: Real,
    pub linear_damping: Real,
    pub angular_damping: Real,
    pub allow_sleep: bool,
    pub awake: bool,
    pub fixed_rotation: bool,
    pub bullet: bool,
    pub active: bool,
    pub gravity_scale: Real,
    pub user_data: usize,
}

impl Default for BodyDef {
    fn default() -> Self {
        BodyDef {
            kind: BodyType::Static,
            position: Vec2::ZERO,
            angle: 0.0,
            linear_velocity: Vec2::ZERO,
            angular_velocity: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            allow_sleep: true,
            awake: true,
            fixed_rotation: false,
            bullet: false,
            active: true,
            gravity_scale: 1.0,
            user_data: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    pub category_bits: u16,
    pub mask_bits: u16,
    pub group_index: i16,
}

impl Default for Filter {
    fn default() -> Self {
        Filter {
            category_bits: 0x0001,
            mask_bits: 0xFFFF,
            group_index: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FixtureDef {
    pub shape: Shape,
    pub friction: Real,
    pub restitution: Real,
    pub density: Real,
    pub is_sensor: bool,
    pub filter: Filter,
    pub user_data: usize,
}

impl FixtureDef {
    pub fn new(shape: Shape, density: Real) -> Self {
        FixtureDef {
            shape,
            friction: 0.2f32 as Real,
            restitution: 0.0,
            density,
            is_sensor: false,
            filter: Filter::default(),
            user_data: 0,
        }
    }
}

const BODY_ISLAND: u16 = 0x0001;
const BODY_AWAKE: u16 = 0x0002;
const BODY_AUTO_SLEEP: u16 = 0x0004;
const BODY_BULLET: u16 = 0x0008;
const BODY_FIXED_ROTATION: u16 = 0x0010;
const BODY_ACTIVE: u16 = 0x0020;

const CONTACT_ISLAND: u32 = 0x0001;
const CONTACT_TOUCHING: u32 = 0x0002;
const CONTACT_ENABLED: u32 = 0x0004;
const CONTACT_FILTER: u32 = 0x0008;
const CONTACT_TOI: u32 = 0x0020;

const WORLD_NEW_FIXTURE: u32 = 0x0001;
const WORLD_LOCKED: u32 = 0x0002;
const WORLD_CLEAR_FORCES: u32 = 0x0004;

/// A contact edge: one of a contact's two nodes (`false` = node A, `true` = node B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeRef {
    pub contact: ContactId,
    pub side_b: bool,
}

#[derive(Clone, Copy, Debug)]
struct ContactEdge {
    other: BodyId,
    prev: Option<EdgeRef>,
    next: Option<EdgeRef>,
}

#[derive(Clone, Debug)]
pub struct FixtureProxy {
    pub aabb: Aabb,
    pub proxy_id: i32,
    pub child_index: usize,
}

#[derive(Clone, Debug)]
pub struct Fixture {
    pub shape: Shape,
    pub density: Real,
    pub friction: Real,
    pub restitution: Real,
    pub is_sensor: bool,
    pub filter: Filter,
    pub user_data: usize,
    body: BodyId,
    next: Option<FixtureId>,
    proxies: Vec<FixtureProxy>,
}

impl Fixture {
    pub fn body(&self) -> BodyId {
        self.body
    }
}

#[derive(Clone, Debug)]
pub struct Body {
    pub kind: BodyType,
    flags: u16,
    island_index: usize,
    pub xf: Transform,
    pub sweep: Sweep,
    pub linear_velocity: Vec2,
    pub angular_velocity: Real,
    pub force: Vec2,
    pub torque: Real,
    prev: Option<BodyId>,
    next: Option<BodyId>,
    fixture_list: Option<FixtureId>,
    pub fixture_count: usize,
    contact_list: Option<EdgeRef>,
    pub mass: Real,
    pub inv_mass: Real,
    /// Rotational inertia about the center of mass.
    pub i: Real,
    pub inv_i: Real,
    pub linear_damping: Real,
    pub angular_damping: Real,
    pub gravity_scale: Real,
    pub sleep_time: Real,
    pub user_data: usize,
}

impl Body {
    pub fn is_awake(&self) -> bool {
        self.flags & BODY_AWAKE != 0
    }

    pub fn is_active(&self) -> bool {
        self.flags & BODY_ACTIVE != 0
    }

    pub fn is_bullet(&self) -> bool {
        self.flags & BODY_BULLET != 0
    }

    pub fn position(&self) -> Vec2 {
        self.xf.p
    }

    pub fn angle(&self) -> Real {
        self.sweep.a
    }

    pub fn world_center(&self) -> Vec2 {
        self.sweep.c
    }

    pub fn local_center(&self) -> Vec2 {
        self.sweep.local_center
    }

    pub fn inertia(&self) -> Real {
        self.i + self.mass * dot(self.sweep.local_center, self.sweep.local_center)
    }

    pub fn world_point(&self, local: Vec2) -> Vec2 {
        mul_xf(&self.xf, local)
    }

    pub fn world_vector(&self, local: Vec2) -> Vec2 {
        mul_rot(self.xf.q, local)
    }

    pub fn linear_velocity_from_world_point(&self, p: Vec2) -> Vec2 {
        self.linear_velocity + cross_sv(self.angular_velocity, p - self.sweep.c)
    }

    fn set_awake(&mut self, flag: bool) {
        if flag {
            if self.flags & BODY_AWAKE == 0 {
                self.flags |= BODY_AWAKE;
                self.sleep_time = 0.0;
            }
        } else {
            self.flags &= !BODY_AWAKE;
            self.sleep_time = 0.0;
            self.linear_velocity = Vec2::ZERO;
            self.angular_velocity = 0.0;
            self.force = Vec2::ZERO;
            self.torque = 0.0;
        }
    }

    fn synchronize_transform(&mut self) {
        self.xf.q = Rot::new(self.sweep.a);
        self.xf.p = self.sweep.c - mul_rot(self.xf.q, self.sweep.local_center);
    }

    /// Advance to the new safe time; the broad phase is not synchronized.
    fn advance(&mut self, alpha: Real) {
        self.sweep.advance(alpha);
        self.sweep.c = self.sweep.c0;
        self.sweep.a = self.sweep.a0;
        self.xf.q = Rot::new(self.sweep.a);
        self.xf.p = self.sweep.c - mul_rot(self.xf.q, self.sweep.local_center);
    }
}

#[derive(Clone, Debug)]
pub struct Contact {
    flags: u32,
    prev: Option<ContactId>,
    next: Option<ContactId>,
    node_a: ContactEdge,
    node_b: ContactEdge,
    pub fixture_a: FixtureId,
    pub fixture_b: FixtureId,
    pub index_a: usize,
    pub index_b: usize,
    pub manifold: Manifold,
    toi_count: i32,
    toi: Real,
    pub friction: Real,
    pub restitution: Real,
    pub tangent_speed: Real,
}

impl Contact {
    pub fn is_touching(&self) -> bool {
        self.flags & CONTACT_TOUCHING != 0
    }

    pub fn is_enabled(&self) -> bool {
        self.flags & CONTACT_ENABLED != 0
    }
}

/// A contact begin/end notification, drained by the caller with [`World::take_events`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactEvent {
    Begin(FixtureId, FixtureId),
    End(FixtureId, FixtureId),
}

fn mix_friction(a: Real, b: Real) -> Real {
    (a * b).sqrt()
}

fn mix_restitution(a: Real, b: Real) -> Real {
    if a > b { a } else { b }
}

/// Position integration of `b2Island::Solve` in the operation order of Noita's build (the
/// translation test multiplies left to right instead of squaring `h * v`).
fn integrate_position(
    h: Real,
    mut c: Vec2,
    mut a: Real,
    mut v: Vec2,
    mut w: Real,
) -> (Vec2, Real, Vec2, Real) {
    let d = ((v.x * h) * v.x) * h + ((v.y * h) * v.y) * h;
    if MAX_TRANSLATION_SQUARED < d {
        let ratio = MAX_TRANSLATION / d.sqrt();
        v = vec2(v.x * ratio, v.y * ratio);
    }
    let rotation = w * h;
    if MAX_ROTATION_SQUARED < rotation * rotation {
        w *= MAX_ROTATION / abs(rotation);
    }
    c.x += v.x * h;
    c.y += v.y * h;
    a += w * h;
    (c, a, v, w)
}

fn pack_proxy(fixture: FixtureId, child: usize) -> usize {
    (fixture << 24) | child
}

fn unpack_proxy(data: usize) -> (FixtureId, usize) {
    (data >> 24, data & 0xFF_FFFF)
}

/// `b2ContactFilter::ShouldCollide`.
fn default_should_collide(a: &Filter, b: &Filter) -> bool {
    if a.group_index == b.group_index && a.group_index != 0 {
        return a.group_index > 0;
    }
    (a.mask_bits & b.category_bits) != 0 && (a.category_bits & b.mask_bits) != 0
}

pub struct World {
    bodies: Vec<Option<Body>>,
    free_bodies: Vec<BodyId>,
    fixtures: Vec<Option<Fixture>>,
    free_fixtures: Vec<FixtureId>,
    contacts: Vec<Option<Contact>>,
    free_contacts: Vec<ContactId>,

    body_list: Option<BodyId>,
    body_count: usize,
    contact_list: Option<ContactId>,
    contact_count: usize,
    broad_phase: BroadPhase,

    gravity: Vec2,
    flags: u32,
    inv_dt0: Real,
    pub warm_starting: bool,
    pub continuous_physics: bool,
    pub sub_stepping: bool,
    step_complete: bool,
    allow_sleep: bool,
    events: Vec<ContactEvent>,
}

impl World {
    pub fn new(gravity: Vec2) -> Self {
        World {
            bodies: Vec::new(),
            free_bodies: Vec::new(),
            fixtures: Vec::new(),
            free_fixtures: Vec::new(),
            contacts: Vec::new(),
            free_contacts: Vec::new(),
            body_list: None,
            body_count: 0,
            contact_list: None,
            contact_count: 0,
            broad_phase: BroadPhase::new(),
            gravity,
            flags: WORLD_CLEAR_FORCES,
            inv_dt0: 0.0,
            warm_starting: true,
            continuous_physics: true,
            sub_stepping: false,
            step_complete: true,
            allow_sleep: true,
            events: Vec::new(),
        }
    }

    pub fn gravity(&self) -> Vec2 {
        self.gravity
    }

    pub fn set_gravity(&mut self, gravity: Vec2) {
        self.gravity = gravity;
    }

    pub fn body_count(&self) -> usize {
        self.body_count
    }

    pub fn contact_count(&self) -> usize {
        self.contact_count
    }

    pub fn body(&self, id: BodyId) -> &Body {
        self.bodies[id].as_ref().expect("live body")
    }

    fn body_mut(&mut self, id: BodyId) -> &mut Body {
        self.bodies[id].as_mut().expect("live body")
    }

    pub fn fixture(&self, id: FixtureId) -> &Fixture {
        self.fixtures[id].as_ref().expect("live fixture")
    }

    fn fixture_mut(&mut self, id: FixtureId) -> &mut Fixture {
        self.fixtures[id].as_mut().expect("live fixture")
    }

    pub fn contact(&self, id: ContactId) -> &Contact {
        self.contacts[id].as_ref().expect("live contact")
    }

    fn contact_mut(&mut self, id: ContactId) -> &mut Contact {
        self.contacts[id].as_mut().expect("live contact")
    }

    /// Bodies from the most recently created (the original's list order).
    pub fn bodies(&self) -> Vec<BodyId> {
        let mut out = Vec::with_capacity(self.body_count);
        let mut b = self.body_list;
        while let Some(id) = b {
            out.push(id);
            b = self.body(id).next;
        }
        out
    }

    /// The fixtures of a body, most recently created first.
    pub fn fixtures_of(&self, body: BodyId) -> Vec<FixtureId> {
        let mut out = Vec::new();
        let mut f = self.body(body).fixture_list;
        while let Some(id) = f {
            out.push(id);
            f = self.fixture(id).next;
        }
        out
    }

    pub fn take_events(&mut self) -> Vec<ContactEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn is_locked(&self) -> bool {
        self.flags & WORLD_LOCKED != 0
    }

    pub fn create_body(&mut self, def: &BodyDef) -> BodyId {
        assert!(!self.is_locked(), "world is locked");
        let mut flags = 0;
        if def.bullet {
            flags |= BODY_BULLET;
        }
        if def.fixed_rotation {
            flags |= BODY_FIXED_ROTATION;
        }
        if def.allow_sleep {
            flags |= BODY_AUTO_SLEEP;
        }
        if def.awake {
            flags |= BODY_AWAKE;
        }
        if def.active {
            flags |= BODY_ACTIVE;
        }
        let xf = Transform::new(def.position, def.angle);
        let (mass, inv_mass) = if def.kind == BodyType::Dynamic {
            (1.0, 1.0)
        } else {
            (0.0, 0.0)
        };
        let body = Body {
            kind: def.kind,
            flags,
            island_index: 0,
            xf,
            sweep: Sweep {
                local_center: Vec2::ZERO,
                c0: xf.p,
                c: xf.p,
                a0: def.angle,
                a: def.angle,
                alpha0: 0.0,
            },
            linear_velocity: def.linear_velocity,
            angular_velocity: def.angular_velocity,
            force: Vec2::ZERO,
            torque: 0.0,
            prev: None,
            next: self.body_list,
            fixture_list: None,
            fixture_count: 0,
            contact_list: None,
            mass,
            inv_mass,
            i: 0.0,
            inv_i: 0.0,
            linear_damping: def.linear_damping,
            angular_damping: def.angular_damping,
            gravity_scale: def.gravity_scale,
            sleep_time: 0.0,
            user_data: def.user_data,
        };
        let id = match self.free_bodies.pop() {
            Some(id) => {
                self.bodies[id] = Some(body);
                id
            }
            None => {
                self.bodies.push(Some(body));
                self.bodies.len() - 1
            }
        };
        if let Some(head) = self.body_list {
            self.body_mut(head).prev = Some(id);
        }
        self.body_list = Some(id);
        self.body_count += 1;
        id
    }

    pub fn destroy_body(&mut self, id: BodyId) {
        assert!(!self.is_locked(), "world is locked");
        // Attached contacts.
        let mut ce = self.body(id).contact_list;
        while let Some(edge) = ce {
            ce = self.edge(edge).next;
            self.destroy_contact(edge.contact);
        }
        self.body_mut(id).contact_list = None;

        // Fixtures, destroying their broad-phase proxies.
        let mut f = self.body(id).fixture_list;
        while let Some(fid) = f {
            f = self.fixture(fid).next;
            self.destroy_proxies(fid);
            self.fixtures[fid] = None;
            self.free_fixtures.push(fid);
            let b = self.body_mut(id);
            b.fixture_list = f;
            b.fixture_count -= 1;
        }
        let b = self.body_mut(id);
        b.fixture_list = None;
        b.fixture_count = 0;

        let (prev, next) = (self.body(id).prev, self.body(id).next);
        if let Some(p) = prev {
            self.body_mut(p).next = next;
        }
        if let Some(n) = next {
            self.body_mut(n).prev = prev;
        }
        if self.body_list == Some(id) {
            self.body_list = next;
        }
        self.body_count -= 1;
        self.bodies[id] = None;
        self.free_bodies.push(id);
    }

    fn edge(&self, e: EdgeRef) -> &ContactEdge {
        let c = self.contact(e.contact);
        if e.side_b { &c.node_b } else { &c.node_a }
    }

    fn edge_mut(&mut self, e: EdgeRef) -> &mut ContactEdge {
        let c = self.contact_mut(e.contact);
        if e.side_b {
            &mut c.node_b
        } else {
            &mut c.node_a
        }
    }

    /// Bodies touching `body` through contacts (the original's contact list order).
    pub fn contacts_of(&self, body: BodyId) -> Vec<ContactId> {
        let mut out = Vec::new();
        let mut e = self.body(body).contact_list;
        while let Some(edge) = e {
            out.push(edge.contact);
            e = self.edge(edge).next;
        }
        out
    }

    pub fn create_fixture(&mut self, body: BodyId, def: &FixtureDef) -> FixtureId {
        assert!(!self.is_locked(), "world is locked");
        let child_count = def.shape.child_count();
        let fixture = Fixture {
            shape: def.shape.clone(),
            density: def.density,
            friction: def.friction,
            restitution: def.restitution,
            is_sensor: def.is_sensor,
            filter: def.filter,
            user_data: def.user_data,
            body,
            next: self.body(body).fixture_list,
            proxies: Vec::with_capacity(child_count),
        };
        let id = match self.free_fixtures.pop() {
            Some(id) => {
                self.fixtures[id] = Some(fixture);
                id
            }
            None => {
                self.fixtures.push(Some(fixture));
                self.fixtures.len() - 1
            }
        };

        if self.body(body).flags & BODY_ACTIVE != 0 {
            let xf = self.body(body).xf;
            self.create_proxies(id, &xf);
        }

        let b = self.body_mut(body);
        b.fixture_list = Some(id);
        b.fixture_count += 1;

        if def.density > 0.0 {
            self.reset_mass_data(body);
        }
        self.flags |= WORLD_NEW_FIXTURE;
        id
    }

    pub fn create_fixture_from_shape(
        &mut self,
        body: BodyId,
        shape: Shape,
        density: Real,
    ) -> FixtureId {
        self.create_fixture(body, &FixtureDef::new(shape, density))
    }

    pub fn destroy_fixture(&mut self, fixture: FixtureId) {
        assert!(!self.is_locked(), "world is locked");
        let body = self.fixture(fixture).body;

        // Unlink from the body's singly linked list.
        let next = self.fixture(fixture).next;
        if self.body(body).fixture_list == Some(fixture) {
            self.body_mut(body).fixture_list = next;
        } else {
            let mut f = self.body(body).fixture_list;
            while let Some(fid) = f {
                if self.fixture(fid).next == Some(fixture) {
                    self.fixture_mut(fid).next = next;
                    break;
                }
                f = self.fixture(fid).next;
            }
        }

        // Destroy contacts associated with the fixture.
        let mut edge = self.body(body).contact_list;
        while let Some(e) = edge {
            let c = e.contact;
            edge = self.edge(e).next;
            let (fa, fb) = (self.contact(c).fixture_a, self.contact(c).fixture_b);
            if fixture == fa || fixture == fb {
                self.destroy_contact(c);
            }
        }

        if self.body(body).flags & BODY_ACTIVE != 0 {
            self.destroy_proxies(fixture);
        }
        self.fixtures[fixture] = None;
        self.free_fixtures.push(fixture);
        self.body_mut(body).fixture_count -= 1;
        self.reset_mass_data(body);
    }

    fn create_proxies(&mut self, fid: FixtureId, xf: &Transform) {
        let shape = self.fixture(fid).shape.clone();
        let mut proxies = Vec::new();
        for i in 0..shape.child_count() {
            let aabb = shape.compute_aabb(xf, i);
            let proxy_id = self.broad_phase.create_proxy(&aabb, pack_proxy(fid, i));
            proxies.push(FixtureProxy {
                aabb,
                proxy_id,
                child_index: i,
            });
        }
        self.fixture_mut(fid).proxies = proxies;
    }

    fn destroy_proxies(&mut self, fid: FixtureId) {
        let proxies = std::mem::take(&mut self.fixture_mut(fid).proxies);
        for p in proxies {
            self.broad_phase.destroy_proxy(p.proxy_id);
        }
    }

    fn synchronize_fixture(&mut self, fid: FixtureId, xf1: &Transform, xf2: &Transform) {
        let shape = self.fixture(fid).shape.clone();
        let count = self.fixture(fid).proxies.len();
        for i in 0..count {
            let child = self.fixture(fid).proxies[i].child_index;
            let aabb1 = shape.compute_aabb(xf1, child);
            let aabb2 = shape.compute_aabb(xf2, child);
            let combined = Aabb::combined(&aabb1, &aabb2);
            let displacement = xf2.p - xf1.p;
            let proxy_id = {
                let p = &mut self.fixture_mut(fid).proxies[i];
                p.aabb = combined;
                p.proxy_id
            };
            self.broad_phase
                .move_proxy(proxy_id, &combined, displacement);
        }
    }

    fn synchronize_fixtures(&mut self, body: BodyId) {
        let b = self.body(body);
        let mut xf1 = Transform {
            q: Rot::new(b.sweep.a0),
            ..Default::default()
        };
        xf1.p = b.sweep.c0 - mul_rot(xf1.q, b.sweep.local_center);
        let xf2 = b.xf;
        for fid in self.fixtures_of(body) {
            self.synchronize_fixture(fid, &xf1, &xf2);
        }
    }

    pub fn reset_mass_data(&mut self, body: BodyId) {
        let b = self.body_mut(body);
        b.mass = 0.0;
        b.inv_mass = 0.0;
        b.i = 0.0;
        b.inv_i = 0.0;
        b.sweep.local_center = Vec2::ZERO;

        if b.kind == BodyType::Static || b.kind == BodyType::Kinematic {
            b.sweep.c0 = b.xf.p;
            b.sweep.c = b.xf.p;
            b.sweep.a0 = b.sweep.a;
            return;
        }

        let mut local_center = Vec2::ZERO;
        let (mut mass, mut inertia) = (0.0, 0.0);
        let mut f = self.body(body).fixture_list;
        while let Some(fid) = f {
            let fixture = self.fixture(fid);
            f = fixture.next;
            if fixture.density == 0.0 {
                continue;
            }
            let md: MassData = fixture.shape.compute_mass(fixture.density);
            mass += md.mass;
            local_center += md.mass * md.center;
            inertia += md.i;
        }

        let b = self.body_mut(body);
        b.mass = mass;
        b.i = inertia;
        if b.mass > 0.0 {
            b.inv_mass = 1.0 / b.mass;
            local_center = b.inv_mass * local_center;
        } else {
            b.mass = 1.0;
            b.inv_mass = 1.0;
        }

        if b.i > 0.0 && b.flags & BODY_FIXED_ROTATION == 0 {
            b.i -= b.mass * dot(local_center, local_center);
            b.inv_i = 1.0 / b.i;
        } else {
            b.i = 0.0;
            b.inv_i = 0.0;
        }

        let old_center = b.sweep.c;
        b.sweep.local_center = local_center;
        b.sweep.c = mul_xf(&b.xf, b.sweep.local_center);
        b.sweep.c0 = b.sweep.c;
        b.linear_velocity += cross_sv(b.angular_velocity, b.sweep.c - old_center);
    }

    pub fn set_mass_data(&mut self, body: BodyId, data: &MassData) {
        let b = self.body_mut(body);
        if b.kind != BodyType::Dynamic {
            return;
        }
        b.inv_mass = 0.0;
        b.i = 0.0;
        b.inv_i = 0.0;
        b.mass = data.mass;
        if b.mass <= 0.0 {
            b.mass = 1.0;
        }
        b.inv_mass = 1.0 / b.mass;
        if data.i > 0.0 && b.flags & BODY_FIXED_ROTATION == 0 {
            b.i = data.i - b.mass * dot(data.center, data.center);
            b.inv_i = 1.0 / b.i;
        }
        let old_center = b.sweep.c;
        b.sweep.local_center = data.center;
        b.sweep.c = mul_xf(&b.xf, b.sweep.local_center);
        b.sweep.c0 = b.sweep.c;
        b.linear_velocity += cross_sv(b.angular_velocity, b.sweep.c - old_center);
    }

    pub fn set_transform(&mut self, body: BodyId, position: Vec2, angle: Real) {
        assert!(!self.is_locked(), "world is locked");
        {
            let b = self.body_mut(body);
            b.xf.q = Rot::new(angle);
            b.xf.p = position;
            b.sweep.c = mul_xf(&b.xf, b.sweep.local_center);
            b.sweep.a = angle;
            b.sweep.c0 = b.sweep.c;
            b.sweep.a0 = angle;
        }
        let xf = self.body(body).xf;
        for fid in self.fixtures_of(body) {
            self.synchronize_fixture(fid, &xf, &xf);
        }
    }

    pub fn set_type(&mut self, body: BodyId, kind: BodyType) {
        assert!(!self.is_locked(), "world is locked");
        if self.body(body).kind == kind {
            return;
        }
        self.body_mut(body).kind = kind;
        self.reset_mass_data(body);

        if kind == BodyType::Static {
            let b = self.body_mut(body);
            b.linear_velocity = Vec2::ZERO;
            b.angular_velocity = 0.0;
            b.sweep.a0 = b.sweep.a;
            b.sweep.c0 = b.sweep.c;
            self.synchronize_fixtures(body);
        }

        let b = self.body_mut(body);
        b.set_awake(true);
        b.force = Vec2::ZERO;
        b.torque = 0.0;

        let mut ce = self.body(body).contact_list;
        while let Some(edge) = ce {
            ce = self.edge(edge).next;
            self.destroy_contact(edge.contact);
        }
        self.body_mut(body).contact_list = None;

        for fid in self.fixtures_of(body) {
            let ids: Vec<i32> = self
                .fixture(fid)
                .proxies
                .iter()
                .map(|p| p.proxy_id)
                .collect();
            for id in ids {
                self.broad_phase.touch_proxy(id);
            }
        }
    }

    pub fn set_active(&mut self, body: BodyId, flag: bool) {
        assert!(!self.is_locked(), "world is locked");
        if flag == self.body(body).is_active() {
            return;
        }
        if flag {
            self.body_mut(body).flags |= BODY_ACTIVE;
            let xf = self.body(body).xf;
            for fid in self.fixtures_of(body) {
                self.create_proxies(fid, &xf);
            }
        } else {
            self.body_mut(body).flags &= !BODY_ACTIVE;
            for fid in self.fixtures_of(body) {
                self.destroy_proxies(fid);
            }
            let mut ce = self.body(body).contact_list;
            while let Some(edge) = ce {
                ce = self.edge(edge).next;
                self.destroy_contact(edge.contact);
            }
            self.body_mut(body).contact_list = None;
        }
    }

    pub fn set_awake(&mut self, body: BodyId, flag: bool) {
        self.body_mut(body).set_awake(flag);
    }

    pub fn set_linear_velocity(&mut self, body: BodyId, v: Vec2) {
        let b = self.body_mut(body);
        if b.kind == BodyType::Static {
            return;
        }
        if dot(v, v) > 0.0 {
            b.set_awake(true);
        }
        b.linear_velocity = v;
    }

    pub fn set_angular_velocity(&mut self, body: BodyId, w: Real) {
        let b = self.body_mut(body);
        if b.kind == BodyType::Static {
            return;
        }
        if w * w > 0.0 {
            b.set_awake(true);
        }
        b.angular_velocity = w;
    }

    pub fn apply_force(&mut self, body: BodyId, force: Vec2, point: Vec2, wake: bool) {
        let b = self.body_mut(body);
        if b.kind != BodyType::Dynamic {
            return;
        }
        if wake && b.flags & BODY_AWAKE == 0 {
            b.set_awake(true);
        }
        if b.flags & BODY_AWAKE != 0 {
            b.force += force;
            b.torque += cross(point - b.sweep.c, force);
        }
    }

    pub fn apply_force_to_center(&mut self, body: BodyId, force: Vec2, wake: bool) {
        let b = self.body_mut(body);
        if b.kind != BodyType::Dynamic {
            return;
        }
        if wake && b.flags & BODY_AWAKE == 0 {
            b.set_awake(true);
        }
        if b.flags & BODY_AWAKE != 0 {
            b.force += force;
        }
    }

    pub fn apply_torque(&mut self, body: BodyId, torque: Real, wake: bool) {
        let b = self.body_mut(body);
        if b.kind != BodyType::Dynamic {
            return;
        }
        if wake && b.flags & BODY_AWAKE == 0 {
            b.set_awake(true);
        }
        if b.flags & BODY_AWAKE != 0 {
            b.torque += torque;
        }
    }

    pub fn apply_linear_impulse(&mut self, body: BodyId, impulse: Vec2, point: Vec2, wake: bool) {
        let b = self.body_mut(body);
        if b.kind != BodyType::Dynamic {
            return;
        }
        if wake && b.flags & BODY_AWAKE == 0 {
            b.set_awake(true);
        }
        if b.flags & BODY_AWAKE != 0 {
            b.linear_velocity += b.inv_mass * impulse;
            b.angular_velocity += b.inv_i * cross(point - b.sweep.c, impulse);
        }
    }

    pub fn apply_angular_impulse(&mut self, body: BodyId, impulse: Real, wake: bool) {
        let b = self.body_mut(body);
        if b.kind != BodyType::Dynamic {
            return;
        }
        if wake && b.flags & BODY_AWAKE == 0 {
            b.set_awake(true);
        }
        if b.flags & BODY_AWAKE != 0 {
            b.angular_velocity += b.inv_i * impulse;
        }
    }

    pub fn set_allow_sleeping(&mut self, flag: bool) {
        if flag == self.allow_sleep {
            return;
        }
        self.allow_sleep = flag;
        if !flag {
            for id in self.bodies() {
                self.body_mut(id).set_awake(true);
            }
        }
    }

    // ---- contacts -------------------------------------------------------------------------

    /// `b2Body::ShouldCollide` without joints: at least one body must be dynamic.
    fn should_collide(&self, a: BodyId, b: BodyId) -> bool {
        self.body(a).kind == BodyType::Dynamic || self.body(b).kind == BodyType::Dynamic
    }

    fn contact_registered(a: &Shape, b: &Shape) -> Option<bool> {
        // Some(swap): the contact exists; swap when the second fixture is the primary one.
        let is_curve = |s: &Shape| matches!(s, Shape::Edge(_) | Shape::Chain(_));
        match (a, b) {
            (Shape::Circle(_), Shape::Circle(_)) => Some(false),
            (Shape::Polygon(_), Shape::Circle(_)) => Some(false),
            (Shape::Circle(_), Shape::Polygon(_)) => Some(true),
            (Shape::Polygon(_), Shape::Polygon(_)) => Some(false),
            (e, Shape::Circle(_) | Shape::Polygon(_)) if is_curve(e) => Some(false),
            (Shape::Circle(_) | Shape::Polygon(_), e) if is_curve(e) => Some(true),
            // Edges and chains never collide with each other.
            _ => None,
        }
    }

    fn add_pair(&mut self, data_a: usize, data_b: usize) {
        let (fixture_a, index_a) = unpack_proxy(data_a);
        let (fixture_b, index_b) = unpack_proxy(data_b);
        let body_a = self.fixture(fixture_a).body;
        let body_b = self.fixture(fixture_b).body;
        if body_a == body_b {
            return;
        }

        let mut edge = self.body(body_b).contact_list;
        while let Some(e) = edge {
            if self.edge(e).other == body_a {
                let c = self.contact(e.contact);
                let (fa, fb, ia, ib) = (c.fixture_a, c.fixture_b, c.index_a, c.index_b);
                if fa == fixture_a && fb == fixture_b && ia == index_a && ib == index_b {
                    return;
                }
                if fa == fixture_b && fb == fixture_a && ia == index_b && ib == index_a {
                    return;
                }
            }
            edge = self.edge(e).next;
        }

        if !self.should_collide(body_b, body_a) {
            return;
        }
        if !default_should_collide(
            &self.fixture(fixture_a).filter,
            &self.fixture(fixture_b).filter,
        ) {
            return;
        }

        let Some(swap) = Self::contact_registered(
            &self.fixture(fixture_a).shape,
            &self.fixture(fixture_b).shape,
        ) else {
            return;
        };
        let (fa, ia, fb, ib) = if swap {
            (fixture_b, index_b, fixture_a, index_a)
        } else {
            (fixture_a, index_a, fixture_b, index_b)
        };
        let body_a = self.fixture(fa).body;
        let body_b = self.fixture(fb).body;

        let contact = Contact {
            flags: CONTACT_ENABLED,
            prev: None,
            next: self.contact_list,
            node_a: ContactEdge {
                other: body_b,
                prev: None,
                next: self.body(body_a).contact_list,
            },
            node_b: ContactEdge {
                other: body_a,
                prev: None,
                next: self.body(body_b).contact_list,
            },
            fixture_a: fa,
            fixture_b: fb,
            index_a: ia,
            index_b: ib,
            manifold: Manifold::default(),
            toi_count: 0,
            toi: 0.0,
            friction: mix_friction(self.fixture(fa).friction, self.fixture(fb).friction),
            restitution: mix_restitution(
                self.fixture(fa).restitution,
                self.fixture(fb).restitution,
            ),
            tangent_speed: 0.0,
        };
        let c = match self.free_contacts.pop() {
            Some(id) => {
                self.contacts[id] = Some(contact);
                id
            }
            None => {
                self.contacts.push(Some(contact));
                self.contacts.len() - 1
            }
        };

        if let Some(head) = self.contact_list {
            self.contact_mut(head).prev = Some(c);
        }
        self.contact_list = Some(c);

        let edge_a = EdgeRef {
            contact: c,
            side_b: false,
        };
        let edge_b = EdgeRef {
            contact: c,
            side_b: true,
        };
        if let Some(head) = self.body(body_a).contact_list {
            self.edge_mut(head).prev = Some(edge_a);
        }
        self.body_mut(body_a).contact_list = Some(edge_a);
        if let Some(head) = self.body(body_b).contact_list {
            self.edge_mut(head).prev = Some(edge_b);
        }
        self.body_mut(body_b).contact_list = Some(edge_b);

        if !self.fixture(fa).is_sensor && !self.fixture(fb).is_sensor {
            self.body_mut(body_a).set_awake(true);
            self.body_mut(body_b).set_awake(true);
        }
        self.contact_count += 1;
    }

    fn destroy_contact(&mut self, c: ContactId) {
        let (fa, fb) = (self.contact(c).fixture_a, self.contact(c).fixture_b);
        let body_a = self.fixture(fa).body;
        let body_b = self.fixture(fb).body;

        if self.contact(c).is_touching() {
            self.events.push(ContactEvent::End(fa, fb));
        }

        let (prev, next) = (self.contact(c).prev, self.contact(c).next);
        if let Some(p) = prev {
            self.contact_mut(p).next = next;
        }
        if let Some(n) = next {
            self.contact_mut(n).prev = prev;
        }
        if self.contact_list == Some(c) {
            self.contact_list = next;
        }

        for (side_b, body) in [(false, body_a), (true, body_b)] {
            let me = EdgeRef { contact: c, side_b };
            let (ep, en) = (self.edge(me).prev, self.edge(me).next);
            if let Some(p) = ep {
                self.edge_mut(p).next = en;
            }
            if let Some(n) = en {
                self.edge_mut(n).prev = ep;
            }
            if self.body(body).contact_list == Some(me) {
                self.body_mut(body).contact_list = en;
            }
        }

        if self.contact(c).manifold.point_count > 0
            && !self.fixture(fa).is_sensor
            && !self.fixture(fb).is_sensor
        {
            self.body_mut(body_a).set_awake(true);
            self.body_mut(body_b).set_awake(true);
        }

        self.contacts[c] = None;
        self.free_contacts.push(c);
        self.contact_count -= 1;
    }

    fn evaluate(&self, c: ContactId, xf_a: &Transform, xf_b: &Transform) -> Manifold {
        let ct = self.contact(c);
        match (
            &self.fixture(ct.fixture_a).shape,
            &self.fixture(ct.fixture_b).shape,
        ) {
            (Shape::Circle(a), Shape::Circle(b)) => collide_circles(a, xf_a, b, xf_b),
            (Shape::Polygon(a), Shape::Circle(b)) => collide_polygon_and_circle(a, xf_a, b, xf_b),
            (Shape::Polygon(a), Shape::Polygon(b)) => collide_polygons(a, xf_a, b, xf_b),
            (Shape::Edge(a), Shape::Circle(b)) => collide_edge_and_circle(a, xf_a, b, xf_b),
            (Shape::Edge(a), Shape::Polygon(b)) => collide_edge_and_polygon(a, xf_a, b, xf_b),
            (Shape::Chain(a), Shape::Circle(b)) => {
                collide_edge_and_circle(&a.child_edge(ct.index_a), xf_a, b, xf_b)
            }
            (Shape::Chain(a), Shape::Polygon(b)) => {
                collide_edge_and_polygon(&a.child_edge(ct.index_a), xf_a, b, xf_b)
            }
            _ => unreachable!("contacts put the curve or polygon first"),
        }
    }

    /// `b2Contact::Update`: new manifold, warm-start impulse matching, touching state.
    fn update_contact(&mut self, c: ContactId) {
        let old_manifold = self.contact(c).manifold;
        self.contact_mut(c).flags |= CONTACT_ENABLED;

        let was_touching = self.contact(c).is_touching();
        let (fa, fb) = (self.contact(c).fixture_a, self.contact(c).fixture_b);
        let sensor = self.fixture(fa).is_sensor || self.fixture(fb).is_sensor;
        let (body_a, body_b) = (self.fixture(fa).body, self.fixture(fb).body);
        let xf_a = self.body(body_a).xf;
        let xf_b = self.body(body_b).xf;

        let touching;
        if sensor {
            let (ia, ib) = (self.contact(c).index_a, self.contact(c).index_b);
            touching = test_overlap_shapes(
                &self.fixture(fa).shape,
                ia,
                &self.fixture(fb).shape,
                ib,
                &xf_a,
                &xf_b,
            );
            self.contact_mut(c).manifold.point_count = 0;
        } else {
            let mut manifold = self.evaluate(c, &xf_a, &xf_b);
            touching = manifold.point_count > 0;
            for i in 0..manifold.point_count {
                let mp2 = &mut manifold.points[i];
                mp2.normal_impulse = 0.0;
                mp2.tangent_impulse = 0.0;
                let id2 = mp2.id.key();
                for j in 0..old_manifold.point_count {
                    let mp1 = &old_manifold.points[j];
                    if mp1.id.key() == id2 {
                        mp2.normal_impulse = mp1.normal_impulse;
                        mp2.tangent_impulse = mp1.tangent_impulse;
                        break;
                    }
                }
            }
            self.contact_mut(c).manifold = manifold;
            if touching != was_touching {
                self.body_mut(body_a).set_awake(true);
                self.body_mut(body_b).set_awake(true);
            }
        }

        if touching {
            self.contact_mut(c).flags |= CONTACT_TOUCHING;
        } else {
            self.contact_mut(c).flags &= !CONTACT_TOUCHING;
        }
        if !was_touching && touching {
            self.events.push(ContactEvent::Begin(fa, fb));
        }
        if was_touching && !touching {
            self.events.push(ContactEvent::End(fa, fb));
        }
    }

    /// `b2ContactManager::Collide`: update awake contacts, destroying those that stopped overlapping.
    fn collide(&mut self) {
        let mut cur = self.contact_list;
        while let Some(c) = cur {
            let (fa, fb) = (self.contact(c).fixture_a, self.contact(c).fixture_b);
            let (ia, ib) = (self.contact(c).index_a, self.contact(c).index_b);
            let (body_a, body_b) = (self.fixture(fa).body, self.fixture(fb).body);

            if self.contact(c).flags & CONTACT_FILTER != 0 {
                let next = self.contact(c).next;
                if !self.should_collide(body_b, body_a)
                    || !default_should_collide(&self.fixture(fa).filter, &self.fixture(fb).filter)
                {
                    self.destroy_contact(c);
                    cur = next;
                    continue;
                }
                self.contact_mut(c).flags &= !CONTACT_FILTER;
            }

            let active_a =
                self.body(body_a).is_awake() && self.body(body_a).kind != BodyType::Static;
            let active_b =
                self.body(body_b).is_awake() && self.body(body_b).kind != BodyType::Static;
            if !active_a && !active_b {
                cur = self.contact(c).next;
                continue;
            }

            let proxy_a = self.fixture(fa).proxies[ia].proxy_id;
            let proxy_b = self.fixture(fb).proxies[ib].proxy_id;
            if !self.broad_phase.test_overlap(proxy_a, proxy_b) {
                let next = self.contact(c).next;
                self.destroy_contact(c);
                cur = next;
                continue;
            }

            self.update_contact(c);
            cur = self.contact(c).next;
        }
    }

    fn find_new_contacts(&mut self) {
        let mut pairs = Vec::new();
        self.broad_phase.update_pairs(|a, b| pairs.push((a, b)));
        for (a, b) in pairs {
            self.add_pair(a, b);
        }
    }

    // ---- stepping -------------------------------------------------------------------------

    pub fn step(&mut self, dt: Real, velocity_iterations: i32, position_iterations: i32) {
        if self.flags & WORLD_NEW_FIXTURE != 0 {
            self.find_new_contacts();
            self.flags &= !WORLD_NEW_FIXTURE;
        }
        self.flags |= WORLD_LOCKED;

        let step = TimeStep {
            dt,
            inv_dt: if dt > 0.0 { 1.0 / dt } else { 0.0 },
            dt_ratio: self.inv_dt0 * dt,
            velocity_iterations,
            position_iterations,
            warm_starting: self.warm_starting,
        };

        self.collide();

        if self.step_complete && step.dt > 0.0 {
            self.solve(&step);
        }
        if self.continuous_physics && step.dt > 0.0 {
            self.solve_toi(&step);
        }
        if step.dt > 0.0 {
            self.inv_dt0 = step.inv_dt;
        }
        if self.flags & WORLD_CLEAR_FORCES != 0 {
            self.clear_forces();
        }
        self.flags &= !WORLD_LOCKED;
    }

    /// Noita steps with 6 velocity and 2 position iterations (hard-coded in 0x9c0d00).
    pub fn step_noita(&mut self, dt: Real) {
        self.step(dt, 6, 2);
    }

    pub fn clear_forces(&mut self) {
        for id in self.bodies() {
            let b = self.body_mut(id);
            b.force = Vec2::ZERO;
            b.torque = 0.0;
        }
    }

    /// Find islands, integrate and solve constraints, solve position constraints.
    fn solve(&mut self, step: &TimeStep) {
        let all_bodies = self.bodies();
        for &b in &all_bodies {
            self.body_mut(b).flags &= !BODY_ISLAND;
        }
        let mut c = self.contact_list;
        while let Some(id) = c {
            self.contact_mut(id).flags &= !CONTACT_ISLAND;
            c = self.contact(id).next;
        }

        for &seed in &all_bodies {
            if self.body(seed).flags & BODY_ISLAND != 0 {
                continue;
            }
            if !self.body(seed).is_awake() || !self.body(seed).is_active() {
                continue;
            }
            if self.body(seed).kind == BodyType::Static {
                continue;
            }

            let mut island_bodies: Vec<BodyId> = Vec::new();
            let mut island_contacts: Vec<ContactId> = Vec::new();
            let mut stack = vec![seed];
            self.body_mut(seed).flags |= BODY_ISLAND;

            while let Some(b) = stack.pop() {
                self.body_mut(b).island_index = island_bodies.len();
                island_bodies.push(b);
                self.body_mut(b).set_awake(true);

                if self.body(b).kind == BodyType::Static {
                    continue;
                }

                let mut ce = self.body(b).contact_list;
                while let Some(edge) = ce {
                    ce = self.edge(edge).next;
                    let contact_id = edge.contact;
                    let contact = self.contact(contact_id);
                    if contact.flags & CONTACT_ISLAND != 0 {
                        continue;
                    }
                    if !contact.is_enabled() || !contact.is_touching() {
                        continue;
                    }
                    if self.fixture(contact.fixture_a).is_sensor
                        || self.fixture(contact.fixture_b).is_sensor
                    {
                        continue;
                    }
                    // Noita logs and skips a touching contact without manifold points.
                    if contact.manifold.point_count < 1 {
                        continue;
                    }
                    island_contacts.push(contact_id);
                    self.contact_mut(contact_id).flags |= CONTACT_ISLAND;

                    let other = self.edge(edge).other;
                    if self.body(other).flags & BODY_ISLAND != 0 {
                        continue;
                    }
                    stack.push(other);
                    self.body_mut(other).flags |= BODY_ISLAND;
                }
            }

            self.solve_island(&island_bodies, &island_contacts, step);

            for &b in &island_bodies {
                if self.body(b).kind == BodyType::Static {
                    self.body_mut(b).flags &= !BODY_ISLAND;
                }
            }
        }

        for &b in &all_bodies {
            if self.body(b).flags & BODY_ISLAND == 0 {
                continue;
            }
            if self.body(b).kind == BodyType::Static {
                continue;
            }
            self.synchronize_fixtures(b);
        }
        self.find_new_contacts();
    }

    fn solver_contact(&self, c: ContactId) -> SolverContact {
        let ct = self.contact(c);
        let (fa, fb) = (self.fixture(ct.fixture_a), self.fixture(ct.fixture_b));
        let (ba, bb) = (self.body(fa.body), self.body(fb.body));
        SolverContact {
            manifold: ct.manifold,
            friction: ct.friction,
            restitution: ct.restitution,
            tangent_speed: ct.tangent_speed,
            index_a: ba.island_index,
            index_b: bb.island_index,
            inv_mass_a: ba.inv_mass,
            inv_mass_b: bb.inv_mass,
            inv_i_a: ba.inv_i,
            inv_i_b: bb.inv_i,
            local_center_a: ba.sweep.local_center,
            local_center_b: bb.sweep.local_center,
            radius_a: fa.shape.radius(),
            radius_b: fb.shape.radius(),
        }
    }

    fn store_manifolds(&mut self, contacts: &[ContactId], solver: &ContactSolver) {
        let mut manifolds: Vec<Manifold> =
            contacts.iter().map(|&c| self.contact(c).manifold).collect();
        solver.store_impulses(&mut manifolds);
        for (&c, m) in contacts.iter().zip(manifolds) {
            self.contact_mut(c).manifold = m;
        }
    }

    /// `b2Island::Solve`.
    fn solve_island(&mut self, bodies: &[BodyId], contacts: &[ContactId], step: &TimeStep) {
        let h = step.dt;
        let gravity = self.gravity;
        let mut positions = Vec::with_capacity(bodies.len());
        let mut velocities = Vec::with_capacity(bodies.len());

        for &id in bodies {
            let b = self.body_mut(id);
            let c = b.sweep.c;
            let a = b.sweep.a;
            let mut v = b.linear_velocity;
            let mut w = b.angular_velocity;

            b.sweep.c0 = b.sweep.c;
            b.sweep.a0 = b.sweep.a;

            if b.kind == BodyType::Dynamic {
                if !b.force.is_valid() {
                    b.force = Vec2::ZERO;
                }
                if !b.torque.is_finite() {
                    b.torque = 0.0;
                }
                let lin = 1.0 / (b.linear_damping * h + 1.0);
                v = vec2(
                    lin * ((gravity.x * b.gravity_scale + b.inv_mass * b.force.x) * h + v.x),
                    lin * ((gravity.y * b.gravity_scale + b.force.y * b.inv_mass) * h + v.y),
                );
                w = (1.0 / (b.angular_damping * h + 1.0)) * (b.inv_i * h * b.torque + w);
                // Noita caps the linear speed at 250.
                let speed_sqr = v.y * v.y + v.x * v.x;
                if MAX_LINEAR_SPEED * MAX_LINEAR_SPEED < speed_sqr {
                    let len = speed_sqr.sqrt();
                    if EPSILON <= len {
                        v = vec2(v.x * (1.0 / len), (1.0 / len) * v.y);
                    }
                    v = vec2(v.x * MAX_LINEAR_SPEED, v.y * MAX_LINEAR_SPEED);
                }
            }
            positions.push(Position { c, a });
            velocities.push(Velocity { v, w });
        }

        let solver_contacts: Vec<SolverContact> =
            contacts.iter().map(|&c| self.solver_contact(c)).collect();
        let mut solver = ContactSolver::new(*step, solver_contacts);
        solver.initialize_velocity_constraints(&positions, &velocities);
        if step.warm_starting {
            solver.warm_start(&mut velocities);
        }
        for _ in 0..step.velocity_iterations {
            solver.solve_velocity_constraints(&mut velocities);
        }
        self.store_manifolds(contacts, &solver);

        for i in 0..bodies.len() {
            let (c, a, v, w) = integrate_position(
                h,
                positions[i].c,
                positions[i].a,
                velocities[i].v,
                velocities[i].w,
            );
            positions[i] = Position { c, a };
            velocities[i] = Velocity { v, w };
        }

        let mut position_solved = false;
        for _ in 0..step.position_iterations {
            if solver.solve_position_constraints(&mut positions) {
                position_solved = true;
                break;
            }
        }

        for (i, &id) in bodies.iter().enumerate() {
            let b = self.body_mut(id);
            b.sweep.c = positions[i].c;
            b.sweep.a = positions[i].a;
            b.linear_velocity = velocities[i].v;
            b.angular_velocity = velocities[i].w;
            b.synchronize_transform();
        }

        if self.allow_sleep {
            let mut min_sleep_time = FLOAT_MAX;
            let lin_tol_sqr = LINEAR_SLEEP_TOLERANCE * LINEAR_SLEEP_TOLERANCE;
            let ang_tol_sqr = ANGULAR_SLEEP_TOLERANCE * ANGULAR_SLEEP_TOLERANCE;
            for &id in bodies {
                let b = self.body_mut(id);
                if b.kind == BodyType::Static {
                    continue;
                }
                if b.flags & BODY_AUTO_SLEEP == 0
                    || b.angular_velocity * b.angular_velocity > ang_tol_sqr
                    || dot(b.linear_velocity, b.linear_velocity) > lin_tol_sqr
                {
                    b.sleep_time = 0.0;
                    min_sleep_time = 0.0;
                } else {
                    b.sleep_time += h;
                    min_sleep_time = min(min_sleep_time, b.sleep_time);
                }
            }
            if min_sleep_time >= TIME_TO_SLEEP && position_solved {
                for &id in bodies {
                    self.body_mut(id).set_awake(false);
                }
            }
        }
    }

    /// `b2Island::SolveTOI`.
    fn solve_island_toi(
        &mut self,
        bodies: &[BodyId],
        contacts: &[ContactId],
        sub_step: &TimeStep,
        toi_index_a: usize,
        toi_index_b: usize,
    ) {
        let mut positions = Vec::with_capacity(bodies.len());
        let mut velocities = Vec::with_capacity(bodies.len());
        for &id in bodies {
            let b = self.body(id);
            positions.push(Position {
                c: b.sweep.c,
                a: b.sweep.a,
            });
            velocities.push(Velocity {
                v: b.linear_velocity,
                w: b.angular_velocity,
            });
        }

        let solver_contacts: Vec<SolverContact> =
            contacts.iter().map(|&c| self.solver_contact(c)).collect();
        let mut solver = ContactSolver::new(*sub_step, solver_contacts);

        for _ in 0..sub_step.position_iterations {
            if solver.solve_toi_position_constraints(&mut positions, toi_index_a, toi_index_b) {
                break;
            }
        }

        // Leap of faith to the new safe state.
        for idx in [toi_index_a, toi_index_b] {
            let b = self.body_mut(bodies[idx]);
            b.sweep.c0 = positions[idx].c;
            b.sweep.a0 = positions[idx].a;
        }

        solver.initialize_velocity_constraints(&positions, &velocities);
        for _ in 0..sub_step.velocity_iterations {
            solver.solve_velocity_constraints(&mut velocities);
        }

        let h = sub_step.dt;
        for i in 0..bodies.len() {
            let (c, a, v, w) = integrate_position(
                h,
                positions[i].c,
                positions[i].a,
                velocities[i].v,
                velocities[i].w,
            );
            positions[i] = Position { c, a };
            velocities[i] = Velocity { v, w };

            let b = self.body_mut(bodies[i]);
            b.sweep.c = c;
            b.sweep.a = a;
            b.linear_velocity = v;
            b.angular_velocity = w;
            b.synchronize_transform();
        }
    }

    /// Find TOI contacts and solve them (continuous collision).
    fn solve_toi(&mut self, step: &TimeStep) {
        const BODY_CAPACITY: usize = 2 * MAX_TOI_CONTACTS;
        if self.step_complete {
            for b in self.bodies() {
                let body = self.body_mut(b);
                body.flags &= !BODY_ISLAND;
                body.sweep.alpha0 = 0.0;
            }
            let mut c = self.contact_list;
            while let Some(id) = c {
                let ct = self.contact_mut(id);
                ct.flags &= !(CONTACT_TOI | CONTACT_ISLAND);
                ct.toi_count = 0;
                ct.toi = 1.0;
                c = ct.next;
            }
        }

        let mut failed_tois = 0usize;
        // Noita bounds the event loop ("endless loop was broken") and the failed TOI count.
        for _ in 0..MAX_TOI_EVENTS {
            let mut min_contact: Option<ContactId> = None;
            let mut min_alpha = 1.0;

            let mut cur = self.contact_list;
            while let Some(c) = cur {
                cur = self.contact(c).next;
                if !self.contact(c).is_enabled() {
                    continue;
                }
                if self.contact(c).toi_count > MAX_SUB_STEPS {
                    continue;
                }

                let alpha;
                if self.contact(c).flags & CONTACT_TOI != 0 {
                    alpha = self.contact(c).toi;
                } else {
                    let (fa, fb) = (self.contact(c).fixture_a, self.contact(c).fixture_b);
                    if self.fixture(fa).is_sensor || self.fixture(fb).is_sensor {
                        continue;
                    }
                    let (ba, bb) = (self.fixture(fa).body, self.fixture(fb).body);
                    let (type_a, type_b) = (self.body(ba).kind, self.body(bb).kind);

                    let active_a = self.body(ba).is_awake() && type_a != BodyType::Static;
                    let active_b = self.body(bb).is_awake() && type_b != BodyType::Static;
                    if !active_a && !active_b {
                        continue;
                    }
                    let collide_a = self.body(ba).is_bullet() || type_a != BodyType::Dynamic;
                    let collide_b = self.body(bb).is_bullet() || type_b != BodyType::Dynamic;
                    if !collide_a && !collide_b {
                        continue;
                    }

                    let mut alpha0 = self.body(ba).sweep.alpha0;
                    if self.body(ba).sweep.alpha0 < self.body(bb).sweep.alpha0 {
                        alpha0 = self.body(bb).sweep.alpha0;
                        self.body_mut(ba).sweep.advance(alpha0);
                    } else if self.body(bb).sweep.alpha0 < self.body(ba).sweep.alpha0 {
                        alpha0 = self.body(ba).sweep.alpha0;
                        self.body_mut(bb).sweep.advance(alpha0);
                    }

                    let (ia, ib) = (self.contact(c).index_a, self.contact(c).index_b);
                    let input = ToiInput {
                        proxy_a: crate::distance::DistanceProxy::from_shape(
                            &self.fixture(fa).shape,
                            ia,
                        ),
                        proxy_b: crate::distance::DistanceProxy::from_shape(
                            &self.fixture(fb).shape,
                            ib,
                        ),
                        sweep_a: self.body(ba).sweep,
                        sweep_b: self.body(bb).sweep,
                        t_max: 1.0,
                    };
                    let output = time_of_impact(&input);
                    if output.state == ToiState::Failed {
                        failed_tois += 1;
                        if failed_tois > MAX_TOI_EVENTS {
                            return;
                        }
                    }
                    let beta = output.t;
                    alpha = if output.state == ToiState::Touching {
                        min(alpha0 + (1.0 - alpha0) * beta, 1.0)
                    } else {
                        1.0
                    };
                    let ct = self.contact_mut(c);
                    ct.toi = alpha;
                    ct.flags |= CONTACT_TOI;
                }

                if alpha < min_alpha {
                    min_contact = Some(c);
                    min_alpha = alpha;
                }
            }

            let Some(min_contact) = min_contact else {
                self.step_complete = true;
                break;
            };
            if 1.0 - 10.0 * EPSILON < min_alpha {
                self.step_complete = true;
                break;
            }

            let (fa, fb) = (
                self.contact(min_contact).fixture_a,
                self.contact(min_contact).fixture_b,
            );
            let (ba, bb) = (self.fixture(fa).body, self.fixture(fb).body);
            let backup1 = self.body(ba).sweep;
            let backup2 = self.body(bb).sweep;

            self.body_mut(ba).advance(min_alpha);
            self.body_mut(bb).advance(min_alpha);

            self.update_contact(min_contact);
            self.contact_mut(min_contact).flags &= !CONTACT_TOI;
            self.contact_mut(min_contact).toi_count += 1;

            if !self.contact(min_contact).is_enabled() || !self.contact(min_contact).is_touching() {
                self.contact_mut(min_contact).flags &= !CONTACT_ENABLED;
                self.body_mut(ba).sweep = backup1;
                self.body_mut(bb).sweep = backup2;
                self.body_mut(ba).synchronize_transform();
                self.body_mut(bb).synchronize_transform();
                continue;
            }

            self.body_mut(ba).set_awake(true);
            self.body_mut(bb).set_awake(true);

            let mut island_bodies: Vec<BodyId> = Vec::new();
            let mut island_contacts: Vec<ContactId> = Vec::new();
            self.body_mut(ba).island_index = island_bodies.len();
            island_bodies.push(ba);
            self.body_mut(bb).island_index = island_bodies.len();
            island_bodies.push(bb);
            island_contacts.push(min_contact);

            self.body_mut(ba).flags |= BODY_ISLAND;
            self.body_mut(bb).flags |= BODY_ISLAND;
            self.contact_mut(min_contact).flags |= CONTACT_ISLAND;

            for body in [ba, bb] {
                if self.body(body).kind != BodyType::Dynamic {
                    continue;
                }
                let mut ce = self.body(body).contact_list;
                while let Some(edge) = ce {
                    ce = self.edge(edge).next;
                    if island_bodies.len() == BODY_CAPACITY
                        || island_contacts.len() == MAX_TOI_CONTACTS
                    {
                        break;
                    }
                    let contact_id = edge.contact;
                    if self.contact(contact_id).flags & CONTACT_ISLAND != 0 {
                        continue;
                    }
                    let other = self.edge(edge).other;
                    if self.body(other).kind == BodyType::Dynamic
                        && !self.body(body).is_bullet()
                        && !self.body(other).is_bullet()
                    {
                        continue;
                    }
                    let (cfa, cfb) = (
                        self.contact(contact_id).fixture_a,
                        self.contact(contact_id).fixture_b,
                    );
                    if self.fixture(cfa).is_sensor || self.fixture(cfb).is_sensor {
                        continue;
                    }

                    let backup = self.body(other).sweep;
                    if self.body(other).flags & BODY_ISLAND == 0 {
                        self.body_mut(other).advance(min_alpha);
                    }

                    self.update_contact(contact_id);

                    if !self.contact(contact_id).is_enabled()
                        || !self.contact(contact_id).is_touching()
                    {
                        self.body_mut(other).sweep = backup;
                        self.body_mut(other).synchronize_transform();
                        continue;
                    }

                    self.contact_mut(contact_id).flags |= CONTACT_ISLAND;
                    island_contacts.push(contact_id);

                    if self.body(other).flags & BODY_ISLAND != 0 {
                        continue;
                    }
                    self.body_mut(other).flags |= BODY_ISLAND;
                    if self.body(other).kind != BodyType::Static {
                        self.body_mut(other).set_awake(true);
                    }
                    self.body_mut(other).island_index = island_bodies.len();
                    island_bodies.push(other);
                }
            }

            let sub_dt = (1.0 - min_alpha) * step.dt;
            let sub_step = TimeStep {
                dt: sub_dt,
                inv_dt: 1.0 / sub_dt,
                dt_ratio: 1.0,
                position_iterations: 20,
                velocity_iterations: step.velocity_iterations,
                warm_starting: false,
            };
            let (index_a, index_b) = (self.body(ba).island_index, self.body(bb).island_index);
            self.solve_island_toi(
                &island_bodies,
                &island_contacts,
                &sub_step,
                index_a,
                index_b,
            );

            for &body in &island_bodies {
                self.body_mut(body).flags &= !BODY_ISLAND;
                if self.body(body).kind != BodyType::Dynamic {
                    continue;
                }
                self.synchronize_fixtures(body);
                let mut ce = self.body(body).contact_list;
                while let Some(edge) = ce {
                    ce = self.edge(edge).next;
                    self.contact_mut(edge.contact).flags &= !(CONTACT_TOI | CONTACT_ISLAND);
                }
            }

            self.find_new_contacts();

            if self.sub_stepping {
                self.step_complete = false;
                break;
            }
        }
    }

    // ---- queries --------------------------------------------------------------------------

    /// Calls `report(fixture)` for every fixture whose fat AABB overlaps `aabb`; stop on false.
    pub fn query_aabb(&self, aabb: &Aabb, mut report: impl FnMut(FixtureId) -> bool) {
        self.broad_phase.query(aabb, |proxy_id| {
            let (fid, _) = unpack_proxy(self.broad_phase.user_data(proxy_id));
            report(fid)
        });
    }

    /// `b2World::RayCast`: `report(fixture, point, normal, fraction)` returns the new max fraction
    /// (-1 ignores the fixture, 0 terminates, `fraction` clips the ray).
    pub fn ray_cast(
        &self,
        p1: Vec2,
        p2: Vec2,
        mut report: impl FnMut(FixtureId, Vec2, Vec2, Real) -> Real,
    ) {
        let input = RayCastInput {
            p1,
            p2,
            max_fraction: 1.0,
        };
        self.broad_phase.ray_cast(&input, |sub_input, proxy_id| {
            let (fid, child) = unpack_proxy(self.broad_phase.user_data(proxy_id));
            let fixture = self.fixture(fid);
            let xf = self.body(fixture.body).xf;
            match fixture.shape.ray_cast(sub_input, &xf, child) {
                Some(output) => {
                    let fraction = output.fraction;
                    let point = (1.0 - fraction) * sub_input.p1 + fraction * sub_input.p2;
                    report(fid, point, output.normal, fraction)
                }
                None => sub_input.max_fraction,
            }
        });
    }

    /// Axis-aligned box helper used by tests and callers that need a quick wall.
    pub fn box_shape(half_width: Real, half_height: Real) -> Shape {
        Shape::Polygon(PolygonShape::as_box(half_width, half_height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::{ChainShape, CircleShape, EdgeShape};

    fn ground(world: &mut World) -> BodyId {
        let b = world.create_body(&BodyDef {
            position: vec2(0.0, -10.0),
            ..Default::default()
        });
        world.create_fixture_from_shape(b, World::box_shape(50.0, 10.0), 0.0);
        b
    }

    /// The Box2D "Hello World" example: a 1x1 box dropped from y = 4 settles at (0, 1).
    #[test]
    fn hello_world_box_rests_on_the_ground() {
        let mut world = World::new(vec2(0.0, -10.0));
        ground(&mut world);
        let body = world.create_body(&BodyDef {
            kind: BodyType::Dynamic,
            position: vec2(0.0, 4.0),
            ..Default::default()
        });
        let mut def = FixtureDef::new(World::box_shape(1.0, 1.0), 1.0);
        def.friction = 0.3;
        world.create_fixture(body, &def);

        for _ in 0..90 {
            world.step(1.0 / 60.0, 6, 2);
        }
        let p = world.body(body).position();
        assert!(p.x.abs() < 0.01, "x = {}", p.x);
        assert!((p.y - 1.0).abs() < 0.02, "y = {}", p.y);
        assert!(world.body(body).angle().abs() < 0.01);
    }

    fn dynamic(world: &mut World, x: Real, y: Real, shape: Shape) -> BodyId {
        let body = world.create_body(&BodyDef {
            kind: BodyType::Dynamic,
            position: vec2(x, y),
            ..Default::default()
        });
        world.create_fixture_from_shape(body, shape, 1.0);
        body
    }

    #[test]
    fn a_box_rests_on_an_edge() {
        let mut world = World::new(vec2(0.0, -10.0));
        let ground = world.create_body(&BodyDef::default());
        let edge = Shape::Edge(EdgeShape::new(vec2(-20.0, 0.0), vec2(20.0, 0.0)));
        world.create_fixture_from_shape(ground, edge, 0.0);
        let body = dynamic(&mut world, 0.0, 3.0, World::box_shape(0.5, 0.5));
        for _ in 0..180 {
            world.step(1.0 / 60.0, 6, 2);
        }
        let p = world.body(body).position();
        assert!((p.y - 0.5).abs() < 0.02, "y = {}", p.y);
        assert!(p.x.abs() < 0.01 && world.body(body).angle().abs() < 0.01);
    }

    /// A box slid along a chain of collinear edges does not snag on the interior vertices.
    #[test]
    fn a_box_slides_over_chain_vertices_without_snagging() {
        let mut world = World::new(vec2(0.0, -10.0));
        let ground = world.create_body(&BodyDef::default());
        let pts: Vec<Vec2> = (0..=20)
            .map(|i| vec2(i as Real * 2.0 - 20.0, 0.0))
            .collect();
        world.create_fixture_from_shape(ground, Shape::Chain(ChainShape::create_chain(&pts)), 0.0);
        let body = dynamic(&mut world, -15.0, 0.6, World::box_shape(0.5, 0.5));
        let fixture = world.body(body).fixture_list.unwrap();
        world.fixture_mut(fixture).friction = 0.0;
        world.body_mut(body).linear_velocity = vec2(5.0, 0.0);
        for _ in 0..120 {
            world.step(1.0 / 60.0, 6, 2);
        }
        let b = world.body(body);
        assert!(b.position().x > -5.0, "snagged at x = {}", b.position().x);
        assert!(
            (b.position().y - 0.5).abs() < 0.05,
            "y = {}",
            b.position().y
        );
    }

    #[test]
    fn a_ball_is_held_inside_a_chain_loop() {
        let mut world = World::new(vec2(0.0, -10.0));
        let ground = world.create_body(&BodyDef::default());
        let loop_pts = [
            vec2(-5.0, 0.0),
            vec2(5.0, 0.0),
            vec2(5.0, 10.0),
            vec2(-5.0, 10.0),
        ];
        world.create_fixture_from_shape(
            ground,
            Shape::Chain(ChainShape::create_loop(&loop_pts)),
            0.0,
        );
        let ball = dynamic(&mut world, 0.0, 5.0, Shape::Circle(CircleShape::new(0.5)));
        world.body_mut(ball).linear_velocity = vec2(40.0, 30.0);
        for _ in 0..300 {
            world.step(1.0 / 60.0, 6, 2);
        }
        let p = world.body(ball).position();
        assert!(
            p.x.abs() < 5.0 && p.y > 0.0 && p.y < 10.0,
            "escaped to {p:?}"
        );
    }

    #[test]
    fn edges_do_not_collide_with_each_other() {
        let a = Shape::Edge(EdgeShape::new(vec2(0.0, 0.0), vec2(1.0, 0.0)));
        let b = Shape::Chain(ChainShape::create_chain(&[vec2(0.0, 0.0), vec2(1.0, 0.0)]));
        assert_eq!(World::contact_registered(&a, &a), None);
        assert_eq!(World::contact_registered(&a, &b), None);
    }

    #[test]
    fn a_box_falls_and_goes_to_sleep() {
        let mut world = World::new(vec2(0.0, -10.0));
        ground(&mut world);
        let body = world.create_body(&BodyDef {
            kind: BodyType::Dynamic,
            position: vec2(0.0, 2.0),
            ..Default::default()
        });
        world.create_fixture_from_shape(body, World::box_shape(0.5, 0.5), 1.0);
        for _ in 0..240 {
            world.step(1.0 / 60.0, 8, 3);
        }
        assert!(
            !world.body(body).is_awake(),
            "resting bodies sleep after half a second"
        );
    }

    #[test]
    fn fast_body_does_not_tunnel_through_a_thin_wall() {
        let mut world = World::new(vec2(0.0, 0.0));
        let wall = world.create_body(&BodyDef {
            position: vec2(5.0, 0.0),
            ..Default::default()
        });
        world.create_fixture_from_shape(wall, World::box_shape(0.05, 5.0), 0.0);
        let ball = world.create_body(&BodyDef {
            kind: BodyType::Dynamic,
            position: vec2(0.0, 0.0),
            linear_velocity: vec2(180.0, 0.0),
            bullet: true,
            ..Default::default()
        });
        world.create_fixture_from_shape(ball, World::box_shape(0.25, 0.25), 1.0);
        for _ in 0..30 {
            world.step(1.0 / 60.0, 8, 3);
        }
        assert!(
            world.body(ball).position().x < 5.0,
            "x = {}",
            world.body(ball).position().x
        );
    }

    #[test]
    fn stacked_boxes_stay_stacked() {
        let mut world = World::new(vec2(0.0, -10.0));
        ground(&mut world);
        let mut boxes = Vec::new();
        for i in 0..4 {
            let b = world.create_body(&BodyDef {
                kind: BodyType::Dynamic,
                position: vec2(0.0, 0.5 + i as Real * 1.05),
                ..Default::default()
            });
            world.create_fixture_from_shape(b, World::box_shape(0.5, 0.5), 1.0);
            boxes.push(b);
        }
        for _ in 0..300 {
            world.step(1.0 / 60.0, 8, 3);
        }
        for (i, &b) in boxes.iter().enumerate() {
            let p = world.body(b).position();
            assert!(p.x.abs() < 0.05, "box {i} drifted: {p:?}");
            assert!((p.y - (0.5 + i as Real)).abs() < 0.1, "box {i} at {p:?}");
        }
    }

    #[test]
    fn contact_events_report_begin_and_end() {
        let mut world = World::new(vec2(0.0, -10.0));
        ground(&mut world);
        let body = world.create_body(&BodyDef {
            kind: BodyType::Dynamic,
            position: vec2(0.0, 1.5),
            ..Default::default()
        });
        world.create_fixture_from_shape(body, World::box_shape(0.5, 0.5), 1.0);
        let mut began = false;
        for _ in 0..60 {
            world.step(1.0 / 60.0, 8, 3);
            began |= world
                .take_events()
                .iter()
                .any(|e| matches!(e, ContactEvent::Begin(..)));
        }
        assert!(began);
        world.destroy_body(body);
        assert_eq!(world.contact_count(), 0);
        assert!(
            world
                .take_events()
                .iter()
                .any(|e| matches!(e, ContactEvent::End(..)))
        );
    }

    #[test]
    fn ray_cast_hits_the_ground_top() {
        let mut world = World::new(vec2(0.0, -10.0));
        ground(&mut world);
        let mut hit = None;
        world.ray_cast(vec2(0.0, 5.0), vec2(0.0, -5.0), |_, point, _, fraction| {
            hit = Some(point);
            fraction
        });
        let p = hit.expect("ray hits the ground");
        assert!(p.y.abs() < 1e-4, "{p:?}");
    }
}

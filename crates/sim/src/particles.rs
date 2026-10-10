//! `grid::GridParticles`: cells (and cosmetic material specks) that fly freely until they hit
//! something, then land back in the grid. Ported from `GridParticles::Update` (`0x712e40`), the
//! per-particle integrator (`0x713e20`), the collision stepper (`0x712390`), the dead-particle
//! pass (`0x716600`), `AddAsParticle` (`0x7168c0`), the material creator (`0x716ce0`), the
//! landed-cell hand-off (`0x71a590`) and the landing queue (`0x71fde0` / `0x71fba0`).
//!
//! Not ported: the rendering lists (`0x714a30`), the hittable-entity effects (audio, gore), the
//! cosmetic spark list (`obj+0x9C`), force rectangles (`Job_UpdateParticleForces`) and the
//! `liquid_stains == 2` hook (`0x715ca0`). The three engine pools differ only in draw order and
//! forces, so there is a single list. The engine's "local grid" is a per-frame snapshot of the
//! material ids around the camera; here particles read the live world instead.

use crate::noise::simplex3;
use crate::{AIR, Cell, CellType, MaterialId, MaterialProps, World};

mod explosion;
pub use explosion::{ExplosionConfig, ExplosionReport, QueuedExplosion, RAY_COUNT};

/// The reference's particle RNG (`0x1222388`; steps at `0x44d0e0`, `0x44d140`): the same
/// Park-Miller `Random` class as the script generator.
pub use crate::random::Random as ParticleRng;

/// `+0x6A`: set when created from a cell that the particle may later return to.
pub const FLAG_FROM_CELL: u8 = 0x01;
/// `+0x6A`: lookups use the camera-local material grid and its bounds.
pub const FLAG_LOCAL_GRID: u8 = 0x02;
/// `+0x6A`: finished (landed, expired or out of the window); removed by the dead-particle pass.
pub const FLAG_DEAD: u8 = 0x10;
/// `+0x6A`: bounces off solids instead of dying (30% of cell particles).
pub const FLAG_BOUNCE: u8 = 0x20;
/// `+0x6A`: its material has `liquid_stains == 2`.
pub const FLAG_STAINS: u8 = 0x80;
/// `+0x6A`: set on explosion sparks; its effect is not decoded.
pub const FLAG_UNKNOWN_08: u8 = 0x08;
/// `+0x6B`: carries a cell back into the grid when it dies.
pub const FLAG_HAS_CELL: u8 = 0x01;
/// `+0x6B`: collides with the grid.
pub const FLAG_COLLIDE: u8 = 0x02;
/// `+0x6B`: wraps around the local grid horizontally.
pub const FLAG_WRAP_X: u8 = 0x04;
/// `+0x6B`: dies when `lifetime` runs out.
pub const FLAG_LIFETIME: u8 = 0x20;
/// `+0x6B`: dies once it has nearly stopped (needs damping).
pub const FLAG_SETTLE: u8 = 0x40;
/// `+0x6C`: a landing cell that is not placed within 60 frames is discarded.
pub const FLAG_EXPIRES: u8 = 0x02;

/// `+0x69`
pub const MODE_NORMAL: u8 = 0;
/// Rising: reduced gravity; passes lighter materials until it stops.
pub const MODE_RISING: u8 = 1;
/// No collision.
pub const MODE_FREE: u8 = 2;

/// One 0x70-byte particle record.
#[derive(Clone, Debug, PartialEq)]
pub struct Particle {
    /// `+0x00`: swirl force strength (0 = none).
    pub swirl_amp: f32,
    /// `+0x04`: swirl noise frequency.
    pub swirl_freq: f32,
    /// `+0x0C..+0x18`: r, g, b and alpha (`fade`), each 0..=1.
    pub color: [f32; 4],
    /// `+0x1C`
    pub pos: (f32, f32),
    /// `+0x24`: trailing position used for drawing.
    pub prev: (f32, f32),
    /// `+0x2C`
    pub vel: (f32, f32),
    /// `+0x34`: constant acceleration (default gravity 10).
    pub accel: (f32, f32),
    /// `+0x3C`: compared with the density of what it touches (default 1000).
    pub density: f32,
    /// `+0x40`: contacts left while rising (default 20).
    pub bounces: i32,
    /// `+0x44`
    pub age: i32,
    /// `+0x48`: unused by the simulation (default 12).
    pub size: f32,
    /// `+0x4C`: alpha change per second.
    pub fade_rate: f32,
    /// `+0x50`: the cell this particle carries.
    pub cell: Option<Cell>,
    /// `+0x54`: last grid cell visited, as floats.
    pub last_cell: (f32, f32),
    /// `+0x5C`: seconds left (with `FLAG_LIFETIME`).
    pub lifetime: f32,
    /// `+0x60`
    pub damping: f32,
    /// `+0x64`: index into the attractor table, or -1.
    pub attractor: i16,
    /// `+0x66`
    pub material: u16,
    /// `+0x68`: cell type code (1 liquid, 2 gas, 3 solid, 4 fire).
    pub kind: u8,
    /// `+0x69`
    pub mode: u8,
    /// `+0x6A`
    pub flags_a: u8,
    /// `+0x6B`
    pub flags_b: u8,
    /// `+0x6C`
    pub flags_c: u8,
}

impl Particle {
    /// The record constructor (`0x712010`); the caller has already drawn its RNG value.
    fn blank() -> Self {
        Self {
            swirl_amp: 0.0,
            swirl_freq: 0.0,
            color: [0.0, 0.0, 1.0, 0.5],
            pos: (0.0, 0.0),
            prev: (0.0, 0.0),
            vel: (0.0, 0.0),
            accel: (0.0, 10.0),
            density: 1000.0,
            bounces: 20,
            age: 0,
            size: 12.0,
            fade_rate: 0.0,
            cell: None,
            last_cell: (0.0, 0.0),
            lifetime: 1.0,
            damping: 0.0,
            attractor: -1,
            material: 0,
            kind: 0,
            mode: MODE_NORMAL,
            flags_a: 0,
            flags_b: 0x83,
            flags_c: 0,
        }
    }

    pub fn is_dead(&self) -> bool {
        self.flags_a & FLAG_DEAD != 0
    }

    fn set_color(&mut self, argb: u32) {
        let channel = |shift: u32| ((argb >> shift) & 0xFF) as f32 / 255.0;
        self.color = [
            channel(16),
            channel(8),
            channel(0),
            (argb >> 24) as f32 / 255.0,
        ];
    }
}

/// Arguments of the material-speck creator (`0x716ce0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speck {
    pub material: MaterialId,
    pub pos: (f32, f32),
    pub vel: (f32, f32),
    /// ARGB; the material's color at the position when `None`.
    pub color: Option<u32>,
    /// Lifetime range in seconds; the draw picks within it.
    pub life: (f32, f32),
    /// Skip the "position must be empty" check.
    pub allow_occupied: bool,
    /// Require the position to lie in the local grid, and collide with the world.
    pub in_grid: bool,
}

/// A cell on its way back into the grid (the cell object plus its `+0x8`/`+0xC` landing fields).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landing {
    pub cell: Cell,
    /// `SetPos`: where landing starts looking.
    pub x: i32,
    pub y: i32,
    /// `+0x8`: -9999 = arm a 60-frame deadline, > 0 = frame deadline, -1 = none.
    pub timer: i32,
    /// `+0xC`: suppresses the deadline logic.
    pub keep: bool,
}

/// A rectangle (`min x, min y, max x, max y`) in which cell particles that have flown for more
/// than 5 frames are caught (the "hittable" entity boxes passed to `0x716600`).
pub type HitBox = [f32; 4];

const TYPE_LIQUID: i32 = 1;
const TYPE_GAS: i32 = 2;
const TYPE_SOLID: i32 = 3;
const TYPE_FIRE: i32 = 4;

fn type_code(props: &MaterialProps) -> i32 {
    match props.cell_type {
        CellType::Liquid => TYPE_LIQUID,
        CellType::Gas => TYPE_GAS,
        CellType::Solid => TYPE_SOLID,
        CellType::Fire => TYPE_FIRE,
        CellType::Unspecified => 0,
    }
}

/// Landing queue budget after a successful landing (`+0x5BC`).
const LANDING_BUDGET: i32 = 500;
/// Frames a landing cell waits for a spot before being discarded (`0x71a590`).
const LANDING_DEADLINE_FRAMES: u32 = 0x3C;
/// Rows scanned upward for a free spot (`0x71fba0`).
const LANDING_SCAN_ROWS: i32 = 128;
const MAX_PARTICLES: usize = 100_000;

/// All flying particles and the cells waiting to land.
#[derive(Clone, Debug)]
pub struct Particles {
    pub rng: ParticleRng,
    pub particles: Vec<Particle>,
    /// `obj+0xA8`: attractor points `(x, y, strength)`.
    pub attractors: Vec<[f32; 3]>,
    /// Entity boxes that catch cell particles.
    pub hit_boxes: Vec<HitBox>,
    /// `obj+0x88`: cells released by dead particles this frame.
    landed: Vec<Landing>,
    /// `GridWorld+0x5A4`
    immediate: Vec<Landing>,
    /// `GridWorld+0x5B0`
    delayed: Vec<Landing>,
    /// `GridWorld+0x5BC`
    budget: i32,
    /// Camera-local material grid origin and size (`GridWorld+0x498`, `obj+0x24`).
    local_origin: (i32, i32),
    local_size: (i32, i32),
    /// Particles outside this inclusive window die (`obj+0x10..+0x1C`).
    window: Option<(i32, i32, i32, i32)>,
    /// `0x12059f0`: noisy stain strength per degree, filled by the first explosion.
    stain_table: Option<[f32; RAY_COUNT]>,
    /// Explosions held back by the dispatcher's one-per-frame limit (`0x657410`).
    deferred_explosions: Vec<QueuedExplosion>,
    /// `DAT_01206fd4`: explosions run this frame.
    explosions_this_frame: u32,
}

impl Particles {
    pub fn new(seed: u32) -> Self {
        Self {
            rng: ParticleRng::new(seed),
            particles: Vec::new(),
            attractors: Vec::new(),
            hit_boxes: Vec::new(),
            landed: Vec::new(),
            immediate: Vec::new(),
            delayed: Vec::new(),
            budget: LANDING_BUDGET,
            local_origin: (0, 0),
            local_size: (i32::MAX, i32::MAX),
            window: None,
            stain_table: None,
            deferred_explosions: Vec::new(),
            explosions_this_frame: 0,
        }
    }

    /// Sets the camera-local grid used by particles with `FLAG_LOCAL_GRID`.
    pub fn set_local_grid(&mut self, origin: (i32, i32), size: (i32, i32)) {
        self.local_origin = origin;
        self.local_size = size;
    }

    /// Sets the window (inclusive `min x, min y, max x, max y`) particles must stay inside; the
    /// default is the whole world.
    pub fn set_window(&mut self, window: (i32, i32, i32, i32)) {
        self.window = Some(window);
    }

    /// Cells waiting to land (immediate plus delayed).
    pub fn waiting(&self) -> usize {
        self.immediate.len() + self.delayed.len() + self.landed.len()
    }

    /// `AddAsParticle` (`0x7168c0`) for a cell that is no longer in the grid at `(x, y)`.
    /// Returns false (and queues the cell to land) when `local_grid` is set and the position is
    /// outside the local grid, or when the pool is full.
    pub fn add_cell(
        &mut self,
        world: &World,
        cell: Cell,
        (x, y): (i32, i32),
        vel: (f32, f32),
        rising: bool,
        local_grid: bool,
    ) -> bool {
        let landing = Landing {
            cell,
            x,
            y,
            timer: -1,
            keep: false,
        };
        if local_grid && !self.in_local_grid(x, y) {
            self.landed.push(landing);
            return false;
        }
        if self.particles.len() >= MAX_PARTICLES {
            return false;
        }
        let mut p = self.new_record();
        let props = world.materials().get(cell.material);
        p.pos = (x as f32 + 0.5, y as f32 + 0.5);
        p.last_cell = p.pos;
        p.prev = p.pos;
        p.vel = vel;
        p.cell = Some(cell);
        p.set_color(cell.color);
        p.density = props.map_or(p.density, |m| m.density);
        if local_grid {
            p.flags_a |= FLAG_LOCAL_GRID;
        }
        p.material = cell.material.0;
        if let Some(m) = props {
            p.kind = type_code(m) as u8;
            if m.liquid_stains == 2 {
                p.flags_a |= FLAG_STAINS;
            }
        }
        // `(float)step * 4.656613e-10 * 100.0 + 0.0 < 30.0`
        let roll = self.rng.step() as f32 * 4.656_613e-10_f32 * 100.0 + 0.0;
        if roll < 30.0 {
            p.flags_a |= FLAG_BOUNCE;
        }
        if rising {
            p.mode = MODE_RISING;
            if -50.0 < vel.1 {
                p.density = 0.0;
            }
            if self.rng.int(0, 100) < 0x32 {
                p.density = 0.0;
            }
        }
        self.particles.push(p);
        true
    }

    /// `0x717670`: turns the grid cell at `(x, y)` into a rising particle when it is a movable
    /// liquid (not static, not sand-like).
    pub fn add_from_grid(
        &mut self,
        world: &mut World,
        (x, y): (i32, i32),
        vel: (f32, f32),
    ) -> bool {
        if !self.in_local_grid(x, y) {
            return false;
        }
        let Some(cell) = world.cell(x, y).copied() else {
            return false;
        };
        let Some(props) = world.materials().get(cell.material) else {
            return false;
        };
        let settled = cell.is_static
            || (!props.liquid_sand_never_box2d
                && props.liquid_sand
                && cell.state == crate::STATE_RESTING);
        if cell.material == AIR
            || props.cell_type != CellType::Liquid
            || settled
            || props.liquid_sand
        {
            return false;
        }
        *world.cell_mut(x, y).expect("in bounds") = Cell::EMPTY;
        self.add_cell(world, cell, (x, y), vel, true, true)
    }

    /// `0x716ce0`: a cosmetic particle of `material` (no cell to return) at `(x, y)`.
    pub fn add_from_material(&mut self, world: &World, speck: Speck) -> bool {
        let Speck {
            material,
            pos: (px, py),
            vel,
            color,
            life,
            allow_occupied,
            in_grid,
        } = speck;
        let (x, y) = (px as i32, py as i32);
        if in_grid && !self.in_local_grid(x, y) {
            return false;
        }
        if !allow_occupied && world.get(x, y).is_some_and(|m| m != AIR) {
            return false;
        }
        let Some(props) = world.materials().get(material) else {
            return false;
        };
        if material == AIR || self.particles.len() >= MAX_PARTICLES {
            return false;
        }
        let mut p = self.new_record();
        p.pos = (px + 0.5, py + 0.5);
        p.last_cell = p.pos;
        p.prev = p.pos;
        p.set_color(color.unwrap_or_else(|| props.color_at(x, y)));
        if in_grid {
            p.flags_b |= FLAG_COLLIDE;
        } else {
            p.flags_b &= !FLAG_COLLIDE;
        }
        p.vel = vel;
        p.flags_b |= FLAG_LIFETIME;
        p.flags_a |= FLAG_BOUNCE | 0x40;
        p.lifetime = self.rng.float(life.0, life.1);
        if self.rng.int(0, 100) < 0x4B {
            p.flags_b &= 0x7F;
        }
        p.material = material.0;
        p.kind = type_code(props) as u8;
        if props.liquid_stains == 2 {
            p.flags_a |= FLAG_STAINS;
        }
        self.particles.push(p);
        true
    }

    fn in_local_grid(&self, x: i32, y: i32) -> bool {
        let lx = i64::from(x) - i64::from(self.local_origin.0);
        let ly = i64::from(y) - i64::from(self.local_origin.1);
        (0..i64::from(self.local_size.0)).contains(&lx)
            && (0..i64::from(self.local_size.1)).contains(&ly)
    }

    /// Allocates a default record (`0x717400` -> `0x712010`), drawing one RNG value like the
    /// constructor does.
    fn new_record(&mut self) -> Particle {
        self.rng.int(0, 100);
        Particle::blank()
    }

    /// One frame (`GridParticles::Update` then the landing hand-off): integrates every particle
    /// by `dt` seconds at game time `time` (the swirl noise's third coordinate), removes dead
    /// ones, hands their cells to the landing queue and lands what it can.
    pub fn update(&mut self, world: &mut World, dt: f32, time: f64) {
        for s in world.take_specks() {
            self.add_blob_speck(world, s);
        }
        let window = self
            .window
            .unwrap_or((0, 0, world.width() - 1, world.height() - 1));
        for i in 0..self.particles.len() {
            let mut p = std::mem::replace(&mut self.particles[i], Particle::blank());
            self.integrate(&mut p, world, dt, time, window);
            self.particles[i] = p;
        }
        self.remove_dead();
        self.hand_off(world.frame);
        self.land(world);
    }

    /// A reaction blob's speck (`0x71ab50`): a short-lived rising, swirling, fading speck that
    /// ignores the grid.
    fn add_blob_speck(&mut self, world: &World, s: crate::CosmeticSpeck) {
        let speck = Speck {
            material: s.material,
            pos: s.pos,
            vel: s.vel,
            color: None,
            life: (0.5, 1.2),
            allow_occupied: true,
            in_grid: true,
        };
        if self.add_from_material(world, speck)
            && let Some(p) = self.particles.last_mut()
        {
            p.swirl_amp = 0.25;
            p.swirl_freq = 0.25;
            p.accel = (0.0, -5.0);
            p.mode = MODE_FREE;
            p.fade_rate = -1.0 / p.lifetime;
        }
    }

    /// `0x713e20`
    fn integrate(
        &mut self,
        p: &mut Particle,
        world: &World,
        dt: f32,
        time: f64,
        window: (i32, i32, i32, i32),
    ) {
        p.age += 1;
        let new_y = p.pos.1 + p.vel.1 * dt;
        let new_x = p.vel.0 * dt + p.pos.0;
        let fade = p.fade_rate * dt + p.color[3];
        p.color[3] = fade.clamp(0.0, 1.0);
        if p.attractor > -1
            && let Some(a) = self.attractors.get(p.attractor as usize)
        {
            p.vel.0 += (a[0] - p.pos.0) * dt * a[2];
            p.vel.1 += (a[1] - p.pos.1) * dt * a[2];
        }
        if p.mode == MODE_RISING {
            p.vel.0 += p.accel.0 * 0.25 * dt;
            p.vel.1 += p.accel.1 * 0.25 * dt;
        } else {
            p.vel.0 += p.accel.0 * dt;
            p.vel.1 += p.accel.1 * dt;
        }
        let amp = p.swirl_amp;
        if 0.0 < amp {
            let n = simplex3(
                f64::from(new_x * p.swirl_freq),
                f64::from(new_y * p.swirl_freq),
                time,
            );
            let angle = (n as f32) * std::f32::consts::PI;
            let c = f64::from(angle).cos() as f32;
            let s = f64::from(angle).sin() as f32;
            p.vel.1 += c * amp + s * 0.0;
            p.vel.0 += c * 0.0 - s * amp;
        }
        let damping = p.damping;
        if damping != 0.0 {
            p.vel.0 -= p.vel.0 * damping * dt;
            p.vel.1 -= p.vel.1 * damping * dt;
            if p.flags_b & FLAG_SETTLE != 0 && p.vel.0 * p.vel.0 + p.vel.1 * p.vel.1 < 0.010_000_001
            {
                p.flags_a |= FLAG_DEAD;
            }
        }
        if p.flags_b & FLAG_LIFETIME != 0 {
            let before = p.lifetime;
            p.lifetime = before - dt;
            if before - dt < 0.0 {
                p.flags_a |= FLAG_DEAD;
            }
        }
        if p.flags_a & FLAG_FROM_CELL == 0 && p.mode == MODE_RISING && 0.0 < p.vel.1 {
            p.mode = MODE_NORMAL;
        }
        if p.mode == MODE_FREE {
            p.prev = p.pos;
            p.pos = (new_x, new_y);
        } else {
            let old = p.pos;
            self.collide(p, world, (new_x, new_y), window);
            let speed = (p.vel.0 * p.vel.0 + p.vel.1 * p.vel.1).sqrt();
            let dy = old.1 - p.pos.1;
            let dx = old.0 - p.pos.0;
            let len = (dy * dy + dx * dx).sqrt();
            let (ux, uy) = if len <= 0.0 {
                (0.0, 0.0)
            } else {
                (dx / len, dy / len)
            };
            p.prev = (ux * speed * dt + p.pos.0, p.pos.1 + uy * speed * dt);
        }
    }

    /// `0x712390`: moves `p` toward `new`, one grid cell at a time (at most 60 steps).
    fn collide(
        &mut self,
        p: &mut Particle,
        world: &World,
        new: (f32, f32),
        window: (i32, i32, i32, i32),
    ) {
        let (ix, iy) = (new.0 as i32, new.1 as i32);
        if ix < window.0 || window.2 < ix || iy < window.1 || window.3 < iy {
            p.flags_a |= FLAG_DEAD;
            return;
        }
        if p.flags_b & FLAG_COLLIDE == 0 {
            p.pos = new;
            return;
        }
        let dy = new.1 - p.last_cell.1;
        let dx = new.0 - p.last_cell.0;
        let len = (dy * dy + dx * dx).sqrt();
        let steps = ((len + 0.5) as i32).min(60);
        if steps != 0 {
            let (ux, uy) = if 0.0 < len {
                (dx / len, dy / len)
            } else {
                (0.0, 0.0)
            };
            let (mut cx, mut cy) = p.pos;
            for i in 0..steps {
                cx += ux;
                cy += uy;
                let icx = cx as i32;
                if icx == p.pos.0 as i32 && cy as i32 == p.pos.1 as i32 && i >= 1 {
                    continue;
                }
                let local = p.flags_a & FLAG_LOCAL_GRID != 0;
                let (gx, gy) = if local {
                    (
                        (cx - self.local_origin.0 as f32) as i32,
                        (cy - self.local_origin.1 as f32) as i32,
                    )
                } else {
                    (icx, cy as i32)
                };
                if local
                    && !(0 <= gx && gx < self.local_size.0 && 0 <= gy && gy < self.local_size.1)
                {
                    if p.flags_b & FLAG_WRAP_X != 0 {
                        let width = self.local_size.0 as f32;
                        if gx < 0 {
                            p.pos.0 += width;
                            p.last_cell = p.pos;
                            p.prev.0 += width;
                            return;
                        }
                        if self.local_size.0 <= gx {
                            p.pos.0 -= width;
                            p.last_cell = p.pos;
                            p.prev.0 -= width;
                            return;
                        }
                    }
                    p.flags_a |= FLAG_DEAD;
                    return;
                }
                let (wx, wy) = if local {
                    (gx + self.local_origin.0, gy + self.local_origin.1)
                } else {
                    (gx, gy)
                };
                let id = world.get(wx, wy).filter(|&m| m != AIR);
                let props = id.and_then(|m| world.materials().get(m));
                match (id, props) {
                    (Some(id), Some(cd)) => {
                        let kind = type_code(cd);
                        if p.mode == MODE_RISING {
                            if p.bounces < 0 {
                                p.mode = MODE_NORMAL;
                            }
                            if kind != TYPE_LIQUID && kind != TYPE_SOLID {
                                if kind == TYPE_GAS || kind == TYPE_FIRE {
                                    p.mode = MODE_NORMAL;
                                }
                            } else {
                                p.bounces -= 1;
                                let from_cell = p.flags_a & FLAG_FROM_CELL != 0;
                                let own = p.cell.map(|c| c.material);
                                let passes = (from_cell || p.density < cd.density)
                                    && (!from_cell || own == Some(id) || p.density <= cd.density);
                                if !passes {
                                    p.mode = MODE_NORMAL;
                                }
                            }
                        } else if p.mode == MODE_NORMAL
                            && (kind == TYPE_LIQUID || kind == TYPE_SOLID)
                        {
                            if p.flags_a & FLAG_BOUNCE != 0 && p.kind != 1 {
                                self.bounce(p, ux, uy);
                                return;
                            }
                            p.flags_a |= FLAG_DEAD;
                            return;
                        }
                    }
                    _ => {
                        if p.mode == MODE_RISING {
                            p.mode = MODE_NORMAL;
                        }
                    }
                }
                p.pos = (cx, cy);
                p.last_cell = (icx as f32, (cy as i32) as f32);
            }
        }
        p.pos = new;
    }

    /// The bounce branch of `0x712390`: reflect along the dominant axis, add noise, damp.
    fn bounce(&mut self, p: &mut Particle, ux: f32, uy: f32) {
        if ux.abs() <= uy.abs() {
            p.vel.1 *= -1.0;
        } else {
            p.vel.0 *= -1.0;
        }
        if p.vel.0.abs() < 10.0 {
            p.vel.0 += self.rng.float(-10.0, 10.0);
        }
        let s = self.rng.float(0.1, 0.3);
        p.vel.0 *= s;
        p.vel.1 *= s;
        let speed2 = p.vel.0 * p.vel.0 + p.vel.1 * p.vel.1;
        if 1.0 < p.lifetime && speed2 < 1600.0 {
            p.lifetime = self.rng.float(0.5, 1.5);
        }
        if speed2 < 100.0 {
            p.flags_a &= !FLAG_BOUNCE;
        }
    }

    /// `0x716600`: catches particles inside hittable boxes, then removes the dead ones (the
    /// last record fills the gap) and releases their cells.
    fn remove_dead(&mut self) {
        let mut i = 0;
        while i < self.particles.len() {
            let p = &mut self.particles[i];
            if !p.is_dead() && p.flags_b & FLAG_HAS_CELL != 0 && p.cell.is_some() && 5 < p.age {
                let (x, y) = p.pos;
                for b in &self.hit_boxes {
                    if y < b[1] {
                        break;
                    }
                    if b[0] <= x && x <= b[2] && b[1] <= y && y <= b[3] {
                        p.flags_a |= FLAG_DEAD;
                        break;
                    }
                }
            }
            if !p.is_dead() {
                i += 1;
                continue;
            }
            let p = self.particles.swap_remove(i);
            if p.flags_b & FLAG_HAS_CELL != 0
                && let Some(cell) = p.cell
            {
                let expires = p.flags_c & FLAG_EXPIRES != 0;
                self.landed.push(Landing {
                    cell,
                    x: p.pos.0 as i32,
                    y: p.pos.1 as i32,
                    timer: if expires { -9999 } else { -1 },
                    keep: false,
                });
            }
        }
    }

    /// `0x71a590`: moves released cells into the landing queue, arming their deadlines.
    fn hand_off(&mut self, frame: u32) {
        for mut c in std::mem::take(&mut self.landed) {
            if c.timer == -9999 && !c.keep {
                c.timer = (frame + LANDING_DEADLINE_FRAMES) as i32;
            } else if c.timer > 0 && !c.keep {
                c.timer = -1;
            }
            self.immediate.push(c);
        }
    }

    /// `0x71fde0`: the delayed list is retried in random order within a budget, then the
    /// immediate list is placed; failures become delayed.
    fn land(&mut self, world: &mut World) {
        if self.immediate.is_empty() && self.delayed.is_empty() {
            return;
        }
        let frame = world.frame as i32;
        if !self.delayed.is_empty() {
            let mut start = (world.global_rng.rand() as usize) % self.delayed.len();
            if !self.immediate.is_empty() && 300 < self.budget {
                self.budget = 300;
            }
            self.budget = self.budget.min(self.delayed.len() as i32);
            let mut placed_any = false;
            let mut done = 0;
            if 0 < self.budget {
                loop {
                    if self.delayed.is_empty() {
                        break;
                    }
                    if self.delayed.len() <= start {
                        start = 0;
                    }
                    done += 1;
                    let c = self.delayed[start];
                    if !c.keep && 0 < c.timer && c.timer < frame {
                        self.delayed.swap_remove(start);
                    } else {
                        let mut displaced = Vec::new();
                        if Self::place(world, &c, &mut displaced) {
                            placed_any = true;
                            self.delayed.extend(displaced);
                            self.delayed.swap_remove(start);
                        } else {
                            start += 1;
                        }
                    }
                    if done >= self.budget {
                        break;
                    }
                }
            }
            if placed_any {
                self.budget = LANDING_BUDGET;
            } else if 20 < self.budget {
                self.budget = (self.budget as f32 * 0.5) as i32;
            }
            self.delayed.truncate(MAX_PARTICLES);
        }
        if !self.immediate.is_empty() {
            let mut displaced = Vec::new();
            for c in std::mem::take(&mut self.immediate) {
                if !Self::place(world, &c, &mut displaced) {
                    self.delayed.push(c);
                    self.budget = LANDING_BUDGET;
                }
            }
            self.immediate = displaced;
        }
    }

    /// `0x71fba0`: scans up to 128 rows upward from the cell's position for an empty spot or one
    /// holding a lighter non-solid cell (which is displaced). A solid cell ends the scan; a
    /// blocking cell makes the column drift sideways now and then.
    fn place(world: &mut World, landing: &Landing, displaced: &mut Vec<Landing>) -> bool {
        let Some(mine) = world.materials().get(landing.cell.material) else {
            return false;
        };
        let density = mine.density;
        let (mut x, start_y) = (landing.x, landing.y);
        let mut y = start_y;
        while y >= start_y - LANDING_SCAN_ROWS {
            if world.in_bounds(x, y) {
                let occupant = *world.cell(x, y).expect("in bounds");
                let theirs = (occupant.material != AIR)
                    .then(|| {
                        world
                            .materials()
                            .get(occupant.material)
                            .map(|t| (type_code(t), t.density))
                    })
                    .flatten();
                let free = match theirs {
                    None => true,
                    Some((kind, d)) => kind != TYPE_SOLID && d < density,
                };
                if free {
                    let mut cell = landing.cell;
                    cell.rest = cell.rest.min(45);
                    *world.cell_mut(x, y).expect("in bounds") = cell;
                    world.mark(x, y);
                    if theirs.is_some() {
                        displaced.push(Landing {
                            cell: occupant,
                            x,
                            y,
                            timer: -1,
                            keep: false,
                        });
                    }
                    return true;
                }
                if theirs.is_some_and(|(kind, _)| kind == TYPE_SOLID) {
                    return false;
                }
                if y < start_y - 5 && world.global_rng.rand() % 101 < 20 {
                    x += if world.global_rng.rand() % 101 < 50 {
                        1
                    } else {
                        -1
                    };
                }
            }
            y -= 1;
        }
        false
    }

    /// Number of live particles.
    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }
}

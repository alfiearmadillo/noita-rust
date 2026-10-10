//! The cell-changing part of `ConfigExplosion` (`0x657960`): 360 rays measure how far the blast
//! reaches through sand and solid pixels, then every cell in the square around the center is
//! stained, worn down, thrown as a particle, removed or replaced, and sparks are scattered.
//!
//! The global `rand()` (`world.global_rng`) is drawn in the reference's order for the parts
//! ported here, but the entity side of the function (damage, knockback, physics bodies, cracks,
//! electricity, camera shake, light, audio, pixel sprites) draws from it too and is not ported,
//! so streams only match until the first such effect.

use std::collections::BTreeMap;

use super::{FLAG_UNKNOWN_08, Particles, Speck};
use crate::{AIR, Cell, CellType, MaterialId, Materials, World, darkened};

/// Rays cast by an explosion, one per degree.
pub const RAY_COUNT: usize = 360;

/// An `ExplosionConfig` (serializer `0x4bb980`, defaults `0x4b2780`); only what cells use.
#[derive(Clone, Debug, PartialEq)]
pub struct ExplosionConfig {
    /// `+0x04` `never_cache`: the dispatcher (`0x657410`) always runs it at once.
    pub never_cache: bool,
    /// `+0x08` `explosion_radius`
    pub radius: f32,
    /// `+0x78` `hole_enabled`
    pub hole_enabled: bool,
    /// `+0x79` `destroy_non_platform_solid_enabled`
    pub destroy_non_platform_solid: bool,
    /// `+0x8C` `hole_destroy_liquid`: liquids other than sand are destroyed instead of thrown.
    pub hole_destroy_liquid: bool,
    /// `+0x8D` `hole_destroy_physics_dynamic`
    pub hole_destroy_physics_dynamic: bool,
    /// `+0x90` `create_cell_material`; empty means none.
    pub create_cell_material: String,
    /// `+0xA8` `create_cell_probability` (percent)
    pub create_cell_probability: i32,
    /// `+0xB0` `spark_material`; empty means none.
    pub spark_material: String,
    /// `+0xC8`: cells need at least this much `hp` to shed material sparks.
    pub material_sparks_min_hp: i32,
    /// `+0xCC` `material_sparks_probability`
    pub material_sparks_probability: i32,
    /// `+0xD0` / `+0xD4`
    pub material_sparks_count: (i32, i32),
    /// `+0xD8`
    pub material_sparks_enabled: bool,
    /// `+0xD9`: real sparks (cells) instead of specks; not ported.
    pub material_sparks_real: bool,
    /// `+0xDA`
    pub material_sparks_scale_with_hp: bool,
    /// `+0xDB` `sparks_enabled`
    pub sparks_enabled: bool,
    /// `+0xDC` / `+0xE0`
    pub sparks_count: (i32, i32),
    /// `+0xE4` `sparks_inner_radius_coeff`
    pub sparks_inner_radius_coeff: f32,
    /// `+0xE8` `stains_enabled`
    pub stains_enabled: bool,
    /// `+0xEC` `stains_radius` (0 = 0.4 * radius)
    pub stains_radius: f32,
    /// `+0xF0` `ray_energy`
    pub ray_energy: i32,
    /// `+0xF4` `max_durability_to_destroy`
    pub max_durability_to_destroy: i32,
    /// `+0x10C` `cell_explosion_power`: a burning material's blast radius is
    /// `sqrt(power * (0.75..1.25))`, at most 150.
    pub cell_explosion_power: f32,
    /// `+0x110` / `+0x114`: radius range of a reaction's blast.
    pub cell_explosion_radius: (f32, f32),
    /// `+0x11C` `cell_explosion_damage_required`
    pub cell_explosion_damage_required: f32,
    /// `+0x120` `cell_explosion_probability`: chance a reaction triggers its blast.
    pub cell_explosion_probability: f32,
    /// Global `0x1207028`: percent chance a destroyed sand cell becomes `explosion_dirt`
    /// (zero in the executable image; its writer is not found).
    pub sand_to_dirt_percent: i32,
}

impl Default for ExplosionConfig {
    /// Constructor defaults (`0x4b2780`).
    fn default() -> Self {
        Self {
            never_cache: true,
            radius: 20.0,
            hole_enabled: true,
            destroy_non_platform_solid: true,
            hole_destroy_liquid: false,
            hole_destroy_physics_dynamic: true,
            create_cell_material: "fire".to_owned(),
            create_cell_probability: 5,
            spark_material: "spark".to_owned(),
            material_sparks_min_hp: 10_000,
            material_sparks_probability: 5,
            material_sparks_count: (7, 20),
            material_sparks_enabled: false,
            material_sparks_real: false,
            material_sparks_scale_with_hp: false,
            sparks_enabled: true,
            sparks_count: (7, 20),
            sparks_inner_radius_coeff: f32::from_bits(0x3EAA_7EFA),
            stains_enabled: true,
            stains_radius: 0.0,
            ray_energy: 20_000,
            max_durability_to_destroy: 10,
            cell_explosion_power: 1.0,
            cell_explosion_radius: (5.0, 150.0),
            cell_explosion_damage_required: 100.0,
            cell_explosion_probability: 0.0,
            sand_to_dirt_percent: 0,
        }
    }
}

impl ExplosionConfig {
    /// Applies XML attributes (serializer `0x4bb980`); names that cells do not use are ignored.
    pub fn apply(&mut self, attrs: &BTreeMap<String, String>) {
        let flag = |v: &str| matches!(v.trim(), "1" | "true");
        let int = |v: &str| v.trim().parse::<i32>().ok();
        let float = |v: &str| v.trim().parse::<f32>().ok();
        for (key, value) in attrs {
            let v = value.as_str();
            match key.as_str() {
                "never_cache" => self.never_cache = flag(v),
                "explosion_radius" => set(&mut self.radius, float(v)),
                "hole_enabled" => self.hole_enabled = flag(v),
                "destroy_non_platform_solid_enabled" => self.destroy_non_platform_solid = flag(v),
                "hole_destroy_liquid" => self.hole_destroy_liquid = flag(v),
                "hole_destroy_physics_dynamic" => self.hole_destroy_physics_dynamic = flag(v),
                "create_cell_material" => self.create_cell_material = v.trim().to_owned(),
                "create_cell_probability" => set(&mut self.create_cell_probability, int(v)),
                "spark_material" => self.spark_material = v.trim().to_owned(),
                "material_sparks_probability" => {
                    set(&mut self.material_sparks_probability, int(v));
                }
                "material_sparks_count_min" => set(&mut self.material_sparks_count.0, int(v)),
                "material_sparks_count_max" => set(&mut self.material_sparks_count.1, int(v)),
                "material_sparks_enabled" => self.material_sparks_enabled = flag(v),
                "material_sparks_real" => self.material_sparks_real = flag(v),
                "material_sparks_scale_with_hp" => self.material_sparks_scale_with_hp = flag(v),
                "sparks_enabled" => self.sparks_enabled = flag(v),
                "sparks_count_min" => set(&mut self.sparks_count.0, int(v)),
                "sparks_count_max" => set(&mut self.sparks_count.1, int(v)),
                "sparks_inner_radius_coeff" => set(&mut self.sparks_inner_radius_coeff, float(v)),
                "stains_enabled" => self.stains_enabled = flag(v),
                "stains_radius" => set(&mut self.stains_radius, float(v)),
                "ray_energy" => set(&mut self.ray_energy, int(v)),
                "max_durability_to_destroy" => set(&mut self.max_durability_to_destroy, int(v)),
                "cell_explosion_power" => set(&mut self.cell_explosion_power, float(v)),
                "cell_explosion_radius_min" => set(&mut self.cell_explosion_radius.0, float(v)),
                "cell_explosion_radius_max" => set(&mut self.cell_explosion_radius.1, float(v)),
                "cell_explosion_damage_required" => {
                    set(&mut self.cell_explosion_damage_required, float(v));
                }
                "cell_explosion_probability" => set(&mut self.cell_explosion_probability, float(v)),
                _ => {}
            }
        }
    }
}

fn set<T>(slot: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *slot = v;
    }
}
/// An explosion waiting to run: a config copy (radius already chosen) and its center.
#[derive(Clone, Debug, PartialEq)]
pub struct QueuedExplosion {
    pub config: ExplosionConfig,
    pub pos: (f32, f32),
}
/// What an explosion did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExplosionReport {
    /// Cells destroyed per material id (the engine's `vector<double>`).
    pub destroyed: Vec<f64>,
    /// Material specks and rising liquid cells launched.
    pub thrown: usize,
    /// Sparks created.
    pub sparks: usize,
}

/// The config's material names resolved against the world's table.
#[derive(Clone, Copy)]
struct Mats {
    create: Option<MaterialId>,
    spark: Option<MaterialId>,
    dirt: Option<MaterialId>,
}

/// Per-ray result of `0x656b10`.
struct Ray {
    /// Squared distance reached.
    len2: f32,
    /// Largest `min(hp, energy left)` taken from one cell.
    absorb: i32,
}

/// Cell properties the cell loop reads.
#[derive(Clone, Copy)]
struct Info {
    sand: bool,
    liquid: bool,
    solid: bool,
    durability: i32,
    stainable: bool,
    platform: i32,
}

fn info_of(materials: &Materials, material: MaterialId) -> Option<Info> {
    let m = materials.get(material)?;
    Some(Info {
        sand: m.liquid_sand,
        liquid: m.cell_type == CellType::Liquid,
        solid: m.cell_type == CellType::Solid,
        durability: m.durability,
        stainable: m.stainable,
        platform: m.platform_type,
    })
}

/// `rand() / 32767.0`
fn unit(world: &mut World) -> f32 {
    world.global_rng.rand() as f32 / 32767.0
}

/// Ray index of an offset (`atan2` in degrees + 179, rounded half away from zero, clamped).
fn ray_index(dy: f32, dx: f32) -> (usize, f32) {
    let len = (dy * dy + dx * dx).sqrt();
    let angle = if len != 0.0 {
        f64::from(dy / len).atan2(f64::from(dx / len)) as f32
    } else {
        0.0
    };
    let d = f64::from(angle) * 57.295_779_513_082_32 + 179.0;
    let d = if 0.0 <= d { d + 0.5 } else { d - 0.5 };
    let i = d.floor() as i32;
    let i = if i < 0x167 { i.max(0) } else { 0x167 };
    (i as usize, angle)
}

/// `0x656b10`: walks from `center` toward `end` one cell at a time; sand and solid cells absorb
/// energy (up to their `hp`) until the energy is gone or a cell is too durable to destroy.
fn cast_ray(
    world: &mut World,
    energy: i32,
    max_durability: i32,
    center: (f32, f32),
    end: (f32, f32),
) -> Ray {
    let dx = end.0 - center.0;
    let dy = end.1 - center.1;
    let len2 = dx * dx + dy * dy;
    let steps = (len2 + 0.5) as i32;
    let len = len2.sqrt();
    let (ux, uy) = if len > 0.0 {
        (dx / len, dy / len)
    } else {
        (0.0, 0.0)
    };
    let mut ray = Ray {
        len2: 0.0,
        absorb: 0,
    };
    if steps <= 0 {
        return ray;
    }
    let limit = steps.min(0x40_0000) as f32;
    let mut energy = energy;
    let mut cur = center;
    let mut prev = center;
    let mut count = 0;
    let dist2 =
        |p: (f32, f32)| (p.1 - center.1) * (p.1 - center.1) + (p.0 - center.0) * (p.0 - center.0);
    if limit <= (cur.0 - center.0) * (cur.0 - center.0) + (cur.1 - center.1) * (cur.1 - center.1) {
        ray.len2 = dist2(cur);
        return ray;
    }
    loop {
        count += 1;
        cur = (cur.0 + ux, cur.1 + uy);
        if count > 0x800 {
            ray.len2 = dist2(cur);
            return ray;
        }
        if cur.0 as i32 != prev.0 as i32 || cur.1 as i32 != prev.1 as i32 {
            prev = cur;
            let cell = world
                .cell(cur.0 as i32, cur.1 as i32)
                .copied()
                .filter(|c| c.material != AIR);
            if let Some(cell) = cell
                && let Some(props) = world.materials().get(cell.material)
                && (props.liquid_sand || props.cell_type == CellType::Solid)
            {
                let take = cell.hp.min(energy);
                ray.absorb = ray.absorb.max(take);
                energy -= take;
                if props.durability > max_durability || energy == 0 {
                    let bonus = if world.global_rng.rand() & 1 == 0 {
                        1.0
                    } else {
                        0.0
                    };
                    ray.len2 = dist2(cur) + bonus;
                    return ray;
                }
            }
        }
        let d = dist2(cur);
        if limit <= d {
            ray.len2 = d;
            return ray;
        }
    }
}

impl Particles {
    /// Frame start (`0x657300`) then the dispatcher (`0x657410`) for the explosions the world
    /// flushed: earlier deferred ones run first, then one new explosion runs at once and the
    /// rest wait for the next call unless their config has `never_cache`. The reference's
    /// delay ids and timers are not ported.
    pub fn run_explosions(&mut self, world: &mut World) -> Vec<ExplosionReport> {
        let mut reports = Vec::new();
        for q in std::mem::take(&mut self.deferred_explosions) {
            reports.push(self.explode(world, &q.config, q.pos));
        }
        self.explosions_this_frame = 0;
        for q in world.take_explosions() {
            if self.explosions_this_frame < 1 || q.config.never_cache {
                self.explosions_this_frame += 1;
                reports.push(self.explode(world, &q.config, q.pos));
            } else {
                self.deferred_explosions.push(q);
            }
        }
        reports
    }
    /// The cell effects of an explosion of `cfg` centered on `center` (`0x657960`).
    pub fn explode(
        &mut self,
        world: &mut World,
        cfg: &ExplosionConfig,
        center: (f32, f32),
    ) -> ExplosionReport {
        let mut report = ExplosionReport {
            destroyed: vec![0.0; world.materials().len()],
            ..ExplosionReport::default()
        };
        let radius = cfg.radius;
        if radius.is_nan() || radius <= 0.0 {
            return report;
        }
        let find = |name: &str| world.materials().find(name).filter(|&m| m != AIR);
        let res = Mats {
            create: find(&cfg.create_cell_material),
            spark: find(&cfg.spark_material),
            dirt: find("explosion_dirt"),
        };

        // One-time table (`0x12059f0`): noisy stain strength per degree.
        let table = match self.stain_table {
            Some(t) => t,
            None => {
                let mut t = [0.0_f32; RAY_COUNT];
                for (i, v) in t.iter_mut().enumerate() {
                    let s = f64::from(i as f32 / 90.0).sin() as f32;
                    *v = unit(world) * 0.5 + 0.0 + (s + 1.0) * 0.25;
                }
                self.stain_table = Some(t);
                t
            }
        };
        let stain_r = if cfg.stains_radius == 0.0 {
            radius * 0.4
        } else {
            cfg.stains_radius
        };
        let total = radius + stain_r;
        let stain_offset = (world.global_rng.rand() % 0xB5) as usize;

        let mut rays = Vec::with_capacity(RAY_COUNT);
        for i in 0..RAY_COUNT {
            let t = (i as f32 / 360.0) * 6.283_185_5;
            let c = f64::from(t).cos() as f32;
            let s = f64::from(t).sin() as f32;
            let neg = -radius;
            let end_y = center.1 + s * neg + c * 0.0;
            let end_x = center.0 + (c * neg - s * 0.0);
            rays.push(cast_ray(
                world,
                cfg.ray_energy,
                cfg.max_durability_to_destroy,
                center,
                (end_x, end_y),
            ));
        }

        let phase = unit(world) * 360.0 + 0.0;
        let big = 60.0 < radius;

        if cfg.hole_enabled && 0.01 < total && 0.01 < radius {
            world.global_rng.rand();
            self.carve(
                world,
                cfg,
                res,
                center,
                &rays,
                &table,
                stain_offset,
                phase,
                big,
                total,
                &mut report,
            );
        }
        if cfg.sparks_enabled {
            self.scatter_sparks(world, cfg, res, center, &rays, &mut report);
        }
        report
    }

    #[allow(clippy::too_many_arguments)]
    fn carve(
        &mut self,
        world: &mut World,
        cfg: &ExplosionConfig,
        res: Mats,
        center: (f32, f32),
        rays: &[Ray],
        table: &[f32; RAY_COUNT],
        stain_offset: usize,
        phase: f32,
        big: bool,
        total: f32,
        report: &mut ExplosionReport,
    ) {
        let radius = cfg.radius;
        let total2 = total * total;
        let radius2 = radius * radius;
        let reach = (total as i32).max(0);
        let inv = 1.0 / (total2 - radius2);
        let ext = total + 1.0;
        let (xmax, xmin, ymax, ymin) = (
            ext + center.0,
            center.0 - ext,
            ext + center.1,
            center.1 - ext,
        );
        let (w, h) = (world.width() as f32, world.height() as f32);
        if !(xmax < 0.0 || ymax < 0.0 || w < xmin || h < ymin) {
            let lo = (xmin.floor().max(0.0) as i32, ymin.floor().max(0.0) as i32);
            let hi = (
                (xmax.ceil() as i32).min(world.width() - 1),
                (ymax.ceil() as i32).min(world.height() - 1),
            );
            world.mark_area(lo, hi);
        }

        let (base_x, base_y) = (center.0 as i32, center.1 as i32);
        for row in -reach..=reach {
            for col in -reach..=reach {
                let oy = row as f32;
                let ox = col as f32;
                let (ax, ay) = (base_x + col, base_y + row);
                let len = (oy * oy + ox * ox).sqrt();
                let (idx, angle) = ray_index(oy, ox);
                let mut ray_len2 = rays[idx].len2;
                if big {
                    let t = ray_len2 * 0.35;
                    let s = f64::from(angle * 57.29578 * 360.0 + phase).sin() as f32;
                    let t = if 100.0 <= t { 100.0 } else { t };
                    ray_len2 -= s * s * t;
                }
                let r2 = (col * col + row * row) as f32;
                let inside = r2 <= ray_len2;
                let (dir_x, dir_y) = if len <= 0.0 {
                    (0.0, 0.0)
                } else {
                    (ox / len, oy / len)
                };
                let jx = unit(world) * 0.2 - 0.1;
                let jy = unit(world) * 0.2 - 0.1;
                let speed = unit(world) * 100.0 + 100.0;
                let vx = speed * (jx + dir_x);
                let vy = speed * (jy + dir_y);

                let occupied = world.cell(ax, ay).copied().filter(|c| c.material != AIR);
                let Some(cell) = occupied else {
                    if r2 <= total2
                        && let Some(create) = res.create
                        && world.global_rng.rand() % 101 < cfg.create_cell_probability
                        && inside
                    {
                        world.set_cell(ax, ay, create, true);
                    }
                    continue;
                };
                let Some(info) = info_of(world.materials(), cell.material) else {
                    continue;
                };
                let enters = cfg.destroy_non_platform_solid
                    || (info.solid && info.platform == 1)
                    || !info.solid;
                if !enters {
                    continue;
                }
                if radius2 <= r2 || !inside {
                    if (info.sand || info.solid) && cfg.stains_enabled && r2 <= total2 {
                        let t = (1.0 - (r2 - ray_len2) * inv).clamp(0.0, 1.0);
                        let mut k = idx + stain_offset;
                        if k > 0x167 {
                            k -= 0x168;
                        }
                        let k = k.min(0x167);
                        let strength = (unit(world) * 0.199_999_99 + 0.5) * table[k] * t * t;
                        if 0.0 < strength && info.stainable {
                            let color = darkened(cell.color, strength);
                            world.cell_mut(ax, ay).expect("in bounds").color = color;
                        }
                    }
                    continue;
                }
                self.hole_cell(
                    world,
                    cfg,
                    res,
                    report,
                    (ax, ay),
                    (col, row),
                    cell,
                    info,
                    rays[idx].absorb,
                    (vx, vy),
                );
            }
        }
    }

    /// The branch for a cell inside the hole and inside its ray.
    #[allow(clippy::too_many_arguments)]
    fn hole_cell(
        &mut self,
        world: &mut World,
        cfg: &ExplosionConfig,
        res: Mats,
        report: &mut ExplosionReport,
        (ax, ay): (i32, i32),
        (col, row): (i32, i32),
        cell: Cell,
        info: Info,
        absorb: i32,
        (vx, vy): (f32, f32),
    ) {
        if cfg.material_sparks_enabled
            && cfg.material_sparks_min_hp <= cell.hp
            && world.global_rng.rand() % 101 <= cfg.material_sparks_probability
            && let Some(spark) = res.spark
        {
            let (lo, hi) = cfg.material_sparks_count;
            let count = if cfg.material_sparks_scale_with_hp {
                let f = (cell.hp as f32 / 20.0) * cfg.material_sparks_min_hp as f32;
                let n = (world.global_rng.rand() % 3 - 1)
                    + (((hi - lo) as f32 * f * f) + lo as f32) as i32;
                n.min(hi).max(lo)
            } else {
                lo + world.global_rng.rand() % (hi - lo + 1)
            };
            for _ in 0..count {
                if cfg.material_sparks_real {
                    continue;
                }
                let (col4, row4) = (col as f32 * 4.0, row as f32 * 4.0);
                let a = unit(world) - 0.5;
                let (c, s) = (f64::from(a).cos() as f32, f64::from(a).sin() as f32);
                let speck = Speck {
                    material: spark,
                    pos: (ax as f32, ay as f32),
                    vel: (c * col4 - s * row4, s * col4 + c * row4),
                    color: None,
                    life: (5.0, 10.0),
                    allow_occupied: true,
                    in_grid: true,
                };
                if self.add_from_material(world, speck)
                    && let Some(p) = self.particles.last_mut()
                {
                    p.flags_a |= FLAG_UNKNOWN_08;
                    report.sparks += 1;
                }
            }
        }
        if info.durability > cfg.max_durability_to_destroy {
            return;
        }
        let hp = cell.hp - absorb;
        world.cell_mut(ax, ay).expect("in bounds").hp = hp;
        if hp >= 1 {
            return;
        }
        let liquid_class = info.liquid;
        if !liquid_class || info.sand || cfg.hole_destroy_liquid {
            if !cfg.hole_destroy_physics_dynamic {
                return;
            }
            report.destroyed[cell.material.0 as usize] += 1.0;
            let first = unit(world) * 0.7 - 0.35;
            let second = unit(world) * 0.7 - 0.35;
            let speck = Speck {
                material: cell.material,
                pos: (ax as f32, ay as f32),
                vel: (second * vx + vx, first * vy + vy),
                color: Some(cell.color),
                life: (5.0, 10.0),
                allow_occupied: true,
                in_grid: true,
            };
            if self.add_from_material(world, speck) {
                report.thrown += 1;
            }
            let replace = res
                .create
                .filter(|_| world.global_rng.rand() % 101 < cfg.create_cell_probability);
            match (res.create, replace) {
                (Some(_), Some(create)) => world.set_cell(ax, ay, create, true),
                _ => {
                    if info.sand
                        && world.global_rng.rand() % 101 < cfg.sand_to_dirt_percent
                        && let Some(dirt) = res.dirt
                    {
                        world.set_cell(ax, ay, dirt, true);
                        if let Some(c) = world.cell_mut(ax, ay) {
                            c.color = cell.color;
                        }
                    } else {
                        world.remove_cell(ax, ay);
                    }
                }
            }
        } else {
            let mut taken = false;
            if world.global_rng.rand() % 101 < cfg.create_cell_probability
                && let Some(create) = res.create
            {
                world.set_cell(ax, ay, create, true);
                taken = true;
            }
            let up = if vy > 0.0 { -vy } else { vy };
            let first = unit(world) * 0.7 - 0.35;
            let second = unit(world) * 0.7 - 0.35;
            let vel = (
                (second * vx) * 0.5 + vx * 0.5,
                (first * up) * 0.5 + up * 0.5,
            );
            if !taken && self.add_from_grid(world, (ax, ay), vel) {
                report.thrown += 1;
            }
        }
    }

    /// The `sparks_enabled` block: sparks scattered between the inner radius and the edge,
    /// only where the blast reached.
    fn scatter_sparks(
        &mut self,
        world: &mut World,
        cfg: &ExplosionConfig,
        res: Mats,
        center: (f32, f32),
        rays: &[Ray],
        report: &mut ExplosionReport,
    ) {
        let radius = cfg.radius;
        let (lo, hi) = cfg.sparks_count;
        let wanted = lo + world.global_rng.rand() % (hi - lo + 1);
        let cap = wanted * 5;
        let (mut left, mut tries) = (wanted, 0);
        while 0 < left {
            if cap <= tries {
                break;
            }
            tries += 1;
            let angle = unit(world) * 6.283_185_5 + 0.0;
            let inner = cfg.sparks_inner_radius_coeff * radius;
            let dist = unit(world) * (radius - inner) + inner;
            let (c, s) = (f64::from(angle).cos() as f32, f64::from(angle).sin() as f32);
            let dx = c * dist - s * 0.0;
            let dy = s * dist + c * 0.0;
            let len2 = dy * dy + dx * dx;
            let (idx, _) = ray_index(dy, dx);
            if len2 < rays[idx].len2 {
                if let Some(spark) = res.spark {
                    let speck = Speck {
                        material: spark,
                        pos: (center.0 + dx, center.1 + dy),
                        vel: (dx * 4.0, dy * 4.0),
                        color: None,
                        life: (5.0, 10.0),
                        allow_occupied: true,
                        in_grid: true,
                    };
                    if self.add_from_material(world, speck)
                        && let Some(p) = self.particles.last_mut()
                    {
                        p.flags_a |= FLAG_UNKNOWN_08;
                        p.size = 12.0;
                        report.sparks += 1;
                    }
                }
                left -= 1;
            }
        }
    }
}

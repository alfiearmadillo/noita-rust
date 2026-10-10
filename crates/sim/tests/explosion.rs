//! `ConfigExplosion` cell logic (`0x657960`).

use sim::particles::{ExplosionConfig, Particles};
use sim::{AIR, CellType, MaterialId, MaterialProps, Materials, World};

const SAND: MaterialId = MaterialId(1);
const ROCK: MaterialId = MaterialId(2);
const WATER: MaterialId = MaterialId(3);
const HARD: MaterialId = MaterialId(4);
const SAND_COLOR: u32 = 0xff80_8080;

fn materials() -> Materials {
    let air = MaterialProps::new("air", CellType::Unspecified);
    let sand = MaterialProps {
        liquid_sand: true,
        density: 6.0,
        color: SAND_COLOR,
        ..MaterialProps::new("sand", CellType::Liquid)
    };
    let rock = MaterialProps {
        density: 10.0,
        durability: 12,
        color: 0xff40_4040,
        ..MaterialProps::new("rock", CellType::Solid)
    };
    let water = MaterialProps {
        density: 4.0,
        color: 0xff00_00ff,
        ..MaterialProps::new("water", CellType::Liquid)
    };
    let hard = MaterialProps {
        density: 10.0,
        durability: 5,
        hp: 50_000,
        color: 0xff20_2020,
        ..MaterialProps::new("hard", CellType::Solid)
    };
    let fire = MaterialProps::new("fire", CellType::Fire);
    let spark = MaterialProps::new("spark", CellType::Fire);
    Materials::new(vec![air, sand, rock, water, hard, fire, spark])
}

fn filled(material: MaterialId) -> World {
    let mut w = World::new(1, 1, 128, materials(), 21);
    for x in 0..w.width() {
        for y in 0..w.height() {
            w.set(x, y, material).unwrap();
        }
    }
    w
}

fn config(radius: f32) -> ExplosionConfig {
    ExplosionConfig {
        radius,
        ..ExplosionConfig::default()
    }
}

#[test]
fn the_defaults_match_the_constructor() {
    let cfg = ExplosionConfig::default();
    assert_eq!(cfg.create_cell_material, "fire");
    assert_eq!(cfg.spark_material, "spark");
    assert_eq!(
        (cfg.ray_energy, cfg.max_durability_to_destroy),
        (20_000, 10)
    );
    assert_eq!(cfg.cell_explosion_radius, (5.0, 150.0));
}

#[test]
fn attributes_override_the_defaults() {
    let attrs = [
        ("ray_energy", "50000"),
        ("cell_explosion_power", "30"),
        ("create_cell_material", "steam"),
        ("sparks_enabled", "0"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    let mut cfg = ExplosionConfig::default();
    cfg.apply(&attrs);
    assert_eq!((cfg.ray_energy, cfg.cell_explosion_power), (50_000, 30.0));
    assert_eq!(cfg.create_cell_material, "steam");
    assert!(!cfg.sparks_enabled);
}

#[test]
fn a_blast_in_sand_digs_a_round_hole_and_stains_the_rim() {
    let mut w = filled(SAND);
    let mut ps = Particles::new(4);
    let cfg = config(10.0);
    let report = ps.explode(&mut w, &cfg, (64.5, 64.5));

    for dy in -9..=9_i32 {
        for dx in -9..=9_i32 {
            if dx * dx + dy * dy <= 81 {
                assert_ne!(
                    w.get(64 + dx, 64 + dy),
                    Some(SAND),
                    "({dx},{dy}) was not dug"
                );
            }
        }
    }
    assert!(report.destroyed[SAND.0 as usize] > 250.0);
    assert!(report.thrown > 0 && !ps.is_empty(), "debris was thrown");

    assert_eq!(w.get(64 + 20, 64).unwrap(), SAND);
    assert_eq!(w.cell(64 + 20, 64).unwrap().color, SAND_COLOR);
    let stained = (11..=13).any(|d| {
        w.cell(64 + d, 64)
            .is_some_and(|c| (c.color >> 16) & 0xFF < 0x80)
    });
    assert!(stained, "sand just outside the hole is darkened");
}

#[test]
fn a_durable_wall_shadows_what_is_behind_it() {
    let mut w = filled(SAND);
    for y in 0..w.height() {
        for x in 45..47 {
            w.set(x, y, ROCK).unwrap();
        }
    }
    let mut ps = Particles::new(8);
    let cfg = config(20.0);
    ps.explode(&mut w, &cfg, (40.5, 64.5));
    assert_ne!(w.get(30, 64), Some(SAND), "open side is dug");
    assert_eq!(
        w.get(45, 64),
        Some(ROCK),
        "the wall is too durable to destroy"
    );
    assert_eq!(w.get(52, 64), Some(SAND), "the far side is shielded");
}

#[test]
fn tough_cells_lose_hp_instead_of_breaking() {
    let mut w = filled(HARD);
    let mut ps = Particles::new(2);
    let cfg = config(6.0);
    ps.explode(&mut w, &cfg, (64.5, 64.5));
    let worn = [(64, 64), (65, 64), (63, 64), (64, 65), (64, 63)]
        .iter()
        .any(|&(x, y)| w.cell(x, y).is_some_and(|c| c.hp == 30_000));
    assert!(
        worn,
        "the ray's whole energy was taken from the first cells"
    );
    assert_eq!(w.get(64, 64), Some(HARD));
}

#[test]
fn water_is_not_hurt_by_rays_that_never_touch_sand() {
    let mut w = filled(WATER);
    let mut ps = Particles::new(6);
    let cfg = config(12.0);
    let before = w.count(WATER);
    ps.explode(&mut w, &cfg, (64.5, 64.5));
    assert!(
        w.count(WATER) >= before - 40,
        "only the fire roll replaces cells"
    );
    assert_eq!(w.get(64 + 3, 64).map(|m| m != AIR), Some(true));
}

#[test]
fn the_same_seed_gives_the_same_blast() {
    let run = || {
        let mut w = filled(SAND);
        let mut ps = Particles::new(9);
        let cfg = config(14.0);
        ps.explode(&mut w, &cfg, (70.0, 50.0));
        (w.state_hash(), ps.len())
    };
    assert_eq!(run(), run());
}

#[test]
fn the_hole_can_be_disabled() {
    let mut w = filled(SAND);
    let mut ps = Particles::new(1);
    let cfg = ExplosionConfig {
        hole_enabled: false,
        ..config(10.0)
    };
    let before = w.count(SAND);
    ps.explode(&mut w, &cfg, (64.5, 64.5));
    assert_eq!(w.count(SAND), before);
}

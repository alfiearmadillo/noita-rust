//! Explosions started by burning materials (burn hook `0x70fb10`) and by reactions (`0x70ebf0`).

use std::collections::BTreeMap;
use std::sync::Arc;

use sim::particles::{ExplosionConfig, Particles};
use sim::{CellType, MaterialId, MaterialProps, Materials, ReactionProps, World};

const SAND: MaterialId = MaterialId(1);
const POWDER: MaterialId = MaterialId(2);
const A: MaterialId = MaterialId(5);
const B: MaterialId = MaterialId(6);

fn base_materials() -> Vec<MaterialProps> {
    let air = MaterialProps::new("air", CellType::Unspecified);
    let sand = MaterialProps {
        liquid_sand: true,
        density: 6.0,
        color: 0xff80_8080,
        ..MaterialProps::new("sand", CellType::Liquid)
    };
    let config = ExplosionConfig {
        cell_explosion_power: 40.0,
        cell_explosion_damage_required: 0.0,
        ..ExplosionConfig::default()
    };
    let powder = MaterialProps {
        liquid_sand: true,
        density: 6.0,
        on_fire: true,
        requires_oxygen: false,
        generates_flames: 0,
        generates_smoke: 0,
        color: 0xff00_ffff,
        explosion: Some(Arc::new(config)),
        ..MaterialProps::new("powder", CellType::Liquid)
    };
    vec![
        air,
        sand,
        powder,
        MaterialProps::new("fire", CellType::Fire),
        MaterialProps::new("spark", CellType::Fire),
        MaterialProps {
            liquid_static: true,
            ..MaterialProps::new("a", CellType::Liquid)
        },
        MaterialProps {
            liquid_static: true,
            ..MaterialProps::new("b", CellType::Liquid)
        },
    ]
}

fn run_active(w: &mut World, frames: usize) {
    for _ in 0..frames {
        w.mark_area((0, 0), (w.width() - 1, w.height() - 1));
        w.step();
    }
}

#[test]
fn a_burning_explosive_material_queues_a_blast_sized_by_its_power() {
    let mut w = World::new(1, 1, 128, Materials::new(base_materials()), 11);
    for x in 0..w.width() {
        for y in 0..w.height() {
            w.set(x, y, SAND).unwrap();
        }
    }
    w.set(64, 64, POWDER).unwrap();
    run_active(&mut w, 4);
    let queued = w.take_explosions();
    assert_eq!(queued.len(), 1, "one explosion per flush");
    let blast = &queued[0];
    assert!(!blast.config.never_cache);
    assert_eq!(blast.pos, (64.0, 64.0));
    let r = blast.config.radius;
    assert!(
        (40.0_f32 * 0.75).sqrt() <= r && r <= (40.0_f32 * 1.25).sqrt(),
        "radius {r}"
    );
}

#[test]
fn queued_blasts_run_one_per_frame_and_dig() {
    let mut w = World::new(1, 1, 128, Materials::new(base_materials()), 3);
    for x in 0..w.width() {
        for y in 0..w.height() {
            w.set(x, y, SAND).unwrap();
        }
    }
    w.set(64, 64, POWDER).unwrap();
    run_active(&mut w, 4);
    let mut ps = Particles::new(2);
    let reports = ps.run_explosions(&mut w);
    assert_eq!(reports.len(), 1);
    assert!(reports[0].destroyed[SAND.0 as usize] > 20.0);
    assert_ne!(
        w.get(64, 66),
        Some(SAND),
        "the hole is dug around the powder"
    );
    assert!(
        ps.run_explosions(&mut w).is_empty(),
        "nothing more is queued"
    );
}

#[test]
fn a_reaction_can_carry_an_explosion() {
    let attrs = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|&(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    };
    let reaction = ReactionProps {
        attrs: attrs(&[
            ("probability", "100"),
            ("input_cell1", "a"),
            ("input_cell2", "b"),
            ("output_cell1", "sand"),
            ("output_cell2", "sand"),
        ]),
        explosion_config: Some(attrs(&[
            ("cell_explosion_probability", "1"),
            ("cell_explosion_radius_min", "6"),
            ("cell_explosion_radius_max", "8"),
        ])),
    };
    let materials = Materials::new(base_materials()).with_reactions(vec![reaction]);
    let mut w = World::new(1, 1, 64, materials, 5);
    w.set(32, 32, A).unwrap();
    w.set(33, 32, B).unwrap();
    w.set(31, 32, B).unwrap();
    w.set(32, 31, B).unwrap();
    w.set(32, 33, B).unwrap();
    run_active(&mut w, 30);
    let queued = w.take_explosions();
    assert!(!queued.is_empty(), "the reaction fired an explosion");
    let r = queued[0].config.radius;
    assert!((6.0..=8.0).contains(&r), "radius {r}");
    assert_eq!(queued[0].pos.0 as i32, 32);
}

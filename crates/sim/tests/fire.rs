use sim::{CellType, MaterialId, MaterialProps, Materials, World};

#[test]
fn construction_initializes_burning_and_intensity_from_material_data() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        MaterialProps {
            temperature_of_fire: 50,
            ..MaterialProps::new("wood_player", CellType::Liquid)
        },
        MaterialProps {
            on_fire: true,
            temperature_of_fire: 100,
            ..MaterialProps::new("fire", CellType::Fire)
        },
        MaterialProps {
            temperature_of_fire: 0,
            ..MaterialProps::new("spark", CellType::Fire)
        },
    ]);
    let mut world = World::new(1, 1, 64, materials, 7);

    world.set(10, 10, MaterialId(1)).unwrap();
    let wood = world.cell(10, 10).unwrap();
    assert!(!wood.burning);
    assert_eq!(wood.fire_intensity, 50);

    world.set(20, 10, MaterialId(2)).unwrap();
    let fire = world.cell(20, 10).unwrap();
    assert!(fire.burning);
    assert_eq!(fire.fire_intensity, 100);

    world.set(30, 10, MaterialId(3)).unwrap();
    let spark = world.cell(30, 10).unwrap();
    assert!(
        spark.burning,
        "CFireCell construction forces its active flag"
    );
    assert_eq!(spark.fire_intensity, 0);
}

#[test]
fn oxygen_extinguishes_active_gas_without_flame_or_smoke_outputs() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        MaterialProps {
            on_fire: true,
            temperature_of_fire: 2,
            requires_oxygen: true,
            generates_flames: 0,
            gas_speed: 255,
            gas_upwards_speed: 0,
            gas_horizontal_speed: 0,
            gas_downwards_speed: 0,
            ..MaterialProps::new("test_burning_gas", CellType::Gas)
        },
    ]);
    let mut world = World::new(1, 1, 64, materials, 11);
    world.set(32, 32, MaterialId(1)).unwrap();

    world.step();
    let gas = world.cell(32, 32).unwrap();
    assert!(gas.burning);
    assert_eq!(gas.fire_intensity, 1);

    world.step();
    let gas = world.cell(32, 32).unwrap();
    assert!(!gas.burning);
    assert_eq!(gas.fire_intensity, 0);
}

#[test]
fn oxygen_extinguishes_active_liquid_after_movement() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        MaterialProps::new("rock", CellType::Solid),
        MaterialProps {
            on_fire: true,
            temperature_of_fire: 2,
            requires_oxygen: true,
            generates_flames: 0,
            liquid_viscosity: 100,
            liquid_flow_speed: 0.0,
            ..MaterialProps::new("test_burning_liquid", CellType::Liquid)
        },
    ]);
    let mut world = World::new(1, 1, 64, materials, 13);
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx != 0 || dy != 0 {
                world.set(32 + dx, 32 + dy, MaterialId(1)).unwrap();
            }
        }
    }
    world.set(32, 32, MaterialId(2)).unwrap();

    world.step();
    let liquid = world.cell(32, 32).unwrap();
    assert!(liquid.burning);
    assert_eq!(liquid.fire_intensity, 1);

    world.step();
    let liquid = world.cell(32, 32).unwrap();
    assert!(!liquid.burning);
    assert_eq!(liquid.fire_intensity, 0);
}

#[test]
fn oxygen_extinguishes_active_static_liquid_when_still_attached() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        MaterialProps {
            on_fire: true,
            temperature_of_fire: 2,
            requires_oxygen: true,
            generates_flames: 0,
            liquid_static: true,
            ..MaterialProps::new("test_static_burning_liquid", CellType::Liquid)
        },
    ]);
    let mut world = World::new(1, 1, 64, materials, 17);
    world.set(32, 32, MaterialId(1)).unwrap();

    world.step();
    let liquid = world.cell(32, 32).unwrap();
    assert!(liquid.is_static);
    assert!(liquid.burning);
    assert_eq!(liquid.fire_intensity, 1);

    world.step();
    let liquid = world.cell(32, 32).unwrap();
    assert!(liquid.is_static);
    assert!(!liquid.burning);
    assert_eq!(liquid.fire_intensity, 0);
}

#[test]
fn burning_static_liquid_with_zero_fire_hp_converts_after_the_update() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        MaterialProps {
            on_fire: true,
            fire_hp: 0,
            liquid_static: true,
            generates_flames: 0,
            on_fire_flame_material: "burnt_solid".to_owned(),
            ..MaterialProps::new("burning_liquid", CellType::Liquid)
        },
        MaterialProps::new("burnt_solid", CellType::Solid),
        MaterialProps {
            on_fire: true,
            fire_hp: -1,
            liquid_static: true,
            on_fire_flame_material: "burnt_solid".to_owned(),
            ..MaterialProps::new("unbreakable_burning_liquid", CellType::Liquid)
        },
        MaterialProps {
            fire_hp: 0,
            liquid_static: true,
            on_fire_flame_material: "burnt_solid".to_owned(),
            ..MaterialProps::new("unlit_liquid", CellType::Liquid)
        },
    ]);
    let mut world = World::new(1, 1, 64, materials, 19);
    world.set(32, 32, MaterialId(1)).unwrap();
    world.set(40, 32, MaterialId(3)).unwrap();
    world.set(48, 32, MaterialId(4)).unwrap();

    world.step();

    let converted = world.cell(32, 32).unwrap();
    assert_eq!(converted.material, MaterialId(2));
    assert!(!converted.is_static);
    assert!(!converted.burning);
    assert_eq!(world.cell(40, 32).unwrap().material, MaterialId(3));
    assert_eq!(
        world.cell(48, 32).unwrap().material,
        MaterialId(4),
        "only burning static cells reach the fire-HP rolls"
    );
}

#[test]
fn burning_moving_liquid_with_zero_fire_hp_converts_where_it_lands() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        MaterialProps {
            on_fire: true,
            fire_hp: 0,
            generates_flames: 0,
            on_fire_flame_material: "burnt_solid".to_owned(),
            ..MaterialProps::new("burning_liquid", CellType::Liquid)
        },
        MaterialProps::new("burnt_solid", CellType::Solid),
    ]);
    let mut world = World::new(1, 1, 64, materials, 29);
    world.set(32, 10, MaterialId(1)).unwrap();

    world.step();

    let burnt: Vec<_> = (0..64)
        .flat_map(|y| (0..64).map(move |x| (x, y)))
        .filter(|&(x, y)| world.get(x, y) == Some(MaterialId(2)))
        .collect();
    assert_eq!(burnt.len(), 1);
    assert_ne!(burnt[0], (32, 10), "the cell moved before converting");
    assert!((0..64).all(|y| (0..64).all(|x| world.get(x, y) != Some(MaterialId(1)))));
}

#[test]
fn burning_static_liquid_burns_out_faster_next_to_empty_space() {
    let burning = |fire_hp| MaterialProps {
        on_fire: true,
        fire_hp,
        liquid_static: true,
        generates_flames: 0,
        on_fire_flame_material: "burnt_solid".to_owned(),
        ..MaterialProps::new("burning_liquid", CellType::Liquid)
    };
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        burning(29),
        MaterialProps::new("burnt_solid", CellType::Solid),
        burning(30_000),
        MaterialProps::new("rock", CellType::Solid),
    ]);
    let mut world = World::new(1, 1, 64, materials, 23);
    // Below 30 HP the second roll always passes, so an exposed cell converts at once.
    world.set(16, 16, MaterialId(1)).unwrap();
    // Enclosed, only the first roll (about 1 in 30001) can convert.
    world.set(40, 40, MaterialId(3)).unwrap();
    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        world.set(40 + dx, 40 + dy, MaterialId(4)).unwrap();
    }

    world.step();

    assert_eq!(world.cell(16, 16).unwrap().material, MaterialId(2));
    assert_eq!(world.cell(40, 40).unwrap().material, MaterialId(3));
}

fn fire() -> MaterialProps {
    MaterialProps {
        on_fire: true,
        temperature_of_fire: 100,
        ..MaterialProps::new("fire", CellType::Fire)
    }
}

const NEIGHBORS: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

#[test]
fn darkened_scales_channels_with_a_floor_and_skips_dark_colors() {
    assert_eq!(sim::darkened(0xFFC8_6410, 0.5), 0xFF64_3213);
    assert_eq!(sim::darkened(0x8010_1010, 0.5), 0x8010_1010);
}

#[test]
fn intense_fire_ignites_an_enclosing_burnable_liquid() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        fire(),
        MaterialProps {
            burnable: true,
            autoignition_temperature: 0,
            fire_hp: -1,
            liquid_static: true,
            ..MaterialProps::new("wood", CellType::Liquid)
        },
    ]);
    let mut world = World::new(1, 1, 64, materials, 31);
    world.set(32, 32, MaterialId(1)).unwrap();
    for (dx, dy) in NEIGHBORS {
        world.set(32 + dx, 32 + dy, MaterialId(2)).unwrap();
    }

    world.step();

    let lit = NEIGHBORS
        .iter()
        .filter(|&&(dx, dy)| world.cell(32 + dx, 32 + dy).unwrap().burning)
        .count();
    assert!(lit >= 1, "intensity 100 spreads on both attempts");
}

#[test]
fn burnable_gas_touched_by_fire_becomes_its_flame_material() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        fire(),
        MaterialProps {
            burnable: true,
            liquid_static: true,
            ..MaterialProps::new("flammable_gas", CellType::Gas)
        },
    ]);
    let mut world = World::new(1, 1, 64, materials, 37);
    world.set(32, 32, MaterialId(1)).unwrap();
    for (dx, dy) in NEIGHBORS {
        world.set(32 + dx, 32 + dy, MaterialId(2)).unwrap();
    }

    world.step();

    let converted = NEIGHBORS
        .iter()
        .filter(|&&(dx, dy)| world.get(32 + dx, 32 + dy) == Some(MaterialId(1)))
        .count();
    assert!(converted >= 1);
}

#[test]
fn fire_darkens_sand_like_neighbors() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        fire(),
        MaterialProps {
            liquid_sand: true,
            liquid_static: true,
            color: 0xFFC8_C8C8,
            ..MaterialProps::new("rock_static", CellType::Liquid)
        },
    ]);
    let mut world = World::new(1, 1, 64, materials, 41);
    world.set(32, 32, MaterialId(1)).unwrap();
    for (dx, dy) in NEIGHBORS {
        world.set(32 + dx, 32 + dy, MaterialId(2)).unwrap();
    }

    world.step();

    assert!(
        NEIGHBORS
            .iter()
            .any(|&(dx, dy)| world.cell(32 + dx, 32 + dy).unwrap().color != 0xFFC8_C8C8)
    );
}

#[test]
fn free_fire_never_falls_and_burns_out() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        fire(),
    ]);
    let mut world = World::new(1, 1, 64, materials, 43);
    world.set(32, 60, MaterialId(1)).unwrap();
    for _ in 0..300 {
        world.step();
        assert!((61..64).all(|y| (0..64).all(|x| world.get(x, y) == Some(sim::AIR))));
    }
    assert_eq!(world.count(MaterialId(1)), 0);
}

#[test]
fn burning_gas_emits_flames_above_and_keeps_its_intensity() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        fire(),
        MaterialProps {
            on_fire: true,
            temperature_of_fire: 50,
            generates_flames: 100,
            requires_oxygen: true,
            gas_speed: 255,
            gas_upwards_speed: 0,
            gas_horizontal_speed: 0,
            gas_downwards_speed: 0,
            on_fire_flame_material: "fire".to_owned(),
            ..MaterialProps::new("burning_gas", CellType::Gas)
        },
    ]);
    let mut world = World::new(1, 1, 64, materials, 47);
    world.set(32, 32, MaterialId(2)).unwrap();

    for _ in 0..5 {
        world.step();
    }

    let gas = world.cell(32, 32).unwrap();
    assert!(gas.burning, "emissions refill the intensity");
    assert!(
        gas.fire_intensity >= 45,
        "only blocked frames lose intensity"
    );
    assert!(
        world.count(MaterialId(1)) >= 1,
        "flames appear above the gas"
    );
}

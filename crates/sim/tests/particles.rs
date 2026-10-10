//! `grid::GridParticles`: flying cells and their landing.

use sim::particles::{FLAG_BOUNCE, ParticleRng, Particles, Speck};
use sim::{AIR, CellType, MaterialId, MaterialProps, Materials, World};

const SAND: MaterialId = MaterialId(1);
const ROCK: MaterialId = MaterialId(2);
const WATER: MaterialId = MaterialId(3);

fn world() -> World {
    let air = MaterialProps::new("air", CellType::Unspecified);
    let sand = MaterialProps {
        liquid_sand: true,
        density: 6.0,
        color: 0xff80_8080,
        ..MaterialProps::new("sand", CellType::Liquid)
    };
    let rock = MaterialProps {
        density: 10.0,
        color: 0xff40_4040,
        ..MaterialProps::new("rock", CellType::Solid)
    };
    let water = MaterialProps {
        density: 4.0,
        color: 0xff00_00ff,
        ..MaterialProps::new("water", CellType::Liquid)
    };
    let mut w = World::new(1, 1, 128, Materials::new(vec![air, sand, rock, water]), 7);
    for x in 0..w.width() {
        for y in 100..w.height() {
            w.set(x, y, ROCK).unwrap();
        }
    }
    w
}

fn take(w: &mut World, x: i32, y: i32) -> sim::Cell {
    let cell = *w.cell(x, y).unwrap();
    w.set(x, y, AIR).unwrap();
    cell
}

#[test]
fn rng_is_the_minimal_standard_generator() {
    let mut rng = ParticleRng::new(0);
    let mut seen = Vec::new();
    for _ in 0..4 {
        rng.float(0.0, 1.0);
        seen.push(rng.state());
    }
    assert_eq!(seen, [16807, 282_475_249, 1_622_650_073, 984_943_658]);
}

#[test]
fn int_and_float_draws_stay_in_range() {
    let mut rng = ParticleRng::new(99);
    for _ in 0..1000 {
        let i = rng.int(0, 100);
        assert!((0..=100).contains(&i));
        let f = rng.float(-10.0, 10.0);
        assert!((-10.0..=10.0).contains(&f));
    }
}

#[test]
fn a_free_particle_accelerates_with_gravity() {
    let mut w = world();
    let mut ps = Particles::new(1);
    let speck = Speck {
        material: SAND,
        pos: (50.0, 10.0),
        vel: (0.0, 0.0),
        color: None,
        life: (5.0, 10.0),
        allow_occupied: true,
        in_grid: false,
    };
    assert!(ps.add_from_material(&w, speck));
    for _ in 0..5 {
        ps.update(&mut w, 0.1, 0.0);
    }
    let p = &ps.particles[0];
    assert!((p.vel.1 - 5.0).abs() < 1e-4, "vy = {}", p.vel.1);
    assert!(p.pos.1 > 11.0, "y = {}", p.pos.1);
    assert_eq!(p.pos.0, 50.5);
}

#[test]
fn a_falling_cell_lands_on_the_floor() {
    let mut w = world();
    w.set(50, 20, SAND).unwrap();
    let cell = take(&mut w, 50, 20);
    let mut ps = Particles::new(5);
    assert!(ps.add_cell(&w, cell, (50, 20), (0.0, 0.0), false, false));
    ps.particles[0].flags_a &= !FLAG_BOUNCE;
    for _ in 0..200 {
        ps.update(&mut w, 0.1, 0.0);
        if ps.is_empty() && ps.waiting() == 0 {
            break;
        }
    }
    assert!(ps.is_empty());
    assert_eq!(ps.waiting(), 0);
    assert_eq!(w.count(SAND), 1);
    assert_eq!(w.get(50, 99), Some(SAND), "rests on the rock at y = 100");
}

#[test]
fn a_blocked_landing_climbs_to_the_first_free_row() {
    let mut w = world();
    for x in 30..70 {
        for y in 90..100 {
            w.set(x, y, SAND).unwrap();
        }
    }
    let before = w.count(SAND);
    w.set(50, 5, SAND).unwrap();
    let cell = take(&mut w, 50, 5);
    let mut ps = Particles::new(11);
    assert!(ps.add_cell(&w, cell, (50, 95), (0.0, 0.0), false, false));
    ps.particles[0].mode = 2;
    ps.particles[0].vel = (0.0, 0.0);
    ps.particles[0].accel = (0.0, 0.0);
    ps.particles[0].flags_b = 0x81;
    ps.particles[0].lifetime = 0.0;
    ps.particles[0].flags_b |= sim::particles::FLAG_LIFETIME;
    ps.update(&mut w, 0.1, 0.0);
    assert!(ps.is_empty());
    assert_eq!(w.count(SAND), before + 1);
    let landed = (30..70).find(|&x| w.get(x, 89) == Some(SAND));
    assert!(landed.is_some(), "lands on row 89, above the pile");
}

#[test]
fn landing_in_rock_waits_in_the_queue() {
    let mut w = world();
    w.set(50, 5, SAND).unwrap();
    let cell = take(&mut w, 50, 5);
    let mut ps = Particles::new(3);
    assert!(ps.add_cell(&w, cell, (50, 105), (0.0, 0.0), false, false));
    ps.particles[0].mode = 2;
    ps.particles[0].flags_b = 0x81 | sim::particles::FLAG_LIFETIME;
    ps.particles[0].lifetime = 0.0;
    ps.update(&mut w, 0.1, 0.0);
    assert!(ps.is_empty());
    assert_eq!(ps.waiting(), 1, "a solid cell stops the upward scan");
    assert_eq!(w.count(SAND), 0);
}

#[test]
fn a_cell_in_water_displaces_it() {
    let mut w = world();
    for x in 40..60 {
        for y in 90..100 {
            w.set(x, y, WATER).unwrap();
        }
    }
    w.set(50, 5, SAND).unwrap();
    let cell = take(&mut w, 50, 5);
    let mut ps = Particles::new(2);
    assert!(ps.add_cell(&w, cell, (50, 95), (0.0, 0.0), false, false));
    ps.particles[0].mode = 2;
    ps.particles[0].flags_b = 0x81 | sim::particles::FLAG_LIFETIME;
    ps.particles[0].lifetime = 0.0;
    ps.update(&mut w, 0.1, 0.0);
    assert_eq!(w.get(50, 95), Some(SAND));
    assert_eq!(
        w.count(WATER),
        199,
        "the displaced water is queued, not lost"
    );
    assert_eq!(ps.waiting(), 1);
}

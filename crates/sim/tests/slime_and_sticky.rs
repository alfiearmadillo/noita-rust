//! Slime (0x70b7c0) and `liquid_sticks_to_ceiling` (0x70b2d0, 0x70c6e0) rules. Material values
//! from the reference `slime` and `snow_sticky`.

mod common;

use common::{ROCK, run, run_active};
use sim::{AIR, CellType, MaterialId, MaterialProps, Materials, World};

const SLIME: MaterialId = MaterialId(4);
const STICKY: MaterialId = MaterialId(5);

fn world() -> World {
    let mut props: Vec<MaterialProps> = (0..4)
        .map(|i| common::materials().get(MaterialId(i)).unwrap().clone())
        .collect();
    props.push(MaterialProps {
        density: 5.0,
        liquid_gravity: 0.1,
        liquid_sticks_to_ceiling: 40,
        liquid_slime: true,
        ..MaterialProps::new("slime", CellType::Liquid)
    });
    props.push(MaterialProps {
        density: 8.0,
        liquid_gravity: 0.6,
        liquid_sand: true,
        liquid_sticks_to_ceiling: 100,
        liquid_viscosity: 20,
        ..MaterialProps::new("snow_sticky", CellType::Liquid)
    });
    let mut w = World::new(2, 2, 32, Materials::new(props), 7);
    for x in 0..w.width() {
        w.set(x, w.height() - 1, ROCK).unwrap();
    }
    w
}

fn floor(w: &World) -> i32 {
    w.height() - 2
}

#[test]
fn slime_falls_to_the_floor() {
    let mut w = world();
    w.set(20, 5, SLIME).unwrap();
    run_active(&mut w, 400);
    // Falling slime moves diagonally and re-rolls its direction, so the column is RNG-dependent.
    let y = floor(&w);
    assert!((0..w.width()).any(|x| w.get(x, y) == Some(SLIME)));
    assert_eq!(w.count(SLIME), 1);
}

#[test]
fn lone_slime_does_not_spread_with_both_sides_open() {
    let mut w = world();
    let y = floor(&w);
    w.set(20, y, SLIME).unwrap();
    run_active(&mut w, 200);
    assert_eq!(w.get(20, y), Some(SLIME), "water would wander; slime stays");
}

#[test]
fn slime_creeps_away_from_an_occupied_side() {
    let mut w = world();
    let y = floor(&w);
    w.set(19, y, ROCK).unwrap();
    w.set(20, y, SLIME).unwrap();
    // Each update either stops on the viscosity roll or adds 0.9 to flow; it steps at flow >= 1.
    run_active(&mut w, 40);
    assert_eq!(w.get(20, y), Some(AIR), "moved off its start");
    assert_eq!(w.count(SLIME), 1);
    assert!(
        (21..w.width()).any(|x| w.get(x, y) == Some(SLIME)),
        "crept right, away from the rock"
    );
}

#[test]
fn slime_crawls_down_along_a_wall() {
    let mut w = world();
    for y in 10..=floor(&w) {
        w.set(30, y, ROCK).unwrap();
    }
    // Resting on a ledge at the wall: the occupied side lets it slide diagonally down.
    w.set(29, 10, SLIME).unwrap();
    w.set(28, 11, ROCK).unwrap();
    run_active(&mut w, 400);
    let y = floor(&w);
    assert_eq!(w.count(SLIME), 1);
    assert!(
        (0..w.width()).any(|x| w.get(x, y) == Some(SLIME)),
        "reached the floor"
    );
}

#[test]
fn sticky_sand_hangs_from_a_ceiling() {
    let mut w = world();
    for x in 0..w.width() {
        w.set(x, 10, ROCK).unwrap();
        w.set(x, 11, STICKY).unwrap();
    }
    run_active(&mut w, 60);
    let hanging = (0..w.width())
        .filter(|&x| w.get(x, 11) == Some(STICKY))
        .count();
    assert!(
        hanging > w.width() as usize / 2,
        "only {hanging} stuck to the ceiling"
    );
    // With an empty cell below, a stuck cell comes loose (chance sticks/101 per check) and sticks
    // again on its next update, so a hanging cell is static or still in a stuck state.
    assert!(
        (0..w.width()).all(|x| w.get(x, 11) != Some(STICKY) || {
            let c = w.cell(x, 11).unwrap();
            c.is_static || (5..=7).contains(&c.state)
        }),
        "hanging cells are stuck"
    );
}

#[test]
fn sticky_sand_falls_once_the_ceiling_is_gone() {
    let mut w = world();
    for x in 0..w.width() {
        w.set(x, 10, ROCK).unwrap();
        w.set(x, 11, STICKY).unwrap();
    }
    run_active(&mut w, 60);
    for x in 0..w.width() {
        w.set(x, 10, AIR).unwrap();
    }
    run_active(&mut w, 1500);
    let hanging = (0..w.width())
        .filter(|&x| w.get(x, 11) == Some(STICKY))
        .count();
    assert_eq!(hanging, 0, "loose cells fall on full-frame checks");
    // A grain landing on a cell that is still falling and not yet updated this frame (e.g. one that
    // re-stuck mid-fall, keeping its `vel`) is deleted (0x70b5e8).
    assert!(w.count(STICKY) <= w.width() as usize);
}

#[test]
fn unsupported_sticky_sand_just_falls() {
    let mut w = world();
    w.set(20, 5, STICKY).unwrap();
    run(&mut w, 300);
    assert_eq!(
        common::row_count(&w, floor(&w), STICKY),
        1,
        "landed on the floor"
    );
}

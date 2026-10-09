//! Gas cells (`CGasCell` 0x70a4a0, 0x70a670, 0x70a810). Material values from the reference
//! `steam` (lifetime 1000, density 1) with the `CellData` gas speed defaults.

mod common;

use common::{ROCK, WATER, run};
use sim::{AIR, CellType, MaterialId, MaterialProps, Materials, World};

const STEAM: MaterialId = MaterialId(4);
const HEAVY: MaterialId = MaterialId(5);

fn world(steam_lifetime: i32) -> World {
    let mut props: Vec<MaterialProps> = (0..4)
        .map(|i| common::materials().get(MaterialId(i)).unwrap().clone())
        .collect();
    props.push(MaterialProps {
        density: 1.0,
        lifetime: steam_lifetime,
        ..MaterialProps::new("steam", CellType::Gas)
    });
    props.push(MaterialProps {
        density: 3.0,
        ..MaterialProps::new("heavy_gas", CellType::Gas)
    });
    let mut w = World::new(1, 1, 64, Materials::new(props), 3);
    for i in 0..64 {
        for (x, y) in [(i, 0), (i, 63), (0, i), (63, i)] {
            w.set(x, y, ROCK).unwrap();
        }
    }
    w
}

fn count_in_rows(w: &World, rows: std::ops::Range<i32>, id: MaterialId) -> usize {
    rows.flat_map(|y| (0..w.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| w.get(x, y) == Some(id))
        .count()
}

#[test]
fn gas_rises_and_spreads_under_the_ceiling() {
    let mut w = world(0);
    for x in 28..36 {
        w.set(x, 60, STEAM).unwrap();
    }
    run(&mut w, 400);
    assert_eq!(w.count(STEAM), 8);
    assert_eq!(count_in_rows(&w, 1..4, STEAM), 8, "all under the ceiling");
}

#[test]
fn liquids_fall_through_gas() {
    let mut w = world(0);
    for y in 1..30 {
        w.set(20, y, STEAM).unwrap();
    }
    w.set(20, 31, WATER).unwrap();
    w.set(20, 30, WATER).unwrap();
    run(&mut w, 300);
    assert_eq!(w.count(WATER), 2);
    assert_eq!(
        count_in_rows(&w, 62..63, WATER),
        2,
        "water reached the floor"
    );
    assert_eq!(w.count(STEAM), 29);
}

#[test]
fn lighter_gas_rises_through_denser_gas() {
    let mut w = world(0);
    for x in 1..63 {
        for y in 1..20 {
            w.set(x, y, HEAVY).unwrap();
        }
    }
    for x in 20..30 {
        w.set(x, 40, STEAM).unwrap();
    }
    run(&mut w, 3000);
    assert!(
        count_in_rows(&w, 1..10, STEAM) >= 5,
        "{} of 10 steam cells near the top",
        count_in_rows(&w, 1..10, STEAM)
    );
}

/// Age counts the cell's own moves (vt[16] returns `+0x34`), so gas that cannot move never
/// expires, while moving gas is removed once its age reaches `lifetime`.
#[test]
fn lifetime_counts_moves_not_frames() {
    let mut w = world(20);
    // Boxed in on all sides by rock: never moves.
    for (x, y) in [(10, 9), (9, 10), (11, 10), (10, 11)] {
        w.set(x, y, ROCK).unwrap();
    }
    w.set(10, 10, STEAM).unwrap();
    for x in 30..40 {
        w.set(x, 50, STEAM).unwrap();
    }
    run(&mut w, 2000);
    assert_eq!(w.get(10, 10), Some(STEAM), "trapped gas does not age");
    assert_eq!(w.count(STEAM), 1, "moving gas expired");
    assert_eq!(w.get(35, 50), Some(AIR));
}

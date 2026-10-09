//! World scheduler (0x726340 / 0x72a150): 64x64 regions with their own dirty rects, rects shared
//! across region borders, and one random full-region scan per frame.

mod common;

use common::{ROCK, SAND, WATER, run};
use sim::{AIR, REGION_SIZE, World};

fn world(width_chunks: u32, height_chunks: u32) -> World {
    let mut w = common::world(width_chunks, height_chunks, 64);
    let floor = w.height() - 1;
    for x in 0..w.width() {
        w.set(x, floor, ROCK).unwrap();
    }
    w
}

#[test]
fn falling_sand_crosses_region_borders() {
    let mut w = world(2, 3);
    w.set(REGION_SIZE - 1, 3, SAND).unwrap();
    run(&mut w, 600);
    let floor = w.height() - 2;
    assert_eq!(common::row_count(&w, floor, SAND), 1, "reached the floor");
}

/// The sweep rescans one region per frame, which draws the global RNG, but a settled world marks
/// nothing and its cells stay put.
#[test]
fn a_quiet_world_goes_idle() {
    let mut w = world(2, 2);
    run(&mut w, 30);
    assert_eq!(w.dirty_rect(), None);
    let floor = w.height() - 1;
    run(&mut w, 30);
    assert_eq!(w.dirty_rect(), None);
    assert_eq!(common::row_count(&w, floor, ROCK) as i32, w.width());
}

fn count(w: &World, id: sim::MaterialId) -> usize {
    (0..w.height())
        .flat_map(|y| (0..w.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| w.get(x, y) == Some(id))
        .count()
}

/// Fast water landing on water that is still falling and not yet updated this frame is deleted
/// (0x70c0c0). Regions above a border update first here, so a stream crossing it loses water.
#[test]
fn water_falling_into_a_later_region_is_lost() {
    let mut w = world(1, 3);
    for y in 0..40 {
        w.set(30, y, WATER).unwrap();
    }
    run(&mut w, 400);
    assert!(count(&w, WATER) < 40, "{} of 40 left", count(&w, WATER));
}

#[test]
fn water_falling_within_one_region_is_kept() {
    let mut w = world(1, 1);
    for y in 0..20 {
        w.set(30, y, WATER).unwrap();
    }
    run(&mut w, 400);
    assert_eq!(count(&w, WATER), 20);
}

#[test]
fn poured_water_does_not_hang_over_air() {
    let mut w = world(2, 2);
    for y in 10..40 {
        for x in 10..70 {
            w.set(x, y, SAND).unwrap();
        }
    }
    for frame in 0..1500 {
        if (100..200).contains(&frame) && frame % 2 == 0 {
            for dx in 0..3 {
                let _ = w.set(60 + dx, 3, WATER);
            }
        }
        w.step();
    }
    let floating = (0..w.height() - 1)
        .flat_map(|y| (0..w.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| w.get(x, y) == Some(WATER) && w.get(x, y + 1) == Some(AIR))
        .count();
    assert_eq!(floating, 0);
}

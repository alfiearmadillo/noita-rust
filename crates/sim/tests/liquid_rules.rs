//! Tests for the ordinary-liquid rules ported from `CLiquidCell` (0x70bdc0).

mod common;

use common::{ROCK, SAND, WATER, run, world};
use sim::{CellType, MaterialId, MaterialProps, Materials, MsvcRand, STATE_RESTING, World};

/// Rock-walled basin: floor at the bottom row, walls at x=0 and x=width-1.
fn basin(w: &mut World) {
    let (wd, floor) = (w.width(), w.height() - 1);
    for x in 0..wd {
        w.set(x, floor, ROCK).unwrap();
    }
    for y in 0..=floor {
        w.set(0, y, ROCK).unwrap();
        w.set(wd - 1, y, ROCK).unwrap();
    }
}

fn water_rows(w: &World) -> Vec<i32> {
    (0..w.height())
        .filter(|&y| (0..w.width()).any(|x| w.get(x, y) == Some(WATER)))
        .collect()
}

#[test]
fn msvc_range_is_inclusive() {
    let mut r = MsvcRand::new(3);
    let vals: Vec<_> = (0..200).map(|_| r.range(0, 1)).collect();
    assert!(vals.contains(&0) && vals.contains(&1));
    assert!(vals.iter().all(|v| (0..=1).contains(v)));
}

#[test]
fn fresh_cells_start_resting() {
    let mut w = world(1, 1, 8);
    w.set(3, 3, WATER).unwrap();
    let c = w.cell(3, 3).unwrap();
    assert_eq!(
        (c.state, c.rest, c.vel, c.flow),
        (STATE_RESTING, 0, 0.0, 0.0)
    );
}

#[test]
fn water_column_spreads_into_a_thin_layer() {
    let mut w = world(2, 1, 16);
    basin(&mut w);
    for y in 0..12 {
        w.set(16, y, WATER).unwrap();
    }
    run(&mut w, 1500);
    assert_eq!(w.count(WATER), 12);
    let floor = w.height() - 1;
    assert_eq!(
        water_rows(&w),
        [floor - 1],
        "water should lie in one layer on the floor"
    );
}

#[test]
fn water_levels_across_a_basin() {
    let mut w = world(2, 2, 16);
    basin(&mut w);
    // A tall block on the left side of the basin.
    for x in 1..8 {
        for y in 4..24 {
            w.set(x, y, WATER).unwrap();
        }
    }
    let total = w.count(WATER);
    run(&mut w, 3000);
    assert_eq!(w.count(WATER), total);
    let depth = |x: i32| {
        (0..w.height())
            .filter(|&y| w.get(x, y) == Some(WATER))
            .count()
    };
    let depths: Vec<_> = (1..w.width() - 1).map(depth).collect();
    let (min, max) = (depths.iter().min().unwrap(), depths.iter().max().unwrap());
    // Resting liquid (rest counter 5) can leave single-pixel bumps, so allow two.
    assert!(max - min <= 2, "surface not level: {depths:?}");
}

#[test]
fn water_does_not_sink_through_sand() {
    let mut w = world(1, 1, 16);
    basin(&mut w);
    let floor = w.height() - 1;
    for x in 1..w.width() - 1 {
        w.set(x, floor - 1, SAND).unwrap();
    }
    w.set(8, 2, WATER).unwrap();
    run(&mut w, 300);
    for x in 1..w.width() - 1 {
        assert_eq!(w.get(x, floor - 1), Some(SAND));
    }
    assert_eq!(water_rows(&w), [floor - 2]);
}

/// Flow starts only when exactly one side is open (0x70c1ee), so a lone droplet on flat ground stays.
#[test]
fn lone_droplet_on_flat_ground_stays() {
    let mut w = world(1, 1, 16);
    basin(&mut w);
    let floor = w.height() - 1;
    w.set(8, floor - 1, WATER).unwrap();
    run(&mut w, 300);
    assert_eq!(w.get(8, floor - 1), Some(WATER));
}

#[test]
fn droplet_at_ledge_edge_drops_off_diagonally() {
    let mut w = world(1, 2, 16);
    let floor = w.height() - 1;
    for x in 0..8 {
        w.set(x, 10, ROCK).unwrap();
    }
    w.set(7, 9, WATER).unwrap();
    run(&mut w, 300);
    assert_eq!(w.get(7, 9), Some(sim::AIR));
    assert_eq!(water_rows(&w), [floor]);
}

#[test]
fn water_on_ledge_mostly_flows_off_and_is_conserved() {
    let mut w = world(1, 2, 16);
    for x in 0..8 {
        w.set(x, 10, ROCK).unwrap();
    }
    for x in 2..8 {
        w.set(x, 9, WATER).unwrap();
    }
    run(&mut w, 800);
    assert_eq!(w.count(WATER), 6);
    let on_ledge = (0..8).filter(|&x| w.get(x, 9) == Some(WATER)).count();
    assert!(on_ledge < 6, "no water left the ledge");
    assert_eq!(w.get(0, 10), Some(ROCK));
}

#[test]
fn blocked_long_falls_record_splash_stats_unless_the_liquid_stains_itself() {
    let make_world = |liquid_stains_self, seed| {
        let materials = Materials::new(vec![
            MaterialProps::new("air", CellType::Unspecified),
            MaterialProps::new("rock", CellType::Solid),
            MaterialProps {
                liquid_gravity: 1.5,
                liquid_stains_self,
                ..MaterialProps::new("test_liquid", CellType::Liquid)
            },
        ]);
        let mut world = World::new(1, 1, 64, materials, seed);
        for y in 0..=60 {
            world.set(31, y, MaterialId(1)).unwrap();
            world.set(33, y, MaterialId(1)).unwrap();
        }
        for x in 30..=34 {
            world.set(x, 60, MaterialId(1)).unwrap();
        }
        world.set(32, 0, MaterialId(2)).unwrap();
        world
    };

    let seed = (0..256)
        .find(|&seed| {
            let mut world = make_world(false, seed);
            run(&mut world, 100);
            world
                .take_splash_stats()
                .iter()
                .any(|stats| stats.event_count > 0)
        })
        .expect("some deterministic seed reaches the blocked extra-step branch");

    let mut recording = make_world(false, seed);
    run(&mut recording, 100);
    let stats = recording.take_splash_stats();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].event_count, 1);
    assert_eq!(stats[0].sample_total, 1.0);
    assert_eq!(stats[0].sum_x, 32.0);
    assert!((16.0..60.0).contains(&stats[0].sum_y));
    assert_eq!(
        recording.take_splash_stats()[0],
        sim::SplashStats::default()
    );

    let mut excluded = make_world(true, seed);
    run(&mut excluded, 100);
    assert_eq!(excluded.take_splash_stats()[0], sim::SplashStats::default());
}

#[test]
fn liquid_older_than_its_lifetime_disappears() {
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        MaterialProps::new("rock", CellType::Solid),
        MaterialProps {
            lifetime: 20,
            ..MaterialProps::new("fading", CellType::Liquid)
        },
    ]);
    let mut w = World::new(1, 1, 64, materials, 3);
    for x in 20..30 {
        w.set(x, 60, MaterialId(2)).unwrap();
    }
    // Settled cells leave the scan set, so keep the whole world dirty.
    let step = |w: &mut World| {
        w.mark_area((0, 0), (63, 63));
        w.step();
    };
    for _ in 0..10 {
        step(&mut w);
    }
    assert_eq!(w.count(MaterialId(2)), 10, "younger than the lifetime");
    for _ in 0..40 {
        step(&mut w);
    }
    assert_eq!(w.count(MaterialId(2)), 0);
}

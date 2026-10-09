//! Tests for the sand rules ported from `CLiquidCell` (see README "Recovered rules").

mod common;

use common::{ROCK, SAND, WATER, all_sand_supported, row_count, run, run_active, world};
use sim::{AIR, MsvcRand, OutOfBounds, STATE_FALLING, STATE_RESTING};

#[test]
fn msvc_rand_matches_crt_sequence() {
    // Known output of MSVC `srand(1); rand()...`.
    let mut r = MsvcRand::new(1);
    let seq: Vec<_> = (0..5).map(|_| r.rand()).collect();
    assert_eq!(seq, [41, 18467, 6334, 26500, 19169]);
}

#[test]
fn fresh_sand_moves_one_cell_and_starts_falling() {
    let mut w = world(1, 2, 8);
    w.set(3, 0, SAND).unwrap();
    w.step();
    assert_eq!(w.get(3, 0), Some(AIR));
    let c = w.cell(3, 1).unwrap();
    assert_eq!(c.material, SAND);
    assert_eq!(c.state, STATE_FALLING);
    assert_eq!(c.vel, 0.0);
}

#[test]
fn falling_sand_accelerates() {
    let mut w = world(1, 8, 16);
    w.set(8, 0, SAND).unwrap();
    run(&mut w, 40);
    let y = (0..w.height())
        .find(|&y| (0..w.width()).any(|x| w.get(x, y) == Some(SAND)))
        .unwrap();
    assert!(y > 40, "fell only {y} cells in 40 frames");
}

#[test]
fn sand_settles_supported_and_is_conserved() {
    let mut w = world(2, 2, 16);
    for x in 4..28 {
        for y in 0..6 {
            w.set(x, y, SAND).unwrap();
        }
    }
    let before = w.count(SAND);
    run(&mut w, 400);
    assert_eq!(w.count(SAND), before);
    assert!(all_sand_supported(&w));
}

#[test]
fn single_grain_on_grain_slides_off_diagonally() {
    let mut w = world(1, 1, 16);
    let floor = w.height() - 1;
    w.set(8, floor, SAND).unwrap();
    w.set(8, floor - 1, SAND).unwrap();
    run_active(&mut w, 200);
    assert_eq!(w.get(8, floor), Some(SAND));
    let on_floor = [7, 9]
        .iter()
        .filter(|&&x| w.get(x, floor) == Some(SAND))
        .count();
    assert_eq!(on_floor, 1, "top grain should slide to a floor neighbor");
}

#[test]
fn isolated_static_pixel_is_removed_on_a_full_update() {
    let mut w = world(1, 1, 32);
    w.set(8, 8, ROCK).unwrap();
    w.set(20, 8, ROCK).unwrap();
    w.set(21, 8, ROCK).unwrap();
    // Every 11th region update is full, so 22 frames include at least one.
    run_active(&mut w, 22);
    assert_eq!(w.get(8, 8), Some(AIR));
    assert_eq!(w.get(20, 8), Some(ROCK));
    assert_eq!(w.get(21, 8), Some(ROCK));
}

#[test]
fn rock_static_never_moves_and_supports_sand() {
    let mut w = world(1, 2, 8);
    for x in 0..8 {
        w.set(x, 10, ROCK).unwrap();
    }
    w.set(4, 0, SAND).unwrap();
    run(&mut w, 100);
    for x in 0..8 {
        assert_eq!(w.get(x, 10), Some(ROCK));
    }
    assert_eq!(row_count(&w, 9, SAND), 1, "sand lands on the rock");
}

#[test]
fn denser_sand_sinks_through_water() {
    let mut w = world(1, 1, 16);
    let floor = w.height() - 1;
    for y in 0..=floor {
        w.set(6, y, ROCK).unwrap();
        w.set(8, y, ROCK).unwrap();
    }
    for y in 4..=floor {
        w.set(7, y, WATER).unwrap();
    }
    w.set(7, 0, SAND).unwrap();
    run_active(&mut w, 500);
    assert_eq!(w.get(7, floor), Some(SAND));
    assert_eq!(w.count(WATER), (floor - 3) as usize);
}

/// Resting sand does not mark itself (0x70b78c), so once nothing moves nothing is marked.
#[test]
fn enclosed_resting_sand_stops_being_scanned() {
    let mut w = world(1, 1, 8);
    let floor = w.height() - 1;
    w.set(2, floor, ROCK).unwrap();
    w.set(3, floor, SAND).unwrap();
    w.set(4, floor, ROCK).unwrap();
    run(&mut w, 100);
    let c = *w.cell(3, floor).unwrap();
    assert_eq!(c.state, STATE_RESTING);
    assert_eq!(w.dirty_rect(), None);
    run(&mut w, 100);
    let after = *w.cell(3, floor).unwrap();
    assert_eq!((after.material, after.state), (c.material, c.state));
    assert_eq!(w.dirty_rect(), None, "the sweep rescans without marking");
}

#[test]
fn dirty_rect_tracks_only_moving_cells() {
    let mut w = world(4, 4, 16);
    let floor = w.height() - 1;
    for x in 0..w.width() {
        w.set(x, floor, ROCK).unwrap();
    }
    run(&mut w, 2);
    assert_eq!(w.dirty_rect(), None, "static rock never marks");
    w.set(10, 5, SAND).unwrap();
    w.step();
    let ((x0, y0), (x1, y1)) = w.dirty_rect().expect("falling sand marks");
    assert!(x0 >= 9 && x1 <= 11, "rect x {x0}..{x1}");
    assert!(y0 >= 5 && y1 <= 7, "rect y {y0}..{y1}");
}

#[test]
fn painting_into_settled_world_resumes_simulation() {
    let mut w = world(1, 2, 16);
    run(&mut w, 5);
    assert_eq!(w.dirty_rect(), None);
    w.set(4, 0, SAND).unwrap();
    run(&mut w, 200);
    assert_eq!(row_count(&w, w.height() - 1, SAND), 1);
}

#[test]
fn out_of_bounds_access_is_rejected_without_mutation() {
    let mut w = world(1, 1, 8);
    let (wd, ht) = (w.width(), w.height());
    for (x, y) in [(-1, 0), (0, -1), (wd, 0), (0, ht), (i32::MIN, i32::MAX)] {
        assert_eq!(w.get(x, y), None);
        assert_eq!(w.chunk_of(x, y), None);
        assert_eq!(w.set(x, y, SAND), Err(OutOfBounds { x, y }));
    }
    assert_eq!(w.count(SAND), 0);
}

#[test]
fn bottom_edge_blocks_and_side_edges_do_not_wrap() {
    let mut w = world(1, 1, 8);
    let (right, floor) = (w.width() - 1, w.height() - 1);
    w.set(0, 0, SAND).unwrap();
    w.set(right, 0, SAND).unwrap();
    run(&mut w, 100);
    assert_eq!(w.count(SAND), 2);
    assert!(all_sand_supported(&w));
    assert!(
        (0..w.width())
            .filter(|&x| w.get(x, floor) == Some(SAND))
            .count()
            == 2
    );
}

#[test]
fn sand_crosses_chunk_seam_into_lower_chunk_storage() {
    let mut w = world(2, 2, 4);
    w.set(5, 3, SAND).unwrap();
    w.step();
    assert_eq!(w.get(5, 4), Some(SAND));
    let sand_in = |cx, cy| {
        w.chunk_cells(cx, cy)
            .unwrap()
            .iter()
            .filter(|c| c.material == SAND)
            .count()
    };
    assert_eq!(sand_in(1, 0), 0);
    assert_eq!(sand_in(1, 1), 1);
}

#[test]
fn same_seed_same_result_different_seed_diverges() {
    let build = |seed| {
        let mut w = sim::World::new(2, 2, 16, common::materials(), seed);
        for x in 0..w.width() {
            for y in 0..8 {
                w.set(x, y, SAND).unwrap();
            }
        }
        run(&mut w, 60);
        w.state_hash()
    };
    assert_eq!(build(7), build(7));
    assert_ne!(build(7), build(8));
}

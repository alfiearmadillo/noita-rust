//! `liquid_stains` (0x70ce40): a moving liquid recolors the sand beside or below it.

use sim::{CellType, MaterialId, MaterialProps, Materials, World};

const SAND: MaterialId = MaterialId(1);
const BLOOD: MaterialId = MaterialId(2);
const SAND_COLOR: u32 = 0xff80_8080;

fn world(stains: u32, custom: u32, sand_stainable: bool) -> World {
    let air = MaterialProps::new("air", CellType::Unspecified);
    let sand = MaterialProps {
        liquid_sand: true,
        density: 6.0,
        color: SAND_COLOR,
        stainable: sand_stainable,
        ..MaterialProps::new("sand", CellType::Liquid)
    };
    let blood = MaterialProps {
        density: 4.0,
        color: 0xffaa_0000,
        liquid_stains: stains,
        liquid_stains_custom_color: custom,
        ..MaterialProps::new("blood", CellType::Liquid)
    };
    let mut w = World::new(1, 1, 64, Materials::new(vec![air, sand, blood]), 3);
    for x in 0..w.width() {
        w.set(x, 63, SAND).unwrap();
    }
    w.set(32, 62, BLOOD).unwrap();
    w
}

fn run_active(w: &mut World, frames: usize) {
    for _ in 0..frames {
        w.mark_area((0, 0), (w.width() - 1, w.height() - 1));
        w.step();
    }
}

fn red(w: &World, x: i32, y: i32) -> u32 {
    (w.cell(x, y).unwrap().color >> 16) & 0xFF
}

#[test]
fn tint_stain_pulls_the_sand_below_toward_the_stain_color() {
    let mut w = world(2, 0xffff_0000, true);
    run_active(&mut w, 400);
    assert_eq!(w.get(32, 62), Some(BLOOD), "the blood stays put");
    assert!(red(&w, 32, 63) > 0x80 + 20, "red = {:#x}", red(&w, 32, 63));
    assert_eq!(
        red(&w, 10, 63),
        0x80,
        "cells away from the blood are untouched"
    );
}

#[test]
fn stain_never_changes_the_base_color() {
    let mut w = world(2, 0xffff_0000, true);
    run_active(&mut w, 400);
    assert_eq!(w.cell(32, 63).unwrap().base_color, SAND_COLOR);
}

#[test]
fn liquid_stains_zero_leaves_colors_alone() {
    let mut w = world(0, 0xffff_0000, true);
    run_active(&mut w, 400);
    assert_eq!(w.cell(32, 63).unwrap().color, SAND_COLOR);
}

#[test]
fn unstainable_cells_keep_their_color() {
    let mut w = world(2, 0xffff_0000, false);
    run_active(&mut w, 400);
    assert_eq!(w.cell(32, 63).unwrap().color, SAND_COLOR);
}

#[test]
fn darkening_stain_drags_the_color_toward_seven_tenths_of_the_base() {
    let mut w = world(1, 0, true);
    run_active(&mut w, 400);
    let c = w.cell(32, 63).unwrap().color;
    assert!(red(&w, 32, 63) < 0x80 && red(&w, 32, 63) >= 0x59, "{c:#x}");
}

//! Shared test fixtures. Material values are copied from the reference `materials.xml`
//! (`sand`, `rock_static`, `water`) and the `CellData` constructor defaults.

#![allow(dead_code)]

use sim::{CellType, MaterialId, MaterialProps, Materials, World};

pub const SAND: MaterialId = MaterialId(1);
pub const ROCK: MaterialId = MaterialId(2);
pub const WATER: MaterialId = MaterialId(3);

pub fn materials() -> Materials {
    let air = MaterialProps::new("air", CellType::Unspecified);
    let sand = MaterialProps {
        density: 6.0,
        liquid_sand: true,
        liquid_gravity: 0.9,
        ..MaterialProps::new("sand", CellType::Liquid)
    };
    let rock = MaterialProps {
        density: 10.0,
        liquid_sand: true,
        liquid_static: true,
        liquid_gravity: 1.2,
        ..MaterialProps::new("rock_static", CellType::Liquid)
    };
    let water = MaterialProps {
        density: 4.0,
        liquid_gravity: 1.5,
        ..MaterialProps::new("water", CellType::Liquid)
    };
    Materials::new(vec![air, sand, rock, water])
}

pub fn world(width_chunks: u32, height_chunks: u32, chunk_size: u32) -> World {
    World::new(width_chunks, height_chunks, chunk_size, materials(), 1)
}

pub fn run(w: &mut World, frames: usize) {
    for _ in 0..frames {
        w.step();
    }
}

/// Keeps the whole world in the scan every frame, to test cell rules independently of the
/// dirty-rect scheduler (which can leave idle cells unscanned, as in the reference).
pub fn run_active(w: &mut World, frames: usize) {
    for _ in 0..frames {
        w.mark_area((0, 0), (w.width() - 1, w.height() - 1));
        w.step();
    }
}

/// Number of `material` cells in row `y`.
pub fn row_count(w: &World, y: i32, material: sim::MaterialId) -> usize {
    (0..w.width())
        .filter(|&x| w.get(x, y) == Some(material))
        .count()
}

/// Every sand cell rests on something (non-air or the world floor).
pub fn all_sand_supported(w: &World) -> bool {
    (0..w.height()).all(|y| {
        (0..w.width()).all(|x| {
            w.get(x, y) != Some(SAND) || y == w.height() - 1 || w.get(x, y + 1) != Some(sim::AIR)
        })
    })
}

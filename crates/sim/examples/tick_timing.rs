//! Tick timing for the falling-sand prototype. Compare debug vs release:
//!
//! `cargo run -p sim --example tick_timing` and `cargo run -p sim --example tick_timing --release`

use std::time::{Duration, Instant};

use sim::{CellType, MaterialId, MaterialProps, Materials, World};

const TICKS: usize = 300;
const SAND: MaterialId = MaterialId(1);

fn main() {
    // Sand values from the reference materials.xml.
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        MaterialProps {
            density: 6.0,
            liquid_sand: true,
            liquid_gravity: 0.9,
            ..MaterialProps::new("sand", CellType::Liquid)
        },
    ]);
    let mut world = World::new(8, 8, 64, materials, 1);
    let (w, h) = (world.width(), world.height());
    for y in 0..h / 2 {
        for x in 0..w {
            if (x * 7 + y * 13) % 3 == 0 {
                world.set(x, y, SAND).expect("in bounds");
            }
        }
    }
    let sand = world.count(SAND);

    let mut samples: Vec<Duration> = (0..TICKS)
        .map(|_| {
            let start = Instant::now();
            world.step();
            start.elapsed()
        })
        .collect();
    samples.sort();

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let total: Duration = samples.iter().sum();
    println!("profile={profile} world={w}x{h} chunk=64 sand={sand} ticks={TICKS}");
    println!(
        "total={total:?} p50={:?} p95={:?} max={:?}",
        samples[TICKS / 2],
        samples[TICKS * 95 / 100],
        samples[TICKS - 1]
    );
}

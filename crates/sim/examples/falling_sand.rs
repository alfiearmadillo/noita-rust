//! Text-mode visual harness: prints the grid so falling and chunk seams can be eyeballed.
//!
//! Run: `cargo run -p sim --example falling_sand`

use sim::{AIR, CellType, MaterialId, MaterialProps, Materials, World};

// Values from the reference materials.xml (`sand`, `rock_static`).
fn materials() -> Materials {
    Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        MaterialProps {
            density: 6.0,
            liquid_sand: true,
            liquid_gravity: 0.9,
            ..MaterialProps::new("sand", CellType::Liquid)
        },
        MaterialProps {
            density: 10.0,
            liquid_sand: true,
            liquid_static: true,
            liquid_gravity: 1.2,
            ..MaterialProps::new("rock_static", CellType::Liquid)
        },
    ])
}

const SAND: MaterialId = MaterialId(1);
const ROCK: MaterialId = MaterialId(2);

fn render(world: &World) -> String {
    let cs = world.chunk_size();
    let mut out = String::new();
    for y in 0..world.height() {
        if y > 0 && y % cs == 0 {
            out.push_str(&"-".repeat(world.width() as usize));
            out.push('\n');
        }
        for x in 0..world.width() {
            out.push(match world.get(x, y) {
                Some(SAND) => 'o',
                Some(ROCK) => '#',
                _ => '.',
            });
        }
        out.push('\n');
    }
    out
}

fn layout(world: &World) -> Vec<MaterialId> {
    (0..world.height())
        .flat_map(|y| (0..world.width()).map(move |x| world.get(x, y).unwrap_or(AIR)))
        .collect()
}

fn main() {
    let mut world = World::new(3, 2, 8, materials(), 1);
    for x in 4..12 {
        world.set(x, 12, ROCK).expect("in bounds");
    }
    for x in 5..10 {
        for y in 0..4 {
            world.set(x, y, SAND).expect("in bounds");
        }
    }
    for (x, y) in [(15, 2), (16, 2), (23, 5)] {
        world.set(x, y, SAND).expect("in bounds");
    }

    let mut unchanged = 0;
    while unchanged < 20 && world.tick() < 500 {
        if world.tick().is_multiple_of(4) {
            println!("frame {}\n{}", world.tick(), render(&world));
        }
        let before = layout(&world);
        world.step();
        unchanged = if layout(&world) == before {
            unchanged + 1
        } else {
            0
        };
    }
    println!("frame {}\n{}", world.tick(), render(&world));
}

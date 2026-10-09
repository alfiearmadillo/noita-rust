//! Pours water onto a sand slope and reports liquid cells left hanging over air once the
//! dirty-rect scheduler goes idle around them.

use sim::{AIR, CellType, MaterialId, MaterialProps, Materials, World};

fn main() {
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
        ..MaterialProps::new("rock_static", CellType::Liquid)
    };
    let water = MaterialProps {
        density: 4.0,
        liquid_gravity: 1.5,
        ..MaterialProps::new("water", CellType::Liquid)
    };
    let (sand_id, rock_id, water_id) = (MaterialId(1), MaterialId(2), MaterialId(3));
    let mut w = World::new(4, 3, 64, Materials::new(vec![air, sand, rock, water]), 1);
    let (width, height) = (w.width(), w.height());
    for x in 0..width {
        w.set(x, height - 1, rock_id).unwrap();
    }
    for y in 10..60 {
        for x in 20..120 {
            w.set(x, y, sand_id).unwrap();
        }
    }
    let mut stuck_max = 0;
    for frame in 0..3000 {
        if (200..400).contains(&frame) && frame % 2 == 0 {
            for dx in 0..4 {
                let _ = w.set(100 + dx, 5, water_id);
            }
        }
        w.step();
        if frame % 100 == 99 {
            let floating: Vec<(i32, i32)> = (0..height - 1)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .filter(|&(x, y)| w.get(x, y) == Some(water_id) && w.get(x, y + 1) == Some(AIR))
                .collect();
            stuck_max = stuck_max.max(floating.len());
            println!(
                "frame {:4}: {:3} water over air, dirty {:?}{}",
                frame + 1,
                floating.len(),
                w.dirty_rect(),
                floating
                    .first()
                    .map(|p| format!(", e.g. {p:?}"))
                    .unwrap_or_default()
            );
        }
    }
    println!("max floating: {stuck_max}");
}

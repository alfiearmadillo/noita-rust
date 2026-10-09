//! Traces a character walking into a wall: position, velocity and ground flag per frame.

use actors::{Body, Character, Input, Platforming};
use sim::{CellType, MaterialId, MaterialProps, Materials, World};

fn main() {
    let rock = MaterialProps {
        liquid_sand: true,
        liquid_static: true,
        ..MaterialProps::new("rock_static", CellType::Liquid)
    };
    let table = Materials::new(vec![MaterialProps::new("air", CellType::Unspecified), rock]);
    let mut w = World::new(4, 4, 64, table, 1);
    for x in 0..w.width() {
        for y in 100..103 {
            w.set(x, y, MaterialId(1)).unwrap();
        }
    }
    for y in 60..100 {
        w.set(60, y, MaterialId(1)).unwrap();
        w.set(61, y, MaterialId(1)).unwrap();
    }
    let mut c = Character::new((50.0, 96.0), Platforming::player(), Body::player());
    let idle = Input {
        mouse: (1000.0, 0.0),
        ..Input::default()
    };
    for _ in 0..30 {
        c.step(&idle, &w, &());
    }
    let right = Input {
        right: true,
        ..idle
    };
    let mut grounded = 0;
    for f in 0..240 {
        c.step(&right, &w, &());
        grounded += u32::from(c.on_ground);
        if (20..80).contains(&f) {
            println!(
                "{f:3} pos=({:.4},{:.4}) vel=({:.3},{:.3}) ground={} wall={}",
                c.pos.0, c.pos.1, c.vel.0, c.vel.1, c.on_ground, c.collided_horizontally
            );
        }
    }
    println!("grounded {grounded}/240 frames");
}

//! `cell_holes_in_texture` (`0x7048c0`): no cell where the texture is fully transparent.

use std::sync::Arc;

use sim::{AIR, CellType, MaterialId, MaterialProps, Materials, Texture, World};

const VINE: MaterialId = MaterialId(1);

fn world(holes: bool) -> World {
    // Two texels: opaque, then transparent; the texture tiles across x.
    let texture = Texture::new(2, 1, vec![10, 20, 30, 255, 0, 0, 0, 0]).unwrap();
    let vine = MaterialProps {
        cell_holes_in_texture: holes,
        texture: Some(Arc::new(texture)),
        ..MaterialProps::new("vine", CellType::Liquid)
    };
    let air = MaterialProps::new("air", CellType::Unspecified);
    World::new(1, 1, 16, Materials::new(vec![air, vine]), 1)
}

#[test]
fn transparent_texels_make_no_cell() {
    let mut w = world(true);
    for x in 0..4 {
        w.set(x, 0, VINE).unwrap();
    }
    let got: Vec<_> = (0..4).map(|x| w.get(x, 0).unwrap()).collect();
    assert_eq!(got, [VINE, AIR, VINE, AIR]);
}

#[test]
fn a_hole_replaces_what_was_there() {
    let mut w = world(true);
    w.set(1, 0, VINE).unwrap();
    assert_eq!(w.get(1, 0), Some(AIR));
}

#[test]
fn without_the_flag_every_texel_makes_a_cell() {
    let mut w = world(false);
    for x in 0..4 {
        w.set(x, 0, VINE).unwrap();
    }
    assert_eq!(w.count(VINE), 4);
}

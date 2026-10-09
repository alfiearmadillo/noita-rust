mod common;

use common::{SAND, world};
use sim::{CellType, MaterialId, MaterialProps, Materials, Texture, World};

fn tex_2x2() -> Texture {
    // (0,0) red, (1,0) green, (0,1) blue, (1,1) white; alpha 255.
    let rgba = vec![
        255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 255, 255,
    ];
    Texture::new(2, 2, rgba).unwrap()
}

#[test]
fn texture_sample_wraps_both_axes_including_negative() {
    let t = tex_2x2();
    assert_eq!(t.sample(0, 0), 0xffff_0000);
    assert_eq!(t.sample(1, 0), 0xff00_ff00);
    assert_eq!(t.sample(0, 1), 0xff00_00ff);
    assert_eq!(t.sample(3, 2), t.sample(1, 0));
    assert_eq!(t.sample(-1, -1), t.sample(1, 1));
}

#[test]
fn texture_rejects_bad_sizes() {
    assert!(Texture::new(0, 2, vec![]).is_none());
    assert!(Texture::new(2, 2, vec![0; 15]).is_none());
}

#[test]
fn cell_color_comes_from_texture_at_placement_and_travels() {
    let textured = MaterialProps {
        density: 6.0,
        liquid_sand: true,
        liquid_gravity: 0.9,
        texture: Some(tex_2x2().into()),
        ..MaterialProps::new("tsand", CellType::Liquid)
    };
    let table = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        textured,
    ]);
    let mut w = World::new(1, 1, 8, table, 1);
    let id = MaterialId(1);
    w.set(1, 0, id).unwrap();
    assert_eq!(w.cell(1, 0).unwrap().color, 0xff00_ff00);
    w.step();
    assert_eq!(
        w.cell(1, 1).unwrap().color,
        0xff00_ff00,
        "color moves with the cell"
    );
}

#[test]
fn untextured_material_uses_flat_color() {
    let mut w = world(1, 1, 8);
    w.set(2, 2, SAND).unwrap();
    let flat = w.materials().get(SAND).unwrap().color;
    assert_eq!(w.cell(2, 2).unwrap().color, flat);
}

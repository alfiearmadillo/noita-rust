//! Wang tile generation on small hand-built tilesets (no reference data needed).

use worldgen::wang::{Tile, Tileset, WangError, WangRng};

fn solid(colors: [i8; 6], len: usize, rgb: [u8; 3]) -> Tile {
    Tile {
        colors,
        pixels: rgb.repeat(len),
    }
}

/// One corner color per type, so every slot accepts the single horizontal and vertical tile.
fn trivial(side: usize) -> Tileset {
    Tileset {
        is_corner: true,
        num_color: [1, 1, 1, 1, 0, 0],
        short_side_len: side,
        h_tiles: vec![solid([0; 6], side * side * 2, [200, 0, 0])],
        v_tiles: vec![solid([0; 6], side * side * 2, [0, 0, 200])],
    }
}

#[test]
fn the_rng_follows_the_reference_stream() {
    // from_seed(1): state 1 stepped once -> 16807; each draw steps again and scales by
    // 4.656612875e-10 * 2147483645, which truncates one below the raw state.
    let mut rng = WangRng::new(1.0);
    assert_eq!(rng.rand(), 282_475_248);
    assert_eq!(rng.rand(), 1_622_650_071);
}

#[test]
fn a_single_tile_pair_tiles_the_map_in_a_herringbone() {
    let ts = trivial(4);
    let map = ts.generate(&mut WangRng::new(5.0), 24, 16).unwrap();
    let px = |x: usize, y: usize| {
        let i = (y * 24 + x) * 3;
        [map[i], map[i + 1], map[i + 2]]
    };
    // Band 0 (y 0..4): the bottom of the vertical tile started a row earlier at x 8, the new
    // vertical tile at x 12, horizontal blocks on both sides.
    assert_eq!(px(0, 0), [200, 0, 0]);
    assert_eq!(px(7, 3), [200, 0, 0]);
    assert_eq!(px(8, 0), [0, 0, 200]);
    assert_eq!(px(15, 3), [0, 0, 200]);
    assert_eq!(px(16, 0), [200, 0, 0]);
    assert_eq!(px(23, 3), [200, 0, 0]);
    // Band 1 is displaced by one tile.
    assert_eq!(px(0, 4), [0, 0, 200]);
    assert_eq!(px(4, 4), [200, 0, 0]);
    assert_eq!(px(11, 7), [200, 0, 0]);
    assert_eq!(px(12, 4), [0, 0, 200]);
    // Every pixel is covered by exactly one of the two tiles.
    assert!(
        map.as_chunks::<3>()
            .0
            .iter()
            .all(|p| *p == [200, 0, 0] || *p == [0, 0, 200])
    );
}

#[test]
fn unsatisfiable_constraints_are_reported() {
    let mut ts = trivial(4);
    ts.h_tiles[0].colors = [1; 6];
    assert_eq!(
        ts.generate(&mut WangRng::new(5.0), 24, 16),
        Err(WangError::NoMatchingTile)
    );
}

#[test]
fn bad_templates_are_rejected() {
    let blank = vec![0u8; 30 * 30 * 3];
    // A blank image decodes to a header of 0xFF ^ i*55 patterns that is never a valid layout.
    assert!(Tileset::from_image(&blank, 30, 30).is_err());
    assert_eq!(
        Tileset::from_image(&blank[..10], 30, 30),
        Err(WangError::ImageTooSmall)
    );
}

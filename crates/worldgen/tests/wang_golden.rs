//! `Tileset::generate` against the reference's `stbhw_generate_image` (`0x866b90`) run in
//! Ghidra's emulator (`scripts/emulate_golden.py wang`). Tiles carry index-derived patterns, so
//! the files hold no game art; only the constraint structure of real templates is used.

use std::collections::HashMap;

use worldgen::wang::{Tile, Tileset, WangRng};

const GOLDEN: &str = include_str!("data/wang_golden.txt");

fn pattern(global_index: usize, x: usize, y: usize, width: usize) -> [u8; 3] {
    [
        global_index as u8,
        (global_index >> 8) as u8,
        (y * width + x) as u8,
    ]
}

fn tiles(colors: &[[i8; 6]], first_index: usize, tw: usize, th: usize) -> Vec<Tile> {
    colors
        .iter()
        .enumerate()
        .map(|(n, c)| {
            let mut pixels = Vec::with_capacity(tw * th * 3);
            for y in 0..th {
                for x in 0..tw {
                    pixels.extend(pattern(first_index + n, x, y, tw));
                }
            }
            Tile { colors: *c, pixels }
        })
        .collect()
}

#[test]
fn generate_matches_the_reference_function() {
    let mut sets: HashMap<String, Tileset> = HashMap::new();
    let mut lines = GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty());
    let mut checked = 0;
    let mut kinds = [0; 2];
    while let Some(line) = lines.next() {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "tileset" => {
                let corner = f[2] == "1";
                let mut nc = [0i32; 6];
                for (i, v) in nc.iter_mut().enumerate() {
                    *v = f[3 + i].parse().unwrap();
                }
                let side: usize = f[9].parse().unwrap();
                let (nh, nv): (usize, usize) = (f[10].parse().unwrap(), f[11].parse().unwrap());
                let mut colors = Vec::new();
                for _ in 0..nh + nv {
                    let row = lines.next().unwrap();
                    let c: Vec<i8> = row.split(' ').map(|v| v.parse().unwrap()).collect();
                    colors.push([c[0], c[1], c[2], c[3], c[4], c[5]]);
                }
                sets.insert(
                    f[1].to_owned(),
                    Tileset {
                        is_corner: corner,
                        num_color: nc,
                        short_side_len: side,
                        h_tiles: tiles(&colors[..nh], 0, 2 * side, side),
                        v_tiles: tiles(&colors[nh..], nh, side, 2 * side),
                    },
                );
            }
            "case" => {
                let ts = &sets[f[1]];
                let seed =
                    f64::from_bits(u64::from_str_radix(f[2].trim_start_matches("0x"), 16).unwrap());
                let (w, h): (usize, usize) = (f[3].parse().unwrap(), f[4].parse().unwrap());
                assert_eq!(f[5], "1");
                let want: Vec<u8> = (0..f[6].len() / 2)
                    .map(|i| u8::from_str_radix(&f[6][2 * i..2 * i + 2], 16).unwrap())
                    .collect();
                let got = ts
                    .generate(&mut WangRng::new(seed), w, h)
                    .unwrap_or_else(|e| panic!("{} seed {seed} {w}x{h}: {e:?}", f[1]));
                assert_eq!(got, want, "{} seed {seed} {w}x{h}", f[1]);
                kinds[usize::from(ts.is_corner)] += 1;
                checked += 1;
            }
            other => panic!("unknown record {other}"),
        }
    }
    assert!(
        checked >= 25 && kinds[0] >= 9 && kinds[1] >= 15,
        "{checked} {kinds:?}"
    );
}

//! `WangMap::from_wang_rgb` against vectors from the reference conversion (`0x8704c0`) running in
//! Ghidra's emulator (`scripts/emulate_golden.py convert`). Spawn-function colors are not
//! covered (the reference's lookup object is left empty). The images are random, not game data.

use worldgen::terrain::{ColorTable, WangMap};

const GOLDEN: &str = include_str!("data/convert_golden.txt");

fn rgb(s: &str) -> [u8; 3] {
    let v = u32::from_str_radix(s, 16).unwrap();
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

#[test]
fn conversion_matches_the_reference() {
    let mut bad = Vec::new();
    let mut counts = [0usize; 3]; // material pixels, air-color pixels, dilated pixels
    let mut cases = 0;
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let parts: Vec<&str> = line.split(" | ").collect();
        let head: Vec<&str> = parts[0].split(' ').collect();
        let (w, h): (usize, usize) = (head[2].parse().unwrap(), head[3].parse().unwrap());
        let table: Vec<u32> = head[4..]
            .iter()
            .map(|s| u32::from_str_radix(s, 16).unwrap())
            .collect();
        let pixels: Vec<u8> = parts[1].split(' ').flat_map(rgb).collect();
        let want_f: Vec<u32> = parts[2]
            .split(' ')
            .map(|s| u32::from_str_radix(s, 16).unwrap())
            .collect();
        let want_m: Vec<u16> = parts[3].split(' ').map(|s| s.parse().unwrap()).collect();
        let got =
            WangMap::from_wang_rgb(&pixels, w, h, &ColorTable::new(table.clone()), |_| None).map;
        let got_f: Vec<u32> = got.values.data.iter().map(|v| v.to_bits()).collect();
        cases += 1;
        for (f, m) in want_f.iter().zip(&want_m) {
            counts[0] += usize::from(*m > 1);
            counts[1] += usize::from(*m == 1);
            counts[2] += usize::from(*f == 0 && *m != 0);
        }
        if got_f != want_f || got.materials.data != want_m {
            bad.push(format!(
                "case {}: {w}x{h} floats {:?} vs {:?}, materials {:?} vs {:?}",
                head[1], got_f, want_f, got.materials.data, want_m
            ));
        }
    }
    assert!(
        cases >= 60 && counts.iter().all(|&n| n > 20),
        "{cases} {counts:?}"
    );
    assert!(
        bad.is_empty(),
        "{} differ, first: {:?}",
        bad.len(),
        &bad[..bad.len().min(2)]
    );
}

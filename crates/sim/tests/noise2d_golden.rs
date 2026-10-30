//! The 2D noise functions against vectors produced by running the reference code
//! (`0x872d40`, `0x872a20`, `0x872be0`) in Ghidra's emulator (`scripts/emulate_golden.py noise2d`).

use sim::noise::{perlin2, simplex2, value2};

const GOLDEN: &str = include_str!("data/noise2d_golden.csv");

fn bits(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

#[test]
fn the_2d_noise_functions_match_the_reference() {
    let mut counts = std::collections::BTreeMap::new();
    let mut bad = Vec::new();
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split(',').collect();
        let (x, y) = (f32::from_bits(bits(f[1])), f32::from_bits(bits(f[2])));
        let want = f32::from_bits(bits(f[3]));
        let got = match f[0] {
            "simplex2" => simplex2(x, y),
            "perlin2" => perlin2(x, y),
            "value2" => value2(x, y),
            other => panic!("unknown function {other}"),
        };
        *counts.entry(f[0].to_owned()).or_insert(0) += 1;
        if got.to_bits() != want.to_bits() {
            bad.push(format!("{}({x}, {y}): got {got}, want {want}", f[0]));
        }
    }
    assert!(counts.values().all(|&n| n >= 300), "{counts:?}");
    assert!(
        bad.is_empty(),
        "{} differ, first: {:?}",
        bad.len(),
        &bad[..bad.len().min(4)]
    );
}

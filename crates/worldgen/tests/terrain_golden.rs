//! The Wang map samplers and the noise blend against vectors from the reference running in
//! Ghidra's emulator (`scripts/emulate_golden.py terrain`). The images are random, not game data.

use std::collections::HashMap;

use worldgen::terrain::{Image, WangMap, terrain_value};

const GOLDEN: &str = include_str!("data/terrain_golden.txt");

fn num(s: &str) -> u64 {
    match s.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16).unwrap(),
        None => s.parse().unwrap(),
    }
}

#[test]
fn samplers_match_the_reference() {
    let mut maps: HashMap<u64, WangMap> = HashMap::new();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut bad = Vec::new();
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "image" => {
                let (w, h): (i32, i32) = (f[2].parse().unwrap(), f[3].parse().unwrap());
                let n = (w * h) as usize;
                let values = f[4..4 + n]
                    .iter()
                    .map(|s| f32::from_bits(num(s) as u32))
                    .collect();
                let materials = f[4 + n..4 + 2 * n]
                    .iter()
                    .map(|s| s.parse().unwrap())
                    .collect();
                maps.insert(
                    num(f[1]),
                    WangMap {
                        values: Image::new(w, h, values),
                        materials: Image::new(w, h, materials),
                    },
                );
            }
            "case" => {
                let map = &maps[&num(f[1])];
                let (u, v) = (f64::from_bits(num(f[3])), f64::from_bits(num(f[4])));
                let want = num(f[5]) as u32;
                let got = match f[2] {
                    "round" => map.round_index(u, v) as u32,
                    "smooth" => map.smooth(u, v).to_bits(),
                    "mode1" => map.edge_mode1(u, v).to_bits(),
                    "mode2" => map.edge_mode2(u, v).to_bits(),
                    other => panic!("unknown sampler {other}"),
                };
                *counts.entry(f[2].to_owned()).or_default() += 1;
                if got != want {
                    bad.push(format!("{} ({u}, {v}): got {got:#x}, want {want:#x}", f[2]));
                }
            }
            "blend" => {
                let (x, y) = (f64::from_bits(num(f[1])), f64::from_bits(num(f[2])));
                let base = f32::from_bits(num(f[3]) as u32);
                let want = num(f[4]) as u32;
                let got = terrain_value(x, y, base).to_bits();
                *counts.entry("blend".to_owned()).or_default() += 1;
                if got != want {
                    bad.push(format!(
                        "blend ({x}, {y}, {base}): got {got:#x}, want {want:#x}"
                    ));
                }
            }
            other => panic!("unknown record {other}"),
        }
    }
    assert!(counts.values().all(|&n| n >= 300), "{counts:?}");
    assert!(
        bad.is_empty(),
        "{} differ, first: {:?}",
        bad.len(),
        &bad[..bad.len().min(6)]
    );
}

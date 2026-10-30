//! `WangBiome::material_at` (the type-2 branch of `0x87d0e0`) against vectors from the reference
//! running in Ghidra's emulator (`scripts/emulate_golden.py material_at`). The maps, material
//! parameters and rules are random, not game data.

use std::collections::HashMap;

use worldgen::rules::{Rule, RuleList};
use worldgen::terrain::{Image, WangBiome, WangMap, WangParams};

const GOLDEN: &str = include_str!("data/material_at_golden.txt");

fn num(s: &str) -> u64 {
    match s.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16).unwrap(),
        None => s.parse().unwrap(),
    }
}

fn float(s: &str) -> f32 {
    f32::from_bits(num(s) as u32)
}

fn parse_rule(line: &str) -> Rule {
    let f: Vec<&str> = line.split(' ').skip(1).collect();
    let flag = |i: usize| num(f[i]) != 0;
    let n = num(f[25]) as usize;
    Rule {
        material: num(f[0]) as u32,
        min: float(f[1]),
        max: float(f[2]),
        add_perlin: flag(3),
        add_perlin_scale: [float(f[4]), float(f[5])],
        is_rare: flag(6),
        limit_y: flag(7),
        limit_min_y: float(f[8]),
        limit_max_y: float(f[9]),
        rare_use_perlin: flag(10),
        rare_use_fbm_perlin: flag(11),
        rare_use_polka: flag(12),
        rare_scale: [float(f[13]), float(f[14])],
        rare_offset_by_seed: flag(15),
        rare_offset: [float(f[16]), float(f[17])],
        polka_radius_low: float(f[18]),
        polka_radius_high: float(f[19]),
        polka_boxed: flag(20),
        polka_probability: float(f[21]),
        rare_required_min: float(f[22]),
        rare_required_max: float(f[23]),
        is_polygon: flag(24),
        polygon: (0..n)
            .map(|k| [float(f[26 + 2 * k]), float(f[27 + 2 * k])])
            .collect(),
    }
}

#[derive(Default)]
struct Case {
    biome: WangBiome,
    params: Vec<WangParams>,
}

#[test]
fn material_at_matches_the_reference() {
    let mut biomes: HashMap<u64, Case> = HashMap::new();
    let mut lines = GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty());
    let (mut checked, mut table_hits, mut rule_hits) = (0, 0, 0);
    let mut bad = Vec::new();
    while let Some(line) = lines.next() {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "biome" => {
                let (w, h): (i32, i32) = (f[5].parse().unwrap(), f[6].parse().unwrap());
                let n = (w * h) as usize;
                let values = f[7..7 + n].iter().map(|s| float(s)).collect();
                let mats = f[7 + n..7 + 2 * n]
                    .iter()
                    .map(|s| s.parse().unwrap())
                    .collect();
                let case = biomes.entry(num(f[1])).or_default();
                case.biome.offset = (f64::from_bits(num(f[2])), f64::from_bits(num(f[3])));
                case.biome.world_seed = f[4].parse().unwrap();
                case.biome.map = WangMap {
                    values: Image::new(w, h, values),
                    materials: Image::new(w, h, mats),
                };
            }
            "cells" => {
                let case = biomes.entry(num(f[1])).or_default();
                case.params = f[2..]
                    .chunks(3)
                    .map(|c| WangParams {
                        noise_percent: float(c[0]),
                        curvature: float(c[1]),
                        noise_type: c[2].parse().unwrap(),
                    })
                    .collect();
            }
            "rules" => {
                let n: usize = f[4].parse().unwrap();
                let rules = (0..n).map(|_| parse_rule(lines.next().unwrap())).collect();
                biomes.entry(num(f[1])).or_default().biome.rules = RuleList {
                    lo: float(f[2]),
                    hi: float(f[3]),
                    rules,
                };
            }
            "case" => {
                let case = &biomes[&num(f[1])];
                let (x, y) = (f64::from_bits(num(f[2])), f64::from_bits(num(f[3])));
                let want = num(f[5]) as u32;
                let got = case.biome.material_at(x, y, |i| {
                    case.params.get(i as usize - 1).copied().unwrap_or_default()
                });
                checked += 1;
                match f[4] {
                    "T" if want > 0 => table_hits += 1,
                    "R" if want > 0 => rule_hits += 1,
                    _ => {}
                }
                if got != want {
                    bad.push(format!("biome {} ({x}, {y}): got {got}, want {want}", f[1]));
                }
            }
            other => panic!("unknown record {other}"),
        }
    }
    assert!(
        checked >= 900 && table_hits >= 100 && rule_hits >= 100,
        "{checked} {table_hits} {rule_hits}"
    );
    assert!(
        bad.is_empty(),
        "{} of {checked} differ, first: {:?}",
        bad.len(),
        &bad[..bad.len().min(5)]
    );
}

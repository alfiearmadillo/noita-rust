//! `RuleList::pick` (`0x86d2a0`, with the polka field, fractal noise and polygon test it uses)
//! against vectors from the reference running in Ghidra's emulator
//! (`scripts/emulate_golden.py rules`). The lists are random, not game data.

use worldgen::rules::{Rule, RuleList};

const GOLDEN: &str = include_str!("data/rules_golden.txt");

fn num(s: &str) -> u32 {
    match s.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16).unwrap(),
        None => s.parse().unwrap(),
    }
}

fn float(s: &str) -> f32 {
    f32::from_bits(num(s))
}

fn parse_rule(line: &str) -> Rule {
    let f: Vec<&str> = line.split(' ').skip(1).collect();
    let flag = |i: usize| num(f[i]) != 0;
    let n = num(f[25]) as usize;
    Rule {
        material: num(f[0]),
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

#[test]
fn pick_matches_the_reference() {
    let mut list = RuleList::default();
    let mut seed = 0;
    let mut checked = 0;
    let mut hits = 0;
    let mut rare_hits = 0;
    // Hits by rule variant: fbm, plain simplex, polka, polygon, seed offset, add_perlin.
    let mut variants = [0; 6];
    let mut bad = Vec::new();
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "list" => {
                list = RuleList {
                    lo: float(f[2]),
                    hi: float(f[3]),
                    rules: Vec::new(),
                };
                seed = f[4].parse().unwrap();
            }
            "rule" => list.rules.push(parse_rule(line)),
            "case" => {
                let (x, y): (i32, i32) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                let v = float(f[3]);
                let want = num(f[4]);
                let got = list.pick(x, y, v, seed);
                checked += 1;
                if want != 0 {
                    hits += 1;
                    if let Some(r) = list.rules.iter().find(|r| r.material == want) {
                        if r.is_rare {
                            rare_hits += 1;
                            variants[0] += usize::from(r.rare_use_perlin && r.rare_use_fbm_perlin);
                            variants[1] += usize::from(r.rare_use_perlin && !r.rare_use_fbm_perlin);
                            variants[2] += usize::from(r.rare_use_polka);
                            variants[4] += usize::from(r.rare_offset_by_seed);
                        }
                        variants[3] += usize::from(r.is_polygon);
                        variants[5] += usize::from(r.add_perlin);
                    }
                }
                if got != want {
                    bad.push(format!(
                        "({x}, {y}, {v}) seed {seed}: got {got}, want {want}"
                    ));
                }
            }
            other => panic!("unknown record {other}"),
        }
    }
    assert!(
        checked >= 900 && hits >= 200 && rare_hits >= 40,
        "{checked} {hits} {rare_hits}"
    );
    assert!(variants.iter().all(|&n| n >= 5), "{variants:?}");
    assert!(
        bad.is_empty(),
        "{} of {checked} differ, first: {:?}",
        bad.len(),
        &bad[..bad.len().min(4)]
    );
}

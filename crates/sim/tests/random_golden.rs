//! The `Random` class and its helpers against vectors produced by running the reference
//! functions in Ghidra's emulator (`scripts/emulate_golden.py random`).

use sim::random::{Random, mix};

const GOLDEN: &str = include_str!("data/random_golden.csv");

fn hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

fn with_state(state: i64) -> Random {
    if state == 0 {
        Random::zero()
    } else {
        Random::new((state - 1) as u32)
    }
}

fn f32_of(s: &str) -> f32 {
    f32::from_bits(hex(s) as u32)
}

#[test]
fn the_random_class_matches_the_reference_functions() {
    let mut counts = std::collections::BTreeMap::new();
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split(',').collect();
        let kind = f[0];
        *counts.entry(kind).or_insert(0) += 1;
        match kind {
            "int" => {
                let mut r = with_state(f[1].parse().unwrap());
                let got = r.int(f[2].parse().unwrap(), f[3].parse().unwrap());
                assert_eq!(got, f[4].parse::<i32>().unwrap(), "{line}");
                assert_eq!(i64::from(r.state()), f[5].parse::<i64>().unwrap(), "{line}");
            }
            "float" => {
                let mut r = with_state(f[1].parse().unwrap());
                let got = r.float(f32_of(f[2]), f32_of(f[3]));
                assert_eq!(got.to_bits(), f32_of(f[4]).to_bits(), "{line}");
                assert_eq!(i64::from(r.state()), f[5].parse::<i64>().unwrap(), "{line}");
            }
            "unit" => {
                let mut r = with_state(f[1].parse().unwrap());
                let got = r.next_unit();
                assert_eq!(got.to_bits(), hex(f[2]), "{line}");
                assert_eq!(i64::from(r.state()), f[3].parse::<i64>().unwrap(), "{line}");
            }
            "from_seed" => {
                let r = Random::from_seed(f64::from_bits(hex(f[1])));
                assert_eq!(i64::from(r.state()), f[2].parse::<i64>().unwrap(), "{line}");
            }
            "mix" => {
                let (a, b, c): (u32, u32, u32) = (
                    f[1].parse().unwrap(),
                    f[2].parse().unwrap(),
                    f[3].parse().unwrap(),
                );
                assert_eq!(mix(a, b, c), f[4].parse::<u32>().unwrap(), "{line}");
            }
            "dist" => {
                let mut r = with_state(f[1].parse().unwrap());
                let got = r.distribution_unit(f32_of(f[2]), f32_of(f[3]), f32_of(f[4]));
                assert_eq!(got.to_bits(), f32_of(f[5]).to_bits(), "{line}");
                assert_eq!(i64::from(r.state()), f[6].parse::<i64>().unwrap(), "{line}");
            }
            "distf" => {
                let mut r = with_state(f[1].parse().unwrap());
                let got = r.distribution_float(
                    f32_of(f[2]),
                    f32_of(f[3]),
                    f32_of(f[4]),
                    f32_of(f[5]),
                    f32_of(f[6]),
                );
                assert_eq!(got.to_bits(), f32_of(f[7]).to_bits(), "{line}");
                assert_eq!(i64::from(r.state()), f[8].parse::<i64>().unwrap(), "{line}");
            }
            other => panic!("unknown kind {other}"),
        }
    }
    assert!(counts.values().all(|&n| n >= 40), "{counts:?}");
}

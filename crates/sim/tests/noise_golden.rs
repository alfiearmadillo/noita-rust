//! `simplex3` against vectors produced by running the reference noise function (`0x872560`) in
//! Ghidra's emulator (`scripts/emulate_golden.py noise`).

use sim::noise::simplex3;

const GOLDEN: &str = include_str!("data/noise_golden.csv");

fn hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

#[test]
fn simplex3_matches_the_reference_function() {
    let mut worst = 0.0f64;
    let mut mismatches = Vec::new();
    let mut n = 0;
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split(',').collect();
        let [x, y, z] = [f[0], f[1], f[2]].map(|s| f64::from_bits(hex(s)));
        let want = f64::from_bits(hex(f[3]));
        let got = simplex3(x, y, z);
        n += 1;
        if got.to_bits() != want.to_bits() {
            worst = worst.max((got - want).abs());
            mismatches.push(format!("({x}, {y}, {z}): got {got}, want {want}"));
        }
    }
    assert!(n >= 400);
    assert!(
        mismatches.is_empty(),
        "{} of {n} differ (worst {worst:e}); first: {:?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(3)]
    );
}

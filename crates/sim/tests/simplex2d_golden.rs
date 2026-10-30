//! `simplex2d` (`0x872300`) against vectors produced by running the reference code in Ghidra's
//! emulator (`scripts/emulate_golden.py simplex2d`).

use sim::noise::simplex2d;

const GOLDEN: &str = include_str!("data/simplex2d_golden.csv");

fn bits(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

#[test]
fn simplex2d_matches_the_reference() {
    let mut n = 0;
    let mut bad = Vec::new();
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split(',').collect();
        let (x, y) = (f64::from_bits(bits(f[0])), f64::from_bits(bits(f[1])));
        let want = f64::from_bits(bits(f[2]));
        let got = simplex2d(x, y);
        n += 1;
        if got.to_bits() != want.to_bits() {
            bad.push(format!("({x}, {y}): got {got}, want {want}"));
        }
    }
    assert!(n >= 400, "{n}");
    assert!(
        bad.is_empty(),
        "{} differ, first: {:?}",
        bad.len(),
        &bad[..bad.len().min(4)]
    );
}

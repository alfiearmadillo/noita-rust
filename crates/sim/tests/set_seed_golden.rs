//! `Random::set_seed` against vectors produced by running the reference's `SetRandomSeed`
//! (`0x485890`) in Ghidra's emulator (`scripts/emulate_golden.py set_seed`).

use sim::random::Random;

const GOLDEN: &str = include_str!("data/set_seed_golden.csv");

fn hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

#[test]
fn set_seed_matches_the_reference_function() {
    let mut checked = 0;
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split(',').collect();
        let seed: i32 = f[0].parse().unwrap();
        let (x, y) = (f64::from_bits(hex(f[1])), f64::from_bits(hex(f[2])));
        let want = f64::from_bits(hex(f[3]));
        let mut r = Random::zero();
        r.set_seed(seed, x, y);
        assert_eq!(f64::from(r.state()), want, "seed {seed} x {x} y {y}");
        checked += 1;
    }
    assert!(checked >= 300, "only {checked} vectors");
}

//! `world_to_wang` against vectors produced by running `0x87ce50` in Ghidra's emulator
//! (`scripts/emulate_golden.py warp`; the `floor` import is replaced by an integer stub).

use worldgen::warp::world_to_wang;

const GOLDEN: &str = include_str!("data/warp_golden.csv");

fn bits64(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

#[test]
fn world_to_wang_matches_the_reference_function() {
    let mut n = 0;
    let mut bad = Vec::new();
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split(',').collect();
        let off = (f64::from_bits(bits64(f[0])), f64::from_bits(bits64(f[1])));
        let strength = f32::from_bits(bits64(f[2]) as u32);
        let (x, y) = (f64::from_bits(bits64(f[3])), f64::from_bits(bits64(f[4])));
        let want = (
            f32::from_bits(bits64(f[5]) as u32),
            f32::from_bits(bits64(f[6]) as u32),
        );
        let got = world_to_wang(off, strength, x, y);
        n += 1;
        if got.0.to_bits() != want.0.to_bits() || got.1.to_bits() != want.1.to_bits() {
            bad.push(format!(
                "s={strength} ({x}, {y}): got {got:?}, want {want:?}"
            ));
        }
    }
    assert!(n >= 400);
    assert!(
        bad.is_empty(),
        "{} differ, first: {:?}",
        bad.len(),
        &bad[..bad.len().min(3)]
    );
}

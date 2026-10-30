//! `generate_biome` on the reference files (skipped when they are not extracted).

use std::path::PathBuf;

use worldgen::generate::{generate_biome, load_materials, spawn_colors};
use worldgen::world::region_wang_seed;

fn files() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files");
    p.join("data/materials.xml").exists().then_some(p)
}

#[test]
fn spawn_function_colors_are_the_written_rgb() {
    let colors = spawn_colors(
        "RegisterSpawnFunction( 0xff0000ff, \"spawn_nest\" )\n\
         RegisterSpawnFunction( 0xffB40000, \"spawn_fungi\" )\n-- RegisterSpawnFunction( 1, \"x\" )",
    );
    assert_eq!(colors.len(), 2);
    assert_eq!(colors[&0x0000ff], "spawn_nest");
    assert_eq!(colors[&0xb40000], "spawn_fungi");
}

#[test]
fn region_seed_is_a_later_draw_of_the_world_seed_chain() {
    // Independent check with the plain modulus instead of the engine's Schrage form.
    let step = |s: i64| s * 16807 % 2_147_483_647;
    for (seed, width, attempt) in [
        (12345u32, 300usize, 0u32),
        (336_730_580, 1024, 0),
        (7, 5, 2),
    ] {
        let mut s = f64::from(seed);
        if s >= 2_147_483_647.0 {
            s *= 0.5;
        }
        let mut s = step(s as i64);
        for _ in 0..width % 11 + (seed % 12) as usize + attempt as usize + 1 {
            s = step(s);
        }
        let want = s as f64 * 4.656_612_875e-10 * 2_147_483_645.0;
        assert_eq!(region_wang_seed(seed, width, attempt), want, "{seed}");
    }
}
#[test]
fn coal_mines_generate_deterministically_with_caves() {
    let Some(files) = files() else { return };
    let defs = load_materials(&files).unwrap();
    let gen_with =
        |seed: u32| generate_biome(&files, &defs, "coalmine", seed, (64, 64), (256, 192)).unwrap();
    let a = gen_with(12345);
    assert_eq!(a, gen_with(12345));
    assert_ne!(a.materials, gen_with(777).materials);
    let air = a.materials.iter().filter(|&&m| m == 0).count();
    let total = a.materials.len();
    assert!(
        air > total / 10 && air < total * 9 / 10,
        "air share {air}/{total}"
    );
    // The map contains solid rock and ground, not only air and one material.
    let mut kinds: Vec<u16> = a.materials.clone();
    kinds.sort_unstable();
    kinds.dedup();
    assert!(kinds.len() >= 3, "{kinds:?}");
}

//! `generate_biome` on the reference files (skipped when they are not extracted).

use std::path::PathBuf;

use worldgen::generate::{generate_biome, load_materials, spawn_colors};

fn files() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files");
    p.join("data/materials.xml").exists().then_some(p)
}

#[test]
fn spawn_function_colors_are_abgr_words() {
    let colors = spawn_colors(
        "RegisterSpawnFunction( 0xff0000ff, \"spawn_nest\" )\n\
         RegisterSpawnFunction( 0xffB40000, \"spawn_fungi\" )\n-- RegisterSpawnFunction( 1, \"x\" )",
    );
    assert_eq!(colors.len(), 2);
    assert_eq!(colors[&0xff0000], "spawn_nest");
    assert_eq!(colors[&0x0000b4], "spawn_fungi");
}

#[test]
fn coal_mines_generate_deterministically_with_caves() {
    let Some(files) = files() else { return };
    let defs = load_materials(&files).unwrap();
    let gen_with = |seed: f64| {
        generate_biome(
            &files,
            &defs,
            "coalmine",
            seed,
            (400, 300),
            (256, 192),
            (3, 3),
        )
        .unwrap()
    };
    let a = gen_with(12345.0);
    assert_eq!(a, gen_with(12345.0));
    assert_ne!(a.materials, gen_with(777.0).materials);
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

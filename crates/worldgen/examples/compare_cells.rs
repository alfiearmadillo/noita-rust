//! Compares the cells of a running game with the ported base terrain: dump the world seed with
//! `noita_probe biomes <dir>` and a rectangle with `noita_probe grid <x> <y> <w> <h> --tsv grid.tsv`
//! (pause the game first), then
//! `cargo run --release -p worldgen --example compare_cells -- <dir> grid.tsv <x> <y> <w> <h>`.
//! Cells of other biomes, vegetation, structures and moved liquids make the games differ.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use worldgen::generate::load_materials;
use worldgen::world::WorldTerrain;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(&a[0]);
    let num = |i: usize| a[i].parse::<i32>().expect("number");
    let (x0, y0, w, h) = (num(2), num(3), num(4), num(5));
    let files = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files");
    let world = std::fs::read_to_string(dir.join("world.txt")).expect("world.txt");
    let seed: i32 = world
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("seed "))
        .and_then(|s| s.parse().ok())
        .expect("seed");
    let tsv = std::fs::read_to_string(&a[1]).expect("grid tsv");
    let mut live: HashMap<(i32, i32), String> = HashMap::new();
    for line in tsv.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() > 4 {
            live.insert(
                (f[0].parse().unwrap(), f[1].parse().unwrap()),
                f[4].to_owned(),
            );
        }
    }

    let defs = load_materials(&files).expect("materials");
    let terrain = WorldTerrain::new(&files, &defs, seed as u32).expect("terrain");
    let (mut same, mut other_biome) = (0_usize, 0_usize);
    let mut diffs: BTreeMap<(String, String), usize> = BTreeMap::new();
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let theirs = live.get(&(x, y)).map_or("air", String::as_str);
            let Some(index) = terrain.material_at(x, y) else {
                other_biome += 1;
                continue;
            };
            let ours = terrain.name(index);
            if ours == theirs {
                same += 1;
            } else {
                *diffs
                    .entry((theirs.to_owned(), ours.to_owned()))
                    .or_default() += 1;
            }
        }
    }
    let total = (w * h) as usize;
    let compared = total - other_biome;
    println!(
        "seed {seed}: {same} of {compared} cells equal ({} differ, {other_biome} in biomes that are not ported)",
        compared - same
    );
    let mut top: Vec<_> = diffs.into_iter().collect();
    top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for ((theirs, ours), n) in top.iter().take(15) {
        println!("  game {theirs:<28} port {ours:<28} {n}");
    }
}

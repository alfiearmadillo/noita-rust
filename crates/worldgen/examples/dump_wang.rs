//! Prints the structure of a Wang template (not its pixels) for the emulator checks:
//! `cargo run -p worldgen --example dump_wang -- <template.png>`.
//! Output: `corner num_color[6] side nh nv`, then one line of six colors per tile (h tiles, then v tiles).

use std::path::PathBuf;

use noita_data::sim_materials::load_png;
use worldgen::wang::Tileset;

fn main() {
    let name = std::env::args().nth(1).expect("template file name");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files");
    let tex = load_png(&root.join("data/wang_tiles").join(&name)).expect("template");
    let (w, h) = (tex.width() as usize, tex.height() as usize);
    let rgb: Vec<u8> = tex
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let ts = Tileset::from_image(&rgb, w, h).expect("tileset");
    let nc = ts.num_color;
    println!(
        "{} {} {} {} {} {} {} {} {} {}",
        u8::from(ts.is_corner),
        nc[0],
        nc[1],
        nc[2],
        nc[3],
        nc[4],
        nc[5],
        ts.short_side_len,
        ts.h_tiles.len(),
        ts.v_tiles.len()
    );
    for t in ts.h_tiles.iter().chain(&ts.v_tiles) {
        println!("{}", t.colors.map(|c| c.to_string()).join(" "));
    }
}

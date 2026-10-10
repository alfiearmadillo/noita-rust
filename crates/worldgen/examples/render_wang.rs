//! Renders a generated Wang map to a PNG: `cargo run -p worldgen --example render_wang -- <template> <seed> <out.png>`.

use std::path::PathBuf;

use noita_data::sim_materials::load_png;
use worldgen::wang::Tileset;

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "coalmine.png".into());
    let seed: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(12345.0);
    let out = args.next().unwrap_or_else(|| "wang.png".into());
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
    let map = ts.generate_map(seed, 512, 512).expect("map");
    let file = std::fs::File::create(&out).expect("create output");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 512, 512);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("header")
        .write_image_data(&map)
        .expect("data");
    println!("wrote {out}");
}

//! Renders a biome's terrain from the Wang template: `cargo run -p worldgen --release --example
//! render_biome -- <biome> <seed> <chunks_w> <chunks_h> <out.png>` (for example `coalmine 12345 3 2`).
//!
//! The Wang map comes from the verified generator, the colors become material indices as
//! `0x8704c0` does (emulator-checked except spawn colors), the lookup is the verified
//! `0x87d0e0` Wang branch. Spawn-function pixels, pixel scenes and structures are not placed.

use std::collections::HashMap;
use std::path::PathBuf;

use worldgen::generate::{generate_biome, load_materials};

fn main() {
    let mut args = std::env::args().skip(1);
    let biome = args.next().unwrap_or_else(|| "coalmine".into());
    let seed: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(12345.0);
    let cw: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let ch: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(2);
    let out = args.next().unwrap_or_else(|| "biome.png".into());
    let files = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files");

    let defs = load_materials(&files).expect("materials");
    let (pw, ph) = (cw * 512, ch * 512);
    let area =
        generate_biome(&files, &defs, &biome, seed, (0, 0), (pw, ph), (cw, ch)).expect("generate");
    println!(
        "{biome}: {pw}x{ph} pixels, {} spawn-function pixels",
        area.spawn_pixels
    );

    let mut counts: HashMap<u16, usize> = HashMap::new();
    let mut pixels = vec![0u8; pw * ph * 3];
    for (i, &m) in area.materials.iter().enumerate() {
        *counts.entry(m).or_default() += 1;
        let c = if m == 0 {
            0x20_2028
        } else {
            defs[usize::from(m)].graphics_color.unwrap_or(0xff80_8080) & 0xff_ffff
        };
        pixels[i * 3] = (c >> 16) as u8;
        pixels[i * 3 + 1] = (c >> 8) as u8;
        pixels[i * 3 + 2] = c as u8;
    }
    let mut top: Vec<_> = counts.into_iter().collect();
    top.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    for (m, n) in top.iter().take(12) {
        println!("  {:>7} {}", n, defs[usize::from(*m)].name);
    }
    let file = std::fs::File::create(&out).expect("create output");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), pw as u32, ph as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("header")
        .write_image_data(&pixels)
        .expect("data");
    println!("wrote {out}");
}

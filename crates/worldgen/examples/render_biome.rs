//! Renders a biome's terrain from the Wang template: `cargo run -p worldgen --release --example
//! render_biome -- <biome> <seed> <chunks_w> <chunks_h> <out.png>` (for example `coalmine 12345 3 2`).
//!
//! The Wang map comes from the verified generator, the colors become material indices as
//! `0x8704c0` does (decompile only, not emulator-checked), the lookup is the verified
//! `0x87d0e0` Wang branch. Spawn-function pixels, pixel scenes and structures are not placed.

use std::collections::HashMap;
use std::path::PathBuf;

use noita_data::materials;
use noita_data::sim_materials::load_png;
use worldgen::biome;
use worldgen::terrain::{ColorTable, WangBiome, WangMap, WangParams};
use worldgen::wang::Tileset;

/// `RegisterSpawnFunction( 0x........, "name" )` colors of a biome script, as `0xRRGGBB`
/// assuming the word is `0xAABBGGRR`.
fn spawn_colors(lua: &str) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    for line in lua.lines() {
        let Some(rest) = line.trim().strip_prefix("RegisterSpawnFunction(") else {
            continue;
        };
        let Some((color, name)) = rest.split_once(',') else {
            continue;
        };
        let color = color.trim().trim_start_matches("0x");
        let Ok(word) = u32::from_str_radix(color, 16) else {
            continue;
        };
        let name = name.trim().trim_start_matches('"');
        let name = name.split('"').next().unwrap_or_default();
        let (r, g, b) = (word & 0xff, (word >> 8) & 0xff, (word >> 16) & 0xff);
        out.insert(r << 16 | g << 8 | b, name.to_owned());
    }
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let biome_name = args.next().unwrap_or_else(|| "coalmine".into());
    let seed: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(12345.0);
    let cw: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let ch: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(2);
    let out = args.next().unwrap_or_else(|| "biome.png".into());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files/data");

    let defs = materials::parse(&std::fs::read_to_string(root.join("materials.xml")).unwrap())
        .expect("materials");
    let index_by_name: HashMap<&str, u32> = defs
        .iter()
        .enumerate()
        .rev()
        .map(|(i, d)| (d.name.as_str(), i as u32))
        .collect();
    let wang_color = |i: usize| {
        defs[i]
            .attr("wang_color")
            .and_then(|c| u32::from_str_radix(c.trim(), 16).ok())
            .unwrap_or(0)
    };
    let colors = ColorTable::new((0..defs.len()).map(wang_color).collect());
    let params: Vec<WangParams> = defs
        .iter()
        .map(|d| {
            let mut p = WangParams::default();
            if let Some(v) = d.attr_f32("wang_noise_percent") {
                p.noise_percent = v;
            }
            if let Some(v) = d.attr_f32("wang_curvature") {
                p.curvature = v;
            }
            if let Some(v) = d
                .attr("wang_noise_type")
                .and_then(|v| v.trim().parse().ok())
            {
                p.noise_type = v;
            }
            p
        })
        .collect();

    let xml = std::fs::read_to_string(root.join("biome").join(format!("{biome_name}.xml")))
        .expect("biome xml");
    let (topology, rules) =
        biome::parse(&xml, |n| index_by_name.get(n).copied().unwrap_or(0)).expect("biome");
    println!(
        "{}: {} rules, window [{}, {}], template {:?}",
        topology.name,
        rules.rules.len(),
        rules.lo,
        rules.hi,
        topology.wang_template_file
    );

    let template = topology.wang_template_file.expect("wang template");
    let tex = load_png(&root.join("..").join(&template)).expect("template png");
    let rgb: Vec<u8> = tex
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let tileset =
        Tileset::from_image(&rgb, tex.width() as usize, tex.height() as usize).expect("tileset");
    let (w, h) = (
        (cw * 512 / 10).clamp(1, 1024),
        (ch * 512 / 10).clamp(1, 1024),
    );
    let map_rgb = tileset.generate_map(seed, w, h).expect("wang map");

    let spawn = topology
        .lua_script
        .as_ref()
        .and_then(|s| std::fs::read_to_string(root.join("..").join(s)).ok())
        .map(|s| spawn_colors(&s))
        .unwrap_or_default();
    let spawn_index: HashMap<u32, u32> = spawn
        .keys()
        .enumerate()
        .map(|(i, c)| (*c, i as u32))
        .collect();
    let converted =
        WangMap::from_wang_rgb(&map_rgb, w, h, &colors, |c| spawn_index.get(&c).copied());
    let spawn_pixels = converted.spawn.data.iter().filter(|&&s| s != 0).count();
    println!("{w}x{h} wang pixels, {spawn_pixels} spawn-function pixels");

    let biome = WangBiome {
        map: converted.map,
        rules,
        offset: (0.0, 0.0),
        world_seed: seed as i32,
    };
    let (pw, ph) = (cw * 512, ch * 512);
    let mut pixels = vec![0u8; pw * ph * 3];
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for y in 0..ph {
        for x in 0..pw {
            let m = biome.material_at(x as f64, y as f64, |i| {
                params.get(i as usize).copied().unwrap_or_default()
            });
            *counts.entry(m).or_default() += 1;
            let c = if m == 0 {
                0x20_2028
            } else {
                let g = defs[m as usize].graphics_color.unwrap_or(0xff80_8080);
                g & 0xff_ffff
            };
            let o = (y * pw + x) * 3;
            pixels[o] = (c >> 16) as u8;
            pixels[o + 1] = (c >> 8) as u8;
            pixels[o + 2] = c as u8;
        }
    }
    let mut top: Vec<_> = counts.into_iter().collect();
    top.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    for (m, n) in top.iter().take(12) {
        println!("  {:>7} {}", n, defs[*m as usize].name);
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

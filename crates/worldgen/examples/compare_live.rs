//! Compares the world-size Wang image of a running game with the ported one:
//! `noita_probe biomes <dir>` in the game, then
//! `cargo run --release -p worldgen --example compare_live -- <dir> [n.txt]`.
//! Prints the offset and, per region of the biome map, how many pixels differ.

use std::path::PathBuf;

use worldgen::generate::load_materials;
use worldgen::world::build_world_map;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("dump directory"));
    let which = args.next().unwrap_or_else(|| "42.txt".to_owned());
    let list: Option<usize> = args.next().and_then(|s| s.parse().ok());
    let files = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files");
    let world = std::fs::read_to_string(dir.join("world.txt")).expect("world.txt");
    let mut lines = world.lines();
    let seed: i32 = lines
        .next()
        .and_then(|l| l.strip_prefix("seed "))
        .and_then(|s| s.parse().ok())
        .expect("seed");
    let offset_bits: Vec<u64> = lines
        .next()
        .and_then(|l| l.strip_prefix("offset "))
        .expect("offset")
        .split(' ')
        .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap())
        .collect();
    let live_offset = (
        f64::from_bits(offset_bits[0]),
        f64::from_bits(offset_bits[1]),
    );

    let defs = load_materials(&files).expect("materials");
    let ported = build_world_map(&files, &defs, seed as u32).expect("world map");
    println!(
        "world seed {seed}\nlive offset {live_offset:?}\nported      {:?}  {}",
        ported.offset,
        if ported.offset == live_offset {
            "MATCH"
        } else {
            "DIFFERENT"
        }
    );

    let text = std::fs::read_to_string(dir.join(&which)).expect("dump file");
    let mut l = text.lines();
    let head: Vec<&str> = l.next().unwrap().split(' ').collect();
    let (file, w, h) = (
        head[0],
        head[2].parse::<i32>().unwrap(),
        head[3].parse::<i32>().unwrap(),
    );
    let values: Vec<u32> = l
        .next()
        .unwrap()
        .split(' ')
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let mats: Vec<u16> = l
        .next()
        .unwrap()
        .split(' ')
        .map(|s| s.parse().unwrap())
        .collect();
    let map = &ported.images.map;
    println!(
        "{file}: live {w}x{h}, ported {}x{}",
        map.values.width, map.values.height
    );
    if (map.values.width, map.values.height) != (w, h) {
        return;
    }
    let differs = |i: usize| {
        (
            map.values.data[i].to_bits() != values[i],
            map.materials.data[i] != mats[i],
        )
    };
    let (mut fl, mut mt) = (0_usize, 0_usize);
    let mut float_pairs: std::collections::HashMap<(u32, u32), usize> = Default::default();
    let mut mat_pairs: std::collections::HashMap<(u16, u16), usize> = Default::default();
    for i in 0..(w * h) as usize {
        let (f, m) = differs(i);
        fl += usize::from(f);
        mt += usize::from(m);
        if f {
            *float_pairs
                .entry((values[i], map.values.data[i].to_bits()))
                .or_default() += 1;
        }
        if m {
            *mat_pairs
                .entry((mats[i], map.materials.data[i]))
                .or_default() += 1;
        }
    }
    let mut fp: Vec<_> = float_pairs.into_iter().collect();
    fp.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for ((l, p), n) in fp.iter().take(10) {
        println!(
            "  float live {:.4} ported {:.4}: {n}",
            f32::from_bits(*l),
            f32::from_bits(*p)
        );
    }
    let mut mp: Vec<_> = mat_pairs.into_iter().collect();
    mp.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for ((l, p), n) in mp.iter().take(10) {
        println!("  material live {l} ported {p}: {n}");
    }
    println!(
        "whole image: {fl} float / {mt} material pixels differ of {}",
        w * h
    );
    let mut rf: std::collections::HashMap<(u32, u32, u16), usize> = Default::default();
    let mut rm: std::collections::HashMap<(u16, u16), usize> = Default::default();
    for (n, r) in ported.regions.iter().enumerate() {
        let (x0, y0) = (r.x0 * 512 / 10, r.y0 * 512 / 10);
        let (x1, y1) = (
            ((r.x1 + 1) * 512 / 10) as i32,
            ((r.y1 + 1) * 512 / 10) as i32,
        );
        let (mut cells, mut fl, mut mt) = (0_usize, 0_usize, 0_usize);
        let mut bb = (i32::MAX, i32::MAX, 0, 0);
        for y in y0 as i32..y1.min(h) {
            for x in x0 as i32..x1.min(w) {
                let i = (y * w + x) as usize;
                let (f, m) = differs(i);
                cells += 1;
                fl += usize::from(f);
                mt += usize::from(m);
                if list == Some(n) {
                    if f {
                        bb = (bb.0.min(x), bb.1.min(y), bb.2.max(x), bb.3.max(y));
                        *rf.entry((values[i], map.values.data[i].to_bits(), mats[i]))
                            .or_default() += 1;
                    }
                    if m {
                        *rm.entry((mats[i], map.materials.data[i])).or_default() += 1;
                    }
                }
            }
        }
        if list == Some(n) {
            println!("    float diff bbox {bb:?}, region origin ({x0},{y0})");
            let mut a: Vec<_> = std::mem::take(&mut rf).into_iter().collect();
            a.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            for ((l, p, m), n) in a.iter().take(12) {
                println!(
                    "    float live {:.4} ported {:.4} (material {m}): {n}",
                    f32::from_bits(*l),
                    f32::from_bits(*p)
                );
            }
            let mut a: Vec<_> = std::mem::take(&mut rm).into_iter().collect();
            a.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            for ((l, p), n) in a.iter().take(12) {
                println!("    material live {l} ported {p}: {n}");
            }
        }
        println!(
            "region {n:>3} {:<40} chunks ({},{})-({},{}): {fl} float / {mt} material of {cells}",
            ported.entries.biomes[r.biome].file, r.x0, r.y0, r.x1, r.y1
        );
    }

    if let Some(n) = std::env::var("ATTEMPTS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
    {
        let count = ported.regions.len();
        let mut best = vec![(usize::MAX, 0_u32); count];
        for k in 0..n {
            let world = worldgen::world::build_world_map_with(&files, &defs, seed as u32, |_| k)
                .expect("world map");
            let map = &world.images.map;
            for (i, r) in world.regions.iter().enumerate() {
                let (x0, y0) = (r.x0 * 512 / 10, r.y0 * 512 / 10);
                let (x1, y1) = (
                    ((r.x1 + 1) * 512 / 10) as i32,
                    ((r.y1 + 1) * 512 / 10) as i32,
                );
                let mut d = 0;
                for y in y0 as i32..y1.min(h) {
                    for x in x0 as i32..x1.min(w) {
                        let j = (y * w + x) as usize;
                        d += usize::from(map.values.data[j].to_bits() != values[j])
                            + usize::from(map.materials.data[j] != mats[j]);
                    }
                }
                if d < best[i].0 {
                    best[i] = (d, k);
                }
            }
        }
        for (i, r) in ported.regions.iter().enumerate() {
            let (rw, rh) = (
                (r.x1 + 1) * 512 / 10 - r.x0 * 512 / 10,
                (r.y1 + 1) * 512 / 10 - r.y0 * 512 / 10,
            );
            println!(
                "region {i:>3} {:<36} {rw}x{rh}: best attempt {} ({} px differ)",
                ported.entries.biomes[r.biome].file, best[i].1, best[i].0
            );
        }
    }
}

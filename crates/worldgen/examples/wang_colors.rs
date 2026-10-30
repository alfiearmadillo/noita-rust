//! Lists the colors used by the Wang templates and how they resolve against `materials.xml`
//! `wang_color` values: `cargo run -p worldgen --example wang_colors`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use noita_data::materials;
use noita_data::sim_materials::load_png;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files/data");
    let defs = materials::parse(&std::fs::read_to_string(root.join("materials.xml")).unwrap())
        .expect("materials");
    // The reference scans the table from index 1 for the first RGB match; index 0 is air.
    let mut by_color: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, d) in defs.iter().enumerate() {
        if let Some(c) = d.attr("wang_color") {
            let c = u32::from_str_radix(c.trim(), 16).unwrap() & 0xff_ffff;
            by_color.entry(c).or_default().push(i);
        }
    }
    let dup: Vec<_> = by_color.iter().filter(|(_, v)| v.len() > 1).collect();
    println!(
        "{} materials, {} distinct wang colors, {} duplicated",
        defs.len(),
        by_color.len(),
        dup.len()
    );
    for (c, v) in dup {
        let names: Vec<_> = v.iter().map(|&i| defs[i].name.as_str()).collect();
        println!("  dup {c:06x}: {names:?}");
    }
    println!("air: {:?}", defs[0].attr("wang_color"));

    let mut total: BTreeMap<u32, u64> = BTreeMap::new();
    let mut files = 0;
    for e in std::fs::read_dir(root.join("wang_tiles")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|s| s.to_str()) != Some("png") {
            continue;
        }
        let tex = load_png(&p).unwrap();
        files += 1;
        for px in tex.rgba().as_chunks::<4>().0 {
            let c = u32::from(px[0]) << 16 | u32::from(px[1]) << 8 | u32::from(px[2]);
            *total.entry(c).or_default() += 1;
        }
    }
    println!("{files} templates, {} distinct colors", total.len());
    let mut unknown: Vec<_> = total
        .iter()
        .filter(|(c, _)| !by_color.contains_key(c))
        .collect();
    unknown.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("colors without a material: {}", unknown.len());
    for (c, n) in unknown.iter().take(25) {
        println!("  {c:06x} x{n}");
    }
    let known: u64 = total
        .iter()
        .filter(|(c, _)| by_color.contains_key(c))
        .map(|(_, n)| n)
        .sum();
    let all: u64 = total.values().sum();
    println!("pixels with a material color: {known}/{all}");
    let mut top: Vec<_> = total.iter().collect();
    top.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (c, n) in top.iter().take(12) {
        let m = by_color
            .get(c)
            .map(|v| defs[v[0]].name.as_str())
            .unwrap_or("-");
        println!("  top {c:06x} x{n} {m}");
    }
}

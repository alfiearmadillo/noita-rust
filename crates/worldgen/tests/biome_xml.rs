//! The biome XML reader and the Wang image conversion.

use std::path::PathBuf;

use worldgen::biome::{default_rule, parse};
use worldgen::terrain::{ColorTable, WangMap};

#[test]
fn rules_parse_with_the_constructor_defaults() {
    let xml = r#"<Biome>
        <Topology name="t" type="BIOME_WANG_TILE" wang_template_file="a.png" wang_map_width="10" wang_map_height="20"/>
        <Materials name="t">
            <MaterialComponent material_name="rock" material_min="0.2" material_max="0.8"/>
            <MaterialComponent material_name="gem" material_min="-1" material_max="3" is_rare="1"
                limit_y="1" limit_min_y="5" limit_max_y="50" rare_use_polka="0" is_polygon="1">
                <polygon><Vec2 x="0" y="0"/><Vec2 x="1" y="0"/><Vec2 x="0.5" y="1"/></polygon>
            </MaterialComponent>
            <MaterialComponent material_name="unknown" material_min="-5" material_max="9"/>
        </Materials>
    </Biome>"#;
    let (topo, list) = parse(xml, |n| match n {
        "rock" => 1,
        "gem" => 2,
        _ => 0,
    })
    .unwrap();
    assert_eq!(topo.name, "t");
    assert_eq!((topo.wang_map_width, topo.wang_map_height), (10, 20));
    assert_eq!(list.rules.len(), 3);
    // The window covers only rules with a material.
    assert_eq!((list.lo, list.hi), (-1.0, 3.0));
    let d = default_rule();
    assert_eq!(list.rules[0].polka_probability, d.polka_probability);
    assert!(list.rules[0].rare_use_polka && list.rules[0].polka_boxed);
    assert_eq!(list.rules[0].limit_max_y, 2048.0);
    let gem = &list.rules[1];
    assert!(gem.is_rare && gem.limit_y && !gem.rare_use_polka && gem.is_polygon);
    assert_eq!((gem.limit_min_y, gem.limit_max_y), (5.0, 50.0));
    assert_eq!(gem.polygon, vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]);
    assert_eq!(list.rules[2].material, 0);
}

#[test]
fn conversion_follows_the_reference_classes() {
    // Table: air 0x000042, material 1 is red, material 2 is green.
    let colors = ColorTable::new(vec![0x00_0042, 0xff_0000, 0x00_ff00]);
    assert_eq!(colors.index_of(0x00_0042), 0);
    assert_eq!(colors.index_of(0x00_ff00), 2);
    assert_eq!(colors.index_of(0x12_3456), -1);
    // 4x1: black, white, red, spawn color.
    let rgb = [0, 0, 0, 255, 255, 255, 255, 0, 0, 1, 2, 3];
    let out = WangMap::from_wang_rgb(&rgb, 4, 1, &colors, |c| (c == 0x01_0203).then_some(7));
    let v = &out.map.values.data;
    assert_eq!(v[0], 0.0);
    assert_eq!(v[1], 1.0);
    assert_eq!(v[2], 1.0);
    assert_eq!(v[3], 0.0, "spawn pixels leave the float untouched");
    assert_eq!(out.spawn.data, vec![0, 0, 0, 8]);
    // The red pixel is material 1, stored as index + 1 = 2.
    assert_eq!(out.map.materials.data[2], 2);
    // The spawn pixel and the black one have no material and no float, so they take the
    // majority of their neighbours; the image wraps, so the black pixel sees the spawn pixel
    // and white on its sides and nothing else.
    assert_eq!(out.map.materials.data[3], 2);
    assert_eq!(out.map.materials.data[0], 0);
}

#[test]
fn every_wang_biome_loads() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files/data/biome");
    let Ok(dir) = std::fs::read_dir(&root) else {
        return; // no reference files here
    };
    let mut loaded = 0;
    for e in dir.flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("xml") {
            continue;
        }
        let xml = std::fs::read_to_string(&path).unwrap();
        let Ok((topo, list)) = parse(&xml, |_| 1) else {
            continue; // not a biome (modifiers, scene lists)
        };
        if topo.kind == "BIOME_WANG_TILE" {
            // Some biomes (the temple altar) have no rules at all.
            if !list.rules.is_empty() {
                assert!(list.lo <= list.hi, "{path:?}");
            }
            loaded += 1;
        }
    }
    assert!(loaded >= 45, "{loaded}");
}

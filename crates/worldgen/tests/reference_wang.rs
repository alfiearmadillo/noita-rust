//! Wang tile generation against the extracted reference templates; skips without them.

use std::collections::HashSet;
use std::path::PathBuf;

use noita_data::sim_materials::load_png;
use worldgen::wang::{Tileset, WangError, WangRng};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files")
}

fn template(name: &str) -> Option<(Vec<u8>, usize, usize)> {
    let path = root().join("data/wang_tiles").join(name);
    if !path.exists() {
        eprintln!("skipping: reference data not present");
        return None;
    }
    let tex = load_png(&path).expect("template decodes");
    let (w, h) = (tex.width() as usize, tex.height() as usize);
    let rgb = tex
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    Some((rgb, w, h))
}

fn tileset(name: &str) -> Option<Tileset> {
    let (rgb, w, h) = template(name)?;
    Some(Tileset::from_image(&rgb, w, h).expect("tileset builds"))
}

#[test]
fn every_reference_template_builds_and_generates() {
    let Ok(entries) = std::fs::read_dir(root().join("data/wang_tiles")) else {
        eprintln!("skipping: reference data not present");
        return;
    };
    let mut count = 0;
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".png") {
            continue;
        }
        let ts = tileset(&name).unwrap();
        assert!(!ts.h_tiles.is_empty() && !ts.v_tiles.is_empty(), "{name}");
        let map = ts.generate_map(4242.0, 512, 512);
        assert_eq!(map.map(|m| m.len()), Ok(512 * 512 * 3), "{name}");
        count += 1;
    }
    assert!(count >= 30, "only {count} templates found");
}

#[test]
fn template_headers_decode() {
    let Some(ts) = tileset("coalmine.png") else {
        return;
    };
    assert!(ts.is_corner);
    assert_eq!(ts.num_color[..4], [1, 2, 1, 2]);
    assert_eq!(ts.short_side_len, 13);
    assert_eq!((ts.h_tiles.len(), ts.v_tiles.len()), (72, 72));
    for t in &ts.h_tiles {
        assert_eq!(t.pixels.len(), 26 * 13 * 3);
    }
    for t in &ts.v_tiles {
        assert_eq!(t.pixels.len(), 13 * 26 * 3);
    }
    let edge = tileset("winter_caves.png").unwrap();
    assert!(!edge.is_corner);
    assert_eq!(edge.num_color, [3, 3, 2, 2, 1, 1]);
    assert_eq!(edge.short_side_len, 32);
}

#[test]
fn generation_is_deterministic_and_uses_only_template_pixels() {
    let Some(ts) = tileset("coalmine.png") else {
        return;
    };
    let a = ts.generate_map(777.0, 300, 200).unwrap();
    let b = ts.generate_map(777.0, 300, 200).unwrap();
    let c = ts.generate_map(778.0, 300, 200).unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
    let palette: HashSet<[u8; 3]> = ts
        .h_tiles
        .iter()
        .chain(&ts.v_tiles)
        .flat_map(|t| t.pixels.as_chunks::<3>().0.iter().copied())
        .collect();
    assert!(a.as_chunks::<3>().0.iter().all(|p| palette.contains(p)));
}

#[test]
fn generate_map_drops_the_first_four_rows() {
    let Some(ts) = tileset("coalmine.png") else {
        return;
    };
    let full = ts.generate(&mut WangRng::new(31.0), 200, 104).unwrap();
    let map = ts.generate_map(31.0, 200, 100).unwrap();
    assert_eq!(map, full[4 * 200 * 3..]);
}

#[test]
fn oversized_maps_are_rejected() {
    let Some(ts) = tileset("coalmine.png") else {
        return;
    };
    // The scratch arrays hold 100 tiles plus a 6-tile margin.
    assert!(ts.generate_map(1.0, 1300, 100).is_ok());
    assert_eq!(ts.generate_map(1.0, 1313, 100), Err(WangError::MapTooLarge));
}

//! Checks the player sprite against the locally extracted reference data; skips without it.

use std::path::PathBuf;

use noita_data::sim_materials::load_png;
use noita_data::sprites;

#[test]
fn reference_player_sprite() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files");
    let Ok(xml) = std::fs::read_to_string(root.join("data/enemies_gfx/player.xml")) else {
        eprintln!("skipping: reference data not present");
        return;
    };
    let s = sprites::parse(&xml).expect("player.xml parses");
    assert_eq!(s.filename, "data/enemies_gfx/player.png");
    assert_eq!((s.offset_x, s.offset_y), (6.0, 14.0));
    assert_eq!(s.default_animation, "stand");
    let walk = s.animation("walk").expect("walk");
    assert_eq!((walk.frame_count, walk.frame_wait), (6, 0.105));
    assert!(walk.looping);
    assert!(!s.animation("land").expect("land").looping);
    let sheet = load_png(&root.join(&s.filename)).expect("sheet decodes");
    for a in &s.animations {
        for i in 0..a.frame_count {
            let (x, y, w, h) = a.frame_rect(i);
            assert!(
                x + w <= sheet.width() && y + h <= sheet.height(),
                "{} frame {i} outside the sheet",
                a.name
            );
        }
    }
}

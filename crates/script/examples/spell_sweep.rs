//! Casts every spell of the reference `gun_actions.lua` once and reports the ones that fail:
//! `cargo run -p script --example spell_sweep`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use script::gun::{DeckCard, Gun};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files");
    let gun = Gun::new(&root, 12345).expect("gun.lua loads");
    let actions: mlua::Table = gun.runtime().lua().globals().get("actions").unwrap();
    let ids: Vec<String> = actions
        .sequence_values::<mlua::Table>()
        .map(|a| a.unwrap().get::<String>("id").unwrap())
        .collect();
    // Spell actions look at the entity that holds the wand.
    let holder = gun.runtime().with_world(|w| w.create_entity("wand"));
    gun.runtime().set_updated_entity(holder);
    let mut failures: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ok = 0;
    for (n, id) in ids.iter().enumerate() {
        gun.set_frame(100 + n as i32);
        let cards: Vec<DeckCard> = std::iter::once(id.as_str())
            .chain(["LIGHT_BULLET", "LIGHT_BULLET", "LIGHT_BULLET"])
            .enumerate()
            .map(|(i, a)| DeckCard::new(a, i as i32 + 1))
            .collect();
        let result = gun
            .set_deck(&cards, false)
            .and_then(|()| gun.shoot(1000.0, true));
        match result {
            Ok(_) => ok += 1,
            Err(e) => {
                let msg = e.to_string();
                let first = msg.lines().next().unwrap_or("").to_owned();
                failures.entry(first).or_default().push(id.clone());
            }
        }
    }
    println!("{ok} of {} spells cast cleanly", ids.len());
    for (msg, ids) in &failures {
        println!("{msg}\n    {}", ids.join(" "));
    }
}

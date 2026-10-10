//! The wand engine running the reference `gun.lua` and `gun_actions.lua`; skips without them.

use std::path::PathBuf;

use script::gun::{DeckCard, Gun, GunCommand, GunConfig};
use sim::random::ScriptRandom;

const LIGHT_BULLET: &str = "data/entities/projectiles/deck/light_bullet.xml";
const BOMB: &str = "data/entities/projectiles/bomb.xml";

fn gun() -> Option<Gun> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files");
    if !root.join("data/scripts/gun/gun.lua").exists() {
        eprintln!("skipping: reference data not present");
        return None;
    }
    Some(Gun::new(root, 12345).expect("gun.lua loads"))
}

fn deck(ids: &[&str]) -> Vec<DeckCard> {
    ids.iter()
        .enumerate()
        .map(|(i, id)| DeckCard::new(id, i as i32 + 1))
        .collect()
}

/// The projectile files, trigger markers and registered shot states in call order.
fn outline(commands: &[GunCommand]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|c| match c {
            GunCommand::BeginProjectile(f) => Some(format!("begin {}", f.rsplit('/').next()?)),
            GunCommand::EndProjectile => Some("end".to_owned()),
            GunCommand::BeginTriggerHitWorld => Some("trigger hit_world".to_owned()),
            GunCommand::BeginTriggerTimer(t) => Some(format!("trigger timer {t}")),
            GunCommand::BeginTriggerDeath => Some("trigger death".to_owned()),
            GunCommand::EndTrigger => Some("end trigger".to_owned()),
            GunCommand::RegisterGunAction(_) => Some("shot".to_owned()),
            GunCommand::StartReload(t) => Some(format!("reload {t}")),
            GunCommand::OnNotEnoughManaForAction => Some("no mana".to_owned()),
            _ => None,
        })
        .collect()
}

fn shot_infos(commands: &[GunCommand]) -> Vec<&script::gun::ActionInfo> {
    commands
        .iter()
        .filter_map(|c| match c {
            GunCommand::RegisterGunAction(info) => Some(info.as_ref()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_single_spell_fires_and_starts_a_reload() {
    let Some(gun) = gun() else { return };
    gun.set_deck(&deck(&["LIGHT_BULLET"]), false).unwrap();
    let commands = gun.shoot(100.0, true).unwrap();
    assert_eq!(
        outline(&commands),
        ["begin light_bullet.xml", "end", "shot", "reload 40"]
    );
    assert_eq!(
        commands[0],
        GunCommand::OnActionPlayed("LIGHT_BULLET".into())
    );
    let info = shot_infos(&commands)[0];
    assert_eq!(info.num("fire_rate_wait"), 3.0);
    assert_eq!(info.num("screenshake"), 0.5);
    assert_eq!(info.num("spread_degrees"), -1.0);
    assert_eq!(info.num("damage_critical_chance"), 5.0);
    assert_eq!(info.num("speed_multiplier"), 1.0);
    assert_eq!(info.str("action_id"), "LIGHT_BULLET");
    assert!(commands.contains(&GunCommand::ActionUsed(1)));
    assert!(commands.contains(&GunCommand::SetProjectileConfigs));
    assert_eq!(
        gun.deck_ids().unwrap(),
        ["LIGHT_BULLET"],
        "reload refills the deck"
    );
}

#[test]
fn modifiers_stack_onto_the_next_spell() {
    let Some(gun) = gun() else { return };
    gun.set_deck(&deck(&["DAMAGE", "LIGHT_BULLET", "BOMB"]), false)
        .unwrap();
    let commands = gun.shoot(100.0, false).unwrap();
    assert_eq!(
        outline(&commands),
        ["begin light_bullet.xml", "end", "shot"]
    );
    let info = shot_infos(&commands)[0];
    assert!((info.num("damage_projectile_add") - 0.4).abs() < 1e-12);
    assert_eq!(info.num("gore_particles"), 5.0);
    assert_eq!(info.num("fire_rate_wait"), 8.0);
    assert_eq!(
        info.str("extra_entities"),
        "data/entities/particles/tinyspark_yellow.xml,"
    );
    assert!(commands.contains(&GunCommand::RegisterGunShotEffects {
        recoil_knockback: 10.0
    }));
    let (mana, _) = gun.handle_reload().unwrap();
    assert_eq!(mana, 90.0);
    assert_eq!(gun.deck_ids().unwrap(), ["BOMB"]);
    assert_eq!(gun.discarded_ids().unwrap(), ["DAMAGE", "LIGHT_BULLET"]);
}

#[test]
fn draw_many_spells_cast_several_projectiles() {
    let Some(gun) = gun() else { return };
    gun.set_deck(&deck(&["BURST_2", "LIGHT_BULLET", "BOMB", "DAMAGE"]), false)
        .unwrap();
    let commands = gun.shoot(100.0, false).unwrap();
    assert_eq!(
        outline(&commands),
        [
            "begin light_bullet.xml",
            "end",
            "begin bomb.xml",
            "end",
            "shot"
        ]
    );
    let info = shot_infos(&commands)[0];
    assert_eq!(info.num("fire_rate_wait"), 103.0);
    assert_eq!(info.num("action_draw_many_count"), 2.0);
    let (mana, _) = gun.handle_reload().unwrap();
    assert_eq!(mana, 70.0);
}

#[test]
fn triggers_nest_a_shot_inside_the_projectile() {
    let Some(gun) = gun() else { return };
    gun.set_deck(&deck(&["LIGHT_BULLET_TRIGGER", "BOMB"]), false)
        .unwrap();
    let commands = gun.shoot(100.0, false).unwrap();
    assert_eq!(
        outline(&commands),
        [
            "begin light_bullet.xml",
            "trigger hit_world",
            "begin bomb.xml",
            "end",
            "shot",
            "end trigger",
            "end",
            "shot"
        ]
    );
    let infos = shot_infos(&commands);
    assert_eq!(
        infos[0].num("fire_rate_wait"),
        100.0,
        "inner shot: the bomb only"
    );
    assert_eq!(
        infos[1].num("fire_rate_wait"),
        3.0,
        "outer shot: the trigger spell"
    );
    assert!(commands.contains(&GunCommand::BeginProjectile(LIGHT_BULLET.into())));
    assert!(commands.contains(&GunCommand::BeginProjectile(BOMB.into())));
}

#[test]
fn spells_the_wand_cannot_afford_are_skipped() {
    let Some(gun) = gun() else { return };
    gun.set_deck(&deck(&["BOMB", "LIGHT_BULLET"]), false)
        .unwrap();
    let commands = gun.shoot(10.0, false).unwrap();
    // The bomb costs 25; it goes to the discard pile and the next card is tried.
    assert_eq!(
        outline(&commands),
        ["no mana", "begin light_bullet.xml", "end", "shot"]
    );
    let (mana, _) = gun.handle_reload().unwrap();
    assert_eq!(mana, 5.0);
    // Both cards ended up discarded, the deck ran out and the reload refilled it in order.
    assert_eq!(gun.deck_ids().unwrap(), ["BOMB", "LIGHT_BULLET"]);
    assert!(gun.discarded_ids().unwrap().is_empty());
}

#[test]
fn recharge_shortens_and_mana_spells_refund() {
    let Some(gun) = gun() else { return };
    gun.set_deck(&deck(&["RECHARGE", "LIGHT_BULLET"]), false)
        .unwrap();
    let commands = gun.shoot(100.0, true).unwrap();
    assert_eq!(
        outline(&commands),
        ["begin light_bullet.xml", "end", "shot", "reload 20"]
    );
    assert_eq!(shot_infos(&commands)[0].num("fire_rate_wait"), -7.0);

    gun.set_deck(&deck(&["MANA_REDUCE", "LIGHT_BULLET"]), false)
        .unwrap();
    gun.shoot(100.0, false).unwrap();
    let (mana, _) = gun.handle_reload().unwrap();
    assert_eq!(mana, 100.0 + 30.0 - 5.0);
}

#[test]
fn gun_config_and_base_state_feed_the_shot() {
    let Some(gun) = gun() else { return };
    gun.set_gun(&GunConfig {
        actions_per_round: 2,
        shuffle_deck_when_empty: false,
        reload_time: 25.0,
        deck_capacity: 4,
    })
    .unwrap();
    gun.set_base_state(|state| {
        state.set("fire_rate_wait", 7)?;
        state.set("spread_degrees", 2.5)
    })
    .unwrap();
    gun.set_deck(&deck(&["LIGHT_BULLET", "BOMB"]), false)
        .unwrap();
    let commands = gun.shoot(100.0, true).unwrap();
    assert_eq!(
        outline(&commands),
        [
            "begin light_bullet.xml",
            "end",
            "begin bomb.xml",
            "end",
            "shot",
            "reload 25"
        ]
    );
    let info = shot_infos(&commands)[0];
    assert_eq!(info.num("fire_rate_wait"), 7.0 + 3.0 + 100.0);
    assert_eq!(info.num("spread_degrees"), 2.5 - 1.0);
}

#[test]
fn the_shuffle_uses_the_frame_seeded_script_generator() {
    let Some(gun) = gun() else { return };
    gun.set_gun(&GunConfig {
        shuffle_deck_when_empty: true,
        ..GunConfig::default()
    })
    .unwrap();
    let ids = [
        "BOMB",
        "LIGHT_BULLET",
        "BULLET",
        "AIR_BULLET",
        "SLOW_BULLET",
    ];
    gun.set_frame(777);
    gun.set_deck(&deck(&ids), false).unwrap();

    // The same selection as order_deck(), with the generator called directly.
    let mut rng = ScriptRandom::new(12345);
    rng.set_random_seed(777.0, 777.0);
    let mut pool: Vec<&str> = ids.to_vec();
    let mut shuffled = Vec::new();
    for i in (1..=ids.len() as i32).rev() {
        let index = rng.random_int(1, i) as usize;
        shuffled.push(pool.remove(index - 1));
    }

    // Each cast draws one card from the front of the shuffled deck.
    let mut played = Vec::new();
    for _ in 0..ids.len() {
        let commands = gun.shoot(1000.0, false).unwrap();
        for c in &commands {
            if let GunCommand::OnActionPlayed(id) = c {
                played.push(id.clone());
            }
        }
        gun.handle_reload().unwrap();
    }
    assert_eq!(played, shuffled);
}

#[test]
fn every_reference_spell_casts_without_script_errors() {
    let Some(gun) = gun() else { return };
    // Spell actions look at the entity that holds the wand.
    let holder = gun.runtime().with_world(|w| w.create_entity("wand"));
    gun.runtime().set_updated_entity(holder);
    let actions: mlua::Table = gun.runtime().lua().globals().get("actions").unwrap();
    let ids: Vec<String> = actions
        .sequence_values::<mlua::Table>()
        .map(|a| a.unwrap().get::<String>("id").unwrap())
        .collect();
    assert!(ids.len() >= 400);
    let mut failures = Vec::new();
    for (n, id) in ids.iter().enumerate() {
        gun.set_frame(100 + n as i32);
        let mut cards = vec![DeckCard::new(id, 1)];
        cards.extend((2..=4).map(|i| DeckCard::new("LIGHT_BULLET", i)));
        let result = gun
            .set_deck(&cards, false)
            .and_then(|()| gun.shoot(1000.0, true));
        if let Err(e) = result {
            failures.push(format!(
                "{id}: {}",
                e.to_string().lines().next().unwrap_or("")
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

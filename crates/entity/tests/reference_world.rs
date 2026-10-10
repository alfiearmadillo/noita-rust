//! Schemas and instantiated entities against the extracted reference data; skips without it.

use std::path::PathBuf;
use std::sync::Arc;

use entity::schema::{Kind, Schema, Value};
use entity::world::World;
use entity::{DiskFiles, Loader};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files")
}

fn schema() -> Option<Arc<Schema>> {
    let path = root().join("install/tools_modding/component_documentation.txt");
    let text = std::fs::read_to_string(path).ok().or_else(|| {
        eprintln!("skipping: reference data not present");
        None
    })?;
    Some(Arc::new(Schema::parse(&text)))
}

#[test]
fn the_schema_lists_every_documented_component() {
    let Some(schema) = schema() else { return };
    assert!(schema.len() >= 150, "{} components", schema.len());
    let damage = schema.component("DamageModelComponent").unwrap();
    let hp = damage.field("hp").unwrap();
    assert_eq!((hp.kind, &hp.default), (Kind::Double, &Value::Double(1.0)));
    assert!(!hp.private);
    let ai = schema.component("AIAttackComponent").unwrap();
    assert_eq!(
        ai.field("min_distance").unwrap().default,
        Value::Float(10.0)
    );
    assert_eq!(
        ai.field("attack_ranged_entity_file").unwrap().default,
        Value::Str("data/entities/projectiles/spear.xml".into())
    );
    assert_eq!(
        ai.field("attack_landing_ranged_enabled").unwrap().default,
        Value::Bool(false)
    );
    assert!(ai.field("mNextFrameUsable").unwrap().private);
    // Lens values are plain numbers; unknown types stay raw.
    let character = schema.component("CharacterDataComponent").unwrap();
    assert_eq!(
        character.field("collision_aabb_min_x").unwrap().kind,
        Kind::Float
    );
    let ability = schema.component("AbilityComponent").unwrap();
    assert_eq!(ability.field("gun_config").unwrap().kind, Kind::Raw);
    assert_eq!(
        ability.field("mana_max").unwrap().default,
        Value::Float(100.0)
    );
}

#[test]
fn instantiating_an_entity_applies_defaults_and_overrides() {
    let Some(schema) = schema() else { return };
    let root = root();
    let mut loader = Loader::new(DiskFiles(root));
    let def = loader
        .load("data/entities/animals/zombie_weak.xml")
        .unwrap();
    let mut world = World::new(schema);
    let mut rng = |a: i32, _b: i32| a;
    let id = world.instantiate(
        &def,
        "data/entities/animals/zombie_weak.xml",
        (10.0, 20.0),
        &mut rng,
    );

    let e = world.entity(id).unwrap();
    assert_eq!((e.x, e.y), (10.0, 20.0));
    assert_eq!(e.name, "$animal_zombie");
    assert!(e.has_tag("zombie"));
    assert_eq!(e.components.len(), def.components.len());

    let damage = world
        .component(world.components_of(id, "DamageModelComponent")[0])
        .unwrap();
    let damage_id = damage.id;
    assert_eq!(damage.value("hp"), Some(&Value::Double(0.2)));
    // Untouched members keep their documented default.
    assert_eq!(damage.value("max_hp_cap"), Some(&Value::Double(0.0)));
    assert_eq!(damage.float("damage_multipliers.holy"), Some(1.2f32 as f64));
    // $[1-3] picked the first value from the stub RNG.
    assert_eq!(
        damage.string("blood_sprite_directional"),
        Some("data/particles/bloodsplatters/bloodsplatter_directional_1.xml")
    );

    let ai = world
        .component(world.components_of(id, "AnimalAIComponent")[0])
        .unwrap();
    assert_eq!(ai.boolean("attack_dash_enabled"), Some(true));
    assert_eq!(ai.float("attack_melee_damage_min"), Some(0.2f32 as f64));
    assert!(ai.enabled);

    let character = world
        .component(world.components_of(id, "CharacterDataComponent")[0])
        .unwrap();
    assert_eq!(character.float("collision_aabb_min_x"), Some(-2.0));

    // Killing the entity removes its components.
    world.kill_entity(id);
    assert!(world.is_empty());
    assert!(world.component(damage_id).is_none());
}

#[test]
fn vec2_members_are_written_as_x_and_y_attributes() {
    let Some(schema) = schema() else { return };
    let mut loader = Loader::new(DiskFiles(root()));
    let def = loader.load("data/entities/base_apparition.xml");
    let Ok(def) = def else { return };
    let mut world = World::new(schema);
    let id = world.instantiate(&def, "", (0.0, 0.0), &mut |a, _| a);
    let area = world.components_of(id, "AreaDamageComponent");
    if let Some(&c) = area.first() {
        let c = world.component(c).unwrap();
        assert_eq!(c.value("aabb_min"), Some(&Value::Vec2(-3.0, -14.0)));
    }
}

#[test]
fn children_are_attached_to_their_parent() {
    let Some(schema) = schema() else { return };
    let mut loader = Loader::new(DiskFiles(root()));
    let def = loader.load("data/entities/player.xml").unwrap();
    let mut world = World::new(schema);
    let id = world.instantiate(&def, "data/entities/player.xml", (0.0, 0.0), &mut |a, _| a);
    let kids: Vec<&str> = world
        .entity(id)
        .unwrap()
        .children
        .iter()
        .map(|c| world.entity(*c).unwrap().name.as_str())
        .collect();
    assert!(kids.contains(&"inventory_quick") && kids.contains(&"inventory_full"));
    let quick = world
        .entity(id)
        .unwrap()
        .children
        .iter()
        .copied()
        .find(|c| world.entity(*c).unwrap().name == "inventory_quick")
        .unwrap();
    assert_eq!(world.entity(quick).unwrap().parent, Some(id));
    assert_eq!(world.entity(quick).unwrap().children.len(), 3);
    assert!(world.with_tag("player_unit").contains(&id));
    world.kill_entity(id);
    assert!(world.is_empty(), "children die with the parent");
}

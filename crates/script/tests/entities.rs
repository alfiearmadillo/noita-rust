//! The entity and component script API.

use std::path::PathBuf;

use script::Runtime;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("noita_ent_{name}_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("data")).unwrap();
        Self(dir)
    }

    fn write(&self, rel: &str, text: &str) {
        std::fs::write(self.0.join(rel), text).unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files")
}

#[test]
fn entities_load_move_and_die() {
    let dir = Scratch::new("basic");
    dir.write(
        "data/thing.xml",
        r#"<Entity name="thing" tags="a,b">
  <MyComponent speed="3.5" label="hi" _tags="fast" />
  <MyComponent speed="1" _enabled="0" />
  <Entity name="kid"/>
</Entity>"#,
    );
    let rt = Runtime::new(&dir.0, 1).unwrap();
    let lua = rt.lua();
    let result: mlua::MultiValue = lua
        .load(
            r#"
            local id = EntityLoad("data/thing.xml", 10, 20)
            local x, y, rot, sx, sy = EntityGetTransform(id)
            EntitySetTransform(id, 30, 40, 1.5)
            local x2, y2, rot2 = EntityGetTransform(id)
            local kids = EntityGetAllChildren(id)
            local enabled = EntityGetComponent(id, "MyComponent")
            local all = EntityGetComponentIncludingDisabled(id, "MyComponent")
            local fast = EntityGetFirstComponent(id, "MyComponent", "fast")
            local none = EntityGetComponent(id, "Nope")
            local c = EntityGetFirstComponent(id, "MyComponent")
            local speed = ComponentGetValue2(c, "speed")
            ComponentSetValue2(c, "speed", 9)
            local speed2 = ComponentGetValue2(c, "speed")
            local label = ComponentGetValue2(c, "label")
            return id, x, y, sx, x2, y2, rot2, #kids, #enabled, #all, fast == c, none == nil,
                   speed, speed2, label, EntityGetName(id), EntityGetTags(id), #EntityGetWithTag("b"),
                   ComponentGetTypeName(c), ComponentGetEntity(c) == id
            "#,
        )
        .eval()
        .unwrap();
    let vals: Vec<String> = result
        .iter()
        .map(|v| match v {
            mlua::Value::Number(n) => n.to_string(),
            mlua::Value::Integer(n) => n.to_string(),
            mlua::Value::Boolean(b) => b.to_string(),
            mlua::Value::String(s) => s.to_string_lossy(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        vals,
        [
            "1",
            "10",
            "20",
            "1",
            "30",
            "40",
            "1.5",
            "1",
            "1",
            "2",
            "true",
            "true",
            "3.5",
            "9",
            "hi",
            "thing",
            "a,b",
            "1",
            "MyComponent",
            "true"
        ]
    );
    let alive: bool = lua
        .load("EntityKill(1) return EntityGetIsAlive(1)")
        .eval()
        .unwrap();
    assert!(!alive);
}

#[test]
fn components_can_be_added_and_the_updated_entity_is_settable() {
    let dir = Scratch::new("add");
    let rt = Runtime::new(&dir.0, 1).unwrap();
    let lua = rt.lua();
    let (n, flag, id): (f64, bool, f64) = lua
        .load(
            r#"
            local e = EntityCreateNew("made")
            local c = EntityAddComponent2(e, "Thing", { count = 4, on = true })
            EntitySetComponentIsEnabled(e, c, false)
            return ComponentGetValue2(c, "count"), ComponentGetIsEnabled(c), GetUpdatedEntityID()
            "#,
        )
        .eval()
        .unwrap();
    assert_eq!((n, flag, id), (4.0, false, 0.0));
    rt.set_updated_entity(7);
    let id: f64 = lua.load("return GetUpdatedEntityID()").eval().unwrap();
    assert_eq!(id, 7.0);
    assert_eq!(rt.with_world(|w| w.len()), 1);
}

#[test]
fn reference_entities_expose_typed_values() {
    let root = root();
    if !root.join("data/entities/animals/zombie_weak.xml").exists() {
        eprintln!("skipping: reference data not present");
        return;
    }
    let rt = Runtime::new(&root, 1).unwrap();
    let lua = rt.lua();
    let (hp, x, holy, flag, filename): (f64, f64, f64, bool, String) = lua
        .load(
            r#"
            local id = EntityLoad("data/entities/animals/zombie_weak.xml", 5, 6)
            local dm = EntityGetFirstComponent(id, "DamageModelComponent")
            local ai = EntityGetFirstComponent(id, "AnimalAIComponent")
            local x = EntityGetTransform(id)
            return ComponentGetValue2(dm, "hp"), x, ComponentObjectGetValue2(dm, "damage_multipliers", "holy"),
                   ComponentGetValue2(ai, "attack_dash_enabled"), EntityGetFilename(id)
            "#,
        )
        .eval()
        .unwrap();
    assert!((hp - 0.2).abs() < 1e-12);
    assert_eq!(x, 5.0);
    assert!((holy - 1.2).abs() < 1e-6);
    assert!(flag);
    assert_eq!(filename, "data/entities/animals/zombie_weak.xml");
    // Unknown fields are an error, as in the engine.
    assert!(
        lua.load(
            r#"local id = EntityGetWithTag("zombie")[1]
               ComponentGetValue2(EntityGetFirstComponent(id, "DamageModelComponent"), "nope")"#
        )
        .exec()
        .is_err()
    );
}

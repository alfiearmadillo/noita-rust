//! Checks against the locally extracted reference data. Skips when `noita_files/` is not populated
//! (e.g. CI), since game files are never committed.

use std::path::PathBuf;

use noita_data::materials::{self, MaterialDef};

fn reference_materials() -> Option<Vec<MaterialDef>> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files/data/materials.xml");
    let Ok(xml) = std::fs::read_to_string(&path) else {
        eprintln!("skipping: {} not present", path.display());
        return None;
    };
    Some(materials::parse(&xml).expect("reference materials.xml parses"))
}

fn reference_reactions() -> Option<Vec<materials::ReactionDef>> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files/data/materials.xml");
    let Ok(xml) = std::fs::read_to_string(&path) else {
        eprintln!("skipping: {} not present", path.display());
        return None;
    };
    Some(materials::parse_reactions(&xml).expect("reference reactions parse"))
}

fn find<'a>(all: &'a [MaterialDef], name: &str) -> &'a MaterialDef {
    all.iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn reference_sand_and_rock_static_properties() {
    let Some(all) = reference_materials() else {
        return;
    };

    let sand = find(&all, "sand");
    assert_eq!(sand.attr("cell_type"), Some("liquid"));
    assert_eq!(sand.attr_flag("liquid_sand"), Some(true));
    assert_eq!(sand.attr_flag("liquid_static"), None);
    assert_eq!(sand.attr_f32("density"), Some(6.0));
    assert_eq!(sand.attr_f32("liquid_gravity"), Some(0.9));
    assert_eq!(sand.graphics_color, Some(0xffb8_9e57));

    let rock = find(&all, "rock_static");
    assert_eq!(rock.attr("cell_type"), Some("liquid"));
    assert_eq!(rock.attr_flag("liquid_static"), Some(true));
    assert_eq!(rock.attr_flag("liquid_sand"), Some(true));
    assert_eq!(rock.attr_f32("density"), Some(10.0));
}

#[test]
fn reference_duplicate_material_names() {
    let Some(all) = reference_materials() else {
        return;
    };
    let mut names: Vec<_> = all.iter().map(|m| m.name.as_str()).collect();
    names.sort_unstable();
    let mut dups: Vec<_> = names
        .windows(2)
        .filter(|w| w[0] == w[1])
        .map(|w| w[0])
        .collect();
    dups.dedup();
    // Engine handling of these duplicates is unresolved; the parser keeps both, parent lookup uses the last.
    assert_eq!(dups, ["meat_pumpkin", "rock_box2d"]);
    println!("{} materials", all.len());
}

#[test]
fn reference_required_reactions_reach_the_tag_members() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files/data/materials.xml");
    let Ok(xml) = std::fs::read_to_string(&path) else {
        eprintln!("skipping: {} not present", path.display());
        return;
    };
    let defs = materials::parse(&xml).unwrap();
    let required = materials::parse_required_reactions(&xml).unwrap();
    assert_eq!(required.len(), 5);
    let table = noita_data::sim_materials::to_sim_with_reactions(&defs, &[], &required, |_| None);
    let id = |name| table.find(name).unwrap();
    assert_eq!(
        table.required_reaction_max_probability(id("mushroom_seed")),
        100
    );
    assert_eq!(
        table.required_reaction_max_probability(id("ceiling_plant_material")),
        1000
    );
    let tagged: Vec<_> = (0..table.len())
        .map(|i| sim::MaterialId(i as u16))
        .filter(|&m| {
            table
                .get(m)
                .is_some_and(|p| p.tags.iter().any(|t| t == "requires_air"))
        })
        .collect();
    assert!(!tagged.is_empty(), "the [requires_air] tag has members");
    for m in tagged {
        assert_eq!(table.required_reaction_max_probability(m), 1000);
    }
}

#[test]
fn reference_reaction_records_preserve_order_and_explosion_config() {
    let Some(reactions) = reference_reactions() else {
        return;
    };
    assert_eq!(reactions.len(), 325);
    assert_eq!(
        reactions[0].attrs.get("input_cell1").map(String::as_str),
        Some("cement")
    );
    assert_eq!(
        reactions[0].attrs.get("probability").map(String::as_str),
        Some("70")
    );
    assert_eq!(
        reactions
            .iter()
            .filter(|reaction| reaction.explosion_config.is_some())
            .count(),
        3
    );

    let Some(defs) = reference_materials() else {
        return;
    };
    let table = noita_data::sim_materials::to_sim_with_reactions(&defs, &reactions, &[], |_| None);
    assert_eq!(table.reactions().len(), reactions.len());
    let water = table.get(table.find("water").unwrap()).unwrap();
    assert!(water.tags.iter().any(|tag| tag == "water"));
    for reaction_index in 0..table.reactions().len() {
        for input in 1..=3 {
            let Some(selector) = table.reaction_input_selector(reaction_index, input) else {
                continue;
            };
            if let sim::ReactionInputSelector::Tag(tag) = selector {
                assert!(
                    (0..table.len()).any(|index| {
                        table
                            .get(sim::MaterialId(index as u16))
                            .is_some_and(|material| material.tags.iter().any(|value| value == &tag))
                    }),
                    "reaction input tag {tag:?} has no material"
                );
            }
        }
    }
    assert_eq!(
        table.reactions()[0]
            .attrs
            .get("input_cell1")
            .map(String::as_str),
        Some("cement")
    );

    let id = |name| table.find(name).unwrap();
    let cement = table.reactions_between(false, id("cement"), id("concrete_static"));
    assert_eq!(cement.len(), 1);
    assert_eq!(cement[0].probability, 7000);
    assert_eq!(cement[0].output1, id("concrete_static"));
    assert_eq!(cement[0].req_lifetime, 300);
    assert!(cement[0].destroy_lonely_pixels);
    assert!(table.reaction_max_probability(false, id("cement")) >= 7000);
    let pairs = (0..table.len())
        .flat_map(|a| (0..table.len()).map(move |b| (a, b)))
        .filter(|&(a, b)| {
            let (a, b) = (sim::MaterialId(a as u16), sim::MaterialId(b as u16));
            !table.reactions_between(false, a, b).is_empty()
                || !table.reactions_between(true, a, b).is_empty()
        })
        .count();
    println!("{pairs} reacting material pairs after tag expansion");
    assert!(pairs > 325, "tag inputs expand to many pairs");
}

#[test]
fn reference_sim_table_has_air_first_and_sand_props() {
    let Some(all) = reference_materials() else {
        return;
    };
    let table = noita_data::sim_materials::to_sim(&all, |_| None);
    assert_eq!(
        table.len(),
        all.len(),
        "air is the first reference definition"
    );
    assert_eq!(table.get(sim::AIR).unwrap().name, "air");
    let sand = table.get(table.find("sand").unwrap()).unwrap();
    assert!(sand.liquid_sand && !sand.liquid_static);
    assert_eq!(sand.liquid_viscosity, 50);
    let rock = table.get(table.find("rock_static").unwrap()).unwrap();
    assert!(rock.liquid_static);
    let radioactive_cloud = table.get(table.find("cloud_radioactive").unwrap()).unwrap();
    assert!(radioactive_cloud.liquid_stains_self);
}

#[test]
fn reference_fire_and_spark_properties_reach_sim_table() {
    let Some(all) = reference_materials() else {
        return;
    };
    let table = noita_data::sim_materials::to_sim(&all, |_| None);

    let fire = table.get(table.find("fire").unwrap()).unwrap();
    assert_eq!(fire.cell_type, sim::CellType::Fire);
    assert!(!fire.burnable && fire.on_fire && fire.requires_oxygen);
    assert_eq!(fire.temperature_of_fire, 100);

    let spark = table.get(table.find("spark").unwrap()).unwrap();
    assert_eq!(spark.cell_type, sim::CellType::Fire);
    assert!(!spark.burnable && !spark.on_fire && spark.requires_oxygen);
    assert_eq!(spark.temperature_of_fire, 0);

    let blue_fire = table.get(table.find("fire_blue").unwrap()).unwrap();
    assert!(blue_fire.on_fire);
    assert_eq!(blue_fire.temperature_of_fire, fire.temperature_of_fire);

    let alcohol_gas = table.get(table.find("alcohol_gas").unwrap()).unwrap();
    assert!(alcohol_gas.burnable && alcohol_gas.requires_oxygen);
    assert_eq!(
        (alcohol_gas.generates_flames, alcohol_gas.generates_smoke),
        (20, 1)
    );
    assert_eq!(alcohol_gas.autoignition_temperature, 10);
    assert_eq!(alcohol_gas.temperature_of_fire, 80);

    let wood = table.get(table.find("wood_player").unwrap()).unwrap();
    assert!(wood.burnable && wood.requires_oxygen);
    assert_eq!(wood.fire_hp, 600);
    assert_eq!(wood.autoignition_temperature, 85);
    assert_eq!(wood.temperature_of_fire, 50);
    assert_eq!(wood.on_fire_convert_to_material, "wood_player_b2");

    let teleportation = table
        .get(table.find("magic_liquid_unstable_teleportation").unwrap())
        .unwrap();
    assert_eq!(teleportation.on_fire_flame_material, "fire_blue");
    assert_eq!(teleportation.on_fire_smoke_material, "blood_cold_vapour");
}

#[test]
fn reference_textures_all_decode() {
    let Some(all) = reference_materials() else {
        return;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files");
    let mut failures = Vec::new();
    let table = noita_data::sim_materials::to_sim(&all, |f| {
        noita_data::sim_materials::load_png(&root.join(f))
            .map_err(|e| failures.push(e))
            .ok()
    });
    assert!(failures.is_empty(), "{failures:#?}");
    let sand = table.get(table.find("sand").unwrap()).unwrap();
    assert!(sand.texture.is_some());

    let uncolored: Vec<_> = (0..table.len())
        .map(|i| table.get(sim::MaterialId(i as u16)).unwrap())
        .filter(|m| m.texture.is_none() && m.color == noita_data::sim_materials::MISSING_COLOR)
        .map(|m| m.name.as_str())
        .collect();
    assert!(uncolored.is_empty(), "no color source: {uncolored:?}");
    let steam = table.get(table.find("steam").unwrap()).unwrap();
    assert_eq!(steam.color, 0x7fc5_c5ff, "wang_color fallback (0x6fae61)");
}

#[test]
fn reference_material_names_translate_to_english() {
    let Some(all) = reference_materials() else {
        return;
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../noita_files/data/translations/common.csv");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipping: {} not present", path.display());
        return;
    };
    let english = noita_data::translations::parse_english(&String::from_utf8_lossy(&bytes));
    let name_of = |m: &str| {
        let ui = find(&all, m).attr("ui_name").expect("ui_name");
        noita_data::translations::lookup(&english, ui).map(str::to_owned)
    };
    assert_eq!(
        name_of("monster_powder_test").as_deref(),
        Some("Monstrous powder")
    );
    assert_eq!(name_of("water").as_deref(), Some("Water"));
}

#[test]
fn reference_stain_fields_reach_the_sim_table() {
    let Some(defs) = reference_materials() else {
        return;
    };
    let table = noita_data::sim_materials::to_sim_with_reactions(&defs, &[], &[], |_| None);
    let all: Vec<_> = (0..table.len())
        .filter_map(|i| table.get(sim::MaterialId(i as u16)))
        .collect();
    assert!(all.iter().any(|m| m.liquid_stains & 2 != 0));
    assert!(
        all.iter()
            .any(|m| m.liquid_stains_custom_color == 0xaa83_0000)
    );
    assert!(
        all.iter().any(|m| !m.stainable),
        "seven materials set stainable=0"
    );
    assert!(all.iter().filter(|m| m.stainable).count() > 100);
}

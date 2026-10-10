//! Entity template loading against the extracted reference data; skips without it.

use std::path::PathBuf;

use entity::{DiskFiles, LoadError, Loader};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files")
}

fn loader() -> Option<Loader<DiskFiles>> {
    let root = root();
    if !root.join("data/entities/player.xml").exists() {
        eprintln!("skipping: reference data not present");
        return None;
    }
    Some(Loader::new(DiskFiles(root)))
}

fn xml_files(dir: &std::path::Path, out: &mut Vec<String>, rel: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = format!("{rel}/{name}");
        if e.path().is_dir() {
            xml_files(&e.path(), out, &path);
        } else if name.ends_with(".xml") {
            out.push(path);
        }
    }
}

#[test]
fn every_entity_file_resolves() {
    let Some(mut loader) = loader() else { return };
    let mut files = Vec::new();
    xml_files(&root().join("data/entities"), &mut files, "data/entities");
    let mut ok = 0;
    let mut problems = Vec::new();
    let mut missing = Vec::new();
    for f in &files {
        match loader.load(f) {
            Ok(_) => ok += 1,
            // Sprites, ragdolls and other XML kinds live in the same folders.
            Err(LoadError::NotAnEntity(_)) => {}
            // A handful of files name bases that do not exist in the shipped data.
            Err(LoadError::Missing(p)) => missing.push(p),
            Err(e) => problems.push(e.to_string()),
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(missing.len() <= 5, "{missing:#?}");
    assert!(ok > 2500, "only {ok} entities resolved");
}

#[test]
fn a_derived_entity_overrides_its_base() {
    let Some(mut loader) = loader() else { return };
    let base = loader.load("data/entities/animals/zombie.xml").unwrap();
    let weak = loader
        .load("data/entities/animals/zombie_weak.xml")
        .unwrap();
    assert_eq!(
        base.component("DamageModelComponent").unwrap().attr("hp"),
        Some("0.5")
    );
    let damage = weak.component("DamageModelComponent").unwrap();
    assert_eq!(damage.attr("hp"), Some("0.2"));
    assert_eq!(
        damage.attr("ragdoll_filenames_file"),
        Some("data/ragdolls/zombie_weak/filenames.txt")
    );
    // Attributes the override did not mention keep the base's value.
    assert_eq!(damage.attr("ragdoll_material"), Some("meat"));
    assert_eq!(
        damage.child("damage_multipliers").unwrap().attr("holy"),
        Some("1.2")
    );
    assert_eq!(
        weak.component("SpriteComponent")
            .unwrap()
            .attr("image_file"),
        Some("data/enemies_gfx/zombie_weak.xml")
    );
    // The override did not add or drop components.
    assert_eq!(weak.components.len(), base.components.len());
    assert_eq!(weak.name(), "$animal_zombie");
    assert!(weak.tags().contains(&"zombie"));
    assert!(weak.tags().len() > 1, "tags of the bases accumulate");
}

#[test]
fn include_children_pulls_in_the_base_children() {
    let Some(mut loader) = loader() else { return };
    let base = loader.load("data/entities/player_base.xml").unwrap();
    let player = loader.load("data/entities/player.xml").unwrap();
    assert!(!base.children.is_empty());
    let names: Vec<&str> = player.children.iter().map(|c| c.name()).collect();
    for child in &base.children {
        assert!(names.contains(&child.name()), "{} missing", child.name());
    }
    assert!(names.contains(&"inventory_quick") && names.contains(&"inventory_full"));
    let quick = player
        .children
        .iter()
        .find(|c| c.name() == "inventory_quick")
        .unwrap();
    assert_eq!(quick.children.len(), 3);
    assert!(quick.children[0].component("AbilityComponent").is_some());
}

use std::collections::BTreeMap;

use sim::{
    CellType, Direction, MaterialId, MaterialProps, Materials, ReactionInputSelector,
    ReactionProps, World,
};

fn reaction(attrs: &[(&str, &str)]) -> ReactionProps {
    ReactionProps {
        attrs: attrs
            .iter()
            .map(|&(k, v)| (k.to_owned(), v.to_owned()))
            .collect::<BTreeMap<_, _>>(),
        explosion_config: None,
    }
}

fn static_liquid(name: &str) -> MaterialProps {
    MaterialProps {
        liquid_static: true,
        ..MaterialProps::new(name, CellType::Liquid)
    }
}

const A: MaterialId = MaterialId(1);
const B: MaterialId = MaterialId(2);
const C: MaterialId = MaterialId(3);
const D: MaterialId = MaterialId(4);

fn abcd(reactions: Vec<ReactionProps>) -> Materials {
    Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        static_liquid("a"),
        static_liquid("b"),
        static_liquid("c"),
        static_liquid("d"),
    ])
    .with_reactions(reactions)
}

fn surrounded(materials: Materials, center: MaterialId, around: MaterialId) -> World {
    let mut w = World::new(1, 1, 64, materials, 5);
    w.set(32, 32, center).unwrap();
    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        w.set(32 + dx, 32 + dy, around).unwrap();
    }
    w
}

fn run_active(w: &mut World, frames: usize) {
    for _ in 0..frames {
        w.mark_area((0, 0), (w.width() - 1, w.height() - 1));
        w.step();
    }
}

#[test]
fn tables_expand_tags_in_name_order_and_scale_probability() {
    let tagged = |name: &str| MaterialProps {
        tags: vec!["acidic".to_owned()],
        ..static_liquid(name)
    };
    let materials = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        static_liquid("a"),
        tagged("zinc"),
        tagged("brass"),
        static_liquid("d"),
    ])
    .with_reactions(vec![
        reaction(&[
            ("probability", "12.5"),
            ("input_cell1", "a"),
            ("input_cell2", "[acidic]"),
            ("output_cell1", "d"),
            ("output_cell2", "[acidic]"),
        ]),
        // Duplicates of an existing record are not added again (0x709e10).
        reaction(&[
            ("probability", "12.5"),
            ("input_cell1", "a"),
            ("input_cell2", "zinc"),
            ("output_cell1", "d"),
            ("output_cell2", "zinc"),
        ]),
        reaction(&[
            ("probability", "40"),
            ("fast_reaction", "1"),
            ("input_cell1", "a"),
            ("input_cell2", "d"),
            ("output_cell1", "d"),
            ("output_cell2", "d"),
            ("direction", "bottom"),
        ]),
    ]);
    let (zinc, brass) = (MaterialId(2), MaterialId(3));

    let with_zinc = materials.reactions_between(false, A, zinc);
    assert_eq!(with_zinc.len(), 1);
    assert_eq!(with_zinc[0].probability, 1250);
    assert_eq!(with_zinc[0].output1, D);
    assert_eq!(
        with_zinc[0].output2, zinc,
        "an output tag keeps the matched input"
    );
    assert_eq!(materials.reactions_between(false, A, brass).len(), 1);
    assert_eq!(materials.reaction_max_probability(false, A), 1250);
    assert_eq!(materials.reaction_max_probability(false, zinc), 0);

    let fast = materials.reactions_between(true, A, D);
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].direction, Some(Direction::Bottom));
    assert_eq!(materials.reaction_max_probability(true, A), 4000);
    assert!(materials.reactions_between(false, A, D).is_empty());
}

#[test]
fn certain_reaction_converts_the_cell_and_one_neighbor() {
    let materials = abcd(vec![reaction(&[
        ("probability", "100"),
        ("input_cell1", "a"),
        ("input_cell2", "b"),
        ("output_cell1", "c"),
        ("output_cell2", "d"),
    ])]);
    let mut w = surrounded(materials, A, B);
    // Static cells check reactions on 1 in 10 full updates (every 11th region update).
    run_active(&mut w, 2000);
    assert_eq!(w.get(32, 32), Some(C));
    assert_eq!(w.count(D), 1);
    assert_eq!(w.count(B), 3);
}

#[test]
fn fast_reaction_runs_on_ordinary_updates() {
    let materials = abcd(vec![reaction(&[
        ("probability", "100"),
        ("fast_reaction", "1"),
        ("input_cell1", "a"),
        ("input_cell2", "b"),
        ("output_cell1", "c"),
        ("output_cell2", "d"),
    ])]);
    let mut w = surrounded(materials, A, B);
    run_active(&mut w, 20);
    assert_eq!(w.get(32, 32), Some(C));
    assert_eq!(w.count(D), 1);
}

#[test]
fn direction_limits_which_neighbor_can_react() {
    let rule = |direction| {
        abcd(vec![reaction(&[
            ("probability", "100"),
            ("fast_reaction", "1"),
            ("input_cell1", "a"),
            ("input_cell2", "b"),
            ("output_cell1", "c"),
            ("output_cell2", "d"),
            ("direction", direction),
        ])])
    };
    let place = |materials, dy| {
        let mut w = World::new(1, 1, 64, materials, 9);
        w.set(32, 32, A).unwrap();
        w.set(32, 32 + dy, B).unwrap();
        w
    };

    let mut above = place(rule("bottom"), -1);
    run_active(&mut above, 200);
    assert_eq!(above.get(32, 32), Some(A));

    let mut below = place(rule("bottom"), 1);
    run_active(&mut below, 200);
    assert_eq!(below.get(32, 32), Some(C));
    assert_eq!(below.get(32, 33), Some(D));
}

#[test]
fn blob_fills_a_disc_after_the_region_update() {
    let materials = abcd(vec![reaction(&[
        ("probability", "100"),
        ("fast_reaction", "1"),
        ("input_cell1", "a"),
        ("input_cell2", "b"),
        ("output_cell1", "c"),
        ("output_cell2", "d"),
        ("blob_radius1", "2"),
    ])]);
    let mut w = surrounded(materials, A, B);
    run_active(&mut w, 20);
    // dx² + dy² < 4: the center, its four neighbors and the four diagonals; the neighbor's
    // change to `d` is overwritten by the disc.
    assert_eq!(w.count(C), 9);
    assert_eq!(w.count(D), 0);
    assert_eq!(w.count(B), 0);
    assert_eq!(w.get(34, 32), Some(MaterialId(0)), "dx² = r² is outside");
}

fn speck_blob_world() -> World {
    let materials = abcd(vec![reaction(&[
        ("probability", "100"),
        ("fast_reaction", "1"),
        ("input_cell1", "a"),
        ("input_cell2", "b"),
        ("output_cell1", "c"),
        ("output_cell2", "d"),
        ("blob_radius1", "2"),
        ("cosmetic_particle", "d"),
    ])]);
    let mut w = surrounded(materials, A, B);
    run_active(&mut w, 20);
    w
}

#[test]
fn blob_cells_emit_cosmetic_specks() {
    let mut w = speck_blob_world();
    let specks = w.take_specks();
    assert_eq!(specks.len(), 9, "one per converted cell");
    for s in &specks {
        assert_eq!(s.material, D);
        assert!((-10.0..=10.0).contains(&s.vel.0), "{:?}", s.vel);
        assert!((-20.0..=-10.0).contains(&s.vel.1), "{:?}", s.vel);
    }
}

#[test]
fn blob_specks_rise_swirl_and_fade() {
    let mut w = speck_blob_world();
    let mut ps = sim::particles::Particles::new(1);
    let dt = 1.0 / 60.0;
    ps.update(&mut w, dt, 0.0);
    assert_eq!(ps.len(), 9);
    let p = &ps.particles[0];
    assert_eq!(p.mode, sim::particles::MODE_FREE);
    assert_eq!(p.accel, (0.0, -5.0));
    assert!(
        (p.fade_rate * (p.lifetime + dt) + 1.0).abs() < 1e-4,
        "fade over the lifetime"
    );
}
#[test]
fn restricted_blob_converts_only_input_cells_along_rays() {
    let materials = abcd(vec![reaction(&[
        ("probability", "100"),
        ("fast_reaction", "1"),
        ("input_cell1", "a"),
        ("input_cell2", "b"),
        ("output_cell1", "a"),
        ("output_cell2", "c"),
        ("blob_radius2", "3"),
        ("blob_restrict_to_input_material2", "1"),
        ("direction", "bottom"),
    ])]);
    let mut w = World::new(1, 1, 64, materials, 3);
    w.set(32, 32, A).unwrap();
    for y in 33..=40 {
        for x in 24..=40 {
            w.set(x, y, if y == 35 { D } else { B }).unwrap();
        }
    }
    run_active(&mut w, 20);
    assert_eq!(w.get(32, 33), Some(C), "the center changes first");
    assert!(w.count(C) > 3);
    for y in 0..64 {
        for x in 0..64 {
            if w.get(x, y) == Some(C) {
                assert!(
                    (33..=34).contains(&y),
                    "rays stop at the `d` row and at air: {x},{y}"
                );
                assert!((29..=35).contains(&x), "rays are radius 3: {x},{y}");
            }
        }
    }
    assert_eq!(w.count(D), 17);
    assert_eq!(w.count(B) + w.count(C), 17 * 7);
}

#[test]
fn reaction_inputs_match_names_tags_and_any_liquid() {
    let water = MaterialProps {
        tags: vec!["liquid".to_owned(), "water".to_owned()],
        ..MaterialProps::new("water", CellType::Liquid)
    };
    let rock = MaterialProps::new("rock", CellType::Solid);
    let sand = MaterialProps {
        liquid_sand: true,
        ..MaterialProps::new("sand", CellType::Liquid)
    };
    assert!(
        !ReactionInputSelector::parse("[any_liquid]")
            .unwrap()
            .matches(&sand),
        "[any_liquid] excludes liquid_sand (0x7061f0)"
    );

    assert!(
        ReactionInputSelector::parse("water")
            .unwrap()
            .matches(&water)
    );
    assert!(
        ReactionInputSelector::parse("[water]")
            .unwrap()
            .matches(&water)
    );
    assert!(
        ReactionInputSelector::parse("[any_liquid]")
            .unwrap()
            .matches(&water)
    );
    assert!(
        !ReactionInputSelector::parse("[any_liquid]")
            .unwrap()
            .matches(&rock)
    );
    assert!(
        !ReactionInputSelector::parse("[water]")
            .unwrap()
            .matches(&rock)
    );
}

#[test]
fn reaction_input_parser_rejects_empty_or_malformed_selectors() {
    assert_eq!(ReactionInputSelector::parse(""), None);
    assert_eq!(ReactionInputSelector::parse("[]"), None);
    assert_eq!(ReactionInputSelector::parse("[water"), None);
    assert_eq!(ReactionInputSelector::parse("water]"), None);
}

fn with_required(required: Vec<ReactionProps>) -> Materials {
    // A harmless normal reaction keeps `+0x228 > 0`, which static liquids need to react at all.
    abcd(vec![reaction(&[
        ("probability", "1"),
        ("input_cell1", "a"),
        ("input_cell2", "d"),
        ("output_cell1", "d"),
        ("output_cell2", "d"),
    ])])
    .with_required_reactions(required)
}

fn req_a_needs_air(probability: &str, output: &str) -> ReactionProps {
    reaction(&[
        ("probability", probability),
        ("input_cell1", "a"),
        ("input_cell2", "air"),
        ("output_cell1", output),
        ("output_cell2", "air"),
    ])
}

#[test]
fn required_reaction_fires_when_no_neighbor_holds_the_input() {
    let mut w = surrounded(with_required(vec![req_a_needs_air("100", "c")]), A, B);
    run_active(&mut w, 400);
    assert_eq!(w.get(32, 32), Some(C), "a sealed cell takes the output");
}

#[test]
fn required_reaction_is_satisfied_by_any_neighbor_with_the_input() {
    let mut w = surrounded(with_required(vec![req_a_needs_air("100", "c")]), A, B);
    w.set(32, 31, MaterialId(0)).unwrap();
    run_active(&mut w, 400);
    assert_eq!(w.get(32, 32), Some(A), "an open side keeps the cell");
}

#[test]
fn required_reaction_with_a_direction_only_counts_that_side() {
    // A record for another side stops the scan, so only the bottom neighbor can satisfy it.
    let record = |direction| {
        reaction(&[
            ("probability", "100"),
            ("input_cell1", "a"),
            ("input_cell2", "air"),
            ("output_cell1", "c"),
            ("output_cell2", "air"),
            ("direction", direction),
        ])
    };
    let mut open_below = surrounded(with_required(vec![record("bottom")]), A, B);
    open_below.set(32, 33, MaterialId(0)).unwrap();
    run_active(&mut open_below, 400);
    assert_eq!(open_below.get(32, 32), Some(A));

    let mut open_above = surrounded(with_required(vec![record("bottom")]), A, B);
    open_above.set(32, 31, MaterialId(0)).unwrap();
    run_active(&mut open_above, 400);
    assert_eq!(open_above.get(32, 32), Some(C));
}

#[test]
fn required_reaction_chooses_the_most_probable_surviving_record() {
    let mut w = surrounded(
        with_required(vec![req_a_needs_air("10", "c"), req_a_needs_air("50", "d")]),
        A,
        B,
    );
    run_active(&mut w, 400);
    assert_eq!(w.get(32, 32), Some(D));
}

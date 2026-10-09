//! Character movement against a sim world (rules in README "Recovered rules: characters").

use actors::{AnimationSet, Body, Character, Input, Platforming, blocks};
use sim::{CellType, MaterialId, MaterialProps, Materials, World};

const ROCK: MaterialId = MaterialId(1);
const WATER: MaterialId = MaterialId(2);
const FLOOR: i32 = 100;

fn world() -> World {
    let rock = MaterialProps {
        density: 10.0,
        liquid_sand: true,
        liquid_static: true,
        ..MaterialProps::new("rock_static", CellType::Liquid)
    };
    let water = MaterialProps {
        density: 4.0,
        liquid_gravity: 1.5,
        ..MaterialProps::new("water", CellType::Liquid)
    };
    let table = Materials::new(vec![
        MaterialProps::new("air", CellType::Unspecified),
        rock,
        water,
    ]);
    let mut w = World::new(4, 4, 64, table, 1);
    for x in 0..w.width() {
        for y in FLOOR..FLOOR + 3 {
            w.set(x, y, ROCK).unwrap();
        }
    }
    w
}

fn player(x: f32, y: f32) -> Character {
    Character::new((x, y), Platforming::player(), Body::player())
}

/// Every animation the reference player sprite uses here.
struct PlayerSprite;

impl AnimationSet for PlayerSprite {
    fn has(&self, name: &str) -> bool {
        [
            "stand",
            "walk",
            "jump_up",
            "jump_fall",
            "land",
            "fly_idle",
            "fly_move",
            "swim_idle",
            "swim_move",
            "walk_backwards",
        ]
        .contains(&name)
    }
}

fn run(c: &mut Character, w: &World, input: Input, frames: usize) {
    for _ in 0..frames {
        c.step(&input, w, &PlayerSprite);
    }
}

/// Mouse far to the right so the player faces right.
const IDLE: Input = Input {
    left: false,
    right: false,
    up: false,
    down: false,
    fly: false,
    run: false,
    mouse: (1000.0, 0.0),
};
const RIGHT: Input = Input {
    right: true,
    ..IDLE
};
/// W: the reference binds `key_up` to both Up and Fly.
const W: Input = Input {
    up: true,
    fly: true,
    ..IDLE
};

#[test]
fn falls_and_lands_on_rock() {
    let w = world();
    let mut c = player(50.0, 50.0);
    run(&mut c, &w, IDLE, 180);
    assert!(c.on_ground);
    assert_eq!(
        c.vel.1, 60.0,
        "grounded characters keep pressing down at 60"
    );
    let feet = c.pos.1 + c.body.aabb_max.1;
    assert!(
        (FLOOR as f32 - 1.0..FLOOR as f32).contains(&feet),
        "feet at {feet}"
    );
}

#[test]
fn walking_accelerates_to_clamped_run_velocity() {
    let w = world();
    let mut c = player(50.0, 96.0);
    run(&mut c, &w, IDLE, 30);
    let x0 = c.pos.0;
    run(&mut c, &w, RIGHT, 120);
    // Asymptotic approach (snaps only on overshoot), so it converges to 57 in f32.
    assert!(
        (c.vel.0 - 57.0).abs() < 1e-3,
        "run_velocity 154 clamped to 57: {}",
        c.vel.0
    );
    assert!(c.pos.0 - x0 > 100.0, "moved {}", c.pos.0 - x0);
    run(&mut c, &w, IDLE, 120);
    assert!(c.vel.0.abs() < 1e-3);
}

#[test]
fn first_frame_of_walking_uses_accel_x() {
    let w = world();
    let mut c = player(50.0, 96.0);
    run(&mut c, &w, IDLE, 30);
    run(&mut c, &w, RIGHT, 1);
    assert!((c.vel.0 - 57.0 * 0.15).abs() < 1e-4, "vx {}", c.vel.0);
}

#[test]
fn up_alone_jumps_to_ballistic_apex() {
    let w = world();
    let mut c = player(50.0, 96.0);
    run(&mut c, &w, IDLE, 30);
    let y0 = c.pos.1;
    let up = Input { up: true, ..IDLE };
    c.step(&up, &w, &PlayerSprite);
    assert!(c.vel.1 < 0.0, "jump starts upward");
    let mut apex = c.pos.1;
    for _ in 0..60 {
        c.step(&up, &w, &PlayerSprite);
        apex = apex.min(c.pos.1);
    }
    // v^2 / 2g with v = 95 - 350/60 (gravity applied in the jump frame), g = 350.
    let v = 95.0 - 350.0 / 60.0;
    let expected = v * v / (2.0 * 350.0);
    assert!(((y0 - apex) - expected).abs() < 1.5, "rose {}", y0 - apex);
    run(&mut c, &w, up, 120);
    assert!(c.on_ground, "holding up past the buffer does not re-jump");
}

#[test]
fn holding_w_levitates_until_energy_runs_out() {
    let w = world();
    let mut c = player(50.0, 96.0);
    run(&mut c, &w, IDLE, 30);
    assert_eq!(c.flying_time_left, 3.0, "clamped to fly_time_max");
    let y0 = c.pos.1;
    let mut apex = c.pos.1;
    let mut frames = 0;
    while c.flying_time_left > 0.0 && frames < 1000 {
        c.step(&W, &w, &PlayerSprite);
        apex = apex.min(c.pos.1);
        frames += 1;
    }
    // One frame of drain per frame held: 3 s at 60 Hz.
    assert!((179..=181).contains(&frames), "flew {frames} frames");
    assert!(y0 - apex > 100.0, "rose only {}", y0 - apex);
    assert_eq!(c.jetpack_emitting, 1);
    c.step(&W, &w, &PlayerSprite);
    assert_eq!(c.jetpack_emitting, 0, "no energy, no thrust");
}

#[test]
fn levitation_approaches_terminal_rise_speed() {
    let w = world();
    let mut c = player(50.0, 60.0);
    run(&mut c, &w, IDLE, 60);
    run(&mut c, &w, W, 60);
    // vy' = vy + 0.25 (-95 - vy) + 350/60 has fixed point vy = -95 + 4 * 350/60.
    let terminal = -95.0 + 4.0 * 350.0 / 60.0;
    assert!((c.vel.1 - terminal).abs() < 0.05, "vy {}", c.vel.1);
}

#[test]
fn energy_recharges_on_ground() {
    let w = world();
    let mut c = player(50.0, 96.0);
    run(&mut c, &w, IDLE, 30);
    run(&mut c, &w, W, 60);
    let low = c.flying_time_left;
    assert!(low < 2.1, "drained to {low}");
    run(&mut c, &w, IDLE, 240);
    assert!(c.on_ground);
    assert_eq!(c.flying_time_left, 3.0);
}

#[test]
fn steps_up_small_ledges() {
    let mut w = world();
    for x in 60..w.width() {
        for dy in 1..=3 {
            w.set(x, FLOOR - dy, ROCK).unwrap();
        }
    }
    let mut c = player(40.0, 96.0);
    run(&mut c, &w, IDLE, 30);
    run(&mut c, &w, RIGHT, 90);
    assert!(c.pos.0 > 70.0, "stuck at {}", c.pos.0);
    let feet = c.pos.1 + c.body.aabb_max.1;
    assert!(feet < (FLOOR - 3) as f32, "did not climb: feet {feet}");
}

#[test]
fn tall_wall_blocks_and_flags_collision() {
    let mut w = world();
    for y in FLOOR - 20..FLOOR {
        w.set(60, y, ROCK).unwrap();
        w.set(61, y, ROCK).unwrap();
    }
    let mut c = player(40.0, 96.0);
    run(&mut c, &w, IDLE, 30);
    let mut collided = false;
    for _ in 0..120 {
        c.step(&RIGHT, &w, &PlayerSprite);
        collided |= c.collided_horizontally;
    }
    assert!(
        c.pos.0 + c.body.aabb_max.0 < 60.0,
        "passed wall: {}",
        c.pos.0
    );
    assert!(collided);
}

#[test]
fn w_against_a_wall_still_climbs() {
    let mut w = world();
    for y in FLOOR - 40..FLOOR {
        w.set(60, y, ROCK).unwrap();
        w.set(61, y, ROCK).unwrap();
    }
    let mut c = player(50.0, 96.0);
    run(&mut c, &w, RIGHT, 60);
    let y0 = c.pos.1;
    run(
        &mut c,
        &w,
        Input {
            up: true,
            fly: true,
            ..RIGHT
        },
        30,
    );
    assert!(y0 - c.pos.1 > 15.0, "rose {}", y0 - c.pos.1);
}

#[test]
fn slippery_ground_weakens_control() {
    let ice = MaterialProps {
        density: 10.0,
        liquid_sand: true,
        liquid_static: true,
        slippery: true,
        ..MaterialProps::new("ice_static", CellType::Liquid)
    };
    let table = Materials::new(vec![MaterialProps::new("air", CellType::Unspecified), ice]);
    let mut w = World::new(4, 4, 64, table, 1);
    for x in 0..w.width() {
        w.set(x, FLOOR, MaterialId(1)).unwrap();
    }
    let mut c = player(50.0, 96.0);
    run(&mut c, &w, IDLE, 30);
    assert!(c.on_slippery_ground);
    run(&mut c, &w, RIGHT, 1);
    assert!(
        (c.vel.0 - 57.0 * 0.15 * 0.15).abs() < 1e-4,
        "accel_x scaled by 0.15 while steering: {}",
        c.vel.0
    );
    run(&mut c, &w, RIGHT, 120);
    let vx = c.vel.0;
    c.step(&IDLE, &w, &PlayerSprite);
    let expected = vx + (0.0 - vx) * 0.15 * 0.085;
    assert!((c.vel.0 - expected).abs() < 1e-3, "coasts: {}", c.vel.0);
}

#[test]
fn isolated_pixels_and_liquids_do_not_block() {
    let mut w = world();
    w.set(50, 70, ROCK).unwrap();
    for x in 45..56 {
        w.set(x, 80, WATER).unwrap();
    }
    assert!(!blocks(&w, 50, 70), "lone pixel");
    assert!(!blocks(&w, 50, 80), "water");
    assert!(blocks(&w, 50, FLOOR));
    let mut c = player(50.0, 50.0);
    run(&mut c, &w, IDLE, 180);
    assert!(
        c.pos.1 > 90.0,
        "should fall past the pixel and water, at {}",
        c.pos.1
    );
}

#[test]
fn water_buoys_the_player_up_to_the_surface() {
    let mut w = world();
    for y in 60..FLOOR {
        for x in 30..70 {
            w.set(x, y, WATER).unwrap();
        }
    }
    let mut c = player(50.0, 40.0);
    run(&mut c, &w, IDLE, 240);
    // Idle buoyancy 1.2 > 1: floats until the probe (7 px above the origin) leaves the water.
    assert!(
        (60.0..72.0).contains(&c.pos.1),
        "should bob at the surface, at {}",
        c.pos.1
    );
}

#[test]
fn animations_follow_movement() {
    let w = world();
    let mut c = player(50.0, 96.0);
    run(&mut c, &w, IDLE, 30);
    assert_eq!(c.animator.current(), Some("stand"));
    run(&mut c, &w, RIGHT, 10);
    assert_eq!(c.animator.current(), Some("walk"));
    let left_facing_right = Input { left: true, ..IDLE };
    run(&mut c, &w, left_facing_right, 20);
    assert_eq!(
        c.animator.current(),
        Some("walk_backwards"),
        "moving against the aim direction"
    );
    run(&mut c, &w, IDLE, 30);
    c.step(&Input { up: true, ..IDLE }, &w, &PlayerSprite);
    run(&mut c, &w, IDLE, 3);
    assert_eq!(c.animator.current(), Some("jump_up"));
}

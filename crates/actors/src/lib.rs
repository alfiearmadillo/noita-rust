//! Character movement ported from `CharacterPlatformingSystem` (velocity and animation choice,
//! 0xa7ea90; fly-energy clamp 0xa811b0) and `CharacterCollisionSystem` (platforming_type 2,
//! 0xa659c0). Knockback/precision jumping, AI jump velocity, crouch sensors and game effects are
//! not ported yet.

mod animator;

pub use animator::{AnimState, AnimationSet, Animator};
use sim::{AIR, CellType, World};

/// `CharacterPlatformingComponent` fields (offsets in README).
#[derive(Clone, Debug, PartialEq)]
pub struct Platforming {
    pub velocity_min_x: f32,
    pub velocity_max_x: f32,
    pub velocity_min_y: f32,
    pub velocity_max_y: f32,
    pub run_velocity: f32,
    pub fly_velocity_x: f32,
    pub jump_velocity_x: f32,
    pub jump_velocity_y: f32,
    /// Frames after a jump press during which a grounded character still jumps.
    pub jump_keydown_buffer: i32,
    pub fly_speed_max_up: f32,
    pub fly_speed_max_down: f32,
    pub fly_speed_mult: f32,
    pub fly_speed_change_spd: f32,
    pub fly_model_player: bool,
    pub fly_smooth_y: bool,
    pub accel_x: f32,
    pub accel_x_air: f32,
    pub pixel_gravity: f32,
    pub swim_idle_buoyancy_coeff: f32,
    pub swim_down_buoyancy_coeff: f32,
    pub swim_up_buoyancy_coeff: f32,
    pub swim_drag: f32,
    pub swim_extra_horizontal_drag: f32,
    pub mouse_look: bool,
    pub mouse_look_buffer: f32,
    pub keyboard_look: bool,
    pub turning_buffer: f32,
    pub run_animation_velocity_switching_threshold: f32,
    pub run_animation_velocity_switching_enabled: bool,
    pub turn_animation_frames_between: i32,
}

impl Platforming {
    /// `data/entities/player_base.xml`; unset fields use the documented component defaults.
    pub fn player() -> Self {
        Self {
            velocity_min_x: -57.0,
            velocity_max_x: 57.0,
            velocity_min_y: -200.0,
            velocity_max_y: 350.0,
            run_velocity: 154.0,
            fly_velocity_x: 52.0,
            jump_velocity_x: 56.0,
            jump_velocity_y: -95.0,
            jump_keydown_buffer: 2,
            fly_speed_max_up: 95.0,
            fly_speed_max_down: 85.0,
            fly_speed_mult: 20.0,
            fly_speed_change_spd: 0.25,
            fly_model_player: false,
            fly_smooth_y: false,
            accel_x: 0.15,
            accel_x_air: 0.1,
            pixel_gravity: 350.0,
            swim_idle_buoyancy_coeff: 1.2,
            swim_down_buoyancy_coeff: 0.7,
            swim_up_buoyancy_coeff: 0.9,
            swim_drag: 0.95,
            swim_extra_horizontal_drag: 0.9,
            mouse_look: true,
            mouse_look_buffer: 1.0,
            keyboard_look: true,
            turning_buffer: 0.5,
            run_animation_velocity_switching_threshold: 45.0,
            run_animation_velocity_switching_enabled: false,
            turn_animation_frames_between: 20,
        }
    }
}

/// `CharacterDataComponent` fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub aabb_min: (f32, f32),
    pub aabb_max: (f32, f32),
    pub climb_over_y: i32,
    pub buoyancy_check_offset_y: i32,
    pub fly_time_max: f32,
    pub fly_recharge_spd: f32,
    pub fly_recharge_spd_ground: f32,
    pub flying_needs_recharge: bool,
    pub flying_in_air_wait_frames: i32,
    pub flying_recharge_removal_frames: i32,
    pub ground_stickyness: f32,
}

impl Body {
    /// `data/entities/player_base.xml`; unset fields use the documented component defaults.
    pub fn player() -> Self {
        Self {
            aabb_min: (-2.0, -4.5),
            aabb_max: (2.0, 2.1),
            climb_over_y: 4,
            buoyancy_check_offset_y: -7,
            fly_time_max: 3.0,
            fly_recharge_spd: 0.4,
            fly_recharge_spd_ground: 6.0,
            flying_needs_recharge: true,
            flying_in_air_wait_frames: 38,
            flying_recharge_removal_frames: 8,
            ground_stickyness: 0.0,
        }
    }
}

/// Physical input for one frame. The reference binds `key_up` (W) to both the Up and Fly
/// buttons; either starts a jump.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub fly: bool,
    pub run: bool,
    /// Cursor in world coordinates (`mMousePosition`).
    pub mouse: (f32, f32),
}

/// `ControlsComponent` `mButtonDown*` / `mButtonFrame*` pair.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Button {
    pub down: bool,
    /// Frame the button last went down.
    pub frame: u64,
}

impl Button {
    fn update(&mut self, down: bool, frame: u64) {
        if !self.down && down {
            self.frame = frame;
        }
        self.down = down;
    }

    fn since(self, frame: u64) -> i64 {
        frame as i64 - self.frame as i64
    }
}

/// `ControlsComponent` state used by platforming.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    pub left: Button,
    pub right: Button,
    pub up: Button,
    pub down: Button,
    pub run: Button,
    pub fly: Button,
    /// `mFlyingTargetY` (`+0x12C`): the player system sets it to `y - 10` each frame (0xa8d978).
    pub flying_target_y: f32,
    pub aiming_vector: (f32, f32),
    pub mouse: (f32, f32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Character {
    pub pos: (f32, f32),
    /// `mVelocity` (`CharacterData+0x108`), pixels per second.
    pub vel: (f32, f32),
    /// `is_on_ground` (`+0xB8`).
    pub on_ground: bool,
    /// `is_on_slippery_ground` (`+0xB9`): `slippery` of the cell last landed on or bumped.
    pub on_slippery_ground: bool,
    /// `mCollidedHorizontally` (`+0x114`).
    pub collided_horizontally: bool,
    pub frames_on_ground: i32,
    pub last_frame_on_ground: u64,
    /// `mFlyingTimeLeft` (`+0x110`), seconds.
    pub flying_time_left: f32,
    /// Entity transform scale x: 1 faces right, -1 left.
    pub facing: f32,
    pub platforming: Platforming,
    pub body: Body,
    pub controls: Controls,
    pub animator: Animator,
    /// `mJetpackEmitting`: -1 undefined, 0 off, 1 on.
    pub jetpack_emitting: i32,
    frame: u64,
    ex_animation_pos: (f32, f32),
    frames_in_air_counter: i32,
    fly_throttle: f32,
    smoothed_flying_target_y: f32,
    next_turn_animation_frame: u64,
    frames_not_swimming: i32,
    frames_swimming: i32,
    should_crouch: bool,
    last_posture_switch_frame: i64,
}

/// Fixed step the reference divides by (`60` at 0xa7ee85); the system `dt` field `+0x24`.
pub const DT: f32 = 1.0 / 60.0;

/// `0 > v ? -1 : 1`, the reference's sign test.
fn sign(v: f32) -> f32 {
    if 0.0 > v { -1.0 } else { 1.0 }
}

impl Character {
    pub fn new(pos: (f32, f32), platforming: Platforming, body: Body) -> Self {
        Self {
            pos,
            vel: (0.0, 0.0),
            on_ground: false,
            on_slippery_ground: false,
            collided_horizontally: false,
            frames_on_ground: 0,
            last_frame_on_ground: 0,
            flying_time_left: 1000.0,
            facing: 1.0,
            platforming,
            body,
            controls: Controls::default(),
            animator: Animator::default(),
            jetpack_emitting: -1,
            frame: 0,
            ex_animation_pos: (0.0, 0.0),
            frames_in_air_counter: -1,
            fly_throttle: 0.0,
            smoothed_flying_target_y: 0.0,
            next_turn_animation_frame: 0,
            frames_not_swimming: 10,
            frames_swimming: 0,
            should_crouch: false,
            last_posture_switch_frame: -1,
        }
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn is_crouching(&self) -> bool {
        self.should_crouch
    }

    /// One frame: player controls, platforming (velocity, animation), collision movement,
    /// ground bookkeeping, then the fly-energy clamp.
    pub fn step(&mut self, input: &Input, world: &World, sprite: &dyn AnimationSet) {
        self.frame += 1;
        let f = self.frame;
        let c = &mut self.controls;
        c.left.update(input.left, f);
        c.right.update(input.right, f);
        c.up.update(input.up, f);
        c.down.update(input.down, f);
        c.run.update(input.run, f);
        c.fly.update(input.fly, f);
        c.flying_target_y = self.pos.1 - 10.0;
        c.mouse = input.mouse;
        c.aiming_vector = (input.mouse.0 - self.pos.0, input.mouse.1 - self.pos.1);

        let dx = self.update_velocity(world);
        self.update_animation(dx, sprite);
        self.move_and_collide(world);

        // 0xa65d76
        if self.on_ground && self.vel.1 > 0.0 {
            self.frames_on_ground += 1;
            self.last_frame_on_ground = f;
        } else {
            self.frames_on_ground = 0;
        }
        // 0xa812a0
        if self.body.flying_needs_recharge {
            self.flying_time_left = self.flying_time_left.min(self.body.fly_time_max);
        }
    }

    /// Velocity part of 0xa7ea90. Returns the horizontal velocity it wrote.
    fn update_velocity(&mut self, world: &World) -> f32 {
        let p = &self.platforming;
        let b = &self.body;
        let c = self.controls;
        let frame = self.frame;
        let gravity_dt = p.pixel_gravity * DT;
        let (vx, vy) = self.vel;

        // 0xa7ebad: probe at (x, y + buoyancy_check_offset_y); liquid cell that is not sand.
        let in_liquid = world
            .cell(
                self.pos.0 as i32,
                self.pos.1 as i32 + b.buoyancy_check_offset_y,
            )
            .filter(|cell| cell.material != AIR)
            .and_then(|cell| world.materials().get(cell.material))
            .is_some_and(|m| m.cell_type == CellType::Liquid && !m.liquid_sand);

        // 0xa7ed73: grounded, and Up or Fly pressed within jump_keydown_buffer frames.
        let buffer = i64::from(p.jump_keydown_buffer);
        let recent = |b: Button| b.down && b.since(frame) <= buffer;
        let (mut vx_j, mut vy_n) = (vx, vy);
        if self.on_ground && (recent(c.up) || recent(c.fly)) {
            vy_n = p.jump_velocity_y;
            if c.right.down {
                vx_j += p.jump_velocity_x;
            }
            if c.left.down {
                vx_j -= p.jump_velocity_x;
            }
        }

        // 0xa7ee0d: flying.
        let mut flying = false;
        if c.fly.down {
            if c.fly.frame + 1 == frame {
                self.smoothed_flying_target_y = c.flying_target_y;
            }
            if !(b.flying_needs_recharge && self.flying_time_left <= 0.0) {
                if p.fly_model_player {
                    if self.fly_throttle > -p.fly_speed_max_up {
                        self.fly_throttle -= p.fly_speed_change_spd / 60.0;
                    }
                    let t = if p.fly_speed_max_down > self.fly_throttle {
                        self.fly_throttle
                    } else {
                        p.fly_speed_max_down
                    };
                    self.fly_throttle = if t > -p.fly_speed_max_up {
                        t
                    } else {
                        -p.fly_speed_max_up
                    };
                    vy_n += self.fly_throttle / 60.0;
                } else {
                    self.smoothed_flying_target_y = if p.fly_smooth_y {
                        self.smoothed_flying_target_y
                            + (c.flying_target_y - self.smoothed_flying_target_y) * 0.05
                    } else {
                        c.flying_target_y
                    };
                    let t = p.fly_speed_mult * (self.smoothed_flying_target_y - self.pos.1)
                        - gravity_dt;
                    let t = if p.fly_speed_max_down > t {
                        t
                    } else {
                        p.fly_speed_max_down
                    };
                    let t = if t > -p.fly_speed_max_up {
                        t
                    } else {
                        -p.fly_speed_max_up
                    };
                    let next = vy_n + p.fly_speed_change_spd * (t - vy_n);
                    vy_n = if sign(t - next) != sign(t - vy_n) {
                        t
                    } else {
                        next
                    };
                }
                flying = true;
            }
        } else if self.on_ground {
            self.flying_time_left += b.fly_recharge_spd_ground * DT;
        } else if c.fly.since(frame) > i64::from(b.flying_in_air_wait_frames) {
            self.flying_time_left += b.fly_recharge_spd * DT;
        }
        if !flying {
            self.fly_throttle -= DT * self.fly_throttle * 20.0;
        }

        // 0xa7f0b7: drain while Fly is held, or shortly after a press while still rising.
        let removal = i64::from(b.flying_recharge_removal_frames);
        if c.fly.down || c.fly.since(frame) <= removal {
            let drain = c.fly.down || (c.fly.since(frame) <= removal && 0.0 > vy_n);
            if self.flying_time_left > 0.0 && drain && b.flying_needs_recharge && !in_liquid {
                self.flying_time_left -= DT;
            }
        }
        if self.jetpack_emitting != i32::from(flying) {
            self.jetpack_emitting = i32::from(flying);
        }

        // 0xa7f266
        vy_n += gravity_dt;
        let speed = if c.fly.down {
            p.fly_velocity_x
        } else {
            p.run_velocity
        };
        let speed = if p.velocity_max_x > speed {
            speed
        } else {
            p.velocity_max_x
        };
        let mut speed = if speed > p.velocity_min_x {
            speed
        } else {
            p.velocity_min_x
        };
        if c.run.down {
            speed *= 2.0;
        }
        let target = if c.left.down {
            -speed
        } else if c.right.down {
            speed
        } else {
            0.0
        };
        let mut vx_n = if self.on_slippery_ground {
            // 0xa7f337: weak control, and a push off an edge only one foot stands on.
            let k = if !c.left.down && !c.right.down {
                0.085
            } else {
                0.15
            };
            let mut v = (target - vx_j) * p.accel_x * k + vx_j;
            let fy = (b.aabb_max.1 + self.pos.1 + 1.0) as i32;
            let foot = |ax: f32| {
                world
                    .get((ax + self.pos.0) as i32, fy)
                    .is_some_and(|m| m != AIR)
            };
            match (foot(b.aabb_min.0), foot(b.aabb_max.0)) {
                (true, false) => v += 1.5,
                (false, true) => v -= 1.5,
                _ => {}
            }
            v
        } else if c.fly.down && !self.on_ground {
            vx_j + (target - vx_j) * p.accel_x_air
        } else {
            let next = vx_j + (target - vx_j) * p.accel_x;
            if sign(target - next) != sign(target - vx_j) {
                target
            } else {
                next
            }
        };

        // 0xa7f5f3: swimming.
        let not_swimming = self.frames_not_swimming;
        if in_liquid {
            self.frames_swimming += 1;
            self.frames_not_swimming = 0;
            let buoyancy = if c.down.down {
                p.swim_down_buoyancy_coeff
            } else if c.fly.down {
                p.swim_up_buoyancy_coeff
            } else {
                p.swim_idle_buoyancy_coeff
            };
            vy_n = p.swim_drag * (vy_n - buoyancy * gravity_dt);
            vx_n = p.swim_extra_horizontal_drag * (p.swim_drag * vx_n);
        } else {
            self.frames_not_swimming = not_swimming + 1;
            self.frames_swimming = 0;
        }

        // 0xa7f7a1
        let gs = b.ground_stickyness;
        if gs > 0.0 {
            let cap = gs / 3.0;
            vx_n -= (vx_n.abs() * 0.3).min(cap) * sign(vx_n);
            vy_n -= (vy_n.abs() * 0.3).min(cap) * sign(vy_n);
        }

        // 0xa7f856: only vy is clamped.
        let vy_n = if p.velocity_max_y > vy_n {
            vy_n
        } else {
            p.velocity_max_y
        };
        let vy_n = if vy_n > p.velocity_min_y {
            vy_n
        } else {
            p.velocity_min_y
        };
        self.vel = (vx_n, vy_n);
        vx_n
    }

    /// Facing and animation part of 0xa7ea90 (0xa7f8c4..). Movement is measured from the
    /// position at the previous call, so it reflects last frame's collision move.
    fn update_animation(&mut self, vx: f32, sprite: &dyn AnimationSet) {
        let p = &self.platforming;
        let c = self.controls;
        let frame = self.frame;
        let (dx, dy) = (
            self.pos.0 - self.ex_animation_pos.0,
            self.pos.1 - self.ex_animation_pos.1,
        );
        self.ex_animation_pos = self.pos;
        let old = self.facing;

        if p.mouse_look {
            let d = c.mouse.0 - self.pos.0;
            let buffer = p.mouse_look_buffer;
            if (-buffer > d && old > 0.0) || (d > buffer && 0.0 > old) {
                self.facing = -old;
            }
        } else if p.keyboard_look {
            let (l, r) = (c.left.down, c.right.down);
            if (l && !r && old > 0.0) || (!l && r && 0.0 > old) {
                self.facing = -old;
            }
        } else {
            let tb = p.turning_buffer;
            let turn = (-tb >= dx && old > 0.0) || (dx >= tb && 0.0 > old);
            if turn && frame > self.next_turn_animation_frame {
                self.next_turn_animation_frame = frame + p.turn_animation_frames_between as u64;
                self.facing = -old;
            }
        }

        let moving = if self.on_slippery_ground {
            c.left.down || c.right.down
        } else {
            dx.abs() > 0.1
        };
        let forwards = sign(dx) == self.facing;
        let crouch_name = |crouched: bool, a: &'static str, b: &'static str| {
            if crouched { b } else { a }
        };
        let anim = &mut self.animator;

        if !self.on_ground {
            if self.frames_not_swimming < 6 {
                anim.stop(sprite, "eat");
                if self.frames_in_air_counter > 3 {
                    if moving {
                        anim.play(sprite, "swim_move", 0, "walk");
                    } else {
                        anim.play(sprite, "swim_idle", 0, "stand");
                    }
                }
            } else if c.fly.down && self.frames_in_air_counter > 10 {
                if moving {
                    if sign(dx) == sign(c.aiming_vector.0) {
                        anim.play(sprite, "fly_move", 0, "walk");
                    } else if !anim.play(sprite, "fly_move_backwards", 0, "fly_move") {
                        anim.play(sprite, "walk", 0, "stand");
                    }
                } else if !anim.play(sprite, "fly_idle", 0, "") {
                    anim.play(sprite, "stand", 0, "");
                }
            } else if 0.0 > dy && (frame as i64 - self.last_frame_on_ground as i64) < 20 {
                anim.play(sprite, "jump_up", 0, "");
            } else if dy >= 0.0 && self.frames_in_air_counter > 4 {
                anim.play(sprite, "jump_fall", 0, "");
            }
            self.frames_in_air_counter += 1;
            return;
        }

        // 0xa80097: crouching (the ceiling sensor check at "crouch_sensor" is not ported).
        let mut crouch = c.down.down;
        if frame as i64 - self.last_posture_switch_frame > 10 {
            if crouch != self.should_crouch {
                self.last_posture_switch_frame = frame as i64;
            }
        } else {
            crouch = self.should_crouch;
        }
        self.should_crouch = crouch;

        if self.frames_in_air_counter > 0 {
            anim.stop(sprite, "knockback");
        }
        if self.frames_in_air_counter > 7 {
            anim.play(sprite, "land", 1, "");
        }
        self.frames_in_air_counter = 0;
        // 0xa803c5: a direction pressed this or last frame does not count as sliding.
        let slide_dx =
            if (0.0 > dx && c.left.since(frame) <= 1) || (dx > 0.0 && c.right.since(frame) <= 1) {
                0.0
            } else {
                dx
            };

        if moving {
            anim.stop(sprite, "eat");
            let run = if p.run_animation_velocity_switching_enabled {
                vx.abs() >= p.run_animation_velocity_switching_threshold
            } else {
                c.run.down
            };
            if run {
                anim.play(
                    sprite,
                    crouch_name(crouch, "run", "run_crouched"),
                    0,
                    crouch_name(crouch, "walk", "walk_crouched"),
                );
            } else if forwards {
                anim.play(
                    sprite,
                    crouch_name(crouch, "walk", "walk_crouched"),
                    0,
                    "walk",
                );
            } else {
                anim.play(
                    sprite,
                    crouch_name(crouch, "walk_backwards", "walk_backwards_crouched"),
                    0,
                    "walk_backwards",
                );
            }
        } else if self.on_slippery_ground && slide_dx.abs() > 0.2 {
            let top = anim.current().unwrap_or_default();
            if top != "slide" && top != "slide_crouched" {
                let slide = crouch_name(crouch, "slide", "slide_crouched");
                anim.play_then(
                    sprite,
                    crouch_name(crouch, "slide_start", "slide_start_crouched"),
                    0,
                    slide,
                    slide,
                );
            }
        } else if matches!(sprite.rect_animation(), "slide" | "slide_crouched") {
            anim.stop(sprite, "slide");
            anim.stop(sprite, "slide_crouched");
            anim.play_then(
                sprite,
                crouch_name(crouch, "slide_end", "slide_end_crouched"),
                1,
                "stand",
                crouch_name(crouch, "stand", "stand_crouched"),
            );
        } else {
            anim.play(
                sprite,
                crouch_name(crouch, "stand", "stand_crouched"),
                0,
                "stand",
            );
        }
    }

    /// Collision movement for platforming_type 2 (0xa659c0).
    fn move_and_collide(&mut self, world: &World) {
        let target = (self.pos.0 + self.vel.0 * DT, self.pos.1 + self.vel.1 * DT);
        self.on_ground = false;
        self.on_slippery_ground = false;
        self.collided_horizontally = false;
        self.move_toward(target, world);
        // 0xa65c31: grounded, moving down and not swimming (`mFramesNotSwimming > 0`).
        if self.on_ground && self.vel.1 > 0.0 && self.frames_not_swimming > 0 {
            self.vel.1 = 60.0;
        }
    }

    /// 0xa64850: step toward `target` at most 1 pixel at a time; stop at the first blocked step.
    fn move_toward(&mut self, target: (f32, f32), world: &World) {
        let (dx, dy) = (target.0 - self.pos.0, target.1 - self.pos.1);
        let mut remaining = (dx * dx + dy * dy).sqrt();
        if remaining <= 0.0 {
            return;
        }
        let dir = (dx / remaining, dy / remaining);
        remaining = remaining.min(1_048_576.0);
        while remaining > 0.0 {
            let step = remaining.min(1.0);
            let next = (self.pos.0 + dir.0 * step, self.pos.1 + dir.1 * step);
            remaining -= 1.0;
            match self.try_move(next, world) {
                Some(pos) => self.pos = pos,
                None => return,
            }
        }
    }

    /// 0xa64460: accept `pos` if clear, else try step-up, ceiling, then wall resolution.
    fn try_move(&mut self, pos: (f32, f32), world: &World) -> Option<(f32, f32)> {
        let (min, max) = (self.body.aabb_min, self.body.aabb_max);
        let Some((hx, hy)) = first_blocking(world, pos, min, max) else {
            return Some(pos);
        };
        let hit_slippery = world
            .get(hx, hy)
            .and_then(|m| world.materials().get(m))
            .is_some_and(|m| m.slippery);
        let (hx, hy) = (hx as f32, hy as f32);

        if self.vel.1 > 0.0 {
            let d = (pos.1 + max.1) - hy + 1.0;
            if self.body.climb_over_y as f32 >= d {
                let up = (pos.0, pos.1 - d);
                if first_blocking(world, up, min, max).is_none() {
                    self.on_slippery_ground = hit_slippery;
                    self.on_ground = true;
                    return Some(up);
                }
            }
        }

        if self.vel.1 < 0.0 && (hy - (pos.1 + min.1)).abs() <= 1.0 {
            self.vel.1 = 0.0;
            let down = (pos.0, hy - min.1 + 1.0);
            if first_blocking(world, down, min, max).is_none() {
                self.on_slippery_ground = hit_slippery;
                return Some(down);
            }
        }

        let mut dx = 0.0;
        if self.vel.0 < 0.0 && (hx - (pos.0 + min.0)).abs() <= 1.0 {
            self.vel.0 = 0.0;
            dx = hx - (pos.0 + min.0) + 1.0;
        }
        if self.vel.0 > 0.0 && (hx - (pos.0 + max.0)).abs() <= 1.0 {
            self.vel.0 = 0.0;
            dx = -((pos.0 + max.0) - hx + 1.0);
        }
        if dx != 0.0 {
            self.collided_horizontally = true;
            let side = (pos.0 + dx, pos.1);
            if first_blocking(world, side, min, max).is_none() {
                return Some(side);
            }
        }
        None
    }
}

/// 0x640270: first cell (rows, then columns) in the floored AABB that blocks characters.
pub fn first_blocking(
    world: &World,
    pos: (f32, f32),
    min: (f32, f32),
    max: (f32, f32),
) -> Option<(i32, i32)> {
    let (x0, x1) = (
        (pos.0 + min.0).floor() as i32,
        (pos.0 + max.0).floor() as i32,
    );
    let (y0, y1) = (
        (pos.1 + min.1).floor() as i32,
        (pos.1 + max.1).floor() as i32,
    );
    (y0..=y1).find_map(|y| (x0..=x1).find(|&x| blocks(world, x, y)).map(|x| (x, y)))
}

/// 0x63ef70: a platform cell blocks only if one of its neighbors (below, left, right, above) is
/// also a platform cell, so isolated pixels are passable.
pub fn blocks(world: &World, x: i32, y: i32) -> bool {
    is_platform(world, x, y)
        && [(0, 1), (-1, 0), (1, 0), (0, -1)]
            .iter()
            .any(|&(dx, dy)| is_platform(world, x + dx, y + dy))
}

/// `CLiquidCell` vt[11] (0x70b0b0) == 1: a `liquid_sand` cell that is static or resting, whose
/// material has a nonzero `platform_type` (default -1). Outside the world is not a platform.
pub fn is_platform(world: &World, x: i32, y: i32) -> bool {
    let Some(cell) = world.cell(x, y) else {
        return false;
    };
    if cell.material == AIR {
        return false;
    }
    let Some(m) = world.materials().get(cell.material) else {
        return false;
    };
    m.liquid_sand && m.platform_type != 0 && (cell.is_static || cell.state == sim::STATE_RESTING)
}

//! Minimal Bevy harness: draws the headless `sim` world as a nearest-neighbor texture.
//!
//! Materials and their textures are loaded at runtime from the extracted reference data
//! (`$NOITA_FILES/data/materials.xml`, default `<repo>/noita_files`).
//!
//! Controls: A/D walk, W jump/levitate (the reference binds `key_up` to Up and Fly), S crouch,
//! mouse aims (facing), Space pause/resume, Period step one frame while paused, R reset,
//! 1/2/3 pick the left-mouse brush (sand/water/rock_static), right mouse paint rock_static,
//! middle mouse erase, E explode at the cursor. The debug menu (top left) picks any material for
//! the left brush.

use std::path::PathBuf;
use std::process::ExitCode;

use actors::{AnimationSet, Body, Character, DT, Input, Platforming};
use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::RelativeCursorPosition;
use bevy::window::PrimaryWindow;
use noita_data::materials::MaterialDef;
use noita_data::sim_materials::{load_png, to_sim_with_reactions};
use noita_data::sprites::{self, SpriteDef};
use sim::particles::{ExplosionConfig, Particles};
use sim::{AIR, CellType, MaterialId, MaterialProps, Materials, World};

const WIDTH_CHUNKS: u32 = 4;
const HEIGHT_CHUNKS: u32 = 3;
const CHUNK_SIZE: u32 = 64;
/// Unrecovered: the reference's simulation rate.
const TICK_HZ: f64 = 60.0;
const BRUSH_RADIUS: i32 = 3;
/// Local placeholder; the reference draws background art behind air.
const BACKGROUND: [u8; 4] = [20, 20, 28, 255];

#[derive(Resource)]
struct Sim(World);

/// Flying cells and the explosion dispatcher (`grid::GridParticles`).
#[derive(Resource)]
struct Flying(Particles);

const PARTICLE_SEED: u32 = 1;

#[derive(Resource)]
struct Player(Character);

/// Held controls, sampled in `Update` and consumed by the fixed-step player update.
#[derive(Resource, Default)]
struct PlayerInput(Input);

/// `data/enemies_gfx/player.xml` and playback of the animator's top state.
#[derive(Resource)]
struct PlayerSheet {
    def: SpriteDef,
    image: Handle<Image>,
    playing: String,
    /// Seconds since `playing` started.
    time: f32,
}

struct Anims<'a>(&'a PlayerSheet);

impl AnimationSet for Anims<'_> {
    fn has(&self, name: &str) -> bool {
        self.0.def.animation(name).is_some()
    }

    fn rect_animation(&self) -> &str {
        &self.0.playing
    }
}

#[derive(Component)]
struct PlayerSprite;

const PLAYER_SPAWN: (f32, f32) = (60.0, 20.0);

/// Where `R` puts the player back.
#[derive(Resource)]
struct Spawn((f32, f32));

fn new_player(spawn: (f32, f32)) -> Character {
    Character::new(spawn, Platforming::player(), Body::player())
}

#[derive(Resource)]
struct Palette {
    table: Materials,
    /// The parsed `materials.xml`; `defs[i]` is material `MaterialId(i)`.
    defs: Vec<MaterialDef>,
    /// Left-mouse brush presets for keys 1..: sand, water, rock_static.
    brushes: [MaterialId; 3],
    /// English `ui_name` per material index; empty when untranslated.
    ui_names: Vec<String>,
}

impl Palette {
    /// `Translated name (internal_name)`, or just the internal name when there is no translation.
    fn label(&self, id: MaterialId) -> String {
        let internal = &self.table.get(id).expect("known").name;
        match self
            .ui_names
            .get(usize::from(id.0))
            .filter(|n| !n.is_empty())
        {
            Some(ui) if !ui.eq_ignore_ascii_case(internal) => format!("{ui} ({internal})"),
            _ => internal.clone(),
        }
    }
}

#[derive(Resource)]
struct Control {
    paused: bool,
    step_requested: bool,
    /// Left-mouse material.
    brush: MaterialId,
}

#[derive(Resource)]
struct Canvas(Handle<Image>);

fn noita_files() -> PathBuf {
    std::env::var_os("NOITA_FILES").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files"),
        PathBuf::from,
    )
}

fn load_palette() -> Result<Palette, String> {
    let root = noita_files();
    let path = root.join("data/materials.xml");
    let xml = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "cannot read {}: {e}\nExtract the reference data first (see README: Reference files).",
            path.display()
        )
    })?;
    let defs =
        noita_data::materials::parse(&xml).map_err(|e| format!("{}: {e}", path.display()))?;
    let reactions = noita_data::materials::parse_reactions(&xml)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let required = noita_data::materials::parse_required_reactions(&xml)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let table = to_sim_with_reactions(&defs, &reactions, &required, |f| {
        load_png(&root.join(f))
            .inspect_err(|e| eprintln!("texture skipped: {e}"))
            .ok()
    });
    let find = |name| table.find(name).ok_or(format!("material {name} missing"));
    let brushes = [find("sand")?, find("water")?, find("rock_static")?];
    // Optional: copy data/translations/common.csv next to materials.xml for readable names.
    let english = std::fs::read(root.join("data/translations/common.csv"))
        .map(|b| noita_data::translations::parse_english(&String::from_utf8_lossy(&b)))
        .unwrap_or_default();
    let ui_of = |name: &str| {
        defs.iter()
            .find(|d| d.name == name)
            .and_then(|d| d.attr("ui_name"))
            .and_then(|ui| noita_data::translations::lookup(&english, ui))
            .unwrap_or_default()
            .to_owned()
    };
    let ui_names = (0..table.len())
        .map(|i| ui_of(&table.get(MaterialId(i as u16)).expect("in range").name))
        .collect();
    Ok(Palette {
        table,
        defs,
        brushes,
        ui_names,
    })
}

fn load_player_sprite() -> Result<(SpriteDef, sim::Texture), String> {
    let root = noita_files();
    let path = root.join("data/enemies_gfx/player.xml");
    let xml = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let def = sprites::parse(&xml).map_err(|e| format!("{}: {e}", path.display()))?;
    let sheet = load_png(&root.join(&def.filename))?;
    Ok((def, sheet))
}

fn main() -> ExitCode {
    let (palette, (sheet_def, sheet)) =
        match load_palette().and_then(|p| Ok((p, load_player_sprite()?))) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
    let spawn = scene_spawn(&palette);
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "noita-rust sandbox".into(),
                        resolution: (1024, 768).into(),
                        ..default()
                    }),
                    ..default()
                }),
            FrameTimeDiagnosticsPlugin::default(),
            LogDiagnosticsPlugin::default(),
        ))
        .insert_resource(Time::<Fixed>::from_hz(TICK_HZ))
        .insert_resource(Control {
            paused: false,
            step_requested: false,
            brush: palette.brushes[0],
        })
        .insert_resource(palette)
        .insert_resource(SheetSource(Some((sheet_def, sheet))))
        .init_resource::<PlayerInput>()
        .insert_resource(Player(new_player(spawn)))
        .insert_resource(Spawn(spawn))
        .insert_resource(Flying(Particles::new(PARTICLE_SEED)))
        .add_systems(Startup, (setup, spawn_debug_menu))
        .add_systems(FixedUpdate, step_sim)
        .add_systems(
            Update,
            (
                menu_buttons,
                scroll_menu,
                update_brush_label,
                read_player_input,
                handle_input,
                paint,
                explode_at_cursor,
                upload_texture,
                sync_player_sprite,
                update_title,
            )
                .chain(),
        )
        .run();
    ExitCode::SUCCESS
}

/// The world to play in: the test scene, or a generated biome when `NOITA_BIOME` names one
/// (`NOITA_SEED` picks the seed; default 12345). See `worldgen::generate`: only the Wang
/// terrain is generated, not structures, spawn functions or pixel scenes.
fn build_world(p: &Palette) -> World {
    let Some(biome) = std::env::var_os("NOITA_BIOME") else {
        return test_scene(p);
    };
    let biome = biome.to_string_lossy().into_owned();
    let seed: u32 = std::env::var("NOITA_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12345);
    let mut world = World::new(WIDTH_CHUNKS, HEIGHT_CHUNKS, CHUNK_SIZE, p.table.clone(), 1);
    let (w, h) = (world.width() as usize, world.height() as usize);
    // Start a little inside the first biome map chunk that shows the biome.
    match worldgen::generate::generate_biome(
        &noita_files(),
        &p.defs,
        &biome,
        seed,
        (64, 64),
        (w, h),
    ) {
        Ok(area) => {
            for (i, &m) in area.materials.iter().enumerate() {
                let (x, y) = ((i % w) as i32, (i / w) as i32);
                world.set(x, y, MaterialId(m)).expect("in bounds");
            }
            world
        }
        Err(e) => {
            eprintln!("NOITA_BIOME={biome}: {e}; using the test scene");
            test_scene(p)
        }
    }
}

/// Where the player starts: the first standing spot (air above solid ground) of a generated
/// world, else the test scene's spawn.
fn scene_spawn(p: &Palette) -> (f32, f32) {
    if std::env::var_os("NOITA_BIOME").is_none() {
        return PLAYER_SPAWN;
    }
    let world = build_world(p);
    let solid = |x, y| {
        world
            .get(x, y)
            .and_then(|m| p.table.get(m))
            .is_some_and(|m| m.cell_type == CellType::Solid)
    };
    for y in 20..world.height() - 2 {
        for x in 12..world.width() - 12 {
            let roomy =
                (y - 14..=y).all(|yy| (x - 3..=x + 3).all(|xx| world.get(xx, yy) == Some(AIR)));
            if roomy && solid(x, y + 1) {
                return (x as f32, y as f32 - 6.0);
            }
        }
    }
    PLAYER_SPAWN
}

fn test_scene(p: &Palette) -> World {
    let [sand, water, rock] = p.brushes;
    let mut world = World::new(WIDTH_CHUNKS, HEIGHT_CHUNKS, CHUNK_SIZE, p.table.clone(), 1);
    let (w, h) = (world.width(), world.height());
    for y in h - 4..h {
        for x in 0..w {
            world.set(x, y, rock).expect("in bounds");
        }
    }
    for x in w / 8..w / 2 {
        world.set(x, h / 2, rock).expect("in bounds");
    }
    for x in w / 2..w * 7 / 8 {
        world.set(x, h * 3 / 4, rock).expect("in bounds");
    }
    for y in h * 3 / 4 - 12..h * 3 / 4 {
        world.set(w * 7 / 8, y, rock).expect("in bounds");
        for x in w * 5 / 8..w * 7 / 8 {
            world.set(x, y, water).expect("in bounds");
        }
    }
    for y in 4..40 {
        for x in w / 4..w * 3 / 4 {
            world.set(x, y, sand).expect("in bounds");
        }
    }
    world
}

/// Loaded before the app starts; moved into `PlayerSheet` once the image asset exists.
#[derive(Resource)]
struct SheetSource(Option<(SpriteDef, sim::Texture)>);

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    palette: Res<Palette>,
    mut source: ResMut<SheetSource>,
) {
    let world = build_world(&palette);
    let (w, h) = (world.width() as u32, world.height() as u32);
    let mut image = Image::new_fill(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &BACKGROUND,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    write_pixels(&world, image.data.as_mut().expect("filled image has data"));
    let handle = images.add(image);

    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: w as f32,
                min_height: h as f32,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn(Sprite::from_image(handle.clone()));
    let (def, sheet) = source.0.take().expect("setup runs once");
    let sheet_image = images.add(Image::new(
        Extent3d {
            width: sheet.width(),
            height: sheet.height(),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        sheet.rgba().to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ));
    commands.spawn((
        Sprite::from_image(sheet_image.clone()),
        Transform::from_xyz(0.0, 0.0, 1.0),
        PlayerSprite,
    ));
    commands.insert_resource(PlayerSheet {
        playing: def.default_animation.clone(),
        def,
        image: sheet_image,
        time: 0.0,
    });
    commands.insert_resource(Canvas(handle));
    commands.insert_resource(Sim(world));
}

fn step_sim(
    mut sim: ResMut<Sim>,
    mut flying: ResMut<Flying>,
    mut control: ResMut<Control>,
    input: Res<PlayerInput>,
    mut player: ResMut<Player>,
    mut sheet: ResMut<PlayerSheet>,
) {
    if !control.paused || std::mem::take(&mut control.step_requested) {
        sim.0.step();
        let time = sim.0.tick() as f64 * f64::from(DT);
        flying.0.update(&mut sim.0, DT, time);
        flying.0.run_explosions(&mut sim.0);
        let sheet = &mut *sheet;
        let character = &mut player.0;
        character.step(&input.0, &sim.0, &Anims(&*sheet));
        advance_animation(sheet, &mut character.animator);
    }
}

/// `SpriteAnimatorSystem` (0xc1b820): a finished animation pops the top state (playing its
/// follow-up or `next_animation`); the sprite then switches to the top state, restarting only
/// when the name changes. Frame timing is local (`frame_wait` seconds of 1/60 s ticks), since the
/// sprite update itself is not yet recovered.
fn advance_animation(sheet: &mut PlayerSheet, animator: &mut actors::Animator) {
    if let Some(a) = sheet.def.animation(&sheet.playing)
        && !a.looping
        && sheet.time >= a.frame_wait * a.frame_count as f32
    {
        let next = a.next_animation.clone();
        animator.finished(&Anims(&*sheet), &next);
    }
    if let Some(top) = animator.current()
        && top != sheet.playing
    {
        sheet.playing = top.to_owned();
        sheet.time = 0.0;
        return;
    }
    sheet.time += DT;
}

fn read_player_input(
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    sim: Res<Sim>,
    spawn: Res<Spawn>,
    mut input: ResMut<PlayerInput>,
    mut player: ResMut<Player>,
) {
    let (camera, camera_transform) = *camera;
    let mouse = window
        .cursor_position()
        .and_then(|c| camera.viewport_to_world_2d(camera_transform, c).ok())
        .map_or(input.0.mouse, |p| from_screen(&sim.0, p));
    let w = keys.pressed(KeyCode::KeyW);
    input.0 = Input {
        left: keys.pressed(KeyCode::KeyA),
        right: keys.pressed(KeyCode::KeyD),
        up: w,
        down: keys.pressed(KeyCode::KeyS),
        fly: w,
        run: false,
        mouse,
    };
    if keys.just_pressed(KeyCode::KeyR) {
        player.0 = new_player(spawn.0);
    }
}

/// Sim coordinates (y down, origin top-left) to Bevy world coordinates (y up, origin centered).
fn to_screen(world: &World, (x, y): (f32, f32)) -> Vec2 {
    Vec2::new(
        x - world.width() as f32 / 2.0,
        world.height() as f32 / 2.0 - y,
    )
}

fn from_screen(world: &World, p: Vec2) -> (f32, f32) {
    (
        p.x + world.width() as f32 / 2.0,
        world.height() as f32 / 2.0 - p.y,
    )
}

/// Current frame of the playing animation: `(frame index, animation)`.
fn current_frame(sheet: &PlayerSheet) -> Option<(u32, &noita_data::sprites::RectAnimation)> {
    let a = sheet.def.animation(&sheet.playing)?;
    let n = a.frame_count.max(1);
    let i = if a.frame_wait > 0.0 {
        (sheet.time / a.frame_wait) as u32
    } else {
        0
    };
    Some((if a.looping { i % n } else { i.min(n - 1) }, a))
}

fn sync_player_sprite(
    sim: Res<Sim>,
    player: Res<Player>,
    sheet: Res<PlayerSheet>,
    mut sprite: Single<(&mut Transform, &mut Sprite), With<PlayerSprite>>,
) {
    let Some((i, a)) = current_frame(&sheet) else {
        return;
    };
    let (x, y, w, h) = a.frame_rect(i);
    let (transform, sprite) = &mut *sprite;
    sprite.image = sheet.image.clone();
    sprite.rect = Some(Rect::new(
        x as f32,
        y as f32,
        (x + w) as f32,
        (y + h) as f32,
    ));
    let c = &player.0;
    sprite.flip_x = c.facing < 0.0;
    // Sprite origin is `offset` from the frame's top-left; facing mirrors it around the entity.
    let local_x = (w as f32 / 2.0 - sheet.def.offset_x) * c.facing;
    let local_y = h as f32 / 2.0 - sheet.def.offset_y;
    let center = to_screen(&sim.0, (c.pos.0 + local_x, c.pos.1 + local_y));
    transform.translation = center.extend(1.0);
}

fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    palette: Res<Palette>,
    mut control: ResMut<Control>,
    mut sim: ResMut<Sim>,
    mut flying: ResMut<Flying>,
) {
    if keys.just_pressed(KeyCode::Space) {
        control.paused = !control.paused;
    }
    if keys.just_pressed(KeyCode::Period) && control.paused {
        control.step_requested = true;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        sim.0 = build_world(&palette);
        flying.0 = Particles::new(PARTICLE_SEED);
    }
    for (i, key) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
        .into_iter()
        .enumerate()
    {
        if keys.just_pressed(key) {
            control.brush = palette.brushes[i];
        }
    }
}

fn paint(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    menu: Single<&RelativeCursorPosition, With<MenuRoot>>,
    palette: Res<Palette>,
    control: Res<Control>,
    mut sim: ResMut<Sim>,
) {
    if menu.cursor_over() {
        return;
    }
    let brush = [
        (MouseButton::Left, control.brush),
        (MouseButton::Right, palette.brushes[2]),
        (MouseButton::Middle, AIR),
    ]
    .into_iter()
    .find(|(button, _)| mouse.pressed(*button));
    let Some((_, material)) = brush else { return };
    let (camera, camera_transform) = *camera;
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(pos) = camera.viewport_to_world_2d(camera_transform, cursor) else {
        return;
    };

    let world = &mut sim.0;
    // Sprite is centered on the origin with one world unit per cell; world y points up.
    let cx = (pos.x + world.width() as f32 / 2.0).floor() as i32;
    let cy = (world.height() as f32 / 2.0 - pos.y).floor() as i32;
    for dy in -BRUSH_RADIUS..=BRUSH_RADIUS {
        for dx in -BRUSH_RADIUS..=BRUSH_RADIUS {
            if dx * dx + dy * dy <= BRUSH_RADIUS * BRUSH_RADIUS {
                let _ = world.set(cx + dx, cy + dy, material);
            }
        }
    }
}

fn upload_texture(
    sim: Res<Sim>,
    flying: Res<Flying>,
    canvas: Res<Canvas>,
    mut images: ResMut<Assets<Image>>,
) {
    if !sim.is_changed() && !flying.is_changed() {
        return;
    }
    if let Some(mut image) = images.get_mut(&canvas.0)
        && let Some(data) = image.data.as_mut()
    {
        write_pixels(&sim.0, data);
        write_particles(&sim.0, &flying.0, data);
    }
}

/// Particles drawn over the world by their position and color.
fn write_particles(world: &World, flying: &Particles, rgba: &mut [u8]) {
    for p in &flying.particles {
        let (x, y) = (p.pos.0 as i32, p.pos.1 as i32);
        if !world.in_bounds(x, y) {
            continue;
        }
        let i = ((y * world.width() + x) * 4) as usize;
        let byte = |v: f32| (v * 255.0) as u8;
        let alpha = p.color[3];
        let mix = |s: f32, bg: u8| byte(s * alpha + f32::from(bg) / 255.0 * (1.0 - alpha));
        rgba[i] = mix(p.color[0], rgba[i]);
        rgba[i + 1] = mix(p.color[1], rgba[i + 1]);
        rgba[i + 2] = mix(p.color[2], rgba[i + 2]);
    }
}

/// E: a default explosion (`ExplosionConfig` constructor defaults) at the cursor.
fn explode_at_cursor(
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut sim: ResMut<Sim>,
    mut flying: ResMut<Flying>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    let (camera, camera_transform) = *camera;
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(pos) = camera.viewport_to_world_2d(camera_transform, cursor) else {
        return;
    };
    let world = &mut sim.0;
    let center = (
        (pos.x + world.width() as f32 / 2.0).floor() + 0.5,
        (world.height() as f32 / 2.0 - pos.y).floor() + 0.5,
    );
    flying.0.explode(world, &ExplosionConfig::default(), center);
}

fn update_title(
    sim: Res<Sim>,
    control: Res<Control>,
    player: Res<Player>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    if sim.is_changed() || control.is_changed() {
        let state = if control.paused { " [paused]" } else { "" };
        let brush = &sim.0.materials().get(control.brush).expect("known").name;
        window.title = format!(
            "noita-rust sandbox - frame {} - brush {brush} - fly {:.2}s{state}",
            sim.0.tick(),
            player.0.flying_time_left
        );
    }
}

#[derive(Component)]
struct MenuRoot;

/// Scrolling container below the menu header.
#[derive(Component)]
struct MenuBody;

#[derive(Component)]
struct BrushLabel;

/// Header button that shows or hides `target`.
#[derive(Component)]
struct Toggle {
    target: Entity,
    title: String,
}

#[derive(Component)]
struct PickMaterial(MaterialId);

const MENU_FONT_SIZE: f32 = 13.0;
const MENU_BG: Color = Color::srgba(0.06, 0.06, 0.09, 0.9);
const HEADER_BG: Color = Color::srgb(0.16, 0.16, 0.22);
const ITEM_BG: Color = Color::NONE;
const HOVER_BG: Color = Color::srgb(0.28, 0.28, 0.36);
const CATEGORIES: [&str; 7] = ["Sand", "Liquid", "Static", "Gas", "Fire", "Solid", "Other"];

fn category(m: &MaterialProps) -> &'static str {
    match m.cell_type {
        CellType::Liquid if m.liquid_static => "Static",
        CellType::Liquid if m.liquid_sand => "Sand",
        CellType::Liquid => "Liquid",
        CellType::Gas => "Gas",
        CellType::Fire => "Fire",
        CellType::Solid => "Solid",
        CellType::Unspecified => "Other",
    }
}

fn menu_text(text: impl Into<String>) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: MENU_FONT_SIZE.into(),
            ..default()
        },
        TextColor(Color::WHITE),
    )
}

fn toggle_label(title: &str, open: bool) -> String {
    format!("{} {title}", if open { "[-]" } else { "[+]" })
}

/// A header button followed by `content`; `open` must match the content's `display`.
fn spawn_section(commands: &mut Commands, title: String, content: Entity, open: bool) -> Entity {
    let header = commands
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(HEADER_BG),
        ))
        .with_child(menu_text(toggle_label(&title, open)))
        .insert(Toggle {
            target: content,
            title,
        })
        .id();
    commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .add_children(&[header, content])
        .id()
}

fn spawn_debug_menu(mut commands: Commands, palette: Res<Palette>) {
    let table = &palette.table;
    let mut groups: Vec<(&str, Vec<(MaterialId, &MaterialProps)>)> =
        CATEGORIES.iter().map(|&c| (c, Vec::new())).collect();
    for id in (1..table.len()).map(|i| MaterialId(i as u16)) {
        let m = table.get(id).expect("in range");
        let group = groups
            .iter_mut()
            .find(|(c, _)| *c == category(m))
            .expect("every category is listed");
        group.1.push((id, m));
    }

    let mut body_children = vec![
        commands
            .spawn((
                BrushLabel,
                menu_text(""),
                Node {
                    padding: UiRect::all(Val::Px(4.0)),
                    ..default()
                },
            ))
            .id(),
    ];
    for (name, mut list) in groups.into_iter().filter(|(_, l)| !l.is_empty()) {
        list.sort_by_cached_key(|&(id, _)| palette.label(id).to_lowercase());
        let items: Vec<Entity> = list
            .iter()
            .map(|&(id, m)| {
                let [a, r, g, b] = m.color_at(0, 0).to_be_bytes();
                commands
                    .spawn((
                        Button,
                        PickMaterial(id),
                        Node {
                            column_gap: Val::Px(6.0),
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(Val::Px(14.0), Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(ITEM_BG),
                    ))
                    .with_child((
                        Node {
                            width: Val::Px(10.0),
                            height: Val::Px(10.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba_u8(r, g, b, a.max(64))),
                    ))
                    .with_child(menu_text(palette.label(id)))
                    .id()
            })
            .collect();
        let content = commands
            .spawn(Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                ..default()
            })
            .add_children(&items)
            .id();
        let title = format!("{name} ({})", list.len());
        body_children.push(spawn_section(&mut commands, title, content, false));
    }

    let body = commands
        .spawn((
            MenuBody,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                max_height: Val::Vh(85.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
        ))
        .add_children(&body_children)
        .id();
    let menu = spawn_section(&mut commands, "Debug".to_owned(), body, true);
    commands.entity(menu).insert((
        MenuRoot,
        RelativeCursorPosition::default(),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(8.0),
            top: Val::Px(8.0),
            width: Val::Px(230.0),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        BackgroundColor(MENU_BG),
    ));
}

type MenuButton<'a> = (
    &'a Interaction,
    Option<&'a Toggle>,
    Option<&'a PickMaterial>,
    &'a Children,
    &'a mut BackgroundColor,
);

fn menu_buttons(
    mut buttons: Query<MenuButton, Changed<Interaction>>,
    mut nodes: Query<&mut Node>,
    mut texts: Query<&mut Text>,
    mut control: ResMut<Control>,
) {
    for (interaction, toggle, pick, children, mut background) in &mut buttons {
        let idle = if toggle.is_some() { HEADER_BG } else { ITEM_BG };
        background.0 = match interaction {
            Interaction::None => idle,
            Interaction::Hovered | Interaction::Pressed => HOVER_BG,
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Some(&PickMaterial(id)) = pick {
            control.brush = id;
        }
        if let Some(toggle) = toggle
            && let Ok(mut node) = nodes.get_mut(toggle.target)
        {
            let open = node.display == Display::None;
            node.display = if open { Display::Flex } else { Display::None };
            if let Some(label) = children.iter().find(|&c| texts.contains(c))
                && let Ok(mut text) = texts.get_mut(label)
            {
                text.0 = toggle_label(&toggle.title, open);
            }
        }
    }
}

fn scroll_menu(
    scroll: Res<AccumulatedMouseScroll>,
    menu: Single<&RelativeCursorPosition, With<MenuRoot>>,
    body: Single<(&mut ScrollPosition, &ComputedNode), With<MenuBody>>,
) {
    if scroll.delta.y == 0.0 || !menu.cursor_over() {
        return;
    }
    let line = match scroll.unit {
        MouseScrollUnit::Line => 3.0 * MENU_FONT_SIZE,
        MouseScrollUnit::Pixel => 1.0,
    };
    let (mut position, node) = body.into_inner();
    let max = ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0);
    position.0.y = (position.0.y - scroll.delta.y * line).clamp(0.0, max);
}

fn update_brush_label(
    control: Res<Control>,
    palette: Res<Palette>,
    mut label: Single<&mut Text, With<BrushLabel>>,
) {
    if control.is_changed() {
        label.0 = format!("Left brush: {}", palette.label(control.brush));
    }
}

/// Straight-alpha ARGB over the opaque background, as RGBA.
fn over_background(argb: u32) -> [u8; 4] {
    let [a, r, g, b] = argb.to_be_bytes();
    let a = u16::from(a);
    let mix = |s: u8, bg: u8| ((u16::from(s) * a + u16::from(bg) * (255 - a)) / 255) as u8;
    [
        mix(r, BACKGROUND[0]),
        mix(g, BACKGROUND[1]),
        mix(b, BACKGROUND[2]),
        255,
    ]
}

/// Writes each cell's own color into a row-major RGBA8 buffer of `width * height * 4` bytes.
fn write_pixels(world: &World, rgba: &mut [u8]) {
    let w = world.width();
    for y in 0..world.height() {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let c = match world.cell(x, y) {
                Some(cell) if cell.material != AIR => over_background(cell.color),
                _ => BACKGROUND,
            };
            rgba[i..i + 4].copy_from_slice(&c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{CellType, MaterialProps};

    #[test]
    fn write_pixels_uses_cell_colors_row_major() {
        let mat = |name, color| MaterialProps {
            color,
            ..MaterialProps::new(name, CellType::Liquid)
        };
        let table = Materials::new(vec![
            MaterialProps::new("air", CellType::Unspecified),
            mat("a", 0xff01_0203),
            mat("b", 0xff04_0506),
        ]);
        let mut world = World::new(2, 1, 2, table, 1);
        world.set(3, 1, MaterialId(1)).unwrap();
        world.set(0, 0, MaterialId(2)).unwrap();
        let mut rgba = vec![0; 4 * 2 * 4];
        write_pixels(&world, &mut rgba);
        assert_eq!(rgba[0..4], [4, 5, 6, 255]);
        assert_eq!(rgba[(4 + 3) * 4..(4 + 3) * 4 + 4], [1, 2, 3, 255]);
        assert_eq!(rgba[4..8], BACKGROUND);
    }

    #[test]
    fn translucent_colors_blend_over_background() {
        assert_eq!(over_background(0xffc8_6432), [200, 100, 50, 255]);
        assert_eq!(over_background(0), BACKGROUND);
        let half = over_background(0x80ff_ffff);
        assert!(half[0] > BACKGROUND[0] && half[0] < 255);
    }
}

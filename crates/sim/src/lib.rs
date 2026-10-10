//! Headless material grid. Deliberately independent of Bevy or any other game framework.
//!
//! Update rules are ported from the reference executable; addresses in comments refer to the
//! reference build recorded in the README. Coordinates: `x` right, `y` down (as in the reference).

use std::fmt;
use std::sync::Arc;

pub mod noise;
pub mod particles;
mod reactions;

pub use reactions::{Direction, Reaction};

/// Index into [`Materials`]. Index 0 is always air (empty space), as in `materials.xml`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaterialId(pub u16);

pub const AIR: MaterialId = MaterialId(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellType {
    Liquid,
    Gas,
    Solid,
    Fire,
    /// No `cell_type` given; such cells are not simulated (engine default unrecovered).
    Unspecified,
}

/// Simulation-relevant `CellData` fields. Defaults are the `CellData` constructor's (0x4ac9f3..).
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialProps {
    pub name: String,
    pub tags: Vec<String>,
    pub cell_type: CellType,
    pub density: f32,
    pub liquid_sand: bool,
    pub liquid_slime: bool,
    pub liquid_static: bool,
    pub liquid_stains_self: bool,
    /// `+0x170`: bit 1 darkens the neighbor toward 0.7 of its base color, bit 2 tints it toward
    /// this cell's stain color, bit 4 pulls it toward a mix of that and its base color.
    pub liquid_stains: u32,
    /// `+0x174`: stain color (ARGB) used instead of the cell's own color when nonzero.
    pub liquid_stains_custom_color: u32,
    /// `+0x8D`: neighbors of other cells may stain it (constructor default 1).
    pub stainable: bool,
    pub liquid_gravity: f32,
    pub liquid_viscosity: i32,
    pub liquid_sticks_to_ceiling: i32,
    pub liquid_damping: f32,
    pub liquid_flow_speed: f32,
    /// `+0x190`
    pub liquid_sand_never_box2d: bool,
    /// `+0x10D`: characters landing on it get `is_on_slippery_ground`.
    pub slippery: bool,
    /// `platform_type` (`+0x3C`, default -1); nonzero lets resting/static sand cells carry characters.
    pub platform_type: i32,
    /// `+0x154`: gas cells are removed once their move count reaches it (0 = never).
    pub lifetime: i32,
    /// `+0x194..+0x197`: percent chances per update (gas cells).
    pub gas_speed: u8,
    pub gas_upwards_speed: u8,
    pub gas_horizontal_speed: u8,
    pub gas_downwards_speed: u8,
    pub burnable: bool,
    pub on_fire: bool,
    pub fire_hp: i32,
    pub autoignition_temperature: i32,
    pub temperature_of_fire: i32,
    pub generates_smoke: i32,
    pub generates_flames: i32,
    pub requires_oxygen: bool,
    pub on_fire_convert_to_material: String,
    pub on_fire_flame_material: String,
    pub on_fire_smoke_material: String,
    /// `<Graphics color>` (`+0x64`), ARGB; used when there is no texture.
    pub color: u32,
    /// `<Graphics texture_file>` image (`+0x88`).
    pub texture: Option<Arc<Texture>>,
}

impl MaterialProps {
    pub fn new(name: impl Into<String>, cell_type: CellType) -> Self {
        Self {
            name: name.into(),
            tags: Vec::new(),
            cell_type,
            density: 1.0,
            liquid_sand: false,
            liquid_slime: false,
            liquid_static: false,
            liquid_stains_self: false,
            liquid_stains: 0,
            liquid_stains_custom_color: 0,
            stainable: true,
            liquid_gravity: 0.5,
            liquid_viscosity: 50,
            liquid_sticks_to_ceiling: 0,
            liquid_damping: 0.8,
            liquid_flow_speed: 0.9,
            liquid_sand_never_box2d: false,
            slippery: false,
            platform_type: -1,
            lifetime: 0,
            gas_speed: 50,
            gas_upwards_speed: 100,
            gas_horizontal_speed: 100,
            gas_downwards_speed: 90,
            burnable: false,
            on_fire: false,
            // Fire defaults from the CellData constructor (0x4ac88e..0x4ac96a).
            fire_hp: 0,
            autoignition_temperature: 100,
            temperature_of_fire: 10,
            generates_smoke: 0,
            generates_flames: 30,
            requires_oxygen: true,
            on_fire_convert_to_material: String::new(),
            on_fire_flame_material: "fire".to_owned(),
            on_fire_smoke_material: "smoke".to_owned(),
            color: 0,
            texture: None,
        }
    }

    /// Color of a cell created at `(x, y)` (0x7044a0; the `randomize_colors` jitter path is unported
    /// because no reference material sets it).
    pub fn color_at(&self, x: i32, y: i32) -> u32 {
        self.texture.as_ref().map_or(self.color, |t| t.sample(x, y))
    }
}

/// RGBA8 image, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture {
    width: i32,
    height: i32,
    rgba: Vec<u8>,
}

impl Texture {
    /// `None` unless both sides are positive and `rgba.len() == width * height * 4`.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        let (w, h) = (i32::try_from(width).ok()?, i32::try_from(height).ok()?);
        let len = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        (w > 0 && h > 0 && rgba.len() == len).then_some(Self {
            width: w,
            height: h,
            rgba,
        })
    }

    pub fn width(&self) -> u32 {
        self.width as u32
    }

    pub fn height(&self) -> u32 {
        self.height as u32
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// Tiled lookup (0x7042c0): both axes wrap; returns ARGB.
    pub fn sample(&self, x: i32, y: i32) -> u32 {
        let i = ((y.rem_euclid(self.height) * self.width + x.rem_euclid(self.width)) * 4) as usize;
        let [r, g, b, a] = [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ];
        u32::from_be_bytes([a, r, g, b])
    }
}

#[derive(Clone, Debug)]
pub struct Materials {
    props: Vec<MaterialProps>,
    reactions: Vec<ReactionProps>,
    tables: reactions::ReactionTables,
}

/// A `<Reaction>` record from `materials.xml` with its raw attributes.
#[derive(Clone, Debug, PartialEq)]
pub struct ReactionProps {
    pub attrs: std::collections::BTreeMap<String, String>,
    pub explosion_config: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReactionInputSelector {
    Material(String),
    Tag(String),
    AnyLiquid,
}

impl ReactionInputSelector {
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.starts_with('[') || value.ends_with(']') {
            let tag = value.strip_prefix('[')?.strip_suffix(']')?;
            if tag.is_empty() {
                return None;
            }
            return Some(if tag == "any_liquid" {
                Self::AnyLiquid
            } else {
                Self::Tag(tag.to_owned())
            });
        }
        (!value.is_empty()).then(|| Self::Material(value.to_owned()))
    }

    pub fn matches(&self, material: &MaterialProps) -> bool {
        match self {
            Self::Material(name) => material.name == *name,
            Self::Tag(tag) => material.tags.iter().any(|material_tag| material_tag == tag),
            // 0x7061f0: liquids that are not `liquid_sand` (`+0x160`).
            Self::AnyLiquid => material.cell_type == CellType::Liquid && !material.liquid_sand,
        }
    }
}

impl Materials {
    /// `props[0]` is treated as air regardless of its fields.
    pub fn new(props: Vec<MaterialProps>) -> Self {
        assert!(!props.is_empty(), "material table needs air at index 0");
        assert!(props.len() <= usize::from(u16::MAX), "too many materials");
        Self {
            props,
            reactions: Vec::new(),
            tables: reactions::ReactionTables::default(),
        }
    }

    /// Attaches `materials.xml` reaction records and builds the lookup tables from them.
    pub fn with_reactions(mut self, reactions: Vec<ReactionProps>) -> Self {
        self.tables = reactions::ReactionTables::build(&self.props, &reactions);
        self.reactions = reactions;
        self
    }

    /// Attaches `<ReqReaction>` records (`Reactions_AddRequired` `0x706da0`); call after
    /// [`Self::with_reactions`], which rebuilds the other tables.
    pub fn with_required_reactions(mut self, reactions: Vec<ReactionProps>) -> Self {
        self.tables.add_required(&self.props, &reactions);
        self
    }

    /// Records for a reacting `own` cell next to `neighbor`, in table order.
    pub fn reactions_between(
        &self,
        fast: bool,
        own: MaterialId,
        neighbor: MaterialId,
    ) -> &[Reaction] {
        self.tables.list(fast, own, neighbor)
    }

    /// `CellData+0x228` (normal) or `+0x22C` (fast): the largest probability among the records
    /// whose first input is `id`; 0 means the material never starts such a reaction.
    pub fn reaction_max_probability(&self, fast: bool, id: MaterialId) -> i32 {
        self.tables.max_probability(fast, id)
    }

    /// `CellData+0x230`: the largest `<ReqReaction>` probability for `id`.
    pub fn required_reaction_max_probability(&self, id: MaterialId) -> i32 {
        self.tables.max_required_probability(id)
    }

    pub fn get(&self, id: MaterialId) -> Option<&MaterialProps> {
        self.props.get(usize::from(id.0))
    }

    pub fn reactions(&self) -> &[ReactionProps] {
        &self.reactions
    }

    pub fn reaction_input_selector(
        &self,
        reaction: usize,
        input: u8,
    ) -> Option<ReactionInputSelector> {
        let key = match input {
            1 => "input_cell1",
            2 => "input_cell2",
            3 => "input_cell3",
            _ => return None,
        };
        self.reactions
            .get(reaction)?
            .attrs
            .get(key)
            .and_then(|value| ReactionInputSelector::parse(value))
    }

    /// Last definition wins when names repeat (engine behavior for duplicates is unrecovered).
    pub fn find(&self, name: &str) -> Option<MaterialId> {
        self.props
            .iter()
            .rposition(|m| m.name == name)
            .map(|i| MaterialId(i as u16))
    }

    pub fn len(&self) -> usize {
        self.props.len()
    }

    pub fn is_empty(&self) -> bool {
        self.props.is_empty()
    }
}

/// The reference's per-world RNG: MSVC `rand()` (inlined at e.g. 0x70b367).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MsvcRand {
    pub state: u32,
}

impl MsvcRand {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// 0..=32767
    pub fn rand(&mut self) -> i32 {
        self.state = self.state.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
        ((self.state >> 16) & 0x7FFF) as i32
    }

    /// `min + rand() % (max - min + 1)` (0x4abc70).
    pub fn range(&mut self, min: i32, max: i32) -> i32 {
        min + self.rand() % (max - min + 1)
    }
}

/// `0..n` shuffled like MSVC 2013 `std::shuffle` over `rand()` (0x996a20): element `i` swaps with
/// a uniformly drawn index in `0..=i`, drawing 15-bit values and rejecting the biased tail.
fn shuffled(n: usize, rng: &mut MsvcRand) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    for i in 1..n {
        let range = i as u32 + 1;
        let pick = loop {
            let (mut value, mut max) = (0u32, 0u32);
            while max < range - 1 {
                value = (value << 14).wrapping_mul(2) | rng.rand() as u32;
                max = (max << 14).wrapping_mul(2) | 0x7FFF;
            }
            if max / range > value / range || max % range == range - 1 {
                break value % range;
            }
        };
        order.swap(i, pick as usize);
    }
    order
}

/// Cell state byte (`+0x20`).
pub const STATE_FALLING: u8 = 1;
/// Liquid moved sideways only this frame.
pub const STATE_FLOWING: u8 = 2;
/// Initial state (`CLiquidCell` constructor 0x70ae67) and blocked state.
pub const STATE_RESTING: u8 = 3;
/// Liquid moved down with `vel >= 4`.
pub const STATE_FAST: u8 = 8;
/// Resting cells stop updating once `rest` reaches this (0x70c9ae).
pub const SLEEP_REST: u8 = 50;
/// Sand stuck (`liquid_sticks_to_ceiling`) to its right, upper and left neighbor.
pub const STATE_STUCK_RIGHT: u8 = 5;
pub const STATE_STUCK_UP: u8 = 6;
pub const STATE_STUCK_LEFT: u8 = 7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub material: MaterialId,
    /// `+0x20`
    pub state: u8,
    /// `+0x21`: frames spent resting; capped to 45 on wake.
    pub rest: u8,
    /// `+0x22`: static; set from `liquid_static` (0x70aeac) or when sand sticks to a neighbor.
    pub is_static: bool,
    /// `+0x23`: frames spent falling (liquids), 0xFF once landed.
    pub fall_frames: u8,
    /// `+0x24`: accumulated fall speed.
    pub vel: f32,
    /// `+0x28`: horizontal flow (liquids).
    pub flow: f32,
    /// ARGB chosen when the cell was created; travels with the cell.
    pub color: u32,
    /// `+0x34`: the color the cell was created with; stains pull `color` back toward it.
    pub base_color: u32,
    /// Last frame this cell updated (liquid `+0x2C`, gas `+0x1C`).
    frame: u32,
    /// Gas `+0x18`: sideways direction tried first, -1 or 1.
    pub dir: i8,
    /// Gas `+0x34`: starts at a random offset and counts the cell's own moves (vt[15]/vt[16]).
    pub age: i32,
    /// Base-cell `+0x10`: currently burning; `CFireCell` construction forces this on.
    pub burning: bool,
    /// Base-cell `+0x11`: initialized from `temperature_of_fire`.
    pub fire_intensity: u8,
    /// Liquid `+0x38` / fire `+0x20`: world frame when created; reactions' `req_lifetime` age.
    pub created: u32,
}

impl Cell {
    const EMPTY: Cell = Cell::new(AIR, 0, false);

    const fn new(material: MaterialId, color: u32, is_static: bool) -> Self {
        Self {
            material,
            state: STATE_RESTING,
            rest: 0,
            is_static,
            fall_frames: 0,
            vel: 0.0,
            flow: 0.0,
            color,
            base_color: color,
            frame: 0,
            dir: 0,
            age: 0,
            burning: false,
            fire_intensity: 0,
            created: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfBounds {
    pub x: i32,
    pub y: i32,
}

impl fmt::Display for OutOfBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cell ({}, {}) is outside the world", self.x, self.y)
    }
}

impl std::error::Error for OutOfBounds {}

/// 0x70e760: scale each RGB channel of an ARGB color by `1 - factor` (f32, truncated) with a
/// floor of 19; colors whose brightest channel is below 19 are unchanged.
pub fn darkened(argb: u32, factor: f32) -> u32 {
    const FLOOR: u32 = 19;
    let channel = |shift: u32| (argb >> shift) & 0xFF;
    let (r, g, b) = (channel(16), channel(8), channel(0));
    if r.max(g).max(b) < FLOOR {
        return argb;
    }
    let keep = 1.0 - factor;
    let scale = |v: u32| ((v as f32 * keep) as u32).max(FLOOR);
    (argb & 0xFF00_0000) | (scale(r) << 16) | (scale(g) << 8) | scale(b)
}

/// 0xddcd40 with the keep-alpha flag the stain code always sets: each channel of `a` weighted by
/// `t` plus each of `b` weighted by `1 - t` (f32, truncated, wrapping per byte); alpha is `b`'s.
fn blend_keep_alpha(a: u32, b: u32, t: f32) -> u32 {
    if a == b {
        return a;
    }
    let inv = 1.0 - t;
    let channel = |shift: u32| {
        let (ca, cb) = ((a >> shift) & 0xFF, (b >> shift) & 0xFF);
        ((ca as f32 * t) as u32).wrapping_add((cb as f32 * inv) as u32) & 0xFF
    };
    (b & 0xFF00_0000) | (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// 0x44aaa0 / 0x44ab60: unpack to f32 channels (/255), scale the three low channels by 0.7 and
/// repack with truncation. The alpha channel makes the same float round trip.
fn dimmed_to_seven_tenths(color: u32) -> u32 {
    let unpack = |shift: u32| ((color >> shift) & 0xFF) as f32 / 255.0;
    let pack = |v: f32| ((v * 255.0) as i32 as u32) & 0xFF;
    let (c0, c1, c2, a) = (unpack(0), unpack(8), unpack(16), unpack(24));
    let scale = 0.7f32;
    (pack(a) << 24) | (pack(c2 * scale) << 16) | (pack(c1 * scale) << 8) | pack(c0 * scale)
}

/// End of a liquid-flow update (0x70c590 vs 0x70c6a2).
enum FlowEnd {
    Moved,
    Rest,
}

/// A rectangular world stored as a row-major grid of square chunks, simulated by 64x64 worker
/// regions (`GridWorldThreaded`).
#[derive(Clone, Debug)]
pub struct World {
    chunk_size: i32,
    width_chunks: i32,
    height_chunks: i32,
    chunks: Vec<Vec<Cell>>,
    materials: Materials,
    frame: u32,
    regions: Vec<Region>,
    regions_x: i32,
    /// Regions updated this frame, in list order (scheduler `+0x0`/`+0x4`).
    active: Vec<usize>,
    /// Global `rand()` state (`0x120741C`) used for scheduling.
    global_rng: MsvcRand,
    /// Region being updated; its RNG, dirty rect and movable rect serve the cell rules.
    current: Option<usize>,
    /// RNG of the world object passed to cells constructed outside a region update (`+0x4`;
    /// its seed is not recovered, the world seed is used).
    spawn_rng: MsvcRand,
    /// Sweep over every region, one per frame, in shuffled order (`0x726340`, tile list
    /// `0x1225d4c`); `sweep_pos` is the next entry.
    sweep_order: Vec<usize>,
    sweep_pos: usize,
    /// CRT `rand()` used by the shuffle (separate from the game's `0x120741C`; seed unrecovered).
    sweep_rng: MsvcRand,
}

/// `GridWorldThreaded` splash aggregate (`+0xE0..+0xEC`), drained by the caller when consumed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SplashStats {
    pub sum_x: f32,
    pub sum_y: f32,
    /// Floating sample total at worker offset `+0xE8`.
    pub sample_total: f32,
    /// Integer event count at worker offset `+0xEC`.
    pub event_count: u32,
}

/// Region side (`0x7259e0`: shift 6).
pub const REGION_SIZE: i32 = 64;
/// How far outside its region a worker may move cells (`0x725f40`).
const REGION_MARGIN: i32 = 32;

/// One `GridWorldThreaded` worker.
#[derive(Clone, Debug)]
struct Region {
    /// `+0x470/+0x474`; size is `REGION_SIZE`.
    origin: (i32, i32),
    /// `+0x480..+0x48C`: cells marked by this worker's updates (or shared to it). Inclusive.
    dirty: Rect,
    /// `+0x4`: seeded with the world seed ^ 0xC (0x729948).
    rng: MsvcRand,
    /// `+0x460`: every 11th update is "full" (skips the sand sleep check); random start phase.
    full_counter: u32,
    /// `+0x46C`: scan the whole region on the next update.
    full_scan: bool,
    /// `+0x560..+0x56C`: cells may move within `[min, max)`.
    movable: Rect,
    /// `+0x49C`: deferred SetCell requests (world vfn 0x20) with their replace flag, applied
    /// after every region updates.
    cell_changes: Vec<(i32, i32, MaterialId, bool)>,
    /// `+0x4B4`: reaction blobs (world vfn 0x30, `World_QueueBlob` 0x72b250), applied after
    /// this region's cell changes.
    blobs: Vec<Blob>,
    /// `+0xE0..+0xEC`: splash position and sample aggregates.
    splash: SplashStats,
}

/// One 0x1C-byte blob record (`World_QueueBlob` 0x72b250).
#[derive(Clone, Copy, Debug)]
struct Blob {
    x: i32,
    y: i32,
    material: MaterialId,
    radius: i32,
    /// `+0x18`: only convert cells of `restrict_material`, along rays from the center.
    restrict: bool,
    restrict_material: MaterialId,
    particle: Option<MaterialId>,
}

/// Inclusive cell rectangle; empty when `min > max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    min: (i32, i32),
    max: (i32, i32),
}

impl Rect {
    const EMPTY: Rect = Rect {
        min: (i32::MAX, i32::MAX),
        max: (i32::MIN, i32::MIN),
    };

    fn is_empty(&self) -> bool {
        self.min.0 > self.max.0 || self.min.1 > self.max.1
    }

    fn grow(&mut self, x: i32, y: i32) {
        self.min = (self.min.0.min(x), self.min.1.min(y));
        self.max = (self.max.0.max(x), self.max.1.max(y));
    }

    fn union(self, o: Rect) -> Rect {
        if o.is_empty() {
            return self;
        }
        let mut r = self;
        r.grow(o.min.0, o.min.1);
        r.grow(o.max.0, o.max.1);
        r
    }
}

impl World {
    /// Panics if any size is zero or the world is too large to address with `i32`.
    pub fn new(
        width_chunks: u32,
        height_chunks: u32,
        chunk_size: u32,
        materials: Materials,
        seed: u32,
    ) -> Self {
        assert!(
            width_chunks > 0 && height_chunks > 0 && chunk_size > 0,
            "world sizes must be non-zero"
        );
        let to_i32 = |v: u32| i32::try_from(v).expect("world size exceeds i32");
        let (wc, hc, cs) = (
            to_i32(width_chunks),
            to_i32(height_chunks),
            to_i32(chunk_size),
        );
        wc.checked_mul(cs).expect("world width exceeds i32");
        hc.checked_mul(cs).expect("world height exceeds i32");

        let cells_per_chunk = (cs as usize) * (cs as usize);
        let (w, h) = (wc * cs, hc * cs);
        let regions_x = (w + REGION_SIZE - 1) / REGION_SIZE;
        let regions_y = (h + REGION_SIZE - 1) / REGION_SIZE;
        let mut global_rng = MsvcRand::new(seed);
        let mut sweep_rng = MsvcRand::new(seed);
        let regions: Vec<Region> = (0..regions_x * regions_y)
            .map(|i| Region {
                origin: ((i % regions_x) * REGION_SIZE, (i / regions_x) * REGION_SIZE),
                dirty: Rect::EMPTY,
                rng: MsvcRand::new(seed ^ 0xC),
                full_counter: (global_rng.rand() % 11) as u32,
                // A freshly loaded region is scanned in full once (`+0x46C`, 0x7269c3).
                full_scan: true,
                movable: Rect::EMPTY,
                cell_changes: Vec::new(),
                blobs: Vec::new(),
                splash: SplashStats::default(),
            })
            .collect();
        Self {
            chunk_size: cs,
            width_chunks: wc,
            height_chunks: hc,
            chunks: vec![vec![Cell::EMPTY; cells_per_chunk]; (wc as usize) * (hc as usize)],
            materials,
            frame: 0,
            active: (0..regions.len()).collect(),
            sweep_order: shuffled(regions.len(), &mut sweep_rng),
            sweep_pos: 0,
            sweep_rng,
            regions,
            regions_x,
            global_rng,
            current: None,
            spawn_rng: MsvcRand::new(seed),
        }
    }

    pub fn width(&self) -> i32 {
        self.width_chunks * self.chunk_size
    }

    pub fn height(&self) -> i32 {
        self.height_chunks * self.chunk_size
    }

    pub fn chunk_size(&self) -> i32 {
        self.chunk_size
    }

    pub fn materials(&self) -> &Materials {
        &self.materials
    }

    /// Takes and clears the splash aggregate for each region, in row-major region order.
    pub fn take_splash_stats(&mut self) -> Vec<SplashStats> {
        self.regions
            .iter_mut()
            .map(|region| std::mem::take(&mut region.splash))
            .collect()
    }

    /// Number of completed `step` calls.
    pub fn tick(&self) -> u64 {
        u64::from(self.frame)
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        (0..self.width()).contains(&x) && (0..self.height()).contains(&y)
    }

    /// Chunk coordinates containing cell `(x, y)`, or `None` if out of bounds.
    pub fn chunk_of(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        self.in_bounds(x, y)
            .then(|| (x / self.chunk_size, y / self.chunk_size))
    }

    /// Row-major cells of chunk `(cx, cy)`, or `None` if no such chunk exists.
    pub fn chunk_cells(&self, cx: i32, cy: i32) -> Option<&[Cell]> {
        let in_range =
            (0..self.width_chunks).contains(&cx) && (0..self.height_chunks).contains(&cy);
        in_range.then(|| self.chunks[(cy * self.width_chunks + cx) as usize].as_slice())
    }

    pub fn cell(&self, x: i32, y: i32) -> Option<&Cell> {
        self.locate(x, y).map(|(c, i)| &self.chunks[c][i])
    }

    pub fn get(&self, x: i32, y: i32) -> Option<MaterialId> {
        self.cell(x, y).map(|c| c.material)
    }

    /// Places a freshly constructed cell, colored for its position (`SetMaterial` vt[13] 0x70d0b0),
    /// and marks it for update.
    pub fn set(&mut self, x: i32, y: i32, material: MaterialId) -> Result<(), OutOfBounds> {
        let props = self
            .materials
            .get(material)
            .unwrap_or_else(|| panic!("unknown material {material:?}"));
        let color = props.color_at(x, y);
        let is_liquid = props.cell_type == CellType::Liquid;
        let mut cell = Cell::new(material, color, is_liquid && props.liquid_static);
        cell.burning = props.on_fire || props.cell_type == CellType::Fire;
        cell.fire_intensity = props.temperature_of_fire as u8;
        cell.created = self.frame;
        if !self.in_bounds(x, y) {
            return Err(OutOfBounds { x, y });
        }
        if props.on_fire {
            // Base constructor 0x70f450 consumes global rand() for on_fire materials.
            self.global_rng.rand();
        }
        if props.cell_type == CellType::Gas {
            // `CGasCell` constructor (0x70a4a0).
            let lifetime = props.lifetime;
            let rng = &mut self.spawn_rng;
            cell.dir = if rng.rand() % 101 < 50 { 1 } else { -1 };
            if lifetime > 0 {
                let v = ((lifetime as f32 * 0.3) as i32).max(1);
                cell.age = rng.rand() % (2 * v + 1) - v;
            }
        }
        *self.cell_mut(x, y).expect("in bounds") = cell;
        self.mark(x, y);
        Ok(())
    }

    /// Union of every region's pending scan area (min and max corners, inclusive), if any. A
    /// region scans only its own 64x64 part of it; see `step`.
    pub fn dirty_rect(&self) -> Option<((i32, i32), (i32, i32))> {
        let all = self
            .regions
            .iter()
            .fold(Rect::EMPTY, |acc, r| acc.union(r.dirty));
        (!all.is_empty()).then_some((all.min, all.max))
    }

    /// Includes the inclusive rectangle in the next scan of every region it touches
    /// (`IMPL_AddUpdateAABB` 0x728a90), e.g. for explosions.
    pub fn mark_area(&mut self, min: (i32, i32), max: (i32, i32)) {
        self.add_update_aabb(min, max, None);
    }

    pub fn count(&self, material: MaterialId) -> usize {
        self.chunks
            .iter()
            .flatten()
            .filter(|c| c.material == material)
            .count()
    }

    /// Stable FNV-1a hash of dimensions, RNG state and every cell's simulation state in world
    /// row-major order. Independent of chunk size and Rust version; excludes the frame counter.
    pub fn state_hash(&self) -> u64 {
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |bytes: &[u8]| {
            for &b in bytes {
                hash = (hash ^ u64::from(b)).wrapping_mul(PRIME);
            }
        };
        feed(&self.width().to_le_bytes());
        feed(&self.height().to_le_bytes());
        feed(&self.global_rng.state.to_le_bytes());
        for r in &self.regions {
            feed(&r.rng.state.to_le_bytes());
        }
        for y in 0..self.height() {
            for x in 0..self.width() {
                let c = self.cell(x, y).expect("in bounds");
                feed(&c.material.0.to_le_bytes());
                feed(&[c.state, c.rest, c.fall_frames, u8::from(c.is_static)]);
                feed(&c.vel.to_bits().to_le_bytes());
                feed(&c.flow.to_bits().to_le_bytes());
                feed(&c.color.to_le_bytes());
                feed(&[c.dir as u8]);
                feed(&c.age.to_le_bytes());
                feed(&[u8::from(c.burning), c.fire_intensity]);
            }
        }
        hash
    }

    /// Advances the simulation by one frame (scheduler 0x726340, worker update 0x72a150).
    ///
    /// 1. Regions whose dirty rect is empty drop out of the active list; a region whose rect,
    ///    grown by 1, reaches outside it shares that rect with the regions it overlaps
    ///    (0x7263a8, `IMPL_AddUpdateAABB`), which become active.
    /// 2. One region of the update window (here the whole world), taken in shuffled order, is
    ///    scanned in full and activated, so every region is revisited every `regions` frames
    ///    even when nothing marked it (0x726340, sweep at 0x726b00).
    /// 3. One active region, picked with the global RNG, is scanned in full (0x7271cb).
    /// 4. Active regions update. The reference runs them on threads ordered by a dependency
    ///    graph that is not recovered; here they run in four checkerboard passes so that regions
    ///    updated back to back never share cells, each pass in list order.
    pub fn step(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        let previous = std::mem::take(&mut self.active);
        let mut shared = Vec::new();
        for &r in &previous {
            let (d, full_scan, (ox, oy)) = {
                let region = &self.regions[r];
                (region.dirty, region.full_scan, region.origin)
            };
            if d.is_empty() && !full_scan {
                continue;
            }
            self.activate(r);
            if d.is_empty() {
                continue;
            }
            let grown = Rect {
                min: (d.min.0 - 1, d.min.1 - 1),
                max: (d.max.0 + 1, d.max.1 + 1),
            };
            let inside = |(x, y): (i32, i32)| {
                (ox..ox + REGION_SIZE).contains(&x) && (oy..oy + REGION_SIZE).contains(&y)
            };
            if !inside(grown.min) || !inside(grown.max) {
                shared.push((grown, r));
            }
        }
        for (rect, from) in shared {
            self.add_update_aabb(rect.min, rect.max, Some(from));
        }
        if let Some(&r) = self.sweep_order.get(self.sweep_pos) {
            self.regions[r].full_scan = true;
            self.activate(r);
            self.sweep_pos += 1;
            if self.sweep_pos >= self.sweep_order.len() {
                self.sweep_pos = 0;
                self.sweep_order = shuffled(self.regions.len(), &mut self.sweep_rng);
            }
        }
        if !self.active.is_empty() {
            let pick = self.global_rng.rand() as usize % self.active.len();
            let r = self.active[pick];
            self.regions[r].full_scan = true;
        }
        let order = self.active.clone();
        for pass in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            for &r in &order {
                let (gx, gy) = (r as i32 % self.regions_x, r as i32 / self.regions_x);
                if (gx % 2, gy % 2) == pass {
                    self.update_region(r);
                }
            }
        }
        self.current = None;
        // Per-worker flush 0x72abf0: cell changes, then blobs.
        for &r in &order {
            for (x, y, material, replace) in std::mem::take(&mut self.regions[r].cell_changes) {
                self.set_cell(x, y, material, replace);
            }
            for blob in std::mem::take(&mut self.regions[r].blobs) {
                self.apply_blob(blob);
            }
        }
    }

    /// `GridWorld::SetCell` (0x71b140): an occupied cell is kept unless `replace`, else removed;
    /// then a non-air material is placed.
    fn set_cell(&mut self, x: i32, y: i32, material: MaterialId, replace: bool) {
        if !self.in_bounds(x, y) {
            return;
        }
        if self.occupied(x, y) {
            if !replace {
                return;
            }
            self.remove_cell(x, y);
        }
        if material != AIR {
            self.set(x, y, material).expect("in bounds");
        }
    }

    /// `GridWorld` vt[12] (0x71ab50). Unrestricted: every cell with `dx² + dy² < r²` is set,
    /// row by row. Restricted: the center must hold `restrict_material` and changes; then
    /// `round(2πr)` rays of `r` unit steps convert `restrict_material` cells, pass through cells
    /// already of the output and stop at anything else. Cosmetic particles (two global `rand()`
    /// per converted cell) are not spawned.
    fn apply_blob(&mut self, blob: Blob) {
        let Blob {
            x,
            y,
            material,
            radius: r,
            restrict,
            restrict_material,
            particle,
        } = blob;
        let particle_rolls = |world: &mut Self| {
            if particle.is_some_and(|p| p.0 > 0) {
                world.global_rng.rand();
                world.global_rng.rand();
            }
        };
        if !restrict {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy < r * r {
                        self.set_cell(x + dx, y + dy, material, true);
                        particle_rolls(self);
                    }
                }
            }
            return;
        }
        if material == restrict_material || r < 0 {
            return;
        }
        if !self.occupied(x, y) || self.get(x, y) != Some(restrict_material) {
            return;
        }
        self.change_material(x, y, material);
        if r == 0 {
            return;
        }
        let rays = (r as f32 * std::f32::consts::TAU + 0.5) as i32;
        let step = std::f32::consts::TAU / rays as f32;
        let mut angle = 0.0f32;
        for _ in 0..rays {
            let (sin, cos) = f64::from(angle).sin_cos();
            let (cos, sin) = (cos as f32, sin as f32);
            angle += step;
            let (mut fx, mut fy) = (x as f32, y as f32);
            for _ in 0..r {
                fx += cos;
                fy += sin;
                let (cx, cy) = (fx as i32, fy as i32);
                let here = self.get(cx, cy).unwrap_or(AIR);
                // The center check above rules out an air `restrict_material`, so the empty-cell
                // SetCell branch of the reference never runs.
                if here == restrict_material {
                    self.change_material(cx, cy, material);
                    particle_rolls(self);
                } else if here != material {
                    break;
                }
            }
        }
    }

    /// World vfn 0x30 (`World_QueueBlob` 0x72b250): skipped when restricted to its own output.
    fn queue_blob(
        &mut self,
        (x, y): (i32, i32),
        material: MaterialId,
        radius: u8,
        restrict: bool,
        restrict_material: MaterialId,
        particle: Option<MaterialId>,
    ) {
        if restrict && material == restrict_material {
            return;
        }
        let region = self.current.expect("inside a region update");
        self.regions[region].blobs.push(Blob {
            x,
            y,
            material,
            radius: i32::from(radius as i8),
            restrict,
            restrict_material,
            particle,
        });
    }

    /// `GridWorld::SetCell` with no material and replace set (0x71b140 -> vt[17] 0x71b400 ->
    /// 0x71b480): whatever occupies `(x, y)` now is removed; `(x, y-1)` and `(x, y)` are marked.
    fn remove_cell(&mut self, x: i32, y: i32) {
        if !self.occupied(x, y) {
            return;
        }
        *self.cell_mut(x, y).expect("in bounds") = Cell::EMPTY;
        self.mark(x, y - 1);
        self.mark(x, y);
    }

    fn activate(&mut self, r: usize) {
        if !self.active.contains(&r) {
            self.active.push(r);
        }
    }

    /// `IMPL_AddUpdateAABB` (0x728a90): mark the rect, clamped to each overlapped region, in that
    /// region (skipping `exclude`), and activate it. Areas spanning over 100 regions are refused.
    fn add_update_aabb(&mut self, min: (i32, i32), max: (i32, i32), exclude: Option<usize>) {
        let g = |v: i32| v.div_euclid(REGION_SIZE);
        let (gx0, gy0, gx1, gy1) = (g(min.0), g(min.1), g(max.0), g(max.1));
        if (gx1 - gx0) * (gy1 - gy0) > 100 {
            return;
        }
        let regions_y = self.regions.len() as i32 / self.regions_x;
        for gy in gy0..=gy1 {
            for gx in gx0..=gx1 {
                if !(0..self.regions_x).contains(&gx) || !(0..regions_y).contains(&gy) {
                    continue;
                }
                let r = (gy * self.regions_x + gx) as usize;
                if Some(r) == exclude {
                    continue;
                }
                self.activate(r);
                let (ox, oy) = self.regions[r].origin;
                let clamp = |(x, y): (i32, i32)| {
                    (
                        x.min(ox + REGION_SIZE).max(ox),
                        y.min(oy + REGION_SIZE).max(oy),
                    )
                };
                let (a, b) = (clamp(min), clamp(max));
                let d = &mut self.regions[r].dirty;
                d.grow(a.0, a.1);
                d.grow(b.0, b.1);
            }
        }
    }

    /// Worker update (0x72a150): scan the region's part of its dirty rect (or all of it when
    /// `full_scan`), rows bottom-to-top, each row in the direction `rand() & 1` (1 = left to
    /// right) from the region's own RNG.
    fn update_region(&mut self, r: usize) {
        self.current = Some(r);
        let (w, h) = (self.width(), self.height());
        let region = &mut self.regions[r];
        let (ox, oy) = region.origin;
        // 0x725f40: the margin applies where the neighboring area is loaded (inside the world).
        let lo = |o: i32| {
            if o - REGION_MARGIN >= 0 {
                o - REGION_MARGIN
            } else {
                o
            }
        };
        let hi = |o: i32, size: i32| {
            let end = o + REGION_SIZE;
            (if end + REGION_MARGIN < size {
                end + REGION_MARGIN
            } else {
                end
            })
            .min(size)
        };
        region.movable = Rect {
            min: (lo(ox), lo(oy)),
            max: (hi(ox, w), hi(oy, h)),
        };
        region.full_counter += 1;
        let full = region.full_counter > 10;
        if full {
            region.full_counter = 0;
        }
        let area = if std::mem::take(&mut region.full_scan) {
            Rect {
                min: (ox, oy),
                max: (ox + REGION_SIZE, oy + REGION_SIZE),
            }
        } else {
            region.dirty
        };
        region.dirty = Rect::EMPTY;
        let m = region.movable;
        let (ex, ey) = (ox + REGION_SIZE - 1, oy + REGION_SIZE - 1);
        let x0 = area.min.0.min(ex).max(ox).max(m.min.0);
        let y0 = area.min.1.min(ey).max(oy).max(m.min.1);
        let x1 = area.max.0.min(ex).max(ox).min(m.max.0 - 1);
        let y1 = area.max.1.min(ey).max(oy).min(m.max.1 - 1);
        for y in (y0..=y1).rev() {
            if self.rng().rand() & 1 == 1 {
                for x in x0..=x1 {
                    self.update_cell(x, y, full);
                }
            } else {
                for x in (x0..=x1).rev() {
                    self.update_cell(x, y, full);
                }
            }
        }
    }

    /// The updating region's RNG (`+0x4`).
    fn rng(&mut self) -> &mut MsvcRand {
        let r = self.current.expect("cell rules run inside a region update");
        &mut self.regions[r].rng
    }

    /// World vfn 0x74 (0x72b390): whether cells may move to `(x, y)` in this region's update.
    fn movable(&self, x: i32, y: i32) -> bool {
        self.current.is_some_and(|r| {
            let m = self.regions[r].movable;
            (m.min.0..m.max.0).contains(&x) && (m.min.1..m.max.1).contains(&y)
        })
    }

    /// World vfn 0x14 (0x72b080): during an update, grow the updating region's dirty rect
    /// (even outside the region); otherwise mark the cell in the region that contains it.
    fn mark(&mut self, x: i32, y: i32) {
        match self.current {
            Some(r) => self.regions[r].dirty.grow(x, y),
            None => self.add_update_aabb((x, y), (x, y), None),
        }
    }

    /// `CLiquidCell` vt[21] (0x70c6e0): static-cell checks, movement dispatch, burning, fire-HP
    /// conversion and reactions. The `+0x21C` queue remains unported.
    fn update_cell(&mut self, x: i32, y: i32, full: bool) {
        let frame = self.frame;
        let Some(cell) = self.cell(x, y).copied() else {
            return;
        };
        let Some(mat) = self.materials.get(cell.material) else {
            return;
        };
        if cell.material != AIR && mat.cell_type == CellType::Gas {
            self.update_gas(x, y, full);
            return;
        }
        if cell.material != AIR && mat.cell_type == CellType::Fire {
            self.update_fire(x, y);
            return;
        }
        if cell.material == AIR || mat.cell_type != CellType::Liquid {
            return;
        }
        let (is_sand, is_slime, sticks, lifetime, stains) = (
            mat.liquid_sand,
            mat.liquid_slime,
            mat.liquid_sticks_to_ceiling,
            mat.lifetime,
            mat.liquid_stains != 0,
        );
        if cell.is_static {
            if full && self.rng().rand() % 101 < 20 {
                if sticks > 0 && self.update_stuck(x, y, cell.state, sticks) {
                    return;
                }
                // 0x70c835: the reaction check on half of these full-update rolls.
                if self
                    .materials
                    .reaction_max_probability(false, cell.material)
                    > 0
                    && self.rng().rand() % 101 < 50
                {
                    self.check_reactions(x, y);
                    return;
                }
            } else if self.materials.reaction_max_probability(true, cell.material) > 0 {
                self.fast_reactions(x, y);
                return;
            }
            if cell.burning {
                self.burn_hook(x, y);
                self.update_fire_hp_conversion(x, y, true);
            } else if full && self.isolated(x, y) {
                self.change_material(x, y, AIR);
            }
            return;
        }
        if !full && cell.state == STATE_RESTING && cell.rest >= SLEEP_REST {
            return;
        }
        if cell.frame == frame {
            return;
        }
        let c = self.cell_mut(x, y).expect("in bounds");
        c.frame = frame;
        if stains {
            self.stain_neighbor(x, y);
        }
        // 0x70c6e0: a liquid older than its `lifetime` (+0x154) becomes air.
        if lifetime > 0 && self.cell_age(x, y) >= lifetime {
            self.change_material(x, y, AIR);
            return;
        }
        let c = self.cell_mut(x, y).expect("in bounds");
        if is_sand || is_slime {
            if c.state == STATE_RESTING {
                if c.rest <= SLEEP_REST {
                    c.rest += 1;
                }
            } else {
                c.rest = 0;
            }
        }
        let final_position = if is_sand {
            self.move_sand(x, y)
        } else if is_slime {
            self.mark(x, y);
            let (nx, ny) = self.move_slime(x, y);
            self.mark(nx, ny);
            Some((nx, ny))
        } else {
            self.move_liquid(x, y)
        };
        let Some((final_x, final_y)) = final_position else {
            return;
        };
        if self.cell(final_x, final_y).is_some_and(|c| c.burning) {
            self.burn_hook(final_x, final_y);
            if self.update_fire_hp_conversion(final_x, final_y, false) {
                return;
            }
        }
        // 0x70cae5: a `CellData+0x21C` gate (only seen as 0) would queue the cell instead.
        if full {
            self.check_reactions(final_x, final_y);
        } else if self.materials.reaction_max_probability(true, cell.material) > 0 {
            self.fast_reactions(final_x, final_y);
        }
    }

    /// `CFireCell` vt[21] (0x709fc0). Unlike liquids and gases there is no per-frame guard, so a
    /// cell that rises is updated again when its new row is scanned.
    fn update_fire(&mut self, x: i32, y: i32) {
        self.spread_fire(x, y);
        self.spread_fire(x, y);
        if self.rng().rand() % 101 >= 35 {
            self.finish_fire(x, y);
            return;
        }
        let open = |w: &Self, tx: i32, ty: i32| w.movable(tx, ty) && w.in_bounds(tx, ty);
        let mut above = None;
        if open(self, x, y - 1) {
            if !self.occupied(x, y - 1) {
                self.fire_move_to((x, y), (x, y - 1));
                return;
            }
            above = Some((x, y - 1));
        }
        if self.rng().rand() % 101 < 35 {
            return;
        }
        let dir = if self.rng().rand() % 101 < 50 { -1 } else { 1 };
        for tx in [x + dir, x - dir] {
            if open(self, tx, y) && !self.occupied(tx, y) {
                self.fire_move_to((x, y), (tx, y));
                return;
            }
        }
        // Darken a short, wandering column of sand-like cells above (0x70a1a6..0x70a33c).
        if let Some((ax, ay)) = above
            && self.darkenable(ax, ay)
            && self.rng().rand() % 101 < 20
        {
            let mut factor = self.rng().rand() as f32 / 32767.0 * 0.149_999_99 + 0.2;
            let rows = self.rng().range(1, 6) + 1;
            self.darken(ax, ay, factor);
            for i in 2..rows {
                let scale = self.rng().rand() as f32 / 32767.0 * 0.199_999_99 + 0.6;
                let dx = self.rng().rand() % 3 - 1;
                factor *= scale;
                let (tx, ty) = (x + dx, y - i);
                if !(open(self, tx, ty) && self.darkenable(tx, ty)) {
                    break;
                }
                self.darken(tx, ty, factor);
            }
        }
        self.finish_fire(x, y);
    }

    /// End of 0x709fc0: mark the cell, then remove it with chance 1/16; otherwise run the
    /// reaction check (vt[35] 0x70f160).
    fn finish_fire(&mut self, x: i32, y: i32) {
        self.mark(x, y);
        if self.rng().rand() & 15 == 0 {
            self.change_material(x, y, AIR);
        } else {
            self.check_reactions(x, y);
        }
    }

    /// `CFireCell` vt[18] (0x70a3b0) into an empty cell: move and mark the new position.
    fn fire_move_to(&mut self, from: (i32, i32), to: (i32, i32)) {
        let cell = *self.cell(from.0, from.1).expect("in bounds");
        *self.cell_mut(from.0, from.1).expect("in bounds") = Cell::EMPTY;
        *self.cell_mut(to.0, to.1).expect("in bounds") = cell;
        self.mark(to.0, to.1);
    }

    /// Fire spread (0x70f9e0): with chance `(intensity + 1) / 101` pick a random 4-neighbor
    /// (`rand() & 3`: left, right, up, down); darken it if sand-like, and try to ignite it if it
    /// is a burnable liquid or gas that is not already burning.
    fn spread_fire(&mut self, x: i32, y: i32) {
        let intensity = self.cell(x, y).expect("in bounds").fire_intensity;
        if self.rng().rand() % 101 > i32::from(intensity as i8) {
            return;
        }
        let (nx, ny) = match self.rng().rand() & 3 {
            0 => (x - 1, y),
            1 => (x + 1, y),
            2 => (x, y - 1),
            _ => (x, y + 1),
        };
        if !self.movable(nx, ny) || !self.occupied(nx, ny) {
            return;
        }
        if self.darkenable(nx, ny) {
            let factor = self.rng().rand() as f32 / 32767.0 * 0.001_000_000_2 + 0.0045;
            self.darken(nx, ny, factor);
        }
        let target = *self.cell(nx, ny).expect("in bounds");
        let props = self.materials.get(target.material).expect("known material");
        // vt[28]/vt[29]: fire cells are never burnable and always count as burning.
        let burnable =
            matches!(props.cell_type, CellType::Liquid | CellType::Gas) && props.burnable;
        if burnable && !target.burning {
            self.ignite(nx, ny, intensity);
            self.mark(nx, ny);
        }
    }

    /// vt[30] with the ignition roll 0x70f5e0 (`rand() % 101 < 100 - autoignition_temperature`,
    /// the load-time `CellData+0x98`). Liquids (0x70cdf0) that caught fire with `fire_hp == 0`
    /// and every touched gas (0x70aa80) become their flame material.
    fn ignite(&mut self, x: i32, y: i32, intensity: u8) {
        let props = self
            .materials
            .get(self.get(x, y).expect("in bounds"))
            .expect("known material");
        let threshold = 100 - props.autoignition_temperature;
        let (kind, fire_hp, flame) = (
            props.cell_type,
            props.fire_hp,
            props.on_fire_flame_material.clone(),
        );
        if self.rng().rand() % 101 < threshold {
            let cell = self.cell_mut(x, y).expect("in bounds");
            cell.burning = true;
            cell.fire_intensity = intensity;
        }
        let burning = self.cell(x, y).expect("in bounds").burning;
        let to_flame = match kind {
            CellType::Liquid => burning && fire_hp == 0,
            _ => true,
        };
        if to_flame && let Some(target) = self.materials.find(&flame) {
            self.change_material(x, y, target);
        }
    }

    /// vt[25] (`liquid_sand` for liquids, false for gas and fire) or vt[23] (false for all).
    fn darkenable(&self, x: i32, y: i32) -> bool {
        self.liquid_at(x, y).is_some_and(|(_, m)| m.liquid_sand)
    }

    /// 0x70e860: the cell's own color becomes `darkened(color, factor)`.
    fn darken(&mut self, x: i32, y: i32, factor: f32) {
        let cell = self.cell_mut(x, y).expect("in bounds");
        cell.color = darkened(cell.color, factor);
    }

    /// Burning liquid fire-HP rolls (moved cells 0x70cab3, static cells 0x70c8ce, whose extra
    /// exposed-cell roll follows an unported `worker+0x451` roll); the cell becomes its flame
    /// material (`+0xE0`). Returns whether the cell was converted.
    fn update_fire_hp_conversion(&mut self, x: i32, y: i32, is_static: bool) -> bool {
        let Some(cell) = self.cell(x, y).copied() else {
            return false;
        };
        let Some(props) = self.materials.get(cell.material) else {
            return false;
        };
        let (fire_hp, target_name) = (props.fire_hp, props.on_fire_flame_material.clone());
        if fire_hp < 0 {
            return false;
        }
        if self.rng().range(0, fire_hp) != 0
            && (!is_static
                || self.rng().range(0, fire_hp) >= 30
                || self.random_neighbor_occupied(x, y))
        {
            return false;
        }
        // How the engine resolves a missing target name is unrecovered.
        let Some(target) = self.materials.find(&target_name) else {
            return false;
        };
        self.change_material(x, y, target);
        true
    }

    /// `liquid_stains` roll of a moving liquid (0x70ce40): about a quarter of updates pick the cell
    /// to the left, right or below and recolor it; the RNG draws happen even when nothing is stained.
    fn stain_neighbor(&mut self, x: i32, y: i32) {
        if self.rng().rand() & 0x7F < 0x5F {
            return;
        }
        let (nx, ny) = match self.rng().rand() % 3 {
            0 => (x - 1, y),
            1 => (x + 1, y),
            _ => (x, y + 1),
        };
        if !self.movable(nx, ny) || !self.occupied(nx, ny) {
            return;
        }
        let (this, other) = (
            *self.cell(x, y).expect("in bounds"),
            *self.cell(nx, ny).expect("in bounds"),
        );
        let (Some(me), Some(target)) = (
            self.materials.get(this.material),
            self.materials.get(other.material),
        ) else {
            return;
        };
        if matches!(target.cell_type, CellType::Gas | CellType::Fire) || !target.stainable {
            return;
        }
        let same_material = me.liquid_stains_self && other.material == this.material;
        if !same_material && target.cell_type == CellType::Liquid && !target.liquid_sand {
            return;
        }
        let stain = if me.liquid_stains_custom_color != 0 {
            me.liquid_stains_custom_color
        } else {
            this.color
        };
        let flags = me.liquid_stains;
        let mut color = other.color;
        if flags & 1 != 0 {
            let dimmed = dimmed_to_seven_tenths(other.base_color);
            if color != dimmed {
                color = blend_keep_alpha(dimmed, color, 0.1);
            }
        }
        if flags & 2 != 0 {
            color = blend_keep_alpha(stain, color, 0.2);
        }
        if flags & 4 != 0 {
            let mixed = blend_keep_alpha(stain, other.base_color, 0.5);
            if color != mixed {
                color = blend_keep_alpha(mixed, color, 0.2);
            }
        }
        self.cell_mut(nx, ny).expect("in bounds").color = color;
    }

    /// 0x70ad30: all four orthogonal neighbors are movable and empty.
    fn isolated(&self, x: i32, y: i32) -> bool {
        [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
            .into_iter()
            .all(|(nx, ny)| self.movable(nx, ny) && !self.occupied(nx, ny))
    }

    /// Worker vfn +4 (0x743e80): `rand() & 3` picks left/right/up/down; if that cell is not
    /// movable the opposite one is used; `None` if neither is movable.
    fn random_neighbor(&mut self, x: i32, y: i32) -> Option<(i32, i32)> {
        let (dx, dy) = match self.rng().rand() & 3 {
            0 => (-1, 0),
            1 => (1, 0),
            2 => (0, -1),
            _ => (0, 1),
        };
        [(x + dx, y + dy), (x - dx, y - dy)]
            .into_iter()
            .find(|&(nx, ny)| self.movable(nx, ny))
    }

    /// [`Self::random_neighbor`] holds a cell.
    fn random_neighbor_occupied(&mut self, x: i32, y: i32) -> bool {
        self.random_neighbor(x, y)
            .is_some_and(|(nx, ny)| self.occupied(nx, ny))
    }

    /// 0x4abca0: one step left/right/up/down chosen by `rand() & 3`.
    fn step_direction(&mut self, x: i32, y: i32) -> (i32, i32) {
        match self.rng().rand() & 3 {
            0 => (x - 1, y),
            1 => (x + 1, y),
            2 => (x, y - 1),
            _ => (x, y + 1),
        }
    }

    /// Cell vt[35] (0x70f160). Cells that are not burning, and burning `on_fire` materials half
    /// the time, run the reaction dispatcher; other burning cells react as the `fire` material
    /// (`CellFactory+0xA4`) with a random neighbor.
    fn check_reactions(&mut self, x: i32, y: i32) {
        let cell = *self.cell(x, y).expect("in bounds");
        let props = self.materials.get(cell.material).expect("known material");
        // vt[29]: fire cells always count as burning.
        if !(cell.burning || props.cell_type == CellType::Fire) {
            self.dispatch_reactions(x, y);
            return;
        }
        if props.on_fire && self.rng().rand() % 101 < 50 {
            self.dispatch_reactions(x, y);
            return;
        }
        let Some(fire) = self.materials.find("fire") else {
            return;
        };
        let max = self.materials.reaction_max_probability(false, fire);
        if self.rng().rand() % 10001 > max {
            return;
        }
        let Some((nx, ny)) = self.random_neighbor(x, y) else {
            return;
        };
        let Some(neighbor) = self.get(nx, ny).filter(|&m| m != AIR) else {
            return;
        };
        let roll = self.rng().rand() % 1025;
        let Some(reaction) = self
            .materials
            .tables
            .lookup(false, fire, neighbor, roll)
            .cloned()
        else {
            return;
        };
        if self.rng().rand() % (max + 1) >= reaction.probability {
            return;
        }
        if reaction.blob_radius2 == 0 {
            self.change_material(nx, ny, reaction.output2);
        } else {
            self.queue_blob(
                (nx, ny),
                reaction.output2,
                reaction.blob_radius2,
                reaction.blob_restrict2,
                reaction.input2,
                reaction.cosmetic_particle,
            );
        }
        if reaction.blob_radius1 != 0 {
            self.queue_blob(
                (x, y),
                reaction.output1,
                reaction.blob_radius1,
                reaction.blob_restrict1,
                reaction.input1,
                reaction.cosmetic_particle,
            );
            return;
        }
        if reaction.output1 != fire {
            self.change_material(x, y, reaction.output1);
        }
    }

    /// Reaction dispatcher (0x70e8c0). `roll = rand() % 10001`; a required reaction (`+0x230`)
    /// takes precedence while `roll` is below its maximum; otherwise `roll` must not exceed the
    /// material's largest probability, a random neighbor's record is picked by `roll` and fires
    /// when `roll` does not exceed its probability. The spontaneous freezing/melting rolls read
    /// worker bytes `+0x452`/`+0x453`, which the exe never writes, so they stay off.
    fn dispatch_reactions(&mut self, x: i32, y: i32) {
        self.react_with_neighbor(x, y, false);
    }

    /// Cell vt[36] (0x70eaf0): the same as the dispatcher, using the fast-reaction table.
    fn fast_reactions(&mut self, x: i32, y: i32) {
        self.react_with_neighbor(x, y, true);
    }

    /// `Cell_RequiredReactions` (vt[37] `0x70ef90`): each of the four neighbors (up, down, left,
    /// right) removes the records whose input2 it holds; the likeliest survivor changes the cell.
    fn required_reactions(&mut self, x: i32, y: i32, roll: i32) -> bool {
        const SIDES: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        let own = self.get(x, y).expect("in bounds");
        let mut list: Vec<Reaction> = self
            .materials
            .tables
            .required_for(own, roll)
            .into_iter()
            .cloned()
            .collect();
        if list.is_empty() {
            return false;
        }
        for (side, (dx, dy)) in SIDES.into_iter().enumerate() {
            let (nx, ny) = (x + dx, y + dy);
            if !self.movable(nx, ny) {
                continue;
            }
            let neighbor = self.get(nx, ny).unwrap_or(AIR);
            // A record for another side ends the scan instead of being skipped (0x70ef90).
            for i in 0..list.len() {
                if list[i].direction.is_some_and(|d| d as usize != side) {
                    break;
                }
                if list[i].input2 == neighbor {
                    list.swap_remove(i);
                    break;
                }
            }
            if list.is_empty() {
                return false;
            }
        }
        let mut best = &list[0];
        for r in &list[1..] {
            if best.probability < r.probability {
                best = r;
            }
        }
        if best.output1 == own {
            return false;
        }
        let output = best.output1;
        self.change_material(x, y, output);
        true
    }

    fn react_with_neighbor(&mut self, x: i32, y: i32, fast: bool) {
        let own = self.get(x, y).expect("in bounds");
        let max = self.materials.reaction_max_probability(fast, own);
        if fast && max <= 0 {
            return;
        }
        let roll = self.rng().rand() % 10001;
        if !fast {
            let required = self.materials.tables.max_required_probability(own);
            if required > 0 && roll < required && self.required_reactions(x, y, roll) {
                return;
            }
        }
        if roll > max {
            return;
        }
        let (nx, ny) = self.step_direction(x, y);
        if !self.movable(nx, ny) {
            return;
        }
        let neighbor = self.get(nx, ny).unwrap_or(AIR);
        let Some(reaction) = self
            .materials
            .tables
            .lookup(fast, own, neighbor, roll)
            .cloned()
        else {
            return;
        };
        if roll <= reaction.probability {
            self.apply_reaction(&reaction, (x, y), (nx, ny));
        }
    }

    /// vt[16]: liquids and fire count frames since creation, gases their moves.
    fn cell_age(&self, x: i32, y: i32) -> i32 {
        let cell = self.cell(x, y).expect("in bounds");
        match self.materials.get(cell.material).map(|m| m.cell_type) {
            Some(CellType::Gas) => cell.age,
            _ => self.frame.wrapping_sub(cell.created) as i32,
        }
    }

    /// 0x70ebf0. Unported: `convert_all` (world vfn 0x34), entity spawns, audio and attached
    /// explosions (which also draw from the RNG).
    fn apply_reaction(&mut self, reaction: &Reaction, (x, y): (i32, i32), (nx, ny): (i32, i32)) {
        if reaction.req_lifetime > 0 && self.cell_age(x, y) < reaction.req_lifetime {
            return;
        }
        if let Some(direction) = reaction.direction {
            let side = match (nx.cmp(&x), ny.cmp(&y)) {
                (std::cmp::Ordering::Less, _) => Direction::Left,
                (std::cmp::Ordering::Greater, _) => Direction::Right,
                (_, std::cmp::Ordering::Less) => Direction::Top,
                _ => Direction::Bottom,
            };
            if side != direction {
                return;
            }
        }
        if let Some((input3, output3)) = reaction.third {
            let (mut tx, mut ty) = self.step_direction(x, y);
            if (tx, ty) == (nx, ny) {
                (tx, ty) = (2 * x - nx, 2 * y - ny);
            }
            if !self.movable(tx, ty) {
                return;
            }
            if self.get(tx, ty).unwrap_or(AIR) != input3 {
                return;
            }
            if let Some(output3) = output3
                && !reaction.convert_all
            {
                self.set_or_change(tx, ty, output3);
            }
        }
        if reaction.blob_radius2 != 0 {
            self.queue_blob(
                (nx, ny),
                reaction.output2,
                reaction.blob_radius2,
                reaction.blob_restrict2,
                reaction.input2,
                reaction.cosmetic_particle,
            );
        } else if !reaction.convert_all {
            self.set_or_change(nx, ny, reaction.output2);
        }
        if reaction.blob_radius1 != 0 {
            self.queue_blob(
                (x, y),
                reaction.output1,
                reaction.blob_radius1,
                reaction.blob_restrict1,
                reaction.input1,
                reaction.cosmetic_particle,
            );
        } else if !reaction.convert_all {
            let lonely = reaction.destroy_lonely_pixels
                && !self.occupied(x - 1, y)
                && !self.occupied(x + 1, y);
            let output1 = if lonely { AIR } else { reaction.output1 };
            self.change_material(x, y, output1);
        }
    }

    /// ChangeMaterial on an existing cell, or a deferred SetCell (world vfn 0x20) into empty space.
    fn set_or_change(&mut self, x: i32, y: i32, target: MaterialId) {
        if self.occupied(x, y) {
            self.change_material(x, y, target);
        } else if target != AIR {
            let region = self.current.expect("inside a region update");
            self.regions[region]
                .cell_changes
                .push((x, y, target, false));
        }
    }

    /// Cell vt[38] (0x70f360): deferred SetCell for air, a different cell type, or a burning cell
    /// whose target is not `burnable` (`+0x8E`); otherwise in place, keeping the burning state.
    fn change_material(&mut self, x: i32, y: i32, target: MaterialId) {
        let Some(cell) = self.cell(x, y).copied() else {
            return;
        };
        if target == cell.material {
            return;
        }
        let target_props = self.materials.get(target).expect("known material");
        let current_props = self.materials.get(cell.material).expect("known material");
        if target == AIR
            || current_props.cell_type != target_props.cell_type
            || (cell.burning && !target_props.burnable)
        {
            // World vfn 0x20 with replace set: queued by a worker, immediate on `GridWorld`
            // (blob flush).
            match self.current {
                Some(region) => self.regions[region].cell_changes.push((x, y, target, true)),
                None => self.set_cell(x, y, target, true),
            }
        } else {
            let (color, is_static) = (target_props.color_at(x, y), target_props.liquid_static);
            let is_liquid = target_props.cell_type == CellType::Liquid;
            let frame = self.frame;
            let converted = self.cell_mut(x, y).expect("in bounds");
            converted.material = target;
            converted.color = color;
            converted.base_color = color;
            converted.is_static = is_static;
            // Liquid vt[13] (0x70d0b0) restamps the creation frame.
            if is_liquid {
                converted.created = frame;
            }
            self.mark(x, y);
        }
    }

    /// Static path of 0x70c6e0 for `liquid_sticks_to_ceiling` cells: come loose when the neighbor
    /// they stick to no longer holds, or with chance `sticks/101` when the cell below is empty.
    fn update_stuck(&mut self, x: i32, y: i32, state: u8, sticks: i32) -> bool {
        let (sx, sy) = match state {
            STATE_STUCK_RIGHT => (x + 1, y),
            STATE_STUCK_LEFT => (x - 1, y),
            _ => (x, y - 1),
        };
        let holds = self.movable(sx, sy) && self.holds_stuck_cell(sx, sy);
        if holds
            && (self.rng().rand() % 101 >= sticks
                || !self.movable(x, y + 1)
                || self.get(x, y + 1) != Some(AIR))
        {
            return false;
        }
        self.cell_mut(x, y).expect("in bounds").is_static = false;
        true
    }

    /// `CLiquidCell` vt[27] (0x70b160): 1 = firm support, 2 = settled sand without stickiness.
    fn support_type(&self, x: i32, y: i32) -> u8 {
        let Some((c, m)) = self.liquid_at(x, y) else {
            return 0;
        };
        let settled = c.state == STATE_RESTING && c.rest >= 20;
        if c.is_static {
            1
        } else if m.liquid_sand_never_box2d || !m.liquid_sand {
            0
        } else if m.liquid_sticks_to_ceiling != 0
            && (matches!(c.state, STATE_STUCK_RIGHT..=STATE_STUCK_LEFT) || settled)
        {
            1
        } else if settled {
            2
        } else {
            0
        }
    }

    /// 0x70ac50: sand that a sticky cell can attach to.
    fn sticks_to(&self, x: i32, y: i32) -> bool {
        self.liquid_at(x, y).is_some_and(|(_, m)| m.liquid_sand) && self.support_type(x, y) == 1
    }

    /// 0x70ac80: a neighbor that keeps a stuck cell in place.
    fn holds_stuck_cell(&self, x: i32, y: i32) -> bool {
        self.liquid_at(x, y)
            .is_some_and(|(c, _)| matches!(c.state, STATE_STUCK_RIGHT..=STATE_STUCK_LEFT))
            || self.support_type(x, y) == 1
    }

    /// `CLiquidCell` vt[26] (0x70b120): what slime treats as solid above it.
    fn holds_slime(&self, x: i32, y: i32) -> bool {
        self.liquid_at(x, y).is_some_and(|(c, m)| {
            c.is_static || (!m.liquid_sand_never_box2d && m.liquid_sand && c.state == STATE_RESTING)
        })
    }

    /// The cell and its material if `(x, y)` holds a liquid-class cell.
    fn liquid_at(&self, x: i32, y: i32) -> Option<(&Cell, &MaterialProps)> {
        let c = self.cell(x, y)?;
        let m = self.materials.get(c.material)?;
        (c.material != AIR && m.cell_type == CellType::Liquid).then_some((c, m))
    }

    fn occupied(&self, x: i32, y: i32) -> bool {
        self.get(x, y).is_some_and(|m| m != AIR)
    }

    /// `CGasCell` vt[21] (0x70a670). Gas heat and fire emissions remain unported.
    fn update_gas(&mut self, x: i32, y: i32, full: bool) {
        let frame = self.frame;
        let cell = *self.cell(x, y).expect("in bounds");
        if cell.frame == frame {
            return;
        }
        self.cell_mut(x, y).expect("in bounds").frame = frame;
        let mat = self.materials.get(cell.material).expect("known material");
        let (lifetime, is_static, speed) = (mat.lifetime, mat.liquid_static, mat.gas_speed);
        if lifetime > 0 && cell.age >= lifetime {
            // vt[38] 0x70f360 with material 0 queues world vfn 0x20 (0x72b170).
            let r = self.current.expect("inside a region update");
            self.regions[r].cell_changes.push((x, y, AIR, true));
            return;
        }
        if !full && cell.rest >= 100 {
            return;
        }
        if is_static {
            self.cell_mut(x, y).expect("in bounds").rest = 100;
        } else if self.rng().rand() % 101 > i32::from(speed) {
            if self.movable(x, y - 1) && self.gas_can_swap(x, y, x, y - 1) {
                self.mark(x, y);
            }
            return;
        }
        // 0x70a670 also runs a heat block here (`+0x2C > 0`: cool by `rand() % 3`, mirror into the
        // fire intensity, maybe spread fire); no code sets a gas cell's heat, so it never runs.
        if self.cell(x, y).expect("in bounds").burning {
            self.burn_hook(x, y);
            self.mark(x, y);
        }
        self.move_gas(x, y, full);
    }

    /// Shared burn hook (0x70fb10) for burning cells: roll a flame (0x70f8c0) and a smoke
    /// (0x70f800) emission into the row above, lose or refill intensity when the material
    /// `requires_oxygen`, then spread fire. The `+0x100` particle effect roll and the heat
    /// aggregates (`worker+0x170/+0x1B8`, queue `+0x4D8`) are unported.
    fn burn_hook(&mut self, x: i32, y: i32) {
        let Some(cell) = self.cell(x, y).copied() else {
            return;
        };
        let Some(props) = self.materials.get(cell.material) else {
            return;
        };
        if !cell.burning {
            return;
        }
        let (flames, smoke, oxygen, refill) = (
            props.generates_flames,
            props.generates_smoke,
            props.requires_oxygen,
            props.temperature_of_fire as u8,
        );
        let (flame, smoke_material) = (
            self.materials.find(&props.on_fire_flame_material),
            self.materials.find(&props.on_fire_smoke_material),
        );

        let mut emitted = false;
        if flames > 0 && self.rng().rand() % 101 < flames {
            // The amount only matters on the immediate-create path, which workers do not take.
            self.rng().rand();
            emitted = self.emit_above(x, y, flame);
        }
        if oxygen {
            let c = self.cell_mut(x, y).expect("in bounds");
            c.fire_intensity = if emitted {
                refill
            } else {
                c.fire_intensity.wrapping_sub(1)
            };
            if (c.fire_intensity as i8) < 1 {
                c.burning = false;
            }
        }
        if smoke > 0 && self.rng().rand() % 101 < smoke {
            self.rng().rand();
            self.emit_above(x, y, smoke_material);
        }
        if self.cell(x, y).expect("in bounds").burning {
            self.spread_fire(x, y);
            self.mark(x, y);
        }
    }

    /// 0x70f8c0 / 0x70f800: `rand() % 3` picks the cell up-left, above or up-right; an empty,
    /// movable one gets a deferred SetCell without replace.
    fn emit_above(&mut self, x: i32, y: i32, material: Option<MaterialId>) -> bool {
        let tx = x + self.rng().rand() % 3 - 1;
        let ty = y - 1;
        if !(self.movable(tx, ty) && self.in_bounds(tx, ty)) || self.occupied(tx, ty) {
            return false;
        }
        if let Some(material) = material.filter(|&m| m != AIR) {
            let region = self.current.expect("inside a region update");
            self.regions[region]
                .cell_changes
                .push((tx, ty, material, false));
        }
        true
    }

    /// Gas movement (0x70a810): up, then sideways (last direction first), then down into empty
    /// space, each gated by its `gas_*_speed` percent roll. A gas that did not move runs the
    /// reaction check on full updates and fast reactions otherwise.
    fn move_gas(&mut self, x: i32, y: i32, full: bool) {
        let cell = *self.cell(x, y).expect("in bounds");
        let mat = self.materials.get(cell.material).expect("known material");
        let (up, side, down) = (
            i32::from(mat.gas_upwards_speed),
            i32::from(mat.gas_horizontal_speed),
            i32::from(mat.gas_downwards_speed),
        );
        if !mat.liquid_static {
            if self.rng().rand() % 101 < up
                && self.movable(x, y - 1)
                && self.gas_can_swap(x, y, x, y - 1)
            {
                self.gas_move_to((x, y), (x, y - 1));
                return;
            }
            if self.rng().rand() % 101 < side {
                let d = i32::from(cell.dir);
                if self.movable(x + d, y) && self.gas_can_swap(x, y, x + d, y) {
                    self.gas_move_to((x, y), (x + d, y));
                    return;
                }
                if self.movable(x - d, y) && self.gas_can_swap(x, y, x - d, y) {
                    self.gas_move_to((x, y), (x - d, y));
                    self.cell_mut(x - d, y).expect("in bounds").dir = -cell.dir;
                    return;
                }
            }
            if self.rng().rand() % 101 < down && self.movable(x, y + 1) && !self.occupied(x, y + 1)
            {
                self.gas_move_to((x, y), (x, y + 1));
                return;
            }
            let c = self.cell_mut(x, y).expect("in bounds");
            if c.rest < 100 {
                c.rest += 1;
            }
        }
        let rest = self.cell(x, y).expect("in bounds").rest;
        if rest < 100 && self.movable(x, y - 1) && self.gas_can_swap(x, y, x, y - 1) {
            self.mark(x, y);
        }
        if full {
            self.check_reactions(x, y);
        } else if self.materials.reaction_max_probability(true, cell.material) > 0 {
            self.fast_reactions(x, y);
        }
    }

    /// Whether the gas at `(x, y)` may swap into `(tx, ty)` (0x70a5a0): empty yes; solids and
    /// liquids no; the same gas no; otherwise `rand() % 101 >= 75` and a denser gas.
    fn gas_can_swap(&mut self, x: i32, y: i32, tx: i32, ty: i32) -> bool {
        let Some(target) = self.get(tx, ty).filter(|&m| m != AIR) else {
            return true;
        };
        let mover = self.get(x, y).expect("in bounds");
        let kind = |w: &Self, id| w.materials.get(id).map(|m| m.cell_type);
        let target_kind = kind(self, target);
        if matches!(target_kind, Some(CellType::Solid | CellType::Liquid)) {
            return false;
        }
        let target_is_gas = target_kind == Some(CellType::Gas);
        if target_is_gas && target == mover {
            return false;
        }
        if self.rng().rand() % 101 < 75 || !target_is_gas {
            return false;
        }
        let density = |w: &Self, id| w.materials.get(id).expect("known").density;
        density(self, target) > density(self, mover)
    }

    /// Gas move (vt[18] 0x70aad0): swap with the target, reset the rest counter on a vertical
    /// move, count the move in `age`, and mark the new position.
    fn gas_move_to(&mut self, from: (i32, i32), to: (i32, i32)) {
        let a = *self.cell(from.0, from.1).expect("in bounds");
        let b = *self.cell(to.0, to.1).expect("in bounds");
        *self.cell_mut(from.0, from.1).expect("in bounds") = b;
        let moved = self.cell_mut(to.0, to.1).expect("in bounds");
        *moved = a;
        if to.1 != from.1 {
            moved.rest = 0;
        }
        moved.age += 1;
        self.mark(to.0, to.1);
    }

    /// Slime movement (0x70b7c0). Returns the cell's final position.
    fn move_slime(&mut self, x: i32, y: i32) -> (i32, i32) {
        let cell = *self.cell(x, y).expect("in bounds");
        let id = cell.material;
        let mat = self.materials.get(id).expect("known material");
        let (gravity, viscosity) = (mat.liquid_gravity, mat.liquid_viscosity);

        // Crawl diagonally down along an occupied side; `rand() & 1` picks which side first.
        let order = if self.rng().rand() & 1 == 1 {
            [-1, 1]
        } else {
            [1, -1]
        };
        let mut dir = 0;
        for d in order {
            if self.occupied(x + d, y) && self.displaceable(id, x + d, y + 1) {
                dir = d;
                self.cell_mut(x, y).expect("in bounds").flow = d as f32;
                break;
            }
        }

        if self.displaceable(id, x + dir, y + 1) {
            self.wake(x, y - 1);
            let (mut cx, mut cy) = self.move_to((x, y), (x + dir, y + 1));
            if self.movable(cx, cy - 1) && self.holds_slime(cx, cy - 1) {
                return (cx, cy);
            }
            let c = self.cell_mut(cx, cy).expect("in bounds");
            if cell.state != STATE_FALLING {
                c.vel = 0.0;
                c.state = STATE_FALLING;
                return (cx, cy);
            }
            c.vel += gravity * 0.25;
            let mut v = c.vel - 1.0;
            while v > 0.0 {
                if (self.rng().rand() & 0x7F) < 12 {
                    dir = self.random_dx();
                }
                if !self.displaceable(id, cx + dir, cy + 1) {
                    self.cell_mut(cx, cy).expect("in bounds").vel = 0.0;
                    return (cx, cy);
                }
                (cx, cy) = self.move_to((cx, cy), (cx + dir, cy + 1));
                v -= 2.0;
            }
            return (cx, cy);
        }

        if viscosity < 100 && self.rng().rand() % 101 < viscosity {
            return (x, y);
        }
        for d in [-1, 1] {
            if self.displaceable(id, x + d, y + 1) {
                self.wake(x, y - 1);
                let (nx, ny) = self.move_to((x, y), (x + d, y + 1));
                self.cell_mut(nx, ny).expect("in bounds").state = STATE_FALLING;
                return (nx, ny);
            }
        }

        // Sideways only towards an empty side while the other is occupied (or outside the world).
        let (l_in, r_in) = (self.movable(x - 1, y), self.movable(x + 1, y));
        let left_empty = l_in && !self.occupied(x - 1, y);
        let right_empty = r_in && !self.occupied(x + 1, y);
        let dir = if right_empty && !left_empty {
            1
        } else if left_empty && !right_empty {
            -1
        } else {
            if self.occupied(x + 1, y) && self.occupied(x - 1, y) {
                self.cell_mut(x, y).expect("in bounds").state = STATE_RESTING;
            }
            return (x, y);
        };
        let c = self.cell_mut(x, y).expect("in bounds");
        c.flow += dir as f32 * 0.9;
        c.state = STATE_FLOWING;
        if c.flow.abs() < 1.0 {
            return (x, y);
        }
        let (nx, ny) = self.move_to((x, y), (x + dir, y));
        let c = self.cell_mut(nx, ny).expect("in bounds");
        c.flow -= dir as f32;
        c.state = STATE_FLOWING;
        (nx, ny)
    }

    /// Sticky side for `liquid_sticks_to_ceiling` sand at `(x, y)` (0x70acc0): a position hash
    /// gives 3..5 on odd rows and 7..9 on even rows; 5 = right, 7 = left, others = up.
    fn stick_side(x: i32, y: i32) -> i32 {
        let s = x
            .wrapping_add(0x6757_7458)
            .wrapping_add(y.wrapping_mul(0x5B));
        let a = ((s >> 13) ^ s)
            .wrapping_mul(0x343FD)
            .wrapping_add(0x26_9EC3);
        let v = ((a >> ((x & 0xF) + 1)) ^ a) & 0x7FFF;
        if y % 2 != 0 { 5 - v % 3 } else { 7 + v % 3 }
    }

    /// Sand movement (0x70b2d0).
    fn move_sand(&mut self, x: i32, y: i32) -> Option<(i32, i32)> {
        let cell = *self.cell(x, y).expect("in bounds");
        let id = cell.material;
        let mat = self.materials.get(id).expect("known material");
        let (gravity, viscosity) = (mat.liquid_gravity, mat.liquid_viscosity);

        if mat.liquid_sticks_to_ceiling > 0 {
            let side = Self::stick_side(x, y);
            let (sx, sy, state) = match side {
                5 => (x + 1, y, STATE_STUCK_RIGHT),
                7 => (x - 1, y, STATE_STUCK_LEFT),
                _ => (x, y - 1, STATE_STUCK_UP),
            };
            if self.movable(sx, sy) && self.sticks_to(sx, sy) {
                let c = self.cell_mut(x, y).expect("in bounds");
                c.is_static = true;
                c.state = state;
                return Some((x, y));
            }
        }

        if cell.state == STATE_FALLING {
            let r = self.rng().rand() as f32 / 32767.0;
            if (1.0 - gravity) * 0.5 > r {
                self.mark(x, y);
                return Some((x, y));
            }
        }

        if self.displaceable(id, x, y + 1) {
            self.wake(x, y - 1);
            let (mut cx, mut cy) = self.move_to((x, y), (x, y + 1));
            if cell.state != STATE_FALLING {
                let c = self.cell_mut(cx, cy).expect("in bounds");
                c.vel = 0.0;
                c.state = STATE_FALLING;
                if self.on_moving_liquid(cx, cy + 1) {
                    self.destroy(cx, cy);
                    return None;
                }
                return Some((cx, cy));
            }
            let c = self.cell_mut(cx, cy).expect("in bounds");
            c.vel += gravity * 0.25;
            let mut v = c.vel - 1.0;
            let mut dx = 0;
            while v > 0.0 {
                if (self.rng().rand() & 0x7F) < 0x40 {
                    dx = self.random_dx();
                }
                if !self.displaceable(id, cx + dx, cy + 1) {
                    if self.on_moving_liquid(cx + dx, cy + 1) {
                        self.destroy(cx, cy);
                        return None;
                    }
                    if self.occupied(cx + dx, cy + 1) {
                        self.cell_mut(cx, cy).expect("in bounds").vel = 0.0;
                    }
                    return Some((cx, cy));
                }
                (cx, cy) = self.move_to((cx, cy), (cx + dx, cy + 1));
                v -= 2.0;
            }
            return Some((cx, cy));
        }

        if viscosity < 100 && self.rng().rand() % 101 < viscosity {
            for dx in [-1, 1] {
                if self.displaceable(id, x + dx, y + 1) {
                    self.wake(x, y - 1);
                    let (nx, ny) = self.move_to((x, y), (x + dx, y + 1));
                    self.cell_mut(nx, ny).expect("in bounds").state = STATE_FALLING;
                    return Some((nx, ny));
                }
            }
        }

        // Out-of-world neighbors read as empty, like unloaded chunks in the reference.
        let occupied = |w: &Self, nx: i32| w.get(nx, y).is_some_and(|m| m != AIR);
        if occupied(self, x + 1) && occupied(self, x - 1) {
            self.cell_mut(x, y).expect("in bounds").state = STATE_RESTING;
        }
        Some((x, y))
    }

    /// Non-sand, non-slime liquid movement (0x70bdc0).
    fn move_liquid(&mut self, x0: i32, y0: i32) -> Option<(i32, i32)> {
        let cell = *self.cell(x0, y0).expect("in bounds");
        let id = cell.material;
        let mat = self.materials.get(id).expect("known material");
        let (gravity, damping, flow_speed) = (
            mat.liquid_gravity,
            mat.liquid_damping,
            mat.liquid_flow_speed,
        );
        let stains_self = mat.liquid_stains_self;
        let (mut x, mut y) = (x0, y0);
        let mut moved_side = false;
        let mut moved_down = false;
        let fast = cell.vel >= gravity * 0.25;

        if self.displaceable(id, x, y + 1) {
            self.wake(x, y - 1);
            moved_down = true;
            (x, y) = self.move_to((x, y), (x, y + 1));
            let c = self.cell_mut(x, y).expect("in bounds");
            c.vel += gravity * 0.25;
            let mut v = c.vel - 1.0;
            if c.fall_frames < 0xFE {
                c.fall_frames += 1;
            }
            let mut dx = 0;
            while v > 0.0 {
                if (self.rng().rand() & 0x7F) < 0x40 {
                    let flow = self.cell(x, y).expect("in bounds").flow;
                    dx = if flow.abs() >= 1.0 {
                        if flow >= 0.0 { 1 } else { -1 }
                    } else {
                        self.random_dx()
                    };
                }
                if self.displaceable(id, x + dx, y + 1) {
                    (x, y) = self.move_to((x, y), (x + dx, y + 1));
                    v -= 2.0;
                    continue;
                }
                if self.movable(x + dx, y + 1) && self.occupied(x + dx, y + 1) {
                    let fall_frames = self.cell(x, y).expect("in bounds").fall_frames;
                    if fall_frames > 15 && fall_frames < u8::MAX && !stains_self {
                        self.record_splash(x, y);
                    }
                    self.cell_mut(x, y).expect("in bounds").fall_frames = 0xFF;
                    if fast && self.on_moving_liquid(x + dx, y + 1) {
                        self.destroy(x, y);
                        return None;
                    }
                    self.cell_mut(x, y).expect("in bounds").vel = 0.0;
                }
                break;
            }
        } else if self.movable(x, y + 1) && self.occupied(x, y + 1) {
            if fast && self.on_moving_liquid(x, y + 1) {
                self.destroy(x, y);
                return None;
            }
            self.cell_mut(x, y).expect("in bounds").vel = 0.0;
        }

        let end = 'flow: {
            if !self.movable(x, y + 1) {
                break 'flow if moved_down {
                    FlowEnd::Moved
                } else {
                    FlowEnd::Rest
                };
            }
            let below = *self.cell(x, y + 1).expect("in bounds");
            if !self.can_displace(id, x, y + 1) {
                self.cell_mut(x, y).expect("in bounds").fall_frames = 0xFF;
                let below_moving = self.is_liquid(below.material) && below.vel > 0.0;
                if !below_moving {
                    moved_side |= self.start_flow(id, &mut x, y, flow_speed, damping);
                }
            }

            let flow = self.cell(x, y).expect("in bounds").flow;
            if flow.abs() >= 1.0 {
                let dir = if flow > 0.0 { 1 } else { -1 };
                let back_x = x - dir;
                while self.cell(x, y).expect("in bounds").flow.abs() >= 1.0
                    && self.displaceable(id, x + dir, y)
                {
                    (x, y) = self.move_to((x, y), (x + dir, y));
                    moved_side = true;
                    self.cell_mut(x, y).expect("in bounds").flow *= damping;
                }
                if moved_side {
                    self.wake(back_x, y);
                    break 'flow FlowEnd::Moved;
                }
                break 'flow if moved_down {
                    FlowEnd::Moved
                } else {
                    FlowEnd::Rest
                };
            }

            if moved_down || moved_side {
                break 'flow FlowEnd::Moved;
            }
            if !self.movable(x, y + 1) || self.can_displace(id, x, y + 1) {
                break 'flow FlowEnd::Rest;
            }
            let dir = if self.rng().range(0, 1) == 0 { 1 } else { -1 };
            for d in [dir, -dir] {
                if self.displaceable(id, x + d, y + 1) {
                    self.cell_mut(x, y).expect("in bounds").flow = d as f32;
                    self.wake(x - d, y - 1);
                    (x, y) = self.move_to((x, y), (x + d, y + 1));
                    moved_down = true;
                    break 'flow FlowEnd::Moved;
                }
            }
            FlowEnd::Rest
        };

        let c = self.cell_mut(x, y).expect("in bounds");
        match end {
            FlowEnd::Moved => {
                c.rest = 0;
                if moved_down {
                    c.state = if c.vel >= 4.0 {
                        STATE_FAST
                    } else {
                        STATE_FALLING
                    };
                } else {
                    c.state = STATE_FLOWING;
                    c.fall_frames = 0xFF;
                }
                self.mark(x, y);
                self.wake(x, y + 1);
            }
            FlowEnd::Rest => {
                c.fall_frames = 0xFF;
                c.state = STATE_RESTING;
                if c.rest < 5 {
                    c.rest += 1;
                    self.mark(x, y);
                }
            }
        }
        Some((x, y))
    }

    fn record_splash(&mut self, x: i32, y: i32) {
        let Some(region) = self.current else {
            return;
        };
        let stats = &mut self.regions[region].splash;
        stats.sum_x += x as f32;
        stats.sum_y += y as f32;
        stats.sample_total += 1.0;
        stats.event_count = stats.event_count.wrapping_add(1);
    }

    /// Lateral flow start (0x70c1ee..0x70c317): push towards the only open side. Returns whether
    /// the cell moved; updates `x`.
    fn start_flow(
        &mut self,
        id: MaterialId,
        x: &mut i32,
        y: i32,
        flow_speed: f32,
        damping: f32,
    ) -> bool {
        let right_open = self.movable(*x + 1, y)
            && self.can_displace(id, *x + 1, y)
            && !self.can_displace(id, *x - 1, y);
        let dir = if right_open {
            1
        } else if self.movable(*x - 1, y)
            && self.can_displace(id, *x - 1, y)
            && !self.can_displace(id, *x + 1, y)
        {
            -1
        } else {
            return false;
        };
        self.wake(*x - dir, y);
        let c = self.cell_mut(*x, y).expect("in bounds");
        c.flow += dir as f32 * flow_speed;
        if c.flow * dir as f32 >= 1.0 {
            (*x, _) = self.move_to((*x, y), (*x + dir, y));
            self.cell_mut(*x, y).expect("in bounds").flow *= damping;
            return true;
        }
        false
    }

    /// `rand() & 7`: 1 -> -1, 2 -> +1, else 0.
    fn random_dx(&mut self) -> i32 {
        match self.rng().rand() & 7 {
            1 => -1,
            2 => 1,
            _ => 0,
        }
    }

    /// `(x, y)` holds a liquid (vt[22]) not yet updated this frame (`+0x2C`) and still falling.
    fn on_moving_liquid(&self, x: i32, y: i32) -> bool {
        self.cell(x, y)
            .is_some_and(|o| self.is_liquid(o.material) && o.frame < self.frame && o.vel > 0.0)
    }

    /// A mover landing on such a liquid clears its own grid slot and is queued through world vfn
    /// 0x4C (`0x72b350` -> `GridWorld` vt[19] `0x71f7d0`, `+0x5A4`) for deletion after the update
    /// (`0x7183f2`: vt[39] then the destructor): the cell is lost, without a mark.
    fn destroy(&mut self, x: i32, y: i32) {
        *self.cell_mut(x, y).expect("in bounds") = Cell::EMPTY;
    }

    /// Bounds check (world vfn 0x74) followed by the displacement check.
    fn displaceable(&mut self, mover: MaterialId, x: i32, y: i32) -> bool {
        self.movable(x, y) && self.can_displace(mover, x, y)
    }

    /// Displacement check (0x70b200). Outside the world reads as a null cell (displaceable), as the
    /// reference's grid lookup does for unloaded areas; callers bounds-check where it does.
    /// Gas and fire cells always give way; solids never do.
    fn can_displace(&mut self, mover: MaterialId, x: i32, y: i32) -> bool {
        let Some(target) = self.get(x, y) else {
            return true;
        };
        if target == AIR {
            return true;
        }
        let kind = self.materials.get(target).map(|m| m.cell_type);
        if matches!(kind, Some(CellType::Gas | CellType::Fire)) {
            return true;
        }
        if self.rng().rand() % 101 < 75 {
            return false;
        }
        if !self.is_liquid(target) {
            return false;
        }
        let (m, t) = (
            self.materials.get(mover).expect("known"),
            self.materials.get(target).expect("known"),
        );
        if m.liquid_sand && t.liquid_sand {
            return false;
        }
        m.density > t.density
    }

    fn is_liquid(&self, id: MaterialId) -> bool {
        id != AIR
            && self
                .materials
                .get(id)
                .is_some_and(|m| m.cell_type == CellType::Liquid)
    }

    /// Wake (0x70abf0): marks an occupied cell within the movable area and calls its vt[17], which
    /// caps a liquid's rest counter at 45 (0x70d180) and does nothing for other cells.
    fn wake(&mut self, x: i32, y: i32) {
        if !self.movable(x, y) || !self.occupied(x, y) {
            return;
        }
        self.mark(x, y);
        if let Some(c) = self.cell(x, y).copied()
            && self.is_liquid(c.material)
        {
            let c = self.cell_mut(x, y).expect("in bounds");
            c.rest = c.rest.min(45);
        }
    }

    /// Swap (0x70cc00): the mover and the target cell exchange places; the mover's new position is
    /// marked. Returns the new position.
    fn move_to(&mut self, from: (i32, i32), to: (i32, i32)) -> (i32, i32) {
        let a = *self.cell(from.0, from.1).expect("in bounds");
        let b = *self.cell(to.0, to.1).expect("in bounds");
        *self.cell_mut(from.0, from.1).expect("in bounds") = b;
        let moved = self.cell_mut(to.0, to.1).expect("in bounds");
        *moved = a;
        moved.rest = moved.rest.saturating_sub(1);
        self.mark(to.0, to.1);
        to
    }

    fn cell_mut(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        let (c, i) = self.locate(x, y)?;
        Some(&mut self.chunks[c][i])
    }

    fn locate(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let (cx, cy) = self.chunk_of(x, y)?;
        let cs = self.chunk_size;
        let chunk = (cy * self.width_chunks + cx) as usize;
        let local = ((y % cs) * cs + (x % cs)) as usize;
        Some((chunk, local))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_world(width_chunks: u32, height_chunks: u32) -> World {
        let air = MaterialProps::new("air", CellType::Unspecified);
        World::new(
            width_chunks,
            height_chunks,
            64,
            Materials::new(vec![air]),
            3,
        )
    }

    #[test]
    fn shuffle_is_a_seeded_permutation() {
        let mut a = MsvcRand::new(9);
        let mut b = MsvcRand::new(9);
        let order = shuffled(50, &mut a);
        assert_eq!(order, shuffled(50, &mut b));
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(order, sorted, "50 elements are not left in order");
    }

    /// The scheduler fully scans one region per frame in shuffled order (0x726340), so even a
    /// region nothing marked is rescanned within `regions` frames.
    #[test]
    fn sweep_visits_every_region_once_per_cycle() {
        let mut w = empty_world(3, 2);
        let regions = w.regions.len();
        let mut visited = Vec::new();
        for _ in 0..regions {
            visited.push(w.sweep_order[w.sweep_pos]);
            w.step();
        }
        let mut sorted = visited.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..regions).collect::<Vec<_>>());
        assert_eq!(w.sweep_pos, 0, "the cycle wrapped and reshuffled");
    }

    #[test]
    fn blend_weights_the_first_color_and_keeps_the_second_alpha() {
        assert_eq!(blend_keep_alpha(0xFFFF_0000, 0x8000_0000, 0.2), 0x8033_0000);
        assert_eq!(blend_keep_alpha(0xFF12_3456, 0xFF12_3456, 0.5), 0xFF12_3456);
    }

    #[test]
    fn seven_tenths_dimming_truncates_each_channel() {
        assert_eq!(dimmed_to_seven_tenths(0xFF80_8080), 0xFF59_5959);
        assert_eq!(dimmed_to_seven_tenths(0xFF00_0000), 0xFF00_0000);
    }
}

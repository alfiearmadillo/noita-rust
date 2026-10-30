//! Biome material rules: the `<MaterialComponent>` list of a biome and how the reference picks a
//! material from it (`0x86d2a0`), with its helpers: the polka-dot field (`0x872140`) and its cell
//! hashes (`0x86fd40`, `0x86fe30`), fractal simplex noise (`0x9296d0`) and the point-in-polygon
//! test (`0xddc9a0`). Everything is single precision like the reference; `simplex2d` runs in doubles.
//!
//! Field offsets are those of the 0x84-byte `MaterialComponent` (serializer `0x77c680`).

use sim::noise::simplex2d;

/// `x` truncated, stepped down when that rounded up (`cvttss2si` plus the compare/decrement).
fn floor_f32(v: f32) -> i32 {
    let t = v as i32;
    if (t as f32) > v { t - 1 } else { t }
}

/// `v - floor(v)`.
fn frac(v: f32) -> f32 {
    v - floor_f32(v) as f32
}

/// Shared first stage of the cell hashes: `(y'^2) * (x'^2)` with `x' = x - 71 floor(x / 71) + 26`
/// and `y' = y - 71 floor(y / 71) + 161`.
fn cell_product(x: f32, y: f32) -> f32 {
    const INV71: f32 = f32::from_bits(0x3c66_c2b4);
    let cy = floor_f32(y * INV71);
    let cx = floor_f32(x * INV71);
    let xs = x - cx as f32 * 71.0 + 26.0;
    let ys = y - cy as f32 * 71.0 + 161.0;
    (ys * ys) * (xs * xs)
}

/// `0x86fd40`: the probability hash of a polka cell, in [0, 1).
pub fn cell_hash(x: f32, y: f32) -> f32 {
    const K: f32 = f32::from_bits(0x3a84_cd4e);
    frac(cell_product(x, y) * K)
}

/// `0x86fe30`: three hashes of a polka cell (offset x, offset y, radius), each in [0, 1).
pub fn cell_hash3(x: f32, y: f32) -> [f32; 3] {
    const K: [f32; 3] = [
        f32::from_bits(0x3a89_ce48),
        f32::from_bits(0x3acb_dc41),
        f32::from_bits(0x3aa3_2fcf),
    ];
    let p = cell_product(x, y);
    [frac(K[0] * p), frac(K[1] * p), frac(K[2] * p)]
}

/// `0x872140`: a field of random discs (or rounded boxes) in unit cells. Returns `(1 - d)^3` inside a
/// dot and 0 elsewhere. `probability` is the chance a cell holds a dot; the radius is drawn
/// between `radius_low` and `radius_high`.
pub fn polka(
    x: f32,
    y: f32,
    radius_low: f32,
    radius_high: f32,
    probability: f32,
    boxed: bool,
) -> f32 {
    let (ix, iy) = (floor_f32(x), floor_f32(y));
    if cell_hash(ix as f32, iy as f32) >= probability {
        return 0.0;
    }
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let r = cell_hash3(ix as f32, iy as f32);
    let radius = (radius_high - radius_low) * r[2] + radius_low;
    if 0.0 >= radius {
        return 0.0;
    }
    let s = 2.0 / radius;
    let dy = (s - 2.0) * r[1] + (fy * s - (s - 1.0));
    let dx = (s - 2.0) * r[0] + (fx * s - (s - 1.0));
    let (dy2, dx2) = (dy * dy, dx * dx);
    let sum = if boxed {
        let sum = dy2 * dy2 + dx2 * dx2;
        if sum > 1.0 {
            return 0.0;
        }
        sum
    } else {
        let sum = dy2 + dx2;
        if 1.0 > sum { sum } else { 1.0 }
    };
    let t = 1.0 - sum;
    t * t * t
}

/// The octave rotation matrix at `0x1054700` (stored as `[m0, m1, m2, m3]`).
const FBM_M: [f32; 4] = [
    f32::from_bits(0x3f57_6a94),
    f32::from_bits(0x3f0a_511a),
    f32::from_bits(0x3f0a_511a),
    f32::from_bits(0xbf57_6a94),
];

/// `0x9296d0`: four octaves of `simplex2d`, weights 1/2, 1/4, 1/8, 1/16, each octave rotated by
/// `FBM_M` and scaled by 2.02, 2.33 and 2.01; normalised by 0.9375.
pub fn fbm(x: f32, y: f32) -> f32 {
    let n = |a: f32, b: f32| simplex2d(f64::from(a), f64::from(b)) as f32;
    let m = FBM_M;
    let mut acc = n(x, y) * 0.5;
    let x1 = (m[0] * x + m[2] * y) * 2.02;
    let y1 = (m[1] * x + m[3] * y) * 2.02;
    acc += n(x1, y1) * 0.25;
    let x2 = (y1 * m[2] + x1 * m[0]) * 2.33;
    let y2 = (y1 * m[3] + x1 * m[1]) * 2.33;
    acc += n(x2, y2) * 0.125;
    let x3 = (y2 * m[2] + x2 * m[0]) * 2.01;
    let y3 = (y2 * m[3] + x2 * m[1]) * 2.01;
    acc += n(x3, y3) * 0.0625;
    acc / 0.9375
}

/// `0xddc9a0`: even-odd point-in-polygon with single-precision edge intersections.
pub fn point_in_polygon(px: f32, py: f32, poly: &[[f32; 2]]) -> bool {
    let n = poly.len();
    if n == 0 {
        return false;
    }
    let mut inside = false;
    let mut prev = poly[n - 1];
    let mut prev_above = py <= prev[1];
    for &cur in poly {
        let cur_above = py <= cur[1];
        if prev_above != cur_above {
            let mut crosses = px <= prev[0];
            if crosses != (px <= cur[0]) {
                crosses = px <= cur[0] - ((prev[0] - cur[0]) * (cur[1] - py)) / (prev[1] - cur[1]);
            }
            if crosses {
                inside = !inside;
            }
        }
        prev = cur;
        prev_above = cur_above;
    }
    inside
}

/// One `<MaterialComponent>`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rule {
    /// Resolved material (`+4`); 0 disables the rule.
    pub material: u32,
    /// `material_min` / `material_max` (`+0x24`, `+0x28`): the value window `[min, max)`.
    pub min: f32,
    pub max: f32,
    /// `add_perlin`, `add_perlin_scale_x/y` (`+0x2c`, `+0x30`, `+0x34`).
    pub add_perlin: bool,
    pub add_perlin_scale: [f32; 2],
    /// `is_rare` (`+0x38`): the rule also needs the rare-noise test.
    pub is_rare: bool,
    /// `limit_y`, `limit_min_y`, `limit_max_y` (`+0x39`, `+0x3c`, `+0x40`).
    pub limit_y: bool,
    pub limit_min_y: f32,
    pub limit_max_y: f32,
    /// `rare_use_perlin`, `rare_use_fbm_perlin`, `rare_use_polka` (`+0x44`, `+0x45`, `+0x46`).
    pub rare_use_perlin: bool,
    pub rare_use_fbm_perlin: bool,
    pub rare_use_polka: bool,
    /// `rare_scale_x/y` (`+0x48`, `+0x4c`).
    pub rare_scale: [f32; 2],
    /// `rare_offset_by_seed` (`+0x50`), `rare_offset_x/y` (`+0x54`, `+0x58`).
    pub rare_offset_by_seed: bool,
    pub rare_offset: [f32; 2],
    /// `rare_polka_radius_low/high` (`+0x5c`, `+0x60`), `rare_polka_is_boxed` (`+0x64`),
    /// `rare_polka_probability` (`+0x68`).
    pub polka_radius_low: f32,
    pub polka_radius_high: f32,
    pub polka_boxed: bool,
    pub polka_probability: f32,
    /// `rare_required_min/max` (`+0x6c`, `+0x70`): the rare value must lie in `(min, max]`.
    pub rare_required_min: f32,
    pub rare_required_max: f32,
    /// `is_polygon` (`+0x74`) and `polygon` (`+0x78`): the 512-cell tile position must be inside.
    pub is_polygon: bool,
    pub polygon: Vec<[f32; 2]>,
}

/// A biome's rules (`0x86d2a0`'s `this`): a global window `[lo, hi]` and the ordered list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleList {
    pub lo: f32,
    pub hi: f32,
    pub rules: Vec<Rule>,
}

fn mod512(v: i32) -> i32 {
    let r = v % 512;
    if r < 0 { r + 512 } else { r }
}

impl Rule {
    /// The rare-noise test: whether it keeps this rule at `(x, y)`.
    fn rare_passes(&self, x: i32, y: i32, world_seed: i32) -> bool {
        let mut px = self.rare_offset[0] + x as f32 * self.rare_scale[0];
        let mut py = self.rare_offset[1] + y as f32 * self.rare_scale[1];
        if self.rare_offset_by_seed {
            // The seed as an unsigned 32-bit value in double precision.
            let s = f64::from(world_seed) + if world_seed < 0 { 4_294_967_296.0 } else { 0.0 };
            px += (s * f64::from_bits(0x3ef0_21a3_4d53_6e32)) as f32;
            py += (s * f64::from_bits(0x3efa_4d38_60fd_4277)) as f32;
        }
        if self.rare_use_perlin {
            let v = if self.rare_use_fbm_perlin {
                fbm(px, py)
            } else {
                simplex2d(f64::from(px), f64::from(py)) as f32
            };
            if self.rare_required_min >= v || v > self.rare_required_max {
                return false;
            }
        }
        if self.rare_use_polka {
            let v = polka(
                px,
                py,
                self.polka_radius_low,
                self.polka_radius_high,
                self.polka_probability,
                self.polka_boxed,
            );
            if self.rare_required_min >= v || v > self.rare_required_max {
                return false;
            }
        }
        true
    }
}

impl RuleList {
    /// `0x86d2a0`: the first rule whose conditions hold at world position `(x, y)` for the value
    /// `v`; 0 when none does. `world_seed` is the global at `0x1205004`.
    pub fn pick(&self, x: i32, y: i32, v: f32, world_seed: i32) -> u32 {
        if self.lo > v || v > self.hi {
            return 0;
        }
        for rule in &self.rules {
            if rule.limit_y {
                let yf = y as f32;
                if rule.limit_min_y > yf || yf > rule.limit_max_y {
                    continue;
                }
            }
            if rule.is_polygon {
                let px = mod512(x) as f32 * (1.0 / 512.0);
                let py = mod512(y) as f32 * (1.0 / 512.0);
                if !point_in_polygon(px, py, &rule.polygon) {
                    continue;
                }
            }
            if rule.material == 0 {
                continue;
            }
            let mut f = v;
            if rule.add_perlin {
                let n = simplex2d(
                    f64::from(x as f32 * rule.add_perlin_scale[0]),
                    f64::from(y as f32 * rule.add_perlin_scale[1]),
                );
                f = n as f32 + v;
            }
            if f < rule.min || rule.max <= f {
                continue;
            }
            if !rule.is_rare || rule.rare_passes(x, y, world_seed) {
                return rule.material;
            }
        }
        0
    }
}

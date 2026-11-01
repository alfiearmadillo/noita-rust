//! Terrain lookup for Wang biomes: the biome's Wang map images, the samplers the reference runs
//! over them (`0x8712d0`, `0x870e60`, `0x871160`, `0x870f80`), the noise blend (`0x87d010`) and
//! the material lookup for a Wang biome (`0x87d0e0`, biome type 2).
//!
//! The images wrap toroidally and read as zero when empty (`0x92a1d0`, `0x92a310`).

use sim::noise::{simplex2, value2};

use crate::rules::RuleList;
use crate::warp::world_to_wang;

/// `cvttsd2si` then step down when that rounded up.
fn floor_i32(v: f64) -> i32 {
    let t = v as i32;
    if f64::from(t) > v { t - 1 } else { t }
}

/// A row-major image whose reads wrap around both axes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Image<T> {
    pub width: i32,
    pub height: i32,
    pub data: Vec<T>,
}

impl<T: Copy + Default> Image<T> {
    pub fn new(width: i32, height: i32, data: Vec<T>) -> Self {
        debug_assert_eq!(data.len(), (width.max(0) * height.max(0)) as usize);
        Self {
            width,
            height,
            data,
        }
    }

    /// `0x92a1d0` / `0x92a310` / `0x92a3a0`: the pixel at `(x, y)` with euclidean wrapping, or
    /// the default when the image has no data.
    pub fn get(&self, x: i32, y: i32) -> T {
        if self.data.is_empty() || self.width <= 0 || self.height <= 0 {
            return T::default();
        }
        let (x, y) = (x.rem_euclid(self.width), y.rem_euclid(self.height));
        self.data[(y * self.width + x) as usize]
    }
}

/// The part of a region's Wang object that the lookup reads: the float image (`+0x00`) and the
/// material image (`+0x20`, material index + 1, 0 = none).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WangMap {
    pub values: Image<f32>,
    pub materials: Image<u16>,
}

/// `(3 - 2t) t^2` in single precision.
fn smoothstep(t: f32) -> f32 {
    (3.0 - t * 2.0) * (t * t)
}

impl WangMap {
    /// `0x8712d0`: the material index (table index, or -1 for none) at `(u, v)`, rounding to the
    /// nearest pixel through the smoothstep of the fraction.
    pub fn round_index(&self, u: f64, v: f64) -> i32 {
        let (mut iu, mut iv) = (floor_i32(u), floor_i32(v));
        let fu = (u - f64::from(iu)) as f32;
        let fv = (v - f64::from(iv)) as f32;
        let a = smoothstep(fu);
        let b = smoothstep(fv);
        if 0.5 <= b {
            iv += 1;
        }
        if 0.5 <= a {
            iu += 1;
        }
        i32::from(self.materials.get(iu, iv)) - 1
    }

    /// `0x870e60`: bilinear sample of the float image with smoothstep weights.
    pub fn smooth(&self, u: f64, v: f64) -> f32 {
        let (iu, iv) = (floor_i32(u), floor_i32(v));
        let su = smoothstep((u - f64::from(iu)) as f32);
        let sv = smoothstep((v - f64::from(iv)) as f32);
        let g = |x, y| self.values.get(x, y);
        let (p01, p00, p10) = (g(iu, iv + 1), g(iu, iv), g(iu + 1, iv));
        let top = (p10 - p00) * su + p00;
        let p11 = g(iu + 1, iv + 1);
        let bottom = (p11 - p01) * su + p01;
        (bottom - top) * sv + top
    }

    /// `0x871160` (`wang_noise_type` 1): a solid pixel stays solid; an empty one is kept solid
    /// when the neighbours around its nearest corner are.
    pub fn edge_mode1(&self, u: f64, v: f64) -> f32 {
        let (u, v) = (u + 0.5, v + 0.5);
        let (iu, iv) = (floor_i32(u), floor_i32(v));
        let g = |x, y| self.values.get(x, y);
        if g(iu, iv) > 0.5 {
            return 1.0;
        }
        let fu = (u - f64::from(iu)) as f32;
        let fv = (v - f64::from(iv)) as f32;
        let inv = 1.0 - fu;
        let (px, py) = if fu >= fv {
            if inv > fv { (iu, iv - 1) } else { (iu + 1, iv) }
        } else if inv > fv {
            (iu - 1, iv)
        } else {
            (iu, iv + 1)
        };
        if g(px, py) >= 0.5 {
            return 1.0;
        }
        let dx = if 0.5 <= fu { 1 } else { -1 };
        let dy = if 0.5 <= fv { 1 } else { -1 };
        let (x1, y1) = (iu + dx, iv + dy);
        if 0.5 > g(x1, y1) {
            return 0.0;
        }
        if g(x1, iv) >= 0.5 {
            return 1.0;
        }
        if g(iu, y1) < 0.5 {
            return 0.0;
        }
        1.0
    }

    /// `0x870f80` (`wang_noise_type` 2): like mode 1 but a corner only fills when the neighbours
    /// are solid *and* hold the same material as the pixel.
    pub fn edge_mode2(&self, u: f64, v: f64) -> f32 {
        let (u, v) = (u + 0.5, v + 0.5);
        let (iu, iv) = (floor_i32(u), floor_i32(v));
        let f = |x, y| self.values.get(x, y);
        let m = |x, y| self.materials.get(x, y);
        if f(iu, iv) > 0.5 {
            return 1.0;
        }
        let fu = u - f64::from(iu);
        let fv = v - f64::from(iv);
        let m0 = m(iu, iv);
        let (dx, dy) = if fu < fv { (-1, 1) } else { (1, -1) };
        let (x1, y1) = (iu + dx, iv + dy);
        if f(x1, iv) >= 0.5 && f(iu, y1) >= 0.5 && m(x1, iv) == m0 && m(iu, y1) == m0 {
            return 1.0;
        }
        let d = if 1.0 - fu < fv { 1 } else { -1 };
        let (x1, y1) = (iu + d, iv + d);
        if f(x1, iv) >= 0.5 && f(iu, y1) >= 0.5 && m(x1, iv) == m0 && m(iu, y1) == m0 {
            1.0
        } else {
            0.0
        }
    }
}

/// `0x704f00`: material table index of a Wang pixel color, by the `wang_color` of each material.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColorTable {
    colors: Vec<u32>,
}

impl ColorTable {
    /// `colors[i]` is the `wang_color` of material `i` as `0xRRGGBB` (alpha ignored); index 0
    /// is air.
    pub fn new(colors: Vec<u32>) -> Self {
        Self { colors }
    }

    /// Index 0 when `rgb` is the air color, else the first material from index 1 with that
    /// color, else -1.
    pub fn index_of(&self, rgb: u32) -> i32 {
        let rgb = rgb & 0xff_ffff;
        if self.colors.first().is_some_and(|c| c & 0xff_ffff == rgb) {
            return 0;
        }
        for (i, c) in self.colors.iter().enumerate().skip(1) {
            if c & 0xff_ffff == rgb {
                return i as i32;
            }
        }
        -1
    }
}

/// The last pass of `0x8704c0`, chosen by its fourth argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillMode {
    /// Pixels with neither material nor float take the majority material of their neighbours.
    Neighbors,
    /// Only spawn-function pixels whose neighbours are mostly air-colored become air.
    SpawnToAir,
}

/// The result of converting a generated Wang image (`0x8704c0`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Converted {
    pub map: WangMap,
    /// Spawn function index + 1 per pixel (0 = none).
    pub spawn: Image<u8>,
}

/// `0x92a260`: the most frequent non-zero value of four, the first to reach the top count.
fn majority(v: [u16; 4]) -> u16 {
    let mut best = 0;
    let mut best_count = 0;
    let mut counts: Vec<(u16, u32)> = Vec::new();
    for x in v {
        if x == 0 {
            continue;
        }
        let n = match counts.iter_mut().find(|(k, _)| *k == x) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                counts.push((x, 1));
                1
            }
        };
        if n > best_count {
            best = x;
            best_count = n;
        }
    }
    best
}

impl WangMap {
    /// `0x8704c0` as `0x879bf0` calls it: turn a generated `width` x `height` RGB image into the
    /// float, material and spawn images.
    ///
    /// * black pixels stay 0;
    /// * a color that is a material's `wang_color` stores `index + 1` and a float of 1 (air
    ///   color: -1);
    /// * another color that `spawn_of` knows (an index below 255) stores `index + 1` in the
    ///   spawn image and leaves the float at 0;
    /// * everything else, white included, stores its gray `(g + 2 b) / 3`.
    ///
    /// Pixels with neither a material nor a float then take the majority material of their left,
    /// right, upper and lower neighbours.
    pub fn from_wang_rgb(
        rgb: &[u8],
        width: usize,
        height: usize,
        colors: &ColorTable,
        spawn_of: impl Fn(u32) -> Option<u32>,
    ) -> Converted {
        Self::from_wang_rgb_with(rgb, width, height, colors, spawn_of, FillMode::Neighbors)
    }

    /// `from_wang_rgb` with the choice of the final pass (the call flag of `0x8704c0`).
    pub fn from_wang_rgb_with(
        rgb: &[u8],
        width: usize,
        height: usize,
        colors: &ColorTable,
        spawn_of: impl Fn(u32) -> Option<u32>,
        mode: FillMode,
    ) -> Converted {
        let n = width * height;
        let mut values = vec![0.0_f32; n];
        let mut materials = vec![0_u16; n];
        let mut spawn = vec![0_u8; n];
        for (i, p) in rgb.as_chunks::<3>().0.iter().take(n).enumerate() {
            let c = u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
            if c == 0 {
                continue;
            }
            let gray = || {
                let b = (f32::from(p[2]) + 0.0) * 0.003_921_569;
                (((f32::from(p[1]) + 0.0) * 0.003_921_569) + b + b) / 3.0
            };
            if c == 0xff_ffff {
                values[i] = gray();
                continue;
            }
            let m = colors.index_of(c);
            if m != -1 {
                if m > 0xfffe {
                    values[i] = gray();
                } else if m == 0 {
                    values[i] = -1.0;
                    materials[i] = 1;
                } else {
                    values[i] = 1.0;
                    materials[i] = (m + 1) as u16;
                }
                continue;
            }
            match spawn_of(c) {
                Some(s) if s < 0xff => spawn[i] = (s + 1) as u8,
                _ => values[i] = gray(),
            }
        }
        let (w, h) = (width as i32, height as i32);
        if mode == FillMode::SpawnToAir {
            let mut img = Image::new(w, h, materials);
            let air_pixels = img.data.iter().filter(|&&m| m == 1).count();
            if spawn.iter().any(|&s| s != 0) && air_pixels > 0 {
                for y in 0..h {
                    for x in 0..w {
                        let i = (y * w + x) as usize;
                        if spawn[i] != 0 && img.data[i] == 0 {
                            let m = majority([
                                img.get(x - 1, y),
                                img.get(x + 1, y),
                                img.get(x, y - 1),
                                img.get(x, y + 1),
                            ]);
                            if m == 1 {
                                img.data[i] = 1;
                                values[i] = -1.0;
                            }
                        }
                    }
                }
            }
            return Converted {
                map: WangMap {
                    values: Image::new(w, h, values),
                    materials: img,
                },
                spawn: Image::new(w, h, spawn),
            };
        }
        let old = Image::new(w, h, materials);
        let mut dilated = old.data.clone();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if old.data[i] == 0 && values[i] == 0.0 {
                    let m = majority([
                        old.get(x - 1, y),
                        old.get(x + 1, y),
                        old.get(x, y - 1),
                        old.get(x, y + 1),
                    ]);
                    if m != 0 {
                        dilated[i] = m;
                    }
                }
            }
        }
        Converted {
            map: WangMap {
                values: Image::new(w, h, values),
                materials: Image::new(w, h, dilated),
            },
            spawn: Image::new(w, h, spawn),
        }
    }
}

/// `0x87d010`: the base value for rule lookups, `base` plus a bump shaped by value and simplex
/// noise; at least 0.5.
pub fn terrain_value(x: f64, y: f64, base: f32) -> f32 {
    let a = value2((x * 0.035) as f32, (y * 0.07) as f32);
    let t = a * 15.5;
    let w = (base - 0.5) * 0.5;
    const K: f32 = f32::from_bits(0x3d48_6834);
    let n = simplex2((x as f32 + t) * K, (y as f32 + t) * K);
    let k = w * w * 5.35 * 0.95;
    let r = n * k + base;
    if 0.5 > r { 0.5 } else { r }
}

/// The per-material numbers the Wang lookup reads from `CellData`: `wang_noise_percent`
/// (`+0x234`, default 1), `wang_curvature` (`+0x238`, default 0.5), `wang_noise_type`
/// (`+0x23c`, default 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WangParams {
    pub noise_percent: f32,
    pub curvature: f32,
    pub noise_type: i32,
}

impl Default for WangParams {
    fn default() -> Self {
        Self {
            noise_percent: 1.0,
            curvature: 0.5,
            noise_type: 0,
        }
    }
}

/// A Wang biome (`0x87d0e0`, biome type 2) as the lookup sees it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WangBiome {
    pub map: WangMap,
    pub rules: RuleList,
    /// The globals `0x1206fb0` / `0x12051b0`.
    pub offset: (f64, f64),
    /// The global world seed `0x1205004`.
    pub world_seed: i32,
}

impl WangBiome {
    /// The material table index at world position `(x, y)`; 0 means none (air). `params` gives
    /// the Wang numbers of a table index.
    pub fn material_at(&self, x: f64, y: f64, params: impl Fn(u32) -> WangParams) -> u32 {
        lookup(
            &self.map,
            &self.rules,
            self.offset,
            self.world_seed,
            (x, y),
            params,
        )
    }
}

/// `WangBiome::material_at` on borrowed parts, so biomes can share one image.
pub fn lookup(
    map: &WangMap,
    rules: &RuleList,
    offset: (f64, f64),
    world_seed: i32,
    (x, y): (f64, f64),
    params: impl Fn(u32) -> WangParams,
) -> u32 {
    {
        let (u0, v0) = world_to_wang(offset, 0.0, x, y);
        let idx = map.round_index(f64::from(u0), f64::from(v0));
        let mut threshold = 0.5_f32;
        let (u, v, value);
        if idx > 0 {
            let p = params(idx as u32);
            (u, v) = world_to_wang(offset, p.noise_percent, x, y);
            threshold = p.curvature;
            let (uf, vf) = (f64::from(u), f64::from(v));
            value = match p.noise_type {
                1 => map.edge_mode1(uf, vf),
                2 => map.edge_mode2(uf, vf),
                _ => map.smooth(uf, vf),
            };
        } else {
            (u, v) = world_to_wang(offset, 1.0, x, y);
            value = map.smooth(f64::from(u), f64::from(v));
        }
        if value < threshold {
            return 0;
        }
        let idx = map.round_index(f64::from(u), f64::from(v));
        if idx > 0 {
            return idx as u32;
        }
        let base = terrain_value(x, y, value);
        rules.pick(x.round() as i32, y.round() as i32, base, world_seed)
    }
}

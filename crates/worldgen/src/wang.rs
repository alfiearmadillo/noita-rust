//! Herringbone Wang tile maps. The game compiles Sean Barrett's public-domain
//! `stb_herringbone_wang_tile.h` with three changes that matter here: the generator's scratch
//! arrays live in the tileset instead of statics, `STB_HBWANG_RAND()` is a per-thread
//! `Random` (`0x866120`) seeded from a double, and map sizes are limited to 100x100 tiles
//! (`STB_HBWANG_MAX_X/Y`).
//!
//! Ported from `stbhw_generate_image` (`0x866b90`), `stbhw_build_tileset_from_image`
//! (`0x867250`, count helper `0x8663b0`), `stbhw__choose_tile` (`0x866970`),
//! `stbhw__change_color` (`0x866a80`), the tile blitters (`0x866830`, `0x8668d0`) and the wrapper
//! that generates a biome's map (`0x867770`: seed, generate `height + 4` rows, drop the first 4).

use sim::random::Random;

pub const MAX_X: usize = 100;
pub const MAX_Y: usize = 100;
/// Scratch array pitch of `c_color` and `h_color` (`STB_HBWANG_MAX_X + 6`).
const WIDE: usize = MAX_X + 6;
/// Pitch of `v_color` (`STB_HBWANG_MAX_X + 5`).
const NARROW: usize = MAX_X + 5;
const ROWS: usize = MAX_Y + 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WangError {
    /// The template image does not carry a valid header or its tile counts are unusable.
    BadTemplate,
    /// The template image is smaller than its header says.
    ImageTooSmall,
    /// The requested map needs more than `MAX_X` x `MAX_Y` tiles ("increase STB_HBWANG_MAX_X/Y").
    MapTooLarge,
    /// No tile satisfies the constraints around a slot.
    NoMatchingTile,
}

/// `STB_HBWANG_RAND()` (`0x866120`): `(int)(next * 4.656612875e-10 * 2147483645)` from a
/// `Random` built with `Random::Random(double seed)` (`0x44d070`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WangRng(Random);

impl WangRng {
    pub fn new(seed: f64) -> Self {
        Self(Random::from_seed(seed))
    }

    pub fn rand(&mut self) -> u32 {
        // CRT double-to-int64 conversion; the value is below 2^31.
        (self.0.next_unit() * 2_147_483_645.0) as i64 as u32
    }
}

/// One herringbone tile: six edge or corner colors plus its RGB pixels (`2n x n` for the
/// horizontal list, `n x 2n` for the vertical one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    /// `a..f` of the diagrams in the stb header.
    pub colors: [i8; 6],
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tileset {
    pub is_corner: bool,
    pub num_color: [i32; 6],
    pub short_side_len: usize,
    pub h_tiles: Vec<Tile>,
    pub v_tiles: Vec<Tile>,
}

struct Config {
    is_corner: bool,
    short: i32,
    num_color: [i32; 6],
    vary_x: i32,
    vary_y: i32,
}

struct TemplateInfo {
    size_x: i64,
    size_y: i64,
    h_count: i64,
    v_count: i64,
}

impl Config {
    fn info(&self) -> TemplateInfo {
        let nc = self.num_color.map(i64::from);
        let (vx, vy, s) = (
            i64::from(self.vary_x),
            i64::from(self.vary_y),
            i64::from(self.short),
        );
        let (horz_w, horz_h, vert_w, vert_h) = if self.is_corner {
            (
                nc[1] * nc[2] * nc[3] * vx,
                nc[0] * nc[1] * nc[2] * vy,
                nc[0] * nc[3] * nc[2] * vy,
                nc[1] * nc[0] * nc[3] * vx,
            )
        } else {
            (
                nc[0] * nc[1] * nc[2] * vx,
                nc[3] * nc[4] * nc[2] * vy,
                nc[0] * nc[5] * nc[1] * vy,
                nc[3] * nc[4] * nc[5] * vx,
            )
        };
        let horz_x = horz_w * (2 * s + 3);
        let horz_y = horz_h * (s + 3);
        let vert_x = vert_w * (s + 3);
        let vert_y = vert_h * (2 * s + 3);
        TemplateInfo {
            size_x: horz_x.max(vert_x),
            size_y: 2 + horz_y + 2 + vert_y,
            h_count: horz_w * horz_h,
            v_count: vert_w * vert_h,
        }
    }
}

type Range = (i32, i32);

/// `stbhw__process_h_row` / `stbhw__process_v_row`.
fn process_row(
    horizontal: bool,
    c: &Config,
    ypos: i32,
    r: [Range; 6],
    variants: i32,
    emit: &mut impl FnMut(bool, i32, i32, [i32; 6]),
) {
    let step = if horizontal {
        2 * c.short + 3
    } else {
        c.short + 3
    };
    let mut xpos = 0;
    for _ in 0..variants {
        for f in r[5].0..=r[5].1 {
            for e in r[4].0..=r[4].1 {
                for d in r[3].0..=r[3].1 {
                    for cc in r[2].0..=r[2].1 {
                        for b in r[1].0..=r[1].1 {
                            for a in r[0].0..=r[0].1 {
                                emit(horizontal, xpos, ypos, [a, b, cc, d, e, f]);
                                xpos += step;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// `stbhw__process_template`: visits every tile slot of the template in order.
fn process_template(c: &Config, emit: &mut impl FnMut(bool, i32, i32, [i32; 6])) {
    let nc = c.num_color;
    let all = |n: i32| (0, n - 1);
    let one = |v: i32| (v, v);
    let mut ypos = 2;
    if c.is_corner {
        for k in 0..nc[2] {
            for j in 0..nc[1] {
                for i in 0..nc[0] {
                    for _ in 0..c.vary_y {
                        let r = [all(nc[1]), all(nc[2]), all(nc[3]), one(i), one(j), one(k)];
                        process_row(true, c, ypos, r, c.vary_x, emit);
                        ypos += c.short + 3;
                    }
                }
            }
        }
        ypos += 2;
        for k in 0..nc[3] {
            for j in 0..nc[0] {
                for i in 0..nc[1] {
                    for _ in 0..c.vary_x {
                        let r = [all(nc[0]), all(nc[3]), all(nc[2]), one(i), one(j), one(k)];
                        process_row(false, c, ypos, r, c.vary_y, emit);
                        ypos += c.short * 2 + 3;
                    }
                }
            }
        }
    } else {
        for k in 0..nc[3] {
            for j in 0..nc[4] {
                for i in 0..nc[2] {
                    for _ in 0..c.vary_y {
                        let r = [all(nc[2]), one(k), all(nc[1]), one(j), all(nc[0]), one(i)];
                        process_row(true, c, ypos, r, c.vary_x, emit);
                        ypos += c.short + 3;
                    }
                }
            }
        }
        ypos += 2;
        for k in 0..nc[3] {
            for j in 0..nc[4] {
                for i in 0..nc[5] {
                    for _ in 0..c.vary_x {
                        let r = [all(nc[0]), one(i), all(nc[1]), one(j), all(nc[5]), one(k)];
                        process_row(false, c, ypos, r, c.vary_y, emit);
                        ypos += c.short * 2 + 3;
                    }
                }
            }
        }
    }
}

impl Tileset {
    /// `stbhw_build_tileset_from_image`: `rgb` is `w * h` three-byte pixels. The configuration is
    /// stored in the last nine bytes of the first row (read backwards, each XORed with `i * 55`).
    pub fn from_image(rgb: &[u8], w: usize, h: usize) -> Result<Self, WangError> {
        if w == 0 || rgb.len() < w * h * 3 {
            return Err(WangError::ImageTooSmall);
        }
        let mut header = [0u8; 9];
        for (i, byte) in header.iter_mut().enumerate() {
            *byte = rgb[w * 3 - 1 - i] ^ (i as u8).wrapping_mul(55);
        }
        let h8 = header.map(i32::from);
        let c = if header[7] == 0xC0 {
            Config {
                is_corner: true,
                num_color: [h8[0], h8[1], h8[2], h8[3], 0, 0],
                vary_x: h8[4],
                vary_y: h8[5],
                short: h8[6],
            }
        } else {
            Config {
                is_corner: false,
                num_color: [h8[0], h8[1], h8[2], h8[3], h8[4], h8[5]],
                vary_x: h8[6],
                vary_y: h8[7],
                short: h8[8],
            }
        };
        if c.vary_x > 64
            || c.vary_y > 64
            || c.short == 0
            || c.num_color[..4].iter().any(|&n| n > 32)
        {
            return Err(WangError::BadTemplate);
        }
        let used = if c.is_corner { 4 } else { 6 };
        if c.num_color[..used].contains(&0) || c.vary_x == 0 || c.vary_y == 0 {
            return Err(WangError::BadTemplate);
        }
        let info = c.info();
        if info.h_count > 1 << 22 || info.v_count > 1 << 22 {
            return Err(WangError::BadTemplate);
        }
        if (w as i64) < info.size_x || (h as i64) < info.size_y {
            return Err(WangError::ImageTooSmall);
        }

        let len = c.short as usize;
        let mut h_tiles = Vec::with_capacity(info.h_count as usize);
        let mut v_tiles = Vec::with_capacity(info.v_count as usize);
        let stride = w * 3;
        process_template(&c, &mut |horizontal, xpos, ypos, abcdef| {
            let (tw, th) = if horizontal {
                (len * 2, len)
            } else {
                (len, len * 2)
            };
            let (x0, y0) = (xpos as usize + 1, ypos as usize + 1);
            let mut pixels = Vec::with_capacity(tw * th * 3);
            for j in 0..th {
                let start = (y0 + j) * stride + x0 * 3;
                pixels.extend_from_slice(&rgb[start..start + tw * 3]);
            }
            let tile = Tile {
                colors: abcdef.map(|v| v as i8),
                pixels,
            };
            if horizontal {
                h_tiles.push(tile);
            } else {
                v_tiles.push(tile);
            }
        });
        Ok(Self {
            is_corner: c.is_corner,
            num_color: c.num_color,
            short_side_len: len,
            h_tiles,
            v_tiles,
        })
    }

    /// `stbhw_generate_image` without weights: an RGB map of `w * h` pixels.
    pub fn generate(&self, rng: &mut WangRng, w: usize, h: usize) -> Result<Vec<u8>, WangError> {
        let side = self.short_side_len as i32;
        let (wi, hi) = (w as i32, h as i32);
        let xmax = wi / side + 6;
        let ymax = hi / side + 6;
        if xmax as usize > WIDE || ymax as usize > ROWS {
            return Err(WangError::MapTooLarge);
        }
        let mut out = vec![0u8; w * h * 3];
        let mut grid = Grid::new();
        if self.is_corner {
            self.generate_corner(rng, &mut grid, &mut out, (wi, hi), (xmax, ymax))?;
        } else {
            self.generate_edge(rng, &mut grid, &mut out, (wi, hi))?;
        }
        Ok(out)
    }

    fn generate_corner(
        &self,
        rng: &mut WangRng,
        g: &mut Grid,
        out: &mut [u8],
        (w, h): (i32, i32),
        (xmax, ymax): (i32, i32),
    ) -> Result<(), WangError> {
        let side = self.short_side_len as i32;
        let cc = self.num_color;
        for j in 0..ymax {
            for i in 0..xmax {
                let p = ((i - j + 1) & 3) as usize;
                g.c[idx(j, i, WIDE)] = (rng.rand() % cc[p] as u32) as i8;
            }
        }
        // Remove 3x2 and 2x3 blocks of identical vertices.
        let same = |g: &Grid, x: i32, y: i32| g.c[idx(y, x, WIDE)] == g.c[idx(y + 1, x + 1, WIDE)];
        for j in 0..ymax - 3 {
            for i in 0..xmax - 3 {
                if same(g, i, j)
                    && same(g, i, j + 1)
                    && same(g, i, j + 2)
                    && same(g, i + 1, j)
                    && same(g, i + 1, j + 1)
                    && same(g, i + 1, j + 2)
                {
                    let p = ((i + 1) - (j + 1) + 1) & 3;
                    if cc[p as usize] > 1 {
                        let k = idx(j + 1, i + 1, WIDE);
                        g.c[k] = change_color(rng, i32::from(g.c[k]), cc[p as usize]);
                    }
                }
                if same(g, i, j)
                    && same(g, i + 1, j)
                    && same(g, i + 2, j)
                    && same(g, i, j + 1)
                    && same(g, i + 1, j + 1)
                    && same(g, i + 2, j + 1)
                {
                    let p = ((i + 2) - (j + 1) + 1) & 3;
                    if cc[p as usize] > 1 {
                        let k = idx(j + 1, i + 2, WIDE);
                        g.c[k] = change_color(rng, i32::from(g.c[k]), cc[p as usize]);
                    }
                }
            }
        }

        let mut ypos = -side;
        let mut j = -1;
        while ypos < h {
            let phase = j & 3;
            let mut i = if phase == 0 { 0 } else { phase - 4 };
            loop {
                let mut xpos = i * side;
                if xpos >= w {
                    break;
                }
                if xpos + side * 2 >= 0 && ypos >= 0 {
                    let at = [
                        (j + 2, i + 2),
                        (j + 2, i + 3),
                        (j + 2, i + 4),
                        (j + 3, i + 2),
                        (j + 3, i + 3),
                        (j + 3, i + 4),
                    ];
                    let t = choose_tile(&self.h_tiles, rng, &mut g.c, WIDE, at)?;
                    draw_tile(
                        out,
                        (w, h),
                        (xpos, ypos),
                        &self.h_tiles[t],
                        (side * 2, side),
                    );
                }
                xpos += side * 3;
                if xpos < w {
                    let at = [
                        (j + 2, i + 5),
                        (j + 3, i + 5),
                        (j + 4, i + 5),
                        (j + 2, i + 6),
                        (j + 3, i + 6),
                        (j + 4, i + 6),
                    ];
                    let t = choose_tile(&self.v_tiles, rng, &mut g.c, WIDE, at)?;
                    draw_tile(
                        out,
                        (w, h),
                        (xpos, ypos),
                        &self.v_tiles[t],
                        (side, side * 2),
                    );
                }
                i += 4;
            }
            ypos += side;
            j += 1;
        }
        Ok(())
    }

    fn generate_edge(
        &self,
        rng: &mut WangRng,
        g: &mut Grid,
        out: &mut [u8],
        (w, h): (i32, i32),
    ) -> Result<(), WangError> {
        let side = self.short_side_len as i32;
        let mut ypos = -side;
        let mut j = -1;
        while ypos < h {
            let phase = j & 3;
            let mut i = if phase == 0 { 0 } else { phase - 4 };
            loop {
                let mut xpos = i * side;
                if xpos >= w {
                    break;
                }
                if xpos + side * 2 >= 0 && ypos >= 0 {
                    let mut cons = [
                        g.h[idx(j + 2, i + 2, WIDE)],
                        g.h[idx(j + 2, i + 3, WIDE)],
                        g.v[idx(j + 2, i + 2, NARROW)],
                        g.v[idx(j + 2, i + 4, NARROW)],
                        g.h[idx(j + 3, i + 2, WIDE)],
                        g.h[idx(j + 3, i + 3, WIDE)],
                    ];
                    let t = pick(&self.h_tiles, rng, &mut cons)?;
                    g.h[idx(j + 2, i + 2, WIDE)] = cons[0];
                    g.h[idx(j + 2, i + 3, WIDE)] = cons[1];
                    g.v[idx(j + 2, i + 2, NARROW)] = cons[2];
                    g.v[idx(j + 2, i + 4, NARROW)] = cons[3];
                    g.h[idx(j + 3, i + 2, WIDE)] = cons[4];
                    g.h[idx(j + 3, i + 3, WIDE)] = cons[5];
                    draw_tile(
                        out,
                        (w, h),
                        (xpos, ypos),
                        &self.h_tiles[t],
                        (side * 2, side),
                    );
                }
                xpos += side * 3;
                if xpos < w {
                    let mut cons = [
                        g.h[idx(j + 2, i + 5, WIDE)],
                        g.v[idx(j + 2, i + 5, NARROW)],
                        g.v[idx(j + 2, i + 6, NARROW)],
                        g.v[idx(j + 3, i + 5, NARROW)],
                        g.v[idx(j + 3, i + 6, NARROW)],
                        g.h[idx(j + 4, i + 5, WIDE)],
                    ];
                    let t = pick(&self.v_tiles, rng, &mut cons)?;
                    g.h[idx(j + 2, i + 5, WIDE)] = cons[0];
                    g.v[idx(j + 2, i + 5, NARROW)] = cons[1];
                    g.v[idx(j + 2, i + 6, NARROW)] = cons[2];
                    g.v[idx(j + 3, i + 5, NARROW)] = cons[3];
                    g.v[idx(j + 3, i + 6, NARROW)] = cons[4];
                    g.h[idx(j + 4, i + 5, WIDE)] = cons[5];
                    draw_tile(
                        out,
                        (w, h),
                        (xpos, ypos),
                        &self.v_tiles[t],
                        (side, side * 2),
                    );
                }
                i += 4;
            }
            ypos += side;
            j += 1;
        }
        Ok(())
    }

    /// The biome map wrapper (`0x867770`): seeds a fresh generator, generates `h + 4` rows and
    /// drops the first four, so the visible map starts four rows into the herringbone pattern.
    pub fn generate_map(&self, seed: f64, w: usize, h: usize) -> Result<Vec<u8>, WangError> {
        let mut rng = WangRng::new(seed);
        let full = self.generate(&mut rng, w, h + 4)?;
        Ok(full[4 * w * 3..].to_vec())
    }
}

fn idx(row: i32, col: i32, pitch: usize) -> usize {
    row as usize * pitch + col as usize
}

/// The generator's scratch arrays (`c_color`, `v_color`, `h_color`); -1 means unconstrained.
struct Grid {
    c: Vec<i8>,
    v: Vec<i8>,
    h: Vec<i8>,
}

impl Grid {
    fn new() -> Self {
        Self {
            c: vec![-1; ROWS * WIDE],
            v: vec![-1; ROWS * NARROW],
            h: vec![-1; (MAX_Y + 5) * WIDE],
        }
    }
}

/// `stbhw__change_color` without weights.
fn change_color(rng: &mut WangRng, old: i32, options: i32) -> i8 {
    let offset = 1 + rng.rand() % (options - 1) as u32;
    ((old + offset as i32) % options) as i8
}

/// `stbhw__choose_tile` for the corner map: constraints are six cells of one array.
fn choose_tile(
    list: &[Tile],
    rng: &mut WangRng,
    cells: &mut [i8],
    pitch: usize,
    at: [(i32, i32); 6],
) -> Result<usize, WangError> {
    let mut cons = at.map(|(r, c)| cells[idx(r, c, pitch)]);
    let t = pick(list, rng, &mut cons)?;
    for (k, &(r, c)) in at.iter().enumerate() {
        cells[idx(r, c, pitch)] = cons[k];
    }
    Ok(t)
}

/// Two passes over the tiles that match the constraints: count them, then stop at a random one.
/// The chosen tile's colors replace the constraints.
fn pick(list: &[Tile], rng: &mut WangRng, cons: &mut [i8; 6]) -> Result<usize, WangError> {
    let mut m: i64 = 1 << 30;
    for _ in 0..2 {
        let mut n: i64 = 0;
        for (i, tile) in list.iter().enumerate() {
            let fits = cons
                .iter()
                .zip(&tile.colors)
                .all(|(&want, &have)| want < 0 || want == have);
            if fits {
                n += 1;
                if n > m {
                    *cons = tile.colors;
                    return Ok(i);
                }
            }
        }
        if n == 0 {
            return Err(WangError::NoMatchingTile);
        }
        m = i64::from(rng.rand() % n as u32);
    }
    unreachable!("the second pass always selects a tile")
}

/// `stbhw__draw_h_tile` / `stbhw__draw_v_tile`: blit with clipping to the map.
fn draw_tile(
    out: &mut [u8],
    (w, h): (i32, i32),
    (x, y): (i32, i32),
    tile: &Tile,
    (tw, th): (i32, i32),
) {
    for j in 0..th {
        if y + j < 0 || y + j >= h {
            continue;
        }
        for i in 0..tw {
            if x + i < 0 || x + i >= w {
                continue;
            }
            let src = ((j * tw + i) * 3) as usize;
            let dst = (((y + j) * w + x + i) * 3) as usize;
            out[dst..dst + 3].copy_from_slice(&tile.pixels[src..src + 3]);
        }
    }
}

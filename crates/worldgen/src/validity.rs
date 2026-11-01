//! The validity check the region driver (`0x879bf0`) runs on every generated Wang region image of at
//! most 205 pixels in height (`0x8771a0`): the start area is cleared, a path of open pixels must
//! lead from it to the bottom row, and the `0xc0ffee` blobs are turned into open or solid pixels.
//! A region without such a path is generated again with the next seed.
//!
//! Pixels are the engine's words, `0xAABBGGRR`. Not ported: the per-spawn-function limits (extra
//! spawn pixels are blanked, which leaves the float and material images unchanged), the second
//! flood fill that tags reachable open pixels (alpha `0xfe`, only used for the flag image) and the
//! structure rectangles of coalmine and excavationsite (`0x876fb0`).

use std::collections::VecDeque;

use sim::random::Random;

/// `0xAABBGGRR` of the "open or solid at random" blob color `0xc0ffee`.
const BLOB: u32 = 0xffee_ffc0;
const BLACK: u32 = 0xff00_0000;
const WHITE: u32 = 0xffff_ffff;
/// `DAT_011525d4` / `DAT_0115303c`: the world x range of the start area.
const START_X: (i32, i32) = (159, 223);

/// A picture of pixel words.
#[derive(Clone, Debug, PartialEq)]
pub struct Pixels {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u32>,
}

impl Pixels {
    /// From packed RGB bytes (opaque).
    pub fn from_rgb(rgb: &[u8], width: usize, height: usize) -> Self {
        let data = rgb
            .as_chunks::<3>()
            .0
            .iter()
            .take(width * height)
            .map(|p| 0xff00_0000 | u32::from(p[2]) << 16 | u32::from(p[1]) << 8 | u32::from(p[0]))
            .collect();
        Self {
            width,
            height,
            data,
        }
    }

    /// From RGBA bytes.
    pub fn from_rgba(rgba: &[u8], width: usize, height: usize) -> Self {
        let data = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .take(width * height)
            .map(|p| {
                u32::from(p[3]) << 24
                    | u32::from(p[2]) << 16
                    | u32::from(p[1]) << 8
                    | u32::from(p[0])
            })
            .collect();
        Self {
            width,
            height,
            data,
        }
    }

    /// Back to packed RGB bytes.
    pub fn to_rgb(&self) -> Vec<u8> {
        self.data
            .iter()
            .flat_map(|w| [*w as u8, (*w >> 8) as u8, (*w >> 16) as u8])
            .collect()
    }

    fn at(&self, x: usize, y: usize) -> u32 {
        self.data[y * self.width + x]
    }
}

/// What the check needs besides the image.
pub struct Params<'a> {
    /// The region's template file (`data/wang_tiles/coalmine.png` is special).
    pub template: &'a str,
    /// World x of the region's first pixel (`biome map x * 512 - offset x`).
    pub origin_x: i32,
    pub world_seed: i32,
    /// `data/wang_tiles/extra_layers/coalmine.png`, only for the coalmine template.
    pub extra: Option<&'a Pixels>,
}

fn passable(c: u32) -> bool {
    c & 0xff_ffff == 0 || c == BLOB
}

/// `0x876980`: the runs `(x, y, length)` of open pixels along row `y`.
fn runs(img: &Pixels, y: usize) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    let mut x = 0;
    while x < img.width {
        let len = (x..img.width)
            .take_while(|&i| passable(img.at(i, y)))
            .count();
        if len != 0 {
            out.push((x, y, len));
            x += len;
        }
        x += 1;
    }
    out
}

/// `0x876680`: breadth first search over open pixels (down, left, right, up); the result holds,
/// for every reached pixel, the step that leads back (2 up, 3 down, 4 left, 5 right).
fn search(img: &Pixels, start: (usize, usize)) -> Vec<u8> {
    let (w, h) = (img.width, img.height);
    let mut grid = vec![0_u8; w * h];
    let mut queue = vec![start];
    let mut i = 0;
    while i < queue.len() {
        let (x, y) = queue[i];
        i += 1;
        let mut visit = |nx: usize, ny: usize, code: u8, queue: &mut Vec<(usize, usize)>| {
            if passable(img.at(nx, ny)) && grid[ny * w + nx] == 0 {
                queue.push((nx, ny));
                grid[ny * w + nx] = code;
            }
        };
        if y + 1 < h {
            visit(x, y + 1, 2, &mut queue);
        }
        if x > 0 {
            visit(x - 1, y, 5, &mut queue);
        }
        if x + 1 < w {
            visit(x + 1, y, 4, &mut queue);
        }
        if y > 0 {
            visit(x, y - 1, 3, &mut queue);
        }
    }
    grid
}

/// `0x876aa0`: recolors the 4-connected pixels of the start pixel's color and flags them.
fn fill(img: &mut Pixels, flags: &mut [bool], start: (usize, usize), color: u32) {
    let (w, h) = (img.width, img.height);
    let reference = img.at(start.0, start.1);
    let mut queue = VecDeque::from([start]);
    flags[start.1 * w + start.0] = true;
    while let Some((x, y)) = queue.pop_front() {
        img.data[y * w + x] = color;
        let mut take = |nx: usize, ny: usize, img: &Pixels| {
            if img.at(nx, ny) == reference && !flags[ny * w + nx] {
                flags[ny * w + nx] = true;
                queue.push_back((nx, ny));
            }
        };
        if x > 0 {
            take(x - 1, y, img);
        }
        if y > 0 {
            take(x, y - 1, img);
        }
        if x + 1 < w {
            take(x + 1, y, img);
        }
        if y + 1 < h {
            take(x, y + 1, img);
        }
    }
}

/// `(int)f` as the CPU does it: out of range gives `i32::MIN`.
fn trunc(f: f32) -> i32 {
    if f >= 2_147_483_648.0 || f.is_nan() {
        i32::MIN
    } else {
        f as i32
    }
}

fn step(s: i32) -> i32 {
    let v = s
        .wrapping_mul(0x41a7)
        .wrapping_add((s / 0x1f31d).wrapping_mul(-0x7fff_ffff));
    if v < 1 {
        v.wrapping_add(0x7fff_ffff)
    } else {
        v
    }
}

/// `0x876ce0`: every blob left becomes white (probability one half) or black, in raster order,
/// drawing from a generator seeded by the world seed that keeps its state as a float.
fn randomize_blobs(img: &mut Pixels, flags: &mut [bool], world_seed: i32) {
    let mut d = f64::from(world_seed);
    if world_seed < 0 {
        d += 4_294_967_296.0;
    }
    if d >= 2_147_483_647.0 {
        d *= 0.5;
    }
    let mut f = step(d as i32) as f32;
    for y in 0..img.height {
        for x in 0..img.width {
            if !flags[y * img.width + x] && img.at(x, y) == BLOB {
                f = step(trunc(f)) as f32;
                let color = if f * 4.656_613e-10_f32 + 0.0 < 0.5 {
                    WHITE
                } else {
                    BLACK
                };
                fill(img, flags, (x, y), color);
            }
        }
    }
}

/// The `<RandomColor>` pass of the region driver (after `validate`): in raster order, a pixel whose
/// word is a key of `table` recolors its whole 4-connected blob with one of the key's colors,
/// chosen with a draw of the region generator (`rng`: the chain that produced the Wang seeds).
pub fn random_colors(img: &mut Pixels, table: &[(u32, Vec<u32>)], rng: &mut Random) {
    let mut flags = vec![false; img.width * img.height];
    for y in 0..img.height {
        for x in 0..img.width {
            let c = img.at(x, y);
            if c & 0xff_ffff == 0 {
                continue;
            }
            let Some((_, outputs)) = table.iter().rev().find(|(k, o)| *k == c && !o.is_empty())
            else {
                continue;
            };
            let pick = (rng.next_unit() * outputs.len() as f64) as usize;
            fill(img, &mut flags, (x, y), outputs[pick]);
        }
    }
}

/// `0x8771a0` as far as it shapes the image and decides success; returns whether the region is
/// valid (the image is only meaningful then).
pub fn validate(img: &mut Pixels, p: &Params) -> bool {
    let (w, h) = (img.width, img.height);
    let top = runs(img, 0);
    let bottom = runs(img, h - 1);
    let coalmine = p.template == "data/wang_tiles/coalmine.png";
    // The start entries (x, y, length) and the changes to the image around them. A start area
    // outside the region leaves every open run of the top row as a start.
    let starts = if coalmine {
        if let Some(extra) = p.extra {
            for y in 0..extra.height.min(h) {
                for x in 0..extra.width.min(w) {
                    let c = extra.at(x, y);
                    if c == 0xff42_0000 {
                        img.data[y * w + x] = BLACK;
                    } else if c >> 24 != 0 && c != BLACK {
                        img.data[y * w + x] = c;
                    }
                }
            }
        }
        vec![(0x8e, 0, 0xc)]
    } else {
        let x0 = (START_X.0 - p.origin_x) / 10;
        let len = (START_X.1 - START_X.0) / 10;
        if x0 < 0 || x0 as usize >= w || h < 1 || x0 + len < 0 || (x0 + len) as usize >= w || h < 7
        {
            top
        } else {
            for y in 0..=6 {
                for x in x0..=x0 + len {
                    img.data[y * w + x as usize] = BLACK;
                }
            }
            vec![(x0 as usize, 0, 0)]
        }
    };
    if coalmine {
        for y in 0..15.min(h) {
            for x in 0x8c..0x9e.min(w) {
                if img.data[y * w + x] == 0xfffe_fefe {
                    img.data[y * w + x] = BLACK;
                }
            }
        }
    }
    let mut found = None;
    for &(sx, sy, slen) in &starts {
        let origin = (sx + slen / 2, sy);
        if origin.0 >= w || origin.1 >= h {
            continue;
        }
        let grid = search(img, origin);
        if let Some(target) = bottom
            .iter()
            .map(|&(x, y, len)| (x + len / 2, y))
            .find(|&(x, y)| grid[y * w + x] != 0)
        {
            found = Some((origin, target, grid));
            break;
        }
    }
    let Some((origin, target, grid)) = found else {
        return false;
    };
    if coalmine && let Some(extra) = p.extra {
        for y in 0..extra.height.min(h) {
            for x in 0..extra.width.min(w) {
                if extra.at(x, y) == 0xff00_4200 {
                    img.data[y * w + x] = BLACK;
                }
            }
        }
    }
    let mut path = Vec::new();
    let (mut x, mut y) = target;
    while (x, y) != origin {
        path.push((x, y));
        match grid[y * w + x] {
            2 => y -= 1,
            3 => y += 1,
            4 => x -= 1,
            5 => x += 1,
            _ => break,
        }
    }
    let mut flags = vec![false; w * h];
    for &(x, y) in &path {
        if img.at(x, y) == BLOB {
            fill(img, &mut flags, (x, y), BLACK);
        }
    }
    randomize_blobs(img, &mut flags, p.world_seed);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic(rows: &[&str]) -> Pixels {
        let data = rows
            .iter()
            .flat_map(|r| {
                r.chars().map(|c| match c {
                    '.' => BLACK,
                    '#' => WHITE,
                    _ => BLOB,
                })
            })
            .collect();
        Pixels {
            width: rows[0].len(),
            height: rows.len(),
            data,
        }
    }

    #[test]
    fn runs_stop_at_solid_pixels() {
        let img = pic(&["..#.o#"]);
        assert_eq!(runs(&img, 0), vec![(0, 0, 2), (3, 0, 2)]);
    }

    #[test]
    fn search_steps_lead_back_to_the_start() {
        let img = pic(&["..#", "#..", "#.#"]);
        let grid = search(&img, (0, 0));
        // (1,0) was reached from the left (4), (1,1) from above (2), (2,1) from the left (4).
        assert_eq!(grid[1], 4);
        assert_eq!(grid[4], 2);
        assert_eq!(grid[5], 4);
        assert_eq!(grid[7], 2);
        assert_eq!(grid[3], 0);
    }

    #[test]
    fn a_blob_on_the_path_opens_and_the_others_follow_the_generator() {
        // Start area at x 159..223 with origin 0: pixels 15..21 of the first 7 rows are cleared.
        let mut img = Pixels {
            width: 24,
            height: 8,
            data: vec![WHITE; 24 * 8],
        };
        for y in 0..8 {
            img.data[y * 24 + 15] = BLOB;
        }
        img.data[7 * 24 + 20] = BLOB;
        let p = Params {
            template: "t.png",
            origin_x: 0,
            world_seed: 12345,
            extra: None,
        };
        assert!(validate(&mut img, &p));
        // The column of blobs at x = 15 (rows 0..=6 cleared to black, row 7 a blob on the path).
        assert_eq!(img.at(15, 7), BLACK);
        assert!(img.data.iter().all(|&c| c != BLOB));
    }

    #[test]
    fn a_closed_region_is_invalid() {
        let mut img = Pixels {
            width: 24,
            height: 10,
            data: vec![WHITE; 24 * 10],
        };
        let p = Params {
            template: "t.png",
            origin_x: 0,
            world_seed: 1,
            extra: None,
        };
        assert!(!validate(&mut img, &p));
    }
}

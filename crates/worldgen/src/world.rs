//! The world-size Wang image `ProceduralTerrain::Init` (`0x87a900`) builds: the biome map is split
//! into regions, each Wang region gets an image from `0x879bf0`, and the biome-map cells of the
//! region are copied into one image `(biome map width * 512 / 10) x (height * 512 / 10)` that
//! every Wang biome then samples. Checked against the running game with `noita_probe biomes`
//! and the `compare_live` example.

use std::collections::HashMap;
use std::path::Path;

use noita_data::materials::{self, MaterialDef};
use noita_data::sim_materials::load_png;
use sim::random::Random;

use crate::biome::{self, Topology};
use crate::generate::{Generated, spawn_colors, wang_params};
use crate::rules::RuleList;
use crate::terrain::{ColorTable, Converted, Image, WangBiome, WangMap};
use crate::wang::Tileset;

/// One `<Biome biome_filename color>` entry of `_biomes_all.xml`.
pub struct BiomeDef {
    pub file: String,
    /// `0xRRGGBB` of its pixels in the biome map.
    pub color: u32,
    pub topology: Topology,
    pub rules: RuleList,
}

impl BiomeDef {
    /// Whether `Init` builds Wang regions for it.
    pub fn is_wang(&self) -> bool {
        self.topology.kind == "BIOME_WANG_TILE"
            && !self.topology.static_tile
            && self
                .topology
                .wang_template_file
                .as_ref()
                .is_some_and(|t| !t.is_empty())
    }
}

pub struct Entries {
    pub biomes: Vec<BiomeDef>,
    pub map_file: String,
    /// `biome_offset_y`, in chunks.
    pub offset_y: i32,
}

/// Reads `_biomes_all.xml` and every biome it lists (unreadable biomes keep an empty definition).
pub fn load_entries(files: &Path, defs: &[MaterialDef]) -> Result<Entries, String> {
    let index: HashMap<&str, u32> = defs
        .iter()
        .enumerate()
        .rev()
        .map(|(i, d)| (d.name.as_str(), i as u32))
        .collect();
    let resolve = |n: &str| index.get(n).copied().unwrap_or(0);
    let path = files.join("data/biome/_biomes_all.xml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let root = entity::parse_xml(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut biomes = Vec::new();
    for c in root.children.iter().filter(|c| c.tag == "Biome") {
        let (Some(file), Some(color)) = (c.attr("biome_filename"), c.attr("color")) else {
            continue;
        };
        let color = u32::from_str_radix(color.trim(), 16).map_or(0, |v| v & 0xff_ffff);
        let (topology, rules) = std::fs::read_to_string(files.join(file))
            .ok()
            .and_then(|xml| biome::parse(&xml, resolve).ok())
            .unwrap_or_default();
        biomes.push(BiomeDef {
            file: file.to_owned(),
            color,
            topology,
            rules,
        });
    }
    Ok(Entries {
        biomes,
        map_file: root
            .attr("biome_image_map")
            .unwrap_or("data/biome_impl/biome_map.png")
            .to_owned(),
        offset_y: root
            .attr("biome_offset_y")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0),
    })
}

/// The biome map: one entry index (the first with the pixel's color) per chunk.
pub struct BiomeMap {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Option<usize>>,
}

pub fn load_biome_map(files: &Path, entries: &Entries) -> Result<BiomeMap, String> {
    let tex = load_png(&files.join(&entries.map_file))?;
    let (width, height) = (tex.width() as usize, tex.height() as usize);
    let cells = tex
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let rgb = u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
            entries.biomes.iter().position(|b| b.color == rgb)
        })
        .collect();
    Ok(BiomeMap {
        width,
        height,
        cells,
    })
}

/// A region: the bounding box (in chunks, inclusive) of a 4-connected component of one entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub biome: usize,
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

/// `0x87a900`'s region loop: scan row by row; an unclaimed cell of a Wang biome starts a region
/// (`0x92a970`, a flood fill returning the bounding box) and claims every cell of that biome
/// inside the box.
pub fn find_regions(map: &BiomeMap, is_wang: impl Fn(usize) -> bool) -> Vec<Region> {
    let (w, h) = (map.width, map.height);
    let mut claimed = vec![false; w * h];
    let mut regions = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let Some(b) = map.cells[y * w + x] else {
                continue;
            };
            if claimed[y * w + x] || !is_wang(b) {
                continue;
            }
            let mut seen = vec![false; w * h];
            let mut stack = vec![(x, y)];
            seen[y * w + x] = true;
            let mut r = Region {
                biome: b,
                x0: x,
                y0: y,
                x1: x,
                y1: y,
            };
            while let Some((cx, cy)) = stack.pop() {
                r.x0 = r.x0.min(cx);
                r.y0 = r.y0.min(cy);
                r.x1 = r.x1.max(cx);
                r.y1 = r.y1.max(cy);
                let mut push = |nx: usize, ny: usize| {
                    if map.cells[ny * w + nx] == Some(b) && !seen[ny * w + nx] {
                        seen[ny * w + nx] = true;
                        stack.push((nx, ny));
                    }
                };
                if cx > 0 {
                    push(cx - 1, cy);
                }
                if cy > 0 {
                    push(cx, cy - 1);
                }
                if cx + 1 < w {
                    push(cx + 1, cy);
                }
                if cy + 1 < h {
                    push(cx, cy + 1);
                }
            }
            for yy in r.y0..=r.y1 {
                for xx in r.x0..=r.x1 {
                    if map.cells[yy * w + xx] == Some(b) {
                        claimed[yy * w + xx] = true;
                    }
                }
            }
            regions.push(r);
        }
    }
    regions
}

/// The seed `0x879bf0` hands to the Wang generator for attempt `attempt` (0 = the first) of a
/// region `width` Wang pixels wide, from the region seed (world seed + new game plus count): a
/// `Random(double)` advanced `width % 11 + seed % 12` steps, then one step per attempt scaled like
/// the Wang RNG does.
pub fn region_wang_seed(seed: u32, width: usize, attempt: u32) -> f64 {
    let mut r = Random::from_seed(f64::from(seed));
    for _ in 0..width % 11 + (seed % 12) as usize {
        r.next_unit();
    }
    let mut v = 0.0;
    for _ in 0..=attempt {
        v = r.next_unit() * 2_147_483_645.0;
    }
    v
}

/// The world-size images and what produced them. Material indices are positions in the engine's
/// material table (`noita_data::materials::engine_order`), not in document order.
pub struct WorldMap {
    pub images: Converted,
    pub entries: Entries,
    pub biome_map: BiomeMap,
    pub regions: Vec<Region>,
    /// `0x1206fb0` / `0x12051b0`: world position + offset is the position in the biome map.
    pub offset: (f64, f64),
    /// The materials in engine order.
    pub materials: Vec<MaterialDef>,
    /// Engine table index -> index into the document-order definitions passed in.
    pub to_document: Vec<usize>,
}

fn wang_pixels(chunks: usize) -> usize {
    chunks * 512 / 10
}

/// Builds the world-size Wang images for `world_seed`; `doc_defs` are the materials in document
/// order. Every region takes its first generation attempt: the validity check (`0x8771a0`,
/// regions at most 205 Wang pixels tall) and the coalmine / excavationsite start-area step
/// (`0x876fb0`) are not ported.
pub fn build_world_map(
    files: &Path,
    doc_defs: &[MaterialDef],
    world_seed: u32,
) -> Result<WorldMap, String> {
    build_world_map_with(files, doc_defs, world_seed, |_| 0)
}

/// uild_world_map with the generation attempt to use for each region (by index in
/// WorldMap::regions order).
pub fn build_world_map_with(
    files: &Path,
    doc_defs: &[MaterialDef],
    world_seed: u32,
    attempt_of: impl Fn(usize) -> u32,
) -> Result<WorldMap, String> {
    let to_document = materials::engine_order(doc_defs);
    let defs: Vec<MaterialDef> = to_document.iter().map(|&i| doc_defs[i].clone()).collect();
    let defs = defs.as_slice();
    let entries = load_entries(files, defs)?;
    let biome_map = load_biome_map(files, &entries)?;
    let regions = find_regions(&biome_map, |b| entries.biomes[b].is_wang());
    let colors = ColorTable::new(
        defs.iter()
            .map(|d| {
                d.attr("wang_color")
                    .and_then(|c| u32::from_str_radix(c.trim(), 16).ok())
                    .unwrap_or(0)
            })
            .collect(),
    );
    let (ww, wh) = (wang_pixels(biome_map.width), wang_pixels(biome_map.height));
    let mut values = vec![0.0_f32; ww * wh];
    let mut materials = vec![0_u16; ww * wh];
    let mut spawn = vec![0_u8; ww * wh];
    let mut tilesets: HashMap<String, Tileset> = HashMap::new();
    let mut done = vec![false; biome_map.width * biome_map.height];
    for (region_index, r) in regions.iter().enumerate() {
        let def = &entries.biomes[r.biome];
        let template = def.topology.wang_template_file.clone().unwrap_or_default();
        if !tilesets.contains_key(&template) {
            let tex = load_png(&files.join(&template))?;
            let rgb: Vec<u8> = tex
                .rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            let set = Tileset::from_image(&rgb, tex.width() as usize, tex.height() as usize)
                .map_err(|e| format!("{template}: {e:?}"))?;
            tilesets.insert(template.clone(), set);
        }
        let (x0w, y0w) = (wang_pixels(r.x0), wang_pixels(r.y0));
        let (rw, rh) = (wang_pixels(r.x1 + 1) - x0w, wang_pixels(r.y1 + 1) - y0w);
        let (rw, rh) = (rw.clamp(1, 1024), rh.clamp(1, 1024));
        let rgb = tilesets[&template]
            .generate_map(
                region_wang_seed(world_seed, rw, attempt_of(region_index)),
                rw,
                rh,
            )
            .map_err(|e| format!("{template}: {e:?}"))?;
        let spawn_index: HashMap<u32, u32> = def
            .topology
            .lua_script
            .as_ref()
            .and_then(|s| std::fs::read_to_string(files.join(s)).ok())
            .map(|s| spawn_colors(&s))
            .unwrap_or_default()
            .keys()
            .enumerate()
            .map(|(i, c)| (*c, i as u32))
            .collect();
        let img = WangMap::from_wang_rgb(&rgb, rw, rh, &colors, |c| spawn_index.get(&c).copied());
        for cy in r.y0..=r.y1 {
            for cx in r.x0..=r.x1 {
                let cell = cy * biome_map.width + cx;
                if biome_map.cells[cell] != Some(r.biome) || done[cell] {
                    continue;
                }
                done[cell] = true;
                let (dx, dy) = (wang_pixels(cx), wang_pixels(cy));
                let (w, h) = (wang_pixels(cx + 1) - dx, wang_pixels(cy + 1) - dy);
                for yy in 0..h {
                    for xx in 0..w {
                        let (x, y) = (dx + xx, dy + yy);
                        if x >= ww || y >= wh {
                            continue;
                        }
                        let (sx, sy) = (dx + xx - x0w, dy + yy - y0w);
                        let i = y * ww + x;
                        if sx < rw && sy < rh {
                            let s = sy * rw + sx;
                            values[i] = img.map.values.data[s];
                            materials[i] = img.map.materials.data[s];
                            spawn[i] = img.spawn.data[s];
                        } else {
                            (values[i], materials[i], spawn[i]) = (0.0, 0, 0);
                        }
                    }
                }
            }
        }
    }
    let (w, h) = (ww as i32, wh as i32);
    let offset = (
        biome_map.width as f64 * 0.5 * 512.0,
        f64::from(entries.offset_y) * 512.0,
    );
    Ok(WorldMap {
        images: Converted {
            map: WangMap {
                values: Image::new(w, h, values),
                materials: Image::new(w, h, materials),
            },
            spawn: Image::new(w, h, spawn),
        },
        entries,
        biome_map,
        regions,
        offset,
        materials: defs.to_vec(),
        to_document,
    })
}

/// Generates `size` pixels of `biome` (a file name in `data/biome` without extension), starting
/// `inset` pixels inside the first biome map chunk that shows it, in the world of `world_seed`.
/// The validity retries, spawn functions and pixel scenes are not ported.
///
/// `files` is the directory holding the extracted reference `data/`; `defs` are its parsed
/// materials in document order (the table index is the position).
pub fn generate_biome(
    files: &Path,
    defs: &[MaterialDef],
    biome_name: &str,
    world_seed: u32,
    inset: (i32, i32),
    size: (usize, usize),
) -> Result<Generated, String> {
    let world = build_world_map(files, defs, world_seed)?;
    let target = format!("data/biome/{biome_name}.xml");
    let entry = world
        .entries
        .biomes
        .iter()
        .position(|b| b.file == target && b.is_wang())
        .ok_or_else(|| format!("{target} is not a Wang biome of _biomes_all.xml"))?;
    let cell = world
        .biome_map
        .cells
        .iter()
        .position(|c| *c == Some(entry))
        .ok_or_else(|| format!("{target} does not appear in the biome map"))?;
    let (cx, cy) = (
        (cell % world.biome_map.width) as i32,
        (cell / world.biome_map.width) as i32,
    );
    let origin = (
        cx * 512 + inset.0 - world.offset.0 as i32,
        cy * 512 + inset.1 - world.offset.1 as i32,
    );
    let params = wang_params(&world.materials);
    let spawn_pixels = world.images.spawn.data.iter().filter(|&&s| s != 0).count();
    let rules = world.entries.biomes[entry].rules.clone();
    let biome = WangBiome {
        map: world.images.map,
        rules,
        offset: world.offset,
        world_seed: world_seed as i32,
    };
    let mut out = Vec::with_capacity(size.0 * size.1);
    for y in 0..size.1 {
        for x in 0..size.0 {
            let wx = f64::from(origin.0) + x as f64;
            let wy = f64::from(origin.1) + y as f64;
            let m = biome.material_at(wx, wy, |i| {
                params.get(i as usize).copied().unwrap_or_default()
            });
            out.push(world.to_document.get(m as usize).map_or(0, |&i| i as u16));
        }
    }
    Ok(Generated {
        width: size.0,
        height: size.1,
        origin,
        materials: out,
        spawn_pixels,
    })
}

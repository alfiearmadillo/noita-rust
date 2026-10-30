//! Generates the materials of a rectangle of a Wang biome from a seed: the verified Wang map
//! generator, the region image conversion and the type 2 material lookup, wired together.
//!
//! This is the slice of world generation that is ported: no biome driver retries, spawn
//! functions, pixel scenes, `BitmapCaves` or vegetation.

use std::collections::HashMap;
use std::path::Path;

use noita_data::materials::{self, MaterialDef};
use noita_data::sim_materials::load_png;

use crate::biome;
use crate::rules::RuleList;
use crate::terrain::{ColorTable, Converted, WangBiome, WangMap, WangParams};
use crate::wang::{Tileset, WangRng};
use sim::random::Random;

/// The offset `ProceduralTerrain::Init` (`0x87a900`) draws from the world seed: a `Random(double)`
/// stepped three times, the third draw scaled to +-100000 in single precision for x; y is 7168
/// (14 chunks, the `biome_offset_y` of `_biomes_all.xml`). World position + offset is the
/// position in the biome map.
pub fn wang_offset(world_seed: u32) -> (f64, f64) {
    let mut r = Random::from_seed(f64::from(world_seed));
    r.next_unit();
    r.next_unit();
    let third = r.next_unit() as f32;
    (f64::from(third * 200_000.0 - 100_000.0), 7168.0)
}

/// The `<Biome biome_filename color>` entries of `_biomes_all.xml`, in load order.
pub fn biome_entries(files: &Path) -> Result<Vec<(String, String)>, String> {
    let path = files.join("data/biome/_biomes_all.xml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let root = entity::parse_xml(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(root
        .children
        .iter()
        .filter(|c| c.tag == "Biome")
        .filter_map(|c| {
            Some((
                c.attr("biome_filename")?.to_owned(),
                c.attr("color")?.to_owned(),
            ))
        })
        .collect())
}

/// The first biome map cell (in chunks) whose color belongs to an entry of `biome_file`.
fn biome_map_cell(
    files: &Path,
    entries: &[(String, String)],
    biome_file: &str,
) -> Result<(i32, i32), String> {
    let colors: Vec<u32> = entries
        .iter()
        .filter(|(f, _)| f == biome_file)
        .filter_map(|(_, c)| u32::from_str_radix(c.trim(), 16).ok())
        .map(|c| c & 0xff_ffff)
        .collect();
    let tex = load_png(&files.join("data/biome_impl/biome_map.png"))?;
    let w = tex.width() as usize;
    for (i, p) in tex.rgba().as_chunks::<4>().0.iter().enumerate() {
        let rgb = u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
        if colors.contains(&rgb) {
            return Ok(((i % w) as i32, (i / w) as i32));
        }
    }
    Err(format!("{biome_file} does not appear in the biome map"))
}

/// The materials of a rectangle: `materials[y * width + x]` is a material table index (0 = air
/// or nothing).
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    pub width: usize,
    pub height: usize,
    /// World position of the first pixel.
    pub origin: (i32, i32),
    pub materials: Vec<u16>,
    /// Wang pixels that carried a spawn-function color (they stay empty here).
    pub spawn_pixels: usize,
}

/// `RegisterSpawnFunction( 0x........, "name" )` colors of a biome script as `0xRRGGBB`; the Lua
/// number is the engine's `0xAABBGGRR` pixel word.
pub fn spawn_colors(lua: &str) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    for line in lua.lines() {
        let Some(rest) = line.trim().strip_prefix("RegisterSpawnFunction(") else {
            continue;
        };
        let Some((color, name)) = rest.split_once(',') else {
            continue;
        };
        let Ok(word) = u32::from_str_radix(color.trim().trim_start_matches("0x"), 16) else {
            continue;
        };
        let name = name.trim().trim_start_matches('"');
        let name = name.split('"').next().unwrap_or_default();
        let (r, g, b) = (word & 0xff, (word >> 8) & 0xff, (word >> 16) & 0xff);
        out.insert(r << 16 | g << 8 | b, name.to_owned());
    }
    out
}

/// The Wang numbers of every material (`wang_noise_percent`, `wang_curvature`,
/// `wang_noise_type`), with the `CellData` constructor defaults for absent attributes.
pub fn wang_params(defs: &[MaterialDef]) -> Vec<WangParams> {
    defs.iter()
        .map(|d| {
            let mut p = WangParams::default();
            if let Some(v) = d.attr_f32("wang_noise_percent") {
                p.noise_percent = v;
            }
            if let Some(v) = d.attr_f32("wang_curvature") {
                p.curvature = v;
            }
            if let Some(v) = d
                .attr("wang_noise_type")
                .and_then(|v| v.trim().parse().ok())
            {
                p.noise_type = v;
            }
            p
        })
        .collect()
}

/// One `_biomes_all.xml` entry after loading: its definition and, for a Wang biome, the RGB map
/// the exe generated for it.
pub struct LoadedBiome {
    pub file: String,
    pub topology: biome::Topology,
    pub rules: RuleList,
    pub map_rgb: Option<Vec<u8>>,
}

fn name_index(defs: &[MaterialDef]) -> HashMap<&str, u32> {
    defs.iter()
        .enumerate()
        .rev()
        .map(|(i, d)| (d.name.as_str(), i as u32))
        .collect()
}

/// Loads the `_biomes_all.xml` entries in order, up to and including the first one whose file
/// is `stop_at` (all of them when `None`), generating each Wang biome's map from one shared
/// generator stream that starts as `Random(world seed)`: the exe passes seed 0 when it builds
/// these maps, which continues the thread's generator (`0x867500`). Entries that cannot be read
/// are skipped. The stream order is derived from the decompile.
pub fn load_biomes(
    files: &Path,
    defs: &[MaterialDef],
    world_seed: u32,
    stop_at: Option<&str>,
) -> Result<Vec<LoadedBiome>, String> {
    let index_by_name = name_index(defs);
    let resolve = |n: &str| index_by_name.get(n).copied().unwrap_or(0);
    let mut rng = WangRng::new(f64::from(world_seed));
    let mut tilesets: HashMap<String, Tileset> = HashMap::new();
    let mut loaded = Vec::new();
    for (file, _) in biome_entries(files)? {
        let Ok(xml) = std::fs::read_to_string(files.join(&file)) else {
            continue;
        };
        let Ok((topology, rules)) = biome::parse(&xml, resolve) else {
            continue;
        };
        let (w, h) = (
            topology.wang_map_width as usize,
            topology.wang_map_height as usize,
        );
        let mut map_rgb = None;
        if topology.kind == "BIOME_WANG_TILE"
            && let Some(template) = &topology.wang_template_file
            && !template.is_empty()
            && w > 0
            && h > 0
        {
            if !tilesets.contains_key(template) {
                let tex = load_png(&files.join(template))?;
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
            map_rgb = Some(
                tilesets[template]
                    .generate_map_with(&mut rng, w, h)
                    .map_err(|e| format!("{template}: {e:?}"))?,
            );
        }
        let done = stop_at == Some(file.as_str());
        loaded.push(LoadedBiome {
            file,
            topology,
            rules,
            map_rgb,
        });
        if done {
            break;
        }
    }
    Ok(loaded)
}

/// The float, material and spawn images of a loaded Wang biome (`0x8704c0`), or `None` for a
/// biome without a generated map.
pub fn convert_biome(files: &Path, defs: &[MaterialDef], biome: &LoadedBiome) -> Option<Converted> {
    let rgb = biome.map_rgb.as_ref()?;
    let colors = ColorTable::new(
        defs.iter()
            .map(|d| {
                d.attr("wang_color")
                    .and_then(|c| u32::from_str_radix(c.trim(), 16).ok())
                    .unwrap_or(0)
            })
            .collect(),
    );
    let spawn_index: HashMap<u32, u32> = biome
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
    let (w, h) = (
        biome.topology.wang_map_width as usize,
        biome.topology.wang_map_height as usize,
    );
    Some(WangMap::from_wang_rgb(rgb, w, h, &colors, |c| {
        spawn_index.get(&c).copied()
    }))
}

/// Generates `size` pixels of `biome` (a file name in `data/biome` without extension), starting
/// `inset` pixels inside the first biome map chunk that shows it, in the world of `world_seed`.
///
/// The biome's Wang map is what `ProceduralTerrain::Init` builds when it loads the biome (see
/// `load_biomes`); the validity retries, spawn functions and pixel scenes are not ported.
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
    let params = wang_params(defs);
    let target = format!("data/biome/{biome_name}.xml");
    let mut loaded = load_biomes(files, defs, world_seed, Some(&target))?;
    if loaded.last().is_none_or(|b| b.file != target) {
        return Err(format!("{target} is not in _biomes_all.xml"));
    }
    let last = loaded.pop().expect("checked above");
    let converted = convert_biome(files, defs, &last)
        .ok_or_else(|| format!("{biome_name} is not a Wang biome"))?;
    let rules = last.rules;
    let entries = biome_entries(files)?;
    let spawn_pixels = converted.spawn.data.iter().filter(|&&s| s != 0).count();

    let offset = wang_offset(world_seed);
    let (cx, cy) = biome_map_cell(files, &entries, &target)?;
    let origin = (
        cx * 512 + inset.0 - offset.0 as i32,
        cy * 512 + inset.1 - offset.1 as i32,
    );
    let biome = WangBiome {
        map: converted.map,
        rules,
        offset,
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
            out.push(m as u16);
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

/// Loads `data/materials.xml` under `files`.
pub fn load_materials(files: &Path) -> Result<Vec<MaterialDef>, String> {
    let path = files.join("data/materials.xml");
    let xml = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    materials::parse(&xml).map_err(|e| format!("{}: {e}", path.display()))
}

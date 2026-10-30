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
use crate::terrain::{ColorTable, WangBiome, WangMap, WangParams};
use crate::wang::Tileset;

/// The materials of a rectangle: `materials[y * width + x]` is a material table index (0 = air
/// or nothing).
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    pub width: usize,
    pub height: usize,
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

/// Generates `size` pixels of `biome` (a file name in `data/biome` without extension) starting at
/// world position `origin`. The Wang map covers `chunks` 512-pixel chunks from world (0, 0), so
/// the rectangle should lie inside it.
///
/// `files` is the directory holding the extracted reference `data/`; `defs` are its parsed
/// materials in document order (the table index is the position).
pub fn generate_biome(
    files: &Path,
    defs: &[MaterialDef],
    biome_name: &str,
    seed: f64,
    origin: (i32, i32),
    size: (usize, usize),
    chunks: (usize, usize),
) -> Result<Generated, String> {
    let index_by_name: HashMap<&str, u32> = defs
        .iter()
        .enumerate()
        .rev()
        .map(|(i, d)| (d.name.as_str(), i as u32))
        .collect();
    let colors = ColorTable::new(
        defs.iter()
            .map(|d| {
                d.attr("wang_color")
                    .and_then(|c| u32::from_str_radix(c.trim(), 16).ok())
                    .unwrap_or(0)
            })
            .collect(),
    );
    let params = wang_params(defs);

    let xml_path = files.join("data/biome").join(format!("{biome_name}.xml"));
    let xml =
        std::fs::read_to_string(&xml_path).map_err(|e| format!("{}: {e}", xml_path.display()))?;
    let (topology, rules) = biome::parse(&xml, |n| index_by_name.get(n).copied().unwrap_or(0))
        .map_err(|e| format!("{}: {e}", xml_path.display()))?;
    let template = topology
        .wang_template_file
        .ok_or_else(|| format!("{biome_name} has no Wang template"))?;
    let tex = load_png(&files.join(&template))?;
    let rgb: Vec<u8> = tex
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let tileset = Tileset::from_image(&rgb, tex.width() as usize, tex.height() as usize)
        .map_err(|e| format!("{template}: {e:?}"))?;
    let (w, h) = (
        (chunks.0 * 512 / 10).clamp(1, 1024),
        (chunks.1 * 512 / 10).clamp(1, 1024),
    );
    let map_rgb = tileset
        .generate_map(seed, w, h)
        .map_err(|e| format!("{template}: {e:?}"))?;

    let spawn_index: HashMap<u32, u32> = topology
        .lua_script
        .as_ref()
        .and_then(|s| std::fs::read_to_string(files.join(s)).ok())
        .map(|s| spawn_colors(&s))
        .unwrap_or_default()
        .keys()
        .enumerate()
        .map(|(i, c)| (*c, i as u32))
        .collect();
    let converted =
        WangMap::from_wang_rgb(&map_rgb, w, h, &colors, |c| spawn_index.get(&c).copied());
    let spawn_pixels = converted.spawn.data.iter().filter(|&&s| s != 0).count();

    let biome = WangBiome {
        map: converted.map,
        rules,
        offset: (0.0, 0.0),
        world_seed: seed as i32,
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

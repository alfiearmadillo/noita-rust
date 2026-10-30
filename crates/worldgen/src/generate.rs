//! Generates the materials of a rectangle of a Wang biome from a seed: the verified Wang map
//! generator, the region image conversion and the type 2 material lookup, wired together.
//!
//! This is the slice of world generation that is ported: no biome driver retries, spawn
//! functions, pixel scenes, `BitmapCaves` or vegetation.

use std::collections::HashMap;
use std::path::Path;

use noita_data::materials::{self, MaterialDef};

use crate::terrain::WangParams;

pub use crate::world::generate_biome;

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

/// `RegisterSpawnFunction( 0x........, "name" )` colors of a biome script as `0xRRGGBB`: the Lua
/// number is written `0xAARRGGBB`, and the template pixels the running game treats as spawn pixels
/// have exactly those RGB values.
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
        out.insert(word & 0xff_ffff, name.to_owned());
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

/// Loads `data/materials.xml` under `files`.
pub fn load_materials(files: &Path) -> Result<Vec<MaterialDef>, String> {
    let path = files.join("data/materials.xml");
    let xml = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    materials::parse(&xml).map_err(|e| format!("{}: {e}", path.display()))
}

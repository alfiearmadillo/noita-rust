//! Converts parsed `materials.xml` definitions into the `sim` material table.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use sim::{CellType, MaterialProps, Materials, ReactionProps, Texture};

use crate::materials::{MaterialDef, ReactionDef};

/// Fallback ARGB when a definition has neither color nor texture; deliberately loud.
pub const MISSING_COLOR: u32 = 0xffff_00ff;

/// Index 0 is air; other ids follow `materials.xml` document order (engine id order unrecovered).
/// `load_texture` receives each `texture_file` path as written (e.g. `data/materials_gfx/sand.png`).
pub fn to_sim(
    defs: &[MaterialDef],
    mut load_texture: impl FnMut(&str) -> Option<Texture>,
) -> Materials {
    let mut cache: HashMap<&str, Option<Arc<Texture>>> = HashMap::new();
    let mut props = Vec::with_capacity(defs.len() + 1);
    if defs.first().is_none_or(|d| d.name != "air") {
        props.push(MaterialProps::new("air", CellType::Unspecified));
    }
    for d in defs {
        // An empty `texture_file` overrides an inherited one (e.g. `glowstone_altar`) and means none.
        let texture = d
            .texture_file
            .as_deref()
            .filter(|f| !f.is_empty())
            .and_then(|f| {
                cache
                    .entry(f)
                    .or_insert_with(|| load_texture(f).map(Arc::new))
                    .clone()
            });
        // Load-time fallback (0x6fae61): with no Graphics color, `+0x64` takes `wang_color` (`+0x40`).
        let wang = d
            .attr("wang_color")
            .and_then(|s| u32::from_str_radix(s.trim(), 16).ok());
        let color = d
            .graphics_color
            .filter(|&c| c != 0)
            .or(wang)
            .unwrap_or(if texture.is_some() { 0 } else { MISSING_COLOR });
        props.push(MaterialProps {
            color,
            texture,
            ..props_of(d)
        });
    }
    Materials::new(props)
}

/// Converts materials and preserves the parsed reaction records in source order.
pub fn to_sim_with_reactions(
    defs: &[MaterialDef],
    reactions: &[ReactionDef],
    required: &[ReactionDef],
    load_texture: impl FnMut(&str) -> Option<Texture>,
) -> Materials {
    let props = |records: &[ReactionDef]| {
        records
            .iter()
            .map(|reaction| ReactionProps {
                attrs: reaction.attrs.clone(),
                explosion_config: reaction.explosion_config.clone(),
            })
            .collect()
    };
    to_sim(defs, load_texture)
        .with_reactions(props(reactions))
        .with_required_reactions(props(required))
}

/// Decodes a PNG into RGBA8.
pub fn load_png(path: &Path) -> Result<Texture, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| format!("{}: image too large", path.display()))?;
    let mut buf = vec![0; size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => px
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(format!("{}: palette not expanded", path.display())),
    };
    Texture::new(info.width, info.height, rgba)
        .ok_or_else(|| format!("{}: bad size", path.display()))
}

fn props_of(d: &MaterialDef) -> MaterialProps {
    let cell_type = match d.attr("cell_type") {
        Some("liquid") => CellType::Liquid,
        Some("gas") => CellType::Gas,
        Some("solid") => CellType::Solid,
        Some("fire") => CellType::Fire,
        _ => CellType::Unspecified,
    };
    let int = |k| d.attr(k).and_then(|s| s.trim().parse::<i32>().ok());
    let base = MaterialProps::new(d.name.clone(), cell_type);
    MaterialProps {
        tags: d.tags().map(str::to_owned).collect(),
        density: d.attr_f32("density").unwrap_or(base.density),
        liquid_sand: d.attr_flag("liquid_sand").unwrap_or(base.liquid_sand),
        liquid_slime: d.attr_flag("liquid_slime").unwrap_or(base.liquid_slime),
        liquid_static: d.attr_flag("liquid_static").unwrap_or(base.liquid_static),
        liquid_stains_self: d
            .attr_flag("liquid_stains_self")
            .unwrap_or(base.liquid_stains_self),
        liquid_stains: int("liquid_stains").map_or(base.liquid_stains, |v| v as u32),
        liquid_stains_custom_color: d
            .attr("liquid_stains_custom_color")
            .and_then(|s| u32::from_str_radix(s.trim(), 16).ok())
            .unwrap_or(base.liquid_stains_custom_color),
        stainable: d.attr_flag("stainable").unwrap_or(base.stainable),
        cell_holes_in_texture: d
            .attr_flag("cell_holes_in_texture")
            .unwrap_or(base.cell_holes_in_texture),
        liquid_gravity: d.attr_f32("liquid_gravity").unwrap_or(base.liquid_gravity),
        liquid_viscosity: int("liquid_viscosity").unwrap_or(base.liquid_viscosity),
        liquid_sticks_to_ceiling: int("liquid_sticks_to_ceiling")
            .unwrap_or(base.liquid_sticks_to_ceiling),
        liquid_damping: d.attr_f32("liquid_damping").unwrap_or(base.liquid_damping),
        liquid_flow_speed: d
            .attr_f32("liquid_flow_speed")
            .unwrap_or(base.liquid_flow_speed),
        liquid_sand_never_box2d: d
            .attr_flag("liquid_sand_never_box2d")
            .unwrap_or(base.liquid_sand_never_box2d),
        slippery: d.attr_flag("slippery").unwrap_or(base.slippery),
        platform_type: int("platform_type").unwrap_or(base.platform_type),
        lifetime: int("lifetime").unwrap_or(base.lifetime),
        durability: int("durability").unwrap_or(base.durability),
        hp: int("hp").unwrap_or(base.hp),
        explosion: d.explosion_config.as_ref().map(|attrs| {
            let mut config = sim::particles::ExplosionConfig::default();
            config.apply(attrs);
            std::sync::Arc::new(config)
        }),
        // Byte fields (`+0x194..+0x197`).
        gas_speed: int("gas_speed").map_or(base.gas_speed, |v| v as u8),
        gas_upwards_speed: int("gas_upwards_speed").map_or(base.gas_upwards_speed, |v| v as u8),
        gas_horizontal_speed: int("gas_horizontal_speed")
            .map_or(base.gas_horizontal_speed, |v| v as u8),
        gas_downwards_speed: int("gas_downwards_speed")
            .map_or(base.gas_downwards_speed, |v| v as u8),
        burnable: d.attr_flag("burnable").unwrap_or(base.burnable),
        on_fire: d.attr_flag("on_fire").unwrap_or(base.on_fire),
        fire_hp: int("fire_hp").unwrap_or(base.fire_hp),
        autoignition_temperature: int("autoignition_temperature")
            .unwrap_or(base.autoignition_temperature),
        temperature_of_fire: int("temperature_of_fire").unwrap_or(base.temperature_of_fire),
        generates_smoke: int("generates_smoke").unwrap_or(base.generates_smoke),
        generates_flames: int("generates_flames").unwrap_or(base.generates_flames),
        requires_oxygen: d
            .attr_flag("requires_oxygen")
            .unwrap_or(base.requires_oxygen),
        on_fire_convert_to_material: d
            .attr("on_fire_convert_to_material")
            .unwrap_or(&base.on_fire_convert_to_material)
            .to_owned(),
        on_fire_flame_material: d
            .attr("on_fire_flame_material")
            .unwrap_or(&base.on_fire_flame_material)
            .to_owned(),
        on_fire_smoke_material: d
            .attr("on_fire_smoke_material")
            .unwrap_or(&base.on_fire_smoke_material)
            .to_owned(),
        ..base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::parse;

    #[test]
    fn maps_fields_defaults_and_colors() {
        let defs = parse(
            r#"<Materials>
              <CellData name="air"/>
              <CellData name="sand" cell_type="liquid" density="6" liquid_sand="1" liquid_stains_self="1" liquid_gravity="0.9">
                <Graphics color="ffb89e57"/>
              </CellData>
            </Materials>"#,
        )
        .unwrap();
        let table = to_sim(&defs, |_| None);
        assert_eq!(table.len(), 2);
        let sand = table.get(table.find("sand").unwrap()).unwrap();
        assert_eq!(sand.cell_type, CellType::Liquid);
        assert!(sand.liquid_sand && !sand.liquid_static && sand.liquid_stains_self);
        assert_eq!((sand.density, sand.liquid_gravity), (6.0, 0.9));
        assert_eq!(sand.liquid_viscosity, 50, "constructor default");
        assert_eq!(sand.color, 0xffb8_9e57);
    }

    #[test]
    fn keeps_reaction_records_in_the_sim_material_table() {
        let xml = r#"<Materials>
          <CellData name="air"/>
          <Reaction probability="50" input_cell1="sodium">
            <ExplosionConfig ray_energy="50000"/>
          </Reaction>
          <Reaction probability="70" output_cell1="steam"/>
        </Materials>"#;
        let defs = crate::materials::parse(xml).unwrap();
        let reactions = crate::materials::parse_reactions(xml).unwrap();
        let table = to_sim_with_reactions(&defs, &reactions, &[], |_| None);

        assert_eq!(table.reactions().len(), 2);
        assert_eq!(
            table.reactions()[0]
                .attrs
                .get("input_cell1")
                .map(String::as_str),
            Some("sodium")
        );
        assert_eq!(
            table.reactions()[0]
                .explosion_config
                .as_ref()
                .and_then(|config| config.get("ray_energy"))
                .map(String::as_str),
            Some("50000")
        );
        assert_eq!(
            table.reactions()[1]
                .attrs
                .get("output_cell1")
                .map(String::as_str),
            Some("steam")
        );
    }

    #[test]
    fn maps_fire_properties_and_inherited_material_names() {
        let defs = parse(
                        r#"<Materials>
                            <CellData name="air"/>
                            <CellData name="wood" burnable="1" fire_hp="600" autoignition_temperature="85"
                                temperature_of_fire="90" generates_smoke="1" generates_flames="2" requires_oxygen="1"
                                on_fire_convert_to_material="ash" on_fire_flame_material="fire"
                                on_fire_smoke_material="smoke"/>
                            <CellDataChild _parent="wood" name="dry_wood" on_fire="1" fire_hp="300"/>
                        </Materials>"#,
                )
                .unwrap();
        let table = to_sim(&defs, |_| None);
        let wood = table.get(table.find("wood").unwrap()).unwrap();
        assert!(wood.burnable && wood.requires_oxygen);
        assert!(!wood.on_fire);
        assert_eq!(wood.fire_hp, 600);
        assert_eq!(wood.autoignition_temperature, 85);
        assert_eq!(wood.temperature_of_fire, 90);
        assert_eq!((wood.generates_smoke, wood.generates_flames), (1, 2));
        assert_eq!(wood.on_fire_convert_to_material, "ash");
        assert_eq!(wood.on_fire_flame_material, "fire");
        assert_eq!(wood.on_fire_smoke_material, "smoke");

        let dry_wood = table.get(table.find("dry_wood").unwrap()).unwrap();
        assert!(dry_wood.on_fire);
        assert_eq!(dry_wood.fire_hp, 300);
        assert_eq!(dry_wood.on_fire_convert_to_material, "ash");
    }

    #[test]
    fn inserts_air_when_first_definition_is_not_air() {
        let defs = parse(r#"<Materials><CellData name="rock"/></Materials>"#).unwrap();
        let table = to_sim(&defs, |_| None);
        assert_eq!(table.find("air"), Some(sim::AIR));
        assert_eq!(table.get(sim::MaterialId(1)).unwrap().color, MISSING_COLOR);
    }

    #[test]
    fn textures_load_once_per_file_and_inherit() {
        let defs = parse(
            r#"<Materials>
              <CellData name="air"/>
              <CellData name="a"><Graphics texture_file="t.png"/></CellData>
              <CellDataChild _parent="a" name="b"/>
            </Materials>"#,
        )
        .unwrap();
        let mut loads = 0;
        let table = to_sim(&defs, |f| {
            assert_eq!(f, "t.png");
            loads += 1;
            Texture::new(1, 1, vec![1, 2, 3, 4])
        });
        assert_eq!(loads, 1);
        for id in [1, 2] {
            let m = table.get(sim::MaterialId(id)).unwrap();
            assert_eq!(m.color_at(5, 5), 0x0401_0203);
        }
    }

    #[test]
    fn empty_texture_file_clears_inherited_texture() {
        let defs = parse(
            r#"<Materials>
              <CellData name="air"/>
              <CellData name="a"><Graphics texture_file="t.png"/></CellData>
              <CellDataChild _parent="a" name="b"><Graphics texture_file="" color="ff2feb48"/></CellDataChild>
            </Materials>"#,
        )
        .unwrap();
        let table = to_sim(&defs, |_| Texture::new(1, 1, vec![1, 2, 3, 4]));
        let b = table.get(sim::MaterialId(2)).unwrap();
        assert!(b.texture.is_none());
        assert_eq!(b.color_at(0, 0), 0xff2f_eb48);
    }
}

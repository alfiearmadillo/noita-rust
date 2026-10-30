//! Biome definitions (`data/biome/*.xml`): the topology attributes the world generator reads and
//! the `<Materials>` rule list.

use std::fmt;

use entity::Node;

use crate::rules::{Rule, RuleList};

/// The `<Topology>` attributes of a biome.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Topology {
    pub name: String,
    pub kind: String,
    pub wang_template_file: Option<String>,
    pub wang_map_width: u32,
    pub wang_map_height: u32,
    pub lua_script: Option<String>,
}

#[derive(Debug)]
pub enum BiomeError {
    Xml(String),
    Missing(&'static str),
}

impl fmt::Display for BiomeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xml(e) => write!(f, "invalid XML: {e}"),
            Self::Missing(what) => write!(f, "missing <{what}>"),
        }
    }
}

impl std::error::Error for BiomeError {}

/// A fresh `MaterialComponent` as the reference constructs it (default block in `0x77c680`);
/// attributes absent from the XML keep these values.
pub fn default_rule() -> Rule {
    Rule {
        material: 0,
        min: 0.1,
        max: 0.1,
        add_perlin: false,
        add_perlin_scale: [1.0, 1.0],
        is_rare: false,
        limit_y: false,
        limit_min_y: 100.0,
        limit_max_y: 2048.0,
        rare_use_perlin: false,
        rare_use_fbm_perlin: false,
        rare_use_polka: true,
        rare_scale: [0.05, 0.05],
        rare_offset_by_seed: false,
        rare_offset: [0.0, 0.0],
        polka_radius_low: 0.2,
        polka_radius_high: 0.65,
        polka_boxed: true,
        polka_probability: 0.2,
        rare_required_min: 0.2,
        rare_required_max: 1.0,
        is_polygon: false,
        polygon: Vec::new(),
    }
}

fn flag(node: &Node, name: &str, into: &mut bool) {
    if let Some(v) = node.attr(name) {
        *into = v.trim() != "0";
    }
}

fn float(node: &Node, name: &str, into: &mut f32) {
    if let Some(v) = node.attr(name).and_then(|v| v.trim().parse().ok()) {
        *into = v;
    }
}

/// Reads a `<MaterialComponent>`; `resolve` maps `material_name` to a material table index
/// (0 for unknown names, which disables the rule).
fn parse_rule(node: &Node, resolve: &impl Fn(&str) -> u32) -> Rule {
    let mut r = default_rule();
    r.material = node.attr("material_name").map_or(0, resolve);
    float(node, "material_min", &mut r.min);
    float(node, "material_max", &mut r.max);
    flag(node, "add_perlin", &mut r.add_perlin);
    float(node, "add_perlin_scale_x", &mut r.add_perlin_scale[0]);
    float(node, "add_perlin_scale_y", &mut r.add_perlin_scale[1]);
    flag(node, "is_rare", &mut r.is_rare);
    flag(node, "limit_y", &mut r.limit_y);
    float(node, "limit_min_y", &mut r.limit_min_y);
    float(node, "limit_max_y", &mut r.limit_max_y);
    flag(node, "rare_use_perlin", &mut r.rare_use_perlin);
    flag(node, "rare_use_fbm_perlin", &mut r.rare_use_fbm_perlin);
    flag(node, "rare_use_polka", &mut r.rare_use_polka);
    float(node, "rare_scale_x", &mut r.rare_scale[0]);
    float(node, "rare_scale_y", &mut r.rare_scale[1]);
    flag(node, "rare_offset_by_seed", &mut r.rare_offset_by_seed);
    float(node, "rare_offset_x", &mut r.rare_offset[0]);
    float(node, "rare_offset_y", &mut r.rare_offset[1]);
    float(node, "rare_polka_radius_low", &mut r.polka_radius_low);
    float(node, "rare_polka_radius_high", &mut r.polka_radius_high);
    flag(node, "rare_polka_is_boxed", &mut r.polka_boxed);
    float(node, "rare_polka_probability", &mut r.polka_probability);
    float(node, "rare_required_min", &mut r.rare_required_min);
    float(node, "rare_required_max", &mut r.rare_required_max);
    flag(node, "is_polygon", &mut r.is_polygon);
    if let Some(poly) = node.child("polygon") {
        r.polygon = poly
            .children
            .iter()
            .filter(|c| c.tag == "Vec2")
            .map(|v| {
                let get = |a| v.attr(a).and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
                [get("x"), get("y")]
            })
            .collect();
    }
    r
}

impl RuleList {
    /// The list the loader builds (`0x86f4f0`): the window `[lo, hi]` spans the
    /// `material_min` / `material_max` of every rule with a material.
    pub fn from_rules(rules: Vec<Rule>) -> Self {
        let mut lo = f32::MAX;
        let mut hi = -f32::MAX;
        for r in rules.iter().filter(|r| r.material != 0) {
            if r.min < lo {
                lo = r.min;
            }
            if hi <= r.max && r.max != hi {
                hi = r.max;
            }
        }
        Self { lo, hi, rules }
    }
}

/// Parses a biome file: its `<Topology>` and its `<Materials>` rule list.
pub fn parse(xml: &str, resolve: impl Fn(&str) -> u32) -> Result<(Topology, RuleList), BiomeError> {
    let root = entity::parse_xml(xml).map_err(BiomeError::Xml)?;
    let topo = root
        .child("Topology")
        .ok_or(BiomeError::Missing("Topology"))?;
    let materials = root
        .child("Materials")
        .ok_or(BiomeError::Missing("Materials"))?;
    let num = |name: &str| {
        topo.attr(name)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    };
    let topology = Topology {
        name: topo.attr("name").unwrap_or_default().to_owned(),
        kind: topo.attr("type").unwrap_or_default().to_owned(),
        wang_template_file: topo.attr("wang_template_file").map(str::to_owned),
        wang_map_width: num("wang_map_width"),
        wang_map_height: num("wang_map_height"),
        lua_script: topo.attr("lua_script").map(str::to_owned),
    };
    let rules = materials
        .children
        .iter()
        .filter(|c| c.tag == "MaterialComponent")
        .map(|c| parse_rule(c, &resolve))
        .collect();
    Ok((topology, RuleList::from_rules(rules)))
}

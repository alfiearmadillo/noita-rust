//! `data/materials.xml` reader: `CellData` / `CellDataChild` definitions with `_parent` inheritance.
//!
//! Attributes are kept as raw strings; typed accessors interpret them. Missing attributes return
//! `None` because the engine's defaults live in the executable and are not yet recovered.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialDef {
    pub name: String,
    /// Resolved attributes (parent's, overridden by the child's own).
    pub attrs: BTreeMap<String, String>,
    /// `<Graphics color="AARRGGBB">`, inherited from the parent when absent.
    pub graphics_color: Option<u32>,
    /// `<Graphics texture_file=...>`, inherited from the parent when absent.
    pub texture_file: Option<String>,
    /// Attributes of a nested `<ExplosionConfig>`, inherited from the parent when absent.
    pub explosion_config: Option<BTreeMap<String, String>>,
}

impl MaterialDef {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }

    pub fn attr_f32(&self, key: &str) -> Option<f32> {
        self.attr(key)?.trim().parse().ok()
    }

    /// `"1"` / `"0"` flags.
    pub fn attr_flag(&self, key: &str) -> Option<bool> {
        match self.attr(key)?.trim() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        }
    }

    pub fn tags(&self) -> impl Iterator<Item = &str> {
        self.attr("tags")
            .unwrap_or("")
            .split(',')
            .map(|t| t.trim().trim_start_matches('[').trim_end_matches(']'))
            .filter(|t| !t.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReactionDef {
    pub attrs: BTreeMap<String, String>,
    pub explosion_config: Option<BTreeMap<String, String>>,
}

#[derive(Debug)]
pub enum MaterialsError {
    Xml(roxmltree::Error),
    MissingName { line: u32 },
    UnknownParent { name: String, parent: String },
    InheritanceCycle { name: String },
}

impl fmt::Display for MaterialsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xml(e) => write!(f, "xml: {e}"),
            Self::MissingName { line } => write!(f, "material without name at line {line}"),
            Self::UnknownParent { name, parent } => {
                write!(f, "{name}: parent {parent:?} is not defined")
            }
            Self::InheritanceCycle { name } => write!(f, "{name}: _parent chain forms a cycle"),
        }
    }
}

impl std::error::Error for MaterialsError {}

/// A definition as written, before inheritance is applied.
struct RawDef {
    parent: Option<String>,
    own: MaterialDef,
}

/// Parses materials in document order. Parents may be defined after their children.
pub fn parse(xml: &str) -> Result<Vec<MaterialDef>, MaterialsError> {
    let doc = roxmltree::Document::parse(xml).map_err(MaterialsError::Xml)?;
    let mut raw: Vec<RawDef> = Vec::new();

    for node in doc.root_element().children().filter(|n| n.is_element()) {
        let is_child = match node.tag_name().name() {
            "CellData" => false,
            "CellDataChild" => true,
            _ => continue,
        };
        let name = node
            .attribute("name")
            .ok_or_else(|| MaterialsError::MissingName {
                line: doc.text_pos_at(node.range().start).row,
            })?
            .to_owned();
        let graphics = node.children().find(|c| c.has_tag_name("Graphics"));
        raw.push(RawDef {
            parent: is_child.then(|| node.attribute("_parent").unwrap_or_default().to_owned()),
            own: MaterialDef {
                name,
                attrs: node
                    .attributes()
                    .map(|a| (a.name().to_owned(), a.value().to_owned()))
                    .collect(),
                graphics_color: graphics
                    .and_then(|g| g.attribute("color"))
                    .and_then(parse_argb),
                texture_file: graphics
                    .and_then(|g| g.attribute("texture_file"))
                    .map(str::to_owned),
                explosion_config: node
                    .children()
                    .find(|c| c.has_tag_name("ExplosionConfig"))
                    .map(|c| {
                        c.attributes()
                            .map(|a| (a.name().to_owned(), a.value().to_owned()))
                            .collect()
                    }),
            },
        });
    }

    let index: BTreeMap<&str, usize> = raw
        .iter()
        .enumerate()
        .map(|(i, r)| (r.own.name.as_str(), i))
        .collect();
    let mut resolved: Vec<Option<MaterialDef>> = vec![None; raw.len()];
    for i in 0..raw.len() {
        resolve(i, &raw, &index, &mut resolved, &mut Vec::new())?;
    }
    Ok(resolved
        .into_iter()
        .map(|d| d.expect("all resolved"))
        .collect())
}

/// Parses top-level `<Reaction>` records in document order, preserving their raw attributes.
pub fn parse_reactions(xml: &str) -> Result<Vec<ReactionDef>, MaterialsError> {
    parse_records(xml, "Reaction")
}

/// Parses top-level `<ReqReaction>` records (the reacting cell turns into output1 unless a
/// neighbor holds input2) in document order.
pub fn parse_required_reactions(xml: &str) -> Result<Vec<ReactionDef>, MaterialsError> {
    parse_records(xml, "ReqReaction")
}

fn parse_records(xml: &str, tag: &str) -> Result<Vec<ReactionDef>, MaterialsError> {
    let doc = roxmltree::Document::parse(xml).map_err(MaterialsError::Xml)?;
    Ok(doc
        .root_element()
        .children()
        .filter(|node| node.has_tag_name(tag))
        .map(|node| ReactionDef {
            attrs: node
                .attributes()
                .map(|attr| (attr.name().to_owned(), attr.value().to_owned()))
                .collect(),
            explosion_config: node
                .children()
                .find(|child| child.has_tag_name("ExplosionConfig"))
                .map(|config| {
                    config
                        .attributes()
                        .map(|attr| (attr.name().to_owned(), attr.value().to_owned()))
                        .collect()
                }),
        })
        .collect())
}

fn resolve(
    i: usize,
    raw: &[RawDef],
    index: &BTreeMap<&str, usize>,
    resolved: &mut [Option<MaterialDef>],
    visiting: &mut Vec<usize>,
) -> Result<(), MaterialsError> {
    if resolved[i].is_some() {
        return Ok(());
    }
    let own = &raw[i].own;
    let Some(parent) = &raw[i].parent else {
        resolved[i] = Some(own.clone());
        return Ok(());
    };
    if visiting.contains(&i) {
        return Err(MaterialsError::InheritanceCycle {
            name: own.name.clone(),
        });
    }
    let &p = index
        .get(parent.as_str())
        .ok_or_else(|| MaterialsError::UnknownParent {
            name: own.name.clone(),
            parent: parent.clone(),
        })?;
    visiting.push(i);
    resolve(p, raw, index, resolved, visiting)?;
    visiting.pop();

    let mut def = resolved[p].clone().expect("parent resolved");
    def.name.clone_from(&own.name);
    def.attrs
        .extend(own.attrs.iter().map(|(k, v)| (k.clone(), v.clone())));
    if own.graphics_color.is_some() {
        def.graphics_color = own.graphics_color;
    }
    if own.texture_file.is_some() {
        def.texture_file.clone_from(&own.texture_file);
    }
    if own.explosion_config.is_some() {
        def.explosion_config.clone_from(&own.explosion_config);
    }
    resolved[i] = Some(def);
    Ok(())
}

fn parse_argb(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim(), 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<Materials>
      <CellData name="air" wang_color="ff000042"></CellData>
      <CellData name="sand" tags="[sand_ground],[corrodible]" density="6" cell_type="liquid"
                liquid_sand="1" liquid_gravity="0.9">
        <Graphics texture_file="data/materials_gfx/sand.png" color="ffb89e57"/>
      </CellData>
      <CellDataChild _parent="sand" name="sand_blue" density="7">
        <Graphics color="ff008240"/>
      </CellDataChild>
      <Reaction probability="10"/>
    </Materials>"#;

    #[test]
    fn parses_attributes_graphics_and_tags() {
        let m = parse(XML).unwrap();
        assert_eq!(
            m.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            ["air", "sand", "sand_blue"]
        );
        let sand = &m[1];
        assert_eq!(sand.attr("cell_type"), Some("liquid"));
        assert_eq!(sand.attr_f32("density"), Some(6.0));
        assert_eq!(sand.attr_flag("liquid_sand"), Some(true));
        assert_eq!(sand.attr_flag("liquid_static"), None);
        assert_eq!(sand.graphics_color, Some(0xffb8_9e57));
        assert_eq!(
            sand.tags().collect::<Vec<_>>(),
            ["sand_ground", "corrodible"]
        );
        assert_eq!(m[0].graphics_color, None);
    }

    #[test]
    fn parses_reactions_and_nested_explosion_config_in_document_order() {
        let xml = r#"<Materials>
                    <Reaction probability="50" input_cell1="sodium" input_cell2="[water]">
                        <ExplosionConfig ray_energy="50000" cell_explosion_power="30"/>
                    </Reaction>
                    <Reaction probability="70" output_cell1="concrete_static"/>
                </Materials>"#;

        let reactions = parse_reactions(xml).unwrap();
        assert_eq!(reactions.len(), 2);
        assert_eq!(
            reactions[0].attrs.get("input_cell2").map(String::as_str),
            Some("[water]")
        );
        assert_eq!(
            reactions[0]
                .explosion_config
                .as_ref()
                .and_then(|config| config.get("ray_energy"))
                .map(String::as_str),
            Some("50000")
        );
        assert_eq!(
            reactions[1].attrs.get("probability").map(String::as_str),
            Some("70")
        );
        assert_eq!(reactions[1].explosion_config, None);
    }

    #[test]
    fn child_inherits_and_overrides_parent() {
        let m = parse(XML).unwrap();
        let blue = &m[2];
        assert_eq!(blue.attr_f32("density"), Some(7.0));
        assert_eq!(blue.attr_flag("liquid_sand"), Some(true));
        assert_eq!(blue.attr("_parent"), Some("sand"));
        assert_eq!(blue.graphics_color, Some(0xff00_8240));
        assert_eq!(
            blue.texture_file.as_deref(),
            Some("data/materials_gfx/sand.png")
        );
    }

    #[test]
    fn unknown_parent_is_an_error() {
        let xml = r#"<Materials><CellDataChild _parent="nope" name="x"/></Materials>"#;
        assert!(matches!(
            parse(xml),
            Err(MaterialsError::UnknownParent { .. })
        ));
    }

    #[test]
    fn parent_may_be_defined_after_child() {
        let xml = r#"<Materials>
            <CellDataChild _parent="late" name="kid" hp="5"/>
            <CellData name="late" density="3" hp="1"/>
        </Materials>"#;
        let m = parse(xml).unwrap();
        assert_eq!(m[0].name, "kid");
        assert_eq!(m[0].attr_f32("density"), Some(3.0));
        assert_eq!(m[0].attr_f32("hp"), Some(5.0));
    }

    #[test]
    fn inheritance_cycle_is_an_error() {
        let xml = r#"<Materials>
            <CellDataChild _parent="b" name="a"/>
            <CellDataChild _parent="a" name="b"/>
        </Materials>"#;
        assert!(matches!(
            parse(xml),
            Err(MaterialsError::InheritanceCycle { .. })
        ));
    }
}

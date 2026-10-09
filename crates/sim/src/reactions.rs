//! Reaction tables built from `materials.xml` `<Reaction>` records (CellFactory, expansion
//! `0x703790`, insertion `0x7065c0`, record layout from the serializer `0x701d60`).

use std::collections::HashMap;

use crate::{CellType, MaterialId, MaterialProps, ReactionProps};

/// `+0x38`: `direction` of a record; the neighbor must lie on this side of the reacting cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Top,
    Bottom,
    Left,
    Right,
}

/// One 0x44-byte table record.
#[derive(Clone, Debug, PartialEq)]
pub struct Reaction {
    /// `+0x04`: the XML percent times 100, compared with `rand() % 10001`.
    pub probability: i32,
    /// `+0x08` / `+0x0C`: the reacting cell and its neighbor.
    pub input1: MaterialId,
    pub input2: MaterialId,
    /// `+0x10` / `+0x14`
    pub output1: MaterialId,
    pub output2: MaterialId,
    /// `+0x18..+0x20`: an extra input cell and what it becomes.
    pub third: Option<(MaterialId, Option<MaterialId>)>,
    /// `+0x24`: `cosmetic_particle`; blob cells then draw two global `rand()`s each.
    pub cosmetic_particle: Option<MaterialId>,
    /// `+0x28`
    pub req_lifetime: i32,
    /// `+0x2C` / `+0x2D`: nonzero radii queue a blob instead of changing the cell.
    pub blob_radius1: u8,
    pub blob_radius2: u8,
    /// `+0x2E` / `+0x2F`: `blob_restrict_to_input_material1/2`.
    pub blob_restrict1: bool,
    pub blob_restrict2: bool,
    /// `+0x30`: `destroy_horizontally_lonely_pixels`.
    pub destroy_lonely_pixels: bool,
    /// `+0x31`
    pub convert_all: bool,
    pub direction: Option<Direction>,
    /// `+0x3C`: an `ExplosionConfig` is attached (unported).
    pub has_explosion: bool,
}

/// The normal (`+0x2C`) and fast (`+0x58`) tables, keyed by (input1, input2), and the largest
/// record probability per input1 material (`CellData+0x228` / `+0x22C`). The required table
/// (`+0x84`) is keyed by input1 alone; its maximum is `CellData+0x230`.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReactionTables {
    normal: HashMap<(MaterialId, MaterialId), Vec<Reaction>>,
    fast: HashMap<(MaterialId, MaterialId), Vec<Reaction>>,
    required: HashMap<MaterialId, Vec<Reaction>>,
    max_normal: Vec<i32>,
    max_fast: Vec<i32>,
    max_required: Vec<i32>,
}

const FIELDS: [&str; 6] = [
    "input_cell1",
    "input_cell2",
    "input_cell3",
    "output_cell1",
    "output_cell2",
    "output_cell3",
];

impl ReactionTables {
    pub(crate) fn build(props: &[MaterialProps], raw: &[ReactionProps]) -> Self {
        let mut tables = Self {
            max_normal: vec![0; props.len()],
            max_fast: vec![0; props.len()],
            max_required: vec![0; props.len()],
            ..Self::default()
        };
        let names = sorted_names(props);
        for record in raw {
            let fields = FIELDS.map(|k| record.attrs.get(k).map(|v| v.trim().to_owned()));
            let cosmetic = record
                .attrs
                .get("cosmetic_particle")
                .and_then(|name| props.iter().rposition(|m| m.name == name.trim()))
                .map(|i| MaterialId(i as u16));
            expand(props, &names, fields, &mut |resolved| {
                tables.insert(record, resolved, cosmetic);
            });
        }
        tables
    }

    /// `Reactions_AddRequired` (`0x706da0`) for each `<ReqReaction>`: the record goes into its
    /// input1's list unless it leaves the material unchanged, and raises `CellData+0x230`.
    pub(crate) fn add_required(&mut self, props: &[MaterialProps], raw: &[ReactionProps]) {
        if self.max_required.len() < props.len() {
            self.max_required.resize(props.len(), 0);
        }
        let names = sorted_names(props);
        for record in raw {
            let fields = FIELDS.map(|k| record.attrs.get(k).map(|v| v.trim().to_owned()));
            expand(props, &names, fields, &mut |ids| {
                let (Some(input1), Some(_)) = (ids[0], ids[1]) else {
                    return;
                };
                let reaction = make_reaction(record, ids, None);
                if reaction.output1 == input1 {
                    return;
                }
                let slot = &mut self.max_required[usize::from(input1.0)];
                *slot = (*slot).max(reaction.probability);
                self.required.entry(input1).or_default().push(reaction);
            });
        }
    }

    /// `0x709d90`: the records of `id` whose probability is at least `roll`, in table order.
    pub(crate) fn required_for(&self, id: MaterialId, roll: i32) -> Vec<&Reaction> {
        self.required
            .get(&id)
            .map(|list| list.iter().filter(|r| roll <= r.probability).collect())
            .unwrap_or_default()
    }

    /// `CellData+0x230`.
    pub(crate) fn max_required_probability(&self, id: MaterialId) -> i32 {
        self.max_required
            .get(usize::from(id.0))
            .copied()
            .unwrap_or(0)
    }

    fn insert(
        &mut self,
        record: &ReactionProps,
        ids: [Option<MaterialId>; 6],
        cosmetic_particle: Option<MaterialId>,
    ) {
        let attr = |k: &str| record.attrs.get(k).map(|v| v.trim());
        let flag = |k: &str| attr(k).is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        let (Some(input1), Some(_)) = (ids[0], ids[1]) else {
            return;
        };
        let reaction = make_reaction(record, ids, cosmetic_particle);
        let input2 = reaction.input2;
        let fast = flag("fast_reaction");
        let (table, max) = if fast {
            (&mut self.fast, &mut self.max_fast)
        } else {
            (&mut self.normal, &mut self.max_normal)
        };
        let list = table.entry((input1, input2)).or_default();
        // 0x709e10: an identical record is not added twice.
        if list.contains(&reaction) {
            return;
        }
        let probability = reaction.probability;
        list.push(reaction);
        if input1 != crate::AIR {
            let slot = &mut max[usize::from(input1.0)];
            *slot = (*slot).max(probability);
        }
    }

    /// 0x706cc0 / 0x706d30: the record for `roll`, chosen by `roll % count` among the pair's.
    pub(crate) fn lookup(
        &self,
        fast: bool,
        own: MaterialId,
        neighbor: MaterialId,
        roll: i32,
    ) -> Option<&Reaction> {
        let table = if fast { &self.fast } else { &self.normal };
        let list = table.get(&(own, neighbor))?;
        match list.len() {
            0 => None,
            1 => list.first(),
            n => list.get(roll as usize % n),
        }
    }

    pub(crate) fn list(&self, fast: bool, own: MaterialId, neighbor: MaterialId) -> &[Reaction] {
        let table = if fast { &self.fast } else { &self.normal };
        table.get(&(own, neighbor)).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn max_probability(&self, fast: bool, id: MaterialId) -> i32 {
        let max = if fast {
            &self.max_fast
        } else {
            &self.max_normal
        };
        max.get(usize::from(id.0)).copied().unwrap_or(0)
    }
}

/// CellFactory keeps materials in a name-ordered map; tag expansion walks it in that order.
fn sorted_names(props: &[MaterialProps]) -> Vec<&str> {
    let mut names: Vec<&str> = props.iter().skip(1).map(|m| m.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    names
}

fn make_reaction(
    record: &ReactionProps,
    ids: [Option<MaterialId>; 6],
    cosmetic_particle: Option<MaterialId>,
) -> Reaction {
    let attr = |k: &str| record.attrs.get(k).map(|v| v.trim());
    let flag = |k: &str| attr(k).is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let int = |k: &str| attr(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
    let percent = attr("probability")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let (input1, input2) = (ids[0].expect("input1"), ids[1].expect("input2"));
    Reaction {
        probability: (percent * 100.0) as i32,
        input1,
        input2,
        output1: ids[3].unwrap_or(input1),
        output2: ids[4].unwrap_or(input2),
        third: ids[2].map(|input3| (input3, ids[5])),
        cosmetic_particle,
        req_lifetime: int("req_lifetime"),
        blob_radius1: int("blob_radius1") as u8,
        blob_radius2: int("blob_radius2") as u8,
        blob_restrict1: flag("blob_restrict_to_input_material1"),
        blob_restrict2: flag("blob_restrict_to_input_material2"),
        destroy_lonely_pixels: flag("destroy_horizontally_lonely_pixels"),
        convert_all: flag("convert_all"),
        direction: match attr("direction") {
            Some("top") => Some(Direction::Top),
            Some("bottom") => Some(Direction::Bottom),
            Some("left") => Some(Direction::Left),
            Some("right") => Some(Direction::Right),
            _ => None,
        },
        has_explosion: record.explosion_config.is_some(),
    }
}

/// Substitutes the first `[tag]` field with each matching material (in name order), replacing
/// every field holding the same tag, then recurses; fully named records are resolved to ids.
fn expand(
    props: &[MaterialProps],
    names: &[&str],
    fields: [Option<String>; 6],
    emit: &mut impl FnMut([Option<MaterialId>; 6]),
) {
    let Some(tag) = fields
        .iter()
        .flatten()
        .find(|v| v.starts_with('[') && v.ends_with(']'))
        .cloned()
    else {
        let find = |name: &str| {
            props
                .iter()
                .rposition(|m| m.name == name)
                .map(|i| MaterialId(i as u16))
        };
        let mut ids = [None; 6];
        for (slot, field) in ids.iter_mut().zip(&fields) {
            if let Some(name) = field {
                // An unknown material drops the record, as in 0x703790.
                let Some(id) = find(name) else {
                    return;
                };
                *slot = Some(id);
            }
        }
        emit(ids);
        return;
    };
    let inner = &tag[1..tag.len() - 1];
    for &name in names {
        let Some(material) = props.iter().rev().find(|m| m.name == name) else {
            continue;
        };
        let matches = if inner == "any_liquid" {
            material.cell_type == CellType::Liquid && !material.liquid_sand
        } else {
            material.tags.iter().any(|t| t == inner)
        };
        if !matches {
            continue;
        }
        let substituted = fields
            .clone()
            .map(|f| f.map(|v| if v == tag { name.to_owned() } else { v }));
        expand(props, names, substituted, emit);
    }
}

//! Component schemas from `component_documentation.txt` (the modding tools dump): the member
//! names, types and defaults of every component type.
//!
//! Layout of the file: a component name at column 0, then sections ` - Members`, ` - Privates`,
//! ` - Objects` and ` - Custom data types`, each followed by indented rows of
//! `type  name  default [min, max]  "description"` separated by runs of spaces.

use std::collections::BTreeMap;

/// A typed value of a component field.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i32),
    UInt(u32),
    Int64(i64),
    UInt64(u64),
    Float(f32),
    Double(f64),
    Str(String),
    Vec2(f32, f32),
    IVec2(i32, i32),
    /// Types without a typed representation here (objects, arrays, handles), kept as text.
    Raw(String),
}

/// How a documented type is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Int,
    UInt,
    Int64,
    UInt64,
    Float,
    Double,
    Str,
    Vec2,
    IVec2,
    Raw,
}

impl Kind {
    pub fn of(type_name: &str) -> Kind {
        let t = type_name.trim();
        // A LensValue<T> is a T whose effective value can be modified by game effects.
        let t = t
            .strip_prefix("LensValue<")
            .and_then(|inner| inner.strip_suffix('>'))
            .unwrap_or(t)
            .trim();
        match t {
            "bool" => Kind::Bool,
            "int" | "int32" | "int32_t" | "EntityID" | "EntityTypeID" => Kind::Int,
            "uint32" | "uint32_t" | "unsigned int" => Kind::UInt,
            "int64" => Kind::Int64,
            "uint64" => Kind::UInt64,
            "float" => Kind::Float,
            "double" => Kind::Double,
            "std::string" => Kind::Str,
            "vec2" => Kind::Vec2,
            "ivec2" => Kind::IVec2,
            _ => Kind::Raw,
        }
    }

    /// Converts attribute or default text; unparsable numbers become 0 like the engine's reader.
    pub fn parse(self, text: &str) -> Value {
        let t = text.trim();
        match self {
            Kind::Bool => Value::Bool(matches!(t, "1" | "true" | "True")),
            Kind::Int => Value::Int(number(t) as i32),
            Kind::UInt => Value::UInt(number(t) as u32),
            Kind::Int64 => Value::Int64(number(t) as i64),
            Kind::UInt64 => Value::UInt64(number(t) as u64),
            Kind::Float => Value::Float(number(t) as f32),
            Kind::Double => Value::Double(number(t)),
            Kind::Str => Value::Str(if t == "-" {
                String::new()
            } else {
                text.to_owned()
            }),
            Kind::Vec2 => Value::Vec2(0.0, 0.0),
            Kind::IVec2 => Value::IVec2(0, 0),
            Kind::Raw => Value::Raw(text.to_owned()),
        }
    }
}

fn number(t: &str) -> f64 {
    t.parse::<f64>().unwrap_or(0.0)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub type_name: String,
    pub name: String,
    pub kind: Kind,
    pub default: Value,
    pub private: bool,
    pub doc: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComponentSchema {
    pub name: String,
    pub fields: Vec<Field>,
}

impl ComponentSchema {
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Schema {
    components: BTreeMap<String, ComponentSchema>,
}

impl Schema {
    pub fn parse(text: &str) -> Schema {
        let mut schema = Schema::default();
        let mut current: Option<ComponentSchema> = None;
        let mut private = false;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if !line.starts_with(' ') {
                if let Some(done) = current.take() {
                    schema.components.insert(done.name.clone(), done);
                }
                current = Some(ComponentSchema {
                    name: line.trim().to_owned(),
                    fields: Vec::new(),
                });
            } else if let Some(section) = line.trim_start().strip_prefix("- ") {
                private = section.starts_with("Privates");
            } else if let Some(c) = current.as_mut()
                && let Some(field) = parse_row(line, private)
            {
                c.fields.push(field);
            }
        }
        if let Some(done) = current {
            schema.components.insert(done.name.clone(), done);
        }
        schema
    }

    pub fn component(&self, name: &str) -> Option<&ComponentSchema> {
        self.components.get(name)
    }

    pub fn components(&self) -> impl Iterator<Item = &ComponentSchema> {
        self.components.values()
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
}

/// Splits on runs of two or more spaces.
fn columns(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut last_non_space = 0;
    while i < bytes.len() {
        if bytes[i] != b' ' {
            if start.is_none() {
                start = Some(i);
            }
            last_non_space = i + 1;
            i += 1;
        } else if i + 1 < bytes.len() && bytes[i + 1] == b' ' {
            if let Some(s) = start.take() {
                out.push(&line[s..last_non_space]);
            }
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    if let Some(s) = start {
        out.push(&line[s..last_non_space]);
    }
    out
}

fn parse_row(line: &str, private: bool) -> Option<Field> {
    let cols = columns(line);
    if cols.len() < 3 {
        return None;
    }
    let type_name = cols[0].to_owned();
    let name = cols[1].to_owned();
    // The rest of the row: `default [min, max]  "description"`. A long default can leave only
    // one space before the range, so look for the range bracket instead of the column gap.
    let name_end = line.find(cols[1])? + cols[1].len();
    let rest = line[name_end..].trim();
    let (default_text, doc) = match rest.rfind(" [").filter(|&p| {
        rest[p..]
            .find(']')
            .is_some_and(|e| rest[p..p + e].contains(','))
    }) {
        Some(p) => {
            let end = p + rest[p..].find(']').unwrap_or(0) + 1;
            (
                rest[..p].trim(),
                rest[end..].trim().trim_matches('"').to_owned(),
            )
        }
        None => (
            cols[2].trim(),
            cols.get(3)
                .map(|d| d.trim_matches('"').to_owned())
                .unwrap_or_default(),
        ),
    };
    let kind = Kind::of(&type_name);
    let default = kind.parse(default_text);
    Some(Field {
        type_name,
        name,
        kind,
        default,
        private,
        doc,
    })
}

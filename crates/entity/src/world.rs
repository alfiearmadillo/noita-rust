//! The live entity store: instantiated entities and components with typed values.
//!
//! Values come from the component schema ([`Schema`]): every documented member starts at its
//! documented default, XML attributes override them, and `vec2` members are written in XML as
//! `name_x` / `name_y`. Nested elements (`<damage_multipliers fire="2"/>`) are stored as
//! `object.field` keys; their types are not documented, so numbers become floats and everything
//! else text. `$[a-b]` in an attribute is replaced by a random integer when instantiating.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::schema::{Kind, Schema, Value};
use crate::{EntityDef, Node};

pub type EntityId = i32;
pub type ComponentId = i32;

#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    pub id: ComponentId,
    pub entity: EntityId,
    pub type_name: String,
    pub enabled: bool,
    pub tags: Vec<String>,
    values: BTreeMap<String, Value>,
}

impl Component {
    pub fn value(&self, field: &str) -> Option<&Value> {
        self.values.get(field)
    }

    pub fn set_value(&mut self, field: &str, value: Value) {
        self.values.insert(field.to_owned(), value);
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    pub fn float(&self, field: &str) -> Option<f64> {
        match self.value(field)? {
            Value::Float(f) => Some(f64::from(*f)),
            Value::Double(d) => Some(*d),
            Value::Int(i) => Some(f64::from(*i)),
            Value::UInt(u) => Some(f64::from(*u)),
            _ => None,
        }
    }

    pub fn string(&self, field: &str) -> Option<&str> {
        match self.value(field)? {
            Value::Str(s) | Value::Raw(s) => Some(s),
            _ => None,
        }
    }

    pub fn boolean(&self, field: &str) -> Option<bool> {
        match self.value(field)? {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn fields(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub id: EntityId,
    pub name: String,
    /// The file it was loaded from (`GetFilename`), empty for entities made in code.
    pub filename: String,
    pub tags: Vec<String>,
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub parent: Option<EntityId>,
    pub children: Vec<EntityId>,
    pub components: Vec<ComponentId>,
}

impl Entity {
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

pub struct World {
    schema: Arc<Schema>,
    entities: BTreeMap<EntityId, Entity>,
    components: BTreeMap<ComponentId, Component>,
    next_entity: EntityId,
    next_component: ComponentId,
}

/// Replaces every `$[a-b]` with a random integer in `a..=b` drawn from `rng(a, b)`.
pub fn expand_ranges(text: &str, rng: &mut dyn FnMut(i32, i32) -> i32) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("$[") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let range = after.find(']').and_then(|end| {
            let (a, b) = after[..end].split_once('-')?;
            Some((
                a.trim().parse::<i32>().ok()?,
                b.trim().parse::<i32>().ok()?,
                end,
            ))
        });
        match range {
            Some((a, b, end)) => {
                out.push_str(&rng(a, b).to_string());
                rest = &after[end + 1..];
            }
            None => {
                out.push_str("$[");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn guess(text: &str) -> Value {
    match text.trim().parse::<f32>() {
        Ok(f) if !text.trim().is_empty() => Value::Float(f),
        _ => Value::Str(text.to_owned()),
    }
}

impl World {
    pub fn new(schema: Arc<Schema>) -> Self {
        Self {
            schema,
            entities: BTreeMap::new(),
            components: BTreeMap::new(),
            next_entity: 1,
            next_component: 1,
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    pub fn entity_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.get_mut(&id)
    }

    pub fn component(&self, id: ComponentId) -> Option<&Component> {
        self.components.get(&id)
    }

    pub fn component_mut(&mut self, id: ComponentId) -> Option<&mut Component> {
        self.components.get_mut(&id)
    }

    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// An empty entity at the origin.
    pub fn create_entity(&mut self, name: &str) -> EntityId {
        let id = self.next_entity;
        self.next_entity += 1;
        self.entities.insert(
            id,
            Entity {
                id,
                name: name.to_owned(),
                filename: String::new(),
                tags: Vec::new(),
                x: 0.0,
                y: 0.0,
                rotation: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                parent: None,
                children: Vec::new(),
                components: Vec::new(),
            },
        );
        id
    }

    /// Instantiates a resolved template at a position. `rng(a, b)` draws for `$[a-b]` ranges.
    pub fn instantiate(
        &mut self,
        def: &EntityDef,
        filename: &str,
        position: (f32, f32),
        rng: &mut dyn FnMut(i32, i32) -> i32,
    ) -> EntityId {
        let id = self.create_entity(&expand_ranges(def.name(), rng));
        let tags: Vec<String> = def.tags().iter().map(|t| (*t).to_owned()).collect();
        if let Some(e) = self.entities.get_mut(&id) {
            e.filename = filename.to_owned();
            e.tags = tags;
            e.x = position.0;
            e.y = position.1;
            if let Some(t) = &def.transform {
                let f =
                    |k: &str, d: f32| t.attr(k).and_then(|v| v.trim().parse().ok()).unwrap_or(d);
                // The file's `_Transform` position is an offset from the position passed in.
                e.x += f("position.x", 0.0);
                e.y += f("position.y", 0.0);
                e.rotation = f("rotation", 0.0);
                e.scale_x = f("scale.x", 1.0);
                e.scale_y = f("scale.y", 1.0);
            }
        }
        for node in &def.components {
            self.add_component(id, node, rng);
        }
        for child in &def.children {
            let child_id = self.instantiate(child, "", (position.0, position.1), rng);
            self.attach(id, child_id);
        }
        id
    }

    /// Makes `child` a child of `parent`.
    pub fn attach(&mut self, parent: EntityId, child: EntityId) {
        if let Some(c) = self.entities.get_mut(&child) {
            c.parent = Some(parent);
        }
        if let Some(p) = self.entities.get_mut(&parent) {
            p.children.push(child);
        }
    }

    /// Adds a component built from an XML element.
    pub fn add_component(
        &mut self,
        entity: EntityId,
        node: &Node,
        rng: &mut dyn FnMut(i32, i32) -> i32,
    ) -> ComponentId {
        let id = self.next_component;
        self.next_component += 1;
        let schema = self.schema.component(&node.tag).cloned();
        let mut component = Component {
            id,
            entity,
            type_name: node.tag.clone(),
            enabled: true,
            tags: Vec::new(),
            values: BTreeMap::new(),
        };
        if let Some(s) = &schema {
            for f in &s.fields {
                component.values.insert(f.name.clone(), f.default.clone());
            }
        }
        for (key, raw) in &node.attrs {
            let text = expand_ranges(raw, rng);
            match key.as_str() {
                "_enabled" => component.enabled = matches!(text.trim(), "1" | "true"),
                "_tags" => {
                    component.tags = text
                        .split(',')
                        .map(|t| t.trim().to_owned())
                        .filter(|t| !t.is_empty())
                        .collect();
                }
                _ => set_attribute(&mut component, schema.as_ref(), key, &text),
            }
        }
        for child in &node.children {
            store_object(&mut component, &child.tag, child, rng);
        }
        if let Some(e) = self.entities.get_mut(&entity) {
            e.components.push(id);
        }
        self.components.insert(id, component);
        id
    }

    /// The enabled-or-not components of an entity with a type name, in creation order.
    pub fn components_of(&self, entity: EntityId, type_name: &str) -> Vec<ComponentId> {
        self.entities
            .get(&entity)
            .map(|e| {
                e.components
                    .iter()
                    .copied()
                    .filter(|c| {
                        self.components
                            .get(c)
                            .is_some_and(|c| c.type_name == type_name)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Removes one component from its entity.
    pub fn remove_component(&mut self, id: ComponentId) {
        if let Some(c) = self.components.remove(&id)
            && let Some(e) = self.entities.get_mut(&c.entity)
        {
            e.components.retain(|&x| x != id);
        }
    }

    /// Removes an entity, its components and its children.
    pub fn kill_entity(&mut self, id: EntityId) {
        let Some(entity) = self.entities.remove(&id) else {
            return;
        };
        for c in &entity.components {
            self.components.remove(c);
        }
        if let Some(parent) = entity.parent
            && let Some(p) = self.entities.get_mut(&parent)
        {
            p.children.retain(|&c| c != id);
        }
        for child in entity.children {
            self.kill_entity(child);
        }
    }

    /// Entities carrying a tag, by id.
    pub fn with_tag(&self, tag: &str) -> Vec<EntityId> {
        self.entities
            .values()
            .filter(|e| e.has_tag(tag))
            .map(|e| e.id)
            .collect()
    }
}

fn set_attribute(
    component: &mut Component,
    schema: Option<&crate::schema::ComponentSchema>,
    key: &str,
    text: &str,
) {
    if let Some(field) = schema.and_then(|s| s.field(key)) {
        component
            .values
            .insert(key.to_owned(), field.kind.parse(text));
        return;
    }
    // vec2 members are written as name_x / name_y.
    for (suffix, axis) in [("_x", 0), ("_y", 1)] {
        if let Some(stem) = key.strip_suffix(suffix)
            && let Some(field) = schema.and_then(|s| s.field(stem))
            && matches!(field.kind, Kind::Vec2 | Kind::IVec2)
        {
            let n = text.trim().parse::<f64>().unwrap_or(0.0);
            let value = match (field.kind, component.values.get(stem)) {
                (Kind::Vec2, Some(Value::Vec2(x, y))) => {
                    if axis == 0 {
                        Value::Vec2(n as f32, *y)
                    } else {
                        Value::Vec2(*x, n as f32)
                    }
                }
                (Kind::IVec2, Some(Value::IVec2(x, y))) => {
                    if axis == 0 {
                        Value::IVec2(n as i32, *y)
                    } else {
                        Value::IVec2(*x, n as i32)
                    }
                }
                (Kind::Vec2, _) => Value::Vec2(
                    if axis == 0 { n as f32 } else { 0.0 },
                    if axis == 1 { n as f32 } else { 0.0 },
                ),
                _ => Value::IVec2(
                    if axis == 0 { n as i32 } else { 0 },
                    if axis == 1 { n as i32 } else { 0 },
                ),
            };
            component.values.insert(stem.to_owned(), value);
            return;
        }
    }
    component.values.insert(key.to_owned(), guess(text));
}

fn store_object(
    component: &mut Component,
    prefix: &str,
    node: &Node,
    rng: &mut dyn FnMut(i32, i32) -> i32,
) {
    for (k, v) in &node.attrs {
        let text = expand_ranges(v, rng);
        component
            .values
            .insert(format!("{prefix}.{k}"), guess(&text));
    }
    for child in &node.children {
        store_object(component, &format!("{prefix}.{}", child.tag), child, rng);
    }
}

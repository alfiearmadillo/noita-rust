//! The entity and component script API (`EntityLoad`, `EntityGetFirstComponent`,
//! `ComponentGetValue2`, `GetUpdatedEntityID`, ...), implemented on `entity::world::World`.
//! Signatures follow `lua_api_documentation.txt`; the engine handlers are not decompiled yet.

use entity::schema::Value;
use entity::world::{ComponentId, EntityId};
use mlua::{Lua, MultiValue, Value as Lv, Variadic};
use sim::random::cvtt32;

use crate::ScriptState;

fn state(lua: &Lua) -> mlua::Result<mlua::AppDataRef<'_, ScriptState>> {
    lua.app_data_ref::<ScriptState>()
        .ok_or_else(|| mlua::Error::runtime("script state missing"))
}

fn num(v: f64) -> i32 {
    cvtt32(v)
}

fn lua_number(v: &Lv) -> Option<f64> {
    match v {
        Lv::Number(n) => Some(*n),
        Lv::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

/// A component value as Lua values (a vec2 is two numbers).
fn to_lua(lua: &Lua, v: &Value) -> mlua::Result<Vec<Lv>> {
    Ok(match v {
        Value::Bool(b) => vec![Lv::Boolean(*b)],
        Value::Int(i) => vec![Lv::Number(f64::from(*i))],
        Value::UInt(u) => vec![Lv::Number(f64::from(*u))],
        Value::Int64(i) => vec![Lv::Number(*i as f64)],
        Value::UInt64(u) => vec![Lv::Number(*u as f64)],
        Value::Float(f) => vec![Lv::Number(f64::from(*f))],
        Value::Double(d) => vec![Lv::Number(*d)],
        Value::Str(s) | Value::Raw(s) => vec![Lv::String(lua.create_string(s)?)],
        Value::Vec2(x, y) => vec![Lv::Number(f64::from(*x)), Lv::Number(f64::from(*y))],
        Value::IVec2(x, y) => vec![Lv::Number(f64::from(*x)), Lv::Number(f64::from(*y))],
    })
}

/// Converts Lua arguments to a value of the same kind as `like` (or by Lua type when unknown).
fn from_lua(like: Option<&Value>, args: &[Lv]) -> Option<Value> {
    let first = args.first()?;
    let n = lua_number(first);
    Some(match like {
        Some(Value::Bool(_)) => Value::Bool(match first {
            Lv::Boolean(b) => *b,
            other => lua_number(other).is_some_and(|n| n != 0.0),
        }),
        Some(Value::Int(_)) => Value::Int(num(n?)),
        Some(Value::UInt(_)) => Value::UInt(n? as u32),
        Some(Value::Int64(_)) => Value::Int64(n? as i64),
        Some(Value::UInt64(_)) => Value::UInt64(n? as u64),
        Some(Value::Float(_)) => Value::Float(n? as f32),
        Some(Value::Double(_)) => Value::Double(n?),
        Some(Value::Vec2(..)) => Value::Vec2(n? as f32, args.get(1).and_then(lua_number)? as f32),
        Some(Value::IVec2(..)) => Value::IVec2(num(n?), num(args.get(1).and_then(lua_number)?)),
        Some(Value::Str(_) | Value::Raw(_)) | None => match first {
            Lv::Boolean(b) => Value::Bool(*b),
            Lv::String(s) => Value::Str(s.to_string_lossy()),
            other => Value::Float(lua_number(other)? as f32),
        },
    })
}

fn text_of(v: &Value) -> String {
    match v {
        Value::Bool(b) => if *b { "1" } else { "0" }.to_owned(),
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        Value::Int64(i) => i.to_string(),
        Value::UInt64(u) => u.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Double(d) => d.to_string(),
        Value::Str(s) | Value::Raw(s) => s.clone(),
        Value::Vec2(x, y) => format!("{x},{y}"),
        Value::IVec2(x, y) => format!("{x},{y}"),
    }
}

fn ids_table(lua: &Lua, ids: Vec<i32>) -> mlua::Result<Lv> {
    Ok(Lv::Table(
        lua.create_sequence_from(ids.into_iter().map(f64::from))?,
    ))
}

/// A table of ids, or nil when empty (what `EntityGetComponent` / `EntityGetAllChildren` return).
fn ids_or_nil(lua: &Lua, ids: Vec<i32>) -> mlua::Result<Lv> {
    if ids.is_empty() {
        Ok(Lv::Nil)
    } else {
        ids_table(lua, ids)
    }
}

/// Entities within `radius` of a point (by their position; the edge counts as inside).
fn in_radius(lua: &Lua, x: f64, y: f64, radius: f64, tag: Option<&str>) -> mlua::Result<Vec<i32>> {
    let s = state(lua)?;
    let w = s.world.borrow();
    Ok(w.entities()
        .filter(|e| tag.is_none_or(|t| e.has_tag(t)))
        .filter(|e| {
            let (dx, dy) = (f64::from(e.x) - x, f64::from(e.y) - y);
            dx * dx + dy * dy <= radius * radius
        })
        .map(|e| e.id)
        .collect())
}

/// The nearest entity to a point (0 when there is none).
fn closest(lua: &Lua, x: f64, y: f64, tag: Option<&str>) -> mlua::Result<i32> {
    let s = state(lua)?;
    let w = s.world.borrow();
    let dist = |e: &entity::world::Entity| {
        let (dx, dy) = (f64::from(e.x) - x, f64::from(e.y) - y);
        dx * dx + dy * dy
    };
    Ok(w.entities()
        .filter(|e| tag.is_none_or(|t| e.has_tag(t)))
        .min_by(|a, b| dist(a).total_cmp(&dist(b)))
        .map_or(0, |e| e.id))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    g.set(
        "GetUpdatedEntityID",
        lua.create_function(|lua, ()| Ok(f64::from(state(lua)?.current_entity.get())))?,
    )?;
    g.set(
        "GetUpdatedComponentID",
        lua.create_function(|lua, ()| Ok(f64::from(state(lua)?.current_component.get())))?,
    )?;

    g.set(
        "EntityCreateNew",
        lua.create_function(|lua, name: Option<String>| {
            let s = state(lua)?;
            let id = s
                .world
                .borrow_mut()
                .create_entity(&name.unwrap_or_default());
            Ok(f64::from(id))
        })?,
    )?;
    g.set(
        "EntityLoad",
        lua.create_function(|lua, (file, x, y): (String, Option<f64>, Option<f64>)| {
            let s = state(lua)?;
            let def = s
                .loader
                .borrow_mut()
                .load(&file)
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;
            let mut rng = s.entity_range_rng();
            let id = s.world.borrow_mut().instantiate(
                &def,
                &file,
                (x.unwrap_or(0.0) as f32, y.unwrap_or(0.0) as f32),
                &mut rng,
            );
            Ok(f64::from(id))
        })?,
    )?;
    g.set(
        "EntityKill",
        lua.create_function(|lua, id: f64| {
            state(lua)?.world.borrow_mut().kill_entity(num(id));
            Ok(())
        })?,
    )?;
    g.set(
        "EntityGetIsAlive",
        lua.create_function(|lua, id: f64| {
            Ok(state(lua)?.world.borrow().entity(num(id)).is_some())
        })?,
    )?;
    g.set(
        "EntityGetName",
        lua.create_function(|lua, id: f64| {
            Ok(state(lua)?
                .world
                .borrow()
                .entity(num(id))
                .map(|e| e.name.clone())
                .unwrap_or_default())
        })?,
    )?;
    g.set(
        "EntitySetName",
        lua.create_function(|lua, (id, name): (f64, String)| {
            if let Some(e) = state(lua)?.world.borrow_mut().entity_mut(num(id)) {
                e.name = name;
            }
            Ok(())
        })?,
    )?;
    g.set(
        "EntityGetFilename",
        lua.create_function(|lua, id: f64| {
            Ok(state(lua)?
                .world
                .borrow()
                .entity(num(id))
                .map(|e| e.filename.clone())
                .unwrap_or_default())
        })?,
    )?;

    // Transforms: x, y, rotation, scale_x, scale_y.
    g.set(
        "EntityGetTransform",
        lua.create_function(|lua, id: f64| {
            let s = state(lua)?;
            let w = s.world.borrow();
            let Some(e) = w.entity(num(id)) else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_iter(
                [e.x, e.y, e.rotation, e.scale_x, e.scale_y].map(|v| Lv::Number(f64::from(v))),
            ))
        })?,
    )?;
    g.set(
        "EntitySetTransform",
        lua.create_function(
            |lua,
             (id, x, y, rot, sx, sy): (
                f64,
                f64,
                Option<f64>,
                Option<f64>,
                Option<f64>,
                Option<f64>,
            )| {
                if let Some(e) = state(lua)?.world.borrow_mut().entity_mut(num(id)) {
                    e.x = x as f32;
                    e.y = y.unwrap_or(0.0) as f32;
                    e.rotation = rot.unwrap_or(0.0) as f32;
                    e.scale_x = sx.unwrap_or(1.0) as f32;
                    e.scale_y = sy.unwrap_or(1.0) as f32;
                }
                Ok(())
            },
        )?,
    )?;

    // Tags.
    g.set(
        "EntityGetWithTag",
        lua.create_function(|lua, tag: String| {
            let ids = state(lua)?.world.borrow().with_tag(&tag);
            ids_table(lua, ids)
        })?,
    )?;
    g.set(
        "EntityGetWithName",
        lua.create_function(|lua, name: String| {
            let s = state(lua)?;
            let found = s
                .world
                .borrow()
                .entities()
                .find(|e| e.name == name)
                .map(|e| e.id);
            Ok(f64::from(found.unwrap_or(0)))
        })?,
    )?;
    g.set(
        "EntityAddTag",
        lua.create_function(|lua, (id, tag): (f64, String)| {
            if let Some(e) = state(lua)?.world.borrow_mut().entity_mut(num(id))
                && !e.has_tag(&tag)
            {
                e.tags.push(tag);
            }
            Ok(())
        })?,
    )?;
    g.set(
        "EntityRemoveTag",
        lua.create_function(|lua, (id, tag): (f64, String)| {
            if let Some(e) = state(lua)?.world.borrow_mut().entity_mut(num(id)) {
                e.tags.retain(|t| *t != tag);
            }
            Ok(())
        })?,
    )?;
    g.set(
        "EntityHasTag",
        lua.create_function(|lua, (id, tag): (f64, String)| {
            Ok(state(lua)?
                .world
                .borrow()
                .entity(num(id))
                .is_some_and(|e| e.has_tag(&tag)))
        })?,
    )?;
    g.set(
        "EntityGetTags",
        lua.create_function(|lua, id: f64| {
            Ok(state(lua)?
                .world
                .borrow()
                .entity(num(id))
                .map(|e| e.tags.join(","))
                .unwrap_or_default())
        })?,
    )?;

    // Hierarchy.
    g.set(
        "EntityGetInRadius",
        lua.create_function(|lua, (x, y, r): (f64, f64, f64)| {
            let ids = in_radius(lua, x, y, r, None)?;
            ids_table(lua, ids)
        })?,
    )?;
    g.set(
        "EntityGetInRadiusWithTag",
        lua.create_function(|lua, (x, y, r, tag): (f64, f64, f64, String)| {
            let ids = in_radius(lua, x, y, r, Some(&tag))?;
            ids_table(lua, ids)
        })?,
    )?;
    g.set(
        "EntityGetClosest",
        lua.create_function(|lua, (x, y): (f64, f64)| Ok(f64::from(closest(lua, x, y, None)?)))?,
    )?;
    g.set(
        "EntityGetClosestWithTag",
        lua.create_function(|lua, (x, y, tag): (f64, f64, String)| {
            Ok(f64::from(closest(lua, x, y, Some(&tag))?))
        })?,
    )?;
    g.set(
        "EntityAddChild",
        lua.create_function(|lua, (parent, child): (f64, f64)| {
            state(lua)?
                .world
                .borrow_mut()
                .attach(num(parent), num(child));
            Ok(())
        })?,
    )?;
    g.set(
        "EntityGetParent",
        lua.create_function(|lua, id: f64| {
            Ok(f64::from(
                state(lua)?
                    .world
                    .borrow()
                    .entity(num(id))
                    .and_then(|e| e.parent)
                    .unwrap_or(0),
            ))
        })?,
    )?;
    g.set(
        "EntityGetRootEntity",
        lua.create_function(|lua, id: f64| {
            let s = state(lua)?;
            let w = s.world.borrow();
            let mut cur = num(id);
            while let Some(p) = w.entity(cur).and_then(|e| e.parent) {
                cur = p;
            }
            Ok(f64::from(cur))
        })?,
    )?;
    g.set(
        "EntityGetAllChildren",
        lua.create_function(|lua, (id, tag): (f64, Option<String>)| {
            let s = state(lua)?;
            let w = s.world.borrow();
            let kids: Vec<EntityId> = w
                .entity(num(id))
                .map(|e| e.children.clone())
                .unwrap_or_default()
                .into_iter()
                .filter(|c| {
                    tag.as_deref()
                        .filter(|t| !t.is_empty())
                        .is_none_or(|t| w.entity(*c).is_some_and(|e| e.has_tag(t)))
                })
                .collect();
            ids_or_nil(lua, kids)
        })?,
    )?;

    // Component queries: enabled ones only unless the name says otherwise.
    for (name, include_disabled, first) in [
        ("EntityGetComponent", false, false),
        ("EntityGetFirstComponent", false, true),
        ("EntityGetComponentIncludingDisabled", true, false),
        ("EntityGetFirstComponentIncludingDisabled", true, true),
    ] {
        g.set(
            name,
            lua.create_function(move |lua, (id, ty, tag): (f64, String, Option<String>)| {
                let s = state(lua)?;
                let w = s.world.borrow();
                let ids: Vec<ComponentId> = w
                    .components_of(num(id), &ty)
                    .into_iter()
                    .filter(|c| {
                        w.component(*c).is_some_and(|c| {
                            (include_disabled || c.enabled)
                                && tag
                                    .as_deref()
                                    .filter(|t| !t.is_empty())
                                    .is_none_or(|t| c.has_tag(t))
                        })
                    })
                    .collect();
                if first {
                    Ok(ids.first().map_or(Lv::Nil, |c| Lv::Number(f64::from(*c))))
                } else {
                    ids_or_nil(lua, ids)
                }
            })?,
        )?;
    }
    g.set(
        "EntityGetAllComponents",
        lua.create_function(|lua, id: f64| {
            let ids = state(lua)?
                .world
                .borrow()
                .entity(num(id))
                .map(|e| e.components.clone())
                .unwrap_or_default();
            ids_or_nil(lua, ids)
        })?,
    )?;
    g.set(
        "EntityRemoveComponent",
        lua.create_function(|lua, (_id, comp): (f64, f64)| {
            state(lua)?.world.borrow_mut().remove_component(num(comp));
            Ok(())
        })?,
    )?;
    g.set(
        "EntitySetComponentIsEnabled",
        lua.create_function(|lua, (_id, comp, on): (f64, f64, bool)| {
            if let Some(c) = state(lua)?.world.borrow_mut().component_mut(num(comp)) {
                c.enabled = on;
            }
            Ok(())
        })?,
    )?;
    g.set(
        "EntitySetComponentsWithTagEnabled",
        lua.create_function(|lua, (id, tag, on): (f64, String, bool)| {
            let s = state(lua)?;
            let mut w = s.world.borrow_mut();
            let ids = w
                .entity(num(id))
                .map(|e| e.components.clone())
                .unwrap_or_default();
            for c in ids {
                if let Some(c) = w.component_mut(c)
                    && c.has_tag(&tag)
                {
                    c.enabled = on;
                }
            }
            Ok(())
        })?,
    )?;
    for name in ["EntityAddComponent", "EntityAddComponent2"] {
        g.set(
            name,
            lua.create_function(
                |lua, (id, ty, values): (f64, String, Option<mlua::Table>)| {
                    let s = state(lua)?;
                    let mut node = entity::Node {
                        tag: ty,
                        ..Default::default()
                    };
                    let mut extra: Vec<(String, Vec<Lv>)> = Vec::new();
                    if let Some(t) = values {
                        for pair in t.pairs::<String, Lv>() {
                            let (k, v) = pair?;
                            // Values are applied after creation so their Lua types are kept.
                            extra.push((k, vec![v]));
                        }
                    }
                    node.attrs.clear();
                    let mut rng = s.entity_range_rng();
                    let cid = s.world.borrow_mut().add_component(num(id), &node, &mut rng);
                    let mut w = s.world.borrow_mut();
                    for (k, args) in extra {
                        if k == "_enabled" {
                            if let (Some(c), Some(Lv::Boolean(b))) =
                                (w.component_mut(cid), args.first())
                            {
                                c.enabled = *b;
                            }
                            continue;
                        }
                        let like = w.component(cid).and_then(|c| c.value(&k)).cloned();
                        if let Some(v) = from_lua(like.as_ref(), &args)
                            && let Some(c) = w.component_mut(cid)
                        {
                            c.set_value(&k, v);
                        }
                    }
                    Ok(f64::from(cid))
                },
            )?,
        )?;
    }

    // Component values.
    g.set(
        "ComponentGetValue2",
        lua.create_function(|lua, (comp, field): (f64, String)| {
            let s = state(lua)?;
            let w = s.world.borrow();
            match w.component(num(comp)).and_then(|c| c.value(&field)) {
                Some(v) => Ok(MultiValue::from_iter(to_lua(lua, v)?)),
                None => Err(mlua::Error::runtime(format!(
                    "ComponentGetValue2: component has no field '{field}'"
                ))),
            }
        })?,
    )?;
    g.set(
        "ComponentSetValue2",
        lua.create_function(|lua, (comp, field, values): (f64, String, Variadic<Lv>)| {
            let s = state(lua)?;
            let mut w = s.world.borrow_mut();
            let Some(c) = w.component_mut(num(comp)) else {
                return Err(mlua::Error::runtime(
                    "ComponentSetValue2: no such component",
                ));
            };
            let like = c.value(&field).cloned();
            match from_lua(like.as_ref(), &values) {
                Some(v) => {
                    c.set_value(&field, v);
                    Ok(())
                }
                None => Err(mlua::Error::runtime(format!(
                    "ComponentSetValue2: bad value for '{field}'"
                ))),
            }
        })?,
    )?;
    g.set(
        "ComponentObjectGetValue2",
        lua.create_function(|lua, (comp, obj, field): (f64, String, String)| {
            let s = state(lua)?;
            let w = s.world.borrow();
            let key = format!("{obj}.{field}");
            match w.component(num(comp)).and_then(|c| c.value(&key)) {
                Some(v) => Ok(MultiValue::from_iter(to_lua(lua, v)?)),
                None => Err(mlua::Error::runtime(format!(
                    "ComponentObjectGetValue2: no field '{key}'"
                ))),
            }
        })?,
    )?;
    g.set(
        "ComponentObjectSetValue2",
        lua.create_function(
            |lua, (comp, obj, field, values): (f64, String, String, Variadic<Lv>)| {
                let s = state(lua)?;
                let mut w = s.world.borrow_mut();
                let Some(c) = w.component_mut(num(comp)) else {
                    return Err(mlua::Error::runtime(
                        "ComponentObjectSetValue2: no such component",
                    ));
                };
                let key = format!("{obj}.{field}");
                let like = c.value(&key).cloned();
                if let Some(v) = from_lua(like.as_ref(), &values) {
                    c.set_value(&key, v);
                }
                Ok(())
            },
        )?,
    )?;
    // The string forms.
    g.set(
        "ComponentGetValue",
        lua.create_function(|lua, (comp, field): (f64, String)| {
            let s = state(lua)?;
            let w = s.world.borrow();
            Ok(w.component(num(comp))
                .and_then(|c| c.value(&field))
                .map(text_of)
                .unwrap_or_default())
        })?,
    )?;
    g.set(
        "ComponentSetValue",
        lua.create_function(|lua, (comp, field, text): (f64, String, String)| {
            let s = state(lua)?;
            let mut w = s.world.borrow_mut();
            if let Some(c) = w.component_mut(num(comp)) {
                let value = match c.value(&field) {
                    Some(Value::Bool(_)) => Value::Bool(matches!(text.trim(), "1" | "true")),
                    Some(Value::Int(_)) => {
                        Value::Int(text.trim().parse::<f64>().unwrap_or(0.0) as i32)
                    }
                    Some(Value::Float(_)) => Value::Float(text.trim().parse().unwrap_or(0.0)),
                    Some(Value::Double(_)) => Value::Double(text.trim().parse().unwrap_or(0.0)),
                    _ => Value::Str(text),
                };
                c.set_value(&field, value);
            }
            Ok(())
        })?,
    )?;
    g.set(
        "ComponentGetTypeName",
        lua.create_function(|lua, comp: f64| {
            Ok(state(lua)?
                .world
                .borrow()
                .component(num(comp))
                .map(|c| c.type_name.clone())
                .unwrap_or_default())
        })?,
    )?;
    g.set(
        "ComponentGetEntity",
        lua.create_function(|lua, comp: f64| {
            Ok(f64::from(
                state(lua)?
                    .world
                    .borrow()
                    .component(num(comp))
                    .map_or(0, |c| c.entity),
            ))
        })?,
    )?;
    g.set(
        "ComponentGetIsEnabled",
        lua.create_function(|lua, comp: f64| {
            Ok(state(lua)?
                .world
                .borrow()
                .component(num(comp))
                .is_some_and(|c| c.enabled))
        })?,
    )?;
    g.set(
        "ComponentGetTags",
        lua.create_function(|lua, comp: f64| {
            Ok(state(lua)?
                .world
                .borrow()
                .component(num(comp))
                .map(|c| c.tags.join(","))
                .unwrap_or_default())
        })?,
    )?;
    Ok(())
}

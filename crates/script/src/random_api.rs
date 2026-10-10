//! `SetRandomSeed`, `Random`, `Randomf`, `ProceduralRandom`, `ProceduralRandomf`,
//! `ProceduralRandomi`, `RandomDistribution` and `RandomDistributionf` on top of
//! `sim::random::ScriptRandom` (ported from `0x7c9300`..`0x7cb550`, `0x7ca080`, `0x7ca460`).

use mlua::{Lua, MultiValue, Value};
use sim::random::cvtt32;

use crate::ScriptState;

/// `lua_tonumber`: numbers and numeric strings convert, everything else is 0.
fn number(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => *n,
        Some(Value::Integer(i)) => *i as f64,
        Some(Value::String(s)) => s
            .to_str()
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `lua_tointeger`: the number truncated like `cvttsd2si` (32-bit Lua).
fn integer(v: Option<&Value>) -> i32 {
    cvtt32(number(v))
}

fn requires(usage: &str, needed: usize, given: usize) -> mlua::Error {
    mlua::Error::runtime(format!(
        "{usage} requires {needed} parameters, only {given} given"
    ))
}

fn with_random<T>(
    lua: &Lua,
    f: impl FnOnce(&mut sim::random::ScriptRandom) -> T,
) -> mlua::Result<T> {
    let state = lua
        .app_data_ref::<ScriptState>()
        .ok_or_else(|| mlua::Error::runtime("script state missing"))?;
    let mut random = state.random.borrow_mut();
    Ok(f(&mut random))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    g.set(
        "SetRandomSeed",
        lua.create_function(|lua, args: MultiValue| {
            let usage = "SetRandomSeed( x:number, y:number )";
            if args.len() < 2 {
                return Err(requires(usage, 2, args.len()));
            }
            let (x, y) = (number(args.front()), number(args.get(1)));
            with_random(lua, |r| r.set_random_seed(x, y))
        })?,
    )?;

    // Random() / Random(max) / Random(min, max); Randomf the float counterpart.
    g.set(
        "Random",
        lua.create_function(|lua, args: MultiValue| {
            let value = match args.len() {
                0 => with_random(lua, |r| r.random())?,
                1 => {
                    let max = integer(args.front());
                    f64::from(with_random(lua, |r| r.random_int(0, max))?)
                }
                _ => {
                    let (min, max) = (integer(args.front()), integer(args.get(1)));
                    f64::from(with_random(lua, |r| r.random_int(min, max))?)
                }
            };
            Ok(value)
        })?,
    )?;
    g.set(
        "Randomf",
        lua.create_function(|lua, args: MultiValue| {
            let value = match args.len() {
                0 => with_random(lua, |r| r.random())?,
                1 => {
                    let max = number(args.front()) as f32;
                    f64::from(with_random(lua, |r| r.random_float(0.0, max))?)
                }
                _ => {
                    let (min, max) = (number(args.front()) as f32, number(args.get(1)) as f32);
                    f64::from(with_random(lua, |r| r.random_float(min, max))?)
                }
            };
            Ok(value)
        })?,
    )?;

    // ProceduralRandom: 2 args unit double, 3 args int(0, a), 4 args float(a, b).
    g.set(
        "ProceduralRandom",
        lua.create_function(|lua, args: MultiValue| {
            let usage = "ProceduralRandom( x:number, y:number, a:int|number = optional, b:int|number = optional )";
            if args.len() < 2 {
                return Err(requires(usage, 2, args.len()));
            }
            let (x, y) = (number(args.front()), number(args.get(1)));
            match args.len() {
                2 => with_random(lua, |r| r.procedural_unit(x, y)),
                3 => {
                    let max = integer(args.get(2));
                    Ok(f64::from(with_random(lua, |r| r.procedural_int(x, y, 0, max))?))
                }
                _ => {
                    let (min, max) = (number(args.get(2)) as f32, number(args.get(3)) as f32);
                    Ok(f64::from(with_random(lua, |r| r.procedural_float(x, y, min, max))?))
                }
            }
        })?,
    )?;
    g.set(
        "ProceduralRandomf",
        lua.create_function(|lua, args: MultiValue| {
            let usage =
                "ProceduralRandomf( x:number, y:number, a:number = optional, b:number = optional )";
            if args.len() < 2 {
                return Err(requires(usage, 2, args.len()));
            }
            let (x, y) = (number(args.front()), number(args.get(1)));
            match args.len() {
                2 => with_random(lua, |r| r.procedural_unit(x, y)),
                3 => {
                    let max = number(args.get(2)) as f32;
                    Ok(f64::from(with_random(lua, |r| {
                        r.procedural_float(x, y, 0.0, max)
                    })?))
                }
                _ => {
                    let (min, max) = (number(args.get(2)) as f32, number(args.get(3)) as f32);
                    Ok(f64::from(with_random(lua, |r| {
                        r.procedural_float(x, y, min, max)
                    })?))
                }
            }
        })?,
    )?;
    g.set(
        "ProceduralRandomi",
        lua.create_function(|lua, args: MultiValue| {
            let usage =
                "ProceduralRandomi(  x:number, y:number, a:int = optional, b:int = optional )";
            if args.len() < 2 {
                return Err(requires(usage, 2, args.len()));
            }
            let (x, y) = (number(args.front()), number(args.get(1)));
            let value = match args.len() {
                2 => with_random(lua, |r| r.procedural_bit(x, y))?,
                3 => {
                    let max = integer(args.get(2));
                    with_random(lua, |r| r.procedural_int(x, y, 0, max))?
                }
                _ => {
                    let (min, max) = (integer(args.get(2)), integer(args.get(3)));
                    with_random(lua, |r| r.procedural_int(x, y, min, max))?
                }
            };
            Ok(f64::from(value))
        })?,
    )?;

    // RandomDistribution( min, max, mean, sharpness = 1, baseline = 0.005 ), ints and floats.
    g.set(
        "RandomDistribution",
        lua.create_function(|lua, args: MultiValue| {
            let usage = "RandomDistribution( min:int, max:int, mean:int, sharpness:number = 1, baseline:number = 0.005 ) -> int";
            if args.len() < 3 {
                return Err(requires(usage, 3, args.len()));
            }
            let (min, max, mean) = (
                integer(args.front()),
                integer(args.get(1)),
                integer(args.get(2)),
            );
            let sharpness = if args.len() >= 4 { number(args.get(3)) as f32 } else { 1.0 };
            let baseline = if args.len() >= 5 { number(args.get(4)) as f32 } else { 0.005 };
            Ok(f64::from(with_random(lua, |r| {
                r.random_distribution(min, max, mean, sharpness, baseline)
            })?))
        })?,
    )?;
    g.set(
        "RandomDistributionf",
        lua.create_function(|lua, args: MultiValue| {
            let usage = "RandomDistributionf( min:number, max:number, mean:number, sharpness:number = 1, baseline:number = 0.005 ) -> number";
            if args.len() < 3 {
                return Err(requires(usage, 3, args.len()));
            }
            let (min, max, mean) = (
                number(args.front()) as f32,
                number(args.get(1)) as f32,
                number(args.get(2)) as f32,
            );
            let sharpness = if args.len() >= 4 { number(args.get(3)) as f32 } else { 1.0 };
            let baseline = if args.len() >= 5 { number(args.get(4)) as f32 } else { 0.005 };
            Ok(f64::from(with_random(lua, |r| {
                r.random_distribution_float(min, max, mean, sharpness, baseline)
            })?))
        })?,
    )?;
    Ok(())
}

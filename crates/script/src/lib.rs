//! Lua runtime for the reference game scripts: a LuaJIT state with the reference's script
//! loading rules (`dofile_once`, a virtual file system rooted at the extracted data) and the
//! script API functions as they get ported.

pub mod gun;
mod random_api;

use std::cell::{Cell, RefCell};
use std::path::{Component, Path, PathBuf};

use mlua::{Function, Lua, MultiValue, Table, Value};
use sim::random::ScriptRandom;

/// `dofile_once` as the engine defines it (string at `0x1028ac8`); only `do_mod_appends` is
/// supplied separately.
const DOFILE_ONCE: &str = "__loadonce={} dofile_once=function(filename) local result=nil local cached=__loadonce[filename] if cached~=nil then result=cached[1] else local f,err=loadfile(filename) if f==nil then return f,err end result=f() __loadonce[filename]={result} do_mod_appends(filename) end return result end";

/// Per-state data the API functions share.
pub(crate) struct ScriptState {
    pub random: RefCell<ScriptRandom>,
    pub root: PathBuf,
    pub output: RefCell<Vec<String>>,
    pub commands: RefCell<Vec<gun::GunCommand>>,
    pub frame: Cell<i32>,
}

pub struct Runtime {
    lua: Lua,
}

impl Runtime {
    /// A state whose `data/...` paths resolve under `root` (the extracted `noita_files`).
    pub fn new(root: impl Into<PathBuf>, world_seed: i32) -> mlua::Result<Self> {
        let lua = Lua::new();
        lua.set_app_data(ScriptState {
            random: RefCell::new(ScriptRandom::new(world_seed)),
            root: root.into(),
            output: RefCell::new(Vec::new()),
            commands: RefCell::new(Vec::new()),
            frame: Cell::new(0),
        });
        install_base(&lua)?;
        random_api::install(&lua)?;
        Ok(Self { lua })
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Runs a script file through the global `dofile`.
    pub fn dofile(&self, path: &str) -> mlua::Result<MultiValue> {
        let dofile: Function = self.lua.globals().get("dofile")?;
        dofile.call(path)
    }

    /// Lines the scripts printed.
    pub fn output(&self) -> Vec<String> {
        self.lua
            .app_data_ref::<ScriptState>()
            .map(|s| s.output.borrow().clone())
            .unwrap_or_default()
    }

    /// Sets the new-game-plus count mixed into every position-based random seed.
    pub fn set_new_game_plus(&self, count: i32) {
        if let Some(s) = self.lua.app_data_ref::<ScriptState>() {
            s.random.borrow_mut().new_game_plus = count;
        }
    }
}

/// Resolves a script path under `root`, refusing anything that could leave it.
fn resolve(root: &Path, name: &str) -> Option<PathBuf> {
    let name = name.replace('\\', "/");
    let rel = Path::new(&name);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    Some(root.join(rel))
}

fn install_base(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();

    let loadfile = lua.create_function(|lua, name: String| {
        let root = {
            let state = lua
                .app_data_ref::<ScriptState>()
                .ok_or_else(|| mlua::Error::runtime("script state missing"))?;
            state.root.clone()
        };
        let Some(path) = resolve(&root, &name) else {
            return Ok((Value::Nil, Some(format!("cannot open {name}"))));
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => return Ok((Value::Nil, Some(format!("cannot open {name}: {e}")))),
        };
        match lua.load(bytes).set_name(format!("@{name}")).into_function() {
            Ok(f) => Ok((Value::Function(f), None)),
            Err(e) => Ok((Value::Nil, Some(e.to_string()))),
        }
    })?;
    globals.set("loadfile", loadfile)?;

    let dofile = lua.create_function(|lua, name: String| {
        let loadfile: Function = lua.globals().get("loadfile")?;
        let (f, err): (Value, Option<String>) = loadfile.call(name)?;
        match f {
            Value::Function(f) => f.call::<MultiValue>(()),
            _ => Err(mlua::Error::runtime(err.unwrap_or_default())),
        }
    })?;
    globals.set("dofile", dofile)?;

    let print = lua.create_function(|lua, args: MultiValue| {
        let tostring: Function = lua.globals().get("tostring")?;
        let mut parts = Vec::new();
        for v in args {
            parts.push(tostring.call::<String>(v)?);
        }
        let line = parts.join("\t");
        if let Some(state) = lua.app_data_ref::<ScriptState>() {
            state.output.borrow_mut().push(line);
        }
        Ok(())
    })?;
    globals.set("print", print)?;

    globals.set(
        "do_mod_appends",
        lua.create_function(|_, _name: Value| Ok(()))?,
    )?;
    lua.load(DOFILE_ONCE).set_name("=dofile_once").exec()?;
    Ok(())
}

/// Reads a global table for tests and tools.
pub fn global_table(lua: &Lua, name: &str) -> mlua::Result<Table> {
    lua.globals().get(name)
}

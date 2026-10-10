//! The Lua runtime: loading rules, the random API and the reference gun action table.

use std::path::PathBuf;

use script::Runtime;
use sim::random::ScriptRandom;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../noita_files")
}

/// A scratch directory with a few scripts, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("noita_script_{name}_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("data")).unwrap();
        Self(dir)
    }

    fn write(&self, rel: &str, text: &str) {
        std::fs::write(self.0.join(rel), text).unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn dofile_once_runs_a_script_a_single_time() {
    let dir = Scratch::new("once");
    dir.write("data/counter.lua", "n = (n or 0) + 1 return n * 10");
    let rt = Runtime::new(&dir.0, 1).unwrap();
    let lua = rt.lua();
    let first: i32 = lua
        .load(r#"return dofile_once("data/counter.lua")"#)
        .eval()
        .unwrap();
    let second: i32 = lua
        .load(r#"return dofile_once("data/counter.lua")"#)
        .eval()
        .unwrap();
    assert_eq!((first, second), (10, 10));
    let n: i32 = lua.globals().get("n").unwrap();
    assert_eq!(n, 1);
    // dofile always runs it again.
    rt.dofile("data/counter.lua").unwrap();
    let n: i32 = lua.globals().get("n").unwrap();
    assert_eq!(n, 2);
}

#[test]
fn missing_and_escaping_paths_fail() {
    let dir = Scratch::new("paths");
    let rt = Runtime::new(&dir.0, 1).unwrap();
    let lua = rt.lua();
    let (f, err): (Option<mlua::Function>, String) = lua
        .load(r#"return loadfile("data/nope.lua")"#)
        .eval()
        .unwrap();
    assert!(f.is_none() && err.contains("cannot open"));
    for bad in [
        "../secret.lua",
        "data/../../secret.lua",
        "/etc/passwd",
        "C:\\x.lua",
    ] {
        let src = format!(r#"return loadfile("{}")"#, bad.replace('\\', "\\\\"));
        let (f, _): (Option<mlua::Function>, String) = lua.load(src).eval().unwrap();
        assert!(f.is_none(), "{bad} was opened");
    }
    assert!(rt.dofile("data/nope.lua").is_err());
}

#[test]
fn print_is_captured_and_errors_name_the_script() {
    let dir = Scratch::new("print");
    dir.write("data/hello.lua", "print('a', 1, nil, true)\nerror('boom')");
    let rt = Runtime::new(&dir.0, 1).unwrap();
    let err = rt.dofile("data/hello.lua").unwrap_err().to_string();
    assert!(
        err.contains("data/hello.lua:2") && err.contains("boom"),
        "{err}"
    );
    assert_eq!(rt.output(), vec!["a\t1\tnil\ttrue".to_string()]);
}

#[test]
fn random_functions_follow_the_sim_generator() {
    let rt = Runtime::new(PathBuf::from("."), 424242).unwrap();
    let lua = rt.lua();
    let mut direct = ScriptRandom::new(424242);
    direct.set_random_seed(10.0, 20.0);
    let expected = [
        f64::from(direct.random_int(1, 100)),
        f64::from(direct.random_int(0, 9)),
        f64::from(direct.random_float(-1.0, 1.0)),
        direct.random(),
    ];
    let got: Vec<f64> = lua
        .load("SetRandomSeed(10, 20) return Random(1, 100), Random(9), Randomf(-1, 1), Random()")
        .eval::<mlua::MultiValue>()
        .unwrap()
        .iter()
        .map(|v| match v {
            mlua::Value::Number(n) => *n,
            mlua::Value::Integer(i) => *i as f64,
            other => panic!("not a number: {other:?}"),
        })
        .collect();
    assert_eq!(got, expected);

    let (unit, int, float, bit): (f64, f64, f64, f64) = lua
        .load("return ProceduralRandom(5, 6), ProceduralRandom(5, 6, 9), ProceduralRandomf(5, 6, -2, 2), ProceduralRandomi(5, 6)")
        .eval()
        .unwrap();
    assert_eq!(unit, direct.procedural_unit(5.0, 6.0));
    assert_eq!(int, f64::from(direct.procedural_int(5.0, 6.0, 0, 9)));
    assert_eq!(
        float,
        f64::from(direct.procedural_float(5.0, 6.0, -2.0, 2.0))
    );
    assert_eq!(bit, f64::from(direct.procedural_bit(5.0, 6.0)));
    // Procedural calls do not disturb the shared generator.
    let next: f64 = lua.load("return Random(1, 100)").eval().unwrap();
    assert_eq!(next, f64::from(direct.random_int(1, 100)));
}

#[test]
fn random_arguments_are_checked() {
    let rt = Runtime::new(PathBuf::from("."), 1).unwrap();
    let err = rt
        .lua()
        .load("SetRandomSeed(1)")
        .exec()
        .unwrap_err()
        .to_string();
    assert!(err.contains("requires 2 parameters, only 1 given"), "{err}");
    assert!(rt.lua().load("ProceduralRandom(1)").exec().is_err());
    rt.set_new_game_plus(5);
    let a: f64 = rt
        .lua()
        .load("return ProceduralRandom(1, 2)")
        .eval()
        .unwrap();
    let mut direct = ScriptRandom::new(1);
    direct.new_game_plus = 5;
    assert_eq!(a, direct.procedural_unit(1.0, 2.0));
}

#[test]
fn the_reference_gun_action_table_loads() {
    let root = root();
    if !root.join("data/scripts/gun/gun_actions.lua").exists() {
        eprintln!("skipping: reference data not present");
        return;
    }
    let rt = Runtime::new(&root, 1).unwrap();
    rt.dofile("data/scripts/gun/gun_actions.lua").unwrap();
    let actions: mlua::Table = rt.lua().globals().get("actions").unwrap();
    let count = actions.raw_len();
    assert!(count > 300, "only {count} actions");
    let bomb: mlua::Table = actions.get(1).unwrap();
    assert_eq!(bomb.get::<String>("id").unwrap(), "BOMB");
    assert_eq!(bomb.get::<i32>("mana").unwrap(), 25);
    assert_eq!(bomb.get::<i32>("max_uses").unwrap(), 3);
    assert!(bomb.get::<mlua::Function>("action").is_ok());
}

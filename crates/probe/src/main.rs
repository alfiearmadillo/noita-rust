//! Read-only probe of the running reference game (`noita.exe`): dumps the cell grid and RNG state
//! so the port can be compared against the real simulation. Nothing is written to the game.
//!
//! Usage:
//!   noita_probe status                       process, frame, grid pointers, loaded bounds
//!   noita_probe rng                          the global `rand()` state and frame counter
//!   noita_probe cell <x> <y>                 one cell with its raw bytes
//!   noita_probe grid <x> <y> <w> <h> [--tsv FILE]
//!                                            ASCII map of a rectangle and optional per-cell dump
//!   noita_probe follow <x> <y> <w> <h> <DIR> [--frames N]
//!                                            one TSV per game frame (pause the game for coherent maps)
//!   noita_probe biomes <DIR>                 the world seed, Wang offset and every Wang biome's images
//!   Options for every command: `--grid 0xADDR` overrides the GridWorld pointer.

mod biomes;
mod layout;
mod mem;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use layout::{Cell, GLOBAL_RAND, Grid, VT_GRID_WORLD};
use mem::{Mem, Process};

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    positional: Vec<String>,
    tsv: Option<String>,
    frames: usize,
    grid: Option<u32>,
}

fn parse_args(raw: Vec<String>) -> Result<Args, String> {
    let mut a = Args {
        positional: Vec::new(),
        tsv: None,
        frames: 60,
        grid: None,
    };
    let mut it = raw.into_iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--tsv" => a.tsv = Some(value("--tsv")?),
            "--frames" => {
                a.frames = value("--frames")?
                    .parse()
                    .map_err(|_| "--frames needs a number".to_owned())?;
            }
            "--grid" => a.grid = Some(parse_addr(&value("--grid")?)?),
            _ => a.positional.push(arg),
        }
    }
    Ok(a)
}

fn parse_addr(s: &str) -> Result<u32, String> {
    let s = s.trim();
    let parsed = match s.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => s.parse(),
    };
    parsed.map_err(|_| format!("bad address {s:?}"))
}

fn int(args: &Args, i: usize, what: &str) -> Result<i32, String> {
    args.positional
        .get(i)
        .ok_or(format!("missing {what}"))?
        .parse()
        .map_err(|_| format!("{what} must be an integer"))
}

fn run(raw: Vec<String>) -> Result<(), String> {
    let args = parse_args(raw)?;
    let cmd = args.positional.first().cloned().ok_or(usage())?;
    let process = Process::attach("noita.exe")?;
    let io = |e: std::io::Error| e.to_string();
    match cmd.as_str() {
        "status" => status(&process, &args),
        "peek" => {
            let addr = parse_addr(args.positional.get(1).ok_or("missing address")?)?;
            let len = args.positional.get(2).map_or(Ok(0x80), |s| parse_addr(s))? as usize;
            let bytes = process.bytes(addr, len).map_err(io)?;
            for (i, row) in bytes.chunks(16).enumerate() {
                let words: Vec<String> = row
                    .chunks(4)
                    .filter(|w| w.len() == 4)
                    .map(|w| format!("{:08x}", u32::from_le_bytes([w[0], w[1], w[2], w[3]])))
                    .collect();
                println!("{:08x}  {}", addr + 16 * i as u32, words.join(" "));
            }
            Ok(())
        }
        "workers" => workers(&process),
        "scan" => {
            let value = parse_addr(args.positional.get(1).ok_or("missing value")?)?;
            let start = args
                .positional
                .get(2)
                .map_or(Ok(0x1_0000), |s| parse_addr(s))?;
            let end = args
                .positional
                .get(3)
                .map_or(Ok(0x7FFF_0000), |s| parse_addr(s))?;
            for addr in mem::scan_u32(&process, value, start, end, 200) {
                println!("{addr:08x}");
            }
            Ok(())
        }
        "rng" => {
            let grid = Grid::open(&process, args.grid).map_err(io)?;
            println!(
                "frame {} global rand state {:#010x}",
                grid.frame,
                process.u32(GLOBAL_RAND).map_err(io)?
            );
            Ok(())
        }
        "cell" => {
            let (x, y) = (int(&args, 1, "x")?, int(&args, 2, "y")?);
            let mut grid = Grid::open(&process, args.grid).map_err(io)?;
            match grid.cell(x, y).map_err(io)? {
                None => println!("({x}, {y}): empty"),
                Some(c) => {
                    println!("({x}, {y}): {}", describe(&grid, &c));
                    println!("  raw {}", hex(&c.raw));
                }
            }
            Ok(())
        }
        "grid" => {
            let (x, y) = (int(&args, 1, "x")?, int(&args, 2, "y")?);
            let (w, h) = (int(&args, 3, "w")?, int(&args, 4, "h")?);
            let (w, h) = (
                usize::try_from(w).map_err(|_| "w < 0")?,
                usize::try_from(h).map_err(|_| "h < 0")?,
            );
            let mut grid = Grid::open(&process, args.grid).map_err(io)?;
            let cells = grid.rect(x, y, w, h).map_err(io)?;
            println!("frame {}", grid.frame);
            print!("{}", ascii_map(&grid, &cells, w)?);
            if let Some(path) = &args.tsv {
                std::fs::write(path, tsv(&grid, &cells, x, y, w)?).map_err(|e| e.to_string())?;
                println!("wrote {path}");
            }
            Ok(())
        }
        "ages" => {
            let (x, y) = (int(&args, 1, "x")?, int(&args, 2, "y")?);
            let (w, h) = (int(&args, 3, "w")?, int(&args, 4, "h")?);
            let (w, h) = (
                usize::try_from(w).map_err(|_| "w < 0")?,
                usize::try_from(h).map_err(|_| "h < 0")?,
            );
            let mut grid = Grid::open(&process, args.grid).map_err(io)?;
            let cells = grid.rect(x, y, w, h).map_err(io)?;
            print!("{}", age_report(grid.frame, &cells, (x, y), w));
            Ok(())
        }
        "follow" => {
            let (x, y) = (int(&args, 1, "x")?, int(&args, 2, "y")?);
            let (w, h) = (int(&args, 3, "w")?, int(&args, 4, "h")?);
            let dir = args.positional.get(5).ok_or("missing output directory")?;
            let (w, h) = (
                usize::try_from(w).map_err(|_| "w < 0")?,
                usize::try_from(h).map_err(|_| "h < 0")?,
            );
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            follow(&process, &args, (x, y, w, h), dir)
        }
        "biomes" => {
            let dir = args.positional.get(1).ok_or("missing output directory")?;
            dump_biomes(&process, &args, dir)
        }
        _ => Err(usage()),
    }
}

/// Writes `world.txt` (seed, Wang offset bits) and `<n>.txt` per Wang biome: a header line
/// `file kind width height`, the float image as hex bits, then the material image.
fn dump_biomes(process: &Process, args: &Args, dir: &str) -> Result<(), String> {
    let grid = Grid::open(process, args.grid).map_err(|e| e.to_string())?;
    let world = biomes::read(process, grid.grid).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let write = |name: String, text: String| {
        std::fs::write(std::path::Path::new(dir).join(name), text).map_err(|e| e.to_string())
    };
    write(
        "world.txt".to_owned(),
        format!(
            "seed {}\noffset {:#x} {:#x}\n",
            world.seed,
            world.offset.0.to_bits(),
            world.offset.1.to_bits()
        ),
    )?;
    let names = (0..grid.material_count())
        .map(|id| grid.name(id).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    write("materials.txt".to_owned(), names.join("\n") + "\n")?;
    println!(
        "seed {} wang offset ({}, {}), {} biome objects",
        world.seed,
        world.offset.0,
        world.offset.1,
        world.biomes.len()
    );
    for (i, b) in world.biomes.iter().enumerate() {
        let Some(w) = &b.wang else {
            println!("  {i}: {} (type {}, no Wang images)", b.file, b.kind);
            continue;
        };
        let floats: Vec<String> = w
            .values
            .iter()
            .map(|v| format!("{:x}", v.to_bits()))
            .collect();
        let mats: Vec<String> = w.materials.iter().map(u16::to_string).collect();
        write(
            format!("{i}.txt"),
            format!(
                "{} {} {} {}\n{}\n{}\n",
                b.file,
                b.kind,
                w.width,
                w.height,
                floats.join(" "),
                mats.join(" ")
            ),
        )?;
        println!("  {i}: {} {}x{} -> {i}.txt", b.file, w.width, w.height);
    }
    Ok(())
}

fn usage() -> String {
    "usage: noita_probe <status|rng|cell|grid|follow|biomes> ... (see the file header for options)"
        .to_owned()
}

/// One `GridWorldThreaded` worker per loaded 64x64 region: found by scanning for its vtable.
fn workers(process: &Process) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    let mut rows = Vec::new();
    for addr in mem::scan_u32(process, layout::VT_WORKER, 0x0100_0000, 0x7FFF_0000, 5000) {
        let u = |off: u32| process.u32(addr + off).map_err(io);
        let origin = (u(0x470)? as i32, u(0x474)? as i32);
        let dirty = [u(0x480)?, u(0x484)?, u(0x488)?, u(0x48C)?].map(|v| v as i32);
        rows.push((origin, addr, u(0x4)?, u(0x460)?, u(0x46C)? & 0xFF, dirty));
    }
    rows.sort();
    println!(
        "{} workers; origin rng full_counter full_scan dirty(min,max)",
        rows.len()
    );
    for (origin, addr, rng, counter, scan, d) in rows {
        println!(
            "{addr:08x} ({},{}) rng {rng:#010x} counter {counter} scan {scan} dirty ({},{})..({},{})",
            origin.0, origin.1, d[0], d[1], d[2], d[3]
        );
    }
    Ok(())
}

fn status(process: &Process, args: &Args) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    let grid = Grid::open(process, args.grid).map_err(io)?;
    println!("noita.exe pid {}", process.pid);
    println!("frame {}", grid.frame);
    let note = if grid.grid_vptr == VT_GRID_WORLD {
        "GridWorld vtable"
    } else {
        "NOT the GridWorld vtable; try --grid"
    };
    println!("grid {:#x} (vptr {:#x}, {note})", grid.grid, grid.grid_vptr);
    println!(
        "loaded bounds x {}..{} y {}..{}",
        grid.bounds[0], grid.bounds[2], grid.bounds[1], grid.bounds[3]
    );
    println!("materials {}", grid.material_count());
    Ok(())
}

fn describe(grid: &Grid<'_, Process>, c: &Cell) -> String {
    let name = grid.name(c.material).unwrap_or_else(|_| "?".into());
    let mut s = format!(
        "{} {} (id {}) at {:#x}",
        c.kind.label(),
        name,
        c.material,
        c.ptr
    );
    if c.burning() {
        let _ = write!(s, " burning({})", c.intensity());
    }
    let _ = write!(s, " {}", c.fields());
    s
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const SYMBOLS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Rows of symbols (most common material first gets `A`), then the legend.
fn ascii_map(grid: &Grid<'_, Process>, cells: &[Option<Cell>], w: usize) -> Result<String, String> {
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for c in cells.iter().flatten() {
        *counts.entry(c.material).or_default() += 1;
    }
    let mut order: Vec<_> = counts.iter().collect();
    order.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let symbol = |id: u32| {
        order
            .iter()
            .position(|(m, _)| **m == id)
            .and_then(|i| SYMBOLS.get(i).copied())
            .unwrap_or(b'?') as char
    };
    let mut out = String::new();
    for row in cells.chunks(w.max(1)) {
        out.extend(
            row.iter()
                .map(|c| c.as_ref().map_or('.', |c| symbol(c.material))),
        );
        out.push('\n');
    }
    for (id, count) in &order {
        let name = grid.name(**id).map_err(|e| e.to_string())?;
        let _ = writeln!(out, "{} = {name} (id {id}, {count} cells)", symbol(**id));
    }
    Ok(out)
}

/// Header plus one line per occupied cell: x, y, class, id, name, burning, intensity, fields.
fn tsv(
    grid: &Grid<'_, Process>,
    cells: &[Option<Cell>],
    x0: i32,
    y0: i32,
    w: usize,
) -> Result<String, String> {
    let mut out = String::from("x\ty\tkind\tmaterial\tname\tburning\tintensity\tfields\n");
    for (i, c) in cells.iter().enumerate() {
        let Some(c) = c else { continue };
        let (x, y) = (x0 + (i % w) as i32, y0 + (i / w) as i32);
        let name = grid.name(c.material).map_err(|e| e.to_string())?;
        let _ = writeln!(
            out,
            "{x}\t{y}\t{}\t{}\t{name}\t{}\t{}\t{}",
            c.kind.label(),
            c.material,
            u8::from(c.burning()),
            c.intensity(),
            c.fields()
        );
    }
    Ok(out)
}

/// How long ago each moving cell (non-static liquid or gas) was last updated, per 64x64 region and
/// overall: shows how often the reference rescans idle regions.
fn age_report(frame: u32, cells: &[Option<Cell>], (x0, y0): (i32, i32), w: usize) -> String {
    let mut regions: BTreeMap<(i32, i32), Vec<u32>> = BTreeMap::new();
    for (i, c) in cells.iter().enumerate() {
        let Some(last) = c.as_ref().and_then(Cell::last_frame) else {
            continue;
        };
        let (x, y) = (x0 + (i % w) as i32, y0 + (i / w) as i32);
        regions
            .entry((x >> 6, y >> 6))
            .or_default()
            .push(frame.wrapping_sub(last));
    }
    let mut out = format!("frame {frame}\nregion(x,y) cells min median max age\n");
    let mut buckets = [0usize; 5];
    for ((rx, ry), ages) in &mut regions {
        ages.sort_unstable();
        let _ = writeln!(
            out,
            "({rx},{ry}) {} {} {} {}",
            ages.len(),
            ages[0],
            ages[ages.len() / 2],
            ages[ages.len() - 1]
        );
        for &a in ages.iter() {
            let b = match a {
                0 => 0,
                1..=10 => 1,
                11..=100 => 2,
                101..=1000 => 3,
                _ => 4,
            };
            buckets[b] += 1;
        }
    }
    let _ = writeln!(
        out,
        "cells by age: 0 {} | 1-10 {} | 11-100 {} | 101-1000 {} | >1000 {}",
        buckets[0], buckets[1], buckets[2], buckets[3], buckets[4]
    );
    out
}

fn follow(
    process: &Process,
    args: &Args,
    (x, y, w, h): (i32, i32, usize, usize),
    dir: &str,
) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    let mut last = None;
    let mut written = 0;
    let deadline = Instant::now() + Duration::from_secs(600);
    while written < args.frames && Instant::now() < deadline {
        let mut grid = Grid::open(process, args.grid).map_err(io)?;
        if last == Some(grid.frame) {
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        last = Some(grid.frame);
        let cells = grid.rect(x, y, w, h).map_err(io)?;
        let rand = process.u32(GLOBAL_RAND).map_err(io)?;
        let mut body = format!("# frame {} rand {rand:#010x}\n", grid.frame);
        body.push_str(&tsv(&grid, &cells, x, y, w)?);
        let path = format!("{dir}/frame_{:08}.tsv", grid.frame);
        std::fs::write(&path, body).map_err(|e| e.to_string())?;
        written += 1;
    }
    println!("wrote {written} frames to {dir}");
    Ok(())
}

//! Extracts a `data.wak` archive.
//!
//! Usage: `cargo run -p noita_data --bin wak_extract -- [WAK] [OUT_DIR]`
//! Defaults: `noita_files/install/data/data.wak` -> `noita_files/` (yields `noita_files/data/...`).

use std::path::PathBuf;
use std::process::ExitCode;

use noita_data::wak::Wak;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let wak_path = args.next().map_or_else(
        || PathBuf::from("noita_files/install/data/data.wak"),
        PathBuf::from,
    );
    let out_dir = args
        .next()
        .map_or_else(|| PathBuf::from("noita_files"), PathBuf::from);

    let bytes = match std::fs::read(&wak_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {}: {e}", wak_path.display());
            return ExitCode::FAILURE;
        }
    };
    let wak = match Wak::parse(&bytes) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("cannot parse {}: {e}", wak_path.display());
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = wak.extract_all(&out_dir) {
        eprintln!("extract failed: {e}");
        return ExitCode::FAILURE;
    }
    println!(
        "extracted {} entries from {} into {}",
        wak.entries().len(),
        wak_path.display(),
        out_dir.display()
    );
    ExitCode::SUCCESS
}

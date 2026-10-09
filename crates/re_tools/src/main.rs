//! Read-only analysis of the repo copy of the reference executable. Never point this at the install.
//!
//! Usage:
//!   re_tools vtable <exe> <rtti-name>     e.g. `.?AVCLiquidCell@grid@@`
//!   re_tools disasm <exe> <hex-va> [n]    disassemble n instructions (default 200)
//!   re_tools calls <exe> <hex-va>         direct `call rel32` sites targeting the address
//!   re_tools xref <exe> <string>          code locations that reference the exact NUL-terminated string
//!   re_tools xrefsub <exe> <substring>    like xref, for every string containing the substring
//!   re_tools grep <exe> <substring>       linear-sweep disassembly of .text, lines containing substring
//!   re_tools str <exe> <hex-va>           NUL-terminated ASCII string at the address

mod pe;

use std::process::ExitCode;

use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, IntelFormatter, OpKind};
use pe::{Pe, u32_at};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(cmd), Some(exe)) = (args.first(), args.get(1)) else {
        eprintln!("usage: re_tools <vtable|disasm> <exe> ...");
        return ExitCode::FAILURE;
    };
    let bytes = match std::fs::read(exe) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {exe}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some(pe) = Pe::parse(&bytes) else {
        eprintln!("{exe}: not a PE32 x86 image");
        return ExitCode::FAILURE;
    };
    match (cmd.as_str(), args.get(2)) {
        ("vtable", Some(name)) => vtables(&pe, name),
        ("xref", Some(s)) => xref(&pe, s),
        ("xrefsub", Some(s)) => xrefsub(&pe, s),
        ("grep", Some(s)) => grep(&pe, s),
        ("disasm" | "calls" | "str", Some(va)) => {
            let Ok(va) = u32::from_str_radix(va.trim_start_matches("0x"), 16) else {
                eprintln!("bad hex va: {va}");
                return ExitCode::FAILURE;
            };
            if cmd == "calls" {
                calls(&pe, va);
            } else if cmd == "str" {
                println!("{:?}", pe.ascii_at_va(va, 1));
            } else {
                let n = args.get(3).and_then(|n| n.parse().ok()).unwrap_or(200);
                disasm(&pe, va, n);
            }
        }
        _ => {
            eprintln!("unknown command or missing argument");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

/// MSVC x86 RTTI: TypeDescriptor = {vftable*, spare, name[]}; CompleteObjectLocator =
/// {signature=0, offset, cdOffset, TypeDescriptor*, ClassHierarchyDescriptor*}; vftable[-1] = COL*.
fn vtables(pe: &Pe, name: &str) {
    let mut needle = name.as_bytes().to_vec();
    needle.push(0);
    let Some(name_off) = pe.bytes.windows(needle.len()).position(|w| w == needle) else {
        println!("type name {name} not found");
        return;
    };
    let Some(td_va) = pe.off_to_va(name_off - 8) else {
        println!("type descriptor not mapped");
        return;
    };
    println!("TypeDescriptor {name} @ {td_va:#010x}");

    let Some(rdata) = pe.section(".rdata") else {
        return;
    };
    let text = pe
        .section(".text")
        .map(|s| pe.va_range(s))
        .unwrap_or((0, 0));
    let (start, end) = (rdata.raw as usize, (rdata.raw + rdata.raw_size) as usize);
    let dwords = |target: u32| {
        (start..end.saturating_sub(4))
            .step_by(4)
            .filter(move |&o| u32_at(pe.bytes, o) == Some(target))
    };

    for td_ref in dwords(td_va) {
        let col_off = td_ref - 12;
        if u32_at(pe.bytes, col_off) != Some(0) {
            continue;
        }
        let Some(col_va) = pe.off_to_va(col_off) else {
            continue;
        };
        let this_offset = u32_at(pe.bytes, col_off + 4).unwrap_or(0);
        for col_ref in dwords(col_va) {
            let Some(vt_va) = pe.off_to_va(col_ref + 4) else {
                continue;
            };
            println!("\nvftable @ {vt_va:#010x} (COL {col_va:#010x}, this-offset {this_offset})");
            for i in 0.. {
                match pe.u32_at_va(vt_va + 4 * i) {
                    Some(f) if (text.0..text.1).contains(&f) => println!("  [{i:3}] {f:#010x}"),
                    _ => break,
                }
            }
        }
    }
}

/// Finds every exact NUL-terminated occurrence of `s` (preceded by a NUL) and the `.text`
/// dwords equal to its address.
fn xref(pe: &Pe, s: &str) {
    let mut needle = vec![0];
    needle.extend_from_slice(s.as_bytes());
    needle.push(0);
    let Some(text) = pe.section(".text") else {
        return;
    };
    let (start, end) = (text.raw as usize, (text.raw + text.raw_size) as usize);
    for pos in pe
        .bytes
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle.as_slice())
        .map(|(i, _)| i + 1)
    {
        let Some(str_va) = pe.off_to_va(pos) else {
            continue;
        };
        println!("\"{s}\" @ {str_va:#010x}");
        for off in start..end.saturating_sub(4) {
            if u32_at(pe.bytes, off) == Some(str_va)
                && let Some(va) = pe.off_to_va(off)
            {
                println!("  ref in operand at {va:#010x}");
            }
        }
    }
}

/// Finds strings containing `sub` (extended back to the preceding NUL) and their `.text` refs.
fn xrefsub(pe: &Pe, sub: &str) {
    let needle = sub.as_bytes();
    let mut starts: Vec<usize> = pe
        .bytes
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(i, _)| {
            let mut s = i;
            while s > 0 && pe.bytes[s - 1] != 0 {
                s -= 1;
            }
            s
        })
        .collect();
    starts.dedup();
    for start in starts {
        let end = pe.bytes[start..].iter().position(|&c| c == 0).unwrap_or(0);
        let text = String::from_utf8_lossy(&pe.bytes[start..start + end]);
        match pe.off_to_va(start) {
            Some(va) => {
                println!("{text:?}");
                xref_va(pe, va);
            }
            None => println!("{text:?} (unmapped)"),
        }
    }
}

fn xref_va(pe: &Pe, str_va: u32) {
    let Some(text) = pe.section(".text") else {
        return;
    };
    let (start, end) = (text.raw as usize, (text.raw + text.raw_size) as usize);
    println!("  @ {str_va:#010x}");
    for off in start..end.saturating_sub(4) {
        if u32_at(pe.bytes, off) == Some(str_va)
            && let Some(va) = pe.off_to_va(off)
        {
            println!("  ref in operand at {va:#010x}");
        }
    }
}

/// Linear sweep can mis-decode data embedded in code; confirm hits with `disasm`.
fn grep(pe: &Pe, needle: &str) {
    let Some(text) = pe.section(".text") else {
        return;
    };
    let (start, end) = (text.raw as usize, (text.raw + text.raw_size) as usize);
    let va = pe.off_to_va(start).expect("text mapped");
    let mut decoder = Decoder::with_ip(
        32,
        &pe.bytes[start..end],
        u64::from(va),
        DecoderOptions::NONE,
    );
    let mut formatter = IntelFormatter::new();
    let (mut out, mut ins) = (String::new(), Instruction::default());
    while decoder.can_decode() {
        decoder.decode_out(&mut ins);
        out.clear();
        formatter.format(&ins, &mut out);
        if out.contains(needle) {
            println!("{:08x}  {out}", ins.ip32());
        }
    }
}

/// Byte scan of `.text` for `E8 rel32` whose target is `target`; may include false positives
/// from misaligned bytes, so confirm each hit with `disasm`.
fn calls(pe: &Pe, target: u32) {
    let Some(text) = pe.section(".text") else {
        return;
    };
    let (start, end) = (text.raw as usize, (text.raw + text.raw_size) as usize);
    for off in start..end.saturating_sub(5) {
        if pe.bytes[off] != 0xE8 {
            continue;
        }
        let (Some(rel), Some(va)) = (u32_at(pe.bytes, off + 1), pe.off_to_va(off)) else {
            continue;
        };
        if va.wrapping_add(5).wrapping_add(rel) == target {
            println!("{va:#010x}");
        }
    }
}

fn disasm(pe: &Pe, va: u32, n: usize) {
    let Some(off) = pe.va_to_off(va) else {
        println!("{va:#010x} not mapped");
        return;
    };
    let code = &pe.bytes[off..pe.bytes.len().min(off + n * 16)];
    let mut decoder = Decoder::with_ip(32, code, u64::from(va), DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();
    let mut text = String::new();
    let mut ins = Instruction::default();
    for _ in 0..n {
        if !decoder.can_decode() {
            break;
        }
        decoder.decode_out(&mut ins);
        text.clear();
        formatter.format(&ins, &mut text);
        let note = referenced_value(&ins)
            .and_then(|v| {
                pe.ascii_at_va(v, 4)
                    .map(|s| format!("  ; \"{s}\""))
                    .or_else(|| float_note(pe, &ins, v))
            })
            .unwrap_or_default();
        println!("{:08x}  {text}{note}", ins.ip32());
    }
}

/// Absolute address used by the instruction (immediate or memory displacement), if any.
fn referenced_value(ins: &Instruction) -> Option<u32> {
    (0..ins.op_count()).find_map(|i| match ins.op_kind(i) {
        OpKind::Immediate32 => Some(ins.immediate32()),
        OpKind::Memory if ins.memory_base() == iced_x86::Register::None => {
            Some(ins.memory_displacement32())
        }
        _ => None,
    })
}

/// For SSE/x87 loads of constants, show the f32/f64 value stored at the address.
fn float_note(pe: &Pe, ins: &Instruction, addr: u32) -> Option<String> {
    let size = ins.memory_size().size();
    let bits = pe.u32_at_va(addr)?;
    match size {
        4 => Some(format!("  ; f32 {}", f32::from_bits(bits))),
        8 => {
            let hi = pe.u32_at_va(addr + 4)?;
            Some(format!(
                "  ; f64 {}",
                f64::from_bits((u64::from(hi) << 32) | u64::from(bits))
            ))
        }
        _ => None,
    }
}

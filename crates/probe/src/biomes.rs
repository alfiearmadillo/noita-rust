//! The world generator's state in the running game: the biome objects reachable from the
//! `ProceduralTerrain` grid and the Wang images they sample. Offsets come from the decompile of
//! `0x87d0e0`, `0x87d9a0` and `0x86b9f0`.

use std::io;

use crate::mem::Mem;

/// `DAT_01204b98`: the `ProceduralTerrain` object (the first argument of `0x87a900`).
pub const PROCEDURAL_TERRAIN: u32 = 0x0120_4B98;
/// `DAT_01205004`: the world seed.
pub const WORLD_SEED: u32 = 0x0120_5004;
/// `DAT_01206fb0` / `DAT_012051b0`: the Wang offset (doubles).
pub const WANG_OFFSET: [u32; 2] = [0x0120_6FB0, 0x0120_51B0];

pub struct WangImages {
    pub width: i32,
    pub height: i32,
    pub values: Vec<f32>,
    pub materials: Vec<u16>,
}

pub struct LiveBiome {
    pub file: String,
    pub kind: i32,
    pub wang: Option<WangImages>,
}

pub struct LiveWorld {
    pub seed: i32,
    pub offset: (f64, f64),
    pub biomes: Vec<LiveBiome>,
}

fn f64_at<M: Mem>(mem: &M, addr: u32) -> io::Result<f64> {
    let b = mem.bytes(addr, 8)?;
    Ok(f64::from_le_bytes(b.try_into().expect("8 bytes")))
}

/// An MSVC `std::string` at `base`: inline below 16 bytes of capacity, else a pointer.
fn string_at<M: Mem>(mem: &M, base: u32) -> io::Result<String> {
    let (len, cap) = (mem.u32(base + 0x10)?, mem.u32(base + 0x14)?);
    if len > 4096 {
        return Err(io::Error::other("implausible string length"));
    }
    let at = if cap >= 16 { mem.u32(base)? } else { base };
    Ok(String::from_utf8_lossy(&mem.bytes(at, len as usize)?).into_owned())
}

/// The float image at `base` and the `u16` image at `base + 0x20` (`w, h, size, .., data +0x18, cap +0x1c`).
fn wang_images<M: Mem>(mem: &M, base: u32) -> io::Result<Option<WangImages>> {
    let (w, h) = (mem.i32(base)?, mem.i32(base + 4)?);
    let (data, cap) = (mem.u32(base + 0x18)?, mem.u32(base + 0x1c)?);
    if w <= 0 || h <= 0 || w > 4096 || h > 4096 || data == 0 || cap == 0 {
        return Ok(None);
    }
    let n = (w * h) as usize;
    let values = mem
        .bytes(data, n * 4)?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    let mdata = mem.u32(base + 0x38)?;
    let materials = if mdata == 0 || mem.u32(base + 0x3c)? == 0 {
        vec![0; n]
    } else {
        mem.bytes(mdata, n * 2)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect()
    };
    Ok(Some(WangImages {
        width: w,
        height: h,
        values,
        materials,
    }))
}

/// Every distinct biome object in the terrain grid, in first-seen order.
pub fn read<M: Mem>(mem: &M) -> io::Result<LiveWorld> {
    let terrain = mem.u32(PROCEDURAL_TERRAIN)?;
    if terrain == 0 {
        return Err(io::Error::other(
            "no ProceduralTerrain yet (start or load a game first)",
        ));
    }
    let (gw, gh) = (mem.i32(terrain + 0x50)?, mem.i32(terrain + 0x54)?);
    let grid = mem.u32(terrain + 0x68)?;
    if gw <= 0 || gh <= 0 || gw > 512 || gh > 512 || grid == 0 {
        return Err(io::Error::other(format!(
            "implausible biome grid {gw}x{gh}"
        )));
    }
    let cells = mem.bytes(grid, (gw * gh) as usize * 4)?;
    let mut seen: Vec<u32> = Vec::new();
    for p in cells.as_chunks::<4>().0 {
        let p = u32::from_le_bytes(*p);
        if p != 0 && !seen.contains(&p) {
            seen.push(p);
        }
    }
    let mut biomes = Vec::new();
    for b in seen {
        let kind = mem.i32(b + 4)?;
        let file = string_at(mem, b + 0x2d8)?;
        let wang_ptr = mem.u32(b + 0x1d4)?;
        let wang = if kind == 2 && wang_ptr != 0 {
            wang_images(mem, wang_ptr)?
        } else {
            None
        };
        biomes.push(LiveBiome { file, kind, wang });
    }
    Ok(LiveWorld {
        seed: mem.i32(WORLD_SEED)?,
        offset: (f64_at(mem, WANG_OFFSET[0])?, f64_at(mem, WANG_OFFSET[1])?),
        biomes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Image;

    #[test]
    fn reads_a_wang_biome_from_the_grid() {
        let mut m = Image::default();
        m.put_u32(PROCEDURAL_TERRAIN, 0x1000);
        m.put_u32(WORLD_SEED, 77);
        m.put(WANG_OFFSET[0], &(-12.5f64).to_le_bytes());
        m.put(WANG_OFFSET[1], &7168f64.to_le_bytes());
        // 2x1 grid pointing at the same biome twice.
        m.put_u32(0x1000 + 0x50, 2);
        m.put_u32(0x1000 + 0x54, 1);
        m.put_u32(0x1000 + 0x68, 0x2000);
        m.put(0x2000, &[0x00, 0x30, 0x00, 0x00, 0x00, 0x30, 0x00, 0x00]);
        // Biome: type 2, file name stored out of line, wang object.
        m.put_u32(0x3000 + 4, 2);
        let name = b"data/biome/coalmine.xml";
        m.put_u32(0x3000 + 0x2d8, 0x4000);
        m.put_u32(0x3000 + 0x2d8 + 0x10, name.len() as u32);
        m.put_u32(0x3000 + 0x2d8 + 0x14, 31);
        m.put(0x4000, name);
        m.put_u32(0x3000 + 0x1d4, 0x5000);
        // 2x2 images.
        m.put_u32(0x5000, 2);
        m.put_u32(0x5004, 2);
        m.put_u32(0x5018, 0x6000);
        m.put_u32(0x501c, 4);
        m.put_u32(0x5038, 0x7000);
        m.put_u32(0x503c, 4);
        let floats: Vec<u8> = [0.0f32, 1.0, -1.0, 0.5]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        m.put(0x6000, &floats);
        m.put(0x7000, &[1, 0, 2, 0, 3, 0, 0, 0]);
        let world = read(&m).unwrap();
        assert_eq!((world.seed, world.offset), (77, (-12.5, 7168.0)));
        assert_eq!(world.biomes.len(), 1);
        let b = &world.biomes[0];
        assert_eq!((b.file.as_str(), b.kind), ("data/biome/coalmine.xml", 2));
        let w = b.wang.as_ref().unwrap();
        assert_eq!((w.width, w.height), (2, 2));
        assert_eq!(w.values, vec![0.0, 1.0, -1.0, 0.5]);
        assert_eq!(w.materials, vec![1, 2, 3, 0]);
    }
}

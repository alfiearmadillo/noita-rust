//! Where the reference game keeps its cell grid. Offsets come from the decompile (`0x71cd10`,
//! `0x717670`, `0x719670`, `0x704e70`); the process is a fixed-base 32-bit image (no ASLR).

use std::collections::HashMap;
use std::io;

use crate::mem::Mem;

/// Pointer to the game state object (`DAT_0122374c`); its first dword is the frame counter and
/// `+0x10` points at the `GridWorld` (`+0x14` is a different, smaller world-info object).
pub const STATE_PTR: u32 = 0x0122_374C;
/// The global `rand()` state (`DAT_0120741c`).
pub const GLOBAL_RAND: u32 = 0x0120_741C;

pub const VT_GRID_WORLD: u32 = 0x0100_13BC;
/// `GridWorldThreaded`, one object per loaded 64x64 region.
pub const VT_WORKER: u32 = 0x0101_7B24;
pub const VT_LIQUID: u32 = 0x0100_BB90;
pub const VT_GAS: u32 = 0x0100_7BCC;
pub const VT_FIRE: u32 = 0x0100_96E0;
pub const VT_SOLID: u32 = 0x00FF_8A6C;

const STATE_GRID: u32 = 0x10;
/// The `CellFactory`, returned by `GridWorld` vfn `0x58`.
const GRID_FACTORY: u32 = 0x53C;
/// Chunk-pointer table (512 x 512 entries, origin at chunk 256) and the loaded bounds.
const GRID_CHUNK_TABLE: u32 = 0x508;
const GRID_BOUNDS: u32 = 0x520;
const FACTORY_NAMES: u32 = 4;
const CELL_DATA: u32 = 0x14;
const CELL_DATA_ID: u32 = 0x30;
const NAME_STRIDE: u32 = 0x18;
/// Bytes copied from each cell; enough for every cell class.
pub const CELL_BYTES: usize = 0x40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Liquid,
    Gas,
    Fire,
    Solid,
    Unknown(u32),
}

impl Kind {
    fn from_vptr(vptr: u32) -> Self {
        match vptr {
            VT_LIQUID => Kind::Liquid,
            VT_GAS => Kind::Gas,
            VT_FIRE => Kind::Fire,
            VT_SOLID => Kind::Solid,
            other => Kind::Unknown(other),
        }
    }

    pub fn label(self) -> String {
        match self {
            Kind::Liquid => "liquid".into(),
            Kind::Gas => "gas".into(),
            Kind::Fire => "fire".into(),
            Kind::Solid => "solid".into(),
            Kind::Unknown(v) => format!("vt{v:x}"),
        }
    }
}

/// One cell as stored in the game.
#[derive(Clone, Debug)]
pub struct Cell {
    pub ptr: u32,
    pub kind: Kind,
    pub material: u32,
    pub raw: [u8; CELL_BYTES],
}

impl Cell {
    fn u32_at(&self, off: usize) -> u32 {
        u32::from_le_bytes(self.raw[off..off + 4].try_into().expect("in range"))
    }

    fn f32_at(&self, off: usize) -> f32 {
        f32::from_bits(self.u32_at(off))
    }

    /// Last frame the cell was updated (liquid `+0x2C`, gas `+0x1C`); static liquids are not
    /// stamped, so they have none.
    pub fn last_frame(&self) -> Option<u32> {
        match self.kind {
            Kind::Liquid if self.raw[0x22] == 0 => Some(self.u32_at(0x2C)),
            Kind::Gas => Some(self.u32_at(0x1C)),
            _ => None,
        }
    }

    /// Base-cell flags: `+0x10` burning, `+0x11` fire intensity.
    pub fn burning(&self) -> bool {
        self.raw[0x10] != 0
    }

    pub fn intensity(&self) -> u8 {
        self.raw[0x11]
    }

    /// Kind-specific fields as `key=value` pairs (liquid and gas layouts from the port's README).
    pub fn fields(&self) -> String {
        match self.kind {
            Kind::Liquid => format!(
                "state={} rest={} static={} fall={} vel={} flow={} frame={} color={:08x} base={:08x} created={}",
                self.raw[0x20],
                self.raw[0x21],
                self.raw[0x22],
                self.raw[0x23],
                self.f32_at(0x24),
                self.f32_at(0x28),
                self.u32_at(0x2C),
                self.u32_at(0x30),
                self.u32_at(0x34),
                self.u32_at(0x38),
            ),
            Kind::Gas => format!(
                "dir={} frame={} rest={} heat={} age={}",
                self.u32_at(0x18) as i32,
                self.u32_at(0x1C),
                self.raw[0x28],
                self.u32_at(0x2C),
                self.u32_at(0x34) as i32,
            ),
            _ => self.raw.iter().map(|b| format!("{b:02x}")).collect(),
        }
    }
}

pub struct Grid<'a, M: Mem> {
    mem: &'a M,
    pub frame: u32,
    pub grid: u32,
    pub grid_vptr: u32,
    pub bounds: [i32; 4],
    table: u32,
    names: u32,
    count: u32,
    ids: HashMap<u32, u32>,
}

fn bad(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

impl<'a, M: Mem> Grid<'a, M> {
    /// Opens the grid through the game state; `grid_override` replaces the `GridWorld` address.
    pub fn open(mem: &'a M, grid_override: Option<u32>) -> io::Result<Self> {
        let state = mem.u32(STATE_PTR)?;
        if state == 0 {
            return Err(bad("the game state is not created yet"));
        }
        let frame = mem.u32(state)?;
        let grid = match grid_override {
            Some(g) => g,
            None => mem.u32(state + STATE_GRID)?,
        };
        if grid == 0 {
            return Err(bad("no GridWorld yet (is a world loaded?)"));
        }
        let grid_vptr = mem.u32(grid)?;
        let table = mem.u32(grid + GRID_CHUNK_TABLE)?;
        if table == 0 {
            return Err(bad(format!("GridWorld {grid:#x} has no chunk table")));
        }
        let mut bounds = [0; 4];
        for (i, b) in bounds.iter_mut().enumerate() {
            *b = mem.i32(grid + GRID_BOUNDS + 4 * i as u32)?;
        }
        let factory = mem.u32(grid + GRID_FACTORY)?;
        let names = mem.u32(factory + FACTORY_NAMES)?;
        let end = mem.u32(factory + FACTORY_NAMES + 4)?;
        let count = end.wrapping_sub(names) / NAME_STRIDE;
        Ok(Self {
            mem,
            frame,
            grid,
            grid_vptr,
            bounds,
            table,
            names,
            count,
            ids: HashMap::new(),
        })
    }

    pub fn material_count(&self) -> u32 {
        self.count
    }

    /// Material name by engine id (MSVC12 `std::string`: 16-byte buffer or heap pointer, size at
    /// `+0x10`, capacity at `+0x14`).
    pub fn name(&self, id: u32) -> io::Result<String> {
        if id >= self.count {
            return Ok(format!("#{id}"));
        }
        let s = self.names + id * NAME_STRIDE;
        let size = self.mem.u32(s + 0x10)? as usize;
        let cap = self.mem.u32(s + 0x14)?;
        let data = if cap >= 16 { self.mem.u32(s)? } else { s };
        if size > 256 {
            return Err(bad(format!("implausible material name length {size}")));
        }
        Ok(String::from_utf8_lossy(&self.mem.bytes(data, size)?).into_owned())
    }

    /// Cell pointers for `w` cells starting at `(x, y)` along a row, one read per chunk crossed.
    fn row_ptrs(&self, x: i32, y: i32, w: usize) -> io::Result<Vec<u32>> {
        let mut out = Vec::with_capacity(w);
        let mut cx = x;
        while out.len() < w {
            let run = (512 - (cx & 0x1FF)) as usize;
            let run = run.min(w - out.len());
            let slot = (((y >> 9) - 0x100) & 0x1FF) * 0x200 + (((cx >> 9) - 0x100) & 0x1FF);
            let chunk = self.mem.u32(self.table + 4 * slot as u32)?;
            if chunk == 0 {
                out.extend(std::iter::repeat_n(0, run));
            } else {
                let cells = self.mem.u32(chunk)?;
                let first = (((y & 0x1FF) << 9) | (cx & 0x1FF)) as u32;
                let raw = self.mem.bytes(cells + 4 * first, 4 * run)?;
                out.extend(
                    raw.as_chunks::<4>()
                        .0
                        .iter()
                        .map(|b| u32::from_le_bytes(*b)),
                );
            }
            cx += run as i32;
        }
        Ok(out)
    }

    fn decode(&mut self, ptr: u32) -> io::Result<Cell> {
        let mut raw = [0; CELL_BYTES];
        self.mem.read(ptr, &mut raw)?;
        let vptr = u32::from_le_bytes(raw[0..4].try_into().expect("4 bytes"));
        let cd = u32::from_le_bytes(raw[CELL_DATA as usize..][..4].try_into().expect("4 bytes"));
        let material = match self.ids.get(&cd) {
            Some(&id) => id,
            None => {
                let id = self.mem.u32(cd + CELL_DATA_ID)?;
                self.ids.insert(cd, id);
                id
            }
        };
        Ok(Cell {
            ptr,
            kind: Kind::from_vptr(vptr),
            material,
            raw,
        })
    }

    /// The cell at `(x, y)`, or `None` for empty space or an unloaded chunk.
    pub fn cell(&mut self, x: i32, y: i32) -> io::Result<Option<Cell>> {
        let ptr = self.row_ptrs(x, y, 1)?[0];
        (ptr != 0).then(|| self.decode(ptr)).transpose()
    }

    /// Row-major cells of the rectangle.
    pub fn rect(&mut self, x: i32, y: i32, w: usize, h: usize) -> io::Result<Vec<Option<Cell>>> {
        let mut out = Vec::with_capacity(w * h);
        for row in 0..h as i32 {
            for ptr in self.row_ptrs(x, y + row, w)? {
                out.push(if ptr == 0 {
                    None
                } else {
                    Some(self.decode(ptr)?)
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Image;

    const STATE: u32 = 0x1000_0000;
    const GRID: u32 = 0x1100_0000;
    const FACTORY: u32 = 0x1200_0000;
    const TABLE: u32 = 0x1300_0000;
    const CELLS_A: u32 = 0x2000_0000;
    const CELLS_B: u32 = 0x2100_0000;
    const CHUNK_A: u32 = 0x1400_0000;
    const CHUNK_B: u32 = 0x1500_0000;
    const NAMES: u32 = 0x1600_0000;
    const HEAP_NAME: u32 = 0x1700_0000;
    const CD_SAND: u32 = 0x1800_0000;
    const CD_ROCK: u32 = 0x1810_0000;

    fn slot(cx: i32, cy: i32) -> u32 {
        4 * ((((cy >> 9) - 0x100) & 0x1FF) * 0x200 + (((cx >> 9) - 0x100) & 0x1FF)) as u32
    }

    /// Chunks at world chunk (0,0) and (1,0); a sand cell at (511, 3), a rock at (512, 3) and a
    /// gas cell at (5, 3). Material 0 is "sand" (inline name), 1 is a long name on the heap.
    fn image() -> Image {
        let mut m = Image::default();
        m.put_u32(STATE_PTR, STATE);
        m.put_u32(STATE, 777);
        m.put_u32(STATE + STATE_GRID, GRID);
        m.put_u32(GRID, VT_GRID_WORLD);
        m.put_u32(GRID + GRID_CHUNK_TABLE, TABLE);
        for (i, b) in [-100, -200, 1000, 2000].into_iter().enumerate() {
            m.put_u32(GRID + GRID_BOUNDS + 4 * i as u32, b as u32);
        }
        m.put_u32(GRID + GRID_FACTORY, FACTORY);
        m.put_u32(FACTORY + FACTORY_NAMES, NAMES);
        m.put_u32(FACTORY + FACTORY_NAMES + 4, NAMES + 2 * NAME_STRIDE);

        let mut sand = [0u8; 0x18];
        sand[..4].copy_from_slice(b"sand");
        sand[0x10..0x14].copy_from_slice(&4u32.to_le_bytes());
        sand[0x14..].copy_from_slice(&15u32.to_le_bytes());
        m.put(NAMES, &sand);
        let long = "rock_static_with_a_long_name";
        let mut rock = [0u8; 0x18];
        rock[..4].copy_from_slice(&HEAP_NAME.to_le_bytes());
        rock[0x10..0x14].copy_from_slice(&(long.len() as u32).to_le_bytes());
        rock[0x14..].copy_from_slice(&31u32.to_le_bytes());
        m.put(NAMES + NAME_STRIDE, &rock);
        m.put(HEAP_NAME, long.as_bytes());
        m.put_u32(CD_SAND + CELL_DATA_ID, 0);
        m.put_u32(CD_ROCK + CELL_DATA_ID, 1);

        m.put(TABLE, &vec![0; 4 * 512 * 512]);
        m.put_u32(TABLE + slot(0, 0), CHUNK_A);
        m.put_u32(TABLE + slot(512, 0), CHUNK_B);
        m.put_u32(CHUNK_A, CELLS_A);
        m.put_u32(CHUNK_B, CELLS_B);
        m.put(CELLS_A, &vec![0; 4 * 512 * 512]);
        m.put(CELLS_B, &vec![0; 4 * 512 * 512]);

        let cell = |vptr: u32, cd: u32, fill: &dyn Fn(&mut [u8; CELL_BYTES])| {
            let mut c = [0u8; CELL_BYTES];
            c[..4].copy_from_slice(&vptr.to_le_bytes());
            c[0x14..0x18].copy_from_slice(&cd.to_le_bytes());
            fill(&mut c);
            c
        };
        let sand_cell = cell(VT_LIQUID, CD_SAND, &|c| {
            c[0x20] = 3;
            c[0x22] = 1;
            c[0x30..0x34].copy_from_slice(&0xFFB8_9E57u32.to_le_bytes());
        });
        let rock_cell = cell(VT_LIQUID, CD_ROCK, &|_| {});
        let gas_cell = cell(VT_GAS, CD_SAND, &|c| c[0x28] = 9);
        for (addr, bytes, cell_addr) in [
            (CELLS_A + 4 * ((3 << 9) | 511), sand_cell, 0x3000_0000u32),
            (CELLS_B + 4 * (3 << 9), rock_cell, 0x3000_1000),
            (CELLS_A + 4 * ((3 << 9) | 5), gas_cell, 0x3000_2000),
        ] {
            m.put_u32(addr, cell_addr);
            m.put(cell_addr, &bytes);
        }
        m
    }

    #[test]
    fn opens_the_grid_through_the_state_object() {
        let m = image();
        let g = Grid::open(&m, None).unwrap();
        assert_eq!((g.frame, g.grid, g.grid_vptr), (777, GRID, VT_GRID_WORLD));
        assert_eq!(g.bounds, [-100, -200, 1000, 2000]);
        assert_eq!(g.material_count(), 2);
    }

    #[test]
    fn reads_inline_and_heap_material_names() {
        let m = image();
        let g = Grid::open(&m, None).unwrap();
        assert_eq!(g.name(0).unwrap(), "sand");
        assert_eq!(g.name(1).unwrap(), "rock_static_with_a_long_name");
        assert_eq!(g.name(9).unwrap(), "#9");
    }

    #[test]
    fn decodes_cells_by_class() {
        let m = image();
        let mut g = Grid::open(&m, None).unwrap();
        let sand = g.cell(511, 3).unwrap().unwrap();
        assert_eq!((sand.kind, sand.material), (Kind::Liquid, 0));
        assert!(sand.fields().starts_with("state=3 rest=0 static=1"));
        assert!(sand.fields().contains("color=ffb89e57"));
        let gas = g.cell(5, 3).unwrap().unwrap();
        assert_eq!(gas.kind, Kind::Gas);
        assert!(gas.fields().contains("rest=9"));
        assert_eq!(gas.last_frame(), Some(0));
        assert_eq!(sand.last_frame(), None, "static cells are never stamped");
        assert!(g.cell(6, 3).unwrap().is_none(), "empty space");
    }

    #[test]
    fn a_rect_crosses_chunk_borders_with_one_row_read_per_chunk() {
        let m = image();
        let mut g = Grid::open(&m, None).unwrap();
        let cells = g.rect(510, 3, 4, 1).unwrap();
        let ids: Vec<_> = cells
            .iter()
            .map(|c| c.as_ref().map(|c| c.material))
            .collect();
        assert_eq!(ids, [None, Some(0), Some(1), None]);
    }

    #[test]
    fn unloaded_chunks_read_as_empty() {
        let m = image();
        let mut g = Grid::open(&m, None).unwrap();
        assert!(g.cell(5000, 3).unwrap().is_none());
        assert!(
            g.cell(-1, 3).unwrap().is_none(),
            "negative x lands in an absent chunk"
        );
    }

    #[test]
    fn a_grid_override_replaces_the_state_pointer() {
        let m = image();
        let g = Grid::open(&m, Some(GRID)).unwrap();
        assert_eq!(g.grid, GRID);
        assert!(Grid::open(&Image::default(), None).is_err());
    }
}

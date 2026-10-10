//! Read-only access to a 32-bit process's memory.

use std::io;

pub trait Mem {
    fn read(&self, addr: u32, buf: &mut [u8]) -> io::Result<()>;

    fn bytes(&self, addr: u32, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0; len];
        self.read(addr, &mut buf)?;
        Ok(buf)
    }

    fn u32(&self, addr: u32) -> io::Result<u32> {
        let mut b = [0; 4];
        self.read(addr, &mut b)?;
        Ok(u32::from_le_bytes(b))
    }

    fn i32(&self, addr: u32) -> io::Result<i32> {
        self.u32(addr).map(|v| v as i32)
    }
}

/// Addresses in `[start, end)` holding `value` at 4-byte alignment, up to `limit`. Unreadable
/// pages are skipped, so this walks a whole address space without knowing its mappings.
pub fn scan_u32<M: Mem>(mem: &M, value: u32, start: u32, end: u32, limit: usize) -> Vec<u32> {
    const CHUNK: u32 = 1 << 20;
    const PAGE: u32 = 4096;
    let needle = value.to_le_bytes();
    let mut found = Vec::new();
    let mut base = start & !(PAGE - 1);
    let mut buf = vec![0u8; CHUNK as usize];
    while base < end && found.len() < limit {
        let len = CHUNK.min(end - base);
        let whole = mem.read(base, &mut buf[..len as usize]).is_ok();
        let pieces: Vec<(u32, usize)> = if whole {
            vec![(base, len as usize)]
        } else {
            // Retry page by page and keep the readable ones.
            let mut pages = Vec::new();
            for p in 0..len / PAGE {
                let at = (p * PAGE) as usize;
                if mem
                    .read(base + p * PAGE, &mut buf[at..at + PAGE as usize])
                    .is_ok()
                {
                    pages.push((base + p * PAGE, PAGE as usize));
                }
            }
            pages
        };
        for (addr, n) in pieces {
            let off = (addr - base) as usize;
            for (i, w) in buf[off..off + n].as_chunks::<4>().0.iter().enumerate() {
                if *w == needle && found.len() < limit {
                    found.push(addr + 4 * i as u32);
                }
            }
        }
        match base.checked_add(CHUNK) {
            Some(b) => base = b,
            None => break,
        }
    }
    found
}

/// A running game process opened with read access only.
pub struct Process {
    handle: read_process_memory::ProcessHandle,
    pub pid: u32,
}

impl Process {
    /// Finds the first process with this image name (via `tasklist`) and opens it for reading.
    pub fn attach(image_name: &str) -> Result<Self, String> {
        let out = std::process::Command::new("tasklist")
            .args([
                "/FI",
                &format!("IMAGENAME eq {image_name}"),
                "/FO",
                "CSV",
                "/NH",
            ])
            .output()
            .map_err(|e| format!("cannot run tasklist: {e}"))?;
        let pid = parse_tasklist(&String::from_utf8_lossy(&out.stdout))
            .ok_or_else(|| format!("{image_name} is not running"))?;
        let handle = read_process_memory::ProcessHandle::try_from(pid as read_process_memory::Pid)
            .map_err(|e| format!("cannot open process {pid} for reading: {e}"))?;
        Ok(Self { handle, pid })
    }
}

impl Mem for Process {
    fn read(&self, addr: u32, buf: &mut [u8]) -> io::Result<()> {
        use read_process_memory::CopyAddress;
        self.handle.copy_address(addr as usize, buf)
    }
}

/// First PID in `tasklist /FO CSV /NH` output: `"noita.exe","1234","Console",...`.
fn parse_tasklist(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix('"')?
            .split("\",\"")
            .nth(1)?
            .parse()
            .ok()
    })
}

/// Sparse memory image for tests; later writes show through earlier, larger ones.
#[cfg(test)]
#[derive(Default)]
pub struct Image {
    regions: Vec<(u32, Vec<u8>)>,
}

#[cfg(test)]
impl Image {
    pub fn put(&mut self, addr: u32, bytes: &[u8]) {
        self.regions.push((addr, bytes.to_vec()));
    }

    pub fn put_u32(&mut self, addr: u32, value: u32) {
        self.put(addr, &value.to_le_bytes());
    }
}

#[cfg(test)]
impl Mem for Image {
    fn read(&self, addr: u32, buf: &mut [u8]) -> io::Result<()> {
        for (i, out) in buf.iter_mut().enumerate() {
            let at = u64::from(addr) + i as u64;
            let byte = self.regions.iter().rev().find_map(|(base, bytes)| {
                let off = at.checked_sub(u64::from(*base))?;
                bytes.get(usize::try_from(off).ok()?).copied()
            });
            *out = byte.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("unmapped read at {at:#x}"),
                )
            })?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_comes_from_the_first_csv_line() {
        let out = "\"noita.exe\",\"4242\",\"Console\",\"1\",\"900,000 K\"\r\n";
        assert_eq!(parse_tasklist(out), Some(4242));
        let none = "INFO: No tasks are running which match the specified criteria.\r\n";
        assert_eq!(parse_tasklist(none), None);
    }

    #[test]
    fn scan_finds_aligned_values_and_skips_unmapped_pages() {
        let mut m = Image::default();
        m.put(0x10_0000, &vec![0; 8192]);
        m.put_u32(0x10_0010, 0xDEAD_BEEF);
        m.put_u32(0x10_1ff8, 0xDEAD_BEEF);
        m.put(0x30_0000, &vec![0; 4096]);
        m.put_u32(0x30_0abc, 0xDEAD_BEEF);
        let hits = scan_u32(&m, 0xDEAD_BEEF, 0x10_0000, 0x40_0000, 10);
        assert_eq!(hits, [0x10_0010, 0x10_1ff8, 0x30_0abc]);
        assert_eq!(scan_u32(&m, 0xDEAD_BEEF, 0x10_0000, 0x40_0000, 2).len(), 2);
    }

    #[test]
    fn image_reads_within_a_region_and_rejects_gaps() {
        let mut m = Image::default();
        m.put_u32(0x1000, 0xAABB_CCDD);
        assert_eq!(m.u32(0x1000).unwrap(), 0xAABB_CCDD);
        assert_eq!(m.bytes(0x1001, 1).unwrap(), [0xCC]);
        assert!(m.u32(0x1002).is_err(), "crosses the region end");
        assert!(m.u32(0x2000).is_err());
    }
}

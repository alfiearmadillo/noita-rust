//! Minimal 32-bit PE reader: section table and VA <-> file offset mapping.

#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    pub va: u32,
    pub vsize: u32,
    pub raw: u32,
    pub raw_size: u32,
}

pub struct Pe<'a> {
    pub bytes: &'a [u8],
    pub image_base: u32,
    pub sections: Vec<Section>,
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

pub fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

impl<'a> Pe<'a> {
    /// Parses a PE32 (x86) image; returns `None` for anything else.
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        let pe = u32_at(bytes, 0x3c)? as usize;
        if bytes.get(pe..pe + 4)? != b"PE\0\0" || u16_at(bytes, pe + 4)? != 0x14c {
            return None;
        }
        let nsec = u16_at(bytes, pe + 6)? as usize;
        let opt = pe + 24;
        if u16_at(bytes, opt)? != 0x10b {
            return None;
        }
        let image_base = u32_at(bytes, opt + 28)?;
        let sec_table = opt + u16_at(bytes, pe + 20)? as usize;
        let sections = (0..nsec)
            .map(|i| {
                let o = sec_table + 40 * i;
                let raw_name = bytes.get(o..o + 8)?;
                let name = String::from_utf8_lossy(raw_name)
                    .trim_end_matches('\0')
                    .to_owned();
                Some(Section {
                    name,
                    vsize: u32_at(bytes, o + 8)?,
                    va: u32_at(bytes, o + 12)?,
                    raw_size: u32_at(bytes, o + 16)?,
                    raw: u32_at(bytes, o + 20)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            bytes,
            image_base,
            sections,
        })
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// Absolute VA -> file offset.
    pub fn va_to_off(&self, va: u32) -> Option<usize> {
        let rva = va.checked_sub(self.image_base)?;
        self.sections.iter().find_map(|s| {
            let d = rva.checked_sub(s.va)?;
            (d < s.raw_size).then(|| (s.raw + d) as usize)
        })
    }

    /// File offset -> absolute VA.
    pub fn off_to_va(&self, off: usize) -> Option<u32> {
        let off = u32::try_from(off).ok()?;
        self.sections.iter().find_map(|s| {
            let d = off.checked_sub(s.raw)?;
            (d < s.raw_size).then(|| self.image_base + s.va + d)
        })
    }

    pub fn u32_at_va(&self, va: u32) -> Option<u32> {
        u32_at(self.bytes, self.va_to_off(va)?)
    }

    /// Absolute VA range `[start, end)` of a section.
    pub fn va_range(&self, s: &Section) -> (u32, u32) {
        let start = self.image_base + s.va;
        (start, start + s.vsize)
    }

    /// NUL-terminated printable ASCII string at `va` (at least `min_len` chars).
    pub fn ascii_at_va(&self, va: u32, min_len: usize) -> Option<String> {
        let off = self.va_to_off(va)?;
        let tail = self.bytes.get(off..)?;
        let len = tail.iter().take(256).position(|&c| c == 0)?;
        let s = &tail[..len];
        (len >= min_len
            && s.iter()
                .all(|c| (0x20..0x7f).contains(c) || *c == b'\t' || *c == b'\n'))
        .then(|| String::from_utf8_lossy(s).into_owned())
    }
}

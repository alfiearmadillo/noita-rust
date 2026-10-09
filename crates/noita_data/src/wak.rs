//! `data.wak` archive reader.
//!
//! Layout (determined from the reference build's file, see README):
//! header `u32 0, u32 file_count, u32 toc_end, u32 0`, then `file_count` entries of
//! `u32 offset, u32 size, u32 name_len, name bytes`. All integers little-endian; no compression.

use std::fmt;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WakError {
    Truncated { at: usize },
    BadHeader,
    NameNotUtf8 { index: usize },
    UnsafePath(String),
    DataOutOfRange(String),
}

impl fmt::Display for WakError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { at } => write!(f, "archive truncated at byte {at}"),
            Self::BadHeader => write!(f, "unrecognized wak header"),
            Self::NameNotUtf8 { index } => write!(f, "entry {index} name is not UTF-8"),
            Self::UnsafePath(p) => write!(f, "entry path escapes output directory: {p:?}"),
            Self::DataOutOfRange(p) => write!(f, "entry data out of range: {p:?}"),
        }
    }
}

impl std::error::Error for WakError {}

/// A parsed archive borrowing the raw file bytes.
pub struct Wak<'a> {
    bytes: &'a [u8],
    entries: Vec<Entry>,
}

impl<'a> Wak<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, WakError> {
        let mut r = Reader { bytes, pos: 0 };
        let (zero_a, count, toc_end, zero_b) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
        if zero_a != 0 || zero_b != 0 || toc_end as usize > bytes.len() {
            return Err(WakError::BadHeader);
        }
        let mut entries = Vec::with_capacity((count as usize).min(bytes.len() / 12));
        for index in 0..count as usize {
            let (offset, size, name_len) = (r.u32()?, r.u32()?, r.u32()?);
            let name = r.take(name_len as usize)?;
            let path = std::str::from_utf8(name)
                .map_err(|_| WakError::NameNotUtf8 { index })?
                .to_owned();
            if relative_path(&path).is_none() {
                return Err(WakError::UnsafePath(path));
            }
            let end = offset as usize + size as usize;
            if (offset as usize) < toc_end as usize || end > bytes.len() {
                return Err(WakError::DataOutOfRange(path));
            }
            entries.push(Entry { path, offset, size });
        }
        Ok(Self { bytes, entries })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Contents of the entry with exactly this archive path (e.g. `data/materials.xml`).
    pub fn get(&self, path: &str) -> Option<&'a [u8]> {
        self.entries
            .iter()
            .find(|e| e.path == path)
            .map(|e| self.data(e))
    }

    pub fn data(&self, entry: &Entry) -> &'a [u8] {
        &self.bytes[entry.offset as usize..(entry.offset + entry.size) as usize]
    }

    /// Writes every entry under `out_dir`, preserving archive paths.
    pub fn extract_all(&self, out_dir: &Path) -> std::io::Result<()> {
        for entry in &self.entries {
            let rel = relative_path(&entry.path).expect("validated in parse");
            let target = out_dir.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, self.data(entry))?;
        }
        Ok(())
    }
}

/// Archive path as a relative path with only normal components, or `None` if it could escape.
fn relative_path(path: &str) -> Option<PathBuf> {
    let p = Path::new(path);
    let ok = !path.is_empty()
        && !path.contains(':')
        && p.components().all(|c| matches!(c, Component::Normal(_)));
    ok.then(|| p.to_path_buf())
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], WakError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.bytes.len())
            .ok_or(WakError::Truncated { at: self.pos })?;
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, WakError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let toc_len: usize = 16 + files.iter().map(|(n, _)| 12 + n.len()).sum::<usize>();
        let mut out = Vec::new();
        for v in [0, files.len() as u32, toc_len as u32, 0] {
            out.extend(v.to_le_bytes());
        }
        let mut offset = toc_len as u32;
        for (name, data) in files {
            for v in [offset, data.len() as u32, name.len() as u32] {
                out.extend(v.to_le_bytes());
            }
            out.extend(name.as_bytes());
            offset += data.len() as u32;
        }
        for (_, data) in files {
            out.extend(*data);
        }
        out
    }

    #[test]
    fn parses_entries_and_contents() {
        let bytes = build(&[("data/a.txt", b"hello"), ("data/sub/b.xml", b"<x/>")]);
        let wak = Wak::parse(&bytes).unwrap();
        assert_eq!(wak.entries().len(), 2);
        assert_eq!(wak.get("data/a.txt"), Some(&b"hello"[..]));
        assert_eq!(wak.get("data/sub/b.xml"), Some(&b"<x/>"[..]));
        assert_eq!(wak.get("data/missing"), None);
    }

    #[test]
    fn rejects_paths_that_escape_output_dir() {
        for bad in [
            "../evil",
            "data/../../evil",
            "/abs",
            "C:/evil",
            "C:evil",
            "",
        ] {
            let bytes = build(&[(bad, b"x")]);
            assert!(
                matches!(Wak::parse(&bytes), Err(WakError::UnsafePath(_))),
                "{bad:?} accepted"
            );
        }
    }

    #[test]
    fn rejects_truncated_and_out_of_range() {
        let bytes = build(&[("data/a.txt", b"hello")]);
        assert!(matches!(
            Wak::parse(&bytes[..10]),
            Err(WakError::Truncated { .. })
        ));
        assert!(matches!(
            Wak::parse(&bytes[..bytes.len() - 1]),
            Err(WakError::DataOutOfRange(_))
        ));
    }

    #[test]
    fn extract_all_writes_files() {
        let bytes = build(&[("data/sub/b.xml", b"<x/>")]);
        let dir = std::env::temp_dir().join(format!("wak_test_{}", std::process::id()));
        Wak::parse(&bytes).unwrap().extract_all(&dir).unwrap();
        assert_eq!(std::fs::read(dir.join("data/sub/b.xml")).unwrap(), b"<x/>");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

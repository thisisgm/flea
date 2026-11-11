// A font's facts from its own tables, with no decoder: family and style from `name`, glyphs from `maxp`; the sample job and wire lines are fontinfo/render.rs.
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

mod cache;
pub mod render;
pub mod sample;

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct Facts {
    pub family: Option<String>,
    pub style: Option<String>,
    pub glyphs: Option<u32>,
}

// The facts of the font at path; a file that cannot be opened, or that holds no sfnt, has none.
pub fn read(path: &Path) -> Facts {
    let Ok(mut file) = File::open(path) else { return Facts::default() };
    let Ok(len) = file.metadata().map(|m| m.len()) else { return Facts::default() };
    parse(&mut file, len)
}

// A name table is a few KB; one past this is not read.
const NAME_CAP: u64 = 1 << 20;
const MAX_TEXT_CHARS: usize = 128;
// 'true', 'OTTO', 'typ1' and the TrueType number 1.0.
const SFNT_VERSIONS: [u32; 4] = [0x0001_0000, 0x4F54_544F, 0x7472_7565, 0x7479_7031];
const TTC_TAG: u32 = 0x7474_6366;
const ENGLISH_US: u16 = 0x0409;

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

// n bytes at off, or None when any of them lies past len.
fn bytes_at<R: Read + Seek>(r: &mut R, len: u64, off: u64, n: u64) -> Option<Vec<u8>> {
    if off.checked_add(n)? > len {
        return None;
    }
    r.seek(std::io::SeekFrom::Start(off)).ok()?;
    let mut buf = vec![0u8; usize::try_from(n).ok()?];
    r.read_exact(&mut buf).ok()?;
    Some(buf)
}

// Sample input: bytes 00 01 00 00 | 00 02 ... (sfnt version, table count), then 16 bytes per table, then the tables; every offset is checked against len.
pub fn parse<R: Read + Seek>(r: &mut R, len: u64) -> Facts {
    let Some(dir) = directory(r, len) else { return Facts::default() };
    let table = |tag: &[u8; 4]| table_in(&dir, tag);
    let glyphs = table(b"maxp")
        .and_then(|(off, size)| if size >= 6 { bytes_at(r, len, off, 6) } else { None })
        .and_then(|b| be16(&b, 4))
        .filter(|n| *n > 0)
        .map(u32::from);
    let names = table(b"name").filter(|(_, size)| *size <= NAME_CAP).and_then(|(off, size)| bytes_at(r, len, off, size));
    let pick = |ids: [u16; 2]| names.as_deref().and_then(|t| ids.iter().find_map(|id| name_for(t, *id)));
    Facts { family: pick([16, 1]), style: pick([17, 2]), glyphs }
}

// The table directory of the file's first face, a collection's included; None when the file holds no sfnt.
fn directory<R: Read + Seek>(r: &mut R, len: u64) -> Option<Vec<u8>> {
    let face = match bytes_at(r, len, 0, 16).and_then(|h| (be32(&h, 0) == Some(TTC_TAG) && be32(&h, 8)? > 0).then(|| be32(&h, 12))) {
        Some(first) => u64::from(first.unwrap_or(0)),
        None => 0,
    };
    let head = bytes_at(r, len, face, 12)?;
    let count = usize::from(be16(&head, 4).unwrap_or(0));
    if !SFNT_VERSIONS.contains(&be32(&head, 0).unwrap_or(0)) || count == 0 {
        return None;
    }
    bytes_at(r, len, face + 12, 16 * count as u64)
}

// One table's offset and size from the directory.
fn table_in(dir: &[u8], tag: &[u8; 4]) -> Option<(u64, u64)> {
    let rec = dir.chunks_exact(16).find(|rec| rec[..4] == tag[..])?;
    Some((u64::from(be32(rec, 8)?), u64::from(be32(rec, 12)?)))
}

// Sample input: a name table, 00 00 | 00 01 | 00 12 (format, count, storage), then 12 bytes per record: 00 03 00 01 04 09 00 01 00 0A 00 00, and the UTF-16BE text "Inter".
fn name_for(table: &[u8], id: u16) -> Option<String> {
    let count = usize::from(be16(table, 2)?);
    let storage = usize::from(be16(table, 4)?);
    let mut best: Option<(u8, String)> = None;
    for i in 0..count {
        let at = 6 + 12 * i;
        let (platform, encoding, lang) = (be16(table, at)?, be16(table, at + 2)?, be16(table, at + 4)?);
        if be16(table, at + 6)? != id {
            continue;
        }
        let (size, off) = (usize::from(be16(table, at + 8)?), usize::from(be16(table, at + 10)?));
        let Some(raw) = table.get(storage + off..storage + off + size) else { continue };
        // Best first: Windows Unicode English, Windows Unicode, Windows Symbol, Unicode, then Mac ASCII.
        let rank = match (platform, encoding) {
            (3, 1 | 10) => if lang == ENGLISH_US { 0 } else { 1 },
            (3, 0) => 2,
            (0, _) => 3,
            (1, 0) => 4,
            _ => continue,
        };
        if best.as_ref().is_some_and(|(r, _)| *r <= rank) {
            continue;
        }
        let text = if platform == 1 {
            raw.is_ascii().then(|| String::from_utf8_lossy(raw).into_owned())
        } else if raw.len() % 2 == 0 {
            String::from_utf16(&raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>()).ok()
        } else {
            None
        };
        let Some(text) = text else { continue };
        let text: String = text.trim().chars().take(MAX_TEXT_CHARS).collect();
        if !text.is_empty() {
            best = Some((rank, text));
        }
    }
    best.map(|(_, text)| text)
}

#[cfg(test)]
#[path = "fontinfo_tests.rs"]
mod tests;

// The RAW container walker: the largest embedded JPEG preview and the EXIF facts of a camera RAW, from headers and directories only.
use crate::backend::regfile::open_regular;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

// One candidate's SOF walk never ranges past this, and neither does the APP1 scan.
pub(crate) const JPEG_PREFIX: u64 = 262_144;
// Header and directory bytes read per file in total; SOF-walk bytes are counted separately.
pub(crate) const HEADER_BUDGET: usize = 1_048_576;
// SOF-walk bytes pulled per file across all candidates; then validation stops, not the file.
pub(crate) const SOF_BUDGET: usize = 1_048_576;
// Box payload and RAF prefix bytes pulled per file in total; then those reads stop, not the file.
pub(crate) const BLOB_BUDGET: usize = 1_048_576;
// No read issues more than this in one call, so no allocation comes from an untrusted count.
pub(crate) const MAX_BLOB: u64 = 1_048_576;
// More candidates than this are never validated; a real file holds a handful.
pub(crate) const MAX_CANDIDATES: usize = 16;

// Sample input: a TIFF header "II*\0" ... or a BMFF "ftyp" ... "crx ".
const TIFF_LE: &[u8] = b"II*\x00";
const TIFF_BE: &[u8] = b"MM\x00*";
const ORF_A: &[u8] = b"IIRO";
const ORF_B: &[u8] = b"IIRS";
const RW2_MAGIC: &[u8] = b"IIU\x00";
const RAF_MAGIC: &[u8] = b"FUJIFILMCCD-RAW ";
const CR3_FTYP: &[u8] = b"ftyp";
const CR3_BRAND: &[u8] = b"crx ";

pub use flea::rawname::is_raw_name;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Preview {
    pub offset: u64,
    pub length: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawFacts {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    pub exposure: Option<(u32, u32)>,
    pub f_number: Option<(u32, u32)>,
    pub iso: Option<u32>,
    pub focal_mm: Option<(u32, u32)>,
    pub taken: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub orientation: Option<u16>,
}

// A file plus its length plus the two budgets every walk counts against.
pub(crate) struct Ctx {
    file: File,
    pub len: u64,
    budget: usize,
    blob_left: usize,
    sof_left: usize,
}

impl Ctx {
    pub(crate) fn open(path: &Path) -> Option<Ctx> {
        let file = open_regular(path)?;
        let len = file.metadata().ok()?.len();
        Some(Ctx {
            file,
            len,
            budget: HEADER_BUDGET,
            blob_left: BLOB_BUDGET,
            sof_left: SOF_BUDGET,
        })
    }

    // Directory and structure bytes: bounded by the file, the remaining budget and one blob cap.
    pub(crate) fn read_at(&mut self, off: u64, n: usize) -> Option<Vec<u8>> {
        if n > self.budget || (n as u64) > MAX_BLOB {
            return None;
        }
        let end = off.checked_add(n as u64)?;
        if end > self.len {
            return None;
        }
        self.budget -= n;
        self.file.seek(SeekFrom::Start(off)).ok()?;
        let mut v = vec![0u8; n];
        self.file.read_exact(&mut v).ok()?;
        Some(v)
    }

    // Box payloads and the RAF prefix: bounded by the file, the blob cap and a budget of their own.
    pub(crate) fn read_blob(&mut self, off: u64, n: u64) -> Option<Vec<u8>> {
        if n == 0 || n > MAX_BLOB || n > self.blob_left as u64 {
            return None;
        }
        let end = off.checked_add(n)?;
        if end > self.len {
            return None;
        }
        self.blob_left -= n as usize;
        self.file.seek(SeekFrom::Start(off)).ok()?;
        let mut v = vec![0u8; n as usize];
        self.file.read_exact(&mut v).ok()?;
        Some(v)
    }
}

// Walks the candidate's segments through a Take capped at its span, so a preview costs its headers, and the SOF marker is the verdict.
fn sof_first(r: &mut BufReader<std::io::Take<&mut File>>) -> Option<(u8, u32, u32)> {
    let mut two = [0u8; 2];
    r.read_exact(&mut two).ok()?;
    if two != [0xFF, 0xD8] {
        return None;
    }
    let mut one = [0u8; 1];
    let mut scratch = [0u8; 8192];
    loop {
        loop {
            r.read_exact(&mut one).ok()?;
            if one[0] == 0xFF {
                break;
            }
        }
        let m = loop {
            r.read_exact(&mut one).ok()?;
            if one[0] != 0xFF {
                break one[0];
            }
        };
        if m == 0x00 {
            return None;
        }
        if m == 0x01 || (0xD0..=0xD8).contains(&m) {
            continue;
        }
        if m == 0xDA || m == 0xD9 {
            return None;
        }
        let mut lb = [0u8; 2];
        r.read_exact(&mut lb).ok()?;
        let ln = u16::from_be_bytes(lb) as u64;
        if ln < 2 {
            return None;
        }
        let frame = (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC;
        if frame {
            let mut head = [0u8; 5];
            r.read_exact(&mut head).ok()?;
            let h = u16::from_be_bytes([head[1], head[2]]) as u32;
            let w = u16::from_be_bytes([head[3], head[4]]) as u32;
            return Some((m, w, h));
        }
        // Skipped by discarding rather than seeking, so the Take still bounds the total.
        let mut to_go = ln - 2;
        while to_go > 0 {
            let n = to_go.min(scratch.len() as u64) as usize;
            r.read_exact(&mut scratch[..n]).ok()?;
            to_go -= n as u64;
        }
    }
}

// A candidate is valid only if its first frame is Huffman-coded (SOF0, SOF1, SOF2); SOF3 and the arithmetic frames hold raw sensor data.
pub(crate) fn preview_dims(ctx: &mut Ctx, off: u64, len: u64) -> Option<(u32, u32)> {
    // A span running past the end of the file is a cut file, not a preview.
    if len < 20 || ctx.sof_left == 0 || !matches!(off.checked_add(len), Some(end) if end <= ctx.len) {
        return None;
    }
    let span = len.min(JPEG_PREFIX).min(ctx.sof_left as u64);
    if span < 20 {
        return None;
    }
    ctx.file.seek(SeekFrom::Start(off)).ok()?;
    let (spent, found) = {
        let mut r = BufReader::new(ctx.file.by_ref().take(span));
        let found = sof_first(&mut r);
        (span - r.into_inner().limit(), found)
    };
    ctx.sof_left = ctx.sof_left.saturating_sub(spent as usize);
    let (m, w, h) = found?;
    if m == 0xC0 || m == 0xC1 || m == 0xC2 {
        Some((w, h))
    } else {
        None
    }
}

fn choose(ctx: &mut Ctx, cands: &[(u64, u64)]) -> Option<Preview> {
    let mut best: Option<Preview> = None;
    for &(off, len) in cands.iter().take(MAX_CANDIDATES) {
        let Some((width, height)) = preview_dims(ctx, off, len) else {
            continue;
        };
        let px = width as u64 * height as u64;
        let wins = match &best {
            None => true,
            Some(b) => {
                let bp = b.width as u64 * b.height as u64;
                px > bp || (px == bp && len > b.length)
            }
        };
        if wins {
            best = Some(Preview {
                offset: off,
                length: len,
                width,
                height,
            });
        }
    }
    best
}

fn is_tiff(head: &[u8]) -> bool {
    head.starts_with(TIFF_LE)
        || head.starts_with(TIFF_BE)
        || head.starts_with(ORF_A)
        || head.starts_with(ORF_B)
        || head.starts_with(RW2_MAGIC)
}

// Sample input: the file's first 16 bytes, which carry every magic this walker dispatches on.
pub fn inspect(path: &Path) -> Option<(Option<Preview>, RawFacts)> {
    let mut ctx = Ctx::open(path)?;
    let head = ctx.read_at(0, 16)?;
    if head.starts_with(RAF_MAGIC) {
        let (cands, facts) = super::rawraf::inspect_file(&mut ctx)?;
        return Some((choose(&mut ctx, &cands), facts));
    }
    if head.len() >= 12 && &head[4..8] == CR3_FTYP && &head[8..12] == CR3_BRAND {
        let (cands, facts) = super::rawbmff::inspect_file(&mut ctx)?;
        return Some((choose(&mut ctx, &cands), facts));
    }
    if is_tiff(&head) {
        let (cands, facts) = super::rawtiff::inspect_file(&mut ctx)?;
        return Some((choose(&mut ctx, &cands), facts));
    }
    None
}

#[cfg(test)]
#[path = "rawembed_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "rawshape_tests.rs"]
mod shape_tests;
#[cfg(test)]
#[path = "rawhard_tests.rs"]
mod hard_tests;

// Header-only, bounded sizes for the kinds imagesize.rs does not read: TIFF, ICO, HEIF containers, SVG and SVGZ.
use crate::backend::imagesize::{self, Header};
use crate::backend::imagevector::{gunzip, svg};
use crate::backend::regfile::open_regular;
use flea::ui_logic_preview_core::pixels::{is_heif_name, is_host_decoded, is_vector_name};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

// The first bytes name the kind, and hold every ICO directory and SVG root this reads.
const PROBE: u64 = 65_536;
// A TIFF directory past this many entries, or a HEIF meta box past this size, is not a picture to measure.
const TIFF_ENTRIES: usize = 4096;
const BMFF_META: u64 = 1024 * 1024;
const BMFF_BOXES: usize = 64;

pub fn header(path: &Path) -> Option<Header> {
    if let Some(found) = imagesize::header(path) {
        return Some(found);
    }
    // Only the names Quick Look decodes itself are measured here: a DNG is TIFF inside, and its RAW facts stand.
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if !is_host_decoded(name) {
        return None;
    }
    let mut file = open_regular(path)?;
    let mut head = Vec::new();
    (&mut file).take(PROBE).read_to_end(&mut head).ok()?;
    // Each name takes the decoder the card picks for it: the SVG renderer, libheif, or the image crate's TIFF and ICO.
    let (w, h, orientation) = if is_vector_name(name) {
        if head.starts_with(&[0x1f, 0x8b]) {
            svg(&gunzip(&head, &mut file)?)?
        } else {
            svg(&head)?
        }
    } else if is_heif_name(name) {
        if head.get(4..8) != Some(b"ftyp") || !heif_brand(&head) {
            return None;
        }
        avif(&mut file)?
    } else if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        tiff(&mut file)?
    } else if head.starts_with(&[0, 0, 1, 0]) {
        ico(&head, &mut file)?
    } else {
        return None;
    };
    (w > 0 && h > 0).then_some(Header {
        width: w,
        height: h,
        orientation,
    })
}

fn at<const N: usize>(file: &mut File, offset: u64) -> Option<[u8; N]> {
    let mut b = [0u8; N];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut b).ok()?;
    Some(b)
}

// Sample input: "II" 2A 00 | IFD offset(4) ... IFD: count(2), then 12-byte entries tag(2) type(2) count(4) value(4).
fn tiff(file: &mut File) -> Option<(u32, u32, u8)> {
    let little = at::<2>(file, 0)? == *b"II";
    let u16_of = |b: [u8; 2]| {
        if little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        }
    };
    let u32_of = |b: [u8; 4]| {
        if little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        }
    };
    let ifd = u64::from(u32_of(at::<4>(file, 4)?));
    let count = usize::from(u16_of(at::<2>(file, ifd)?));
    if count > TIFF_ENTRIES {
        return None;
    }
    let mut table = vec![0u8; count * 12];
    file.read_exact(&mut table).ok()?;
    let (mut w, mut h, mut orientation) = (None, None, 1u8);
    for entry in table.chunks_exact(12) {
        let tag = u16_of([entry[0], entry[1]]);
        // SHORT holds its value in the first two bytes of the value field, LONG in all four.
        let value = match u16_of([entry[2], entry[3]]) {
            3 => u32::from(u16_of([entry[8], entry[9]])),
            4 => u32_of([entry[8], entry[9], entry[10], entry[11]]),
            _ => continue,
        };
        match tag {
            256 => w = Some(value),
            257 => h = Some(value),
            // The card's decoder turns the TIFF by this tag, so the card fits the turned sides.
            274 if (1..=8).contains(&value) => orientation = value as u8,
            _ => {}
        }
    }
    Some((w?, h?, orientation))
}

// Sample input: 00 00 01 00 count(2) | 16-byte entries w h colours 0 planes(2) bpp(2) size(4) offset(4).
// The image crate's choice: the deepest colour, then the largest area, starting from the last entry.
fn ico(head: &[u8], file: &mut File) -> Option<(u32, u32, u8)> {
    let count = usize::from(u16::from_le_bytes([*head.get(4)?, *head.get(5)?]));
    let side = |b: u8| if b == 0 { 256 } else { u32::from(b) };
    let entries: Vec<&[u8]> = (0..count)
        .map(|i| head.get(6 + 16 * i..22 + 16 * i))
        .collect::<Option<_>>()?;
    let score = |e: &[u8]| (u16::from_le_bytes([e[6], e[7]]), side(e[0]) * side(e[1]));
    let (last, rest) = entries.split_last()?;
    let mut best = (score(last), *last);
    for entry in rest {
        if score(entry) > best.0 {
            best = (score(entry), *entry);
        }
    }
    let (_, entry) = best;
    let offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]);
    let start = at::<24>(file, u64::from(offset));
    // A PNG entry is measured by its own header; a bitmap entry by the directory.
    match start {
        Some(png) if png.starts_with(b"\x89PNG\r\n\x1a\n") => {
            let (w, h) = imagesize::from_header(&png)?;
            Some((w, h, 1))
        }
        _ => Some((side(entry[0]), side(entry[1]), 1)),
    }
}

// Sample input: size(4) "ftyp" major(4) minor(4) compatible brands(4 each), such as "avif" "mif1" "miaf".
fn heif_brand(head: &[u8]) -> bool {
    let size = head
        .get(0..4)
        .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let supported = |brand: &[u8]| {
        matches!(
            brand,
            b"avif"
                | b"avis"
                | b"heic"
                | b"heix"
                | b"hevc"
                | b"hevx"
                | b"heim"
                | b"heis"
                | b"hevm"
                | b"hevs"
                | b"mif1"
                | b"msf1"
        )
    };
    supported(head.get(8..12).unwrap_or(&[]))
        || head
            .get(16..size.min(head.len()))
            .unwrap_or(&[])
            .chunks_exact(4)
            .any(supported)
}

// Sample input: size(4) type(4) [largesize(8)] body; answers each child's type and body.
fn boxes(body: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut rest = body;
    std::iter::from_fn(move || {
        let size = u32::from_be_bytes(rest.get(0..4)?.try_into().ok()?) as usize;
        let kind = rest.get(4..8)?;
        let (start, end) = match size {
            0 => (8, rest.len()),
            1 => (
                16,
                usize::try_from(u64::from_be_bytes(rest.get(8..16)?.try_into().ok()?)).ok()?,
            ),
            n => (8, n),
        };
        let inner = rest.get(start..end)?;
        rest = &rest[end..];
        Some((kind, inner))
    })
}

fn child<'a>(body: &'a [u8], kind: &[u8]) -> Option<&'a [u8]> {
    boxes(body).find(|(k, _)| *k == kind).map(|(_, b)| b)
}

// Sample input: ftyp | meta(full) { pitm item | iprp { ipco { ispe w h, irot } ipma item -> property indices } }.
// libheif applies irot while decoding, so a quarter turn swaps the sides and leaves orientation 1.
fn avif(file: &mut File) -> Option<(u32, u32, u8)> {
    let mut offset = 0u64;
    let mut meta = None;
    for _ in 0..BMFF_BOXES {
        let Some(top) = at::<8>(file, offset) else {
            break;
        };
        let size = match u32::from_be_bytes([top[0], top[1], top[2], top[3]]) {
            1 => u64::from_be_bytes(at::<8>(file, offset + 8)?),
            n => u64::from(n),
        };
        if &top[4..8] == b"meta" {
            if !(12..=BMFF_META).contains(&size) {
                return None;
            }
            let mut body = vec![0u8; size as usize - 8];
            file.seek(SeekFrom::Start(offset + 8)).ok()?;
            file.read_exact(&mut body).ok()?;
            meta = Some(body);
            break;
        }
        if size < 8 {
            return None;
        }
        offset += size;
    }
    let meta = meta?;
    let inner = meta.get(4..)?;
    let pitm = child(inner, b"pitm")?;
    let primary = if pitm.first()? == &0 {
        u32::from(u16::from_be_bytes(pitm.get(4..6)?.try_into().ok()?))
    } else {
        u32::from_be_bytes(pitm.get(4..8)?.try_into().ok()?)
    };
    let iprp = child(inner, b"iprp")?;
    let props: Vec<(&[u8], &[u8])> = boxes(child(iprp, b"ipco")?).collect();
    let ipma = child(iprp, b"ipma")?;
    let (version, flags) = (*ipma.first()?, *ipma.get(3)?);
    let mut pos = 4;
    let take = |pos: &mut usize, n: usize| -> Option<u32> {
        let b = ipma.get(*pos..*pos + n)?;
        *pos += n;
        Some(b.iter().fold(0u32, |acc, &v| acc << 8 | u32::from(v)))
    };
    let entries = take(&mut pos, 4)?;
    let (mut size, mut quarter) = (None, false);
    for _ in 0..entries {
        let item = take(&mut pos, if version < 1 { 2 } else { 4 })?;
        let links = take(&mut pos, 1)?;
        for _ in 0..links {
            let index = if flags & 1 == 1 {
                take(&mut pos, 2)? & 0x7fff
            } else {
                take(&mut pos, 1)? & 0x7f
            };
            if item != primary || index == 0 {
                continue;
            }
            let (kind, body) = *props.get(index as usize - 1)?;
            if kind == b"ispe" {
                let w = u32::from_be_bytes(body.get(4..8)?.try_into().ok()?);
                let h = u32::from_be_bytes(body.get(8..12)?.try_into().ok()?);
                size = Some((w, h));
            } else if kind == b"irot" {
                quarter = body.first()? & 1 == 1;
            }
        }
    }
    let (w, h) = size?;
    Some(if quarter { (h, w, 1) } else { (w, h, 1) })
}

#[cfg(test)]
#[path = "imageformats_tests.rs"]
mod tests;

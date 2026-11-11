// The specimen a font can draw: the board's sentence when its cmap covers it, else a sentence or alphabet in a script it covers, else the first letters it maps.
use super::{be16, be32, bytes_at, directory, table_in};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

// A CJK cmap is a few hundred KB; one past this is not read.
const CMAP_CAP: u64 = 1 << 20;
// Letters the last fallback takes from the cmap, about the board sentence's length.
const RUN: usize = 32;
// Tried in order once the board's sentence is not covered: two pangrams, then alphabets as code point ranges.
const SENTENCES: [&str; 2] = [
    "Ξεσκεπάζω την ψυχοφθόρα βδελυγμία",
    "Съешь же ещё этих мягких французских булок",
];
const ALPHABETS: [(u32, u32); 6] = [
    (0x10D0, 0x10F0), // Georgian Mkhedruli
    (0x0561, 0x0586), // Armenian
    (0x05D0, 0x05EA), // Hebrew
    (0x0E01, 0x0E2E), // Thai consonants
    (0x0915, 0x0939), // Devanagari consonants
    (0x0985, 0x09B9), // Bengali letters
];

// The Unicode ranges the font at path maps; empty when its cmap cannot be read, which keeps the board's sentence.
pub fn ranges(path: &Path) -> Vec<(u32, u32)> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = file.metadata().map(|m| m.len()) else {
        return Vec::new();
    };
    read(&mut file, len).unwrap_or_default()
}

// Sample input: ranges [(0x20, 0x7e)] keep "The quick brown fox"; [(0x10d0, 0x10f0)] give the Georgian alphabet.
pub fn choose(text: &str, ranges: &[(u32, u32)]) -> String {
    let covered = |c: char| {
        c.is_whitespace()
            || ranges
                .iter()
                .any(|&(first, last)| (first..=last).contains(&u32::from(c)))
    };
    if ranges.is_empty() || text.chars().all(covered) {
        return text.to_string();
    }
    if let Some(sentence) = SENTENCES.iter().find(|s| s.chars().all(covered)) {
        return sentence.to_string();
    }
    let alphabet = |&(first, last): &(u32, u32)| {
        (first..=last)
            .filter_map(char::from_u32)
            .collect::<String>()
    };
    if let Some(letters) = ALPHABETS
        .iter()
        .map(alphabet)
        .find(|letters| letters.chars().all(covered))
    {
        return letters;
    }
    let letters: String = ranges
        .iter()
        .flat_map(|&(first, last)| first..=last)
        .filter_map(char::from_u32)
        .filter(|c| c.is_alphabetic())
        .take(RUN)
        .collect();
    if letters.is_empty() {
        text.to_string()
    } else {
        letters
    }
}

// Sample input: a cmap with a (3,10) format 12 record and a (3,1) format 4 record; the full-repertoire one is read.
fn read<R: Read + Seek>(r: &mut R, len: u64) -> Option<Vec<(u32, u32)>> {
    let dir = directory(r, len)?;
    let (off, size) = table_in(&dir, b"cmap").filter(|(_, size)| *size <= CMAP_CAP)?;
    let cmap = bytes_at(r, len, off, size)?;
    let count = usize::from(be16(&cmap, 2)?);
    // Best first: Unicode full repertoire, Unicode BMP, then a symbol font's own codes.
    let rank = |platform, encoding| match (platform, encoding) {
        (3, 10) | (0, 4) | (0, 6) => Some(0),
        (3, 1) | (0, 0..=3) => Some(1),
        (3, 0) => Some(2),
        _ => None,
    };
    let mut records: Vec<(u8, usize)> = (0..count)
        .filter_map(|i| {
            let at = 4 + 8 * i;
            Some((
                rank(be16(&cmap, at)?, be16(&cmap, at + 2)?)?,
                usize::try_from(be32(&cmap, at + 4)?).ok()?,
            ))
        })
        .collect();
    records.sort_unstable();
    records
        .into_iter()
        .find_map(|(_, at)| subtable(&cmap, at))
        .map(merged)
        .filter(|ranges| !ranges.is_empty())
}

// The ranges clamped to Unicode, sorted and joined, so a crafted cmap's groups walk each code point at most once.
fn merged(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    let last_char = u32::from(char::MAX);
    ranges.retain(|&(first, last)| first <= last && first <= last_char);
    ranges.sort_unstable();
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (first, last) in ranges {
        let last = last.min(last_char);
        match out.last_mut() {
            Some(joined) if first <= joined.1.saturating_add(1) => joined.1 = joined.1.max(last),
            _ => out.push((first, last)),
        }
    }
    out
}

// Sample input: format 12 is 00 0C, pad, length, language, group count, then start, end, glyph (u32 each) per group; format 4 is 00 04, length, language, 2 x segments, three search fields, then ends, a pad, starts.
// Format 12 groups or format 4 segments as code point ranges; any other format is skipped.
fn subtable(cmap: &[u8], at: usize) -> Option<Vec<(u32, u32)>> {
    match be16(cmap, at)? {
        12 => {
            let groups = usize::try_from(be32(cmap, at + 12)?).ok()?;
            (0..groups)
                .map(|g| Some((be32(cmap, at + 16 + 12 * g)?, be32(cmap, at + 20 + 12 * g)?)))
                .collect()
        }
        4 => {
            let segments = usize::from(be16(cmap, at + 6)? / 2);
            let (ends, starts) = (at + 14, at + 16 + 2 * segments);
            // The last segment is the 0xFFFF terminator every format 4 table ends with.
            (0..segments)
                .map(|s| {
                    Some((
                        u32::from(be16(cmap, starts + 2 * s)?),
                        u32::from(be16(cmap, ends + 2 * s)?),
                    ))
                })
                .filter(|range| *range != Some((0xFFFF, 0xFFFF)))
                .collect()
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "sample_tests.rs"]
pub(crate) mod tests;

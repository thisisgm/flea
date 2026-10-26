// SVG and SVGZ sizes as usvg reads the root element; SVGZ inflates under the card's own 4 MB vector bound.
use crate::backend::model3d::codecs::{crc32, inflate};
use flea::ui_logic_pv1::pixels::MAX_SVG_BYTES;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

// Every SVG root this reads sits in the first bytes of the document.
const PROBE: usize = 65_536;

// Sample input: 1F 8B 08 flags mtime(4) xfl os [extra] [name\0] [comment\0] [crc16] deflate | crc32 isize.
pub(crate) fn gunzip(head: &[u8], file: &mut File) -> Option<Vec<u8>> {
    let len = file.metadata().ok()?.len();
    if len > MAX_SVG_BYTES || len < 18 || *head.get(2)? != 8 {
        return None;
    }
    let mut all = head.to_vec();
    if len > all.len() as u64 {
        file.seek(SeekFrom::Start(all.len() as u64)).ok()?;
        file.take(MAX_SVG_BYTES + 1 - all.len() as u64)
            .read_to_end(&mut all)
            .ok()?;
    }
    let flags = all[3];
    let mut pos = 10;
    if flags & 4 != 0 {
        pos += 2 + usize::from(u16::from_le_bytes([*all.get(10)?, *all.get(11)?]));
    }
    for bit in [8, 16] {
        if flags & bit != 0 {
            pos += all.get(pos..)?.iter().position(|&b| b == 0)? + 1;
        }
    }
    if flags & 2 != 0 {
        pos += 2;
    }
    let tail = all.len().checked_sub(8)?;
    let size = u32::from_le_bytes(all[tail + 4..].try_into().ok()?) as u64;
    if size > MAX_SVG_BYTES {
        return None;
    }
    let text = inflate(all.get(pos..tail)?, size as usize, false).ok()?;
    (crc32(&text).to_le_bytes() == all[tail..tail + 4]).then_some(text)
}

// usvg's root size: width and height in user units, a percentage of the viewBox, and 100 for any side left open.
pub(crate) fn svg(text: &[u8]) -> Option<(u32, u32, u8)> {
    // A probe cut inside a character keeps the whole characters before it.
    let probe = &text[..text.len().min(PROBE)];
    let text = match std::str::from_utf8(probe) {
        Ok(text) => text,
        Err(e) => std::str::from_utf8(&probe[..e.valid_up_to()]).ok()?,
    };
    let attrs = root(text)?;
    let view = attr(attrs, "viewBox").and_then(|v| {
        let n: Vec<f64> = v
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| (n[2], n[3]))
    });
    let side = |name: &str, view_side: Option<f64>| -> Option<f64> {
        let (number, unit) = length(attr(attrs, name).unwrap_or("100%"))?;
        Some(match unit {
            "%" => view_side.unwrap_or(100.0) * number / 100.0,
            "" | "px" => number,
            "in" => number * 96.0,
            "cm" => number * 96.0 / 2.54,
            "mm" => number * 96.0 / 25.4,
            "pt" => number * 4.0 / 3.0,
            "pc" => number * 16.0,
            "em" => number * 12.0,
            "ex" => number * 6.0,
            _ => return None,
        })
    };
    let w = side("width", view.map(|v| v.0))?;
    let h = side("height", view.map(|v| v.1))?;
    let px =
        |v: f64| (v.is_finite() && v >= 0.5 && v < f64::from(u32::MAX)).then(|| v.round() as u32);
    Some((px(w)?, px(h)?, 1))
}

// Sample input: `<?xml ...?><!-- c --><!DOCTYPE svg><svg width="9">`; answers the root's attributes, ` width="9"`.
fn root(text: &str) -> Option<&str> {
    let mut rest = text;
    loop {
        rest = &rest[rest.find('<')?..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = &after[after.find("-->")? + 3..];
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = &rest[rest.find('>')? + 1..];
        } else {
            let tag = rest[1..].strip_prefix("svg")?;
            if !tag.starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/') {
                return None;
            }
            return Some(&tag[..tag.find('>')?]);
        }
    }
}

// Sample input: ` xmlns="..." width='24' height="12px"`; answers the named value without its quotes.
fn attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = attrs;
    loop {
        rest = rest.trim_start();
        let eq = rest.find('=')?;
        let key = rest[..eq].trim();
        let value = rest[eq + 1..].trim_start();
        let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let end = value[1..].find(quote)?;
        if key == name {
            return Some(value[1..1 + end].trim());
        }
        rest = &value[end + 2..];
    }
}

// Sample input: "12.5px", "1e2", "2em"; an exponent counts only when a digit follows it, so "em" stays the unit.
fn length(value: &str) -> Option<(f64, &str)> {
    let b = value.as_bytes();
    let digits = |mut i: usize| {
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        i
    };
    let mut end = digits(usize::from(matches!(b.first(), Some(b'+' | b'-'))));
    if b.get(end) == Some(&b'.') {
        end = digits(end + 1);
    }
    if matches!(b.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(b.get(end + 1), Some(b'+' | b'-')));
        if b.get(end + 1 + sign).is_some_and(u8::is_ascii_digit) {
            end = digits(end + 1 + sign);
        }
    }
    let (number, unit) = value.split_at(end);
    Some((number.parse().ok()?, unit.trim()))
}

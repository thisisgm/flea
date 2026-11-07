//! Resolve terminal text once, after logical fitting and before writing cells.
use super::width;
use icu_properties::{props::BidiMirroringGlyph, CodePointMapData};
use std::sync::Arc;
use unicode_bidi::{bidi_class, BidiClass, BidiInfo, Level};

pub fn needed(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(bidi_class(c), BidiClass::R | BidiClass::AL))
}

fn control(c: char) -> bool {
    !c.is_control() && !super::safe(c)
}

// Isolate only the text, leaving its fixed column padding outside the bidi run.
pub fn run(text: &str) -> String {
    if !needed(text) {
        return text.into();
    }
    let Some((open, close)) = super::markers(super::Run::Name) else {
        return text.into();
    };
    let head = text.len() - text.trim_start().len();
    let tail = text.trim_end().len();
    format!(
        "{}{open}{}{close}{}",
        &text[..head],
        percentages(&text[head..tail]),
        &text[tail..]
    )
}

// Sample input: "الصفحة 1 من 1 · 100%"; the numeric percentage stays left to right.
fn percentages(text: &str) -> String {
    let Some((open, close)) = super::markers(super::Run::Number) else {
        return text.into();
    };
    let mut out = String::new();
    let (mut copied, mut number) = (0, None);
    for (at, c) in text.char_indices() {
        if c.is_numeric() {
            number.get_or_insert(at);
        } else if matches!(c, '.' | ',' | '\u{66b}' | '\u{66c}') && number.is_some() {
            continue;
        } else {
            if matches!(c, '%' | '\u{66a}') {
                if let Some(first) = number {
                    let end = at + c.len_utf8();
                    out.push_str(&text[copied..first]);
                    out.push(open);
                    out.push_str(&text[first..end]);
                    out.push(close);
                    copied = end;
                }
            }
            number = None;
        }
    }
    out.push_str(&text[copied..]);
    out
}

// Restore numeric isolation after untrusted controls were stripped by the caller.
pub(super) fn isolate_contents(run: super::Run, text: &str) -> String {
    match run {
        super::Run::Name => percentages(text),
        super::Run::Number => text.into(),
    }
}

fn order(text: &str) -> Vec<(usize, char, bool)> {
    let bidi = BidiInfo::new(text, Some(Level::ltr()));
    let mut ordered = Vec::new();
    for paragraph in &bidi.paragraphs {
        let levels = bidi.reordered_levels(paragraph, paragraph.range.clone());
        let mut clusters: Vec<Vec<(usize, char)>> = Vec::new();
        for (offset, c) in text[paragraph.range.clone()].char_indices() {
            let offset = paragraph.range.start + offset;
            match clusters.last_mut() {
                Some(cluster) if width(c) == 0 && !control(c) && !control(cluster[0].1) => {
                    cluster.push((offset, c));
                }
                _ => clusters.push(vec![(offset, c)]),
            }
        }
        let cluster_levels: Vec<_> = clusters
            .iter()
            .map(|cluster| levels[cluster[0].0])
            .collect();
        for index in BidiInfo::reorder_visual(&cluster_levels) {
            let rtl = cluster_levels[index].is_rtl();
            ordered.extend(
                clusters[index]
                    .iter()
                    .filter(|(_, c)| !control(*c))
                    .map(|&(offset, c)| (offset, c, rtl)),
            );
        }
    }
    ordered
}

// The caret stays attached to the logical boundary when the field paints in visual order.
pub fn caret(text: &str, boundary: usize) -> usize {
    if !needed(text) {
        return super::text_width(&text[..boundary]);
    }
    let (isolated, boundary) = match super::markers(super::Run::Name) {
        Some((open, close)) => (format!("{open}{text}{close}"), boundary + open.len_utf8()),
        None => (text.to_owned(), boundary),
    };
    let mut cells = 0;
    let mut end = 0;
    let mut base = 0;
    for (offset, c, rtl) in order(&isolated) {
        let length = width(c);
        if length > 0 {
            base = cells;
        }
        if offset == boundary {
            return if rtl { base + length } else { cells };
        }
        if offset + c.len_utf8() == boundary {
            end = if rtl { base } else { cells + length };
        }
        cells += length;
    }
    end
}

fn sgr(style: &mut Arc<str>, escape: &str) {
    if matches!(escape, "\x1b[0m" | "\x1b[m") {
        *style = Arc::from("");
    } else {
        *style = Arc::from(format!("{style}{escape}"));
    }
}

fn segment(raw: &str, style: &mut Arc<str>, output: &mut String) {
    if !needed(raw) && !raw.chars().any(control) {
        output.push_str(raw);
        let mut rest = raw;
        while let Some(at) = rest.find('\x1b') {
            rest = &rest[at..];
            let end = escape_end(rest);
            sgr(style, &rest[..end]);
            rest = &rest[end..];
        }
        return;
    }
    let mut plain = String::new();
    let mut styles = Vec::new();
    let mut rest = raw;
    while let Some(c) = rest.chars().next() {
        if c == '\x1b' {
            let end = escape_end(rest);
            sgr(style, &rest[..end]);
            rest = &rest[end..];
        } else {
            styles.push((plain.len(), Arc::clone(style)));
            plain.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    let mut active: Option<&str> = None;
    for (offset, c, rtl) in order(&plain) {
        // Every offset order() yields starts a character pushed above, so the search always finds its style.
        let ink = styles
            .binary_search_by_key(&offset, |(at, _)| *at)
            .map_or(&**style, |index| styles[index].1.as_ref());
        if active != Some(ink) {
            output.push_str("\x1b[0m");
            output.push_str(ink);
            active = Some(ink);
        }
        let shown = if rtl {
            CodePointMapData::<BidiMirroringGlyph>::new()
                .get(c)
                .mirroring_glyph
                .unwrap_or(c)
        } else {
            c
        };
        output.push(shown);
    }
    output.push_str("\x1b[0m");
    output.push_str(style);
}

// Sample input: "\x1b[31m" or "\x1b_Ga=q;AAAA\x1b\\".
fn escape_end(text: &str) -> usize {
    let bytes = text.as_bytes();
    if bytes.get(1) == Some(&b'[') {
        return bytes
            .iter()
            .enumerate()
            .skip(2)
            .find(|(_, b)| (0x40..=0x7e).contains(*b))
            .map_or(text.len(), |(at, _)| at + 1);
    }
    if bytes
        .get(1)
        .is_some_and(|b| matches!(b, b']' | b'P' | b'_' | b'^'))
    {
        return text
            .find("\x1b\\")
            .map(|at| at + 2)
            .or_else(|| text.find('\x07').map(|at| at + 1))
            .unwrap_or(text.len());
    }
    text.char_indices().nth(2).map_or(text.len(), |(at, _)| at)
}

// Cursor commands and graphics remain opaque; SGR styles travel with their source characters.
pub fn frame(text: &str) -> String {
    if !needed(text) && !text.chars().any(control) {
        return text.into();
    }
    let mut output = String::with_capacity(text.len());
    let mut style: Arc<str> = Arc::from("");
    let (mut start, mut at) = (0, 0);
    while let Some(c) = text[at..].chars().next() {
        let end = if c == '\x1b' {
            at + escape_end(&text[at..])
        } else {
            at + c.len_utf8()
        };
        let boundary = if c == '\x1b' {
            !text[at..end].starts_with("\x1b[") || !text[at..end].ends_with('m')
        } else {
            c.is_control()
        };
        if boundary {
            segment(&text[start..at], &mut style, &mut output);
            output.push_str(&text[at..end]);
            start = end;
        }
        at = end;
    }
    segment(&text[start..], &mut style, &mut output);
    output
}

#[cfg(test)]
#[path = "arabic_tests.rs"]
mod tests;

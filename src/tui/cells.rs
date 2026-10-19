use super::{icons, model::Row, theme::{Depth, Theme}};

extern "C" { fn wcwidth(c: i32) -> i32; }

pub fn safe(c: char) -> bool {
    !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{061c}')
}
pub fn clean(text: &str) -> String { text.chars().filter(|c| safe(*c)).collect() }
pub fn width(c: char) -> usize {
    let cells = unsafe { wcwidth(c as i32) };
    if cells >= 0 { return cells as usize; }
    match c as u32 {
        0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe00..=0xfe0f | 0xfe20..=0xfe2f => 0,
        0x1100..=0x115f | 0x2329..=0x232a | 0x2e80..=0xa4cf | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xfe10..=0xfe19 | 0xfe30..=0xfe6f | 0xff01..=0xff60 | 0xffe0..=0xffe6 | 0x20000..=0x3fffd => 2,
        // The rest of glibc 2.41's C.UTF-8 wide set, which a C locale answers -1 for.
        0x231a..=0x231b | 0x23e9..=0x23ec | 0x23f0 | 0x23f3 | 0x25fd..=0x25fe | 0x2614..=0x2615 | 0x2630..=0x2637
        | 0x2648..=0x2653 | 0x267f | 0x268a..=0x268f | 0x2693 | 0x26a1 | 0x26aa..=0x26ab | 0x26bd..=0x26be | 0x26c4..=0x26c5
        | 0x26ce | 0x26d4 | 0x26ea | 0x26f2..=0x26f3 | 0x26f5 | 0x26fa | 0x26fd | 0x2705 | 0x270a..=0x270b | 0x2728 | 0x274c
        | 0x274e | 0x2753..=0x2755 | 0x2757 | 0x2795..=0x2797 | 0x27b0 | 0x27bf | 0x2b1b..=0x2b1c | 0x2b50 | 0x2b55
        | 0xa960..=0xa97c | 0x16fe0..=0x16fe3 | 0x16ff0..=0x16ff1 | 0x17000..=0x187f7 | 0x18800..=0x18cd5 | 0x18cff..=0x18d08
        | 0x1aff0..=0x1aff3 | 0x1aff5..=0x1affb | 0x1affd..=0x1affe | 0x1b000..=0x1b122 | 0x1b132 | 0x1b150..=0x1b152 | 0x1b155
        | 0x1b164..=0x1b167 | 0x1b170..=0x1b2fb | 0x1d300..=0x1d356 | 0x1d360..=0x1d376 | 0x1f004 | 0x1f0cf | 0x1f18e
        | 0x1f191..=0x1f19a | 0x1f200..=0x1f202 | 0x1f210..=0x1f23b | 0x1f240..=0x1f248 | 0x1f250..=0x1f251 | 0x1f260..=0x1f265
        | 0x1f300..=0x1f320 | 0x1f32d..=0x1f335 | 0x1f337..=0x1f37c | 0x1f37e..=0x1f393 | 0x1f3a0..=0x1f3ca | 0x1f3cf..=0x1f3d3
        | 0x1f3e0..=0x1f3f0 | 0x1f3f4 | 0x1f3f8..=0x1f43e | 0x1f440 | 0x1f442..=0x1f4fc | 0x1f4ff..=0x1f53d | 0x1f54b..=0x1f54e
        | 0x1f550..=0x1f567 | 0x1f57a | 0x1f595..=0x1f596 | 0x1f5a4 | 0x1f5fb..=0x1f64f | 0x1f680..=0x1f6c5 | 0x1f6cc
        | 0x1f6d0..=0x1f6d2 | 0x1f6d5..=0x1f6d7 | 0x1f6dc..=0x1f6df | 0x1f6eb..=0x1f6ec | 0x1f6f4..=0x1f6fc | 0x1f7e0..=0x1f7eb
        | 0x1f7f0 | 0x1f90c..=0x1f93a | 0x1f93c..=0x1f945 | 0x1f947..=0x1f9ff | 0x1fa70..=0x1fa7c | 0x1fa80..=0x1fa89
        | 0x1fa8f..=0x1fac6 | 0x1face..=0x1fadc | 0x1fadf..=0x1fae9 | 0x1faf0..=0x1faf8 => 2,
        _ => 1,
    }
}
pub fn text_width(text: &str) -> usize { text.chars().filter(|c| safe(*c)).map(width).sum() }
pub fn fit(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars().filter(|c| safe(*c)) {
        let n = width(c);
        if used + n > limit { break; }
        if n == 0 && used == 0 { continue; }
        out.push(c);
        used += n;
    }
    out.push_str(&" ".repeat(limit - used));
    out
}
pub fn ellipsis(text: &str, limit: usize) -> String {
    if text_width(text) <= limit { return fit(text, limit); }
    if limit == 0 { return String::new(); }
    let mut prefix = fit(text, limit - 1);
    prefix.push('…');
    prefix
}
pub fn centered(text: &str, limit: usize) -> String {
    fit(&format!("{}{}", " ".repeat(limit.saturating_sub(text_width(text)) / 2), text), limit)
}
// Bold on the name's cells only (mark, space and padding plain, a cut name's ellipsis bold); the title is exactly columns cells.
pub fn name_title(mark: &str, name: &str, columns: usize) -> String {
    if columns == 0 { return String::new(); }
    let cut = text_width(mark) + text_width(name) > columns;
    let limit = if cut { columns - 1 } else { columns };
    let mut shown = Vec::new();
    let mut used = 0;
    for (is_name, c) in mark.chars().map(|c| (false, c)).chain(name.chars().map(|c| (true, c))) {
        if !safe(c) { continue; }
        let n = width(c);
        if used + n > limit { break; }
        if n == 0 && used == 0 { continue; }
        shown.push((is_name, c));
        used += n;
    }
    let name_shown = shown.iter().any(|&(is_name, c)| is_name && width(c) > 0);
    shown.resize(shown.len() + limit - used, (false, ' '));
    if cut { shown.push((name_shown, '…')); }
    let mut out = String::new();
    let mut bold = false;
    for (is_name, c) in shown {
        if is_name != bold {
            out.push_str(if is_name { "\x1b[1m" } else { "\x1b[22m" });
            bold = is_name;
        }
        out.push(c);
    }
    if bold { out.push_str("\x1b[22m"); }
    out
}
pub struct Wrapped<'a> { remaining: Option<&'a str>, columns: usize }
impl<'a> Wrapped<'a> {
    pub fn new(text: &'a str, columns: usize) -> Self { Self { remaining: (columns > 0).then_some(text), columns } }
}
impl<'a> Iterator for Wrapped<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        let text = self.remaining.take()?;
        let mut used = 0;
        for (offset, c) in text.char_indices() {
            let cells = if safe(c) { width(c) } else { 0 };
            if used + cells > self.columns && offset > 0 {
                self.remaining = Some(&text[offset..]);
                return Some(&text[..offset]);
            }
            used += cells;
        }
        Some(text)
    }
}
pub fn words(text: &str, columns: usize) -> Vec<String> {
    if columns == 0 { return Vec::new(); }
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && text_width(&line) + 1 + text_width(word) > columns {
            lines.push(std::mem::take(&mut line));
        }
        let mut parts = Wrapped::new(word, columns).peekable();
        while let Some(part) = parts.next() {
            if !line.is_empty() { line.push(' '); }
            line.push_str(part);
            if parts.peek().is_some() || text_width(&line) >= columns { lines.push(std::mem::take(&mut line)); }
        }
    }
    if !line.is_empty() { lines.push(line); }
    lines
}

#[derive(Clone, Copy, Default)]
pub struct RowStyle {
    pub cursor: bool,
    pub focused: bool,
    pub marked: bool,
    pub dimmed: bool,
    pub quiet: bool,
    pub parent: bool,
}
pub fn row(entry: &Row, columns: usize, size: Option<&str>, marks: bool, style: RowStyle, theme: &Theme, query: &str) -> String {
    let base = theme.base();
    let lifted = style.cursor || style.marked;
    let fill = if style.cursor { if style.focused { &theme.cursor } else { &theme.selected } }
        else if style.marked { &theme.marked } else { &theme.background };
    let ink = if style.dimmed { &theme.muted } else if lifted { &theme.foreground }
        else if style.parent { &theme.muted } else if style.quiet { &theme.meta } else if !entry.link.is_empty() { &theme.symlink }
        else if !entry.directory && entry.mode & 0o111 != 0 { &theme.executable } else { &theme.foreground };
    let meta = if style.dimmed { &theme.muted } else if lifted { &theme.foreground } else { &theme.meta };
    let reset = format!("\x1b[0m{fill}{ink}{}", if style.cursor && theme.depth == Depth::Sixteen { "\x1b[7m" } else { "" });
    let mut out = reset.clone();
    if columns == 0 { return out; }
    if style.cursor && style.focused && theme.depth != Depth::Sixteen {
        out.push_str(&theme.accent);
        out.push('▎');
        out.push_str(ink);
    } else { out.push(' '); }
    if columns < 4 {
        out.push_str(&ellipsis(&entry.name, columns - 1));
        out.push_str(&base);
        return out;
    }
    out.push_str(meta);
    out.push_str(icons::row(entry, marks));
    out.push(' ');
    out.push_str(ink);
    let size = size.filter(|s| text_width(s) + 6 <= columns);
    let name_width = columns - 3 - size.map_or(1, |s| text_width(s) + 3);
    // A search result's name is its path under the search; it prints its name, then its folder in the metadata ink.
    let (folder, leaf) = entry.name.rsplit_once('/').unwrap_or(("", &entry.name));
    let display_name = fit(leaf, text_width(leaf));
    let name = ellipsis(&display_name, text_width(&display_name).min(name_width));
    let target_width = name_width - text_width(&name);
    let target = if !folder.is_empty() { ellipsis(&format!(" {folder}"), target_width) } else if entry.link.is_empty() { fit("", target_width) }
        else { ellipsis(&format!(" → {}", entry.link), target_width) };
    if let Some((start, end)) = match_range(&name, query) {
        out.push_str(&name[..start]);
        out.push_str("\x1b[1m");
        out.push_str(&theme.match_fill);
        out.push_str(&name[start..end]);
        out.push_str(&reset);
        out.push_str(&name[end..]);
    } else { out.push_str(&name); }
    if !target.is_empty() { out.push_str(meta); out.push_str(&target); }
    if let Some(size) = size {
        out.push(' ');
        out.push_str(meta);
        out.push_str(size);
        out.push_str("  ");
    } else { out.push(' '); }
    out.push_str(&base);
    out
}
pub(super) fn match_range(text: &str, query: &str) -> Option<(usize, usize)> {
    if query.is_empty() { return None; }
    let query = query.to_lowercase();
    let start = text.to_lowercase().find(&query)?;
    let end = start + query.len();
    let (mut offset, mut first) = (0, None);
    for (index, c) in text.char_indices() {
        let length: usize = c.to_lowercase().map(char::len_utf8).sum();
        if offset <= start && start < offset + length { first = Some(index); }
        if offset < end && end <= offset + length { return first.map(|first| (first, index + c.len_utf8())); }
        offset += length;
    }
    None
}

#[cfg(test)]
#[path = "cells_tests.rs"]
mod tests;

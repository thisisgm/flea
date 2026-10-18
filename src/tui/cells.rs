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
        for part in Wrapped::new(word, columns) {
            if !line.is_empty() { line.push(' '); }
            line.push_str(part);
            if text_width(&line) >= columns { lines.push(std::mem::take(&mut line)); }
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
    let display_name = fit(&entry.name, text_width(&entry.name));
    let name = ellipsis(&display_name, text_width(&display_name).min(name_width));
    let target_width = name_width - text_width(&name);
    let target = if entry.link.is_empty() { fit("", target_width) }
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

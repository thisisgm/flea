use super::{icons, model::Row, theme::Theme};

pub use crate::cellwidth::width;
#[cfg(test)]
use crate::cellwidth::wcwidth;
#[path = "visual.rs"]
pub mod visual;

pub fn mirrors(role: &str) -> bool {
    static POLICY: std::sync::OnceLock<Option<crate::jsondoc::Json>> = std::sync::OnceLock::new();
    POLICY
        .get_or_init(|| crate::jsondoc::parse(include_str!("../../i18n/rtl-policy.json")).ok())
        .as_ref()
        .and_then(|doc| doc.get("roles"))
        .and_then(|roles| roles.get(role))
        .and_then(|role| role.get("action"))
        .and_then(crate::jsondoc::Json::as_str)
        == Some("mirror")
}
pub fn safe(c: char) -> bool {
    !c.is_control()
        && !matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{206f}' | '\u{200e}' | '\u{200f}' | '\u{061c}')
}
// Each untrusted run starts on its own base cell, never on the preceding column's blank or mark.
pub(super) fn characters(text: &str) -> impl Iterator<Item = (char, usize)> + '_ {
    let mut base = false;
    text.chars().filter_map(move |c| {
        if !safe(c) {
            return None;
        }
        let cells = width(c);
        base |= cells > 0;
        base.then_some((c, cells))
    })
}
pub fn clean(text: &str) -> String {
    characters(text).map(|(c, _)| c).collect()
}
// Untrusted runs are isolated with the markers i18n/rtl-policy.json names, never a TUI copy; TUI-12 wires the views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    Name,
    Number,
}
#[derive(Debug, PartialEq)]
struct Runs {
    name: (char, char),
    number: (char, char),
}
fn runs_from(policy: &str) -> Option<Runs> {
    let doc = crate::jsondoc::parse(policy).ok()?;
    let runs = doc.get("text_runs")?;
    if runs.get("reverse_bytes")?.as_bool()?
        || !runs.get("strip_raw_display_controls")?.as_bool()?
    {
        return None;
    }
    let control = |name: &str| match name {
        "FSI" => Some('\u{2068}'),
        "LRI" => Some('\u{2066}'),
        "RLI" => Some('\u{2067}'),
        _ => None,
    };
    let pair = |key: &str| {
        let (open, close) = runs.get(key)?.as_str()?.split_once("+unchanged-text+")?;
        (close == "PDI").then_some(())?;
        Some((control(open)?, '\u{2069}'))
    };
    Some(Runs {
        name: pair("names_and_paths")?,
        number: pair("numbers_dates_keycaps")?,
    })
}
pub(super) fn markers(run: Run) -> Option<(char, char)> {
    static RUNS: std::sync::OnceLock<Option<Runs>> = std::sync::OnceLock::new();
    let runs = RUNS
        .get_or_init(|| runs_from(include_str!("../../i18n/rtl-policy.json")))
        .as_ref()?;
    Some(match run {
        Run::Name => runs.name,
        Run::Number => runs.number,
    })
}
// Call last on an already fitted run: the isolates take no cells, and a refused policy leaves the run clean only.
pub fn isolate(run: Run, text: &str) -> String {
    let text = clean(text);
    let Some((open, close)) = markers(run) else {
        return text;
    };
    let head = text.len() - text.trim_start().len();
    let tail = text.trim_end().len();
    if head >= tail {
        return text;
    }
    format!(
        "{}{open}{}{close}{}",
        &text[..head],
        visual::isolate_contents(run, &text[head..tail]),
        &text[tail..]
    )
}
pub fn text_width(text: &str) -> usize {
    characters(text).map(|(_, cells)| cells).sum()
}
pub fn fit(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for (c, n) in characters(text) {
        if used + n > limit {
            break;
        }
        out.push(c);
        used += n;
    }
    out = visual::run(&out);
    out.push_str(&" ".repeat(limit - used));
    out
}
pub fn ellipsis(text: &str, limit: usize) -> String {
    if text_width(text) <= limit {
        return fit(text, limit);
    }
    if limit == 0 {
        return String::new();
    }
    let mut prefix = fit(text, limit - 1);
    prefix.push('…');
    prefix
}
pub fn centered(text: &str, limit: usize) -> String {
    let text = clean(text);
    fit(
        &format!(
            "{}{}",
            " ".repeat(limit.saturating_sub(text_width(&text)) / 2),
            text
        ),
        limit,
    )
}
// Bold on the name's cells only (mark, space and padding plain, a cut name's ellipsis bold); the title is exactly columns cells.
pub fn name_title(mark: &str, name: &str, columns: usize) -> String {
    if columns == 0 {
        return String::new();
    }
    let cut = text_width(mark) + text_width(name) > columns;
    let limit = if cut { columns - 1 } else { columns };
    let mut shown = Vec::new();
    let mut used = 0;
    for (is_name, c, n) in characters(mark)
        .map(|(c, n)| (false, c, n))
        .chain(characters(name).map(|(c, n)| (true, c, n)))
    {
        if used + n > limit {
            break;
        }
        shown.push((is_name, c));
        used += n;
    }
    let name_shown = shown.iter().any(|&(is_name, c)| is_name && width(c) > 0);
    shown.resize(shown.len() + limit - used, (false, ' '));
    if cut {
        shown.push((name_shown, '…'));
    }
    let mut out = String::new();
    let mut bold = false;
    let rtl = visual::needed(name);
    for (is_name, c) in shown {
        if is_name != bold {
            if bold && rtl {
                out.push('\u{2069}');
            }
            out.push_str(if is_name { "\x1b[1m" } else { "\x1b[22m" });
            if is_name && rtl {
                out.push('\u{2068}');
            }
            bold = is_name;
        }
        out.push(c);
    }
    if bold {
        if rtl {
            out.push('\u{2069}');
        }
        out.push_str("\x1b[22m");
    }
    out
}
pub struct Wrapped<'a> {
    remaining: Option<&'a str>,
    columns: usize,
}
impl<'a> Wrapped<'a> {
    pub fn new(text: &'a str, columns: usize) -> Self {
        Self {
            remaining: (columns > 0).then_some(text),
            columns,
        }
    }
}
impl<'a> Iterator for Wrapped<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        // A borrowed part cannot replace an oversized glyph; omit it together with its following marks.
        let text = self
            .remaining
            .take()?
            .trim_start_matches(|c| !safe(c) || width(c) == 0 || width(c) > self.columns);
        let mut used = 0;
        for (offset, c) in text.char_indices() {
            let cells = if safe(c) { width(c) } else { 0 };
            if used + cells > self.columns {
                self.remaining = Some(&text[offset..]);
                return Some(&text[..offset]);
            }
            used += cells;
        }
        Some(text)
    }
}
pub fn words(text: &str, columns: usize) -> Vec<String> {
    if columns == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && text_width(&line) + 1 + text_width(word) > columns {
            lines.push(std::mem::take(&mut line));
        }
        let mut parts = Wrapped::new(word, columns).peekable();
        while let Some(part) = parts.next() {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&clean(part));
            if parts.peek().is_some() || text_width(&line) >= columns {
                lines.push(std::mem::take(&mut line));
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
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
    pub rtl: bool,
}
pub fn row(
    entry: &Row,
    columns: usize,
    size: Option<&str>,
    marks: bool,
    style: RowStyle,
    theme: &Theme,
    query: &str,
) -> String {
    let base = theme.base();
    let lifted = style.cursor || style.marked;
    let fill = if style.cursor {
        if style.focused {
            &theme.cursor
        } else {
            &theme.selected
        }
    } else if style.marked {
        &theme.marked
    } else {
        &theme.background
    };
    let ink = if style.dimmed {
        &theme.muted
    } else if lifted {
        &theme.foreground
    } else if style.parent {
        &theme.muted
    } else if style.quiet {
        &theme.meta
    } else if !entry.link.is_empty() {
        &theme.symlink
    } else if !entry.directory && entry.mode & 0o111 != 0 {
        &theme.executable
    } else {
        &theme.foreground
    };
    let meta = if style.dimmed {
        &theme.muted
    } else if lifted {
        &theme.foreground
    } else {
        &theme.meta
    };
    let reset = format!(
        "\x1b[0m{fill}{ink}{}",
        theme.depth.row_attribute(style.cursor, style.marked)
    );
    let mut out = reset.clone();
    if columns == 0 {
        return out;
    }
    let edge = if style.cursor && style.focused && !theme.depth.reverses() {
        format!("{}{}{}", theme.accent, "▎", ink)
    } else {
        " ".into()
    };
    if columns < 4 {
        let name = ellipsis(&entry.name, columns - 1);
        out.push_str(&edge);
        out.push_str(&name);
        out.push_str(&base);
        return out;
    }
    let mark = format!("{meta}{} {ink}", icons::row(entry, marks));
    let size = size.filter(|s| !s.is_empty() && text_width(s) + 6 <= columns);
    let name_width = columns - 3 - size.map_or(1, |s| text_width(s) + 3);
    // A search result's name is its path under the search; it prints its name, then its folder in the metadata ink.
    let shown = entry.shown();
    let (folder, leaf) = shown.rsplit_once('/').unwrap_or(("", &shown));
    let display_name = fit(leaf, text_width(leaf));
    let name = ellipsis(&display_name, text_width(&display_name).min(name_width));
    let target_width = name_width - text_width(&name);
    let target = if !folder.is_empty() {
        ellipsis(&format!(" {}", clean(folder)), target_width)
    } else if entry.link.is_empty() {
        fit("", target_width)
    } else {
        ellipsis(&format!(" → {}", clean(&entry.link)), target_width)
    };
    let mut label = String::new();
    if let Some((start, end)) = match_range(&name, query) {
        label.push_str(&name[..start]);
        label.push_str("\x1b[1m");
        label.push_str(&name[start..end]);
        label.push_str(&reset);
        label.push_str(&name[end..]);
    } else {
        label.push_str(&name);
    }
    out.push_str(&edge);
    out.push_str(&mark);
    out.push_str(&label);
    if !target.is_empty() {
        out.push_str(meta);
        out.push_str(&target);
    }
    if let Some(size) = size {
        let size = if style.rtl {
            isolate(Run::Number, size)
        } else {
            size.into()
        };
        out.push_str(&format!(" {meta}{size}  "));
    } else {
        out.push(' ');
    }
    out.push_str(&base);
    out
}
pub(super) fn match_range(text: &str, query: &str) -> Option<(usize, usize)> {
    if query.is_empty() {
        return None;
    }
    let query = query.to_lowercase();
    let start = text.to_lowercase().find(&query)?;
    let end = start + query.len();
    let (mut offset, mut first) = (0, None);
    for (index, c) in text.char_indices() {
        let length: usize = c.to_lowercase().map(char::len_utf8).sum();
        if offset <= start && start < offset + length {
            first = Some(index);
        }
        if offset < end && end <= offset + length {
            return first.map(|first| (first, index + c.len_utf8()));
        }
        offset += length;
    }
    None
}

#[cfg(test)]
#[path = "cells_tests.rs"]
mod tests;

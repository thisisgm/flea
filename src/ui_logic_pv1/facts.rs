// The facts each preview state states, label by label in the Preview column board's order, and the multi summary.
use super::kinds::{self, State, UNKNOWN_KIND};
use super::meta::{Entry, Meta};
use super::photo::{self, is_symlink, permissions};
use super::FACT_SEP;
use crate::i18n::I18n;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
    pub mode: u32,
    pub icon: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

// Values a caller already has, which win over what the meta answer would give.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extra {
    pub duration: String,
    pub rate: String,
    pub pages: String,
    pub entries: String,
    pub unpacked: String,
    pub owner: String,
}

// A multi-selection, an error and a preview still arriving outrank what the row is.
pub fn state(row: Option<&Row>, selection_count: usize, loading: bool, error: &str, kind_name: &str) -> State {
    if selection_count > 1 {
        return State::Multi;
    }
    let Some(row) = row else {
        return if loading { State::Loading } else { State::Unsupported };
    };
    if !error.is_empty() {
        return State::Error;
    }
    if loading {
        return State::Loading;
    }
    if is_symlink(row.mode) {
        return State::Symlink;
    }
    if kind_name == UNKNOWN_KIND {
        return State::Unsupported;
    }
    if let Some(by_extension) = kinds::state_by_extension(&row.name) {
        return by_extension;
    }
    if kinds::is_markdown(&row.name) {
        return State::Text;
    }
    match kinds::kind_state(&row.icon) {
        State::Text if kinds::is_code(&row.name) => State::Code,
        other => other,
    }
}

pub fn is_previewable(row: Option<&Row>) -> bool { row.is_some_and(|r| !r.is_dir) }

fn count(i18n: &I18n, n: u64) -> String { i18n.locale().format_count(n as i64) }
fn floor(i18n: &I18n, text: &str) -> String { i18n.tr_args("preview.floor", &[text]) }
pub fn size(i18n: &I18n, bytes: u64) -> String { i18n.locale().format_size(bytes) }
pub fn date(i18n: &I18n, mtime: i64) -> String { i18n.locale().format_date(mtime).unwrap_or_default() }

// A count the backend stopped early is a floor; a file it could not open states none.
pub fn line_count(meta: Option<&Meta>, i18n: &I18n) -> String {
    match meta {
        Some(m) if !m.lines_failed => match m.lines {
            Some(lines) if m.partial => floor(i18n, &count(i18n, lines)),
            Some(lines) => count(i18n, lines),
            None => String::new(),
        },
        _ => String::new(),
    }
}

// A frame header's size wins; a camera RAW has none, so its sensor size stands in.
pub fn pixels(meta: Option<&Meta>, i18n: &I18n) -> String {
    let Some(m) = meta else { return String::new() };
    let (w, h) = if m.w > 0 && m.h > 0 { (m.w, m.h) } else { m.photo.as_ref().map_or((0, 0), |p| (p.w, p.h)) };
    if w > 0 && h > 0 {
        i18n.tr_args("preview.pixels", &[&w.to_string(), &h.to_string()])
    } else {
        String::new()
    }
}

pub fn frames(meta: Option<&Meta>) -> String { meta.filter(|m| m.frames > 1).map_or(String::new(), |m| m.frames.to_string()) }

// Duration and rate off the meta answer; nothing probed yet is empty rather than zero.
pub fn media_extra(meta: Option<&Meta>, i18n: &I18n) -> (String, String) {
    let Some(m) = meta else { return Default::default() };
    let duration = if m.duration_ms > 0 { photo::duration(m.duration_ms) } else { String::new() };
    (duration, photo::sample_rate(m.sample_rate, i18n))
}

// Entries and the unpacked total off the archive's own index; zero means a failed or absent read.
pub fn archive_extra(meta: Option<&Meta>, i18n: &I18n) -> (String, String) {
    match meta.and_then(|m| m.entries.filter(|e| *e > 0).map(|e| (m, e))) {
        Some((m, entries)) => (count(i18n, entries), if m.unpacked > 0 { size(i18n, m.unpacked) } else { String::new() }),
        None => Default::default(),
    }
}

// "214 entries · 3.4 GB unpacked", Quick Look's one line under an archive's name.
pub fn archive_line(meta: Option<&Meta>, i18n: &I18n) -> String {
    let Some(m) = meta else { return String::new() };
    match m.entries {
        Some(0) if !m.archive_failed => i18n.trn("preview.entries", 0),
        Some(entries) if entries > 0 => {
            let head = i18n.trn("preview.entries", entries as i64);
            if m.unpacked > 0 {
                format!("{head}{FACT_SEP}{}", i18n.tr_args("preview.unpacked_size", &[&size(i18n, m.unpacked)]))
            } else {
                head
            }
        }
        _ => String::new(),
    }
}

// The names the Archive tile lists, bounded by the wire and by what the frame has room for.
pub fn archive_entries(meta: Option<&Meta>, shown: Option<usize>) -> &[Entry] {
    let Some(m) = meta else { return &[] };
    match shown {
        Some(n) => &m.names[..n.min(m.names.len())],
        None => &m.names,
    }
}

// The "+ N more" under the listed names: every entry the frame did not name.
pub fn archive_more(meta: Option<&Meta>, shown: Option<usize>) -> u64 {
    match meta.and_then(|m| m.entries.filter(|e| *e > 0)) {
        Some(entries) => entries.saturating_sub(archive_entries(meta, shown).len() as u64),
        None => 0,
    }
}

// TallPreviews: text, code, an archive list and a PDF fill the column down to the name.
pub fn tall(state: State) -> bool { matches!(state, State::Text | State::Code | State::Archive | State::Pdf) }

pub fn tall_height(column: f64, margin: f64, name: f64, facts: f64, gap: f64) -> f64 {
    (column - 2.0 * margin - name - facts - 2.0 * gap).max(0.0)
}

// "132 lines", "214 entries" or "12 pages"; nothing while the count is not known.
pub fn compact_count(state: State, meta: Option<&Meta>, extra: &Extra, i18n: &I18n) -> String {
    match state {
        State::Pdf => match extra.pages.parse::<i64>() {
            Ok(pages) if pages > 0 => i18n.trn("preview.pages", pages),
            _ => String::new(),
        },
        State::Archive => match meta.filter(|m| !m.archive_failed).and_then(|m| m.entries) {
            Some(entries) => i18n.trn("preview.entries", entries as i64),
            None => String::new(),
        },
        _ => match meta.filter(|m| !m.lines_failed).and_then(|m| m.lines.map(|l| (m, l))) {
            Some((m, lines)) if m.partial => floor(i18n, &i18n.trn("preview.lines", lines as i64)),
            Some((_, lines)) => i18n.trn("preview.lines", lines as i64),
            None => String::new(),
        },
    }
}

// The tall kinds' one facts line: the Kind apart, so it alone elides, then the size and the count.
pub fn compact_line(state: State, row: Option<&Row>, meta: Option<&Meta>, kind_name: &str, extra: &Extra, i18n: &I18n) -> (String, String) {
    let Some(row) = row else { return Default::default() };
    let mut parts = vec![size(i18n, row.size)];
    let counted = compact_count(state, meta, extra, i18n);
    if !counted.is_empty() {
        parts.push(counted);
    }
    let tail = parts.join(FACT_SEP);
    if kind_name.is_empty() {
        (String::new(), tail)
    } else {
        (kind_name.into(), format!("{FACT_SEP}{tail}"))
    }
}

fn pair(i18n: &I18n, key: &str, value: String) -> Fact { Fact { label: i18n.tr(key), value } }

// A label with no value is not drawn; the row goes rather than printing a label with nothing after it.
fn filled(rows: Vec<Fact>) -> Vec<Fact> { rows.into_iter().filter(|f| !f.value.is_empty()).collect() }

pub fn facts(state: State, row: &Row, meta: Option<&Meta>, kind_name: &str, extra: &Extra, i18n: &I18n) -> Vec<Fact> {
    filled(state_rows(state, row, meta, kind_name, extra, i18n))
}

fn or(first: &str, second: String) -> String { if first.is_empty() { second } else { first.into() } }

fn state_rows(state: State, row: &Row, meta: Option<&Meta>, kind_name: &str, extra: &Extra, i18n: &I18n) -> Vec<Fact> {
    let kind = || pair(i18n, "preview.fact.kind", kind_name.into());
    let row_size = || pair(i18n, "preview.fact.size", size(i18n, row.size));
    let mode = || pair(i18n, "preview.fact.mode", permissions(row.mode));
    let mut rows = vec![kind(), row_size(), pair(i18n, "preview.fact.modified", date(i18n, row.mtime))];
    match state {
        State::Image => {
            let shot = photo::photo_facts(meta.and_then(|m| m.photo.as_ref()), i18n);
            rows.extend([
                pair(i18n, "preview.fact.camera", shot.camera), pair(i18n, "preview.fact.lens", shot.lens),
                pair(i18n, "preview.fact.exposure", shot.exposure), pair(i18n, "preview.fact.focal", shot.focal),
                pair(i18n, "preview.fact.pixels", pixels(meta, i18n)), pair(i18n, "preview.fact.frames", frames(meta)),
                pair(i18n, "preview.fact.taken", shot.taken),
            ]);
        }
        State::Video | State::Audio => {
            let (duration, rate) = media_extra(meta, i18n);
            rows.push(pair(i18n, "preview.fact.duration", or(&extra.duration, duration)));
            rows.push(if state == State::Video {
                pair(i18n, "preview.fact.pixels", pixels(meta, i18n))
            } else {
                pair(i18n, "preview.fact.rate", or(&extra.rate, rate))
            });
        }
        State::Pdf => rows.push(pair(i18n, "preview.fact.pages", extra.pages.clone())),
        State::Text => rows.push(pair(i18n, "preview.fact.lines", line_count(meta, i18n))),
        State::Code => rows.extend([pair(i18n, "preview.fact.lines", line_count(meta, i18n)), mode()]),
        State::Archive => {
            let (entries, unpacked) = archive_extra(meta, i18n);
            let (entries, unpacked) = (or(&extra.entries, entries), or(&extra.unpacked, unpacked));
            let value = match (entries.is_empty(), unpacked.is_empty()) {
                (true, _) => String::new(),
                (false, true) => entries,
                (false, false) => i18n.tr_args("preview.archive.out", &[&entries, &unpacked]),
            };
            rows.push(pair(i18n, "preview.fact.entries", value));
        }
        State::Symlink => {
            let points = if meta.is_some_and(|m| m.target_dir) { "preview.points.folder" } else { "preview.points.file" };
            return vec![
                pair(i18n, "preview.fact.kind", i18n.tr("preview.symlink.kind")),
                pair(i18n, "preview.fact.target", meta.map_or(String::new(), |m| m.target.clone())),
                pair(i18n, "preview.fact.points_at", i18n.tr(points)), mode(),
            ];
        }
        State::Loading => return vec![kind(), row_size(), pair(i18n, "preview.fact.state", i18n.tr("preview.state.loading"))],
        State::Error => return vec![kind(), row_size(), mode(), pair(i18n, "preview.fact.owner", extra.owner.clone())],
        _ => rows.push(mode()),
    }
    rows
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Word {
    Image,
    Video,
    Audio,
    Code,
    Text,
    Archive,
    File,
}

impl Word {
    pub fn of(icon: &str) -> Self {
        match kinds::kind_state(icon) {
            State::Image => Word::Image,
            State::Video => Word::Video,
            State::Audio => Word::Audio,
            State::Code => Word::Code,
            State::Text => Word::Text,
            State::Archive => Word::Archive,
            _ => Word::File,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Word::Image => "preview.kinds.image",
            Word::Video => "preview.kinds.video",
            Word::Audio => "preview.kinds.audio",
            Word::Code => "preview.kinds.code",
            Word::Text => "preview.kinds.text",
            Word::Archive => "preview.kinds.archive",
            Word::File => "preview.kinds.file",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub word: Word,
    // The icon of the group's first member, whose mark the frame's stack draws.
    pub icon: String,
    pub count: u64,
}

// The distinct kinds in a selection, in the order met; the stack and the Kinds row read this one pass.
pub fn kind_groups(rows: &[Option<Row>]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for row in rows.iter().flatten() {
        let word = Word::of(&row.icon);
        match groups.iter_mut().find(|g| g.word == word) {
            Some(group) => group.count += 1,
            None => groups.push(Group { word, icon: row.icon.clone(), count: 1 }),
        }
    }
    groups
}

// The canvas stacks three marks over a multi-selection, one per kind.
pub const MULTI_MARK_CAP: usize = 3;

pub fn multi_marks(rows: &[Option<Row>]) -> Vec<String> {
    kind_groups(rows).into_iter().take(MULTI_MARK_CAP).map(|g| g.icon).collect()
}

// "2 images, 1 video, 1 text"; a floor marks every part.
pub fn kind_summary(groups: &[Group], floored: bool, i18n: &I18n) -> String {
    let parts: Vec<String> = groups.iter().map(|g| {
        let text = i18n.trn(g.word.key(), g.count as i64);
        if floored { floor(i18n, &text) } else { text }
    }).collect();
    parts.join(&i18n.tr("preview.list_separator"))
}

// A summary, never a collage; past the held window every number is a floor and says so.
pub fn multi_facts(rows: &[Option<Row>], selection_count: Option<usize>, i18n: &I18n) -> Vec<Fact> {
    let present: Vec<&Row> = rows.iter().flatten().collect();
    let bytes: u64 = present.iter().map(|r| r.size).sum();
    let newest = present.iter().map(|r| r.mtime).filter(|m| *m != 0).max().unwrap_or(0);
    let oldest = present.iter().map(|r| r.mtime).filter(|m| *m != 0).min().unwrap_or(0);
    let floored = selection_count.is_some_and(|c| c > present.len());
    let mark = |text: String| if floored && !text.is_empty() { floor(i18n, &text) } else { text };
    filled(vec![
        pair(i18n, "preview.fact.kinds", kind_summary(&kind_groups(rows), floored, i18n)),
        pair(i18n, "preview.fact.combined", mark(size(i18n, bytes))),
        pair(i18n, "preview.fact.newest", if newest > 0 { mark(date(i18n, newest)) } else { String::new() }),
        pair(i18n, "preview.fact.oldest", if oldest > 0 { mark(date(i18n, oldest)) } else { String::new() }),
    ])
}

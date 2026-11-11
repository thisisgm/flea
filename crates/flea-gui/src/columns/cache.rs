//! A track's bounded LRU of prepared rows: a peek row or a listing row is shaped once while it stays near the viewport.
use super::peeks::PeekRow;
use crate::navigation::sort::Size;
use crate::row::{self, marks::Mark};
use flea::protocol::RowFacts;
use gpui::{Font, GlyphRun, Hsla, Pixels, TextRun, Window};
use std::{borrow::Cow, cell::OnceCell, collections::HashMap, rc::Rc};

/// One row either track kind draws: a peek answers name, folder flag and icon, a listing row its whole facts.
#[derive(Clone, Copy, Debug)]
pub enum Row<'a> {
    Peek(&'a PeekRow),
    Listing(&'a RowFacts),
}

impl Row<'_> {
    pub fn name(&self) -> &str {
        match self {
            Self::Peek(row) => &row.name,
            Self::Listing(row) => &row.name,
        }
    }

    pub fn dir(&self) -> bool {
        match self {
            Self::Peek(row) => row.dir,
            Self::Listing(row) => row.dir,
        }
    }

    fn mark(&self, languages: bool) -> Mark {
        match self {
            Self::Peek(row) => Mark::for_file(&row.name, row.dir, 0, &row.icon, languages),
            Self::Listing(row) => Mark::for_facts(row, languages),
        }
    }

    fn matches(&self, facts: &Facts) -> bool {
        match (self, facts) {
            (Self::Peek(row), Facts::Peek(held)) => *row == held,
            (Self::Listing(row), Facts::Listing(held)) => *row == held,
            _ => false,
        }
    }

    fn owned(&self) -> Facts {
        match self {
            Self::Peek(row) => Facts::Peek((*row).clone()),
            Self::Listing(row) => Facts::Listing((*row).clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Facts {
    Peek(PeekRow),
    Listing(RowFacts),
}

/// One row's shaped cells; colour is chosen at paint, so a cursor or lifted change reshapes nothing.
pub struct Prepared {
    pub plain: GlyphRun,
    /// A folder's name at the chevron budget, None when it fits both.
    pub chevron: Option<GlyphRun>,
    pub size: Option<GlyphRun>,
    pub size_label: String,
    pub location: Option<GlyphRun>,
    pub label: String,
    pub location_label: String,
    pub mark: Mark,
    pub dir: bool,
    matches: [Option<MatchedName>; 2],
}

struct MatchedName {
    label: String,
    run: (usize, usize),
    bold: OnceCell<GlyphRun>,
}

impl Prepared {
    pub fn name(&self, chevron: bool) -> &GlyphRun {
        self.chevron
            .as_ref()
            .filter(|_| chevron)
            .unwrap_or(&self.plain)
    }

    fn matched(&self, chevron: bool) -> Option<&MatchedName> {
        self.matches[usize::from(chevron && self.chevron.is_some())].as_ref()
    }

    /// The cursor's glyphs are prepared before paint; other rows retain their plain name.
    pub fn name_for(&self, chevron: bool, cursor: bool) -> &GlyphRun {
        self.matched(chevron)
            .filter(|_| cursor)
            .and_then(|name| name.bold.get())
            .unwrap_or_else(|| self.name(chevron))
    }

    pub fn prepare_cursor(&self, font: &Font, body: Pixels, window: &Window) {
        for name in self.matches.iter().flatten() {
            name.bold.get_or_init(|| {
                let runs =
                    crate::g2::file_search::run::text_runs(&name.label, Some(name.run), true, font);
                GlyphRun::new(
                    window
                        .text_system()
                        .layout_line(&name.label, body, &runs, None),
                )
            });
        }
    }

    pub fn wash_span(&self, chevron: bool, cursor: bool) -> Option<(Pixels, Pixels)> {
        crate::g2::file_search::run::wash_span(
            self.name(chevron).line(),
            Some(self.matched(chevron)?.run),
            cursor,
        )
    }
}

/// What every cached row was shaped for; any change drops them all.
#[derive(Clone, Debug, PartialEq)]
pub struct Shaping {
    pub font: Font,
    pub body: Pixels,
    pub caption: Pixels,
    pub plain_cells: usize,
    pub chevron_cells: usize,
    /// Cells a clip mark takes from either budget: ceil((gap + mark) / advance).
    pub clip_cells: usize,
    pub size: bool,
    /// ViewState.langMarks: a code file wears its language's logo.
    pub languages: bool,
    pub scale: f32,
    pub words: Rc<row::format::Words>,
    pub locating: bool,
    pub query: Rc<str>,
}

struct Entry {
    cells: (usize, usize),
    facts: Facts,
    clipped: bool,
    walk: Option<Size>,
    prepared: Rc<Prepared>,
    used: u64,
}

#[derive(Default)]
pub struct TrackCache {
    shaping: Option<Shaping>,
    entries: HashMap<usize, Entry>,
    clock: u64,
    capacity: usize,
    pub hits: u64,
    pub misses: u64,
    pub peak: usize,
}

fn shape(text: &str, font: &Font, size: Pixels, window: &Window) -> GlyphRun {
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    GlyphRun::new(window.text_system().layout_line(text, size, &[run], None))
}

fn shape_name(
    (label, run): (Cow<'_, str>, Option<(usize, usize)>),
    shaping: &Shaping,
    window: &Window,
) -> (GlyphRun, Option<MatchedName>) {
    let glyphs = shape(&label, &shaping.font, shaping.body, window);
    let matched = run.map(|run| MatchedName {
        label: label.into_owned(),
        run,
        bold: OnceCell::new(),
    });
    (glyphs, matched)
}

/// ColumnRow.qml sizeText(): the list's own formatter, so every pane prints one text under the same words.
pub fn size_text(facts: &RowFacts, walk: Option<Size>, words: &row::format::Words) -> String {
    row::format::size_text(facts, walk, words)
}

/// Shapes one row once, the name middle-elided to its cells first so no long name is shaped whole.
fn prepare(
    row: Row<'_>,
    clipped: bool,
    walk: Option<Size>,
    shaping: &Shaping,
    window: &Window,
) -> Prepared {
    let take = if clipped { shaping.clip_cells } else { 0 };
    let plain_cells = if clipped {
        shaping.plain_cells.min(shaping.chevron_cells)
    } else {
        shaping.plain_cells
    };
    let shown = row::shown_name(row.name());
    let path = shown.as_ref();
    let name = if shaping.locating {
        flea::ui_logic_file_search::base(path)
    } else {
        path
    };
    let location_label = if shaping.locating {
        let location = crate::g2::file_search::run::location(
            path,
            "",
            "",
            flea::ui_logic_file_search::Scope::Folder,
        );
        // The location leaves the clip slot, its own gap and one cell of name before the mark.
        let cells = (shaping.plain_cells / 2).min(plain_cells.saturating_sub(take + 2));
        if cells == 0 {
            String::new()
        } else {
            row::right_elide(&location, cells).into_owned()
        }
    } else {
        String::new()
    };
    let reserved = if location_label.is_empty() {
        0
    } else {
        location_label.chars().map(row::cells_of).sum::<usize>() + 1
    };
    let budget = |cells: usize| cells.saturating_sub(take).saturating_sub(reserved);
    let run = crate::g2::file_search::run::run_in(name, &shaping.query);
    let elide_at = |cells: usize| match run {
        Some(run) => {
            let (label, (start, end)) = row::elide_around(name, cells, run);
            (label, (start < end).then_some((start, end)))
        }
        None => (row::middle_elide(name, cells), None),
    };
    let width = |text: &str| {
        f32::from(
            shape(text, &shaping.font, shaping.body, window)
                .line()
                .width,
        )
    };
    // One cell is the body face's digit advance, the unit paint.body_advance() counted the budgets in.
    let advance = f32::from(super::paint::digit_advance(
        &shaping.font,
        shaping.body,
        window,
    ));
    let elide = |cells: usize| row::fitted(name, budget(cells), advance, Some(&width), elide_at);
    let label = elide(plain_cells);
    let chevron = row
        .dir()
        .then(|| elide(shaping.chevron_cells))
        .filter(|text| text.0 != label.0);
    let plain_label = label.0.clone().into_owned();
    let (plain, plain_match) = shape_name(label, shaping, window);
    let chevron = chevron.map(|text| shape_name(text, shaping, window));
    let (chevron, chevron_match) =
        chevron.map_or((None, None), |(glyphs, matched)| (Some(glyphs), matched));
    let size_label = match row {
        Row::Listing(facts) if shaping.size => Some(size_text(facts, walk, &shaping.words)),
        _ => None,
    };
    let size = size_label
        .as_ref()
        .map(|label| shape(label, &shaping.font, shaping.caption, window));
    Prepared {
        plain,
        location: (!location_label.is_empty())
            .then(|| shape(&location_label, &shaping.font, shaping.caption, window)),
        label: plain_label,
        location_label,
        chevron,
        size,
        size_label: size_label.unwrap_or_default(),
        mark: row.mark(shaping.languages),
        dir: row.dir(),
        matches: [plain_match, chevron_match],
    }
}

impl TrackCache {
    /// Starts a frame: a new shaping clears the rows, and the bound follows the viewport.
    pub fn begin(&mut self, shaping: &Shaping, capacity: usize) {
        if self.shaping.as_ref() != Some(shaping) {
            self.entries.clear();
            self.shaping = Some(shaping.clone());
        }
        self.capacity = capacity.max(1);
        while self.entries.len() > self.capacity {
            self.evict();
        }
    }

    /// The row shaped from exactly these facts, clip state and folder walk; an equal row is reused as is.
    pub fn get(
        &mut self,
        index: usize,
        row: Row<'_>,
        clipped: bool,
        walk: Option<Size>,
        shaping: &Shaping,
        window: &Window,
    ) -> Rc<Prepared> {
        self.clock += 1;
        let clock = self.clock;
        if let Some(entry) = self.entries.get_mut(&index).filter(|entry| {
            entry.clipped == clipped
                && entry.walk == walk
                && row.matches(&entry.facts)
                && entry.cells == (shaping.plain_cells, shaping.chevron_cells)
        }) {
            entry.used = clock;
            self.hits += 1;
            return Rc::clone(&entry.prepared);
        }
        self.misses += 1;
        let prepared = Rc::new(prepare(row, clipped, walk, shaping, window));
        if !self.entries.contains_key(&index) && self.entries.len() >= self.capacity {
            self.evict();
        }
        self.entries.insert(
            index,
            Entry {
                cells: (shaping.plain_cells, shaping.chevron_cells),
                facts: row.owned(),
                clipped,
                walk,
                prepared: Rc::clone(&prepared),
                used: clock,
            },
        );
        self.peak = self.peak.max(self.entries.len());
        prepared
    }

    /// Drops every row but these, as a hidden view parks.
    pub fn keep(&mut self, live: &[usize]) {
        self.entries.retain(|index, _| live.contains(index));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    fn evict(&mut self) {
        if let Some(oldest) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.used)
            .map(|(index, _)| *index)
        {
            self.entries.remove(&oldest);
        }
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;

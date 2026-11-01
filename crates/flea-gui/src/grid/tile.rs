//! One tile's shaped caption and match boxes, kept in an LRU bounded to the drawn tiles plus the cache rows.
use super::{
    captions::{self, LineMark},
    geometry::Rect,
};
use crate::row::marks::Mark;
use flea::protocol::RowFacts;
use gpui::{Font, GlyphRun, Hsla, Pixels, SharedString, TextRun, Window};
use std::{collections::HashMap, ops::Range, rc::Rc};

/// One drawn caption line: its glyphs, its bytes in the caption, and the width a centred line is placed by.
pub struct Line {
    pub run: GlyphRun,
    pub range: Range<usize>,
    pub centred: f32,
}

/// A tile's caption as GridNames breaks it, shaped once, with its mark; colour is chosen at paint.
pub struct Tile {
    pub(super) name: String,
    icon: String,
    mode: u32,
    per_line: usize,
    count: usize,
    pub caption: String,
    pub lines: Vec<Line>,
    pub mark: Mark,
    pub elided: bool,
    pub widest: f32,
}

impl Tile {
    // QQuickText's integer lineHeightOffset retains the rounded font extents beyond the fixed pitch.
    pub fn text_height(&self, count: usize, pitch: f32) -> f32 {
        let count = count.min(self.lines.len());
        if count == 0 {
            return 0.;
        }
        let natural = self.lines[..count]
            .iter()
            .map(|line| {
                let run = line.run.line();
                f32::from(run.ascent).ceil() + f32::from(run.descent).abs().ceil()
            })
            .fold(0., f32::max);
        count as f32 * pitch + (natural - pitch).max(0.).trunc()
    }
}

/// What every cached tile was shaped for; any change drops them all.
#[derive(Clone, Debug, PartialEq)]
pub struct Shaping {
    pub listing: usize,
    pub font: Font,
    pub size: Pixels,
    pub scale: f32,
}

fn text_run(len: usize, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn shaped(
    text: &str,
    range: Range<usize>,
    runs: &[TextRun],
    size: Pixels,
    window: &Window,
) -> Line {
    let run = GlyphRun::new(window.text_system().layout_line(text, size, runs, None));
    let trimmed = text
        .trim_end_matches(|ch: char| ch.is_whitespace() || ch == '\u{feff}')
        .len();
    let centred = f32::from(run.line().x_for_index(trimmed));
    Line {
        run,
        range,
        centred,
    }
}

/// A tile under this frame's overlay: a path-named row captions its last segment, still keyed by its whole name.
pub fn prepare_for(
    overlay: &super::element::Overlay,
    row: &RowFacts,
    per_line: usize,
    count: usize,
    shaping: &Shaping,
    window: &Window,
) -> Tile {
    if !overlay.leaf {
        return prepare(row, per_line, count, shaping, window);
    }
    let leaf = RowFacts {
        name: row.name.rsplit('/').next().unwrap_or(&row.name).to_owned(),
        ..row.clone()
    };
    Tile {
        name: row.name.clone(),
        ..prepare(&leaf, per_line, count, shaping, window)
    }
}

/// Breaks the name for this many cells and lines and shapes each line; a dead budget hands the whole name over as one line.
pub fn prepare(
    row: &RowFacts,
    per_line: usize,
    count: usize,
    shaping: &Shaping,
    window: &Window,
) -> Tile {
    let budget = if per_line == 0 {
        f64::NAN
    } else {
        per_line as f64
    };
    let caption = captions::grid_caption(&row.name, budget, count as f64).into_owned();
    let mut lines = Vec::with_capacity(count);
    let mut at = 0;
    for text in caption.split('\n') {
        let range = at..at + text.len();
        at = range.end + 1;
        lines.push(shaped(
            text,
            range,
            &[text_run(text.len(), &shaping.font)],
            shaping.size,
            window,
        ));
    }
    let elided = !caption
        .split('\n')
        .flat_map(str::chars)
        .eq(row.name.chars());
    Tile {
        name: row.name.clone(),
        icon: row.icon.clone(),
        mode: row.mode,
        per_line,
        count,
        widest: lines.iter().map(|line| line.centred).fold(0., f32::max),
        caption,
        lines,
        mark: Mark::for_row(&row.icon, row.mode),
        elided,
    }
}

/// The wash box per drawn line the run touches, in label pixels: centred as the line is, then past the letters before the run.
pub fn washes(
    tile: &Tile,
    marks: &[LineMark],
    label_width: f32,
    line_height: f32,
) -> [Option<Rect>; 2] {
    let mut out = [None, None];
    for mark in marks {
        let (Some(line), Some(slot)) = (tile.lines.get(mark.line), out.get_mut(mark.line)) else {
            continue;
        };
        let layout = line.run.line();
        let x = |at: usize| f32::from(layout.x_for_index(at));
        let height = f32::from(layout.ascent + layout.descent).ceil();
        let left = (label_width - x(mark.centred)) / 2. + x(mark.run.start);
        *slot = Some(Rect {
            x: left,
            y: mark.line as f32 * line_height,
            width: x(mark.run.end) - x(mark.run.start),
            height,
        });
    }
    out
}

/// The cursor tile's lines with the typed run set bold instead of washed; a line the run misses stays as shaped.
pub fn bold(
    tile: &Tile,
    marks: &[LineMark],
    shaping: &Shaping,
    window: &Window,
) -> [Option<Line>; 2] {
    let mut out = [None, None];
    let strong = shaping.font.clone().bold();
    for mark in marks {
        let (Some(line), Some(slot)) = (tile.lines.get(mark.line), out.get_mut(mark.line)) else {
            continue;
        };
        let text = &tile.caption[line.range.clone()];
        let runs: Vec<TextRun> = [
            (mark.run.start, &shaping.font),
            (mark.run.len(), &strong),
            (text.len() - mark.run.end, &shaping.font),
        ]
        .into_iter()
        .filter(|(len, _)| *len > 0)
        .map(|(len, font)| text_run(len, font))
        .collect();
        *slot = Some(shaped(
            text,
            line.range.clone(),
            &runs,
            shaping.size,
            window,
        ));
    }
    out
}

struct Entry {
    tile: Rc<Tile>,
    used: u64,
    washes: Option<(SharedString, f32, [Option<Rect>; 2])>,
}

/// Prepared tiles by listing row; a tile is reused only while its name, cells and line count are unchanged.
#[derive(Default)]
pub struct TileCache {
    shaping: Option<Shaping>,
    entries: HashMap<usize, Entry>,
    clock: u64,
    capacity: usize,
    pub hits: u64,
    pub misses: u64,
    pub peak: usize,
}

impl TileCache {
    /// Starts a frame: a new shaping clears the tiles, and the bound follows the drawn tiles plus the cache rows.
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

    /// Tiles held now, never above the bound begin set.
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// The tile shaped for exactly this name and budget, shaping it on a miss.
    pub fn get_or_prepare(
        &mut self,
        listing: usize,
        row: &RowFacts,
        budget: (usize, usize),
        prepare: impl FnOnce() -> Tile,
    ) -> Rc<Tile> {
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(&listing) {
            let tile = &entry.tile;
            if tile.name == row.name
                && tile.icon == row.icon
                && tile.mode == row.mode
                && (tile.per_line, tile.count) == budget
            {
                entry.used = self.clock;
                self.hits += 1;
                return Rc::clone(&entry.tile);
            }
        }
        self.misses += 1;
        if !self.entries.contains_key(&listing) && self.entries.len() >= self.capacity {
            self.evict();
        }
        let tile = Rc::new(prepare());
        self.entries.insert(
            listing,
            Entry {
                tile: Rc::clone(&tile),
                used: self.clock,
                washes: None,
            },
        );
        self.peak = self.peak.max(self.entries.len());
        tile
    }

    /// GridGeometry.heldRow: a row the window left still draws what this index last drew, until a new listing clears the cache.
    pub fn kept(&mut self, listing: usize) -> Option<Rc<Tile>> {
        self.clock += 1;
        let entry = self.entries.get_mut(&listing)?;
        entry.used = self.clock;
        Some(Rc::clone(&entry.tile))
    }

    /// The match boxes of a cached tile for this query and label width, measured once per pair; a resize that keeps the cells still re-centres them.
    pub fn washes(
        &mut self,
        listing: usize,
        query: &SharedString,
        label_width: f32,
        measure: impl FnOnce(&Tile) -> [Option<Rect>; 2],
    ) -> [Option<Rect>; 2] {
        let Some(entry) = self.entries.get_mut(&listing) else {
            return [None, None];
        };
        match &entry.washes {
            Some((seen, width, boxes)) if seen == query && *width == label_width => *boxes,
            _ => {
                let boxes = measure(&entry.tile);
                entry.washes = Some((query.clone(), label_width, boxes));
                boxes
            }
        }
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
mod tests {
    use super::*;
    use std::cell::Cell;

    fn bare(row: &RowFacts) -> Tile {
        Tile {
            name: row.name.clone(),
            icon: row.icon.clone(),
            mode: row.mode,
            per_line: 16,
            count: 2,
            caption: row.name.clone(),
            lines: Vec::new(),
            mark: Mark::File,
            elided: false,
            widest: 0.,
        }
    }

    #[test]
    fn washes_are_measured_once_per_query_and_label_width() {
        let row = RowFacts {
            name: "notes.txt".into(),
            ..RowFacts::default()
        };
        let mut cache = TileCache::default();
        cache.begin(
            &Shaping {
                listing: 1,
                font: gpui::font("JetBrains Mono"),
                size: gpui::px(13.),
                scale: 1.,
            },
            8,
        );
        cache.get_or_prepare(0, &row, (16, 2), || bare(&row));
        let measured = Cell::new(0);
        let mut measure = |width: f32, query: &str| {
            cache.washes(0, &query.into(), width, |_| {
                measured.set(measured.get() + 1);
                [
                    Some(Rect {
                        x: width,
                        y: 0.,
                        width: 1.,
                        height: 1.,
                    }),
                    None,
                ]
            })
        };
        assert_eq!(measure(129., "no")[0].unwrap().x, 129.);
        assert_eq!(measure(129., "no")[0].unwrap().x, 129.);
        assert_eq!(measured.get(), 1, "a still frame measures nothing");
        assert_eq!(
            measure(130., "no")[0].unwrap().x,
            130.,
            "a wider label with the same cells re-centres its washes"
        );
        assert_eq!(measure(130., "not")[0].unwrap().x, 130.);
        assert_eq!(
            measured.get(),
            3,
            "a new width or a new query measures once each"
        );
    }
}

//! The list pane: one custom Element paints the visible rows from prepared lines, with no layout node per row.
#[cfg(test)]
pub(crate) mod bench;
pub mod columns;
pub mod element;
pub mod header;
mod locale;
mod measure;
mod palette;
#[cfg(test)]
mod palette_tests;
pub use measure::Density;
pub mod pane;
#[cfg(test)]
mod probe_tests;
pub mod seam;
#[cfg(test)]
pub(crate) mod size_tests;
pub(crate) mod sizes;
#[cfg(test)]
mod tests;
mod typeface;

pub use element::{ListElement, Shown};
pub use header::HeaderElement;
pub use palette::Palette;
pub use typeface::Typeface;

use crate::{
    navigation::{folder_state::Order, sort::Size},
    palette::Theme,
    row::{cache::RowCache, format::Words, marks::ClipMarks},
};
use columns::{Layout, StoredWidths, Switched, Tokens};
use flea::jsondoc::Json;
use gpui::{point, px, size, Bounds, GlyphRun, LineLayout, Pixels, SharedString, Window};
use std::{cell::Cell, collections::HashMap, rc::Rc, sync::Arc};

// The standalone contents-results Element still uses the reference board's fixed columns.
pub(crate) const PAD_X: f32 = 14.;
pub(crate) const LABEL_X: f32 = PAD_X + 23. + 9.;
pub(crate) const END_W: f32 = 28.;
// Rows kept shaped past the viewport, so a short scroll back reshapes nothing.
const CACHE_SCREENS: usize = 3;
// ui.py's state washes over the foreground: a hover 8%, the header ground 4%, a hairline 12%, a grey cursor 18%, an unfocused mark 6%.
pub const HOVER: f32 = 0.08;
pub const NORMAL: f32 = 0.04;
pub const HAIRLINE: f32 = 0.12;
pub const GREY_CURSOR: f32 = 0.18;
pub const TRACK: f32 = 0.06;
// The cursor rung is accent 18%, a match run accent 14%, and a cut row draws at Omarchy's disabled opacity.
pub const CURSOR_WASH: f32 = 0.18;
pub const MATCH_WASH: f32 = 0.14;
pub const CUT_DIM: f32 = 0.55;
// Omarchy's selection plate is the foreground at 0.35, which the theme tokens ease per palette for 4.5:1 text.
const MARK_FILL: f32 = 0.35;

/// The pane's geometry at the face's body size, from v0.3.12 Theme.qml's formulas, which list.html and list-textsize.html draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub row: f32,
    pub nominal_row: f32,
    pub header: f32,
    pub header_top: f32,
    pub pad: f32,
    pub slot: f32,
    pub mark: f32,
    pub gap: f32,
    pub edge: f32,
    pub lane: f32,
    pub clip: f32,
    pub handle: f32,
    pub hairline: f32,
}

// Theme.qml: bodySmall is Omarchy's 0.917 step, a line box is 1.8 of it, and the chrome is 0.72 of a padded row.
const BODY_SMALL_RATIO: f32 = 0.917;
const LINE_BOX_RATIO: f32 = 1.8;
const CHROME_ROW_RATIO: f32 = 0.72;
// Density's Compact keeps half the control padding, Omarchy's 6 px at its stock base of 12.
const COMPACT_PADDING: f32 = 0.5;
pub(crate) const CONTROL_PADDING_Y: f32 = 6.;
pub(crate) const STOCK_BASE: f32 = 12.;
// Theme.markSize: a mark is sized from the type scale, and Glyph.qml caps it at its slot.
const MARK_RATIO: f32 = 1.45;

impl Metrics {
    pub fn at(body: Pixels) -> Self {
        Self::sized(body, px((f32::from(body) * BODY_SMALL_RATIO).round()))
    }

    pub fn for_typeface(face: &Typeface) -> Self {
        Self::sized(face.base, face.body_small)
    }

    fn sized(body: Pixels, small: Pixels) -> Self {
        let body = f32::from(body);
        let board = |value: f32| (value * body / 14.).round();
        let small = f32::from(small);
        let line = (small * LINE_BOX_RATIO).round();
        let padding = (CONTROL_PADDING_Y * body / STOCK_BASE).round();
        let slot = line;
        let nominal_row = line + 2. * padding;
        Self {
            row: line + 2. * (padding * COMPACT_PADDING).round(),
            nominal_row,
            header: (nominal_row * CHROME_ROW_RATIO).round(),
            header_top: board(4.),
            pad: board(14.),
            slot,
            mark: (small * MARK_RATIO).min(slot),
            gap: board(9.),
            edge: board(2.),
            lane: board(14.),
            // Theme.clipMarkSize and the resize zone are fixed pixels, as are hairlines.
            clip: 12.,
            handle: 9.,
            hairline: 1.,
        }
    }

    pub fn label_x(&self) -> f32 {
        self.pad + self.slot + self.gap
    }
}

/// What the pane's focus, selection and pointer owners say about its rows this frame; the list only reads it.
#[derive(Clone, Debug, Default)]
pub struct RowStates {
    /// The listing these indices belong to; a snapshot from another listing marks no row but the cursor.
    pub listing: usize,
    pub cursor: Option<usize>,
    /// The listing holds the keyboard: the cursor is the accent rung and marks take the plate; else both go grey.
    pub focused: bool,
    /// Marked rows in ascending order; None is no selection, which costs nothing.
    pub selected: Option<Rc<[usize]>>,
    pub hovered: Option<usize>,
    pub renaming: Option<usize>,
    pub drop_target: Option<usize>,
    /// The drag owner's caption for the drop target, the label Drag.js names: move here, copy here or link here.
    pub drop_label: Option<SharedString>,
}

/// One row's states, resolved once in prepaint.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowState {
    pub cursor: bool,
    pub selected: bool,
    pub hovered: bool,
    pub renaming: bool,
    pub drop_target: bool,
}

impl RowState {
    /// Row.qml lifted: the cursor, the pointer, a mark or a drop target draw names, marks and cells in the foreground.
    pub fn lifted(self) -> bool {
        self.cursor || self.selected || self.hovered || self.drop_target
    }
}

impl RowStates {
    /// The pane's cursor alone, focused, as a pane without selection or pointer owners draws it.
    pub fn cursor_only(listing: usize, cursor: usize) -> Self {
        Self {
            listing,
            cursor: Some(cursor),
            focused: true,
            ..Self::default()
        }
    }

    pub fn row(&self, index: usize, listing: usize) -> RowState {
        let cursor = self.cursor == Some(index);
        if self.listing != listing {
            return RowState {
                cursor,
                ..RowState::default()
            };
        }
        RowState {
            cursor,
            selected: self
                .selected
                .as_ref()
                .is_some_and(|marked| marked.binary_search(&index).is_ok()),
            hovered: self.hovered == Some(index),
            renaming: self.renaming == Some(index),
            drop_target: self.drop_target == Some(index),
        }
    }
}

/// What the pane's owners set outside paint: the words, the switched columns and stored widths, and the decorations they own.
pub struct ListView {
    pub words: Rc<Words>,
    pub switched: Switched,
    pub stored: StoredWidths,
    pub lang_marks: bool,
    /// A filter or search's needle, whose run each name marks.
    pub query: String,
    pub searching: bool,
    pub dual: bool,
    pub clip: ClipMarks,
    pub order: Order,
    /// Highlight today's dates: today's local midnight while the setting is on.
    pub today: Option<i64>,
    /// Folder sizes the walk answered, by row of the current listing.
    walked: HashMap<usize, Size>,
    walked_listing: usize,
    /// Bumped by every walked size, so rows resolve their marks once per answer rather than once per frame.
    walked_revision: u64,
    /// Bumped by every change that reshapes text, so the row cache keys on a number rather than on the text itself.
    text: u64,
}

impl Default for ListView {
    fn default() -> Self {
        Self {
            words: Rc::default(),
            switched: Switched::default(),
            stored: StoredWidths::default(),
            lang_marks: true,
            query: String::new(),
            searching: false,
            dual: false,
            clip: ClipMarks::default(),
            order: Order::default(),
            today: None,
            walked: HashMap::new(),
            walked_listing: 0,
            walked_revision: 0,
            text: 0,
        }
    }
}

impl ListView {
    /// ui.json's list keys: "columns", "columnWidths", "langMarks" (absent reads on) and "highlightToday" (absent reads off).
    pub fn apply_settings(&mut self, data: &Json, today_start: i64) {
        self.switched = Switched::from_settings(data);
        self.stored = StoredWidths::from_settings(data);
        let lang_marks = data.get("langMarks").and_then(Json::as_bool) != Some(false);
        if lang_marks != self.lang_marks {
            self.lang_marks = lang_marks;
            self.text += 1;
        }
        self.today = (data.get("highlightToday").and_then(Json::as_bool) == Some(true))
            .then_some(today_start);
    }

    pub fn set_searching(&mut self, searching: bool) {
        if self.searching != searching {
            self.searching = searching;
            self.text += 1;
        }
    }

    pub fn set_query(&mut self, query: &str) {
        if self.query != query {
            self.query.clear();
            self.query.push_str(query);
            self.text += 1;
        }
    }

    pub fn text_revision(&self) -> u64 {
        self.text
    }
}

/// What the last frame drew, read by IPC and tests the way ui/Ipc.qml reads delegate geometry.
#[derive(Clone, Copy, Debug)]
pub struct Drawn {
    pub bounds: Bounds<Pixels>,
    /// The clip every row paints inside: the list's own box, so a partly scrolled row never reaches the header above it.
    pub mask: Bounds<Pixels>,
    pub layout: Layout,
    pub metrics: Metrics,
    pub palette: Option<Palette>,
    pub scroll_y: f32,
    pub listing: usize,
    pub frames: u64,
    /// The drop target row and the box its caption is drawn in, this frame.
    pub drop: Option<(usize, Bounds<Pixels>)>,
    /// Rows drawn as Row.qml's loading bar because the pane does not hold them yet.
    pub loading: usize,
}

impl Default for Drawn {
    fn default() -> Self {
        Self {
            bounds: Bounds::default(),
            mask: Bounds::default(),
            layout: Layout::default(),
            metrics: Metrics::at(px(14.)),
            palette: None,
            scroll_y: 0.,
            listing: 0,
            frames: 0,
            drop: None,
            loading: 0,
        }
    }
}

// The face's advances of "0": the body's turns the name width into cells, the small body's writes the column widths, the caption's the cell budgets.
#[derive(Clone, Copy, Debug)]
struct Advance {
    body: f32,
    small: f32,
    caption: f32,
}

/// Paint state a pane keeps across frames: shaped rows, the face's metrics, the owners' view and this frame's rows.
pub struct ListState {
    pub typeface: Typeface,
    pub density: Density,
    pub rows: RowCache,
    advance: Option<Advance>,
    visible: Vec<Shown>,
    pub(crate) probe: Option<crate::visual::Probe>,
    // The cursor the last prepaint laid out, the one a presented frame drew.
    pub(crate) drawn_cursor: Option<usize>,
    // Whether that frame drew the cursor as the keyboard's accent rung, or as the grey of a pane without it.
    pub(crate) drawn_focused: bool,
    pub view: ListView,
    pub drawn: Drawn,
    /// The bounded snapshot the last frame drew, retained until its replacement is painted.
    pub drawn_rows: Rc<pane::Held>,
    pub header: header::HeaderState,
    // The one empty line every hidden cell shares, so a column that is not drawn shapes nothing.
    blank: Arc<LineLayout>,
    /// Text style reads for the palette, which a theme or style change alone repeats.
    pub palette_reads: u64,
    // The palette and the theme it was read for: a different theme reads it again.
    palette: Option<(Theme, Palette)>,
    // The drag owner's drop caption, shaped once per label, face and size.
    drop_line: Option<element::DropLine>,
    /// Marks and cell lines the last paint drew, so a test can see what paint skipped.
    pub painted_marks: Cell<usize>,
    pub painted_cells: Cell<usize>,
}

impl ListState {
    pub fn new(typeface: Typeface) -> Self {
        Self {
            typeface,
            density: Density::Compact,
            rows: RowCache::default(),
            advance: None,
            visible: Vec::new(),
            probe: None,
            drawn_cursor: None,
            drawn_focused: false,
            view: ListView::default(),
            drawn: Drawn::default(),
            drawn_rows: Rc::default(),
            header: header::HeaderState::default(),
            blank: Arc::new(LineLayout::default()),
            palette_reads: 0,
            palette: None,
            drop_line: None,
            painted_marks: Cell::new(0),
            painted_cells: Cell::new(0),
        }
    }

    /// The row palette, read from the inherited text style once per theme and again only after restyle().
    pub fn palette(&mut self, window: &Window, theme: Theme) -> Palette {
        if let Some((_, palette)) = self.palette.filter(|(read, _)| *read == theme) {
            return palette;
        }
        self.palette_reads += 1;
        let palette = Palette::inherited(&window.text_style(), theme);
        self.palette = Some((theme, palette));
        palette
    }

    /// A theme or text style change: the next frame reads the palette again.
    pub fn restyle(&mut self) {
        self.palette = None;
    }

    /// A list that draws its dates from its first frame, since its words come with it.
    pub fn with_words(typeface: Typeface, words: Words) -> Self {
        let mut state = Self::new(typeface);
        state.view.words = Rc::new(words);
        state
    }

    /// New words from a catalog snapshot: every shaped row and title is stale, so all of them go.
    pub fn set_words(&mut self, words: Words) {
        self.view.words = Rc::new(words);
        self.view.text += 1;
        self.invalidate_text();
        self.header.forget_titles();
    }

    /// A list that is not the view on screen holds no shaped rows, so a hidden List costs nothing.
    pub fn park(&mut self) {
        self.rows = RowCache::default();
        self.visible = Vec::new();
        self.drawn_rows = Rc::default();
        self.header.forget_titles();
    }

    pub fn visible(&self) -> &[Shown] {
        &self.visible
    }
}

pub(crate) fn line_bounds(
    run: &GlyphRun,
    x: Pixels,
    top: Pixels,
    row: f32,
    window: &Window,
) -> Bounds<Pixels> {
    let line = run.line();
    let baseline =
        window.pixel_snap(top + (px(row) - line.ascent - line.descent) / 2. + line.ascent);
    Bounds::new(
        point(x, baseline - line.ascent),
        size(line.width, line.ascent + line.descent),
    )
}

pub(crate) fn mark_bounds(left: Pixels, top: Pixels) -> Bounds<Pixels> {
    Bounds::new(
        point(
            left + px(PAD_X + (23. - 17.) / 2.),
            top + px((31. - 17.) / 2.),
        ),
        size(px(17.), px(17.)),
    )
}

pub(crate) fn mark_bounds_at(left: Pixels, top: Pixels, metrics: &Metrics) -> Bounds<Pixels> {
    Bounds::new(
        point(
            left + px(metrics.pad + (metrics.slot - metrics.mark) / 2.),
            top + px((metrics.row - metrics.mark) / 2.),
        ),
        size(px(metrics.mark), px(metrics.mark)),
    )
}

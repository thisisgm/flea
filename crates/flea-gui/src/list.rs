//! The list pane: one custom Element paints the visible rows from prepared lines, with no layout node per row.
#[cfg(test)]
pub(crate) mod bench;
pub mod columns;
pub mod element;
mod git;
pub mod header;
mod hit;
pub mod hover_wash;
mod loading;
mod locale;
mod measure;
mod metrics;
pub(crate) mod outline;
pub(crate) mod painted_row;
mod palette;
#[cfg(test)]
mod palette_tests;
mod recent;
mod row_text;
mod tags;
mod text_revision;
pub use measure::Density;
pub use metrics::Metrics;
pub(crate) use metrics::{mark_bounds_at, CONTROL_PADDING_Y, MARK_RATIO, STOCK_BASE};
#[cfg(test)]
mod bidi_name_tests;
#[cfg(test)]
mod fixlist_mark_ink_tests;
#[cfg(test)]
mod gh315_filter_tests;
#[cfg(test)]
mod gh315_filter_views_tests;
#[cfg(test)]
mod gh333_tests;
pub mod pane;
pub mod picker;
#[cfg(test)]
mod probe_tests;
pub mod seam;
#[cfg(test)]
pub(crate) mod size_tests;
pub(crate) mod sizes;
#[cfg(test)]
mod tests;
mod typeface;

mod lang;
pub use lang::lang_marks;

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
pub(crate) const MARK_FILL: f32 = 0.35;

/// What the pane's focus, selection and pointer owners say about its rows this frame; the list only reads it.
#[derive(Clone, Debug, Default)]
pub struct RowStates {
    /// The listing these indices belong to; a snapshot from another listing marks no row but the cursor.
    pub listing: usize,
    pub cursor: Option<usize>,
    /// The listing holds the keyboard: the cursor is the accent rung; else it goes grey.
    pub focused: bool,
    /// The dual window's other pane (v0.3.12 paneFocused false), whose marks alone take the grey selected fill.
    pub other_pane: bool,
    /// Marked rows in ascending order; None is no selection, which costs nothing.
    pub selected: Option<Rc<[usize]>>,
    pub hovered: Option<usize>,
    /// Settings > View > Highlight on hover: the pointer's row draws the wash and the lifted ink.
    pub hover_wash: bool,
    pub renaming: Option<usize>,
    /// The rename band's Grow for this side this render: the edited row grows and the rows below move down.
    pub grow: pane::Grow,
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
    /// The pointer's row under Highlight on hover, the only hover that washes or lifts; hovered alone stays the pointer's.
    pub hover_lit: bool,
    pub renaming: bool,
    pub drop_target: bool,
}

impl RowState {
    /// Row.qml lifted: the cursor, the lit pointer row, a mark or a drop target draw names, marks and cells in the foreground.
    pub fn lifted(self) -> bool {
        self.cursor || self.selected || self.hover_lit || self.drop_target
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
            hover_lit: self.hover_wash && self.hovered == Some(index),
            renaming: self.renaming == Some(index),
            drop_target: self.drop_target == Some(index),
        }
    }
}

/// What the pane's owners set outside paint: the words, the switched columns and stored widths, and the decorations they own.
pub struct ListView {
    pub(crate) git: Rc<crate::window::git_status::Paint>,
    pub words: Rc<Words>,
    pub switched: Switched,
    pub stored: StoredWidths,
    pub lang_marks: bool,
    pub tags_shown: bool,
    /// A filter or search's needle, whose run each name marks.
    pub query: String,
    pub searching: bool,
    /// A contents search draws each row as a block, its row and first matching line, so rows take the block height.
    pub contents: bool,
    pub dual: bool,
    pub picker: Option<bool>,
    pub picker_directory: bool,
    /// The picker's Recent history: each row names its leaf, and no title carries a sort arrow (picker-recent-list).
    pub recent: bool,
    pub(crate) place_recent: Option<recent::PlaceRecent>,
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
            git: Rc::default(),
            words: Rc::default(),
            switched: Switched::default(),
            stored: StoredWidths::default(),
            lang_marks: true,
            tags_shown: false,
            query: String::new(),
            searching: false,
            contents: false,
            dual: false,
            picker: None,
            picker_directory: false,
            recent: false,
            place_recent: None,
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
        self.set_lang_marks(lang_marks(data));
        self.today = (data.get("highlightToday").and_then(Json::as_bool) == Some(true))
            .then_some(today_start);
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
    pub mirrored: bool,
    /// The drop target row and the box its caption is drawn in, this frame.
    pub drop: Option<(usize, Bounds<Pixels>)>,
    /// Rows drawn as Row.qml's loading bar because the pane does not hold them yet.
    pub loading: usize,
    /// The refused rename's band this frame: the edited row's grown height and the rows below it moved down.
    pub grow: pane::Grow,
    /// Rows this frame laid out: the filter's narrowed count while one stands, else the listing's total.
    pub count: usize,
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
            mirrored: false,
            drop: None,
            loading: 0,
            grow: pane::Grow::default(),
            count: 0,
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
    /// The listing rows a standing filter left that frame, at their view positions; None with no filter up.
    pub drawn_filter: Option<Rc<[usize]>>,
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
    /// Thumbnails the last paint drew in mark slots.
    pub painted_pictures: Cell<usize>,
    /// Chooser check slots the last paint drew.
    pub painted_checks: Cell<usize>,
    /// The pane's one thumbnail lender, shared by Grid tiles, List rows and the active Columns track.
    pub pictures: Rc<crate::grid::element::pictures::Pictures>,
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
            drawn_filter: None,
            header: header::HeaderState::default(),
            blank: Arc::new(LineLayout::default()),
            palette_reads: 0,
            palette: None,
            drop_line: None,
            painted_marks: Cell::new(0),
            painted_cells: Cell::new(0),
            painted_pictures: Cell::new(0),
            painted_checks: Cell::new(0),
            pictures: Rc::default(),
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
        self.drawn_filter = None;
        self.header.forget_titles();
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

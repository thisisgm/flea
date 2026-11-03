//! The grid tile Element: one Element paints every drawn tile from prepared captions and lent pictures, with no layout node per tile.
mod clipboard;
mod drop;
#[cfg(test)]
mod ink_tests;
mod paint;
#[path = "pictures.rs"]
pub mod pictures;
#[cfg(test)]
#[path = "pictures_tests.rs"]
mod pictures_tests;
#[cfg(test)]
#[path = "row_pictures_tests.rs"]
mod row_pictures_tests;
mod tags;
mod tip;

use super::{
    geometry::Rect,
    layout::Layout,
    tile::{Line, Tile, TileCache},
    FocusFacts, GridView,
};
use crate::list::Palette;
use crate::palette::Theme;
use crate::pane::Held;
use flea::{i18n::I18n, protocol::RowFacts};
use gpui::{
    font, Font, GlyphRun, Hsla, IntoElement, Pixels, RenderImage, SharedString, TextStyle, Window,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// Which picture a sandboxed thumbnail is: an image's own pixels, a raw photo's embedded preview, or a print file's picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureKind {
    Image,
    Raw,
    Model,
}

/// The source facts a thumbnail was made from, so a picture never draws on a row that has since changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub size: u64,
    pub mtime: i64,
}

impl Identity {
    pub fn of(row: &RowFacts) -> Self {
        Self {
            name: row.name.clone(),
            size: row.size,
            mtime: row.mtime,
        }
    }
    fn is(&self, row: &RowFacts) -> bool {
        self.name == row.name && self.size == row.size && self.mtime == row.mtime
    }
}

/// One answer from G2's sandboxed thumbnail worker, already decoded; the grid never decodes.
pub struct Thumbnail {
    pub identity: Identity,
    pub kind: PictureKind,
    pub image: Arc<RenderImage>,
}

impl Thumbnail {
    /// The picture's decoded size in pixels.
    pub fn decoded(&self) -> (u32, u32) {
        let size = self.image.size(0);
        (size.width.0.max(0) as u32, size.height.0.max(0) as u32)
    }
}

/// A clipboard mark its owner hands in for a row: whether it is cut, and the glyph it draws after the caption.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub cut: bool,
    pub key: SharedString,
    pub svg: &'static [u8],
}

/// What other owners lend the grid per row; each answer is a lookup, and the grid keeps none of them past the frame.
pub trait Decor {
    fn thumbnail(&self, _listing: usize) -> Option<Arc<Thumbnail>> {
        None
    }
    fn clip(&self, _listing: usize) -> Option<Clip> {
        None
    }
    /// The clipboard mark for a row the frame holds, by its facts; owners that key by listing keep `clip`.
    fn clip_row(&self, listing: usize, _row: Option<&RowFacts>) -> Option<Clip> {
        self.clip(listing)
    }
    /// A language mark that replaces the kind mark when no picture draws.
    fn language(&self, _row: &RowFacts) -> Option<(SharedString, &'static [u8])> {
        None
    }
    /// The production lender's own state, which the Shell feeds; a test lender has none.
    fn pictures(&self) -> Option<&pictures::Pictures> {
        None
    }
}

/// No pictures, no clipboard and no language marks.
pub struct Plain;
impl Decor for Plain {}

/// The drop a drag would make on a folder tile, DragOps.label's three words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropKind {
    Move,
    Copy,
    Link,
}

/// Per-frame facts other owners set: the query whose run the captions mark, the drop target and the row being renamed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    pub query: SharedString,
    pub drop: Option<(usize, DropKind)>,
    pub renaming: Option<usize>,
    /// Rows named by path (the chooser's Recent) caption with their last segment only.
    pub leaf: bool,
}

/// The roles a tile draws with, derived from the inherited text style until the theme tokens land, as the list does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted_frame: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
}

impl Ink {
    pub fn inherited(style: &TextStyle, theme: Theme) -> Self {
        // The list's roles, as its own tiles draw them: the theme sets the ground, never an inverted literal.
        let base = Palette::inherited(style, theme);
        Self {
            background: base.background,
            foreground: base.foreground,
            muted_frame: base.muted_frame,
            accent: base.accent,
            accent_frame: base.accent_frame,
        }
    }
}

/// One tile as the last frame drew it, in grid pixels; hits and the IPC seam read these, never a fresh layout.
#[derive(Clone)]
pub struct Drawn {
    pub view: usize,
    pub listing: usize,
    pub cell: Rect,
    pub slot: Rect,
    pub caption: Rect,
    pub painted: Rect,
    pub picture: Option<Rect>,
    pub decoded: Option<(u32, u32)>,
    pub kind: Option<PictureKind>,
    pub cursor: bool,
    pub selected: bool,
    pub hovered: bool,
    pub editing: bool,
    pub drop: Option<DropKind>,
    pub lines: usize,
    pub washes: [Option<Rect>; 2],
    tile: Option<Rc<Tile>>,
    image: Option<Arc<RenderImage>>,
    clip: Option<Clip>,
    language: Option<(SharedString, &'static [u8])>,
    bold: bool,
}

impl Drawn {
    pub fn name(&self) -> Option<&str> {
        self.tile.as_ref().map(|tile| tile.name.as_str())
    }
    pub fn caption_text(&self) -> Option<&str> {
        self.tile.as_ref().map(|tile| tile.caption.as_str())
    }
    pub fn text_height(&self, pitch: f32) -> f32 {
        self.tile
            .as_ref()
            .map_or(0., |tile| tile.text_height(self.lines, pitch))
    }
    /// The atlas key of the language mark this tile draws in place of its kind mark, None when it draws a picture or a kind mark.
    pub fn language_key(&self) -> Option<&str> {
        self.language.as_ref().map(|(key, _)| key.as_ref())
    }
    /// GridTile.qml captionElided: the caption differs from the name or is cut, so the hover tip shows the whole name.
    pub fn elided(&self) -> bool {
        self.tile.as_ref().is_some_and(|tile| tile.elided) || self.truncated()
    }
    /// More lines than the tile draws, or a line wider than its label: what Qt would report as cut.
    pub fn truncated(&self) -> bool {
        self.tile.as_ref().is_some_and(|tile| {
            tile.lines.len() > self.lines || tile.widest > self.caption.width + 0.5
        })
    }
}

// The cursor tile's bold lines, kept while its tile and query hold, so a still frame reshapes nothing.
struct Bold {
    listing: usize,
    query: SharedString,
    tile: Rc<Tile>,
    lines: [Option<Line>; 2],
}

/// Paint state the grid keeps across frames: shaped tiles, the drop labels, and what the last frame drew.
pub struct GridState {
    pub font: Font,
    pub(crate) frame_held: Option<Rc<Held>>,
    tiles: TileCache,
    labels: [String; 3],
    shaped_labels: Option<[GlyphRun; 3]>,
    advance: Option<(Pixels, f32)>,
    bold: Option<Bold>,
    drawn: Vec<Drawn>,
    tip: Option<tip::Tip>,
    pub layout: Option<Layout>,
    pub scroll: f32,
    pub frames: u64,
    pub bounds: gpui::Bounds<Pixels>,
    pub ink: Option<Ink>,
    /// The keyboard drives the cursor, so a resting pointer lights no tile until it moves (#322).
    pub quiet: bool,
}

impl GridState {
    pub fn new(font: Font) -> Self {
        Self {
            font,
            frame_held: None,
            tiles: TileCache::default(),
            labels: Default::default(),
            shaped_labels: None,
            advance: None,
            bold: None,
            drawn: Vec::new(),
            tip: None,
            layout: None,
            scroll: 0.,
            frames: 0,
            bounds: gpui::Bounds::default(),
            ink: None,
            quiet: false,
        }
    }
    /// Omarchy's default face until the pane follows the live font-set.
    pub fn omarchy_default() -> Self {
        Self::new(font("JetBrainsMono Nerd Font"))
    }

    /// Takes a new catalog's drop labels; a change drops their shaped lines so the next frame shapes the new words once.
    pub fn set_catalog(&mut self, catalog: &I18n) {
        let labels =
            ["grid.drop.move", "grid.drop.copy", "grid.drop.link"].map(|key| catalog.tr(key));
        if labels != self.labels {
            self.labels = labels;
            self.shaped_labels = None;
        }
    }
    pub fn label(&self, kind: DropKind) -> &str {
        &self.labels[kind as usize]
    }

    pub fn drawn(&self) -> &[Drawn] {
        &self.drawn
    }
    pub(super) fn record_frame(&mut self, bounds: gpui::Bounds<Pixels>, ink: Ink) -> Ink {
        self.bounds = bounds;
        self.ink = Some(ink);
        ink
    }
    pub(crate) fn clear_frame(&mut self) {
        self.drawn.clear();
        self.frame_held = None;
        self.scroll = 0.;
        self.tip = None;
        self.layout = None;
        self.ink = None;
    }
    pub fn drawn_row(&self, listing: usize) -> Option<&Drawn> {
        self.drawn.iter().find(|drawn| drawn.listing == listing)
    }
    /// The name tip the last frame drew: its row, its whole name and its box in grid pixels.
    pub fn tip(&self) -> Option<(usize, &str, Rect)> {
        self.tip
            .as_ref()
            .map(|tip| (tip.listing, tip.name.as_str(), tip.rect))
    }
    pub fn cached_tiles(&self) -> usize {
        self.tiles.size()
    }
    pub fn shaped(&self) -> (u64, u64, usize) {
        (self.tiles.hits, self.tiles.misses, self.tiles.peak)
    }

    /// GridArea.qml tileAt: the view index under a grid point, on a tile's slot or its painted caption, or anywhere on an editing tile.
    pub fn tile_at(&self, x: f32, y: f32) -> Option<usize> {
        let layout = self.layout?;
        let view = layout.index_at(self.scroll, x, y)?;
        let first = self.drawn.first()?.view;
        let drawn = self
            .drawn
            .get(view.checked_sub(first)?)
            .filter(|drawn| drawn.view == view)?;
        (drawn.editing || drawn.slot.contains(x, y) || drawn.painted.contains(x, y)).then_some(view)
    }

    // One advance of "0" at the caption size, Theme.bodySmallAdvance, which turns the label width into cells.
    fn advance(&mut self, size: Pixels, window: &Window) -> f32 {
        match self.advance {
            Some((seen, advance)) if seen == size => advance,
            _ => {
                let text = window.text_system();
                let id = text.resolve_font(&self.font);
                let advance = text
                    .advance(id, size, '0')
                    .map_or(f32::from(size) * 0.6, |advance| f32::from(advance.width));
                self.advance = Some((size, advance));
                advance
            }
        }
    }
}

pub struct GridElement {
    held: Rc<Held>,
    shown: Option<Rc<[usize]>>,
    view: GridView,
    focus: Rc<dyn FocusFacts>,
    decor: Rc<dyn Decor>,
    overlay: Overlay,
    state: Rc<RefCell<GridState>>,
}

impl GridElement {
    fn prepare_hover(
        &self,
        state: &mut GridState,
        layout: Layout,
        scroll: f32,
        bounds: gpui::Bounds<Pixels>,
    ) {
        state.layout = Some(layout);
        state.scroll = scroll;
        if let Some((x, y)) = self.view.pointer.filter(|_| !state.quiet) {
            let hit = state.tile_at(
                x - f32::from(bounds.origin.x),
                y - f32::from(bounds.origin.y),
            );
            for drawn in &mut state.drawn {
                drawn.hovered = hit == Some(drawn.view) && self.focus.hover_shown();
            }
        }
    }

    /// shown is the filter's kept rows, None for every row; the view is the host's GridView for this frame.
    pub fn new(
        held: Rc<Held>,
        shown: Option<Rc<[usize]>>,
        view: GridView,
        focus: Rc<dyn FocusFacts>,
        decor: Rc<dyn Decor>,
        overlay: Overlay,
        state: Rc<RefCell<GridState>>,
    ) -> Self {
        Self {
            held,
            shown,
            view,
            focus,
            decor,
            overlay,
            state,
        }
    }
}

impl IntoElement for GridElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

//! What a track paints with: the palette rungs, the focus snapshot, the clip marks and each slot's retained paint state.
use super::cache::{Prepared, TrackCache};
use super::geometry::Slot;
use super::tracks::TrackView;
use crate::grid::element::Thumbnail;
use crate::list::Typeface;
use crate::navigation::sort::{DirSizes, Size};
use crate::palette::Theme;
use gpui::{Hsla, Pixels, TextStyle, Window};
use std::{cell::RefCell, rc::Rc, sync::Arc};

// The board's rungs over the theme roles: hairline 12%, hover 8%, grey cursor and accent wash 18%, marked plate 35%, a cut row 55%.
pub const HAIR: f32 = 0.12;
pub const HOVER: f32 = 0.08;
pub const SELECTION: f32 = 0.18;
pub const WASH: f32 = 0.18;
pub const MARKED: f32 = 0.35;
pub const CUT: f32 = 0.55;
pub const EDGE: f32 = 2.;
/// Theme.clipMarkSize: the clip mark is 12 px square in its reserved strip.
pub const CLIP: f32 = 12.;
// Rows kept shaped past the viewport, so a short scroll back reshapes nothing.
pub const CACHE_SCREENS: usize = 3;

/// The roles a track draws with, interim from the inherited text style until the theme tokens land.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub metadata: Hsla,
    pub muted_frame: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
}

impl Palette {
    pub fn inherited(style: &TextStyle, theme: Theme) -> Self {
        let base = crate::list::Palette::inherited(style, theme);
        Self {
            background: base.background,
            foreground: base.foreground,
            // ColumnRow.qml: an ancestor's names rest in Theme.color.muted, a search location in metadata.
            muted: base.muted,
            metadata: base.metadata,
            muted_frame: base.muted_frame,
            accent: base.accent,
            accent_frame: base.accent_frame,
        }
    }
}

/// A cut row ghosts only its file mark; the name, size, chevron and clipboard glyph keep their roles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowInk {
    pub mark: Hsla,
    pub name: Hsla,
    pub chevron: Hsla,
    pub clip: Hsla,
}

impl Palette {
    pub fn row_ink(&self, clip: Clip, cursor: bool, lifted: bool, dim: bool) -> RowInk {
        let cut = |ink: Hsla| {
            if clip == Clip::Cut {
                ink.opacity(CUT)
            } else {
                ink
            }
        };
        let frame = if cursor {
            self.foreground
        } else {
            self.muted_frame
        };
        RowInk {
            mark: cut(frame),
            name: if cursor || lifted || !dim {
                self.foreground
            } else {
                self.muted
            },
            chevron: frame,
            clip: self.muted_frame,
        }
    }

    /// The column divider and the rail hairline share one ink.
    pub fn hairline(&self) -> Hsla {
        self.foreground.opacity(HAIR)
    }
    /// The track cursor's 2 px edge, drawn in the accent frame.
    pub fn edge(&self) -> Hsla {
        self.accent_frame
    }
}

/// The focus and selection the active track paints, G1.05's read-only snapshot; peek tracks read only the hover.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Focus {
    pub drop: Option<(Slot, usize, gpui::SharedString)>,
    pub cursor: Option<usize>,
    /// The pane does not hold the keyboard, so the cursor draws Omarchy's grey rung without the edge.
    pub grey: bool,
    /// Marked listing indices, sorted.
    pub selected: Rc<[usize]>,
    pub hover: Option<(Slot, usize)>,
    /// Settings > View > Highlight on hover: the pointer's row washes and brightens; hover stays the pointer's.
    pub hover_wash: bool,
    /// The listing row a rename edits, and the rename band's Grow on the active track.
    pub renaming: Option<usize>,
    pub rename_slot: Option<Slot>,
    pub menu_target: Option<(Slot, usize)>,
    pub grow: crate::list::pane::Grow,
}

impl Focus {
    /// The pointer's row as drawn: lit only while Highlight on hover is ticked.
    pub(super) fn hover_lit(&self, slot: Slot, index: usize) -> bool {
        self.hover_wash && self.hover == Some((slot, index))
    }

    pub(super) fn dropping(&self, slot: Slot, index: usize) -> bool {
        self.drop
            .as_ref()
            .is_some_and(|(target_slot, row, _)| *target_slot == slot && *row == index)
    }
    /// The plate under one row: the grey or accent cursor first, then a marked row, then a lifted or hovered one.
    pub fn plate(&self, palette: &Palette, slot: Slot, index: usize, lifted: bool) -> Option<Hsla> {
        let active = slot == Slot::Active;
        let cursor = active && self.cursor == Some(index);
        match () {
            _ if self
                .drop
                .as_ref()
                .is_some_and(|(target_slot, row, _)| *target_slot == slot && *row == index) =>
            {
                Some(palette.accent.opacity(WASH))
            }
            _ if cursor && self.grey => Some(palette.foreground.opacity(SELECTION)),
            _ if cursor => Some(palette.accent.opacity(WASH)),
            _ if active && self.selected.binary_search(&index).is_ok() => {
                Some(palette.foreground.opacity(MARKED))
            }
            _ if lifted || self.hover_lit(slot, index) => Some(palette.foreground.opacity(HOVER)),
            _ => None,
        }
    }

    /// The 2 px accent edge marks a drop target or the focused pane's cursor.
    pub fn edge(&self, slot: Slot, index: usize) -> bool {
        self.drop
            .as_ref()
            .is_some_and(|(target_slot, row, _)| *target_slot == slot && *row == index)
            || (slot == Slot::Active && self.cursor == Some(index) && !self.grey)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clip {
    None,
    Copy,
    Cut,
}

/// The clipboard's marks per row, the clip producer's; an empty clipboard is asked once per frame, never per row.
pub trait ClipMarks {
    fn is_empty(&self) -> bool;
    fn mark(&self, dir: &str, name: &str) -> Clip;
}

/// The pane's answered folder walks by listing index, G1.03's DirSizes; only the active track reads it.
pub trait FolderSizes {
    fn size_for(&self, index: usize) -> Option<Size>;
}

impl FolderSizes for DirSizes {
    fn size_for(&self, index: usize) -> Option<Size> {
        DirSizes::size_for(self, index)
    }
}

/// Paint state one track keeps across frames: its shaped rows, the face's advance and this frame's rows.
pub struct TrackPaint {
    pub(crate) language_paint: RefCell<Vec<crate::row::language_capture::Painted>>,
    pub(super) drop_line: Option<crate::list::painted_row::DropLine>,
    pub drawn: Option<Drawn>,
    pub paint_serial: u64,
    pub typeface: Typeface,
    pub cache: TrackCache,
    advance: Option<Pixels>,
    pub(super) visible: Vec<(usize, Option<Rc<Prepared>>, Clip)>,
    // The thumbnail each visible row draws in its mark slot, in the order of visible.
    pub(super) pictures: Vec<Option<Arc<Thumbnail>>>,
    /// Thumbnails the last paint drew in mark slots.
    pub painted_pictures: std::cell::Cell<usize>,
    // The view position of visible[0]; each later entry is the next position.
    pub(super) first: usize,
    /// The palette the last frame drew with, for a test that checks the installed theme reached the track.
    pub painted: Option<Palette>,
}

impl TrackPaint {
    /// The name a drawn row was shaped to, cut or whole, for tests that read the painted label.
    #[cfg(test)]
    pub fn drawn_label(&self, index: usize) -> Option<String> {
        self.visible
            .iter()
            .find(|(at, _, _)| *at == index)
            .and_then(|(_, prepared, _)| prepared.as_ref())
            .map(|prepared| prepared.label.clone())
    }
    pub fn new(typeface: Typeface) -> Self {
        Self {
            drawn: None,
            language_paint: Default::default(),
            paint_serial: 0,
            typeface,
            cache: TrackCache::default(),
            advance: None,
            visible: Vec::new(),
            pictures: Vec::new(),
            painted_pictures: Default::default(),
            first: 0,
            painted: None,
            drop_line: None,
        }
    }

    /// The indices this frame paints, for a test and the IPC rects.
    /// The mark slot the last frame placed a visible row's thumbnail in, as window pixels (x, y, side), the side square.
    pub fn slot_box(&self, index: usize) -> Option<(f32, f32, f32)> {
        let drawn = self.drawn.as_ref()?;
        let at = self.visible.iter().position(|(row, _, _)| *row == index)?;
        let top = f32::from(drawn.bounds.origin.y) + (self.first + at) as f32 * drawn.row_height
            - drawn.scroll.max(0.);
        let (x, y, side) = super::geometry::picture_box(drawn.band);
        Some((f32::from(drawn.bounds.origin.x) + x, top + y, side))
    }

    pub fn visible(&self) -> impl Iterator<Item = usize> + '_ {
        self.visible
            .iter()
            .filter(|row| row.1.is_some())
            .map(|row| row.0)
    }

    pub fn prepared(&self, index: usize) -> Option<&Rc<Prepared>> {
        self.visible
            .iter()
            .find(|row| row.0 == index)
            .and_then(|row| row.1.as_ref())
    }

    /// The thumbnail the last frame drew for a row, for a test and the native probe.
    pub fn picture(&self, index: usize) -> Option<&Arc<Thumbnail>> {
        let at = self.visible.iter().position(|row| row.0 == index)?;
        self.pictures.get(at)?.as_ref()
    }

    pub fn clip(&self, index: usize) -> Clip {
        self.visible
            .iter()
            .find(|row| row.0 == index)
            .map_or(Clip::None, |row| row.2)
    }

    /// Keeps only the rows the last frame drew.
    pub fn park(&mut self) {
        let live: Vec<usize> = self.visible().collect();
        self.cache.keep(&live);
        self.drawn = None;
        self.visible.clear();
        self.pictures.clear();
        self.language_paint.borrow_mut().clear();
    }

    /// One advance of "0" at the body size, Theme.bodyAdvance, which turns a name width into cells.
    pub fn body_advance(&mut self, window: &Window) -> Pixels {
        if let Some(advance) = self.advance {
            return advance;
        }
        let advance = digit_advance(&self.typeface.font, self.typeface.body, window);
        self.advance = Some(advance);
        advance
    }
}

/// One advance of "0" in `font` at `size`, six tenths of the size when the face has no digit.
pub fn digit_advance(font: &gpui::Font, size: Pixels, window: &Window) -> Pixels {
    let text = window.text_system();
    text.advance(text.resolve_font(font), size, '0')
        .map_or(size * 0.6, |advance| advance.width)
}

/// The last frame's geometry and snapshot, shared by hit testing and IPC.
pub struct Drawn {
    pub bounds: gpui::Bounds<Pixels>,
    /// The focus snapshot consumed by this frame's row painter.
    pub focus: Rc<Focus>,
    /// The tracks the frame drew and this one's place among them: a frame copies no track.
    pub tracks: Rc<[TrackView]>,
    pub at: usize,
    pub scroll: f32,
    pub held: Option<Rc<crate::list::pane::Held>>,
    /// A standing filter's drawn rows on the active track, None while every row is drawn.
    pub shown: Option<Rc<[usize]>>,
    /// The refused rename's band this track drew with; only the active track ever grows.
    pub grow: crate::list::pane::Grow,
    pub row_height: f32,
    /// The band a row centres its marks and lines in: the row, or a contents block's result row.
    pub band: f32,
    /// The scroll lane this frame kept clear at the track's right edge.
    pub lane: f32,
}

impl Drawn {
    pub fn view(&self) -> &TrackView {
        &self.tracks[self.at]
    }

    /// The listing row drawn at a view position: the position itself, or its place in the filter's rows.
    pub fn row_at(&self, position: usize) -> Option<usize> {
        match &self.shown {
            Some(rows) => rows.get(position).copied(),
            None => Some(position),
        }
    }

    /// The view position a listing row is drawn at, None when the filter hides it.
    pub fn position_of(&self, row: usize) -> Option<usize> {
        match &self.shown {
            Some(rows) => rows.iter().position(|&at| at == row),
            None => Some(row),
        }
    }
}

/// One paint state per slot, built when its track is first drawn and dropped when the count no longer draws it.
#[derive(Default)]
pub struct Painters {
    slots: [Option<Rc<RefCell<TrackPaint>>>; 5],
    pub constructed: usize,
    pub dropped: usize,
    /// The painted state a resting pointer's hover already asked one catch-up frame for, while the paint lagged.
    pub rest_retried: Option<(usize, u32)>,
}

impl Painters {
    pub fn painter(&mut self, slot: Slot, typeface: &Typeface) -> Rc<RefCell<TrackPaint>> {
        let held = &mut self.slots[slot.index()];
        // A new text size rebuilds the track, so its rows, mark slot and lane follow the face (Theme.qml sizeRatio).
        if held
            .as_ref()
            .is_some_and(|paint| paint.borrow().typeface != *typeface)
        {
            *held = None;
        }
        if held.is_none() {
            self.constructed += 1;
        }
        Rc::clone(
            held.get_or_insert_with(|| Rc::new(RefCell::new(TrackPaint::new(typeface.clone())))),
        )
    }

    /// Drop every slot the drawn tracks no longer name.
    pub fn retain(&mut self, tracks: &[TrackView]) {
        for (at, held) in self.slots.iter_mut().enumerate() {
            if held.is_some() && !tracks.iter().any(|view| view.slot.index() == at) {
                *held = None;
                self.dropped += 1;
            }
        }
    }

    /// A hidden view keeps each track's live rows, so a kept return reshapes nothing, and drops the rest.
    pub fn park(&mut self) {
        for paint in self.slots.iter().flatten() {
            paint.borrow_mut().park();
        }
    }

    pub fn built(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    pub fn get(&self, slot: Slot) -> Option<&Rc<RefCell<TrackPaint>>> {
        self.slots[slot.index()].as_ref()
    }
}

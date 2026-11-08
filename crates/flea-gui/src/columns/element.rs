//! One custom Element per drawn track: fixed rows painted as quads, marks and prepared glyph runs, no layout node per row.
use super::cache::{Row, Shaping};
use super::geometry::{
    cells_in, mark_box, name_budget_in, picture_box, Slot, CHEVRON, DIVIDER, GAP, PAD_X,
};
use super::glyphs::{paint_line, paint_svg, CHEVRON_LEFT_SVG, CHEVRON_SVG, COPY_SVG, SCISSORS_SVG};
use super::paint::{
    Clip, ClipMarks, Focus, FolderSizes, Palette, TrackPaint, CACHE_SCREENS, CLIP, CUT, EDGE,
};
pub(crate) use super::tracks::{lifted_row, row_at};
use super::tracks::{TrackRows, TrackView};
use crate::grid::element::pictures::{paint_in_slot, Pictures};
use crate::list::pane::{visible_rows, Grow, Held, ROW_HEIGHT};
use crate::palette::Theme;
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, TransformationMatrix, Window,
};
use std::{cell::RefCell, rc::Rc};
#[path = "tags.rs"]
mod tags;

mod menu_target;
mod options;
mod paint;

pub struct TrackElement {
    tracks: Rc<[TrackView]>,
    at: usize,
    held: Option<Rc<Held>>,
    focus: Rc<Focus>,
    scroll_y: f32,
    clip: Option<Rc<dyn ClipMarks>>,
    sizes: Option<Rc<dyn FolderSizes>>,
    pictures: Option<Rc<Pictures>>,
    paint: Rc<RefCell<TrackPaint>>,
    words: Rc<crate::row::format::Words>,
    languages: bool,
    shown: Option<Rc<[usize]>>,
    tag_slot: bool,
    locating: bool,
    query: Rc<str>,
    row_height: f32,
    band: f32,
}

impl TrackElement {
    // Words is required: a default would let a track print another locale's size text unnoticed.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tracks: Rc<[TrackView]>,
        at: usize,
        held: Option<Rc<Held>>,
        focus: Rc<Focus>,
        scroll_y: f32,
        clip: Option<Rc<dyn ClipMarks>>,
        paint: Rc<RefCell<TrackPaint>>,
        words: Rc<crate::row::format::Words>,
    ) -> Self {
        Self {
            tracks,
            at,
            held,
            focus,
            scroll_y,
            clip,
            sizes: None,
            pictures: None,
            paint,
            words,
            languages: true,
            shown: None,
            tag_slot: false,
            locating: false,
            query: "".into(),
            row_height: ROW_HEIGHT,
            band: ROW_HEIGHT,
        }
    }

    fn view(&self) -> &TrackView {
        &self.tracks[self.at]
    }

    fn total(&self) -> usize {
        match &self.view().rows {
            TrackRows::Peek(rows) => rows.len(),
            TrackRows::Listing => match &self.shown {
                Some(rows) => rows.len(),
                None => self.held.as_ref().map_or(0, |held| held.total),
            },
            TrackRows::Preview | TrackRows::Blank => 0,
        }
    }
}

impl IntoElement for TrackElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for TrackElement {
    type RequestLayoutState = ();
    type PrepaintState = Palette;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Palette {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let mut paint = self.paint.borrow_mut();
        let advance = f32::from(paint.body_advance(window));
        let lane = super::geometry::lane(&paint.typeface);
        let view = &self.tracks[self.at];
        // A refused rename grows the active track's edited row and moves the rows below it down.
        let grow = if view.slot == self.focus.rename_slot.unwrap_or(Slot::Active) {
            self.focus.grow
        } else {
            Grow::default()
        };
        paint.drawn = Some(super::paint::Drawn {
            bounds,
            focus: Rc::clone(&self.focus),
            tracks: Rc::clone(&self.tracks),
            at: self.at,
            scroll: self.scroll_y,
            held: self.held.clone(),
            shown: self.shown.clone(),
            grow,
            row_height: self.row_height,
            band: self.band,
            lane,
        });
        let width = f32::from(bounds.size.width);
        let shows_size = view.slot == Slot::Active;
        let shaping = Shaping {
            font: paint.typeface.font.clone(),
            body: paint.typeface.body,
            caption: paint.typeface.caption,
            plain_cells: name_budget_in(width, lane, shows_size, false, advance),
            chevron_cells: name_budget_in(width, lane, shows_size, true, advance),
            clip_cells: ((GAP + CLIP) / advance.max(1.)).ceil() as usize,
            size: shows_size,
            languages: self.languages,
            scale: window.scale_factor(),
            words: Rc::clone(&self.words),
            locating: self.locating,
            query: Rc::clone(&self.query),
        };
        // ColumnRow.qml reserves the tag slot only on a row that shows tags, so an untagged name keeps the whole budget.
        let reserve = tags::reserved_cells(self.tag_slot, advance);
        let tagged_shaping = Shaping {
            plain_cells: shaping.plain_cells.saturating_sub(reserve),
            chevron_cells: shaping.chevron_cells.saturating_sub(reserve),
            ..shaping.clone()
        };
        let dropping = self
            .focus
            .drop
            .as_ref()
            .filter(|(slot, _, _)| *slot == view.slot)
            .map(|(_, row, label)| {
                let run = crate::list::painted_row::DropLine::shaped(
                    &mut paint.drop_line,
                    label,
                    &shaping.font,
                    shaping.caption,
                    window,
                );
                (*row, f32::from(run.line().width))
            });
        let height = f32::from(bounds.size.height);
        let total = self.total();
        let (first, last) = grow.span(self.scroll_y.max(0.), height, self.row_height, total);
        // An empty clipboard costs one question a frame, never one a row.
        let clip = self.clip.as_ref().filter(|clip| !clip.is_empty());
        let held = self.held.as_deref();
        let TrackPaint {
            cache,
            visible,
            pictures,
            first: start,
            ..
        } = &mut *paint;
        cache.begin(&shaping, visible_rows(height) * CACHE_SCREENS);
        visible.clear();
        pictures.clear();
        // ColumnRow.qml: only the listing's own column draws thumbnails, never a peek.
        let lender = self
            .pictures
            .as_deref()
            .filter(|_| view.slot == Slot::Active);
        // A thumbnail is filtered to the mark slot's device pixels, the side paint draws it at.
        let slot_pixels = (picture_box(self.band).2 * window.scale_factor()).ceil() as u32;
        *start = first;
        for position in first..last.max(first) {
            let index = self.shown.as_ref().map_or(position, |rows| rows[position]);
            let row = row_at(&view.rows, held, index);
            let mark = match (clip, row) {
                (Some(clip), Some(row)) => clip.mark(&view.path, row.name()),
                _ => Clip::None,
            };
            let walk = match (row, &self.sizes) {
                (Some(Row::Listing(facts)), Some(sizes)) if shows_size && facts.dir => {
                    sizes.size_for(index)
                }
                _ => None,
            };
            let tagged = reserve > 0
                && matches!(row, Some(Row::Listing(facts))
                    if facts.tags.as_ref().is_some_and(|names| !names.is_empty()));
            let row_shaping = if tagged { &tagged_shaping } else { &shaping };
            let narrow = dropping
                .filter(|(target, _)| *target == index)
                .map(|(_, label_width)| {
                    let mut narrow = row_shaping.clone();
                    let room = ((width - PAD_X - label_width - GAP - super::geometry::NAME_X)
                        / advance.max(1.))
                    .floor()
                    .max(0.) as usize;
                    let room = room.saturating_sub(if tagged { reserve } else { 0 });
                    narrow.plain_cells = narrow.plain_cells.min(room);
                    narrow.chevron_cells = narrow.chevron_cells.min(room);
                    narrow
                });
            let prepared = row.map(|row| {
                cache.get(
                    index,
                    row,
                    mark != Clip::None,
                    walk,
                    narrow.as_ref().unwrap_or(row_shaping),
                    window,
                )
            });
            if let Some(prepared) = prepared
                .as_ref()
                .filter(|_| shows_size && self.focus.cursor == Some(index))
            {
                prepared.prepare_cursor(&shaping.font, shaping.body, window);
            }
            // ColumnRow.qml dims a cut row's thumbnail with the rest of its mark.
            pictures.push(match (lender, row) {
                (Some(lender), Some(Row::Listing(facts))) => {
                    lender.lend(index, facts).map(|picture| {
                        let dim = if mark == Clip::Cut { CUT } else { 1. };
                        lender.slotted(picture, slot_pixels, dim)
                    })
                }
                _ => None,
            });
            visible.push((index, prepared, mark));
        }
        let palette = Palette::inherited(&window.text_style(), theme);
        paint.painted = Some(palette);
        palette
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        palette: &mut Palette,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.paint_track(bounds, *palette, window, cx);
    }
}

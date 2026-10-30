//! One custom Element per drawn track: fixed rows painted as quads, marks and prepared glyph runs, no layout node per row.
use super::cache::{Row, Shaping};
use super::geometry::{cells, mark_box, name_budget, picture_box, Slot, CHEVRON, DIVIDER, GAP};
use super::glyphs::{paint_line, paint_svg, CHEVRON_SVG, COPY_SVG, SCISSORS_SVG};
use super::paint::{
    Clip, ClipMarks, Focus, FolderSizes, Palette, TrackPaint, CACHE_SCREENS, CLIP, CUT, EDGE,
};
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

mod options;

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
        }
    }

    fn view(&self) -> &TrackView {
        &self.tracks[self.at]
    }

    pub(crate) fn with_row_height(mut self, row_height: f32) -> Self {
        self.row_height = row_height;
        self
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

pub(crate) fn row_at<'a>(
    rows: &'a TrackRows,
    held: Option<&'a Held>,
    index: usize,
) -> Option<Row<'a>> {
    match rows {
        TrackRows::Peek(rows) => rows.get(index).map(Row::Peek),
        TrackRows::Listing => held?.row(index).map(Row::Listing),
        TrackRows::Preview | TrackRows::Blank => None,
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
        let view = &self.tracks[self.at];
        // A refused rename grows the active track's edited row and moves the rows below it down.
        let grow = if view.slot == Slot::Active {
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
        });
        let width = f32::from(bounds.size.width);
        let shows_size = view.slot == Slot::Active;
        let shaping = Shaping {
            font: paint.typeface.font.clone(),
            body: paint.typeface.body,
            caption: paint.typeface.caption,
            plain_cells: name_budget(width, shows_size, false, advance)
                .saturating_sub(tags::reserved_cells(self.tag_slot, advance)),
            chevron_cells: name_budget(width, shows_size, true, advance)
                .saturating_sub(tags::reserved_cells(self.tag_slot, advance)),
            clip_cells: ((GAP + CLIP) / advance.max(1.)).ceil() as usize,
            size: shows_size,
            languages: self.languages,
            scale: window.scale_factor(),
            words: Rc::clone(&self.words),
            locating: self.locating,
            query: Rc::clone(&self.query),
        };
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
            let prepared =
                row.map(|row| cache.get(index, row, mark != Clip::None, walk, &shaping, window));
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
                        if mark == Clip::Cut {
                            lender.faded(picture, CUT)
                        } else {
                            picture
                        }
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
        let palette = *palette;
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let paint = self.paint.borrow();
        let focus = &paint.drawn.as_ref().expect("prepainted track").focus;
        let view = &self.tracks[self.at];
        let held = self.held.as_deref();
        let active = view.slot == Slot::Active;
        let width = f32::from(bounds.size.width);
        let left = bounds.origin.x;
        let grow = paint
            .drawn
            .as_ref()
            .map_or(Grow::default(), |drawn| drawn.grow);
        let top_of = |at: usize| {
            bounds.origin.y
                + px(grow.top(paint.first + at, self.row_height) - self.scroll_y.max(0.))
        };
        // The edited row's plate and edge take its band, so the refusal sits on the row's own ground.
        let row_box = |top: Pixels, w: f32, at: usize| {
            let height = grow.height(paint.first + at, self.row_height);
            Bounds::new(point(left, top), size(px(w), px(height)))
        };
        window.paint_quad(fill(
            bounds,
            crate::window::glass::list_ground(palette.background, window, cx),
        ));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (at, (index, prepared, _)) in paint.visible.iter().enumerate() {
                if prepared.is_none() {
                    continue;
                }
                let top = top_of(at);
                let cursor = active && focus.cursor == Some(*index);
                let lifted = !cursor && lifted_row(view, held, *index);
                if let Some(plate) = focus.plate(&palette, view.slot, *index, lifted) {
                    window.paint_quad(fill(row_box(top, width, at), plate));
                }
                if focus.edge(view.slot, *index) {
                    window.paint_quad(fill(row_box(top, EDGE, at), palette.edge()));
                }
                if let Some(prepared) = prepared {
                    let chevron = prepared.dir && (cursor || lifted);
                    if let Some((from, to)) = prepared.wash_span(chevron, cursor) {
                        let line = prepared.name(chevron).line();
                        let x = left + px(cells(width, active, chevron).name_x);
                        let y = top + (px(ROW_HEIGHT) - line.ascent - line.descent) / 2.;
                        window.paint_quad(fill(
                            Bounds::new(
                                point(x + from, y),
                                size(to - from, line.ascent + line.descent),
                            ),
                            palette.accent.opacity(crate::g2::sr1::run::RUN_WASH),
                        ));
                    }
                }
            }
            // One layer for every mark and glyph, so the scene orders the whole track once rather than once a line.
            let mut pictures_painted = 0;
            window.paint_layer(bounds, |window| {
                for (at, ((index, prepared, clip), picture)) in
                    paint.visible.iter().zip(&paint.pictures).enumerate()
                {
                    let Some(prepared) = prepared else { continue };
                    let top = top_of(at);
                    let cursor = active && focus.cursor == Some(*index);
                    let lifted = !cursor && lifted_row(view, held, *index);
                    let chevron = prepared.dir && (cursor || lifted);
                    let inks = palette.row_ink(*clip, cursor, lifted, view.dim);
                    let geometry = cells(width, active, chevron);
                    // ColumnRow.qml: a thumbnail fills the whole mark slot, fitted whole, in place of the glyph.
                    if let Some(picture) = picture {
                        let (slot_x, slot_y, side) = picture_box();
                        let slot = Bounds::new(
                            point(left + px(slot_x), top + px(slot_y)),
                            size(px(side), px(side)),
                        );
                        paint_in_slot(picture, slot, window);
                        pictures_painted += 1;
                    } else {
                        let (mark_x, mark_y, mark) = mark_box();
                        let (key, data) = prepared.mark.svg();
                        let _ = window.paint_svg(
                            Bounds::new(
                                point(left + px(mark_x), top + px(mark_y)),
                                size(px(mark), px(mark)),
                            ),
                            key,
                            Some(data),
                            TransformationMatrix::unit(),
                            inks.mark,
                            cx,
                        );
                    }
                    let name = prepared.name_for(chevron, cursor);
                    let name_x = left + px(geometry.name_x);
                    paint_line(name, name_x, top, inks.name, window);
                    let mut name_width = f32::from(name.line().width);
                    if let Some(location) = prepared.location.as_ref() {
                        let gap = GAP;
                        let bright = cursor
                            || lifted
                            || self.focus.selected.contains(index)
                            || self.focus.hover == Some((view.slot, *index));
                        paint_line(
                            location,
                            name_x + name.line().width + px(gap),
                            top,
                            if bright { inks.name } else { palette.metadata },
                            window,
                        );
                        name_width += gap + f32::from(location.line().width);
                    }
                    if let Some(Row::Listing(facts)) = row_at(&view.rows, held, *index) {
                        tags::paint(
                            facts,
                            point(left, top),
                            name_width,
                            geometry,
                            *clip,
                            &theme,
                            window,
                        );
                    }
                    if let (Some(size_run), Some(right)) = (&prepared.size, geometry.size_right) {
                        paint_line(
                            size_run,
                            left + px(right) - size_run.line().width,
                            top,
                            inks.name,
                            window,
                        );
                    }
                    if let Some(x) = geometry.chevron_x {
                        let at = Bounds::new(
                            point(left + px(x), top + px((ROW_HEIGHT - CHEVRON) / 2.)),
                            size(px(CHEVRON), px(CHEVRON)),
                        );
                        paint_svg(CHEVRON_SVG, at, inks.chevron, window, cx);
                    }
                    if *clip != Clip::None {
                        let x = geometry.clip_x();
                        let at = Bounds::new(
                            point(left + px(x), top + px((ROW_HEIGHT - CLIP) / 2. - 1.)),
                            size(px(CLIP), px(CLIP)),
                        );
                        let glyph = if *clip == Clip::Cut {
                            SCISSORS_SVG
                        } else {
                            COPY_SVG
                        };
                        paint_svg(glyph, at, inks.clip, window, cx);
                    }
                }
            });
            paint.painted_pictures.set(pictures_painted);
            if let Some(x) = view.divider_x() {
                let at = point(bounds.origin.x + px(x - view.x), bounds.origin.y);
                window.paint_quad(fill(
                    Bounds::new(at, size(px(DIVIDER), bounds.size.height)),
                    palette.hairline(),
                ));
            }
        });
    }
}

/// The row a track lifts, the one named on the path to the active track; the capture probe reads the same rule.
pub(crate) fn lifted_row(view: &TrackView, held: Option<&Held>, index: usize) -> bool {
    view.lifted
        .as_deref()
        .is_some_and(|name| row_at(&view.rows, held, index).is_some_and(|row| row.name() == name))
}

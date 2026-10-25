//! One custom Element per drawn track: fixed rows painted as quads, marks and prepared glyph runs, no layout node per row.
use super::cache::{Row, Shaping};
use super::geometry::{cells, name_budget, Slot, CHEVRON, DIVIDER, GAP, MARK, MARK_SLOT, PAD_X};
use super::paint::{
    Clip, ClipMarks, Focus, FolderSizes, Palette, TrackPaint, CACHE_SCREENS, CLIP, CUT, EDGE,
};
use super::tracks::{TrackRows, TrackView};
use crate::grid::element::pictures::{paint_in_slot, Pictures};
use crate::list::pane::{visible_rows, Held, ROW_HEIGHT};
use crate::palette::Theme;
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    GlyphRun, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, Style,
    TransformationMatrix, Window,
};
use std::{cell::RefCell, rc::Rc};

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
        }
    }

    /// The folder walks the active track's size cells read; a peek track never shows a size.
    pub fn with_sizes(mut self, sizes: Rc<dyn FolderSizes>) -> Self {
        self.sizes = Some(sizes);
        self
    }

    /// The pane's thumbnail lender, which only the active track's listing rows draw from.
    pub fn with_pictures(mut self, pictures: Rc<Pictures>) -> Self {
        self.pictures = Some(pictures);
        self
    }

    fn view(&self) -> &TrackView {
        &self.tracks[self.at]
    }

    fn total(&self) -> usize {
        match &self.view().rows {
            TrackRows::Peek(rows) => rows.len(),
            TrackRows::Listing => self.held.as_ref().map_or(0, |held| held.total),
            TrackRows::Preview | TrackRows::Blank => 0,
        }
    }
}

fn row_at<'a>(rows: &'a TrackRows, held: Option<&'a Held>, index: usize) -> Option<Row<'a>> {
    match rows {
        TrackRows::Peek(rows) => rows.get(index).map(Row::Peek),
        TrackRows::Listing => held?.row(index).map(Row::Listing),
        TrackRows::Preview | TrackRows::Blank => None,
    }
}

/// Paints a prepared line from the tiles its glyph run kept, its baseline centred on the row.
fn paint_line(run: &GlyphRun, x: Pixels, top: Pixels, color: Hsla, window: &mut Window) {
    let line = run.line();
    let baseline =
        window.pixel_snap(top + (px(ROW_HEIGHT) - line.ascent - line.descent) / 2. + line.ascent);
    let _ = window.paint_glyph_run(point(x, baseline), run, color);
}

macro_rules! glyph {
    ($key:literal, $d:literal) => {
        (
            $key,
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" "#,
                r#"stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d=""#,
                $d,
                r#""/></svg>"#
            )
            .as_bytes(),
        )
    };
}

// Lucide's chevron-right, copy, scissors and move-horizontal in the Omarchy cut ui/js/Icons.js carries.
pub const CHEVRON_SVG: (&str, &[u8]) = glyph!("flea-mark:chevron-right", "M9 18l6-6-6-6");
pub const COPY_SVG: (&str, &[u8]) = glyph!("flea-mark:copy", "M9 8h12v13H9z M4 16V3h13");
pub const SCISSORS_SVG: (&str, &[u8]) = glyph!(
    "flea-mark:scissors",
    "M5 3l14 14 M19 3L5 17 M2 17h5v5H2z M17 17h5v5h-5z"
);

pub const MOVE_SVG: (&str, &[u8]) = glyph!(
    "flea-mark:move-horizontal",
    "M4 12h16 M8 8l-4 4 4 4 M16 8l4 4-4 4"
);

pub(super) fn paint_svg(
    glyph: (&'static str, &'static [u8]),
    bounds: Bounds<Pixels>,
    ink: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let _ = window.paint_svg(
        bounds,
        SharedString::new_static(glyph.0),
        Some(glyph.1),
        TransformationMatrix::unit(),
        ink,
        cx,
    );
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
        paint.drawn = Some(super::paint::Drawn {
            bounds,
            tracks: Rc::clone(&self.tracks),
            at: self.at,
            scroll: self.scroll_y,
            held: self.held.clone(),
        });
        let width = f32::from(bounds.size.width);
        let shows_size = view.slot == Slot::Active;
        let shaping = Shaping {
            font: paint.typeface.font.clone(),
            body: paint.typeface.body,
            caption: paint.typeface.caption,
            plain_cells: name_budget(width, shows_size, false, advance),
            chevron_cells: name_budget(width, shows_size, true, advance),
            clip_cells: ((GAP + CLIP) / advance.max(1.)).ceil() as usize,
            size: shows_size,
            scale: window.scale_factor(),
            words: Rc::clone(&self.words),
        };
        let height = f32::from(bounds.size.height);
        let total = self.total();
        let first = (self.scroll_y.max(0.) / ROW_HEIGHT).floor() as usize;
        let last = (((self.scroll_y.max(0.) + height) / ROW_HEIGHT).ceil() as usize).min(total);
        // An empty clipboard costs one question a frame, never one a row.
        let clip = self.clip.as_ref().filter(|clip| !clip.is_empty());
        let held = self.held.as_deref();
        let TrackPaint {
            cache,
            visible,
            pictures,
            ..
        } = &mut *paint;
        cache.begin(&shaping, visible_rows(height) * CACHE_SCREENS);
        visible.clear();
        pictures.clear();
        // ColumnRow.qml: only the listing's own column draws thumbnails, never a peek.
        let lender = self.pictures.as_deref().filter(|_| view.slot == Slot::Active);
        for index in first..last.max(first) {
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
            // ColumnRow.qml dims a cut row's thumbnail with the rest of its mark.
            pictures.push(match (lender, row) {
                (Some(lender), Some(Row::Listing(facts))) => lender.lend(index, facts).map(|picture| {
                    if mark == Clip::Cut {
                        lender.faded(picture, CUT)
                    } else {
                        picture
                    }
                }),
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
        let paint = self.paint.borrow();
        let view = &self.tracks[self.at];
        let held = self.held.as_deref();
        let active = view.slot == Slot::Active;
        let width = f32::from(bounds.size.width);
        let left = bounds.origin.x;
        let top_of =
            |index: usize| bounds.origin.y + px(index as f32 * ROW_HEIGHT - self.scroll_y.max(0.));
        let row_box =
            |top: Pixels, w: f32| Bounds::new(point(left, top), size(px(w), px(ROW_HEIGHT)));
        window.paint_quad(fill(bounds, palette.background));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (index, prepared, _) in &paint.visible {
                if prepared.is_none() {
                    continue;
                }
                let top = top_of(*index);
                let cursor = active && self.focus.cursor == Some(*index);
                let lifted = !cursor && self.lifted(view, held, *index);
                if let Some(plate) = self.focus.plate(&palette, view.slot, *index, lifted) {
                    window.paint_quad(fill(row_box(top, width), plate));
                }
                if self.focus.edge(view.slot, *index) {
                    window.paint_quad(fill(row_box(top, EDGE), palette.edge()));
                }
            }
            // One layer for every mark and glyph, so the scene orders the whole track once rather than once a line.
            let mut pictures_painted = 0;
            window.paint_layer(bounds, |window| {
                for ((index, prepared, clip), picture) in paint.visible.iter().zip(&paint.pictures) {
                    let Some(prepared) = prepared else { continue };
                    let top = top_of(*index);
                    let cursor = active && self.focus.cursor == Some(*index);
                    let lifted = !cursor && self.lifted(view, held, *index);
                    let chevron = prepared.dir && (cursor || lifted);
                    let inks = palette.row_ink(*clip, cursor, lifted, view.dim);
                    let geometry = cells(width, active, chevron);
                    let mark_at = point(
                        left + px(PAD_X + (MARK_SLOT - MARK) / 2.),
                        top + px((ROW_HEIGHT - MARK) / 2.),
                    );
                    // ColumnRow.qml: a thumbnail fills the whole mark slot, fitted whole, in place of the glyph.
                    if let Some(picture) = picture {
                        let slot = Bounds::new(
                            point(left + px(PAD_X), top + px((ROW_HEIGHT - MARK_SLOT) / 2.)),
                            size(px(MARK_SLOT), px(MARK_SLOT)),
                        );
                        paint_in_slot(picture, slot, window);
                        pictures_painted += 1;
                    } else {
                        let (key, data) = prepared.mark.svg();
                        let _ = window.paint_svg(
                            Bounds::new(mark_at, size(px(MARK), px(MARK))),
                            key,
                            Some(data),
                            TransformationMatrix::unit(),
                            inks.mark,
                            cx,
                        );
                    }
                    let name = prepared.name(chevron);
                    let name_x = left + px(geometry.name_x);
                    paint_line(name, name_x, top, inks.name, window);
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
                        let x = geometry.clip_x(f32::from(name.line().width));
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

impl TrackElement {
    fn lifted(&self, view: &TrackView, held: Option<&Held>, index: usize) -> bool {
        view.lifted.as_deref().is_some_and(|name| {
            row_at(&view.rows, held, index).is_some_and(|row| row.name() == name)
        })
    }
}

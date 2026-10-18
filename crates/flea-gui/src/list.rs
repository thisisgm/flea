//! The list pane: one custom Element paints the visible rows from prepared lines, with no layout node per row.
#[cfg(test)]
pub(crate) mod bench;
pub mod pane;
mod locale;
pub mod seam;
#[cfg(test)]
mod tests;

use crate::row::{
    self,
    cache::{Prepared, RowCache, Shaping},
    NameInk,
};
use flea::protocol::RowFacts;
use gpui::{
    fill, font, point, px, relative, size, App, Bounds, ContentMask, Element, ElementId, Font,
    GlobalElementId, GlyphRun, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Style,
    TextRun, TextStyle, TransformationMatrix, Window,
};
use pane::{Held, ROW_HEIGHT};
use std::{cell::RefCell, rc::Rc};

// Board list.html: 14 padding, a 23 mark slot and 9 gap, a 17 mark, Size 70, Modified 134, a 28 end, a 2 edge.
const PAD_X: f32 = 14.;
const MARK_SLOT: f32 = 23.;
const MARK: f32 = 17.;
const LABEL_X: f32 = PAD_X + MARK_SLOT + 9.;
const SIZE_W: f32 = 70.;
const DATE_W: f32 = 134.;
const END_W: f32 = 28.;
const EDGE_W: f32 = 2.;
const CURSOR_WASH: f32 = 0.18;
// Rows kept shaped past the viewport, so a short scroll back reshapes nothing.
const CACHE_SCREENS: usize = 3;

/// Omarchy's default face at the board's body and caption sizes, until the pane follows the live font-set.
#[derive(Clone)]
pub struct Typeface {
    pub font: Font,
    pub body: Pixels,
    pub caption: Pixels,
}

impl Typeface {
    pub fn omarchy_default() -> Self {
        Self {
            font: font("JetBrainsMono Nerd Font"),
            body: px(14.),
            caption: px(12.),
        }
    }
}

/// The palette roles a row draws with, to be filled from Omarchy's theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Hsla,
    pub foreground: Hsla,
    pub metadata: Hsla,
    pub metadata_frame: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
    pub symlink: Hsla,
    pub executable: Hsla,
}

impl Palette {
    /// Interim roles from the inherited text style until the theme tokens land.
    pub fn inherited(style: &TextStyle) -> Self {
        let foreground = style.color;
        Self {
            background: style.background_color.unwrap_or(Hsla {
                l: 1. - foreground.l,
                ..foreground
            }),
            foreground,
            metadata: foreground.opacity(0.62),
            metadata_frame: foreground.opacity(0.5),
            accent: foreground,
            accent_frame: foreground,
            symlink: foreground,
            executable: foreground,
        }
    }

    fn name(&self, ink: NameInk, lifted: bool) -> Hsla {
        match (lifted, ink) {
            (true, _) | (false, NameInk::Foreground) => self.foreground,
            (false, NameInk::Symlink) => self.symlink,
            (false, NameInk::Executable) => self.executable,
        }
    }
}

/// Paint state a pane keeps across frames: shaped rows, the face's metrics and this frame's rows.
pub struct ListState {
    pub typeface: Typeface,
    pub rows: RowCache,
    advance: Option<Pixels>,
    visible: Vec<(usize, Option<Rc<Prepared>>)>,
}

impl ListState {
    pub fn new(typeface: Typeface) -> Self {
        Self {
            typeface,
            rows: RowCache::default(),
            advance: None,
            visible: Vec::new(),
        }
    }

    // One advance of "0" at the body size, Theme.bodyAdvance, which turns the name width into cells.
    fn body_advance(&mut self, window: &Window) -> Pixels {
        if let Some(advance) = self.advance {
            return advance;
        }
        let text = window.text_system();
        let id = text.resolve_font(&self.typeface.font);
        let advance = text
            .advance(id, self.typeface.body, '0')
            .map_or(self.typeface.body * 0.6, |advance| advance.width);
        self.advance = Some(advance);
        advance
    }
}

pub struct ListElement {
    held: Rc<Held>,
    scroll_y: f32,
    cursor: usize,
    state: Rc<RefCell<ListState>>,
}

impl ListElement {
    pub fn new(held: Rc<Held>, scroll_y: f32, cursor: usize, state: Rc<RefCell<ListState>>) -> Self {
        Self {
            held,
            scroll_y,
            cursor,
            state,
        }
    }
}

/// Shapes one row's three cells once, the name already middle-elided to its cells so no long name is shaped whole.
fn prepare(facts: &RowFacts, shaping: &Shaping, window: &Window) -> Prepared {
    let text = window.text_system();
    let line = |value: &str, size: Pixels| {
        let run = TextRun {
            len: value.len(),
            font: shaping.font.clone(),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        GlyphRun::new(text.layout_line(value, size, &[run], None))
    };
    let name = row::decorated_name(facts);
    Prepared {
        name: line(&row::middle_elide(&name, shaping.name_cells), shaping.body),
        size: line(&row::size_text(facts), shaping.caption),
        date: line(&row::format_date(facts.mtime), shaping.caption),
        mark: row::marks::Mark::for_row(&facts.icon, facts.mode),
        ink: row::name_ink(facts),
    }
}

/// Paints a prepared line from the tiles its glyph run kept, so a row already seen costs no lookup per glyph.
fn paint_line(run: &GlyphRun, x: Pixels, top: Pixels, color: Hsla, window: &mut Window) {
    let line = run.line();
    let baseline = window.pixel_snap(top + (px(ROW_HEIGHT) - line.ascent - line.descent) / 2. + line.ascent);
    let _ = window.paint_glyph_run(point(x, baseline), run, color);
}

impl IntoElement for ListElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ListElement {
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
        _: &mut App,
    ) -> Palette {
        let mut state = self.state.borrow_mut();
        let advance = state.body_advance(window);
        let name_width = bounds.size.width - px(LABEL_X + SIZE_W + DATE_W + END_W);
        let shaping = Shaping {
            listing: self.held.listing,
            font: state.typeface.font.clone(),
            body: state.typeface.body,
            caption: state.typeface.caption,
            name_cells: (name_width / advance).floor().max(0.) as usize,
            scale: window.scale_factor(),
        };
        let height = f32::from(bounds.size.height);
        let first = (self.scroll_y / ROW_HEIGHT).floor() as usize;
        let last = (((self.scroll_y + height) / ROW_HEIGHT).ceil() as usize).min(self.held.total);
        let ListState { rows, visible, .. } = &mut *state;
        rows.begin(&shaping, pane::visible_rows(height) * CACHE_SCREENS);
        visible.clear();
        for index in first..last.max(first) {
            let prepared = self
                .held
                .row(index)
                .map(|facts| rows.get_or_prepare(index, facts, || prepare(facts, &shaping, window)));
            visible.push((index, prepared));
        }
        Palette::inherited(&window.text_style())
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
        let state = self.state.borrow();
        let left = bounds.origin.x;
        let right = bounds.origin.x + bounds.size.width;
        let top_of = |index: usize| bounds.origin.y + px(index as f32 * ROW_HEIGHT - self.scroll_y);
        window.paint_quad(fill(bounds, palette.background));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if state.visible.iter().any(|(index, _)| *index == self.cursor) {
                let top = top_of(self.cursor);
                let row = Bounds::new(point(left, top), size(bounds.size.width, px(ROW_HEIGHT)));
                window.paint_quad(fill(row, palette.accent.opacity(CURSOR_WASH)));
                let edge = Bounds::new(point(left, top), size(px(EDGE_W), px(ROW_HEIGHT)));
                window.paint_quad(fill(edge, palette.accent_frame));
            }
            // One layer for every mark and glyph, so the scene orders the whole list once rather than once a line.
            window.paint_layer(bounds, |window| {
                let names = Bounds::new(
                    point(left + px(LABEL_X), bounds.origin.y),
                    size(
                        (bounds.size.width - px(LABEL_X + SIZE_W + DATE_W + END_W)).max(px(0.)),
                        bounds.size.height,
                    ),
                );
                for (index, prepared) in &state.visible {
                    let Some(prepared) = prepared else { continue };
                    let lifted = *index == self.cursor;
                    let top = top_of(*index);
                    let mark_at = point(
                        left + px(PAD_X + (MARK_SLOT - MARK) / 2.),
                        top + px((ROW_HEIGHT - MARK) / 2.),
                    );
                    let (key, data) = prepared.mark.svg();
                    let mark_ink = if lifted { palette.foreground } else { palette.metadata_frame };
                    let _ = window.paint_svg(
                        Bounds::new(mark_at, size(px(MARK), px(MARK))),
                        key,
                        Some(data),
                        TransformationMatrix::unit(),
                        mark_ink,
                        cx,
                    );
                    let meta = if lifted { palette.foreground } else { palette.metadata };
                    let size_right = right - px(END_W + DATE_W);
                    paint_line(&prepared.size, size_right - prepared.size.line().width, top, meta, window);
                    let date_right = right - px(END_W);
                    paint_line(&prepared.date, date_right - prepared.date.line().width, top, meta, window);
                }
                window.with_content_mask(Some(ContentMask { bounds: names }), |window| {
                    for (index, prepared) in &state.visible {
                        let Some(prepared) = prepared else { continue };
                        let ink = palette.name(prepared.ink, *index == self.cursor);
                        paint_line(&prepared.name, names.origin.x, top_of(*index), ink, window);
                    }
                });
            });
        });
    }
}

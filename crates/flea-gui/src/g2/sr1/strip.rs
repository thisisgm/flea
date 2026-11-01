//! The query strip in the header's slot: mark, query and caret, the scope and count, and the way out at the right edge.
use crate::list::Typeface;
use gpui::{
    fill, point, px, relative, size, App, Bounds, Element, ElementId, GlobalElementId, GlyphRun,
    Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, Style, TextRun,
    TransformationMatrix, Window,
};
use std::{cell::RefCell, rc::Rc};
#[path = "query_paint.rs"]
mod query_paint;
pub use query_paint::QueryPaint;

// Board search.html: a 27 strip, 14 padding, a 16 mark, 9 gaps, a 2 by 13 caret 1 after the query, a 1 hairline.
pub const HEIGHT: f32 = 27.;
pub const PAD_X: f32 = 14.;
pub const MARK: f32 = 16.;
pub const GAP: f32 = 9.;
pub const CARET_W: f32 = 2.;
pub const CARET_H: f32 = 13.;
pub const CARET_GAP: f32 = 1.;
pub const HAIRLINE: f32 = 1.;
// SearchStrip.qml's ruleOpacity: the hairline is the foreground at 12%.
pub const RULE_OPACITY: f32 = 0.12;
const ELLIPSIS: &str = "\u{2026}";
const SEARCH_MARK: &[u8] = concat!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" "#,
    r#"stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M16.7 16.7 21 21 M3 11A8 8 0 1 0 19 11A8 8 0 1 0 3 11Z"/></svg>"#
)
.as_bytes();

/// The strip's words, translated by the controller; the element only shapes and draws them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Words {
    pub query: String,
    pub scope: String,
    pub note: String,
    pub way_out: String,
    pub typing: bool,
}

/// The roles the strip draws with, from the live theme at the join.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub surface: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
}

impl Palette {
    pub fn from_list(list: &crate::list::Palette) -> Self {
        Self {
            surface: list.background,
            foreground: list.foreground,
            muted: list.muted,
            accent: list.accent,
        }
    }
}

/// Where each part starts, left to right, with the scope's room after the count and the way out are placed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boxes {
    pub mark_x: f32,
    pub query_x: f32,
    pub caret_x: f32,
    pub scope_x: f32,
    pub scope_w: f32,
    pub note_x: f32,
    pub way_x: f32,
}

/// SearchStrip.qml's anchors: the scope gives way before the count does, since the count is what keeps changing.
pub fn layout(width: f32, query_w: f32, scope_w: f32, note_w: f32, way_w: f32) -> Boxes {
    let mark_x = PAD_X;
    let query_x = mark_x + MARK + GAP;
    let caret_x = query_x + query_w + CARET_GAP;
    let scope_x = caret_x + CARET_W + GAP;
    let way_x = width - PAD_X - way_w;
    let scope_w = scope_w.min((way_x - scope_x - note_w - 2. * GAP).max(0.));
    let note_x = scope_x + scope_w + GAP;
    Boxes {
        mark_x,
        query_x,
        caret_x,
        scope_x,
        scope_w,
        note_x,
        way_x,
    }
}

/// One shaping of the strip, kept until its words, width or face change.
pub struct Prepared {
    key: (Words, u32, u32, Typeface),
    pub query: Rc<GlyphRun>,
    pub scope: GlyphRun,
    pub note: GlyphRun,
    pub way_out: GlyphRun,
    pub boxes: Boxes,
    // The scope's drawn text, its head cut to an ellipsis when the strip is narrow.
    pub scope_text: String,
}

pub struct StripState {
    pub typeface: Typeface,
    pub prepared: Option<Prepared>,
    pub shapings: u64,
    pub edit: QueryPaint,
}

impl StripState {
    pub fn new(typeface: Typeface) -> Self {
        Self {
            typeface,
            prepared: None,
            shapings: 0,
            edit: QueryPaint::default(),
        }
    }
}

pub struct StripElement {
    input: Option<(gpui::Entity<super::input::QueryInput>, gpui::FocusHandle)>,
    words: Rc<Words>,
    palette: Palette,
    state: Rc<RefCell<StripState>>,
}

impl StripElement {
    pub fn with_input(
        mut self,
        input: gpui::Entity<super::input::QueryInput>,
        focus: gpui::FocusHandle,
    ) -> Self {
        self.input = Some((input, focus));
        self
    }

    pub fn new(words: Rc<Words>, palette: Palette, state: Rc<RefCell<StripState>>) -> Self {
        Self {
            input: None,
            words,
            palette,
            state,
        }
    }
}

fn shape(text: &str, size: Pixels, typeface: &Typeface, window: &Window) -> GlyphRun {
    let run = TextRun {
        len: text.len(),
        font: typeface.font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    GlyphRun::new(window.text_system().layout_line(text, size, &[run], None))
}

// The scope's tail that fits `room` behind an ellipsis, cut on a character boundary.
fn left_elide(full: &GlyphRun, text: &str, room: f32, ellipsis: f32) -> String {
    let line = full.line();
    let total = f32::from(line.width);
    let cut = text
        .char_indices()
        .map(|(at, _)| at)
        .find(|at| total - f32::from(line.x_for_index(*at)) + ellipsis <= room)
        .unwrap_or(text.len());
    format!("{ELLIPSIS}{}", &text[cut..])
}

fn prepare(words: &Words, width: f32, typeface: &Typeface, window: &Window) -> Prepared {
    let body = typeface.body;
    let caption = typeface.caption;
    let query = shape(&words.query, body, typeface, window);
    let note = shape(&words.note, caption, typeface, window);
    let way_out = shape(&words.way_out, caption, typeface, window);
    let full = shape(&words.scope, caption, typeface, window);
    let natural = f32::from(full.line().width);
    let scale = crate::list::Metrics::for_typeface(typeface).header / HEIGHT;
    let mut boxes = layout(
        width / scale,
        f32::from(query.line().width) / scale,
        natural / scale,
        f32::from(note.line().width) / scale,
        f32::from(way_out.line().width) / scale,
    );
    boxes.mark_x *= scale;
    boxes.query_x *= scale;
    boxes.caret_x *= scale;
    boxes.scope_x *= scale;
    boxes.scope_w *= scale;
    boxes.note_x *= scale;
    boxes.way_x *= scale;
    let (scope, scope_text) = if boxes.scope_w + 0.5 >= natural {
        (full, words.scope.clone())
    } else {
        let ellipsis = f32::from(shape(ELLIPSIS, caption, typeface, window).line().width);
        let text = left_elide(&full, &words.scope, boxes.scope_w, ellipsis);
        (shape(&text, caption, typeface, window), text)
    };
    Prepared {
        key: (
            words.clone(),
            width.to_bits(),
            window.scale_factor().to_bits(),
            typeface.clone(),
        ),
        query: Rc::new(query),
        scope,
        note,
        way_out,
        boxes,
        scope_text,
    }
}

fn paint_line(
    run: &GlyphRun,
    x: Pixels,
    top: Pixels,
    color: Hsla,
    height: f32,
    window: &mut Window,
) {
    let line = run.line();
    let baseline =
        window.pixel_snap(top + (px(height) - line.ascent - line.descent) / 2. + line.ascent);
    let _ = window.paint_glyph_run(point(x, baseline), run, color);
}

impl IntoElement for StripElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for StripElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
            size: size(
                relative(1.).into(),
                px(crate::list::Metrics::for_typeface(&self.state.borrow().typeface).header).into(),
            ),
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
    ) {
        let mut state = self.state.borrow_mut();
        let width = f32::from(bounds.size.width);
        let fresh = state.prepared.as_ref().is_some_and(|prepared| {
            prepared.key.0 == *self.words
                && prepared.key.1 == width.to_bits()
                && prepared.key.2 == window.scale_factor().to_bits()
                && prepared.key.3 == state.typeface
        });
        if !fresh {
            let prepared = prepare(&self.words, width, &state.typeface, window);
            state.prepared = Some(prepared);
            state.shapings += 1;
        }
        drop(state);
        self.prepaint_query(bounds, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = self.state.borrow();
        let Some(prepared) = &state.prepared else {
            return;
        };
        let palette = self.palette;
        let height = crate::list::Metrics::for_typeface(&state.typeface).header;
        let scale = height / HEIGHT;
        let (left, top) = (bounds.origin.x, bounds.origin.y);
        let boxes = prepared.boxes;
        window.paint_quad(fill(bounds, palette.surface));
        let rule = Bounds::new(
            point(left, top + px(height - HAIRLINE)),
            size(bounds.size.width, px(HAIRLINE)),
        );
        window.paint_quad(fill(rule, palette.foreground.opacity(RULE_OPACITY)));
        let mark = Bounds::new(
            point(
                left + px(boxes.mark_x),
                top + px((height - MARK * scale) / 2.),
            ),
            size(px(MARK * scale), px(MARK * scale)),
        );
        let _ = window.paint_svg(
            mark,
            SharedString::new_static("flea-mark:search"),
            Some(SEARCH_MARK),
            TransformationMatrix::unit(),
            palette.accent,
            cx,
        );
        // The query and its caret stay together in a mirrored window; an edited query moves as the line it draws.
        let query = self.query_unit(window, cx).unwrap_or(Bounds::new(
            point(left + px(boxes.query_x), top),
            size(
                px(boxes.caret_x + CARET_W * scale - boxes.query_x),
                px(height),
            ),
        ));
        window.with_unmirrored(query, |window| {
            if self.paint_query(window, cx) {
                return;
            }
            paint_line(
                &prepared.query,
                left + px(boxes.query_x),
                top,
                palette.foreground,
                height,
                window,
            );
            if self.words.typing {
                let caret = Bounds::new(
                    point(
                        left + px(boxes.caret_x),
                        top + px((height - CARET_H * scale) / 2.),
                    ),
                    size(px(CARET_W * scale), px(CARET_H * scale)),
                );
                window.paint_quad(fill(caret, palette.accent));
            }
        });
        if boxes.scope_w > 0. {
            paint_line(
                &prepared.scope,
                left + px(boxes.scope_x),
                top,
                palette.muted,
                height,
                window,
            );
        }
        paint_line(
            &prepared.note,
            left + px(boxes.note_x),
            top,
            palette.muted,
            height,
            window,
        );
        paint_line(
            &prepared.way_out,
            left + px(boxes.way_x),
            top,
            palette.muted,
            height,
            window,
        );
    }
}

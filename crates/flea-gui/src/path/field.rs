//! The open path field: the accent frame, the line on the window ground, its selection wash, marked text and caret.
use super::entry::Entry;
use super::paint::{paint_run, Ink, Size};
use super::scale::Scale;
use super::suggestions::FRAME;
use super::view::PathView;
use crate::list::Palette;
use crate::palette::Theme;
use gpui::{
    fill, point, px, quad, relative, size, App, BorderStyle, Bounds, ContentMask, CursorStyle,
    Element, ElementId, ElementInputHandler, Entity, GlobalElementId, GlyphRun, Hitbox,
    HitboxBehavior, InspectorElementId, IntoElement, LayoutId, Pixels, Style, TextRun, Window,
};
use std::ops::Range;
use std::rc::Rc;

// Board path-bar: the frame sits 2 px under the strip's top and 1 px over its rule; the text, wash and caret follow Scale.
pub const TOP: f32 = 2.;
pub const BOTTOM: f32 = 1.;
const SELECTION: f32 = 0.24;

/// The frame inside the chrome's path slot.
pub fn frame(slot: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(
        point(slot.origin.x, slot.origin.y + px(TOP)),
        size(
            slot.size.width,
            (slot.size.height - px(TOP + BOTTOM)).max(px(0.)),
        ),
    )
}

/// Where the text starts: inside the frame's 1 px border, then the field's inset.
pub fn text_origin(frame: Bounds<Pixels>, scale: &Scale) -> Pixels {
    frame.origin.x + px(FRAME + scale.inset)
}

/// The caret's left edge: 1 px past the text's advance, the board's 8 px gap less its 7 px margin.
pub fn caret_left(origin: Pixels, advance: Pixels) -> Pixels {
    origin + advance + px(FRAME)
}

/// The top of a band of height px centred in the frame's inside, on a whole pixel.
pub fn band_top(frame: Bounds<Pixels>, height: f32) -> Pixels {
    let inside = f32::from(frame.size.height) - 2. * FRAME;
    px(f32::from(frame.origin.y) + FRAME + ((inside - height) / 2.).floor())
}

/// The selection wash over the selected run from x `from` to `to`, its height centred in the frame.
pub fn wash_box(frame: Bounds<Pixels>, from: Pixels, to: Pixels, scale: &Scale) -> Bounds<Pixels> {
    Bounds::new(
        point(from, band_top(frame, scale.wash)),
        size(to - from, px(scale.wash)),
    )
}

/// The caret: 1 px wide, one px past the advance from the text origin, its height centred in the frame.
pub fn caret_box(
    frame: Bounds<Pixels>,
    origin: Pixels,
    advance: Pixels,
    scale: &Scale,
) -> Bounds<Pixels> {
    Bounds::new(
        point(caret_left(origin, advance), band_top(frame, scale.caret)),
        size(px(1.), px(scale.caret)),
    )
}

/// The line shaped at its last change; edits are unique strings, so it never enters the shared cache.
#[derive(Default)]
pub struct Line {
    pub revision: u64,
    pub run: Option<Rc<GlyphRun>>,
    pub scroll: Pixels,
    /// The session whose platform handler the last paint registered, None when it registered none.
    pub registered: Option<u64>,
    /// The shaped text's paragraph direction, used when no character level is available.
    pub right_to_left: bool,
    /// Resolved levels indexed by UTF-8 byte, refreshed with the shaped text.
    pub bidi_levels: Vec<unicode_bidi::Level>,
}

impl Line {
    /// Resolve character directions once per reshape, including trailing whitespace's paragraph level.
    pub fn set_text_direction(&mut self, text: &str) {
        let bidi = unicode_bidi::BidiInfo::new(text, None);
        self.right_to_left = bidi
            .paragraphs
            .first()
            .is_some_and(|para| para.level.is_rtl());
        self.bidi_levels.clear();
        for para in &bidi.paragraphs {
            let levels = bidi.reordered_levels(para, para.range.clone());
            self.bidi_levels
                .extend_from_slice(&levels[para.range.clone()]);
        }
    }

    /// Whether a platform callback, which always reaches the last registered handler, belongs to entry's session.
    pub fn accepts(&self, entry: &Entry) -> bool {
        entry.session().is_some() && entry.session() == self.registered
    }

    /// The caret before byte index: the character's leading edge, its right edge in a right-to-left run.
    pub fn caret_x(&self, index: usize) -> Pixels {
        let Some(run) = &self.run else {
            return px(0.);
        };
        let (glyphs, width) = (glyphs(run), run.line().width);
        let Some(last) = glyphs.clone().max_by_key(|glyph| glyph.0) else {
            return px(0.);
        };
        let next = glyphs.clone().filter(|glyph| glyph.0 >= index);
        let leftward = |glyph: Glyph| {
            self.bidi_levels
                .get(glyph.0)
                .map_or(self.right_to_left, |level| level.is_rtl())
        };
        match next.min_by_key(|glyph| glyph.0) {
            Some(glyph) if leftward(glyph) => right_of(glyphs, glyph.1, width),
            Some(glyph) => glyph.1,
            None if leftward(last) => last.1,
            None => right_of(glyphs, last.1, width),
        }
    }

    /// The left and right x of the glyphs in a byte range, a caret's x for an empty one.
    pub fn span(&self, range: &Range<usize>) -> (Pixels, Pixels) {
        let edges = self.run.as_ref().into_iter().flat_map(|run| {
            let width = run.line().width;
            glyphs(run).map(move |glyph| (glyph, right_of(glyphs(run), glyph.1, width)))
        });
        edges
            .filter(|(glyph, _)| range.contains(&glyph.0))
            .fold(None, |span: Option<(Pixels, Pixels)>, (glyph, right)| {
                Some(span.map_or((glyph.1, right), |(from, to)| {
                    (from.min(glyph.1), to.max(right))
                }))
            })
            .unwrap_or_else(|| {
                let (from, to) = (self.caret_x(range.start), self.caret_x(range.end));
                (from.min(to), from.max(to))
            })
    }

    /// caret_x at a byte offset of entry's line, which paint shaped from entry.display().
    pub fn entry_caret(&self, entry: &Entry, offset: usize) -> Pixels {
        self.caret_x(entry.display_offset(offset))
    }

    /// span over a byte range of entry's line, which paint shaped from entry.display().
    pub fn entry_span(&self, entry: &Entry, range: &Range<usize>) -> (Pixels, Pixels) {
        self.span(&(entry.display_offset(range.start)..entry.display_offset(range.end)))
    }

    pub fn x_for(&self, index: usize) -> Pixels {
        self.run
            .as_ref()
            .map_or(px(0.), |run| run.line().x_for_index(index))
    }

    pub fn index_for(&self, x: Pixels) -> usize {
        self.run
            .as_ref()
            .map_or(0, |run| run.line().closest_index_for_x(x))
    }
}

type Glyph = (usize, Pixels);

// Each glyph as its byte index and left x; a right-to-left paragraph lists them right to left.
fn glyphs(run: &Rc<GlyphRun>) -> impl Iterator<Item = Glyph> + Clone + '_ {
    let runs = run.line().runs.iter();
    runs.flat_map(|run| {
        run.glyphs
            .iter()
            .map(|glyph| (glyph.index, glyph.position.x))
    })
}

// A glyph's right edge: the left of the nearest glyph drawn right of it, or the line's end.
fn right_of(glyphs: impl Iterator<Item = Glyph>, x: Pixels, width: Pixels) -> Pixels {
    glyphs
        .map(|glyph| glyph.1)
        .filter(|&left| left > x)
        .fold(width, |a, b| a.min(b))
}

/// The input method's rectangle for a UTF-16 range; `text` is the box whose left edge is where unscrolled text starts.
pub fn ime_bounds(
    line: &Line,
    entry: &Entry,
    range: &Range<usize>,
    text: Bounds<Pixels>,
) -> Bounds<Pixels> {
    let (from, to) = line.entry_span(entry, &entry.ime_range(range));
    let left = text.left() - line.scroll;
    Bounds::from_corners(
        point(left + from, text.top()),
        point(left + to, text.bottom()),
    )
}

/// The UTF-16 offset nearest x, measured from where unscrolled text starts.
pub fn ime_index(line: &Line, entry: &Entry, x: Pixels) -> usize {
    entry.offset_to_utf16(entry.from_display(line.index_for(x + line.scroll)))
}

pub struct FieldElement {
    view: Entity<PathView>,
}

impl FieldElement {
    pub fn new(view: Entity<PathView>) -> Self {
        Self { view }
    }
}

impl IntoElement for FieldElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for FieldElement {
    type RequestLayoutState = ();
    type PrepaintState = (Hitbox, Ink);

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
    ) -> (Hitbox, Ink) {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let view = self.view.read(cx);
        let frame = frame(bounds);
        let mut paint = view.paint.borrow_mut();
        let scale = paint.scale();
        paint.area = Some(bounds);
        paint.frame = Some(frame);
        paint.field_origin = text_origin(frame, &scale);
        let pixels = paint.pixels(Size::Field);
        let font = paint.typeface.font.clone();
        let mut line = view.line.borrow_mut();
        if line.run.is_none() || line.revision != view.entry.revision {
            let text = &view.entry.text;
            let run = TextRun {
                len: text.len(),
                font,
                color: Default::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            line.run = Some(Rc::new(GlyphRun::new(window.text_system().layout_line(
                text,
                pixels,
                &[run],
                None,
            ))));
            line.revision = view.entry.revision;
        }
        // The caret stays inside the frame: the line slides under it the way a text input scrolls.
        let room = (frame.size.width - px(2. * (FRAME + scale.inset))).max(px(0.));
        let caret = line.x_for(view.entry.cursor());
        if caret - line.scroll > room {
            line.scroll = caret - room;
        } else if caret < line.scroll {
            line.scroll = caret;
        }
        let width = line.run.as_ref().map_or(px(0.), |run| run.line().width);
        line.scroll = line.scroll.min((width - room).max(px(0.))).max(px(0.));
        let ink = Ink::of(Palette::inherited(&window.text_style(), theme));
        (window.insert_hitbox(frame, HitboxBehavior::Normal), ink)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        (hitbox, ink): &mut (Hitbox, Ink),
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = self.view.read(cx);
        let focus = view.focus.clone();
        let frame = frame(bounds);
        let scale = view.paint.borrow().scale();
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        // The small field follows half of the compositor's corner radius.
        window.paint_quad(quad(
            frame,
            px(view.corner / 2.),
            ink.background,
            px(FRAME),
            ink.accent_frame,
            BorderStyle::Solid,
        ));
        let line = view.line.borrow();
        let origin = text_origin(frame, &scale) - line.scroll;
        let band = Bounds::new(
            point(frame.origin.x + px(FRAME), frame.origin.y),
            size(
                (frame.size.width - px(2. * FRAME)).max(px(0.)),
                frame.size.height,
            ),
        );
        let caret_top = band_top(frame, scale.caret);
        let entry = &view.entry;
        window.with_content_mask(Some(ContentMask { bounds: band }), |window| {
            if !entry.selected.is_empty() {
                let from = origin + line.x_for(entry.selected.start);
                let to = origin + line.x_for(entry.selected.end);
                window.paint_quad(fill(wash_box(frame, from, to, &scale), ink.wash(SELECTION)));
            }
            if let Some(run) = &line.run {
                paint_run(
                    run,
                    origin,
                    frame.origin.y,
                    frame.size.height,
                    ink.foreground,
                    window,
                );
            }
            if let Some(marked) = &entry.marked {
                let from = origin + line.x_for(marked.start);
                let to = origin + line.x_for(marked.end);
                window.paint_quad(fill(
                    Bounds::new(
                        point(from, caret_top + px(scale.caret - 1.)),
                        size(to - from, px(1.)),
                    ),
                    ink.foreground,
                ));
            }
            // Selected text shows no caret, as the board draws it.
            if entry.selected.is_empty() {
                window.paint_quad(fill(
                    caret_box(frame, origin, line.x_for(entry.cursor()), &scale),
                    ink.foreground,
                ));
            }
        });
        drop(line);
        window.handle_input(
            &focus,
            ElementInputHandler::new(frame, self.view.clone()),
            cx,
        );
    }
}

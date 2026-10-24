//! The open path field: the accent frame, the line on the window ground, its selection wash, marked text and caret.
use super::paint::{paint_run, Ink, Size};
use super::view::PathView;
use crate::list::Palette;
use crate::palette::Theme;
use gpui::{
    fill, point, px, quad, relative, size, App, BorderStyle, Bounds, ContentMask, CursorStyle,
    Element, ElementId, ElementInputHandler, Entity, GlobalElementId, GlyphRun, Hitbox,
    HitboxBehavior, InspectorElementId, IntoElement, LayoutId, Pixels, Style, TextRun, Window,
};
use std::rc::Rc;

// Board path-bar: the frame sits 2 px under the strip's top and 1 px over its rule, the text 9 px in, the caret 1 by 17.
pub const TOP: f32 = 2.;
pub const BOTTOM: f32 = 1.;
pub const INSET: f32 = 9.;
const CARET: f32 = 17.;
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

/// The line shaped at its last change; edits are unique strings, so it never enters the shared cache.
#[derive(Default)]
pub struct Line {
    pub revision: u64,
    pub run: Option<Rc<GlyphRun>>,
    pub scroll: Pixels,
}

impl Line {
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
        paint.area = Some(bounds);
        paint.frame = Some(frame);
        paint.field_origin = frame.origin.x + px(INSET);
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
        let room = (frame.size.width - px(2. * INSET)).max(px(0.));
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
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        // The small field follows half of the compositor's corner radius.
        window.paint_quad(quad(
            frame,
            px(view.corner / 2.),
            ink.background,
            px(1.),
            ink.accent_frame,
            BorderStyle::Solid,
        ));
        let line = view.line.borrow();
        let origin = frame.origin.x + px(INSET) - line.scroll;
        let band = Bounds::new(
            point(frame.origin.x + px(1.), frame.origin.y),
            size((frame.size.width - px(2.)).max(px(0.)), frame.size.height),
        );
        let mid = frame.origin.y + (frame.size.height - px(CARET)) / 2.;
        let entry = &view.entry;
        window.with_content_mask(Some(ContentMask { bounds: band }), |window| {
            if !entry.selected.is_empty() {
                let from = origin + line.x_for(entry.selected.start);
                let to = origin + line.x_for(entry.selected.end);
                window.paint_quad(fill(
                    Bounds::new(point(from, mid), size(to - from, px(CARET))),
                    ink.wash(SELECTION),
                ));
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
                    Bounds::new(point(from, mid + px(CARET - 1.)), size(to - from, px(1.))),
                    ink.foreground,
                ));
            }
            // Selected text shows no caret, as the board draws it.
            if entry.selected.is_empty() {
                let at = origin + line.x_for(entry.cursor());
                window.paint_quad(fill(
                    Bounds::new(point(at, mid), size(px(1.), px(CARET))),
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

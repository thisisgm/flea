//! Text input occupies only the existing destination slot; the card retains its own frame and metrics.
use flea::ui_logic_op1::card;
use gpui::{
    div, fill, point, prelude::*, px, relative, size, AlignItems, AnyElement, App, AvailableSpace,
    Bounds, ContentMask, CursorStyle, Div, Element, ElementId, ElementInputHandler, Entity,
    GlobalElementId, GlyphRun, Hitbox, HitboxBehavior, InspectorElementId, LayoutId, MouseButton,
    Pixels, Style, Window,
};
use std::rc::Rc;

pub(crate) fn mount<V: FieldView>(
    view: Entity<V>,
    field: Div,
    typeface: &crate::list::Typeface,
) -> crate::leaf::Painted<crate::locale_geometry::Unmirrored> {
    let down = view.downgrade();
    let moved = view.downgrade();
    // An editable line keeps its text, selection, caret and placeholder together in a mirrored window.
    crate::locale_geometry::unmirrored(
        field
            .id("op1-destination")
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                let _ = down.update(cx, |view, cx| view.field_press(event, window, cx));
            })
            .on_mouse_move(move |event, window, cx| {
                let _ = moved.update(cx, |view, cx| view.field_drag(event, window, cx));
            })
            .child(
                div()
                    .debug_selector(|| "op1-destination".into())
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(Destination(view, typeface.clone())),
            ),
    )
}

/// The slot a card mounts its field in, showing the placeholder while the line is empty.
pub(crate) fn slot(value: &str, placeholder: &str, muted: gpui::Hsla) -> Div {
    div().relative().flex_1().min_w_0().h_full().when(
        value.is_empty() && !placeholder.is_empty(),
        |slot| {
            slot.child(
                div()
                    .absolute()
                    .left(px(1.))
                    .top_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_color(muted)
                    .child(placeholder.to_owned()),
            )
        },
    )
}

use super::field_view::FieldView;

struct Destination<V: FieldView>(Entity<V>, crate::list::Typeface);

pub(super) struct Prepared {
    hitbox: Hitbox,
    text_top: Pixels,
    selection: Option<AnyElement>,
    caret: Option<AnyElement>,
    #[cfg(test)]
    selection_bounds: Option<Bounds<Pixels>>,
    #[cfg(test)]
    caret_bounds: Option<Bounds<Pixels>>,
}

/// The exact inputs and mask used by this Element's last real paint and platform registration.
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct Paint {
    pub bounds: Bounds<Pixels>,
    pub mask: Bounds<Pixels>,
    pub text: String,
    pub font: gpui::Font,
    pub pixels: Pixels,
    pub origin: Pixels,
    pub scroll: Pixels,
    pub selection: Option<Bounds<Pixels>>,
    pub caret: Option<Bounds<Pixels>>,
    pub marked: Option<Bounds<Pixels>>,
    pub input_bounds: Option<Bounds<Pixels>>,
}

// These are the painted selection/caret themselves, preserving the original debug selectors.
fn quad_element(
    name: &'static str,
    bounds: Bounds<Pixels>,
    ink: gpui::Hsla,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let mut element = div()
        .debug_selector(move || name.into())
        .w(bounds.size.width)
        .h(bounds.size.height)
        .bg(ink)
        .into_any_element();
    let _ = element.prepaint_as_root(
        bounds.origin,
        size(
            AvailableSpace::Definite(bounds.size.width),
            AvailableSpace::Definite(bounds.size.height),
        ),
        window,
        cx,
    );
    element
}

/// The glyph band is a measured typographic leaf, laid out and painted like a card Line.
pub(super) struct Text {
    leaf: LayoutId,
    height: Pixels,
}

impl<V: FieldView> IntoElement for Destination<V> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl<V: FieldView> Element for Destination<V> {
    type RequestLayoutState = Text;
    type PrepaintState = Prepared;

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
    ) -> (LayoutId, Text) {
        // GPUI rounds the measured leaf up and snaps its centred edge; paint keeps the fractional line.
        let height = window
            .text_style()
            .line_height
            .to_pixels(self.1.body.into(), window.rem_size());
        let leaf = window
            .request_measured_layout(Style::default(), move |_, _, _, _| size(px(0.), height));
        let style = Style {
            display: gpui::Display::Flex,
            size: size(relative(1.).into(), relative(1.).into()),
            align_items: Some(AlignItems::Center),
            ..Style::default()
        };
        (
            window.request_layout(style, [leaf], cx),
            Text { leaf, height },
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        text: &mut Text,
        window: &mut Window,
        cx: &mut App,
    ) -> Prepared {
        let (selection_bounds, caret_bounds, palette) = {
            let view = Entity::read(&self.0, cx);
            let entry = view.field_entry();
            let mut drawn = view.field_drawn().borrow_mut();
            drawn.bounds = Some(bounds);
            let style = window.text_style();
            let pixels = self.1.body;
            let mut run = style.to_run(entry.text.len());
            run.font = self.1.font.clone();
            if drawn.line.run.is_none()
                || drawn.line.revision != entry.revision
                || drawn.font.as_ref() != Some(&run.font)
                || drawn.pixels != pixels
                || drawn.revealed != view.field_revealed()
                || drawn.held != view.field_held()
            {
                drawn.font = Some(run.font.clone());
                drawn.pixels = pixels;
                // A private line shapes its bullets, and only when it reshapes, so paint allocates nothing.
                drawn.revealed = view.field_revealed();
                drawn.held = view.field_held();
                let shaped_text = Rc::from(super::field_view::display(view).as_ref());
                drawn.line.set_text_direction(&shaped_text);
                drawn.shaped_text = shaped_text;
                run.len = drawn.shaped_text.len();
                drawn.line.run = Some(Rc::new(GlyphRun::new(window.text_system().layout_line(
                    drawn.shaped_text.as_ref(),
                    pixels,
                    &[run],
                    None,
                ))));
                drawn.line.revision = entry.revision;
            }
            let line = &mut drawn.line;
            let caret = super::field_view::caret(view, line, entry.cursor());
            let room = bounds.size.width.max(px(0.));
            if caret - line.scroll > room {
                line.scroll = caret - room;
            } else if caret < line.scroll {
                line.scroll = caret;
            }
            let width = line.run.as_ref().map_or(px(0.), |run| run.line().width);
            line.scroll = line.scroll.min((width - room).max(px(0.))).max(px(0.));
            let active = view.field_active(window, cx);
            let metrics = super::geometry::Metrics::new(&self.1);
            let origin = bounds.left() - line.scroll;
            let selection_bounds = (active && !entry.selected.is_empty()).then(|| {
                let (from, to) = super::field_view::span(view, line, &entry.selected);
                Bounds::new(
                    point(
                        origin + from,
                        bounds.top() + (bounds.size.height - px(metrics.selection)) / 2.,
                    ),
                    size(to - from, px(metrics.selection)),
                )
            });
            let caret_bounds = (active && entry.selected.is_empty()).then(|| {
                Bounds::new(
                    point(
                        origin + caret,
                        bounds.top() + (bounds.size.height - px(metrics.caret)) / 2.,
                    ),
                    size(px(card::RULE), px(metrics.caret)),
                )
            });
            (selection_bounds, caret_bounds, view.field_palette())
        }; // Release the entity and its drawn-cache borrow before element layout/prepaint uses cx.
        let (selection, caret) = window.with_content_mask(Some(ContentMask { bounds }), |window| {
            (
                selection_bounds.map(|rect| {
                    quad_element(
                        "op1-selection",
                        rect,
                        palette.accent.opacity(card::FIELD_WASH),
                        window,
                        cx,
                    )
                }),
                caret_bounds
                    .map(|rect| quad_element("op1-caret", rect, palette.foreground, window, cx)),
            )
        });
        Prepared {
            hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
            text_top: window.layout_bounds(text.leaf).top(),
            selection,
            caret,
            #[cfg(test)]
            selection_bounds,
            #[cfg(test)]
            caret_bounds,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        text: &mut Text,
        prepared: &mut Prepared,
        window: &mut Window,
        cx: &mut App,
    ) {
        let metrics = super::geometry::Metrics::new(&self.1);
        let (run, origin, active, palette, marked, focus, session) = {
            let view = Entity::read(&self.0, cx);
            let entry = view.field_entry();
            let drawn = view.field_drawn().borrow();
            let line = &drawn.line;
            let origin = bounds.left() - line.scroll;
            let active = view.field_active(window, cx);
            let mid = bounds.top() + (bounds.size.height - px(metrics.selection)) / 2.;
            let marked = entry.marked.as_ref().filter(|_| active).map(|marked| {
                let (from, to) = super::field_view::span(view, line, marked);
                Bounds::new(
                    point(origin + from, mid + px(metrics.selection - card::RULE)),
                    size(to - from, px(card::RULE)),
                )
            });
            (
                line.run.clone(),
                origin,
                active,
                view.field_palette(),
                marked,
                view.field_focus(),
                entry.session(),
            )
        }; // The actual paint and platform registration below hold no entity or RefCell read.
        let text_ink = Entity::read(&self.0, cx).field_ink();
        window.set_cursor_style(CursorStyle::IBeam, &prepared.hitbox);
        #[cfg(test)]
        let mut painted_mask = bounds;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = &mut prepared.selection {
                selection.paint(window, cx);
            }
            if let Some(run) = &run {
                crate::path::paint::paint_run(
                    run,
                    origin,
                    prepared.text_top,
                    text.height,
                    text_ink,
                    window,
                );
            }
            if active {
                // The preedit underline tells composing text from the committed draft.
                if let Some(marked) = marked {
                    window.paint_quad(fill(marked, palette.foreground));
                }
                if let Some(caret) = &mut prepared.caret {
                    caret.paint(window, cx);
                }
            }
            #[cfg(test)]
            {
                painted_mask = window.content_mask().bounds;
            }
        });
        #[cfg(test)]
        let mut input_bounds = None;
        let mut registered = None;
        if active {
            if let Some(focus) = &focus {
                window.handle_input(focus, ElementInputHandler::new(bounds, self.0.clone()), cx);
                registered = session;
                #[cfg(test)]
                {
                    input_bounds = Some(bounds);
                }
            }
        }
        // Platform callbacks reach this handler until the next paint, so they carry this session only.
        Entity::read(&self.0, cx)
            .field_drawn()
            .borrow_mut()
            .line
            .registered = registered;
        #[cfg(test)]
        {
            let view = Entity::read(&self.0, cx);
            let drawn = view.field_drawn().borrow();
            let paint = Paint {
                bounds,
                mask: painted_mask,
                text: drawn.shaped_text.to_string(),
                font: self.1.font.clone(),
                pixels: self.1.body,
                origin,
                scroll: drawn.line.scroll,
                selection: prepared.selection_bounds,
                caret: prepared.caret_bounds,
                marked,
                input_bounds,
            };
            drop(drawn);
            view.field_drawn().borrow_mut().paint = Some(paint);
        }
    }
}

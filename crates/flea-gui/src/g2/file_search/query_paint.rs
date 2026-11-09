//! The query's selection, preedit and input-method rectangle share its shaped line.
use super::*;
use crate::path::field::Line;
use crate::strips::query_input::{place, SELECTION};
use gpui::{ContentMask, ElementInputHandler};
use std::cell::Cell;

#[derive(Default)]
pub struct QueryPaint {
    pub line: Rc<RefCell<Line>>,
    pub bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub input_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub caret: Option<Bounds<Pixels>>,
    pub marked: Option<Bounds<Pixels>>,
    pub selection: Option<Bounds<Pixels>>,
}

impl StripElement {
    pub(super) fn prepaint_query(&self, bounds: Bounds<Pixels>, cx: &App) {
        let Some((input, _)) = &self.input else {
            return;
        };
        let input = input.read(cx);
        let entry = input.entry.borrow();
        let mut state = self.state.borrow_mut();
        let Some(prepared) = &state.prepared else {
            return;
        };
        let boxes = prepared.boxes;
        let run = prepared.query.clone();
        let height = crate::list::Metrics::for_typeface(&state.typeface).header;
        let scale = height / HEIGHT;
        let draw = &mut state.edit;
        let mut line = draw.line.borrow_mut();
        line.run = Some(run);
        line.right_to_left = crate::path::entry::right_to_left(&entry.text);
        let text = Bounds::new(
            point(bounds.left() + px(boxes.query_x), bounds.top()),
            size(
                px((boxes.way_x - GAP * scale - boxes.query_x).max(0.)),
                px(height),
            ),
        );
        let band = Bounds::new(
            point(
                text.left(),
                bounds.top() + px((height - CARET_H * scale) / 2.),
            ),
            size(text.size.width, px(CARET_H * scale)),
        );
        let placed = place(
            &mut line,
            &entry,
            band,
            px(CARET_W * scale),
            px(CARET_GAP * scale),
        );
        draw.bounds.set(Some(text));
        draw.input_bounds.set(Some(band));
        draw.caret = placed.caret;
        draw.marked = placed.marked;
        draw.selection = placed.selection;
    }

    /// The edited query's drawn extent, from the field's start to its last glyph, preedit or caret, clipped to the field.
    pub(super) fn query_unit(&self, window: &Window, cx: &App) -> Option<Bounds<Pixels>> {
        let (input, _) = self.input.as_ref()?;
        let state = self.state.borrow();
        let draw = &state.edit;
        let bounds = draw.bounds.get()?;
        let line = draw.line.borrow();
        let mut right =
            bounds.left() + line.run.as_ref().map_or(px(0.), |run| run.line().width) - line.scroll;
        if input.read(cx).focused(window, cx) {
            for edge in [draw.caret, draw.marked, draw.selection]
                .into_iter()
                .flatten()
            {
                right = right.max(edge.right());
            }
        }
        let width = (right.min(bounds.right()) - bounds.left()).max(px(0.));
        Some(Bounds::new(bounds.origin, size(width, bounds.size.height)))
    }

    pub(super) fn paint_query(&self, window: &mut Window, cx: &mut App) -> bool {
        let Some((input, focus)) = &self.input else {
            return false;
        };
        let input_ref = input.read(cx);
        let active = input_ref.focused(window, cx);
        let session = input_ref.entry.borrow().session();
        let state = self.state.borrow();
        let draw = &state.edit;
        let Some(bounds) = draw.bounds.get() else {
            return false;
        };
        let palette = self.palette;
        let height = crate::list::Metrics::for_typeface(&state.typeface).header;
        let line = draw.line.borrow();
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if active {
                if let Some(selection) = draw.selection {
                    window.paint_quad(fill(selection, palette.accent.opacity(SELECTION)));
                }
            }
            if let Some(run) = &line.run {
                paint_line(
                    run,
                    bounds.left() - line.scroll,
                    bounds.top(),
                    palette.foreground,
                    height,
                    window,
                );
            }
            if active {
                if let Some(marked) = draw.marked {
                    window.paint_quad(fill(marked, palette.foreground));
                }
                if let Some(caret) = draw.caret {
                    window.paint_quad(fill(caret, palette.accent));
                }
            }
        });
        drop(line);
        draw.line.borrow_mut().registered = if active { session } else { None };
        if active {
            window.handle_input(focus, ElementInputHandler::new(bounds, input.clone()), cx);
            crate::strips::query_input::on_press(input.clone(), bounds, window);
        }
        true
    }
}

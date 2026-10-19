use super::{number, object};
use crate::list::{self, pane::ROW_HEIGHT, ListState};
use flea::jsondoc::Json;
use gpui::{canvas, point, px, size, Bounds, Div, Pixels, SharedString, Styled, Window};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Default)]
pub(crate) struct Probe(Rc<RefCell<Vec<Json>>>);

impl Probe {
    pub fn empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    pub fn push(
        &self,
        id: &str,
        parent: &str,
        role: &str,
        bounds: Bounds<Pixels>,
        text: &str,
        window: &Window,
    ) {
        let values = |b: Bounds<Pixels>| {
            Json::Arr(
                [
                    f32::from(b.origin.x),
                    f32::from(b.origin.y),
                    f32::from(b.size.width),
                    f32::from(b.size.height),
                ]
                .into_iter()
                .map(number)
                .collect(),
            )
        };
        let viewport = Bounds::new(point(px(0.), px(0.)), window.viewport_size());
        let clipped = bounds.intersect(&viewport);
        if clipped.size.width <= px(0.) || clipped.size.height <= px(0.) {
            return;
        }
        self.0.borrow_mut().push(object([
            ("id", Json::Str(id.into())),
            ("parent", Json::Str(parent.into())),
            ("role", Json::Str(role.into())),
            ("rect", values(bounds)),
            ("pixel_rect", values(clipped)),
            ("text", Json::Str(text.into())),
            ("scale", number(window.scale_factor())),
        ]));
    }

    pub fn json(&self, receipt: &Json) -> Json {
        object([
            ("receipt", receipt.clone()),
            ("roles", Json::Arr(self.0.borrow().clone())),
        ])
    }
}

pub(crate) fn surface(div: Div, id: impl Into<String>, probe: Option<Probe>) -> Div {
    let Some(probe) = probe else { return div };
    let id = id.into();
    overlay(div, move |bounds, window| {
        probe.push(&id, "", "surface", bounds, "", window)
    })
}

/// Records the text a host div draws as a label role, measured with the div's inherited text style.
pub(crate) fn label(
    div: Div,
    id: String,
    parent: String,
    text: SharedString,
    inset: f32,
    right: bool,
    probe: Option<Probe>,
) -> Div {
    let Some(probe) = probe else { return div };
    overlay(div, move |bounds, window| {
        let style = window.text_style();
        let rem = window.rem_size();
        let layout = window.text_system().layout_line(
            &text,
            style.font_size.to_pixels(rem),
            &[style.to_run(text.len())],
            None,
        );
        let height = layout.ascent + layout.descent;
        let x = if right {
            bounds.right() - px(inset) - layout.width
        } else {
            bounds.left() + px(inset)
        };
        let y = bounds.top() + (style.line_height_in_pixels(rem) - height) / 2.;
        let rect = Bounds::new(point(x, y), size(layout.width, height));
        probe.push(&id, &parent, "label", rect, &text, window);
    })
}

fn overlay(div: Div, record: impl FnOnce(Bounds<Pixels>, &Window) + 'static) -> Div {
    use gpui::ParentElement;
    div.child(
        canvas(
            move |bounds, window, _| record(bounds, window),
            |_, _, _, _| {},
        )
        .absolute()
        .left(px(0.))
        .top(px(0.))
        .size_full(),
    )
}

pub(crate) fn rows(state: &ListState, bounds: Bounds<Pixels>, scroll: f32, window: &Window) {
    let Some(probe) = &state.probe else { return };
    let surface = format!("list-{}", f32::from(bounds.origin.x));
    probe.push(&surface, "", "surface", bounds, "", window);
    for (index, prepared) in &state.visible {
        let Some(prepared) = prepared else { continue };
        let top = bounds.origin.y + px(*index as f32 * ROW_HEIGHT - scroll);
        let id = format!("{surface}-row-{index}");
        let rect = Bounds::new(
            point(bounds.origin.x, top),
            size(bounds.size.width, px(ROW_HEIGHT)),
        );
        probe.push(&id, &surface, "row", rect, "", window);
        let rect_at = |x, y, width, height| {
            Bounds::new(
                point(bounds.origin.x + px(x), top + px(y)),
                size(px(width), px(height)),
            )
        };
        probe.push(
            &format!("{id}-glyph"),
            &id,
            "glyph",
            rect_at(list::PAD_X, 0., list::MARK_SLOT, ROW_HEIGHT),
            "",
            window,
        );
        probe.push(
            &format!("{id}-ink"),
            &format!("{id}-glyph"),
            "ink",
            list::mark_bounds(bounds.origin.x, top),
            "",
            window,
        );
        for (role, x, line, text) in [
            (
                "label",
                list::LABEL_X,
                &prepared.name,
                prepared.label.as_str(),
            ),
            (
                "value",
                f32::from(bounds.size.width)
                    - (list::END_W + list::DATE_W)
                    - f32::from(prepared.size.line().width),
                &prepared.size,
                prepared.size_text.as_str(),
            ),
            (
                "value",
                f32::from(bounds.size.width) - list::END_W - f32::from(prepared.date.line().width),
                &prepared.date,
                prepared.date_text.as_str(),
            ),
        ] {
            let text_bounds = list::line_bounds(line, bounds.origin.x + px(x), top, window);
            let key = format!("{id}-{role}-{x}");
            probe.push(&key, &id, role, text_bounds, text, window);
        }
    }
}

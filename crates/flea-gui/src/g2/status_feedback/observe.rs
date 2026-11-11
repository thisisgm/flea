//! Read-only observations written by the elements that paint the feedback surfaces.
use gpui::{canvas, prelude::*, px, Bounds, Hsla, Pixels, SharedString, Window};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug, Default)]
pub struct Text {
    pub bounds: Bounds<Pixels>,
    pub text: SharedString,
    pub implicit_width: f32,
    pub color: Hsla,
    pub font_size: f32,
}

#[derive(Clone, Default)]
pub struct Paint(Rc<RefCell<[Option<Text>; SLOTS.len()]>>);

/// Every painted part has one slot, so the store is exactly as long as this list.
const SLOTS: &[&str] = &[
    "frame",
    "mark",
    "caption",
    "counts",
    "disk",
    "lane",
    "primary",
    "secondary",
    "hint",
    "extra",
    "undo",
    "undo-dot",
    "cancel",
    "headline",
    "byte-line",
    "source",
    "dismiss",
];

fn slot(name: &str) -> Option<usize> {
    SLOTS.iter().position(|part| *part == name)
}

impl Paint {
    pub fn clear(&self) {
        for part in self.0.borrow_mut().iter_mut() {
            *part = None;
        }
    }

    pub fn get(&self, name: &str) -> Option<Text> {
        self.0.borrow()[slot(name)?].clone()
    }

    /// Records what a painter outside the element tree drew, as a watched element would.
    pub fn set(&self, name: &str, text: Text) {
        if let Some(index) = slot(name) {
            self.0.borrow_mut()[index] = Some(text);
        }
    }

    pub fn watch<E: ParentElement>(&self, div: E, name: &'static str, text: SharedString) -> E {
        self.watch_with(div, name, move |bounds, window| {
            let style = window.text_style();
            let font_size = style.font_size.to_pixels(window.rem_size());
            let layout = window.text_system().layout_line(
                &text,
                font_size,
                &[style.to_run(text.len())],
                None,
            );
            Text {
                bounds,
                text,
                implicit_width: f32::from(layout.width),
                color: style.color,
                font_size: f32::from(font_size),
            }
        })
    }

    fn watch_with<E: ParentElement>(
        &self,
        div: E,
        name: &'static str,
        observe: impl FnOnce(Bounds<Pixels>, &Window) -> Text + 'static,
    ) -> E {
        let paint = self.clone();
        let index = slot(name).expect("a feedback paint slot");
        div.child(
            canvas(
                move |bounds, window, _| observe(bounds, window),
                move |_, state, _, _| {
                    paint.0.borrow_mut()[index] = Some(state);
                },
            )
            .absolute()
            .left(px(0.))
            .top(px(0.))
            .size_full(),
        )
    }
}

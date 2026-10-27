//! Geometry recorded when the conversion card paints, retained for its read-only IPC surface.
use gpui::{canvas, prelude::*, Bounds, Div, Pixels};
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
pub struct Observations {
    pub card: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub title: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Observations {
    pub(super) fn card(&self, element: Div) -> Div {
        observe(element, self.card.clone())
    }

    pub(super) fn title(&self, element: Div) -> Div {
        observe(element, self.title.clone())
    }
}

fn observe(element: Div, bounds: Rc<Cell<Option<Bounds<Pixels>>>>) -> Div {
    element.relative().child(
        canvas(
            |_, _, _| {},
            move |painted, _, _, _| bounds.set(Some(painted)),
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full(),
    )
}

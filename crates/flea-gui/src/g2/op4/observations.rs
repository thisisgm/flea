//! Geometry recorded when the conversion card paints, retained for its read-only IPC surface.
use gpui::{canvas, prelude::*, px, Bounds, Div, Pixels};
use std::{cell::Cell, rc::Rc};

/// The card's `border_1()` width, painted outside its absolute child's padding box.
const CARD_BORDER: f32 = 1.;

#[derive(Default)]
pub struct Observations {
    pub card: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub title: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Observations {
    pub(super) fn card(&self, element: Div) -> Div {
        observe(element, self.card.clone(), CARD_BORDER)
    }

    pub(super) fn title(&self, element: Div) -> Div {
        observe(element, self.title.clone(), 0.)
    }
}

fn observe(element: Div, bounds: Rc<Cell<Option<Bounds<Pixels>>>>, border: f32) -> Div {
    element.relative().child(
        canvas(
            |_, _, _| {},
            move |painted, _, _, _| bounds.set(Some(painted)),
        )
        .absolute()
        .top(px(-border))
        .left(px(-border))
        .right(px(-border))
        .bottom(px(-border)),
    )
}

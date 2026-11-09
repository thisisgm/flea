//! Geometry recorded when the conversion card paints, retained for its read-only IPC surface.
use gpui::{canvas, prelude::*, px, Bounds, Div, Pixels};
use std::{cell::Cell, rc::Rc};

/// The card's `border_1()` width, painted outside its absolute child's padding box.
const CARD_BORDER: f32 = 1.;

#[derive(Default)]
pub struct Observations {
    pub card: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub title: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub convert_card: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub convert_title: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Observations {
    pub(super) fn card(&self, element: Div) -> Div {
        observe(element, self.card.clone(), CARD_BORDER)
    }

    pub(super) fn title(&self, element: Div) -> Div {
        observe(element, self.title.clone(), 0.)
    }

    pub(super) fn retain_convert(&self, element: Div) -> Div {
        let (card, title) = (self.card.clone(), self.title.clone());
        let (retained_card, retained_title) =
            (self.convert_card.clone(), self.convert_title.clone());
        element.child(
            canvas(
                |_, _, _| (),
                move |_, _, _, _| {
                    retained_card.set(card.get());
                    retained_title.set(title.get());
                },
            )
            .absolute()
            .size_full(),
        )
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

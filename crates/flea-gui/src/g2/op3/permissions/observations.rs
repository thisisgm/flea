//! IPC coordinates come from the card and control bounds recorded during paint.
use crate::{g2::op3::shell::Control, ipc::rows};
use flea::jsondoc::Json;
use gpui::{canvas, div, prelude::*, Bounds, Div, Pixels, Stateful};
use std::{cell::Cell, rc::Rc};

type Painted = Rc<Cell<Option<Bounds<Pixels>>>>;

#[derive(Clone, Default)]
pub(crate) struct Observations {
    card: Painted,
    controls: [Painted; 12],
}

fn index(control: Control) -> Option<usize> {
    match control {
        Control::Check(at) => Some(at),
        Control::Field => Some(9),
        Control::Button(at) => Some(10 + at),
        // Open With's search and application rows never draw on this card.
        Control::Clear | Control::Applications | Control::Application(_) => None,
    }
}

fn observe(element: Stateful<Div>, painted: Painted) -> Stateful<Div> {
    element.relative().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, _, _| painted.set(Some(bounds)),
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full(),
    )
}

impl Observations {
    pub fn clear(&self) {
        self.card.set(None);
        for bounds in &self.controls {
            bounds.set(None);
        }
    }

    pub fn frame(&self, element: Stateful<Div>) -> Stateful<Div> {
        // The unframed wrapper measures the card's outer border as well as its contents.
        observe(
            div().id("permissions-geometry").child(element),
            self.card.clone(),
        )
    }

    pub fn control(&self, control: Control, element: Stateful<Div>) -> Stateful<Div> {
        match index(control).and_then(|at| self.controls.get(at)) {
            Some(bounds) => observe(element, bounds.clone()),
            None => element,
        }
    }

    pub fn rect(&self) -> Json {
        self.card.get().map_or(Json::Null, |bounds| {
            Json::Str(rows::rect_of(
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            ))
        })
    }

    pub fn centre(&self, control: Control) -> Json {
        index(control)
            .and_then(|at| self.controls.get(at))
            .and_then(|bounds| bounds.get())
            .map_or(Json::Null, |bounds| {
                Json::Str(rows::centre_of_bounds(bounds))
            })
    }
}

//! Where the unlock card painted and what the pointer holds on its buttons, kept for unlockState.
use gpui::{canvas, prelude::*, Bounds, Pixels};
use std::{cell::Cell, rc::Rc};

pub type Painted = Rc<Cell<Option<Bounds<Pixels>>>>;

/// One button's painted bounds and its pointer feedback, hover while over it and press while held inside it.
#[derive(Clone, Default)]
pub struct ButtonProbe {
    pub painted: Painted,
    pub hovered: Rc<Cell<bool>>,
    pub pressed: Rc<Cell<bool>>,
}

#[derive(Clone, Default)]
pub struct Probes {
    pub card: Painted,
    pub eye: Painted,
    pub cancel: ButtonProbe,
    pub unlock: ButtonProbe,
}

impl Probes {
    /// A closed card has no painted geometry, so a stale rect never outlives it.
    pub fn clear(&self) {
        for painted in [
            &self.card,
            &self.eye,
            &self.cancel.painted,
            &self.unlock.painted,
        ] {
            painted.set(None);
        }
        for held in [&self.cancel, &self.unlock] {
            held.hovered.set(false);
            held.pressed.set(false);
        }
    }
}

/// Records the element's painted bounds without changing its layout.
pub fn observe<E: ParentElement + Styled>(element: E, painted: Option<Painted>) -> E {
    match painted {
        Some(painted) => element.relative().child(
            canvas(
                |_, _, _| {},
                move |bounds, _, _, _| painted.set(Some(bounds)),
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        ),
        None => element,
    }
}

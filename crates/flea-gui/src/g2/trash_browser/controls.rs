//! Laid-out bounds of the Trash strip's controls, read by the IPC centre readers.
use gpui::{canvas, prelude::*, App, Bounds, IntoElement, Pixels, Window};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub type Controls = Rc<RefCell<BTreeMap<&'static str, Bounds<Pixels>>>>;

/// An overlay child that records its parent's bounds under name at prepaint, as op1's measured frame does.
pub fn track(controls: &Controls, name: &'static str) -> impl IntoElement {
    let controls = controls.clone();
    canvas(
        move |bounds: Bounds<Pixels>, _: &mut Window, _: &mut App| {
            controls.borrow_mut().insert(name, bounds);
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

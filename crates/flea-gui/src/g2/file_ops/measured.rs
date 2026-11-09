//! Measurements come from the card's laid-out elements, never from the IPC caller.
use gpui::{canvas, prelude::*, px, Bounds, Pixels, Window};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Clone, Default)]
pub struct Frame(Rc<RefCell<BTreeMap<String, Part>>>);

#[derive(Clone)]
pub struct Part {
    pub bounds: Bounds<Pixels>,
    pub text: String,
    pub text_width: Pixels,
    pub line_height: Pixels,
}

impl Frame {
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
    pub fn part(&self, name: &str) -> Option<Part> {
        self.0.borrow().get(name).cloned()
    }

    pub fn track<D: ParentElement>(&self, div: D, name: String, text: String) -> D {
        let frame = self.clone();
        div.child(
            canvas(
                move |bounds, window, _| frame.record(name, bounds, text, window),
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }

    fn record(&self, name: String, bounds: Bounds<Pixels>, text: String, window: &Window) {
        let style = window.text_style();
        let line = window.text_system().layout_line(
            &text,
            style.font_size.to_pixels(window.rem_size()),
            &[style.to_run(text.len())],
            None,
        );
        self.0.borrow_mut().insert(
            name,
            Part {
                bounds,
                text,
                text_width: line.width,
                line_height: style.line_height_in_pixels(window.rem_size()),
            },
        );
    }
}

pub fn rect(part: Option<&Part>) -> String {
    part.map(|part| {
        let b = part.bounds;
        [b.origin.x, b.origin.y, b.size.width, b.size.height]
            .map(|value| f32::from(value).round().to_string())
            .join(" ")
    })
    .unwrap_or_default()
}

pub fn centre(part: Option<&Part>) -> String {
    part.map(|part| {
        let b = part.bounds;
        format!(
            "{} {}",
            f32::from(b.origin.x + b.size.width / 2.).round(),
            f32::from(b.origin.y + b.size.height / 2.).round()
        )
    })
    .unwrap_or_default()
}

pub fn fits(part: &Part, container: &Part) -> bool {
    let (b, outer) = (part.bounds, container.bounds);
    b.size.width > px(0.)
        && b.size.height > px(0.)
        && b.left() >= outer.left()
        && b.right() <= outer.right()
        && b.top() >= outer.top()
        && b.bottom() <= outer.bottom()
}

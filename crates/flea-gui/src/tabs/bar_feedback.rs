use super::{Frame, Labels, StripElement};
use gpui::{fill, point, px, size, Bounds, Hsla, Pixels, Window};
use std::{cell::RefCell, rc::Rc};

impl StripElement {
    pub fn new(frame: Frame, labels: Rc<RefCell<Labels>>) -> Self {
        Self {
            frame,
            labels,
            feedback: None,
            controls: None,
        }
    }

    pub(crate) fn with_controls(
        mut self,
        controls: Option<crate::tabs::accessibility::Controls>,
    ) -> Self {
        self.controls = controls;
        self
    }

    pub(crate) fn with_feedback(
        mut self,
        frames: Rc<RefCell<crate::tabs::held::StripFrames>>,
    ) -> Self {
        self.feedback = Some(frames);
        self
    }
}

pub(super) fn frame_box(window: &mut Window, bounds: Bounds<Pixels>, ink: Hsla) {
    let (x, y, w, h) = (
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width,
        bounds.size.height,
    );
    let edge = px(1.);
    window.paint_quad(fill(Bounds::new(point(x, y), size(w, edge)), ink));
    window.paint_quad(fill(
        Bounds::new(point(x, y + h - edge), size(w, edge)),
        ink,
    ));
    window.paint_quad(fill(Bounds::new(point(x, y), size(edge, h)), ink));
    window.paint_quad(fill(
        Bounds::new(point(x + w - edge, y), size(edge, h)),
        ink,
    ));
}

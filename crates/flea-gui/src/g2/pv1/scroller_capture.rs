//! Passive readback of the vertical lane used by the document scrollers.
use super::{index, Axis, Scroll};
use gpui::{Bounds, Pixels};

pub(crate) struct Lane {
    pub bounds: Bounds<Pixels>,
    pub shown: bool,
    pub visible: bool,
}

impl Scroll {
    pub(crate) fn vertical_lane(&self, shared_corner: bool) -> Option<Lane> {
        let view = self.view?;
        let axis = Axis::Vertical;
        let lane = view.lane(axis);
        let visible = view.scrolls(axis);
        Some(Lane {
            bounds: Bounds::new(view.bounds.origin + lane.origin, lane.size),
            visible,
            shown: flea::ui_logic_pv1::scroll::revealed(
                visible,
                self.moving[index(axis)],
                self.lane == Some(axis) || (shared_corner && self.corner),
                self.drag.is_some_and(|(held, _)| held == axis),
            ),
        })
    }
}

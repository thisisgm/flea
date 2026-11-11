//! Passive readback of the vertical lane used by the document scrollers.
use super::{index, Axis, Scroll};
use gpui::{Bounds, Pixels};

#[derive(Clone, Copy)]
pub(crate) struct Painted {
    pub lane: Bounds<Pixels>,
    pub knob: Bounds<Pixels>,
    pub hovered: bool,
    pub pressed: bool,
}

impl Painted {
    pub(crate) fn control(self, surface: &str) -> Option<flea::jsondoc::Json> {
        use flea::jsondoc::Json;
        let b = self.lane;
        let centre = self.knob.center();
        let source = crate::visual::object([
            ("name", Json::Str("Vertical scroll bar".into())),
            ("value", Json::Str("vertical".into())),
            (
                "rect",
                Json::Str(crate::ipc::rows::rect_of(
                    b.origin.x.into(),
                    b.origin.y.into(),
                    b.size.width.into(),
                    b.size.height.into(),
                )),
            ),
            (
                "centre",
                Json::Str(format!(
                    "{} {}",
                    f32::from(centre.x).round(),
                    f32::from(centre.y).round()
                )),
            ),
            ("visible", Json::Bool(true)),
            ("enabled", Json::Bool(true)),
            ("hovered", Json::Bool(self.hovered)),
            ("pressed", Json::Bool(self.pressed)),
        ]);
        crate::ipc::cohesion_controls::control(&source, surface, "ViewportScrollBar")
    }
}

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
            shown: flea::ui_logic_preview_core::scroll::revealed(
                visible,
                self.moving[index(axis)],
                self.lane == Some(axis) || (shared_corner && self.corner),
                self.drag.is_some_and(|(held, _)| held == axis),
            ),
        })
    }
}

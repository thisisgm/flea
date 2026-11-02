//! The frame recipe recorded by its painter and the currently mounted HTML viewport's lane.
use super::HtmlView;
use crate::g2::pv1::scroller::capture::Lane;
use gpui::{canvas, prelude::*, AnyElement, App, Entity, Hsla};

#[derive(Clone, Copy)]
pub(crate) struct FrameStyle {
    pub hairline: f32,
    pub ring: Hsla,
    pub canvas: Hsla,
}

pub(crate) fn frame(
    view: Option<Entity<HtmlView>>,
    hairline: f32,
    ring: Hsla,
    canvas_ink: Hsla,
) -> Option<AnyElement> {
    let view = view?;
    let style = FrameStyle {
        hairline,
        ring,
        canvas: canvas_ink,
    };
    Some(
        canvas(
            |_, _, _| (),
            move |_, _, _, cx| view.read(cx).frame_style.set(Some(style)),
        )
        .absolute()
        .size_full()
        .into_any_element(),
    )
}

impl HtmlView {
    pub(crate) fn vertical_lane(&self, cx: &App) -> Option<Lane> {
        if self.doc.source_view() {
            self.source.as_ref()?.read(cx).vertical_lane()
        } else {
            self.bars.vertical_lane()
        }
    }
}

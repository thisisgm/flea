//! The frame recipe recorded by its painter and the currently mounted HTML viewport's lane.
use super::HtmlView;
use crate::{
    g2::pv1::scroller::capture::Lane,
    window::selection_preview::{docs::Doc, Frame, Mounted},
};
use gpui::{canvas, prelude::*, px, AnyElement, App, Entity, Hsla};

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
            move |bounds, _, _, cx| {
                let view = view.read(cx);
                view.frame_style.set(Some(style));
                // The absolute child fills the padding box inside the owner's painted border.
                view.frame_bounds.set(Some(bounds.dilate(px(hairline))));
            },
        )
        .absolute()
        .size_full()
        .into_any_element(),
    )
}

/// The column's passive HTML frame child; other mounted kinds create no capture.
pub(crate) fn column(column: &Frame, hairline: f32, ring: Hsla) -> Option<AnyElement> {
    let view = match &column.mounted {
        Mounted::Doc(Doc::Html(view)) => Some(view.clone()),
        _ => None,
    };
    frame(view, hairline, ring, Hsla::transparent_black())
}

impl HtmlView {
    pub(crate) fn viewport_metrics(&self, cx: &App) -> (f32, f32, f32) {
        if self.doc.source_view() {
            self.source
                .as_ref()
                .map_or((0., 0., 0.), |source| source.read(cx).viewport_metrics())
        } else {
            (self.scroll_y, self.frame.1, self.page_size().1 as f32)
        }
    }

    pub(crate) fn vertical_lane(&self, cx: &App) -> Option<Lane> {
        if self.doc.source_view() {
            self.source.as_ref()?.read(cx).vertical_lane()
        } else {
            self.bars.vertical_lane()
        }
    }
}

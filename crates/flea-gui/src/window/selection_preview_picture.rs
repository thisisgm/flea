//! An animated GIF's moving layer, fitted inside the frame's border; the still is fitted::fitted.
use super::{Frame, Kind};
use flea::ui_logic_pv1::{pixels::is_vector_name, thumbs};
use gpui::{div, img, prelude::*, px, AnyElement, IntoElement, RenderImage};
use std::sync::Arc;

use super::render::FRAME_BORDER;

fn fitted(image: Arc<RenderImage>, frame: &Frame, width: f32, height: f32, selector: &'static str) -> AnyElement {
    let size = image.size(0);
    let (w, h) = (size.width.0 as f64, size.height.0 as f64);
    let original = frame.facts.as_ref().map_or((0., 0.), |m| (m.w as f64, m.h as f64));
    let limit = thumbs::preview_limit(
        frame.kind == Kind::Video,
        is_vector_name(&frame.subject.row.name),
        w,
        h,
        original.0,
        original.1,
    );
    let scale = thumbs::fit_scale(
        (width - 2. * FRAME_BORDER).max(1.) as f64,
        (height - 2. * FRAME_BORDER).max(1.) as f64,
        w,
        h,
        limit,
    ) as f32;
    div()
        .debug_selector(move || selector.into())
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            img(image)
                .w(px((w as f32 * scale).round()))
                .h(px((h as f32 * scale).round()))
                .object_fit(gpui::ObjectFit::Fill),
        )
        .into_any_element()
}

/// An animated GIF's moving layer over its still, ready only with more than one frame of its own.
pub(crate) fn moving(frame: &Frame, width: f32, height: f32) -> Option<AnyElement> {
    frame.animation.clone().and_then(|image| {
        (image.frame_count() > 1)
            .then(|| fitted(image, frame, width, height, "column-preview-animation"))
    })
}

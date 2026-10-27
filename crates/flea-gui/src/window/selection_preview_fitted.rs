//! The column preview's poster or still, fitted inside the frame's border.
use super::{render::FRAME_BORDER, Frame, Kind};
use crate::g2::pv3::view::Picture;
use flea::ui_logic_pv1::{pixels::is_vector_name, thumbs};
use gpui::{div, img, prelude::*, px};

/// The poster or still fits inside the border; vectors fill the room and rasters keep their pixel limit.
pub(super) fn fitted(frame: &Frame, width: f32, height: f32) -> Option<gpui::Div> {
    frame.image.clone().map(|image| {
        let size = image.size(0);
        let (w, h) = (size.width.0 as f64, size.height.0 as f64);
        let original = frame
            .facts
            .as_ref()
            .map_or((0., 0.), |m| (m.w as f64, m.h as f64));
        let vector = is_vector_name(&frame.subject.row.name);
        let limit = thumbs::preview_limit(
            frame.kind == Kind::Video,
            vector,
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
            .debug_selector(|| "column-preview-picture".into())
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                img(image)
                    .debug_selector(|| "column-preview-image".into())
                    .w(px((w as f32 * scale).round()))
                    .h(px((h as f32 * scale).round()))
                    .object_fit(gpui::ObjectFit::Fill),
            )
    })
}

/// cols-preview-video.html draws no play mark over the frame; the strip under it carries play.
pub(crate) fn column_picture(picture: Picture) -> Picture {
    match picture {
        Picture::Video { frame, .. } => Picture::Video {
            frame,
            poster: false,
        },
        other => other,
    }
}

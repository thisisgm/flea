//! The column preview's poster or still, fitted inside the frame's border.
use super::{render::FRAME_BORDER, Frame, Kind};
use crate::g2::pv3::{player::MediaPlayer, surface, view::Picture};
use flea::i18n::I18n;
use flea::ui_logic_pv1::{pixels::is_vector_name, thumbs};
use flea::ui_logic_pv3::player::{Media, Status};
use gpui::{div, img, prelude::*, px, RenderImage};
use std::sync::Arc;

/// The poster or still fits inside the border; vectors fill the room and rasters keep their pixel limit.
pub(super) fn fitted(frame: &Frame, width: f32, height: f32, device: f32) -> Option<gpui::Div> {
    frame.image.clone().map(|image| {
        let size = image.size(0);
        let (logical_w, logical_h) = drawn(
            frame,
            (size.width.0 as f64, size.height.0 as f64),
            width,
            height,
            device,
        );
        div()
            .debug_selector(|| "column-preview-picture".into())
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                img(image)
                    .debug_selector(|| "column-preview-image".into())
                    .w(px(logical_w))
                    .h(px(logical_h))
                    .object_fit(gpui::ObjectFit::Fill),
            )
    })
}

/// The logical size a still of `w`x`h` pixels draws at in a frame of `width`x`height`, border included.
pub(super) fn drawn(
    frame: &Frame,
    (w, h): (f64, f64),
    width: f32,
    height: f32,
    device: f32,
) -> (f32, f32) {
    let original = frame
        .facts
        .as_ref()
        .map_or((0., 0.), |m| (m.w as f64, m.h as f64));
    let vector = is_vector_name(&frame.subject.row.name);
    // A raster never draws past one source pixel per device pixel.
    let limit = thumbs::preview_limit(
        frame.kind == Kind::Video,
        vector,
        w,
        h,
        original.0,
        original.1,
    ) / f64::from(device);
    let fit = thumbs::fit_scale(
        (width - 2. * FRAME_BORDER).max(1.) as f64,
        (height - 2. * FRAME_BORDER).max(1.) as f64,
        w,
        h,
        limit,
    );
    let scale = f64::from(fit as f32);
    match crate::preview_quality::fitted_pixels(w, h, fit, limit, device.into()) {
        Some(own) => crate::preview_quality::own_size(own, device.into()),
        None => crate::preview_quality::logical_size(
            (w * scale).round(),
            (h * scale).round(),
            device.into(),
        ),
    }
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

/// A video frame's picture: the still before Play and again once the clip has ended, as v0.3.12 returns to its film-strip poster.
pub(crate) fn column_media(
    player: Option<&MediaPlayer>,
    still: Option<Arc<RenderImage>>,
    catalog: &I18n,
) -> Picture {
    let ended = |player: &MediaPlayer| {
        player.state.has_video
            && player.state.media == Media::Ended
            && player.status() != Status::Failed
    };
    column_picture(match player {
        Some(player) if still.is_none() || !ended(player) => {
            surface::picture(player, &surface::Facts::default(), catalog)
        }
        _ => Picture::Video {
            frame: still,
            poster: true,
        },
    })
}

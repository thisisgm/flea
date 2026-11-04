//! Full-quality timing waits for the raster actually requested by the mounted surface.
use crate::g2::{pv2::surface::Surface, pv3::player::MediaPlayer};

pub(crate) fn pdf_ready(surface: &Surface) -> bool {
    if surface.sizes.is_empty() || surface.render_width == 0 {
        return false;
    }
    let (first, last) = if surface.continuous {
        (
            surface.layout.page_at(surface.view_top),
            surface
                .layout
                .page_at((surface.view_top + surface.view_height - 0.001).max(surface.view_top)),
        )
    } else {
        (surface.page, surface.page)
    };
    (first..=last).all(|page| surface.cache.exact(page, surface.render_width).is_some())
}

pub(crate) fn media_ready(player: &MediaPlayer) -> bool {
    let Some(subject) = &player.subject else {
        return false;
    };
    if !subject.video {
        return true;
    }
    let Some(image) = &player.frame else {
        return false;
    };
    let size = image.size(0);
    let got = (size.width.0 as u32, size.height.0 as u32);
    flea::ui_logic_pv3::picture::decode_size(got.0, got.1, subject.box_w, subject.box_h) == got
}

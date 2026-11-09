//! Replace a column's first thumbnail in place with pixels for the actual device rect.
use super::*;
use flea::ui_logic_preview_core::{pixels::is_vector_name, thumbs};
use gpui::Window;

impl SelectionPreview {
    pub(super) fn full_quality(&self, cx: &gpui::App) -> bool {
        self.drawn
            .as_ref()
            .is_some_and(|frame| match &frame.mounted {
                Mounted::Pdf(pdf) => crate::preview_quality::pdf_ready(&pdf.read(cx).surface),
                Mounted::Media(Some(player)) => {
                    crate::preview_quality::media_ready(player.read(cx))
                }
                _ => !matches!(frame.kind, Kind::Image | Kind::Video) || frame.quality.ready,
            })
    }

    pub(super) fn refine_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(frame) = self.drawn.as_ref() else {
            return;
        };
        let (width, height) = self.frame_size(frame.kind, if self.full_facts { 3 } else { 1 });
        let Some(frame) = self.drawn.as_mut().filter(|frame| !frame.manual) else {
            return;
        };
        if !matches!(frame.kind, Kind::Image | Kind::Video) {
            return;
        }
        if let Mounted::Media(Some(player)) = &frame.mounted {
            let (w, h) = crate::preview_quality::physical(
                f64::from((width - 2.).max(1.)),
                f64::from((height - 2.).max(1.)),
                f64::from(window.scale_factor()),
            );
            player.update(cx, |player, _| player.resize(w, h));
            return;
        }
        let Some(image) = &frame.image else {
            return;
        };
        let seed = (frame.kind == Kind::Video).then(|| image.clone());
        let basis = frame.quality.basis.of(image);
        let Some(size) = refine_size(frame, (width, height), window.scale_factor()) else {
            return;
        };
        let version = (frame.subject.row.size, frame.subject.row.mtime);
        if let Some(Ok(image)) = frame.quality.poll_with_seed(
            frame.subject.path().into(),
            size,
            version,
            seed,
            window,
            cx,
        ) {
            if frame
                .image
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &image))
            {
                frame.quality.basis.made(&image, basis);
                if let Some(old) = frame.image.replace(image) {
                    window.defer(cx, move |window, cx| cx.drop_image(old, Some(window)));
                }
                cx.notify();
            }
        }
    }

    /// A pending local JPEG asks for its sharp pixels at the meta reply, so they can land before any thumbnail.
    pub(super) fn early_picture(
        frame: &mut Frame,
        room: (f32, f32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if frame.image.is_some() || header_size(frame).is_none() {
            return;
        }
        let version = (frame.subject.row.size, frame.subject.row.mtime);
        let path: std::path::PathBuf = frame.subject.path().into();
        // Going back to a file draws its kept picture at once, when that picture still fits the room as drawn.
        let kept = crate::preview_quality::reuse::kept(&path, version)
            .map(|(size, _)| size)
            .filter(|&size| drawn_size(frame, size, room, window.scale_factor()) == size);
        let Some(size) = kept.or_else(|| refine_size(frame, room, window.scale_factor())) else {
            return;
        };
        if let Some(Ok(image)) = frame.quality.poll(path, size, version, window, cx) {
            frame.image = Some(image);
        }
    }
}

/// A local JPEG's upright size from the meta reply, read from the same header and Exif walk as its decoder.
fn header_size(frame: &Frame) -> Option<(f64, f64)> {
    let jpeg = std::path::Path::new(&frame.subject.row.name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ["jpg", "jpeg", "jpe", "jfif"].contains(&ext.to_ascii_lowercase().as_str())
        });
    if frame.kind != Kind::Image || frame.manual || frame.remote || !jpeg {
        return None;
    }
    let facts = frame
        .facts
        .as_ref()
        .filter(|facts| facts.w > 0 && facts.h > 0)?;
    let (w, h) = (f64::from(facts.w), f64::from(facts.h));
    Some(if facts.orient >= 5 { (h, w) } else { (w, h) })
}

/// The device size a frame refines to. A local JPEG fits its header size until a thumbnail is drawn; a drawn
/// thumbnail, or a picture refined from one, keeps its own geometry, so the sharp pixels replace it in place.
fn refine_size(frame: &Frame, room: (f32, f32), scale: f32) -> Option<(u32, u32)> {
    let header = header_size(frame).map(|(w, h)| fitted((w, h), 1., room, scale));
    let own = frame.quality.ready
        && frame.quality.requested.as_ref().map(|request| request.size) == header;
    if let Some(size) = header.filter(|_| frame.image.is_none() || own) {
        return Some(size);
    }
    let drawn = frame.quality.basis.of(frame.image.as_ref()?);
    Some(drawn_size(frame, drawn, room, scale))
}

/// The device size a picture of `drawn` pixels refines to in this room, never past its source's own size.
fn drawn_size(frame: &Frame, drawn: (u32, u32), room: (f32, f32), scale: f32) -> (u32, u32) {
    let (w, h) = (f64::from(drawn.0), f64::from(drawn.1));
    let source = frame
        .facts
        .as_ref()
        .map_or((0., 0.), |meta| (meta.w as f64, meta.h as f64));
    let limit = thumbs::preview_limit(
        frame.kind == Kind::Video,
        is_vector_name(&frame.subject.row.name),
        w,
        h,
        source.0,
        source.1,
    );
    fitted((w, h), limit, room, scale)
}

fn fitted((w, h): (f64, f64), limit: f64, (width, height): (f32, f32), scale: f32) -> (u32, u32) {
    let limit = limit / f64::from(scale);
    let fit = thumbs::fit_scale(
        (width - 2.).max(1.) as f64,
        (height - 2.).max(1.) as f64,
        w,
        h,
        limit,
    );
    crate::preview_quality::fitted_pixels(w, h, fit, limit, scale.into()).unwrap_or_else(|| {
        crate::preview_quality::physical((w * fit).round(), (h * fit).round(), scale.into())
    })
}

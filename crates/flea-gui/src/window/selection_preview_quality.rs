//! Replace a column's first thumbnail in place with pixels for the actual device rect.
use super::*;
use flea::ui_logic_pv1::{pixels::is_vector_name, thumbs};
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
        let (w, h) = (f64::from(basis.0), f64::from(basis.1));
        let source = frame
            .facts
            .as_ref()
            .map_or((0., 0.), |meta| (meta.w as f64, meta.h as f64));
        let device = f64::from(window.scale_factor());
        // A raster asks for no more than its source's own pixels.
        let limit = thumbs::preview_limit(
            frame.kind == Kind::Video,
            is_vector_name(&frame.subject.row.name),
            w,
            h,
            source.0,
            source.1,
        ) / device;
        let fit = thumbs::fit_scale(
            (width - 2.).max(1.) as f64,
            (height - 2.).max(1.) as f64,
            w,
            h,
            limit,
        );
        let size =
            crate::preview_quality::fitted_pixels(w, h, fit, limit, device).unwrap_or_else(|| {
                crate::preview_quality::physical((w * fit).round(), (h * fit).round(), device)
            });
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
}

//! Decode dimensions from the mounted production image, for native quality gates.
use super::*;
use crate::{g2::pv1::Body, window::selection_preview::Mounted};
use gpui::RenderImage;
use std::sync::Arc;

fn dimensions(image: Option<&Arc<RenderImage>>) -> (u32, u32) {
    image.map_or((0, 0), |image| {
        let size = image.size(0);
        (size.width.0 as u32, size.height.0 as u32)
    })
}
fn answer(
    surface: &str,
    kind: &str,
    decoded: (u32, u32),
    requested: Option<((u32, u32), bool)>,
    scale: f32,
    decodes: usize,
) -> Reply {
    let (target, refined) = requested.unwrap_or(((0, 0), false));
    Reply::value(format!(
        r#"{{"surface":"{surface}","kind":"{kind}","decoded":[{},{}],"requested":[{},{}],"refined":{refined},"scale":{scale},"decodes":{decodes}}}"#,
        decoded.0, decoded.1, target.0, target.1
    ))
}
impl PreviewState<'_> {
    pub(super) fn quality(&self) -> Reply {
        let scale = self.window.scale_factor();
        if let Some(look) = self.look().filter(|look| look.is_open()) {
            if let Some(doc) = look.subject().and_then(|subject| subject.body.doc()) {
                let image = match &doc.view {
                    Document::Pdf(pdf) => pdf
                        .read(self.cx)
                        .surface
                        .image(pdf.read(self.cx).surface.page),
                    Document::Media(media) => media.player.read(self.cx).frame.as_ref(),
                    Document::Font(font) => font
                        .read(self.cx)
                        .shown
                        .as_ref()
                        .and_then(|shown| shown.images.first().map(|(image, _)| image)),
                    _ => None,
                };
                return answer("look", doc.kind(), dimensions(image), None, scale, 0);
            }
            if look.subject().is_some_and(|subject| {
                matches!(
                    subject.body,
                    Body::Image { .. } | Body::Raw { .. } | Body::Link { .. }
                )
            }) {
                return answer(
                    "look",
                    "image",
                    dimensions(look.svg_raster().as_ref()),
                    look.quality_pixels(),
                    scale,
                    look.quality_decodes(),
                );
            }
        }
        let Some(host) = self.shell.panes[self.shell.focus_side].as_ref() else {
            return answer("none", "", (0, 0), None, scale, 0);
        };
        if self.shell.shown_view() == View::Grid {
            let Some(grid) = host.grid.as_ref() else {
                return answer("grid", "", (0, 0), None, scale, 0);
            };
            let state = grid.state.borrow();
            let decoded = state
                .drawn_row(host.pane.cursor)
                .and_then(|row| row.decoded)
                .unwrap_or((0, 0));
            return answer(
                "grid",
                "image",
                decoded,
                host.list.borrow().pictures.quality_pixels(host.pane.cursor),
                scale,
                host.list
                    .borrow()
                    .pictures
                    .quality_decodes(host.pane.cursor),
            );
        }
        let Some(view) = host.preview.as_ref().map(|view| view.read(self.cx)) else {
            return answer("none", "", (0, 0), None, scale, 0);
        };
        let Some(frame) = view.drawn.as_ref() else {
            return answer("column", "", (0, 0), None, scale, 0);
        };
        let image = match &frame.mounted {
            Mounted::Pdf(pdf) => pdf
                .read(self.cx)
                .surface
                .image(pdf.read(self.cx).surface.page),
            Mounted::Media(Some(player)) => {
                player.read(self.cx).frame.as_ref().or(frame.image.as_ref())
            }
            _ => frame.image.as_ref(),
        };
        let request = frame.quality.requested.as_ref();
        answer(
            "column",
            frame.kind.name(),
            dimensions(image),
            request.map(|request| (request.size, frame.quality.ready)),
            scale,
            request.map_or(0, |request| {
                crate::preview_quality::reuse::decodes(&request.path)
            }),
        )
    }
}

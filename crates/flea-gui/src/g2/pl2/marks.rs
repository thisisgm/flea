//! Small form marks use the shared monochrome atlas and the same body clip as their fields.
use super::{dialog::Focus, drawing::Drawn, view::NetworkView};
use crate::g2::{mn1::glyphs, op1::palette::CardPalette};
use gpui::{point, px, size, App, Bounds, Pixels, Point, Window};

pub(super) fn paint(
    view: &NetworkView,
    drawn: &Drawn,
    origin: Point<Pixels>,
    p: &CardPalette,
    window: &mut Window,
    cx: &App,
) {
    let Some(body) = drawn.body() else {
        return;
    };
    let clip = Bounds::new(
        origin + point(px(body.x), px(body.y)),
        size(px(body.w), px(body.h)),
    );
    window.with_content_mask(Some(gpui::ContentMask { bounds: clip }), |window| {
        let mut mark = |name, x, y, side, ink| {
            let (key, data) = if name == "alert" {
                let (key, data) = crate::g2::op1::glyph::ALERT;
                (gpui::SharedString::new_static(key), data)
            } else {
                glyphs::mark(name)
            };
            let _ = window.paint_svg(
                Bounds::new(origin + point(px(x), px(y)), size(px(side), px(side))),
                key,
                Some(data),
                gpui::TransformationMatrix::unit(),
                ink,
                cx,
            );
        };
        if let Some((_, rect)) = drawn
            .hits
            .iter()
            .find(|(focus, r)| *focus == Focus::Eye && r.h > 0.)
        {
            mark(
                "eye",
                rect.x + 10.5,
                rect.y + 8.5,
                13.,
                if view.model.reveal {
                    p.accent
                } else {
                    p.muted_frame
                },
            );
        }
        if view.model.form.tls && view.model.form.protocol.tls_control() {
            mark("check", 19., 489. - view.scroll, 14., p.surface);
        }
        if view
            .model
            .status
            .is_some_and(|key| key != super::mount::GUEST_EMPTY)
        {
            mark("alert", 17., 547. - view.scroll, 12., p.error_on_surface);
        }
    });
}

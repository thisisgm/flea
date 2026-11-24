//! A touchpad pinch over a picture (#505): the zoom follows the fingers, and the point under them stays put.
use super::{body::Fit, Body, QuickLook};
use flea::ui_logic_preview_core::keys::{ZOOM_MAX, ZOOM_MIN};
use gpui::{Bounds, Context, Pixels, Point, TouchPhase};

fn f(value: Pixels) -> f64 {
    f64::from(f32::from(value))
}

/// The pinch's next zoom: `start` times the fingers' scale, held to the ladder's ends. At an end the
/// start moves with the fingers, so pinching back turns around at once instead of first undoing the overshoot.
pub(super) fn scaled(start: &mut f64, scale: f64) -> f64 {
    let next = (*start * scale).clamp(ZOOM_MIN, ZOOM_MAX);
    if scale > 0. {
        *start = next / scale;
    }
    next
}

impl QuickLook {
    pub(super) fn picture_pinch(
        &mut self,
        phase: TouchPhase,
        delta: f32,
        at: Point<Pixels>,
        card: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let picture = self.subject.as_ref().is_some_and(|s| {
            matches!(
                s.body,
                Body::Image { .. } | Body::Link { .. } | Body::Raw { .. }
            )
        });
        if !picture || phase == TouchPhase::Ended {
            self.pinch = None;
            return;
        }
        // A pinch that began elsewhere, its start unseen, scales from the zoom it finds.
        let (mut start, mut scale) = match (phase, self.pinch) {
            (TouchPhase::Moved, Some(pinch)) => pinch,
            _ => (self.zoom, 1.),
        };
        scale += f64::from(delta);
        let next = scaled(&mut start, scale);
        self.pinch = Some((start, scale));
        let device = self.svg.as_ref().map_or(1., |picture| picture.scale);
        let Some((_, fit, _)) = self.picture_fit(f(card.size.width), f(card.size.height), device)
        else {
            return;
        };
        let (px, py) = self.picture_point(at, card, &fit);
        if !self.zoom_to(next) {
            return;
        }
        if let Some(view) = self.picture_viewport(card, device) {
            // The new content places the picture centred, as picture_parts draws it.
            let (pw, ph) = (fit.w * next, fit.h * next);
            self.scroll.anchor = None;
            self.scroll.x = (view.content_w - pw) / 2. + px * pw - f(at.x - view.bounds.origin.x);
            self.scroll.y = (view.content_h - ph) / 2. + py * ph - f(at.y - view.bounds.origin.y);
            self.scroll.fit(view);
        }
        cx.notify();
    }

    /// Where `at` falls on the picture as drawn now, as fractions of its width and height.
    fn picture_point(&self, at: Point<Pixels>, card: Bounds<Pixels>, fit: &Fit) -> (f64, f64) {
        let (pw, ph) = (fit.w * self.zoom, fit.h * self.zoom);
        let (x, y) = match self.scroll.view.filter(|_| self.zoom > 1.) {
            Some(view) => (
                (f(at.x - view.bounds.origin.x) + self.scroll.x - (view.content_w - pw) / 2.) / pw,
                (f(at.y - view.bounds.origin.y) + self.scroll.y - (view.content_h - ph) / 2.) / ph,
            ),
            None => (
                (f(at.x - card.origin.x) - fit.x) / fit.w,
                (f(at.y - card.origin.y) - fit.y) / fit.h,
            ),
        };
        (x.clamp(0., 1.), y.clamp(0., 1.))
    }
}

#[cfg(test)]
#[path = "pinch_tests.rs"]
mod tests;

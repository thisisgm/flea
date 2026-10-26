//! Quick Look's image fit, physical decode request and zoomed picture viewport.
use super::{
    bar::{self, Name},
    body::{fit_painted, fit_rounded, Fit},
    card::text,
    look::{self, Look},
    scroller::View,
    Body, QuickLook, RasterState, Subject,
};
use gpui::{
    div, img, point, prelude::*, px, size, AnyElement, Bounds, Context, ObjectFit, Pixels, Window,
};
use std::path::Path;

impl QuickLook {
    fn picture_fit(&self, w: f64, h: f64) -> Option<(&Path, Fit, (f64, f64))> {
        let (path, sw, sh, top, bottom, painted) = match &self.subject.as_ref()?.body {
            Body::Image { path, w, h } => (path, *w, *h, 0., 0., false),
            Body::Link { path, w, h, .. } => (path, *w, *h, f64::from(look::BAR), 0., true),
            Body::Raw { path, w, h, .. } => (path, *w, *h, 0., f64::from(look::BAR), false),
            _ => return None,
        };
        let mut fit = if painted {
            fit_painted(w, h - top - bottom, sw, sh)
        } else {
            fit_rounded(w, h - top - bottom, sw, sh)
        }?;
        fit.y += top;
        Some((path, fit, (sw, sh)))
    }

    pub(super) fn picture_raster(
        &mut self,
        w: f64,
        h: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> RasterState {
        let next = self
            .picture_fit(w, h)
            .map(|(path, f, source)| (path.to_path_buf(), f, source));
        match next {
            Some((path, f, source)) => self.picture_image(&path, f.w, f.h, source, window, cx),
            None => RasterState::default(),
        }
    }

    pub(super) fn picture_viewport(&self, card: Bounds<Pixels>) -> Option<View> {
        if self.zoom <= 1. {
            return None;
        }
        let w = f64::from(f32::from(card.size.width));
        let h = f64::from(f32::from(card.size.height));
        let (_, fit, _) = self.picture_fit(w, h)?;
        let top = if matches!(self.subject.as_ref()?.body, Body::Link { .. }) {
            look::BAR
        } else {
            0.
        };
        let height = h - f64::from(top + look::BAR);
        Some(View {
            bounds: Bounds::new(
                card.origin + point(px(0.), px(top)),
                size(card.size.width, px(height as f32)),
            ),
            content_w: w.max(fit.w * self.zoom),
            content_h: height.max(fit.h * self.zoom),
            sideways: true,
        })
    }

    pub(super) fn picture_action(&mut self, action: &str, cx: &mut Context<Self>) -> bool {
        if !self.subject.as_ref().is_some_and(|s| {
            matches!(
                s.body,
                Body::Image { .. } | Body::Link { .. } | Body::Raw { .. }
            )
        }) {
            return false;
        }
        let next = match action {
            "zoomIn" => flea::ui_logic_pv1::keys::zoom_step(self.zoom, 1),
            "zoomOut" => flea::ui_logic_pv1::keys::zoom_step(self.zoom, -1),
            "zoomFit" => 1.,
            _ => return false,
        };
        // As in PreviewImage.qml, a picture without its first frame cannot zoom in.
        if next != self.zoom && (next <= 1. || self.svg.as_ref().is_some_and(|s| s.image.is_some()))
        {
            let prior = self.scroll.view;
            self.zoom = next;
            if let Some(window) = self.window {
                let _ = window.update(cx, |_, window, _| {
                    let card = self.card(window.viewport_size());
                    if let Some(view) = self.picture_viewport(card) {
                        let (fx, fy) = prior
                            .map(|old| {
                                let w = f64::from(f32::from(old.bounds.size.width));
                                let h = f64::from(f32::from(old.bounds.size.height));
                                (
                                    (self.scroll.x + w / 2.) / old.content_w,
                                    (self.scroll.y + h / 2.) / old.content_h,
                                )
                            })
                            .unwrap_or((0.5, 0.5));
                        self.scroll.x =
                            fx * view.content_w - f64::from(f32::from(view.bounds.size.width)) / 2.;
                        self.scroll.y = fy * view.content_h
                            - f64::from(f32::from(view.bounds.size.height)) / 2.;
                        // A second zoom before the next frame anchors on this view, not the drawn one.
                        self.scroll.fit(view);
                    } else {
                        self.scroll.x = 0.;
                        self.scroll.y = 0.;
                        self.scroll.view = None;
                    }
                });
            }
            cx.notify();
        }
        true
    }

    pub(super) fn picture_parts(
        &self,
        subject: &Subject,
        w: f64,
        h: f64,
        raster: &RasterState,
        look: &Look,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some((_, fit, _)) = self.picture_fit(w, h) else {
            return Vec::new();
        };
        let mut parts = Vec::new();
        let top = if let Body::Link { target, .. } = &subject.body {
            let name = Name {
                text: &subject.name,
                tail: Some((target, look.meta)),
            };
            parts.push(bar::bar(subject.mark(), name, None, true, look, self, cx));
            f64::from(look::BAR)
        } else {
            0.
        };
        let (pw, ph) = (fit.w * self.zoom, fit.h * self.zoom);
        let (x, y) = if let Some(view) = self.scroll.view.filter(|_| self.zoom > 1.) {
            (
                (view.content_w - pw) / 2. - self.scroll.x,
                top + (view.content_h - ph) / 2. - self.scroll.y,
            )
        } else {
            (fit.x, fit.y)
        };
        let mut picture = div()
            .absolute()
            .left(px(x as f32))
            .top(px(y as f32))
            .w(px(pw as f32))
            .h(px(ph as f32))
            .debug_selector(|| "pv1-picture".into());
        if let Some(image) = raster.image.clone() {
            picture = picture.child(img(image).size_full().object_fit(ObjectFit::Fill));
        }
        // The zoom strip is outside the clipped viewport and never covered by the picture.
        let caption = if self.zoom > 1. {
            Some(format!("{}%", (self.zoom * 100.).round() as u32))
        } else if let Body::Raw { caption, .. } = &subject.body {
            Some(caption.clone())
        } else {
            None
        };
        let bottom = if caption.is_some() { look::BAR } else { 0. };
        let content = div()
            .absolute()
            .top(px(top as f32))
            .left_0()
            .right_0()
            .bottom(px(bottom))
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .top(px(-top as f32))
                    .size_full()
                    .child(picture),
            )
            .children(self.scroll.bars(look));
        parts.push(content.into_any_element());
        if let Some(caption) = caption {
            parts.push(
                div()
                    .debug_selector(|| "pv1-caption".into())
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(look::BAR))
                    .bg(if self.zoom > 1. {
                        look.surface
                    } else {
                        look.background
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(text(
                        &caption,
                        look::CAPTION,
                        if self.zoom > 1. {
                            look.foreground
                        } else {
                            look.muted
                        },
                    ))
                    .into_any_element(),
            );
        }
        parts
    }
}

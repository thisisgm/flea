//! Quick Look's image fit, physical decode request and zoomed picture viewport.
use super::{
    Body, QuickLook, RasterState, Subject,
    bar::{self, Name},
    body::{Fit, fit_own, fit_painted_path, fit_rounded_path},
    card::text,
    look::{self, Look},
    scroller::View,
};
use gpui::{
    AnyElement, Bounds, Context, ObjectFit, Pixels, Window, div, img, point, prelude::*, px, size,
};
use std::path::Path;

pub(super) fn pinch_zoom(zoom: f64, delta: f32) -> f64 {
    (zoom * (1. + f64::from(delta))).clamp(
        flea::ui_logic_pv1::keys::ZOOM_MIN,
        flea::ui_logic_pv1::keys::ZOOM_MAX,
    )
}

impl QuickLook {
    pub(super) fn picture_pinch(&mut self, delta: f32, cx: &mut Context<Self>) -> bool {
        if !self.subject.as_ref().is_some_and(|s| {
            matches!(
                s.body,
                Body::Image { .. } | Body::Link { .. } | Body::Raw { .. }
            )
        }) {
            return false;
        }
        let next = pinch_zoom(self.zoom, delta);
        if next != self.zoom {
            self.scroll.anchor = (next > 1.).then(|| self.scroll.centre());
            if next <= 1. {
                self.scroll.x = 0.;
                self.scroll.y = 0.;
                self.scroll.pan = None;
                self.scroll.view = None;
            }
            self.zoom = next;
            cx.notify();
        }
        true
    }

    fn picture_fit(&self, w: f64, h: f64, device: f64) -> Option<(&Path, Fit, (f64, f64))> {
        let (path, sw, sh, top, bottom, painted) = match &self.subject.as_ref()?.body {
            Body::Image { path, w, h } => (path, *w, *h, 0., 0., false),
            Body::Link { path, w, h, .. } => (path, *w, *h, f64::from(look::BAR), 0., true),
            Body::Raw { path, w, h, .. } => (path, *w, *h, 0., f64::from(look::BAR), false),
            _ => return None,
        };
        let room_h = h - top - bottom;
        let vector = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(flea::ui_logic_pv1::pixels::is_vector_name);
        // A raster draws one source pixel per device pixel at most; only zoom enlarges it.
        let own = (!vector && sw >= 1. && sh >= 1.)
            .then(|| crate::preview_quality::own_size((sw as u32, sh as u32), device))
            .and_then(|(ow, oh)| fit_own(w, room_h, ow.into(), oh.into()));
        let (fit_w, fit_h) = if vector {
            (sw, sh)
        } else {
            (sw / device, sh / device)
        };
        let mut fit = match own {
            Some(own) => own,
            None if painted => fit_painted_path(w, room_h, fit_w, fit_h, path)?,
            None => fit_rounded_path(w, room_h, fit_w, fit_h, path)?,
        };
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
            .picture_fit(w, h, window.scale_factor().into())
            .map(|(path, f, source)| (path.to_path_buf(), f, source));
        match next {
            Some((path, f, source)) => self.picture_image(&path, f.w, f.h, source, window, cx),
            None => RasterState::default(),
        }
    }

    pub(super) fn picture_viewport(&self, card: Bounds<Pixels>, device: f64) -> Option<View> {
        if self.zoom <= 1. {
            return None;
        }
        let w = f64::from(f32::from(card.size.width));
        let h = f64::from(f32::from(card.size.height));
        let (_, fit, _) = self.picture_fit(w, h, device)?;
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
            // The next frame centres the new viewport on this point: a key runs inside the window's own update, so no window is borrowed here.
            self.scroll.anchor = (next > 1.).then(|| self.scroll.centre());
            if next <= 1. {
                self.scroll.x = 0.;
                self.scroll.y = 0.;
                self.scroll.pan = None;
                self.scroll.view = None;
            }
            self.zoom = next;
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
        let device = self.svg.as_ref().map_or(1., |picture| picture.scale);
        let Some((path, fit, source)) = self.picture_fit(w, h, device) else {
            return Vec::new();
        };
        let mut parts = Vec::new();
        let top = if let Body::Link { target, .. } = &subject.body {
            let name = Name {
                text: &subject.name,
                tail: Some((target, look.meta)),
            };
            parts.push(bar::bar(
                subject.mark_with_languages(self.languages),
                name,
                None,
                true,
                look,
                self,
                cx,
            ));
            f64::from(look::BAR)
        } else {
            0.
        };
        let (pw, ph) = (fit.w * self.zoom, fit.h * self.zoom);
        let own = crate::preview_quality::own_size((source.0 as u32, source.1 as u32), device);
        let (logical_w, logical_h) = if self.zoom <= 1. && own == (fit.w as f32, fit.h as f32) {
            own
        } else {
            crate::preview_quality::logical_size(pw, ph, device)
        };
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
            .w(px(logical_w))
            .h(px(logical_h))
            .child(super::observations::bounds(
                &self.observations,
                |observed, bounds| observed.picture = Some(bounds),
            ))
            .debug_selector(|| "pv1-picture".into());
        if let Some(image) = raster.image.clone() {
            if crate::bench_thumbs::enabled() {
                let path = path.to_string_lossy().into_owned();
                let decoded = image.size(0);
                let edge = i64::from(decoded.width.0.max(decoded.height.0));
                let marked = edge == i64::from(crate::bench_thumbs::PREVIEW_EDGE);
                picture = picture.child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let painted = window
                                .paint_image(
                                    bounds,
                                    bounds,
                                    gpui::Corners::default(),
                                    image,
                                    0,
                                    false,
                                )
                                .is_ok();
                            if painted && marked {
                                crate::bench_thumbs::painted(
                                    "preview",
                                    &path,
                                    (decoded.width.0 as u32, decoded.height.0 as u32),
                                    bounds,
                                    window,
                                );
                            }
                        },
                    )
                    .size_full(),
                );
            } else {
                picture = picture.child(img(image).size_full().object_fit(ObjectFit::Fill));
            }
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
            .child(super::observations::bounds(
                &self.observations,
                |observed, bounds| observed.surface = Some(bounds),
            ))
            .debug_selector(|| "pv1-surface".into())
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
            let ground = if self.zoom > 1. {
                look.surface
            } else {
                look.background
            };
            parts.push(
                div()
                    .debug_selector(|| "pv1-caption".into())
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(look::BAR))
                    .bg(ground)
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

//! A move keeps the shown card drawn until the new subject reveals, so the swap takes one frame.
use super::{look::Look, QuickLook, Scroll, Subject, SvgPicture};
use gpui::{div, px, AnyElement, Context, IntoElement, ParentElement, Styled, Window};

/// The shown card a move replaced, drawn until the new subject reveals.
pub(super) struct Held {
    subject: Option<Subject>,
    svg: Option<SvgPicture>,
    scroll: Scroll,
    zoom: f64,
}

impl QuickLook {
    /// Keeps the shown subject, its picture, scroll and zoom; a mounted view stays open until the hold lets it go.
    pub(super) fn hold_card(&mut self, cx: &mut Context<Self>) {
        let Some(subject) = self.subject.take() else {
            return;
        };
        let mut svg = self.svg.take();
        if let Some(svg) = svg.as_mut() {
            svg.quality.clear(cx);
        }
        self.held = Some(Held {
            subject: Some(subject),
            svg,
            scroll: std::mem::take(&mut self.scroll),
            zoom: self.zoom,
        });
    }

    pub(super) fn release_held(&mut self, cx: &mut Context<Self>) {
        let Some(held) = self.held.take() else {
            return;
        };
        let current = self.subject.as_ref().and_then(|subject| subject.body.doc());
        if let Some(old) = held
            .subject
            .as_ref()
            .and_then(|subject| subject.body.doc())
            .filter(|old| current.is_none_or(|new| new.id() != old.id()))
        {
            old.close(cx);
        }
        if let Some(svg) = held.svg {
            if svg.interim {
                self.retire_interim(svg.image, cx);
            } else {
                self.retire(svg.image, cx);
            }
        }
    }

    /// The held card as it last drew: its own subject, rect, picture, scroll and zoom, with no new request.
    pub(super) fn held_layer(
        &mut self,
        motion: crate::reveal::Frame,
        look: &Look,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut held = self.held.take()?;
        self.swap_held(&mut held);
        let mut card = self.card(window.viewport_size());
        card.origin.y += px(motion.rise);
        let raster = self.svg.as_ref().map(SvgPicture::state).unwrap_or_default();
        let surface = self.surface(card, Some(raster), look, window, cx);
        self.swap_held(&mut held);
        self.held = Some(held);
        Some(
            div()
                .absolute()
                .size_full()
                .opacity(motion.opacity)
                .child(surface)
                .into_any_element(),
        )
    }

    fn swap_held(&mut self, held: &mut Held) {
        std::mem::swap(&mut self.subject, &mut held.subject);
        std::mem::swap(&mut self.svg, &mut held.svg);
        std::mem::swap(&mut self.scroll, &mut held.scroll);
        std::mem::swap(&mut self.zoom, &mut held.zoom);
    }
}

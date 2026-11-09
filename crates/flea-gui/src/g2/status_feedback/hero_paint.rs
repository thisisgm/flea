//! The hero caption's painter: run from the window's overlay, or from the hero itself when it has none.
use super::super::caption::{paint_line, width};
use super::super::{bump, now_ms};
use super::EmptyHero;
use gpui::{
    point, size, App, Bounds, Context, Element, ElementId, Entity, Font, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, TextRun, Window,
};

/// The caption's face as the last render resolved it; the painter shapes the current phrase with it.
#[derive(Clone, PartialEq)]
pub(super) struct Face {
    pub font: Font,
    pub size: Pixels,
    pub line: Pixels,
    pub tracking: Pixels,
    pub color: Hsla,
}

impl EmptyHero {
    /// Advances the caption before paint: frames only during a fade, one timer between beats, a render only when layout moves.
    pub(crate) fn prepare_caption(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.is_visible() || !self.rotation.visible() {
            self.timer = None;
            return;
        }
        let now = now_ms(self.base, cx);
        self.sync_motion(now, cx);
        self.rotation.advance(now);
        let frame = self.rotation.frame(now);
        self.shown_frame = Some(frame);
        if self.rotation.entering() {
            self.timer = None;
        } else if self.rotation.caption_animating() {
            self.timer = None;
            bump(&self.counters.frames);
            crate::feedback::spinner::request_frame(window, cx);
        } else if let Some(deadline) = self.rotation.next_deadline() {
            self.arm(deadline, now, window.current_view(), cx);
        }
        let Some(face) = self.face.as_ref() else {
            return;
        };
        if self.shaped.as_ref().map(|(phrase, _)| *phrase) != Some(frame.phrase) {
            let text = self.phrases.get(frame.phrase).cloned().unwrap_or_default();
            let run = TextRun {
                len: text.len(),
                font: face.font.clone(),
                color: face.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window
                .text_system()
                .shape_line(text, face.size, &[run], None);
            self.shaped = Some((frame.phrase, line));
        }
    }

    /// Paints the prepared caption centred in its slot, inside the pane's clip, and records what it drew.
    pub(crate) fn paint_caption(&self, window: &mut Window) {
        let (Some((slot, mask)), Some(face), Some((phrase, line)), Some(frame)) = (
            self.place.get(),
            self.face.as_ref(),
            self.shaped.as_ref(),
            self.shown_frame,
        ) else {
            return;
        };
        if !window.is_visible() || !self.rotation.visible() {
            return;
        }
        let wide = width(line, face.tracking);
        let origin = point(
            window.pixel_snap(slot.center().x - wide / 2.),
            slot.origin.y,
        );
        let color = face.color.opacity(frame.opacity * frame.caption);
        window.with_content_mask(Some(mask), |window| {
            paint_line(line, origin, face.line, face.tracking, color, window);
        });
        self.painted.set(
            "caption",
            super::super::observe::Text {
                bounds: Bounds::new(origin, size(wide, slot.size.height)),
                text: self.phrases.get(*phrase).cloned().unwrap_or_default(),
                implicit_width: f32::from(wide),
                color: face.color,
                font_size: f32::from(face.size),
            },
        );
    }
}

/// A hero with no window overlay paints its own caption after its slot has been placed.
pub(super) struct Layer(pub Entity<EmptyHero>);

impl IntoElement for Layer {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Layer {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            position: gpui::Position::Absolute,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0
            .update(cx, |hero, cx| hero.prepare_caption(window, cx));
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.read(cx).paint_caption(window);
    }
}

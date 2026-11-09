//! The marks a card draws: a row's own mark, the alert, and a job's Spinner.qml spiral crawl, painted from static data.
use crate::feedback::spinner;
use flea::ui_logic_file_ops::card;
use gpui::{
    fill, point, px, size, App, Bounds, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine, SharedString, Style, TextRun,
    TransformationMatrix, Window,
};
use std::{rc::Rc, time::Instant};

/// The alert triangle a failed job carries, Lucide's cut at the board's path data.
pub const ALERT: (&str, &[u8]) = (
    "flea-op1:alert",
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M12 2 23 21H1z"/><path d="M12 10v4"/><path d="M12 18L12.01 18"/></svg>"#,
);

/// What one mark paints.
#[derive(Clone)]
pub enum Paint {
    Svg(SharedString, &'static [u8]),
    // One persistent spiral per job, at a fixed time or crawling; the mode names it for accessibility.
    Spiral {
        id: u64,
        mode: spinner::Mode,
        still: Option<f32>,
    },
}

pub struct Glyph {
    pub paint: Paint,
    pub side: f32,
    pub ink: Hsla,
}

impl Glyph {
    pub fn svg((key, data): (&'static str, &'static [u8]), side: f32, ink: Hsla) -> Self {
        Self {
            paint: Paint::Svg(SharedString::new_static(key), data),
            side,
            ink,
        }
    }
}

impl IntoElement for Glyph {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Glyph {
    type RequestLayoutState = ();
    type PrepaintState = Option<Rc<spinner::Crawl>>;

    fn id(&self) -> Option<ElementId> {
        match self.paint {
            Paint::Spiral { id, .. } => Some(("omi-job", id).into()),
            _ => None,
        }
    }
    fn a11y_role(&self) -> Option<gpui::Role> {
        matches!(self.paint, Paint::Spiral { .. }).then_some(gpui::Role::Image)
    }
    fn write_a11y_info(&self, node: &mut gpui::accesskit::Node) {
        if let Paint::Spiral { mode, .. } = self.paint {
            node.set_label(mode.label());
        }
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
            size: size(px(self.side).into(), px(self.side).into()),
            flex_shrink: 0.,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let Paint::Spiral { mode, still, .. } = self.paint else {
            return None;
        };
        window.with_element_state(
            id.expect("Omi job id"),
            |state: Option<Rc<spinner::Crawl>>, window| {
                let mark = state.unwrap_or_default();
                mark.set(mode);
                mark.show_spiral(true, self.ink);
                mark.place(bounds, window);
                mark.prepare(!cx.reduce_motion(), still, window, cx);
                (Some(Rc::clone(&mark)), mark)
            },
        )
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepared: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        match &self.paint {
            Paint::Svg(key, data) => {
                let _ = window.paint_svg(
                    bounds,
                    key.clone(),
                    Some(data),
                    TransformationMatrix::unit(),
                    self.ink,
                    cx,
                );
            }
            Paint::Spiral { .. } => {
                if let Some(mark) = prepared {
                    mark.paint(window);
                }
            }
        }
    }
}

/// A button's frame, resting wash and pointer wash, inset by whole pixels while pressed so its label never scales.
pub struct Chrome {
    pub frame: Hsla,
    pub frame_width: f32,
    pub wash: Option<Hsla>,
    pub overlay: Option<Hsla>,
    pub radius: f32,
    // The press's direction and start: going in while held, coming back after release.
    pub motion: Option<(bool, Instant)>,
}

impl IntoElement for Chrome {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Chrome {
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
        let mut style = Style {
            position: gpui::Position::Absolute,
            ..Style::default()
        };
        style.inset = gpui::Edges {
            top: px(0.).into(),
            right: px(0.).into(),
            bottom: px(0.).into(),
            left: px(0.).into(),
        };
        (window.request_layout(style, None, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let progress = match self.motion {
            None => 0.,
            Some((down, since)) => {
                // Reduced motion lands the press at once, with no frame asked.
                let t = if cx.reduce_motion() {
                    1.
                } else {
                    cx.background_executor()
                        .now()
                        .saturating_duration_since(since)
                        .as_secs_f32()
                        * 1000.
                        / card::PRESS_MS as f32
                };
                if t < 1. {
                    window.request_animation_frame();
                }
                if down {
                    t.min(1.)
                } else {
                    1. - t.min(1.)
                }
            }
        };
        let (dx, dy) = (
            card::press_inset(f32::from(bounds.size.width), progress),
            card::press_inset(f32::from(bounds.size.height), progress),
        );
        let outer = Bounds::new(
            point(bounds.origin.x + px(dx), bounds.origin.y + px(dy)),
            size(
                bounds.size.width - px(2. * dx),
                bounds.size.height - px(2. * dy),
            ),
        );
        let radius = px(self.radius);
        window.paint_quad(gpui::quad(
            outer,
            radius,
            self.wash.unwrap_or(self.frame.opacity(0.)),
            px(self.frame_width),
            self.frame,
            gpui::BorderStyle::Solid,
        ));
        if let Some(overlay) = self.overlay {
            let inner = Bounds::new(
                point(
                    outer.origin.x + px(self.frame_width),
                    outer.origin.y + px(self.frame_width),
                ),
                size(
                    outer.size.width - px(2. * self.frame_width),
                    outer.size.height - px(2. * self.frame_width),
                ),
            );
            window.paint_quad(
                fill(inner, overlay).corner_radii((radius - px(self.frame_width)).max(px(0.))),
            );
        }
    }
}

/// One line of text with letter spacing, which GPUI's text style lacks: shaped once per layout, each glyph moved along by the tracking.
pub struct Tracked {
    pub text: SharedString,
    pub size: f32,
    pub line: f32,
    pub track: f32,
    pub ink: Hsla,
}

impl IntoElement for Tracked {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Tracked {
    type RequestLayoutState = ShapedLine;
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
    ) -> (LayoutId, ShapedLine) {
        let font = window.text_style().font();
        let run = TextRun {
            len: self.text.len(),
            font,
            color: self.ink,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped =
            window
                .text_system()
                .shape_line(self.text.clone(), px(self.size), &[run], None);
        let width = shaped.width + px(self.track * self.text.chars().count() as f32);
        let style = Style {
            size: size(width.into(), px(self.line).into()),
            flex_shrink: 0.,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), shaped)
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut ShapedLine,
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        shaped: &mut ShapedLine,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        crate::g2::status_feedback::caption::Tracked {
            text: self.text.clone(),
            font: window.text_style().font(),
            size: px(self.size),
            line: px(self.line),
            tracking: px(self.track),
            color: self.ink,
        }
        .paint(None, None, bounds, shaped, &mut (), window, cx);
    }
}
